use crate::output::{print_json, print_error};
use chrono::Datelike;
use serde_json::{json, Value};
use std::time::Instant;
use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2;
use rustmet_core::models::find_latest_run;

pub fn run(lat: f64, lon: f64, pretty: bool) {
    if lat < -90.0 || lat > 90.0 {
        print_error(&format!("Invalid latitude {}: must be between -90 and 90", lat));
    }
    if lon < -180.0 || lon > 180.0 {
        print_error(&format!("Invalid longitude {}: must be between -180 and 180", lon));
    }

    let total_start = Instant::now();
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Source 1: METAR observation
    let metar_start = Instant::now();
    let metar_result = fetch_metar(lat, lon);
    let metar_ms = metar_start.elapsed().as_millis() as u64;

    // Source 2: HRRR model data
    let model_start = Instant::now();
    let model_result = fetch_hrrr(lat, lon);
    let model_ms = model_start.elapsed().as_millis() as u64;

    // Source 3: NWS Alerts
    let alerts_start = Instant::now();
    let alerts_result = fetch_alerts(lat, lon);
    let alerts_ms = alerts_start.elapsed().as_millis() as u64;

    let total_ms = total_start.elapsed().as_millis() as u64;

    // Build sources array
    let mut sources = Vec::new();

    // METAR source
    let metar_source = match &metar_result {
        Some(m) => json!({
            "name": "metar",
            "station": m.station,
            "observation_time": m.obs_time,
            "age_minutes": m.age_minutes,
            "available": true,
            "data": {
                "temp_c": m.temp_c,
                "dewpoint_c": m.dewpoint_c,
                "wind_dir": m.wind_dir,
                "wind_speed_kt": m.wind_speed_kt,
                "wind_gust_kt": m.wind_gust_kt,
                "visibility_sm": m.visibility_sm,
                "raw": m.raw,
            }
        }),
        None => json!({
            "name": "metar",
            "available": false,
            "data": null,
        }),
    };
    sources.push(metar_source);

    // HRRR source
    let model_source = match &model_result {
        Some(m) => json!({
            "name": "hrrr_model",
            "run": m.run_label,
            "valid_time": m.valid_time,
            "age_minutes": m.age_minutes,
            "available": true,
            "data": {
                "temp_c": m.temp_c,
                "dewpoint_c": m.dewpoint_c,
                "wind_speed_kt": m.wind_speed_kt,
                "wind_dir": m.wind_dir,
                "cape_jkg": m.cape,
                "composite_refl_dbz": m.refc,
            }
        }),
        None => json!({
            "name": "hrrr_model",
            "available": false,
            "data": null,
        }),
    };
    sources.push(model_source);

    // Alerts source
    let alerts_source = match &alerts_result {
        Some(alerts) => {
            let items: Vec<Value> = alerts.iter().map(|a| {
                json!({
                    "event": a.event,
                    "severity": a.severity,
                    "headline": a.headline,
                })
            }).collect();
            json!({
                "name": "nws_alerts",
                "available": true,
                "alert_count": items.len(),
                "alerts": items,
            })
        }
        None => json!({
            "name": "nws_alerts",
            "available": false,
            "alert_count": 0,
            "alerts": [],
        }),
    };
    sources.push(alerts_source);

    // Compare sources
    let mut comparisons = Vec::new();
    let mut statuses: Vec<(&str, &str)> = Vec::new();

    if let (Some(obs), Some(mdl)) = (&metar_result, &model_result) {
        // Temperature
        if let (Some(obs_t), Some(mdl_t)) = (obs.temp_c, mdl.temp_c) {
            let diff = (obs_t - mdl_t).abs();
            let status = classify_diff(diff, 2.0, 5.0);
            comparisons.push(json!({
                "field": "temperature",
                "units": "°C",
                "observation": round1(obs_t),
                "model": round1(mdl_t),
                "difference": round1(diff),
                "status": status,
            }));
            statuses.push(("temperature", leak_str(status)));
        }

        // Dewpoint
        if let (Some(obs_d), Some(mdl_d)) = (obs.dewpoint_c, mdl.dewpoint_c) {
            let diff = (obs_d - mdl_d).abs();
            let status = classify_diff(diff, 2.0, 5.0);
            comparisons.push(json!({
                "field": "dewpoint",
                "units": "°C",
                "observation": round1(obs_d),
                "model": round1(mdl_d),
                "difference": round1(diff),
                "status": status,
            }));
            statuses.push(("dewpoint", leak_str(status)));
        }

        // Wind speed
        if let (Some(obs_ws), Some(mdl_ws)) = (obs.wind_speed_kt, mdl.wind_speed_kt) {
            let diff = (obs_ws - mdl_ws).abs();
            let status = classify_diff(diff, 5.0, 10.0);
            comparisons.push(json!({
                "field": "wind_speed",
                "units": "kt",
                "observation": round1(obs_ws),
                "model": round1(mdl_ws),
                "difference": round1(diff),
                "status": status,
            }));
            statuses.push(("wind_speed", leak_str(status)));
        }

        // Wind direction
        if let (Some(obs_wd), Some(mdl_wd)) = (obs.wind_dir, mdl.wind_dir) {
            let diff = angular_diff(obs_wd, mdl_wd);
            let status = classify_diff(diff, 20.0, 45.0);
            comparisons.push(json!({
                "field": "wind_direction",
                "units": "°",
                "observation": round1(obs_wd),
                "model": round1(mdl_wd),
                "difference": round1(diff),
                "status": status,
            }));
            statuses.push(("wind_direction", leak_str(status)));
        }
    }

    // Compute confidence
    let (confidence, confidence_score) = compute_confidence(&statuses, &metar_result, &model_result);

    // Generate assessment
    let assessment = generate_assessment(&statuses, &metar_result, &model_result, &alerts_result);

    let output = json!({
        "location": { "lat": lat, "lon": lon },
        "timestamp": timestamp,
        "confidence": confidence,
        "confidence_score": confidence_score,
        "sources": sources,
        "comparisons": comparisons,
        "assessment": assessment,
        "performance": {
            "metar_ms": metar_ms,
            "model_ms": model_ms,
            "alerts_ms": alerts_ms,
            "total_ms": total_ms,
        }
    });

    print_json(&output, pretty);
}

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

