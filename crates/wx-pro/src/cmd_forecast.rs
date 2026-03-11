use crate::output::{print_json, print_error};
use serde_json::json;

const USER_AGENT: &str = "wx-pro/0.1 (Fahrenheit Research)";

pub fn run(lat: f64, lon: f64, hourly: bool, pretty: bool) {
    let points_url = format!("https://api.weather.gov/points/{},{}", lat, lon);
    let points_body = http_get(&points_url);

    let points_json: serde_json::Value = match serde_json::from_str(&points_body) {
        Ok(v) => v,
        Err(e) => print_error(&format!("failed to parse /points response: {}", e)),
    };

    let forecast_key = if hourly { "forecastHourly" } else { "forecast" };
    let forecast_url = match points_json["properties"][forecast_key].as_str() {
        Some(url) => url.to_string(),
        None => print_error(&format!(
            "NWS /points response missing properties.{} — location may be outside US coverage",
            forecast_key
        )),
    };

    let forecast_body = http_get(&forecast_url);

    let forecast_json: serde_json::Value = match serde_json::from_str(&forecast_body) {
        Ok(v) => v,
        Err(e) => print_error(&format!("failed to parse forecast response: {}", e)),
    };

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
