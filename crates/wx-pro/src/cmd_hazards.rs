use crate::output::{print_json, print_error};
use serde_json::json;
use wx_alerts::alerts::Severity;

fn categorize_event(event: &str) -> &'static str {
    let e = event.to_uppercase();
    if e.contains("FIRE") || e.contains("RED FLAG") {
        "fire_weather"
    } else if e.contains("TORNADO") {
        "tornado"
    } else if e.contains("THUNDERSTORM") || e.contains("LIGHTNING") {
        "thunderstorm"
    } else if e.contains("HURRICANE") || e.contains("TROPICAL") {
        "tropical"
    } else if e.contains("FLOOD") || e.contains("FLASH") {
        "flooding"
    } else if e.contains("WINTER") || e.contains("ICE") || e.contains("BLIZZARD")
        || e.contains("FREEZE") || e.contains("FROST") || e.contains("WIND CHILL")
        || e.contains("SNOW")
    {
        "winter"
    } else if e.contains("MARINE") || e.contains("SMALL CRAFT") || e.contains("COASTAL")
        || e.contains("RIP CURRENT") || e.contains("SURF")
    {
        "marine"
    } else if e.contains("WIND") || e.contains("GALE") || e.contains("HIGH WIND") {
        "wind"
    } else {
        "other"
    }
}

fn compute_threat_level(alerts: &[wx_alerts::alerts::Alert]) -> &'static str {
    if alerts.is_empty() {
        return "none";
    }

    let mut max = 0u8; // 1=minor, 2=moderate, 3=severe, 4=extreme
    for a in alerts {
        let level = match a.severity {
            Severity::Extreme => 4,
            Severity::Severe => 3,
            Severity::Moderate => 2,
            Severity::Minor => 1,
            Severity::Unknown => 1,
        };
        if level > max {
            max = level;
        }
    }

    match max {
        4 => "extreme",
        3 => "high",
        2 => "moderate",
        _ => "low",
    }
}

pub fn run(lat: f64, lon: f64, pretty: bool) {
    if lat < -90.0 || lat > 90.0 {
        print_error(&format!("Invalid latitude {}: must be between -90 and 90", lat));
    }
    if lon < -180.0 || lon > 180.0 {
        print_error(&format!("Invalid longitude {}: must be between -180 and 180", lon));
    }

    let alerts = match wx_alerts::alerts::fetch_alerts_by_point(lat, lon) {
        Ok(a) => a,
        Err(e) => print_error(&e),
    };

    let threat_level = compute_threat_level(&alerts);

    let category_names = [
        "fire_weather", "winter", "flooding", "wind",
        "thunderstorm", "tornado", "tropical", "marine", "other",
    ];

    let mut categories = serde_json::Map::new();
    for name in &category_names {
        categories.insert(name.to_string(), json!([]));
    }

    for a in &alerts {
        let cat = categorize_event(&a.event);
        let entry = json!({
            "event": a.event,
            "severity": format!("{:?}", a.severity),
            "headline": a.headline,
        });
        if let Some(arr) = categories.get_mut(cat) {
            if let Some(vec) = arr.as_array_mut() {
                vec.push(entry);
            }
        }
    }

    // Build summary of active categories
    let mut active: Vec<String> = Vec::new();
    for name in &category_names {
        if let Some(arr) = categories.get(*name) {
            if let Some(vec) = arr.as_array() {
                if !vec.is_empty() {
                    active.push(format!("{} ({})", name, vec.len()));
                }
            }
        }
    }

    let summary = if active.is_empty() {
        "No active hazards".to_string()
    } else {
        format!("Active hazards: {}", active.join(", "))
    };

    print_json(&json!({
        "location": { "lat": lat, "lon": lon },
        "threat_level": threat_level,
        "hazard_count": alerts.len(),
        "categories": categories,
        "summary": summary,
    }), pretty);
}
