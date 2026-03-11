use crate::output::{print_json, print_error};
use serde_json::json;

pub fn run(
    state: Option<&str>,
    lat: Option<f64>,
    lon: Option<f64>,
    _radius: f64,
    pretty: bool,
) {
    if state.is_none() && (lat.is_none() || lon.is_none()) {
        print_error("Provide --state or --lat/--lon");
    }

    let alerts_result = if let Some(st) = state {
        wx_alerts::alerts::fetch_alerts_by_state(st)
    } else if let (Some(la), Some(lo)) = (lat, lon) {
        wx_alerts::alerts::fetch_alerts_by_point(la, lo)
    } else {
        Ok(vec![])
    };

    let alert_items: Vec<serde_json::Value> = match &alerts_result {
        Ok(alerts) => alerts.iter().map(|a| {
            json!({
                "event": a.event,
                "severity": format!("{:?}", a.severity),
                "headline": a.headline,
                "areas": a.areas,
                "expires": a.expires,
            })
        }).collect(),
        Err(_) => vec![],
    };

    let day1 = wx_alerts::spc::fetch_day1_outlook().ok();

    let point_risk = if let (Some(la), Some(lo)) = (lat, lon) {
        day1.as_ref().and_then(|outlook| {
            wx_alerts::spc::point_risk_level(la, lo, outlook).map(|cat| cat.label.clone())
        })
    } else {
        None
    };

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

    let watches = wx_alerts::spc::fetch_active_watches().unwrap_or_default();
    let watch_items: Vec<serde_json::Value> = watches.iter().filter(|w| {
        if let Some(st) = state {
            w.states.iter().any(|s| s.eq_ignore_ascii_case(st))
        } else {
            true
        }
    }).map(|w| {
        json!({
            "number": w.number,
            "type": format!("{:?}", w.watch_type),
            "expires": w.expires,
            "states": w.states,
        })
    }).collect();

    let mds = wx_alerts::spc::fetch_mesoscale_discussions().unwrap_or_default();
    let md_items: Vec<serde_json::Value> = mds.iter().map(|md| {
        json!({
            "number": md.number,
            "concerning": md.concerning,
            "summary": md.summary,
            "expires": md.expires,
        })
    }).collect();

    let severe_alert_count = alerts_result.as_ref().map(|alerts| {
        alerts.iter().filter(|a| {
            let event = a.event.to_lowercase();
            event.contains("tornado") || event.contains("severe thunderstorm")
                || event.contains("hail") || event.contains("wind")
        }).count()
    }).unwrap_or(0);

    print_json(&json!({
        "query": {
            "state": state,
            "lat": lat,
            "lon": lon,
        },
        "spc_outlook": outlook_json,
        "active_watches": {
            "count": watch_items.len(),
            "watches": watch_items,
        },
        "mesoscale_discussions": {
            "count": md_items.len(),
            "discussions": md_items,
        },
        "alerts": {
            "total": alert_items.len(),
            "severe_related": severe_alert_count,
            "items": alert_items,
        },
    }), pretty);
}