struct MetarData {
    station: String,
    obs_time: String,
    age_minutes: i64,
    temp_c: Option<f64>,
    dewpoint_c: Option<f64>,
    wind_dir: Option<f64>,
    wind_speed_kt: Option<f64>,
    wind_gust_kt: Option<f64>,
    visibility_sm: Option<f64>,
    raw: String,
}

struct ModelData {
    run_label: String,
    valid_time: String,
    age_minutes: i64,
    temp_c: Option<f64>,
    dewpoint_c: Option<f64>,
    wind_speed_kt: Option<f64>,
    wind_dir: Option<f64>,
    cape: Option<f64>,
    refc: Option<f64>,
}

struct AlertData {
    event: String,
    severity: String,
    headline: String,
}

// ---------------------------------------------------------------------------
// Source fetchers
// ---------------------------------------------------------------------------

fn fetch_metar(lat: f64, lon: f64) -> Option<MetarData> {
    // Find nearest station using the built-in station database
    let station_info = wx_obs::stations::nearest_station(lat, lon);
    let station_id = &station_info.icao;

    // Fetch latest METAR for that station
    let metar = wx_obs::fetch::fetch_metar(station_id).ok()?;

    let now = chrono::Utc::now();
    // Compute observation time from METAR day/hour/minute
    let obs_dt = {
        let mut dt = now.naive_utc().date();
        // If METAR day is different from today, handle month boundary
        if metar.time.day != dt.day() as u8 {
            if metar.time.day > dt.day() as u8 {
                // Previous month
                dt = dt - chrono::TimeDelta::days(1);
            }
        }
        chrono::NaiveDate::from_ymd_opt(dt.year(), dt.month(), metar.time.day as u32)
            .and_then(|d| d.and_hms_opt(metar.time.hour as u32, metar.time.minute as u32, 0))
    };

    let age_minutes = obs_dt.map(|dt| (now.naive_utc() - dt).num_minutes()).unwrap_or(-1);
    let obs_time = obs_dt
        .map(|dt| format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S")))
        .unwrap_or_default();

    let temp_c = metar.temperature.map(|t| t as f64);
    let dewpoint_c = metar.dewpoint.map(|d| d as f64);
    let (wind_dir, wind_speed_kt, wind_gust_kt) = match &metar.wind {
        Some(w) => (
            w.direction.map(|d| d as f64),
            Some(w.speed as f64),
            w.gust.map(|g| g as f64),
        ),
        None => (None, None, None),
    };
    let visibility_sm = metar.visibility.as_ref().map(|v| v.statute_miles);

    Some(MetarData {
        station: station_id.to_string(),
        obs_time,
        age_minutes,
        temp_c,
        dewpoint_c,
        wind_dir,
        wind_speed_kt,
        wind_gust_kt,
        visibility_sm,
        raw: metar.raw.clone(),
    })
}

fn fetch_hrrr(lat: f64, lon: f64) -> Option<ModelData> {
    let client = DownloadClient::new().ok()?;

    // Find latest HRRR run
    let (date, hour) = find_latest_run(&client, "hrrr").ok()?;

    let patterns: Vec<&str> = vec![
        "TMP:2 m above ground",
        "DPT:2 m above ground",
        "UGRD:10 m above ground",
        "VGRD:10 m above ground",
        "CAPE:surface",
        "REFC:entire atmosphere",
    ];

    let result = fetch_with_fallback(&client, "hrrr", &date, hour, "sfc", 0, Some(&patterns), None).ok()?;
    let grib = grib2::Grib2File::from_bytes(&result.data).ok()?;

    if grib.messages.is_empty() {
        return None;
    }

    // Find nearest grid point using first message's grid
    let (lats, lons) = grib2::grid_latlon(&grib.messages[0].grid);
    let nearest_idx = find_nearest_idx(&lats, &lons, lat, lon);

    // Extract variables from messages
    let mut temp_k: Option<f64> = None;
    let mut dewp_k: Option<f64> = None;
    let mut ugrd: Option<f64> = None;
    let mut vgrd: Option<f64> = None;
    let mut cape: Option<f64> = None;
    let mut refc: Option<f64> = None;

    for msg in &grib.messages {
        let d = msg.discipline;
        let cat = msg.product.parameter_category;
        let num = msg.product.parameter_number;

        if let Ok(values) = grib2::unpack_message(msg) {
            if nearest_idx < values.len() {
                let val = values[nearest_idx];
                match (d, cat, num) {
                    (0, 0, 0) => temp_k = Some(val),   // TMP
                    (0, 0, 6) => dewp_k = Some(val),   // DPT
                    (0, 2, 2) => ugrd = Some(val),      // UGRD
                    (0, 2, 3) => vgrd = Some(val),      // VGRD
                    (0, 7, 6) => cape = Some(val),      // CAPE
                    (0, 16, 196) => refc = Some(val),   // REFC
                    _ => {}
                }
            }
        }
    }

    // Convert K to C
    let temp_c = temp_k.map(|k| round1(k - 273.15));
    let dewpoint_c = dewp_k.map(|k| round1(k - 273.15));

    // Wind speed and direction from u/v
    let (wind_speed_kt, wind_dir) = match (ugrd, vgrd) {
        (Some(u), Some(v)) => {
            let speed = (u * u + v * v).sqrt();
            let speed_kt = round1(speed * 1.94384);
            let dir = (270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0);
            (Some(speed_kt), Some(round1(dir)))
        }
        _ => (None, None),
    };

    let cape_val = cape.map(|c| round1(c));
    let refc_val = refc.map(|r| round1(r));

    // Model valid time and age
    let ref_time = grib.messages[0].reference_time;
    let fhour = grib.messages[0].product.forecast_time;
    let valid_dt = ref_time + chrono::TimeDelta::hours(fhour as i64);
    let valid_time = format!("{}Z", valid_dt.format("%Y-%m-%dT%H:%M:%S"));
    let run_label = format!("{}/{}z", date, format!("{:02}", hour));

    let now = chrono::Utc::now().naive_utc();
    let age_minutes = (now - valid_dt).num_minutes();

    Some(ModelData {
        run_label,
        valid_time,
        age_minutes,
        temp_c,
        dewpoint_c: dewpoint_c,
        wind_speed_kt,
        wind_dir,
        cape: cape_val,
        refc: refc_val,
    })
}

fn fetch_alerts(lat: f64, lon: f64) -> Option<Vec<AlertData>> {
    let raw_alerts = wx_alerts::fetch_alerts_by_point(lat, lon).ok()?;
    let alerts = raw_alerts.iter().map(|a| AlertData {
        event: a.event.clone(),
        severity: format!("{:?}", a.severity),
        headline: a.headline.clone(),
    }).collect();
    Some(alerts)
}

// ---------------------------------------------------------------------------
// Comparison helpers
// ---------------------------------------------------------------------------

fn classify_diff(diff: f64, minor_thresh: f64, major_thresh: f64) -> &'static str {
    if diff <= minor_thresh {
        "agree"
    } else if diff <= major_thresh {
        "minor_disagree"
    } else {
        "disagree"
    }
}

