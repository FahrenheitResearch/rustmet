// cmd_forecast.rs — NWS 7-day or hourly forecast with cached /points lookup
//
// Expected bandwidth: ~50KB (NWS forecast JSON)
// With cache hit on /points: saves ~5KB per call

use crate::cache::DiskCache;
use crate::output::{print_json, print_error};
use serde_json::json;

const USER_AGENT: &str = "wx-lite/0.1 (Fahrenheit Research)";

/// Cache /points responses for 24 hours (grid assignment doesn't change).
const POINTS_CACHE_SECS: u64 = 86400;

pub fn run(lat: f64, lon: f64, hourly: bool, pretty: bool) {
    let cache = DiskCache::new();

    // Step 1: GET /points/{lat},{lon} — use cache if available
    let points_key = DiskCache::cache_key(&["nws_points", &format!("{:.4}", lat), &format!("{:.4}", lon)]);
    let points_body = match cache.get(&points_key, POINTS_CACHE_SECS) {
        Some(cached) => cached,
        None => {
            let url = format!("https://api.weather.gov/points/{},{}", lat, lon);
            let body = http_get(&url);
            cache.set(&points_key, &body);
            body
        }
    };

    let points_json: serde_json::Value = match serde_json::from_str(&points_body) {
        Ok(v) => v,
        Err(e) => print_error(&format!("failed to parse /points response: {}", e)),
    };

    // Extract the forecast URL from properties
    let forecast_key = if hourly { "forecastHourly" } else { "forecast" };
    let forecast_url = match points_json["properties"][forecast_key].as_str() {
        Some(url) => url.to_string(),
        None => print_error(&format!(
            "NWS /points response missing properties.{} — location may be outside US coverage",
            forecast_key
        )),
    };

    // Step 2: GET the forecast URL (cache 1 hour)
    let forecast_cache_key = DiskCache::cache_key(&[
        "nws_forecast",
        &format!("{:.4}", lat),
        &format!("{:.4}", lon),
        if hourly { "hourly" } else { "7day" },
    ]);
    let forecast_body = match cache.get(&forecast_cache_key, 3600) {
        Some(cached) => cached,
        None => {
            let body = http_get(&forecast_url);
            cache.set(&forecast_cache_key, &body);
            body
        }
    };

    let forecast_json: serde_json::Value = match serde_json::from_str(&forecast_body) {
        Ok(v) => v,
        Err(e) => print_error(&format!("failed to parse forecast response: {}", e)),
    };

    // Extract periods
    let periods = match forecast_json["properties"]["periods"].as_array() {
        Some(arr) => arr,
        None => print_error("forecast response missing properties.periods"),
    };

    let items: Vec<serde_json::Value> = periods
        .iter()
        .map(|p| {
            json!({
                "name": p["name"],
                "startTime": p["startTime"],
                "endTime": p["endTime"],
                "temperature": p["temperature"],
                "temperatureUnit": p["temperatureUnit"],
                "windSpeed": p["windSpeed"],
                "windDirection": p["windDirection"],
                "shortForecast": p["shortForecast"],
                "detailedForecast": p["detailedForecast"],
                "isDaytime": p["isDaytime"],
            })
        })
        .collect();

    // Build location metadata from /points response
    let location = json!({
        "lat": lat,
        "lon": lon,
        "city": points_json["properties"]["relativeLocation"]["properties"]["city"],
        "state": points_json["properties"]["relativeLocation"]["properties"]["state"],
        "gridId": points_json["properties"]["gridId"],
    });

    print_json(&json!({
        "location": location,
        "type": if hourly { "hourly" } else { "7-day" },
        "periods": items,
    }), pretty);
}

/// Blocking HTTP GET with required NWS User-Agent header.
fn http_get(url: &str) -> String {
    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/geo+json")
        .call();

    match response {
        Ok(resp) => {
            resp.into_body()
                .read_to_string()
                .unwrap_or_else(|e| print_error(&format!("failed to read response body: {}", e)))
        }
        Err(e) => print_error(&format!("HTTP request to {} failed: {}", url, e)),
    }
}
