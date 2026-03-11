use crate::output::{print_json, print_error};
use serde_json::json;
use std::time::Instant;

use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2::{self, grid_latlon};
use rustmet_core::grib2::tables;

pub fn run(
    lat: f64,
    lon: f64,
    var: &str,
    level: &str,
    model: &str,
    hours: u32,
    pretty: bool,
) {
    let start = Instant::now();

    let model_lower = model.to_lowercase();
    if !["hrrr", "rap", "gfs", "nam"].contains(&model_lower.as_str()) {
        print_error(&format!(
            "Model '{}' not supported. Use: hrrr, rap, gfs, nam",
            model
        ));
    }

    // Cap hours based on model
    let max_hours = match model_lower.as_str() {
        "hrrr" => 48,
        "gfs" => 384,
        "rap" => 51,
        "nam" => 84,
        _ => 48,
    };
    let hours = hours.min(max_hours);

    // Build .idx pattern
    let pattern = build_pattern(var, level);

    // Determine product type
    let product = if level.contains("mb") || level.contains("hPa") {
        "prs"
    } else {
        "sfc"
    };

    // Create download client
    let client = DownloadClient::new()
        .unwrap_or_else(|e| print_error(&format!("HTTP client: {}", e)));

    // Find latest model run — may retry with previous run if incomplete
    eprintln!("Finding latest {} run...", model_lower);
    let (mut date, mut run_hour) = rustmet_core::models::find_latest_run(&client, &model_lower)
        .unwrap_or_else(|e| print_error(&format!("No model run: {}", e)));

    let patterns: Vec<&str> = vec![pattern.as_str()];

    // We may need to retry with the previous run if the latest is incomplete
    let max_attempts = 2;
    let mut attempt = 0;
    let mut run_label;
    let mut nearest_idx = 0;
    let mut nearest_lat = 0.0;
    let mut nearest_lon = 0.0;
    let mut dist_km = 0.0;
    let mut param_name = "";
    let mut param_units = "";
    let mut display_units = String::new();
    let mut reference_time = chrono::NaiveDateTime::default();
    let mut forecast: Vec<(u32, f64)> = Vec::new();
    let mut forecast_json: Vec<serde_json::Value> = Vec::new();
    let mut downloads: u32 = 0;
    let mut failed_downloads: u32 = 0;

    loop {
        attempt += 1;
        run_label = format!("{}/{:02}z", date, run_hour);
        eprintln!("Using run: {} (attempt {})", run_label, attempt);

        // Reset state for this attempt
        forecast.clear();
        forecast_json.clear();
        downloads = 0;
        failed_downloads = 0;

        // Download f00 first to get grid info and nearest point
        eprintln!("Downloading {} {} {} f00 [{}]...", model_lower, run_label, product, pattern);
        downloads += 1;

        let result = match fetch_with_fallback(
            &client,
            &model_lower,
            &date,
            run_hour,
            product,
            0,
            Some(&patterns),
            None,
        ) {
            Ok(r) => r,
            Err(e) => {
                if attempt < max_attempts {
                    eprintln!("f00 failed for {}, trying previous run...", run_label);
                    fallback_run(&model_lower, &mut date, &mut run_hour);
                    continue;
                }
                print_error(&format!("Download f00 failed: {}", e))
            }
        };

        // Parse GRIB2
        let grib = match grib2::Grib2File::from_bytes(&result.data) {
            Ok(g) => g,
            Err(e) => print_error(&format!("GRIB2 parse failed: {}", e)),
        };

        if grib.messages.is_empty() {
            print_error("No matching GRIB2 messages found in f00");
        }

        let msg = &grib.messages[0];
        let values = match grib2::unpack_message(msg) {
            Ok(v) => v,
            Err(e) => print_error(&format!("Failed to unpack f00: {}", e)),
        };

        // Generate grid coordinates
        let (lats, lons) = grid_latlon(&msg.grid);

        // Find nearest grid point
        let target_lon = if lons.iter().any(|&l| l > 180.0) && lon < 0.0 {
            lon + 360.0
        } else {
            lon
        };

        let mut min_dist = f64::INFINITY;
        nearest_idx = 0;
        for (i, (&la, &lo)) in lats.iter().zip(lons.iter()).enumerate() {
            let d = (la - lat).powi(2) + (lo - target_lon).powi(2);
            if d < min_dist {
                min_dist = d;
                nearest_idx = i;
            }
        }

        nearest_lat = lats[nearest_idx];
        nearest_lon = lons[nearest_idx];
        if nearest_lon > 180.0 {
            nearest_lon -= 360.0;
        }
        dist_km = (min_dist.sqrt()) * 111.0;

        // Get variable metadata
        param_name = tables::parameter_name(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        );
        param_units = tables::parameter_units(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        );

        // Convert value
        let (f00_value, du) = convert_value(var, values[nearest_idx], param_units);
        display_units = du;
        reference_time = msg.reference_time;

        let valid_time_f00 = reference_time.format("%Y-%m-%dT%H:%M:%S").to_string();
        let rounded_f00 = (f00_value * 10.0).round() / 10.0;
        forecast.push((0, rounded_f00));
        forecast_json.push(json!({
            "fhour": 0,
            "valid_time": valid_time_f00,
            "value": rounded_f00,
        }));

        // Download remaining forecast hours, tracking consecutive failures
        let mut consecutive_failures: u32 = 0;
        let mut incomplete_run = false;

        for fhour in 1..=hours {
            eprintln!("Downloading f{:02}...", fhour);
            downloads += 1;

            let result = match fetch_with_fallback(
                &client,
                &model_lower,
                &date,
                run_hour,
                product,
                fhour,
                Some(&patterns),
                None,
            ) {
                Ok(r) => {
                    consecutive_failures = 0;
                    r
                }
                Err(e) => {
                    eprintln!("Warning: f{:02} download failed: {}", fhour, e);
                    failed_downloads += 1;
                    consecutive_failures += 1;
                    // If 3+ consecutive failures in the first 5 fhours, run is incomplete
                    if consecutive_failures >= 3 && fhour <= 5 {
                        eprintln!("Run {} appears incomplete (3 consecutive failures by f{:02})", run_label, fhour);
                        incomplete_run = true;
                        break;
                    }
                    continue;
                }
            };

            let grib = match grib2::Grib2File::from_bytes(&result.data) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Warning: f{:02} parse failed: {}", fhour, e);
                    failed_downloads += 1;
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 && fhour <= 5 {
                        incomplete_run = true;
                        break;
                    }
                    continue;
                }
            };

            if grib.messages.is_empty() {
                eprintln!("Warning: f{:02} no messages found", fhour);
                failed_downloads += 1;
                consecutive_failures += 1;
                if consecutive_failures >= 3 && fhour <= 5 {
                    incomplete_run = true;
                    break;
                }
                continue;
            }

            consecutive_failures = 0;
            let msg = &grib.messages[0];
            let values = match grib2::unpack_message(msg) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Warning: f{:02} unpack failed: {}", fhour, e);
                    failed_downloads += 1;
                    continue;
                }
            };

            let (val, _) = convert_value(var, values[nearest_idx], param_units);
            let rounded = (val * 10.0).round() / 10.0;
            let valid_time = reference_time + chrono::Duration::hours(fhour as i64);
            let valid_str = valid_time.format("%Y-%m-%dT%H:%M:%S").to_string();

            forecast.push((fhour, rounded));
            forecast_json.push(json!({
                "fhour": fhour,
                "valid_time": valid_str,
                "value": rounded,
            }));
        }

        if incomplete_run && attempt < max_attempts {
            eprintln!("Falling back to previous run...");
            fallback_run(&model_lower, &mut date, &mut run_hour);
            continue;
        }

        break;
    }

    // Compute statistics
    let values_only: Vec<f64> = forecast.iter().map(|f| f.1).collect();
    let ts_min = values_only.iter().cloned().fold(f64::INFINITY, f64::min);
    let ts_max = values_only.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let ts_mean = values_only.iter().sum::<f64>() / values_only.len() as f64;
    let ts_range = ts_max - ts_min;

    // Compute trend: compare first third average to last third average
    let third = values_only.len() / 3;
    let trend = if third > 0 && values_only.len() >= 3 {
        let first_avg: f64 = values_only[..third].iter().sum::<f64>() / third as f64;
        let last_avg: f64 = values_only[values_only.len() - third..].iter().sum::<f64>() / third as f64;
        let mean_abs = ((first_avg.abs() + last_avg.abs()) / 2.0).max(1.0);
        let pct_change = (last_avg - first_avg) / mean_abs;
        if pct_change > 0.05 {
            "rising"
        } else if pct_change < -0.05 {
            "falling"
        } else {
            "steady"
        }
    } else {
        "steady"
    };

    // Detect events
    let events = detect_events(&forecast, var, &display_units);

    let total_ms = start.elapsed().as_millis();

    print_json(&json!({
        "model": model_lower.to_uppercase(),
        "run": run_label,
        "variable": param_name,
        "variable_short": var,
        "level": level,
        "units": display_units,
        "location": {
            "requested": { "lat": lat, "lon": lon },
            "nearest_grid": {
                "lat": (nearest_lat * 1000.0).round() / 1000.0,
                "lon": (nearest_lon * 1000.0).round() / 1000.0,
            },
            "distance_km": (dist_km * 10.0).round() / 10.0,
        },
        "forecast": forecast_json,
        "statistics": {
            "min": (ts_min * 10.0).round() / 10.0,
            "max": (ts_max * 10.0).round() / 10.0,
            "mean": (ts_mean * 10.0).round() / 10.0,
            "range": (ts_range * 10.0).round() / 10.0,
            "trend": trend,
        },
        "events": events,
        "performance": {
            "total_ms": total_ms,
            "downloads": downloads,
            "failed_downloads": failed_downloads,
        },
    }), pretty);
}

