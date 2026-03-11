// cmd_brief.rs — Ultra-compact briefing (the killer feature for bandwidth-constrained agents)
//
// Expected bandwidth: ~50KB total
//   - METAR for nearest station: ~500B
//   - Alert count for point: ~50KB (parse only count + highest severity)
//   - NWS short forecast (next period only): ~5KB (from cached /points)
//
// Returns a single compact JSON with everything an agent needs for situational awareness.

use crate::cache::DiskCache;
use crate::output::{print_json, print_error};
use serde_json::json;

const USER_AGENT: &str = "wx-lite/0.1 (Fahrenheit Research)";

/// Cache /points responses for 24 hours.
const POINTS_CACHE_SECS: u64 = 86400;
/// Cache forecast for 1 hour.
const FORECAST_CACHE_SECS: u64 = 3600;
/// Cache alert state for 5 minutes.
const ALERT_CACHE_SECS: u64 = 300;

pub fn run(lat: f64, lon: f64, pretty: bool) {
    if lat < -90.0 || lat > 90.0 {
        print_error(&format!("Invalid latitude {}: must be between -90 and 90", lat));
    }
    if lon < -180.0 || lon > 180.0 {
        print_error(&format!("Invalid longitude {}: must be between -180 and 180", lon));
    }

    let cache = DiskCache::new();

    // 1. Find nearest station and fetch METAR (~500B)
    let station = wx_obs::stations::nearest_station(lat, lon);
    let (temp_c, wind_str, visibility_miles, ceiling_ft, flight_category, metar_time) =
        fetch_metar_compact(&station.icao);

    // 2. Fetch alert count + highest severity (~50KB, cached 5min)
    let (alerts_active, highest_alert, hazard_level) = fetch_alert_compact(lat, lon, &cache);

    // 3. Fetch short forecast — next period only (~5KB, cached 1h)
    let forecast_next = fetch_forecast_next_period(lat, lon, &cache);

    // 4. Build compact response
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let result = json!({
        "station": station.icao,
        "temp_c": temp_c,
        "wind": wind_str,
        "visibility_miles": visibility_miles,
        "ceiling_ft": ceiling_ft,
        "flight_category": flight_category,
        "alerts_active": alerts_active,
        "highest_alert": highest_alert,
        "forecast_next": forecast_next,
        "hazard_level": hazard_level,
        "metar_time": metar_time,
        "timestamp": timestamp,
    });

    print_json(&result, pretty);
}

/// Fetch METAR and extract only the fields needed for brief output.
fn fetch_metar_compact(station: &str) -> (Option<f64>, String, Option<f64>, Option<u32>, String, Option<String>) {
    match wx_obs::fetch::fetch_recent_metars(station, 1) {
        Ok(metars) => {
            if let Some(m) = metars.into_iter().next() {
                let temp_c = m.temperature.map(|t| t as f64);
                let wind_str = match &m.wind {
                    Some(w) => {
                        let dir_str = match w.direction {
                            Some(d) => {
                                let compass = match d {
                                    0..=22 | 338..=360 => "N",
                                    23..=67 => "NE",
                                    68..=112 => "E",
                                    113..=157 => "SE",
                                    158..=202 => "S",
                                    203..=247 => "SW",
                                    248..=292 => "W",
                                    293..=337 => "NW",
                                    _ => "VRB",
                                };
                                compass.to_string()
                            }
                            None => "VRB".to_string(),
                        };
                        if let Some(g) = w.gust {
                            format!("{} {}G{}kt", dir_str, w.speed, g)
                        } else {
                            format!("{} {}kt", dir_str, w.speed)
                        }
                    }
                    None => "Calm".to_string(),
                };
                let visibility = m.visibility.as_ref().map(|v| v.statute_miles);

                // Find ceiling (lowest BKN or OVC)
                let ceiling = m.clouds.iter()
                    .filter(|c| {
                        let cov = format!("{:?}", c.coverage);
                        cov == "Broken" || cov == "Overcast"
                    })
                    .filter_map(|c| c.height_agl_ft)
                    .min();

                let fc = format!("{:?}", m.flight_category);
                let time_str = format!("{:02}{:02}{:02}Z", m.time.day, m.time.hour, m.time.minute);

                (temp_c, wind_str, visibility, ceiling, fc, Some(time_str))
            } else {
                (None, "N/A".to_string(), None, None, "Unknown".to_string(), None)
            }
        }
        Err(_) => (None, "N/A".to_string(), None, None, "Unknown".to_string(), None),
    }
}

