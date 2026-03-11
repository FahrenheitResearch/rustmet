use serde::Serialize;
use crate::output::{print_json, print_error};

use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2::{self, grid_latlon};
use rustmet_core::grib2::tables;

#[derive(Serialize)]
struct PointResponse {
    lat: f64,
    lon: f64,
    model: String,
    variable: String,
    level: String,
    value: f64,
    units: String,
    valid_time: String,
    run_time: String,
    source: String,
    nearest_grid_point: GridPoint,
}

#[derive(Serialize)]
struct GridPoint {
    lat: f64,
    lon: f64,
    distance_km: f64,
}

pub fn run(lat: f64, lon: f64, model: &str, var: &str, level: &str, pretty: bool) {
    let model_lower = model.to_lowercase();
    if !["hrrr", "rap", "gfs", "nam"].contains(&model_lower.as_str()) {
        print_error(&format!(
            "Model '{}' not supported. Use: hrrr, rap, gfs, nam",
            model
        ));
    }

    // Build the .idx pattern from var + level
    let pattern = build_pattern(var, level);

    // Create download client
    let client = match DownloadClient::new() {
        Ok(c) => c,
        Err(e) => print_error(&format!("Failed to create HTTP client: {}", e)),
    };

    // Find latest run
    eprintln!("Finding latest {} run...", model_lower);
    let (date, hour) = match rustmet_core::models::find_latest_run(&client, &model_lower) {
        Ok(r) => r,
        Err(e) => print_error(&format!("No model run found: {}", e)),
    };

    let run_label = format!("{}/{:02}z", date, hour);

    // Determine product type from level
    let product = if level.contains("mb") || level.contains("hPa") {
        "prs"
    } else {
        "sfc"
    };

    eprintln!("Downloading {} {} {} f00 [{}]...", model_lower, run_label, product, pattern);

    let patterns: Vec<&str> = vec![pattern.as_str()];
    let result = match fetch_with_fallback(
        &client,
        &model_lower,
        &date,
        hour,
        product,
        0,
        Some(&patterns),
        None,
    ) {
        Ok(r) => r,
        Err(e) => print_error(&format!("Download failed: {}", e)),
    };

    // Parse GRIB2
    let grib = match grib2::Grib2File::from_bytes(&result.data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("GRIB2 parse failed: {}", e)),
    };

    if grib.messages.is_empty() {
        print_error("No matching GRIB2 messages found");
    }

    let msg = &grib.messages[0];
    let values = match grib2::unpack_message(msg) {
        Ok(v) => v,
        Err(e) => print_error(&format!("Failed to unpack data: {}", e)),
    };

    let (lats, lons) = grid_latlon(&msg.grid);

    // Normalize target longitude
    let target_lon = if lons.iter().any(|&lo| lo > 180.0) {
        if lon < 0.0 { lon + 360.0 } else { lon }
    } else {
        lon
    };

    // Find nearest grid point
    let mut min_dist = f64::INFINITY;
    let mut nearest_idx = 0;
    for (i, (&la, &lo)) in lats.iter().zip(lons.iter()).enumerate() {
        let dlat = la - lat;
        let dlon = lo - target_lon;
        let dist = dlat * dlat + dlon * dlon;
        if dist < min_dist {
            min_dist = dist;
            nearest_idx = i;
        }
    }

    let raw_value = values[nearest_idx];
    let nearest_lat = lats[nearest_idx];
    let nearest_lon_raw = lons[nearest_idx];
    let nearest_lon = if nearest_lon_raw > 180.0 { nearest_lon_raw - 360.0 } else { nearest_lon_raw };

    // Approximate distance in km
    let dist_km = (min_dist.sqrt()) * 111.0; // rough degrees to km

    // Get units and convert
    let param_name = tables::parameter_name(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );
    let param_units = tables::parameter_units(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );

    // Convert value based on variable type
    let (display_value, display_units) = convert_value(var, raw_value, param_units);

    let valid_time = msg.reference_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let resp = PointResponse {
        lat,
        lon,
        model: model_lower.to_uppercase(),
        variable: param_name.to_string(),
        level: level.to_string(),
        value: (display_value * 100.0).round() / 100.0,
        units: display_units,
        valid_time: valid_time.clone(),
        run_time: run_label,
        source: result.source_name,
        nearest_grid_point: GridPoint {
            lat: (nearest_lat * 1000.0).round() / 1000.0,
            lon: (nearest_lon * 1000.0).round() / 1000.0,
            distance_km: (dist_km * 10.0).round() / 10.0,
        },
    };
    print_json(&resp, pretty);
}

/// Build an .idx search pattern from user-friendly var/level names.
fn build_pattern(var: &str, level: &str) -> String {
    let grib_var = match var.to_lowercase().as_str() {
        "temperature" | "temp" | "t" => "TMP",
        "dewpoint" | "td" | "dew" => "DPT",
        "wind_u" | "u" | "ugrd" => "UGRD",
        "wind_v" | "v" | "vgrd" => "VGRD",
        "gust" => "GUST",
        "pressure" | "pres" | "mslp" => "PRES",
        "cape" => "CAPE",
        "cin" => "CIN",
        "reflectivity" | "refl" | "refc" => "REFC",
        "visibility" | "vis" => "VIS",
        "rh" | "relative_humidity" => "RH",
        "height" | "hgt" => "HGT",
        "precip" | "precipitation" | "apcp" => "APCP",
        "helicity" | "hlcy" | "srh" => "HLCY",
        "pwat" | "precipitable_water" => "PWAT",
        _ => var, // Pass through as-is for exact GRIB2 names
    };

    let grib_level = match level.to_lowercase().as_str() {
        "surface" | "sfc" => "surface",
        "2m" => "2 m above ground",
        "10m" => "10 m above ground",
        "atmosphere" | "entire" => "entire atmosphere",
        l if l.ends_with("mb") || l.ends_with("hpa") => {
            // "500mb" -> "500 mb"
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
                (raw - 273.15, "°C".to_string())
            } else {
                (raw, grib_units.to_string())
            }
        }
        "wind_u" | "u" | "ugrd" | "wind_v" | "v" | "vgrd" | "gust" => {
            // m/s to knots
            (raw * 1.94384, "kt".to_string())
        }
        "pressure" | "pres" | "mslp" => {
            // Pa to hPa
            if raw > 10000.0 {
                (raw / 100.0, "hPa".to_string())
            } else {
                (raw, "hPa".to_string())
            }
        }
        "visibility" | "vis" => {
            (raw / 1000.0, "km".to_string())
        }
        _ => (raw, grib_units.to_string()),
    }
}