/// Detect significant weather events in the forecast timeseries.
fn detect_events(forecast: &[(u32, f64)], var: &str, units: &str) -> Vec<serde_json::Value> {
    let mut events = Vec::new();

    // Temperature-like variables: detect significant changes
    let var_lower = var.to_lowercase();
    if var_lower.contains("temp") || var_lower == "t" || var_lower == "td"
        || var_lower == "dew" || var_lower == "dewpoint"
        || units.contains("K") || units.contains("C") || units.contains("F")
    {
        for window in forecast.windows(4) {
            let delta = window[3].1 - window[0].1;
            if delta < -5.0 {
                events.push(json!({
                    "type": "temperature_drop",
                    "start_fhour": window[0].0,
                    "end_fhour": window[3].0,
                    "change": (delta * 10.0).round() / 10.0,
                    "description": format!(
                        "Temperature drops {:.1} {} over {} hours",
                        delta.abs(), units, window[3].0 - window[0].0
                    )
                }));
                break;
            }
            if delta > 5.0 {
                events.push(json!({
                    "type": "temperature_rise",
                    "start_fhour": window[0].0,
                    "end_fhour": window[3].0,
                    "change": (delta * 10.0).round() / 10.0,
                    "description": format!(
                        "Temperature rises {:.1} {} over {} hours",
                        delta, units, window[3].0 - window[0].0
                    )
                }));
                break;
            }
        }
    }

    // Reflectivity/precip: detect onset and cessation
    if var_lower.contains("refl") || var_lower.contains("refc")
        || var_lower.contains("precip") || var_lower.contains("apcp")
    {
        let threshold = if units.contains("dBZ") || var_lower.contains("refc") {
            15.0
        } else {
            0.1
        };
        for i in 1..forecast.len() {
            if forecast[i - 1].1 < threshold && forecast[i].1 >= threshold {
                events.push(json!({
                    "type": "precipitation_onset",
                    "fhour": forecast[i].0,
                    "value": (forecast[i].1 * 10.0).round() / 10.0,
                    "description": format!("Precipitation begins at f{:02}", forecast[i].0)
                }));
                break;
            }
        }
        for i in 1..forecast.len() {
            if forecast[i - 1].1 >= threshold && forecast[i].1 < threshold {
                events.push(json!({
                    "type": "precipitation_end",
                    "fhour": forecast[i].0,
                    "description": format!("Precipitation ends at f{:02}", forecast[i].0)
                }));
                break;
            }
        }
    }

    // Wind gust: detect peak
    if var_lower.contains("gust") || var_lower.contains("wind") || var_lower.contains("wspd") {
        if let Some((max_idx, _)) = forecast
            .iter()
            .enumerate()
            .max_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap_or(std::cmp::Ordering::Equal))
        {
            let peak = &forecast[max_idx];
            events.push(json!({
                "type": "peak_wind",
                "fhour": peak.0,
                "value": (peak.1 * 10.0).round() / 10.0,
                "description": format!("Peak wind {:.1} {} at f{:02}", peak.1, units, peak.0)
            }));
        }
    }

    // CAPE: detect peak instability
    if var_lower.contains("cape") {
        if let Some((max_idx, _)) = forecast
            .iter()
            .enumerate()
            .max_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap_or(std::cmp::Ordering::Equal))
        {
            let peak = &forecast[max_idx];
            if peak.1 > 500.0 {
                events.push(json!({
                    "type": "peak_instability",
                    "fhour": peak.0,
                    "value": (peak.1 * 10.0).round() / 10.0,
                    "description": format!("Peak CAPE {:.0} {} at f{:02}", peak.1, units, peak.0)
                }));
            }
        }
    }

    events
}

