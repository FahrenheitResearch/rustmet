// cmd_alerts.rs — NWS weather alerts by point or state
//
// Expected bandwidth: ~50-200KB (NWS alerts JSON varies by active count)

use crate::output::{print_json, print_error};
use serde_json::json;

pub fn run(state: Option<&str>, lat: Option<f64>, lon: Option<f64>, all: bool, pretty: bool) {
    // Validate coordinates if provided
    if let Some(la) = lat {
        if la < -90.0 || la > 90.0 {
            print_error(&format!("Invalid latitude {}: must be between -90 and 90", la));
        }
    }
    if let Some(lo) = lon {
        if lo < -180.0 || lo > 180.0 {
            print_error(&format!("Invalid longitude {}: must be between -180 and 180", lo));
        }
    }

    let result = if let Some(st) = state {
        wx_alerts::alerts::fetch_alerts_by_state(st)
    } else if let (Some(la), Some(lo)) = (lat, lon) {
        wx_alerts::alerts::fetch_alerts_by_point(la, lo)
    } else if all {
        wx_alerts::alerts::fetch_active_alerts()
    } else {
        print_error("Provide --state or --lat/--lon to filter alerts (use --all for national feed)");
    };

    match result {
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

            let severe_count = alerts.iter().filter(|a| {
                matches!(a.severity, wx_alerts::alerts::Severity::Severe | wx_alerts::alerts::Severity::Extreme)
            }).count();

            print_json(&json!({
                "total_alerts": items.len(),
                "severe_count": severe_count,
                "alerts": items,
            }), pretty);
        }
        Err(e) => print_error(&e),
    }
}
