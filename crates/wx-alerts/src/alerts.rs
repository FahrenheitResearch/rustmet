//! NWS Active Alerts from api.weather.gov

use serde::{Deserialize, Serialize};
use serde_json::Value;

const BASE_URL: &str = "https://api.weather.gov/alerts/active";
const USER_AGENT: &str = "(wx-alerts, contact@example.com)";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub event: String,
    pub severity: Severity,
    pub certainty: Certainty,
    pub urgency: Urgency,
    pub headline: String,
    pub description: String,
    pub instruction: Option<String>,
    pub sender: String,
    pub effective: String,
    pub expires: String,
    pub areas: Vec<String>,
    pub geocode_fips: Vec<String>,
    pub geocode_ugc: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Severity {
    Extreme,
    Severe,
    Moderate,
    Minor,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Certainty {
    Observed,
    Likely,
    Possible,
    Unlikely,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Urgency {
    Immediate,
    Expected,
    Future,
    Past,
    Unknown,
}

impl Severity {
    fn from_str(s: &str) -> Self {
        match s {
            "Extreme" => Severity::Extreme,
            "Severe" => Severity::Severe,
            "Moderate" => Severity::Moderate,
            "Minor" => Severity::Minor,
            _ => Severity::Unknown,
        }
    }
}

impl Certainty {
    fn from_str(s: &str) -> Self {
        match s {
            "Observed" => Certainty::Observed,
            "Likely" => Certainty::Likely,
            "Possible" => Certainty::Possible,
            "Unlikely" => Certainty::Unlikely,
            _ => Certainty::Unknown,
        }
    }
}

impl Urgency {
    fn from_str(s: &str) -> Self {
        match s {
            "Immediate" => Urgency::Immediate,
            "Expected" => Urgency::Expected,
            "Future" => Urgency::Future,
            "Past" => Urgency::Past,
            _ => Urgency::Unknown,
        }
    }
}

fn get_string(props: &Value, key: &str) -> String {
    props.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn get_string_opt(props: &Value, key: &str) -> Option<String> {
    props.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn parse_areas(props: &Value) -> Vec<String> {
    // areaDesc is a comma-separated string
    props.get("areaDesc")
        .and_then(|v| v.as_str())
        .map(|s| s.split("; ").map(|a| a.trim().to_string()).collect())
        .unwrap_or_default()
}

fn parse_geocodes(props: &Value, key: &str) -> Vec<String> {
    props.get("geocode")
        .and_then(|gc| gc.get(key))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_alert(feature: &Value) -> Option<Alert> {
    let props = feature.get("properties")?;

    Some(Alert {
        id: get_string(props, "id"),
        event: get_string(props, "event"),
        severity: Severity::from_str(
            props.get("severity").and_then(|v| v.as_str()).unwrap_or("Unknown"),
        ),
        certainty: Certainty::from_str(
            props.get("certainty").and_then(|v| v.as_str()).unwrap_or("Unknown"),
        ),
        urgency: Urgency::from_str(
            props.get("urgency").and_then(|v| v.as_str()).unwrap_or("Unknown"),
        ),
        headline: get_string(props, "headline"),
        description: get_string(props, "description"),
        instruction: get_string_opt(props, "instruction"),
        sender: get_string(props, "senderName"),
        effective: get_string(props, "effective"),
        expires: get_string(props, "expires"),
        areas: parse_areas(props),
        geocode_fips: parse_geocodes(props, "FIPS6"),
        geocode_ugc: parse_geocodes(props, "UGC"),
    })
}

fn fetch_alerts_from_url(url: &str) -> Result<Vec<Alert>, String> {
    let body: String = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/geo+json")
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let json: Value = serde_json::from_str(&body)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let features = json.get("features")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "No 'features' array in response".to_string())?;

    let alerts: Vec<Alert> = features.iter()
        .filter_map(parse_alert)
        .collect();

    Ok(alerts)
}

/// Fetch all active alerts nationwide
pub fn fetch_active_alerts() -> Result<Vec<Alert>, String> {
    fetch_alerts_from_url(BASE_URL)
}

/// Fetch active alerts for a specific state (2-letter code)
pub fn fetch_alerts_by_state(state: &str) -> Result<Vec<Alert>, String> {
    let url = format!("{}?area={}", BASE_URL, state.to_uppercase());
    fetch_alerts_from_url(&url)
}

/// Fetch active alerts for a lat/lon point
pub fn fetch_alerts_by_point(lat: f64, lon: f64) -> Result<Vec<Alert>, String> {
    let url = format!("{}?point={:.4},{:.4}", BASE_URL, lat, lon);
    fetch_alerts_from_url(&url)
}

/// Fetch active alerts for a specific zone (e.g., "OKC040")
pub fn fetch_alerts_by_zone(zone: &str) -> Result<Vec<Alert>, String> {
    let url = format!("{}?zone={}", BASE_URL, zone);
    fetch_alerts_from_url(&url)
}

/// Filter alerts to only severe weather types
pub fn filter_severe<'a>(alerts: &'a [Alert]) -> Vec<&'a Alert> {
    const SEVERE_EVENTS: &[&str] = &[
        "Tornado Warning",
        "Tornado Watch",
        "Severe Thunderstorm Warning",
        "Severe Thunderstorm Watch",
        "Flash Flood Warning",
        "Flash Flood Watch",
        "Extreme Wind Warning",
        "Hurricane Warning",
        "Hurricane Watch",
        "Tropical Storm Warning",
        "Tropical Storm Watch",
        "Storm Surge Warning",
        "Blizzard Warning",
        "Ice Storm Warning",
        "Dust Storm Warning",
        "Tsunami Warning",
        "Earthquake Warning",
        "Volcano Warning",
        "Special Weather Statement",
        "Severe Weather Statement",
    ];

    alerts.iter()
        .filter(|a| {
            SEVERE_EVENTS.iter().any(|evt| a.event == *evt)
                || a.severity == Severity::Extreme
                || a.severity == Severity::Severe
        })
        .collect()
}