/// Build an .idx search pattern from user-friendly var/level names.
fn build_pattern(var: &str, level: &str) -> String {
    let grib_var = match var.to_lowercase().as_str() {
        "temperature" | "temp" | "t" => "TMP",
        "dewpoint" | "td" | "dew" => "DPT",
        "wind_u" | "u" | "ugrd" => "UGRD",
        "wind_v" | "v" | "vgrd" => "VGRD",
        "gust" => "GUST",
        "pressure" | "pres" => "PRES",
        "mslp" => "MSLMA",
        "cape" => "CAPE",
        "cin" => "CIN",
        "reflectivity" | "refl" | "refc" => return "REFC:entire atmosphere".to_string(),
        "visibility" | "vis" => "VIS",
        "rh" | "relative_humidity" => "RH",
        "height" | "hgt" => "HGT",
        "precip" | "precipitation" | "apcp" => "APCP",
        "helicity" | "hlcy" | "srh" => {
            let lvl = level.to_lowercase();
            if lvl.contains("1") {
                return "HLCY:1000-0 m above ground".to_string();
            }
            return "HLCY:3000-0 m above ground".to_string();
        }
        "pwat" | "precipitable_water" => return "PWAT:entire atmosphere".to_string(),
        "updraft_helicity" | "uh" | "mxuphl" => {
            return "MXUPHL:5000-2000 m above ground".to_string()
        }
        "wind_speed" | "wspd" | "wind" => "WIND",
        "snow" | "snowfall" | "weasd" => "WEASD",
        "cloud" | "cloud_cover" | "tcc" | "tcdc" => "TCDC",
        _ => var,
    };

    let grib_level = match level.to_lowercase().as_str() {
        "surface" | "sfc" => "surface",
        "2m" => "2 m above ground",
        "10m" => "10 m above ground",
        "atmosphere" | "entire" => "entire atmosphere",
        "0-3km" | "3000-0m" => "3000-0 m above ground",
        "0-1km" | "1000-0m" => "1000-0 m above ground",
        "0-6km" | "6000-0m" => "6000-0 m above ground",
        "2-5km" | "5000-2000m" => "5000-2000 m above ground",
        "255-0mb" | "ml" | "mixed_layer" => "255-0 mb above ground",
        l if l.ends_with("mb") || l.ends_with("hpa") => {
            let num = l.trim_end_matches("mb").trim_end_matches("hpa");
            return format!("{}:{} mb", grib_var, num);
        }
        _ => level,
    };

    format!("{}:{}", grib_var, grib_level)
}

