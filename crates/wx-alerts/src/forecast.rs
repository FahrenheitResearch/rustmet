//! NWS Point Forecast from api.weather.gov

use serde::{Deserialize, Serialize};
use serde_json::Value;

const USER_AGENT: &str = "(wx-alerts, contact@example.com)";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointForecast {
    pub location: String,
    pub elevation_m: f64,
    pub periods: Vec<ForecastPeriod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastPeriod {
    pub name: String,
    pub start_time: String,
    pub end_time: String,
    pub temperature: i32,
    pub temperature_unit: String,
    pub wind_speed: String,
    pub wind_direction: String,
    pub short_forecast: String,
    pub detailed_forecast: String,
    pub is_daytime: bool,
    pub precip_probability: Option<u8>,
}

fn get_string(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|val| val.as_str())
        .unwrap_or("")
        .to_string()
}

/// Resolve the points metadata for a lat/lon, returning (forecast_url, hourly_url, location, elevation_m)
fn resolve_point(lat: f64, lon: f64) -> Result<(String, String, String, f64), String> {
    let url = format!("https://api.weather.gov/points/{:.4},{:.4}", lat, lon);

    let body: String = ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/geo+json")
        .call()
        .map_err(|e| format!("Points API request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let json: Value = serde_json::from_str(&body)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let props = json.get("properties")
        .ok_or_else(|| "No properties in points response".to_string())?;

    let forecast_url = get_string(props, "forecast");
    let hourly_url = get_string(props, "forecastHourly");

    if forecast_url.is_empty() {
        return Err("No forecast URL in points response".to_string());
    }

    let location = format!("{}, {}",
        get_string(props, "relativeLocation")
            .as_str()
            .is_empty()
            .then(|| {
                props.get("relativeLocation")
                    .and_then(|rl| rl.get("properties"))
                    .map(|p| format!("{}, {}",
                        get_string(p, "city"),
                        get_string(p, "state"),
                    ))
                    .unwrap_or_default()
            })
            .unwrap_or_default(),
        get_string(props, "gridId"),
    );

    let elevation_m = props.get("elevation")
        .and_then(|e| e.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    Ok((forecast_url, hourly_url, location, elevation_m))
}

fn parse_periods(json: &Value) -> Vec<ForecastPeriod> {
    let periods = match json.get("properties")
        .and_then(|p| p.get("periods"))
        .and_then(|p| p.as_array())
    {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    periods.iter().filter_map(|p| {
        Some(ForecastPeriod {
            name: get_string(p, "name"),
            start_time: get_string(p, "startTime"),
            end_time: get_string(p, "endTime"),
            temperature: p.get("temperature").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            temperature_unit: get_string(p, "temperatureUnit"),
            wind_speed: get_string(p, "windSpeed"),
            wind_direction: get_string(p, "windDirection"),
            short_forecast: get_string(p, "shortForecast"),
            detailed_forecast: get_string(p, "detailedForecast"),
            is_daytime: p.get("isDaytime").and_then(|v| v.as_bool()).unwrap_or(true),
            precip_probability: p.get("probabilityOfPrecipitation")
                .and_then(|pop| pop.get("value"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u8),
        })
    }).collect()
}

fn fetch_forecast_url(url: &str) -> Result<Value, String> {
    let body: String = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/geo+json")
        .call()
        .map_err(|e| format!("Forecast request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str(&body)
        .map_err(|e| format!("JSON parse error: {}", e))
}

/// Fetch 7-day forecast for a lat/lon point
pub fn fetch_forecast(lat: f64, lon: f64) -> Result<PointForecast, String> {
    let (forecast_url, _hourly_url, location, elevation_m) = resolve_point(lat, lon)?;

    let json = fetch_forecast_url(&forecast_url)?;
    let periods = parse_periods(&json);

    Ok(PointForecast {
        location,
        elevation_m,
        periods,
    })
}

/// Fetch hourly forecast for a lat/lon point
pub fn fetch_hourly_forecast(lat: f64, lon: f64) -> Result<Vec<ForecastPeriod>, String> {
    let (_forecast_url, hourly_url, _location, _elevation_m) = resolve_point(lat, lon)?;

    if hourly_url.is_empty() {
        return Err("No hourly forecast URL available".to_string());
    }

    let json = fetch_forecast_url(&hourly_url)?;
    Ok(parse_periods(&json))
}