fn angular_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn find_nearest_idx(lats: &[f64], lons: &[f64], target_lat: f64, target_lon: f64) -> usize {
    let mut best_idx = 0;
    let mut best_dist = f64::MAX;

    for i in 0..lats.len().min(lons.len()) {
        let dlat = lats[i] - target_lat;
        let dlon = lons[i] - target_lon;
        let dist = dlat * dlat + dlon * dlon;
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }

    best_idx
}

/// Leak a &'static str from a &str for use in the statuses vec.
/// This is fine — we only create a handful of short-lived strings.
fn leak_str(s: &str) -> &'static str {
    match s {
        "agree" => "agree",
        "minor_disagree" => "minor_disagree",
        "disagree" => "disagree",
        _ => "unknown",
    }
}

fn compute_confidence(
    statuses: &[(&str, &str)],
    metar: &Option<MetarData>,
    model: &Option<ModelData>,
) -> (String, f64) {
    // If either source is missing, confidence is reduced
    if metar.is_none() || model.is_none() {
        return ("low".to_string(), 0.25);
    }

    if statuses.is_empty() {
        return ("low".to_string(), 0.30);
    }

    let agree_count = statuses.iter().filter(|(_, s)| *s == "agree").count();
    let minor_count = statuses.iter().filter(|(_, s)| *s == "minor_disagree").count();
    let disagree_count = statuses.iter().filter(|(_, s)| *s == "disagree").count();
    let total = statuses.len();

    let mut score = (agree_count as f64 + minor_count as f64 * 0.5) / total as f64;

    let mut level = if disagree_count == 0 && minor_count <= 1 {
        "high"
    } else if disagree_count <= 1 {
        "medium"
    } else {
        "low"
    };

    // Reduce confidence if METAR is stale (>120 min)
    if let Some(obs) = metar {
        if obs.age_minutes > 120 {
            score -= 0.15;
            if level == "high" {
                level = "medium";
            }
        }
    }

    score = (score * 100.0).round() / 100.0;
    if score < 0.0 {
        score = 0.0;
    }

    (level.to_string(), score)
}

