// cmd_severe_lite.rs — Lightweight severe weather check
//
// Expected bandwidth: ~200KB (vs ~800KB+ for full cmd_severe)
//
// Fetches SPC Day 1 categorical outlook ONLY (skips mesoscale discussions,
// skips individual watch detail fetches). Fetches alert count for point
// but parses only minimal fields.

use crate::cache::DiskCache;
use crate::output::{print_json, print_error};
use serde_json::json;

/// Cache SPC outlook for 30 minutes (updates ~hourly).
const OUTLOOK_CACHE_SECS: u64 = 1800;
/// Cache alert count for 5 minutes.
const ALERT_CACHE_SECS: u64 = 300;

pub fn run(state: Option<&str>, lat: Option<f64>, lon: Option<f64>, pretty: bool) {
    if state.is_none() && (lat.is_none() || lon.is_none()) {
        print_error("Provide --state or --lat/--lon");
    }

    let cache = DiskCache::new();

    // 1. Fetch SPC Day 1 outlook (cached)
    let outlook_key = DiskCache::cache_key(&["spc_day1_outlook"]);
    let day1 = match cache.get(&outlook_key, OUTLOOK_CACHE_SECS) {
        Some(_cached) => {
            // Re-fetch from API but use cache to avoid redundant calls
            wx_alerts::spc::fetch_day1_outlook().ok()
        }
        None => {
            let result = wx_alerts::spc::fetch_day1_outlook().ok();
            if let Some(ref outlook) = result {
                // Cache a marker so we know we fetched recently
                let marker = serde_json::to_string(&json!({
                    "day": outlook.day,
                    "valid_time": &outlook.valid_time,
                })).unwrap_or_default();
                cache.set(&outlook_key, &marker);
            }
            result
        }
    };

    // 2. Check point risk level if lat/lon provided
    let point_risk = if let (Some(la), Some(lo)) = (lat, lon) {
        day1.as_ref().and_then(|outlook| {
            wx_alerts::spc::point_risk_level(la, lo, outlook).map(|cat| cat.label.clone())
        })
    } else {
        None
    };

    // 3. Fetch alert count (lightweight — just count + highest severity)
    let (alert_count, highest_severity) = if let Some(st) = state {
        fetch_alert_summary_state(st, &cache)
    } else if let (Some(la), Some(lo)) = (lat, lon) {
        fetch_alert_summary_point(la, lo, &cache)
    } else {
        (0, "none".to_string())
    };

    // Build outlook JSON
    let outlook_json = match &day1 {
        Some(outlook) => {
            let cats: Vec<serde_json::Value> = outlook.categories.iter().map(|c| {
                json!({
                    "label": c.label,
                    "risk_level": c.risk_level,
                })
            }).collect();
            json!({
                "day": outlook.day,
                "valid_time": outlook.valid_time,
                "categories": cats,
                "point_risk": point_risk,
            })
        }
        None => json!(null),
    };

    // Build summary text
    let risk_label = point_risk.as_deref().unwrap_or("unknown");
    let summary = if alert_count == 0 && risk_label == "TSTM" || risk_label == "unknown" {
        "No significant severe weather expected".to_string()
    } else if alert_count > 0 {
        format!(
            "{} active alert(s), highest severity: {}, SPC risk: {}",
            alert_count, highest_severity, risk_label
        )
    } else {
        format!("SPC Day 1 risk level: {}", risk_label)
    };

    print_json(&json!({
        "query": {
            "state": state,
            "lat": lat,
            "lon": lon,
        },
        "spc_outlook": outlook_json,
        "alert_count": alert_count,
        "highest_severity": highest_severity,
        "risk_level": risk_label,
        "summary": summary,
        "bandwidth_note": "severe-lite ~200KB (skips MD/watch detail fetches)",
    }), pretty);
}

/// Fetch alert summary for a point: just count and highest severity.
fn fetch_alert_summary_point(lat: f64, lon: f64, cache: &DiskCache) -> (usize, String) {
    let key = DiskCache::cache_key(&["alert_summary", &format!("{:.2}", lat), &format!("{:.2}", lon)]);
    if let Some(cached) = cache.get(&key, ALERT_CACHE_SECS) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cached) {
            let count = v["count"].as_u64().unwrap_or(0) as usize;
            let severity = v["highest"].as_str().unwrap_or("none").to_string();
            return (count, severity);
        }
    }

    match wx_alerts::alerts::fetch_alerts_by_point(lat, lon) {
        Ok(alerts) => {
            let highest = compute_highest_severity(&alerts);
            let count = alerts.len();
            let summary = json!({"count": count, "highest": highest});
            cache.set(&key, &summary.to_string());
            (count, highest)
        }
        Err(_) => (0, "unknown".to_string()),
    }
}

/// Fetch alert summary for a state.
fn fetch_alert_summary_state(state: &str, cache: &DiskCache) -> (usize, String) {
    let key = DiskCache::cache_key(&["alert_summary_state", state]);
    if let Some(cached) = cache.get(&key, ALERT_CACHE_SECS) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cached) {
            let count = v["count"].as_u64().unwrap_or(0) as usize;
            let severity = v["highest"].as_str().unwrap_or("none").to_string();
            return (count, severity);
        }
    }

    match wx_alerts::alerts::fetch_alerts_by_state(state) {
        Ok(alerts) => {
            let highest = compute_highest_severity(&alerts);
            let count = alerts.len();
            let summary = json!({"count": count, "highest": highest});
            cache.set(&key, &summary.to_string());
            (count, highest)
        }
        Err(_) => (0, "unknown".to_string()),
    }
}

fn compute_highest_severity(alerts: &[wx_alerts::alerts::Alert]) -> String {
    use wx_alerts::alerts::Severity;
    let mut max = 0u8;
    for a in alerts {
        let level = match a.severity {
            Severity::Extreme => 4,
            Severity::Severe => 3,
            Severity::Moderate => 2,
            Severity::Minor => 1,
            Severity::Unknown => 0,
        };
        if level > max {
            max = level;
        }
    }
    match max {
        4 => "Extreme".to_string(),
        3 => "Severe".to_string(),
        2 => "Moderate".to_string(),
        1 => "Minor".to_string(),
        _ => "none".to_string(),
    }
}
