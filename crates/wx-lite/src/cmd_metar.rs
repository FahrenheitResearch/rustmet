// cmd_metar.rs — Fetch current METAR observation
//
// Expected bandwidth: ~500B (single METAR text from aviationweather.gov)

use crate::output::{print_json, print_error};
use serde_json::json;

pub fn run(station: &str, hours: u32, pretty: bool) {
    match wx_obs::fetch::fetch_recent_metars(station, hours) {
        Ok(metars) => {
            let results: Vec<serde_json::Value> = metars.iter().map(|m| {
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

            print_json(&json!({
                "station": station.to_uppercase(),
                "count": results.len(),
                "observations": results,
            }), pretty);
        }
        Err(e) => print_error(&e),
    }
}
