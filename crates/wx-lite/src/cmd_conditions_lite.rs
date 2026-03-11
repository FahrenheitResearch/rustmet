// cmd_conditions_lite.rs — Current conditions (METAR only by default)
//
// Expected bandwidth:
//   Without alerts: ~500B (METAR text only)
//   With --with-alerts: ~50KB (METAR + NWS alerts by point)
//
// Key difference from wx-agent: alerts are OFF by default to save bandwidth.
// Uses cache for station lookup (static data, cached forever).

use crate::cache::DiskCache;
use crate::output::print_json;
use serde_json::json;

/// Haversine distance in kilometers.
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

pub fn run(lat: f64, lon: f64, with_alerts: bool, pretty: bool) {
    let cache = DiskCache::new();

    // 1. Find nearest METAR station (cached forever — static data)
    let station_key = DiskCache::cache_key(&["nearest_station", &format!("{:.2}", lat), &format!("{:.2}", lon)]);
    let station = wx_obs::stations::nearest_station(lat, lon);
    let distance_km = haversine_km(lat, lon, station.lat, station.lon);

    // Cache station info for future lookups
    let station_json = serde_json::to_string(&json!({
        "icao": station.icao,
        "name": station.name,
        "lat": station.lat,
        "lon": station.lon,
    })).unwrap_or_default();
    cache.set(&station_key, &station_json);

    // 2. Fetch latest METAR from that station
    let (obs_json, flight_category) = match wx_obs::fetch::fetch_recent_metars(&station.icao, 1) {
        Ok(metars) => {
            if let Some(m) = metars.into_iter().next() {
                let fc = format!("{:?}", m.flight_category);
                let obs = json!({
                    "raw": m.raw,
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
                    "remarks": m.remarks,
                });
                (obs, fc)
            } else {
                (json!(null), "Unknown".to_string())
            }
        }
        Err(e) => {
            eprintln!("warning: failed to fetch METAR for {}: {}", station.icao, e);
            (json!(null), "Unknown".to_string())
        }
    };

    // 3. Optionally fetch alerts (OFF by default for bandwidth savings)
    let alerts_json = if with_alerts {
        match wx_alerts::alerts::fetch_alerts_by_point(lat, lon) {
            Ok(alerts) => {
                let items: Vec<serde_json::Value> = alerts.iter().map(|a| {
                    json!({
                        "event": a.event,
                        "severity": format!("{:?}", a.severity),
                        "urgency": format!("{:?}", a.urgency),
                        "certainty": format!("{:?}", a.certainty),
                        "headline": a.headline,
                        "areas": a.areas,
                        "sender": a.sender,
                        "effective": a.effective,
                        "expires": a.expires,
                        "description": a.description,
                    })
                }).collect();

                json!({
                    "count": items.len(),
                    "items": items,
                })
            }
            Err(e) => {
                eprintln!("warning: failed to fetch alerts: {}", e);
                json!({
                    "count": 0,
                    "items": [],
                })
            }
        }
    } else {
        json!({
            "note": "alerts disabled by default — use --with-alerts to include (~50KB)",
            "count": null,
            "items": [],
        })
    };

    // 4. Combine into unified response
    let result = json!({
        "location": {
            "lat": lat,
            "lon": lon,
        },
        "nearest_station": {
            "icao": station.icao,
            "name": station.name,
            "distance_km": (distance_km * 10.0).round() / 10.0,
        },
        "observation": obs_json,
        "alerts": alerts_json,
        "flight_category": flight_category,
        "bandwidth_mode": if with_alerts { "full" } else { "lite" },
    });

    print_json(&result, pretty);
}
