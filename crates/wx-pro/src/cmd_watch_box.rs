use crate::output::{print_json, print_error};
use serde_json::json;
use rustmet_core::models::mrms::MrmsConfig;
use rustmet_core::grib2::{Grib2File, unpack_message_normalized};
use std::io::Read;

const USER_AGENT: &str = "wx-pro/0.1 (Fahrenheit Research)";

/// Single-shot watch-box check: MRMS reflectivity at point + active alerts.
///
/// Future work: continuous monitoring loop with --interval-sec polling.
/// Currently performs a one-time check and returns results.
pub fn run(
    lat: f64,
    lon: f64,
    _radius_km: f64,
    _interval_sec: u64,
    threshold_dbz: f64,
    pretty: bool,
) {
    if lat < -90.0 || lat > 90.0 {
        print_error(&format!("Invalid latitude {}: must be between -90 and 90", lat));
    }
    if lon < -180.0 || lon > 180.0 {
        print_error(&format!("Invalid longitude {}: must be between -180 and 180", lon));
    }

    // Validate point is within MRMS CONUS domain
    let in_mrms_domain = lat >= MrmsConfig::lat_min() && lat <= MrmsConfig::lat_max()
        && lon >= MrmsConfig::lon_min() && lon <= MrmsConfig::lon_max();

    let check_start = std::time::Instant::now();

    // === 1. Check MRMS composite reflectivity at point ===
    let reflectivity_at_point = if in_mrms_domain {
        fetch_mrms_point_value(lat, lon)
    } else {
        json!({
            "value": null,
            "error": "point outside MRMS CONUS domain"
        })
    };

    let refl_value = reflectivity_at_point["value"].as_f64();
    let threshold_exceeded = refl_value.map(|v| v >= threshold_dbz).unwrap_or(false);

    // === 2. Check active alerts for point ===
    let alerts_json = match wx_alerts::alerts::fetch_alerts_by_point(lat, lon) {
        Ok(alerts) => {
            let items: Vec<serde_json::Value> = alerts.iter().map(|a| {
                json!({
                    "event": a.event,
                    "severity": format!("{:?}", a.severity),
                    "headline": a.headline,
                    "expires": a.expires,
                })
            }).collect();

            let severe_count = alerts.iter().filter(|a| {
                matches!(
                    a.severity,
                    wx_alerts::alerts::Severity::Severe | wx_alerts::alerts::Severity::Extreme
                )
            }).count();

            json!({
                "count": items.len(),
                "severe_count": severe_count,
                "items": items,
            })
        }
        Err(e) => {
            eprintln!("{{\"warning\":\"failed to fetch alerts: {}\"}}", e);
            json!({"count": 0, "severe_count": 0, "items": []})
        }
    };

    let alerts_active = alerts_json["count"].as_u64().unwrap_or(0) > 0;

    let check_ms = check_start.elapsed().as_millis();

    // Determine overall threat status
    let status = if threshold_exceeded && alerts_active {
        "ALERT — threshold exceeded AND active alerts"
    } else if threshold_exceeded {
        "WARNING — reflectivity threshold exceeded"
    } else if alerts_active {
        "WATCH — active alerts but below reflectivity threshold"
    } else {
        "CLEAR — no exceedances"
    };

    print_json(&json!({
        "location": {
            "lat": lat,
            "lon": lon,
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "threshold_dbz": threshold_dbz,
        "reflectivity_at_point": reflectivity_at_point,
        "threshold_exceeded": threshold_exceeded,
        "alerts": alerts_json,
        "alerts_active": alerts_active,
        "status": status,
        "performance": {
            "check_ms": check_ms,
        },
        // TODO: Continuous monitoring mode.
        // When --interval-sec is provided, this would loop:
        //   loop {
        //       let result = single_shot_check(lat, lon, threshold_dbz);
        //       println!("{}", serde_json::to_string(&result).unwrap());
        //       std::thread::sleep(Duration::from_secs(interval_sec));
        //   }
        // Each iteration prints a JSON line (JSONL format) for streaming consumption.
        "mode": "single-shot",
    }), pretty);
}

/// Fetch MRMS composite reflectivity value at a point.
fn fetch_mrms_point_value(lat: f64, lon: f64) -> serde_json::Value {
    // Compute latest MRMS datetime (round to nearest 2 minutes)
    let now = chrono::Utc::now();
    let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);
    let rounded_minute = (minute / 2) * 2;
    let dt_str = format!(
        "{}-{}{:02}{:02}",
        now.format("%Y%m%d"),
        now.format("%H"),
        rounded_minute,
        0
    );

    let url = MrmsConfig::composite_reflectivity_url(&dt_str);

    // Download
    let raw_data = match http_get_bytes(&url) {
        Ok(data) => data,
        Err(e) => {
            return json!({
                "value": null,
                "error": format!("MRMS download failed: {}", e),
                "url": url,
            });
        }
    };

    // Decompress
    let data = if MrmsConfig::needs_decompress(&url) {
        let mut decoder = flate2::read::GzDecoder::new(&raw_data[..]);
        let mut decompressed = Vec::new();
        match decoder.read_to_end(&mut decompressed) {
            Ok(_) => decompressed,
            Err(e) => {
                return json!({
                    "value": null,
                    "error": format!("gzip decompress failed: {}", e),
                });
            }
        }
    } else {
        raw_data
    };

    // Parse GRIB2
    let grib = match Grib2File::from_bytes(&data) {
        Ok(g) => g,
        Err(e) => {
            return json!({
                "value": null,
                "error": format!("GRIB2 parse failed: {}", e),
            });
        }
    };

    if grib.messages.is_empty() {
        return json!({"value": null, "error": "no messages in GRIB2"});
    }

    let msg = &grib.messages[0];
    let nx = msg.grid.nx as usize;

    // Convert lat/lon to grid indices
    let gi = ((lon - MrmsConfig::lon_min()) / MrmsConfig::grid_dx()).round() as usize;
    let gj = ((lat - MrmsConfig::lat_min()) / MrmsConfig::grid_dy()).round() as usize;

    match unpack_message_normalized(msg) {
        Ok(values) => {
            let idx = gj * nx + gi;
            if idx < values.len() {
                let val = values[idx];
                if val < -900.0 {
                    json!({"value": null, "note": "no data at point", "datetime": dt_str})
                } else {
                    json!({"value": (val * 10.0).round() / 10.0, "datetime": dt_str})
                }
            } else {
                json!({"value": null, "error": "grid index out of range"})
            }
        }
        Err(e) => json!({"value": null, "error": format!("unpack failed: {}", e)}),
    }
}

/// HTTP GET returning raw bytes.
fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("{}", e))?;

    let mut bytes = Vec::new();
    response.into_body().as_reader().read_to_end(&mut bytes)
        .map_err(|e| format!("failed to read response: {}", e))?;
    Ok(bytes)
}