/// Fetch alert count and highest severity, using cache.
fn fetch_alert_compact(lat: f64, lon: f64, cache: &DiskCache) -> (usize, Option<String>, String) {
    let key = DiskCache::cache_key(&["brief_alerts", &format!("{:.2}", lat), &format!("{:.2}", lon)]);

    // Check cache first
    if let Some(cached) = cache.get(&key, ALERT_CACHE_SECS) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cached) {
            let count = v["count"].as_u64().unwrap_or(0) as usize;
            let highest = v["highest"].as_str().map(|s| s.to_string());
            let hazard = v["hazard_level"].as_str().unwrap_or("none").to_string();
            return (count, highest, hazard);
        }
    }

    match wx_alerts::alerts::fetch_alerts_by_point(lat, lon) {
        Ok(alerts) => {
            let count = alerts.len();
            let highest = alerts.iter()
                .max_by_key(|a| match a.severity {
                    wx_alerts::alerts::Severity::Extreme => 4,
                    wx_alerts::alerts::Severity::Severe => 3,
                    wx_alerts::alerts::Severity::Moderate => 2,
                    wx_alerts::alerts::Severity::Minor => 1,
                    wx_alerts::alerts::Severity::Unknown => 0,
                })
                .map(|a| a.event.clone());

            let hazard_level = if alerts.is_empty() {
                "none"
            } else {
                let max_sev = alerts.iter().map(|a| match a.severity {
                    wx_alerts::alerts::Severity::Extreme => 4u8,
                    wx_alerts::alerts::Severity::Severe => 3,
                    wx_alerts::alerts::Severity::Moderate => 2,
                    wx_alerts::alerts::Severity::Minor => 1,
                    wx_alerts::alerts::Severity::Unknown => 0,
                }).max().unwrap_or(0);
                match max_sev {
                    4 => "extreme",
                    3 => "high",
                    2 => "moderate",
                    1 => "low",
                    _ => "none",
                }
            };

            // Cache result
            let summary = serde_json::json!({
                "count": count,
                "highest": highest,
                "hazard_level": hazard_level,
            });
            cache.set(&key, &summary.to_string());

            (count, highest, hazard_level.to_string())
        }
        Err(_) => (0, None, "unknown".to_string()),
    }
}

/// Fetch only the next forecast period (very compact).
fn fetch_forecast_next_period(lat: f64, lon: f64, cache: &DiskCache) -> Option<String> {
    // Get /points (cached 24h)
    let points_key = DiskCache::cache_key(&["nws_points", &format!("{:.4}", lat), &format!("{:.4}", lon)]);
    let points_body = match cache.get(&points_key, POINTS_CACHE_SECS) {
        Some(cached) => cached,
        None => {
            let url = format!("https://api.weather.gov/points/{},{}", lat, lon);
            match http_get(&url) {
                Some(body) => {
                    cache.set(&points_key, &body);
                    body
                }
                None => return None,
            }
        }
    };

    let points_json: serde_json::Value = serde_json::from_str(&points_body).ok()?;
    let forecast_url = points_json["properties"]["forecast"].as_str()?;

    // Get forecast (cached 1h)
    let forecast_key = DiskCache::cache_key(&[
        "nws_forecast",
        &format!("{:.4}", lat),
        &format!("{:.4}", lon),
        "7day",
    ]);
    let forecast_body = match cache.get(&forecast_key, FORECAST_CACHE_SECS) {
        Some(cached) => cached,
        None => {
            match http_get(forecast_url) {
                Some(body) => {
                    cache.set(&forecast_key, &body);
                    body
                }
                None => return None,
            }
        }
    };

    let forecast_json: serde_json::Value = serde_json::from_str(&forecast_body).ok()?;
    let periods = forecast_json["properties"]["periods"].as_array()?;

    // Return just the first period's short forecast
    periods.first()
        .and_then(|p| p["shortForecast"].as_str())
        .map(|s| {
            // Append temperature if available
            let temp = periods.first()
                .and_then(|p| p["temperature"].as_i64());
            let unit = periods.first()
                .and_then(|p| p["temperatureUnit"].as_str())
                .unwrap_or("F");
            match temp {
                Some(t) => format!("{}, {}{}", s, t, unit),
                None => s.to_string(),
            }
        })
}

/// HTTP GET with NWS User-Agent, returns None on failure (non-fatal).
fn http_get(url: &str) -> Option<String> {
    let resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/geo+json")
        .call()
        .ok()?;

    resp.into_body().read_to_string().ok()
}