/// Convert raw GRIB2 value to user-friendly units.
fn convert_value(var: &str, raw: f64, grib_units: &str) -> (f64, String) {
    match var.to_lowercase().as_str() {
        "temperature" | "temp" | "t" | "dewpoint" | "td" | "dew" => {
            if grib_units.contains("K") {
                (raw - 273.15, "\u{00b0}C".to_string())
            } else {
                (raw, grib_units.to_string())
            }
        }
        "wind_u" | "u" | "ugrd" | "wind_v" | "v" | "vgrd" | "gust"
        | "wind_speed" | "wspd" | "wind" => {
            (raw * 1.94384, "kt".to_string())
        }
        "pressure" | "pres" | "mslp" => {
            if raw > 10000.0 {
                (raw / 100.0, "hPa".to_string())
            } else {
                (raw, "hPa".to_string())
            }
        }
        "visibility" | "vis" => (raw / 1000.0, "km".to_string()),
        _ => (raw, grib_units.to_string()),
    }
}

/// Step back to the previous valid model run.
///
/// For hourly models (HRRR, RAP), goes back 1 hour.
/// For 6-hourly models (GFS, NAM), goes back 6 hours.
/// Handles date rollover automatically.
fn fallback_run(model: &str, date: &mut String, hour: &mut u32) {
    let step: u32 = match model {
        "gfs" | "nam" => 6,
        _ => 1, // hrrr, rap
    };

    if *hour >= step {
        *hour -= step;
    } else {
        // Roll back to previous day
        let dt = chrono::NaiveDate::parse_from_str(date, "%Y%m%d")
            .expect("valid date string");
        let prev = dt - chrono::Duration::days(1);
        *date = prev.format("%Y%m%d").to_string();
        // Wrap to last valid hour of previous day
        let max_hour = match model {
            "gfs" | "nam" => 18,
            _ => 23,
        };
        *hour = max_hour;
    }
    eprintln!("  Falling back to {}/{:02}z", date, hour);
}
