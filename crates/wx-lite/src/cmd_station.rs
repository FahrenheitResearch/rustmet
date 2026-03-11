// cmd_station.rs — Local station lookup (no network)
//
// Expected bandwidth: 0B (all data is compiled into binary)

use crate::output::{print_json, print_error};
use serde_json::json;

pub fn run(query: &str, lat: Option<f64>, lon: Option<f64>, radius_km: f64, pretty: bool) {
    if let (Some(la), Some(lo)) = (lat, lon) {
        // Find stations near a point
        let stations = wx_obs::stations::stations_within(la, lo, radius_km);
        let items: Vec<serde_json::Value> = stations.iter().map(|s| {
            json!({
                "icao": s.icao,
                "name": s.name,
                "state": s.state,
                "lat": s.lat,
                "lon": s.lon,
                "elevation_m": s.elevation_m,
            })
        }).collect();
        print_json(&json!({
            "query": format!("{:.2},{:.2} within {}km", la, lo, radius_km),
            "count": items.len(),
            "stations": items,
        }), pretty);
    } else if !query.is_empty() {
        // Look up by ICAO
        match wx_obs::stations::find_station(query) {
            Some(s) => {
                print_json(&json!({
                    "icao": s.icao,
                    "name": s.name,
                    "state": s.state,
                    "lat": s.lat,
                    "lon": s.lon,
                    "elevation_m": s.elevation_m,
                }), pretty);
            }
            None => print_error(&format!("Station '{}' not found", query)),
        }
    } else {
        print_error("Provide --id or --lat/--lon");
    }
}
