use crate::output::{print_json, print_error};
use serde_json::json;

pub fn run(station: &str, hours: u32, pretty: bool) {
    match wx_obs::fetch::fetch_recent_metars(station, hours) {
        Ok(mut metars) => {
            metars.sort_by(|a, b| {
                let ta = (a.time.day, a.time.hour, a.time.minute);
                let tb = (b.time.day, b.time.hour, b.time.minute);
                ta.cmp(&tb)
            });

            let observations: Vec<serde_json::Value> = metars.iter().map(|m| {
                json!({
                    "raw": m.raw,
                    "station": m.station,
                    "time": format!("{:02}{:02}{:02}Z", m.time.day, m.time.hour, m.time.minute),
                    "wind": m.wind.as_ref().map(|w| json!({
                        "direction": w.direction,
                        "speed_kt": w.speed,
                        "gust_kt": w.gust,
                    })),
                    "visibility_sm": m.visibility.as_ref().map(|v| v.statute_miles),
                    "temperature_c": m.temperature,
                    "dewpoint_c": m.dewpoint,
                    "altimeter_inhg": m.altimeter,
                    "clouds": m.clouds.iter().map(|c| json!({
                        "coverage": format!("{:?}", c.coverage),
                        "height_ft": c.height_agl_ft,
                    })).collect::<Vec<_>>(),
                    "weather": m.weather.iter().map(|w| json!({
                        "intensity": format!("{:?}", w.intensity),
                        "descriptor": w.descriptor,
                        "phenomenon": w.phenomenon,
                    })).collect::<Vec<_>>(),
                    "flight_category": format!("{:?}", m.flight_category),
                })
            }).collect();

            let temps: Vec<i32> = metars.iter().filter_map(|m| m.temperature).collect();
            let altimeters: Vec<f64> = metars.iter().filter_map(|m| m.altimeter).collect();
            let winds: Vec<u16> = metars.iter()
                .filter_map(|m| m.wind.as_ref().map(|w| w.speed))
                .collect();
            let gusts: Vec<u16> = metars.iter()
                .filter_map(|m| m.wind.as_ref().and_then(|w| w.gust))
                .collect();

            let temp_min = temps.iter().copied().min();
            let temp_max = temps.iter().copied().max();
            let wind_max = winds.iter().copied().max();
            let gust_max = gusts.iter().copied().max();
            let pressure_min = altimeters.iter().copied().reduce(f64::min);
            let pressure_max = altimeters.iter().copied().reduce(f64::max);

            let n = metars.len();
            let trend_temp = if n >= 6 {
                let first: Vec<f64> = metars.iter().take(3)
                    .filter_map(|m| m.temperature.map(|t| t as f64)).collect();
                let last: Vec<f64> = metars.iter().rev().take(3)
                    .filter_map(|m| m.temperature.map(|t| t as f64)).collect();
                compute_trend(&avg(&first), &avg(&last), 2.0)
            } else {
                "insufficient_data".to_string()
            };

            let trend_pressure = if n >= 6 {
                let first: Vec<f64> = metars.iter().take(3)
                    .filter_map(|m| m.altimeter).collect();
                let last: Vec<f64> = metars.iter().rev().take(3)
                    .filter_map(|m| m.altimeter).collect();
                compute_trend(&avg(&first), &avg(&last), 0.04)
            } else {
                "insufficient_data".to_string()
            };

            let trend_wind = if n >= 6 {
                let first: Vec<f64> = metars.iter().take(3)
                    .filter_map(|m| m.wind.as_ref().map(|w| w.speed as f64)).collect();
                let last: Vec<f64> = metars.iter().rev().take(3)
                    .filter_map(|m| m.wind.as_ref().map(|w| w.speed as f64)).collect();
                compute_trend(&avg(&first), &avg(&last), 5.0)
            } else {
                "insufficient_data".to_string()
            };

            print_json(&json!({
                "station": station.to_uppercase(),
                "hours": hours,
                "count": observations.len(),
                "observations": observations,
                "summary": {
                    "temp_min_c": temp_min,
                    "temp_max_c": temp_max,
                    "wind_max_kt": wind_max,
                    "gust_max_kt": gust_max,
                    "pressure_min_inhg": pressure_min,
                    "pressure_max_inhg": pressure_max,
                },
                "trend": {
                    "temperature": trend_temp,
                    "pressure": trend_pressure,
                    "wind": trend_wind,
                },
            }), pretty);
        }
        Err(e) => print_error(&e),
    }
}

fn avg(vals: &[f64]) -> Option<f64> {
    if vals.is_empty() {
        None
    } else {
        Some(vals.iter().sum::<f64>() / vals.len() as f64)
    }
}

fn compute_trend(first_avg: &Option<f64>, last_avg: &Option<f64>, threshold: f64) -> String {
    match (first_avg, last_avg) {
        (Some(first), Some(last)) => {
            let diff = last - first;
            if diff > threshold {
                "rising".to_string()
            } else if diff < -threshold {
                "falling".to_string()
            } else {
                "steady".to_string()
            }
        }
        _ => "insufficient_data".to_string(),
    }
}