fn generate_assessment(
    statuses: &[(&str, &str)],
    metar: &Option<MetarData>,
    model: &Option<ModelData>,
    alerts: &Option<Vec<AlertData>>,
) -> Value {
    let agreements: Vec<&str> = statuses.iter()
        .filter(|(_, s)| *s == "agree")
        .map(|(f, _)| *f)
        .collect();

    let conflicts: Vec<&str> = statuses.iter()
        .filter(|(_, s)| *s == "disagree")
        .map(|(f, _)| *f)
        .collect();

    let mut notes: Vec<String> = Vec::new();

    // Missing sources
    if metar.is_none() {
        notes.push("METAR observation unavailable — confidence reduced".to_string());
    }
    if model.is_none() {
        notes.push("HRRR model data unavailable — confidence reduced".to_string());
    }

    // Agreement summary
    if !statuses.is_empty() && conflicts.is_empty() {
        if statuses.iter().all(|(_, s)| *s == "agree") {
            notes.push("All sources in good agreement".to_string());
        } else {
            notes.push("Sources mostly agree with minor differences".to_string());
        }
    }

    // Specific conflict notes
    for (field, status) in statuses {
        if *status == "disagree" {
            match *field {
                "temperature" => {
                    if let (Some(obs), Some(mdl)) = (metar, model) {
                        if let (Some(ot), Some(mt)) = (obs.temp_c, mdl.temp_c) {
                            notes.push(format!(
                                "Model temperature differs from observation by {:.1}°C",
                                (ot - mt).abs()
                            ));
                        }
                    }
                }
                "dewpoint" => {
                    notes.push("Model dewpoint differs significantly from observation".to_string());
                }
                "wind_speed" | "wind_direction" => {
                    notes.push("Model wind differs significantly from observation".to_string());
                }
                _ => {}
            }
        }
    }

    // Wind strength note
    if let Some(obs) = metar {
        if let Some(ws) = obs.wind_speed_kt {
            if ws >= 25.0 {
                notes.push("Strong winds confirmed by observation".to_string());
                if let Some(mdl) = model {
                    if let Some(mws) = mdl.wind_speed_kt {
                        if mws >= 20.0 {
                            notes.push("Strong winds confirmed by both observation and model".to_string());
                        }
                    }
                }
            }
        }
    }

    // CAPE / alert cross-check
    let has_alerts = alerts.as_ref().map_or(false, |a| !a.is_empty());
    let model_cape = model.as_ref().and_then(|m| m.cape).unwrap_or(0.0);
    let model_refc = model.as_ref().and_then(|m| m.refc).unwrap_or(-999.0);

    if model_cape > 1000.0 && has_alerts {
        notes.push("Instability and active alerts suggest elevated severe risk".to_string());
    }
    if model_refc > 35.0 && !has_alerts {
        notes.push("Radar echoes present but no active weather alert".to_string());
    }
    if has_alerts && model_cape <= 0.0 && model_refc <= 20.0 {
        notes.push("Active alert but model shows no instability — possible timing/location issue".to_string());
    }

    // Alert consistency note
    if has_alerts {
        if let Some(alert_list) = alerts {
            let events: Vec<&str> = alert_list.iter().map(|a| a.event.as_str()).collect();
            let wind_alert = events.iter().any(|e| {
                e.contains("Wind") || e.contains("wind")
            });
            if wind_alert {
                if let Some(obs) = metar {
                    if let Some(ws) = obs.wind_speed_kt {
                        if ws >= 20.0 {
                            notes.push("Wind advisory/warning consistent with observed conditions".to_string());
                        }
                    }
                }
            }
        }
    }

    // Staleness note
    if let Some(obs) = metar {
        if obs.age_minutes > 120 {
            notes.push(format!("METAR observation is {} minutes old — data may be stale", obs.age_minutes));
        }
    }

    // Deduplicate notes
    notes.dedup();

    json!({
        "agreements": agreements,
        "conflicts": conflicts,
        "notes": notes,
    })
}
