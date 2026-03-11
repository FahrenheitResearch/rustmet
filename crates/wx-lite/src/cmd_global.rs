// cmd_global.rs — Open-Meteo API for worldwide weather coverage
//
// Expected bandwidth:
//   Current only: ~3-5KB
//   With --forecast: ~10KB
//
// Works ANYWHERE in the world — no NWS dependency, no auth required.
// Critical for the "life-saving coverage anywhere" requirement.

use crate::cache::DiskCache;
use crate::output::{print_json, print_error};
use serde_json::json;

const USER_AGENT: &str = "wx-lite/0.1 (Fahrenheit Research)";

/// Cache current conditions for 15 minutes.
const CURRENT_CACHE_SECS: u64 = 900;
/// Cache daily forecast for 1 hour.
const FORECAST_CACHE_SECS: u64 = 3600;

/// Map WMO weather code to human-readable description.
fn wmo_weather_description(code: u64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Depositing rime fog",
        51 => "Light drizzle",
        53 => "Moderate drizzle",
        55 => "Dense drizzle",
        56 => "Light freezing drizzle",
        57 => "Dense freezing drizzle",
        61 => "Slight rain",
        63 => "Moderate rain",
        65 => "Heavy rain",
        66 => "Light freezing rain",
        67 => "Heavy freezing rain",
        71 => "Slight snowfall",
        73 => "Moderate snowfall",
        75 => "Heavy snowfall",
        77 => "Snow grains",
        80 => "Slight rain showers",
        81 => "Moderate rain showers",
        82 => "Violent rain showers",
        85 => "Slight snow showers",
        86 => "Heavy snow showers",
        95 => "Thunderstorm",
        96 => "Thunderstorm with slight hail",
        99 => "Thunderstorm with heavy hail",
        _ => "Unknown",
    }
}

pub fn run(lat: f64, lon: f64, include_forecast: bool, pretty: bool) {
    if lat < -90.0 || lat > 90.0 {
        print_error(&format!("Invalid latitude {}: must be between -90 and 90", lat));
    }
    if lon < -180.0 || lon > 180.0 {
        print_error(&format!("Invalid longitude {}: must be between -180 and 180", lon));
    }

    let cache = DiskCache::new();

    // Build URL for current conditions
    let mut url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,relative_humidity_2m,apparent_temperature,\
         wind_speed_10m,wind_direction_10m,wind_gusts_10m,\
         precipitation,weather_code,surface_pressure,cloud_cover\
         &timezone=auto",
        lat, lon
    );

    // Optionally add daily forecast
    if include_forecast {
        url.push_str(
            "&daily=temperature_2m_max,temperature_2m_min,precipitation_sum,\
             wind_speed_10m_max,weather_code,sunrise,sunset\
             &forecast_days=7"
        );
    }

    // Check cache
    let cache_key = DiskCache::cache_key(&[
        "open_meteo",
        &format!("{:.4}", lat),
        &format!("{:.4}", lon),
        if include_forecast { "with_forecast" } else { "current" },
    ]);
    let max_age = if include_forecast { FORECAST_CACHE_SECS } else { CURRENT_CACHE_SECS };

    let body = match cache.get(&cache_key, max_age) {
        Some(cached) => cached,
        None => {
            let resp = ureq::get(&url)
                .header("User-Agent", USER_AGENT)
                .call();

            let body = match resp {
                Ok(r) => {
                    r.into_body()
                        .read_to_string()
                        .unwrap_or_else(|e| print_error(&format!("failed to read Open-Meteo response: {}", e)))
                }
                Err(e) => print_error(&format!("Open-Meteo request failed: {}", e)),
            };

            cache.set(&cache_key, &body);
            body
        }
    };

    let data: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => print_error(&format!("failed to parse Open-Meteo response: {}", e)),
    };

    // Check for API error
    if let Some(reason) = data.get("reason").and_then(|r| r.as_str()) {
        print_error(&format!("Open-Meteo API error: {}", reason));
    }

    // Extract current conditions
    let current = &data["current"];
    let weather_code = current["weather_code"].as_u64().unwrap_or(0);

    let mut result = json!({
        "location": {
            "lat": lat,
            "lon": lon,
            "timezone": data["timezone"],
            "elevation_m": data["elevation"],
        },
        "current": {
            "temperature_c": current["temperature_2m"],
            "apparent_temperature_c": current["apparent_temperature"],
            "relative_humidity_pct": current["relative_humidity_2m"],
            "wind_speed_kmh": current["wind_speed_10m"],
            "wind_direction_deg": current["wind_direction_10m"],
            "wind_gusts_kmh": current["wind_gusts_10m"],
            "precipitation_mm": current["precipitation"],
            "surface_pressure_hpa": current["surface_pressure"],
            "cloud_cover_pct": current["cloud_cover"],
            "weather_code": weather_code,
            "weather_description": wmo_weather_description(weather_code),
            "time": current["time"],
        },
        "source": "Open-Meteo (free, global coverage)",
        "bandwidth_estimate": if include_forecast { "~10KB" } else { "~3-5KB" },
    });

    // Add daily forecast if requested
    if include_forecast {
        if let Some(daily) = data.get("daily") {
            let times = daily["time"].as_array();
            let temp_max = daily["temperature_2m_max"].as_array();
            let temp_min = daily["temperature_2m_min"].as_array();
            let precip = daily["precipitation_sum"].as_array();
            let wind_max = daily["wind_speed_10m_max"].as_array();
            let codes = daily["weather_code"].as_array();
            let sunrise = daily["sunrise"].as_array();
            let sunset = daily["sunset"].as_array();

            if let (Some(times), Some(temp_max), Some(temp_min), Some(precip), Some(wind_max), Some(codes)) =
                (times, temp_max, temp_min, precip, wind_max, codes)
            {
                let days: Vec<serde_json::Value> = times.iter().enumerate().map(|(i, t)| {
                    let code = codes.get(i).and_then(|c| c.as_u64()).unwrap_or(0);
                    json!({
                        "date": t,
                        "temp_max_c": temp_max.get(i),
                        "temp_min_c": temp_min.get(i),
                        "precipitation_sum_mm": precip.get(i),
                        "wind_max_kmh": wind_max.get(i),
                        "weather_code": code,
                        "weather_description": wmo_weather_description(code),
                        "sunrise": sunrise.and_then(|s| s.get(i)),
                        "sunset": sunset.and_then(|s| s.get(i)),
                    })
                }).collect();

                if let Some(obj) = result.as_object_mut() {
                    obj.insert("forecast_daily".to_string(), json!(days));
                }
            }
        }
    }

    print_json(&result, pretty);
}
