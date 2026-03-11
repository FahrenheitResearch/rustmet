use crate::output::{print_json, print_error};
use crate::cmd_radar;
use serde_json::json;

/// Combined severe weather briefing — the "AI meteorologist" command.
///
/// One call gets everything for a location:
/// - SPC Day 1 outlook risk level
/// - Active watches within area
/// - Active warnings for point
/// - METAR at nearest station
/// - Radar summary (max reflectivity, max velocity)
///
/// Returns a single JSON object with all sections + a text assessment.
pub fn run(lat: f64, lon: f64, pretty: bool) {
    if lat < -90.0 || lat > 90.0 {
        print_error(&format!("Invalid latitude {}: must be between -90 and 90", lat));
    }
    if lon < -180.0 || lon > 180.0 {
        print_error(&format!("Invalid longitude {}: must be between -180 and 180", lon));
    }

    let briefing_start = std::time::Instant::now();

    // === 1. Current conditions (nearest METAR) ===
    let station = wx_obs::stations::nearest_station(lat, lon);
    let station_dist = haversine_km(lat, lon, station.lat, station.lon);

    let conditions = match wx_obs::fetch::fetch_recent_metars(&station.icao, 1) {
        Ok(metars) => {
            if let Some(m) = metars.into_iter().next() {
                json!({
                    "station": station.icao,
                    "station_name": station.name,
                    "distance_km": (station_dist * 10.0).round() / 10.0,
                    "raw": m.raw,
                    "time": format!("{:02}{:02}{:02}Z", m.time.day, m.time.hour, m.time.minute),
                    "temperature_c": m.temperature,
                    "dewpoint_c": m.dewpoint,
                    "wind": m.wind.as_ref().map(|w| json!({
                        "direction": w.direction,
                        "speed_kt": w.speed,
                        "gust_kt": w.gust,
                    })),
                    "visibility_sm": m.visibility.as_ref().map(|v| v.statute_miles),
                    "altimeter_inhg": m.altimeter,
                    "flight_category": format!("{:?}", m.flight_category),
                    "clouds": m.clouds.iter().map(|c| json!({
                        "coverage": format!("{:?}", c.coverage),
                        "height_ft": c.height_agl_ft,
                    })).collect::<Vec<_>>(),
                    "weather": m.weather.iter().map(|w| json!({
                        "intensity": format!("{:?}", w.intensity),
                        "descriptor": w.descriptor,
                        "phenomenon": w.phenomenon,
                    })).collect::<Vec<_>>(),
                })
            } else {
                json!({"station": station.icao, "error": "no observations available"})
            }
        }
        Err(e) => json!({"station": station.icao, "error": format!("{}", e)}),
    };

    // === 2. Active alerts for point ===
    let alerts_json = match wx_alerts::alerts::fetch_alerts_by_point(lat, lon) {
        Ok(alerts) => {
            let items: Vec<serde_json::Value> = alerts.iter().map(|a| {
                json!({
                    "event": a.event,
                    "severity": format!("{:?}", a.severity),
                    "urgency": format!("{:?}", a.urgency),
                    "headline": a.headline,
                    "expires": a.expires,
                })
            }).collect();

            let severe_count = alerts.iter().filter(|a| {
                let event = a.event.to_lowercase();
                event.contains("tornado") || event.contains("severe thunderstorm")
                    || event.contains("hail") || event.contains("wind")
            }).count();

            json!({
                "total": items.len(),
                "severe_related": severe_count,
                "items": items,
            })
        }
        Err(e) => {
            eprintln!("{{\"warning\":\"failed to fetch alerts: {}\"}}", e);
            json!({"total": 0, "severe_related": 0, "items": []})
        }
    };

    // === 3. SPC Day 1 outlook ===
    let spc_outlook = match wx_alerts::spc::fetch_day1_outlook() {
        Ok(outlook) => {
            let point_risk = wx_alerts::spc::point_risk_level(lat, lon, &outlook)
                .map(|cat| cat.label.clone());

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
        Err(_) => json!(null),
    };

    // === 4. Active watches ===
    let watches = wx_alerts::spc::fetch_active_watches().unwrap_or_default();
    let watch_items: Vec<serde_json::Value> = watches.iter().map(|w| {
        json!({
            "number": w.number,
            "type": format!("{:?}", w.watch_type),
            "expires": w.expires,
            "states": w.states,
        })
    }).collect();

    // === 5. Radar summary (nearest site) ===
    let radar_summary = build_radar_summary(lat, lon);

    // === 6. Generate text assessment ===
    let assessment = generate_assessment(
        &conditions,
        &alerts_json,
        &spc_outlook,
        &radar_summary,
    );

    let briefing_ms = briefing_start.elapsed().as_millis();

    print_json(&json!({
        "location": {
            "lat": lat,
            "lon": lon,
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "conditions": conditions,
        "alerts": alerts_json,
        "spc_outlook": spc_outlook,
        "active_watches": {
            "count": watch_items.len(),
            "watches": watch_items,
        },
        "radar_summary": radar_summary,
        "assessment": assessment,
        "performance": {
            "total_ms": briefing_ms,
        },
    }), pretty);
}

/// Build a quick radar summary from the nearest NEXRAD site.
fn build_radar_summary(lat: f64, lon: f64) -> serde_json::Value {
    let site_id = cmd_radar::find_nearest_site(lat, lon);
    let site_info = wx_radar::sites::find_site(&site_id);

    let now = chrono::Utc::now();
    let today = now.format("%Y/%m/%d").to_string();
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y/%m/%d").to_string();

    let latest_key = cmd_radar::find_latest_file(&site_id, &today)
        .or_else(|| cmd_radar::find_latest_file(&site_id, &yesterday));

    let key = match latest_key {
        Some(k) => k,
        None => {
            return json!({
                "site": site_id,
                "error": "no recent data available"
            });
        }
    };

    let url = format!("https://unidata-nexrad-level2.s3.amazonaws.com/{}", key);
    let raw_data = cmd_radar::http_get_bytes(&url);
    let data = cmd_radar::maybe_decompress_gz(raw_data);

    let l2 = match wx_radar::level2::Level2File::parse(&data) {
        Ok(f) => f,
        Err(e) => {
            return json!({
                "site": site_id,
                "error": format!("parse failed: {}", e)
            });
        }
    };

    // Find max reflectivity
    let mut max_ref: f32 = f32::MIN;
    for sweep in &l2.sweeps {
        for radial in &sweep.radials {
            for moment in &radial.moments {
                if moment.product == wx_radar::products::RadarProduct::Reflectivity {
                    for &val in &moment.data {
                        if !val.is_nan() && val > max_ref {
                            max_ref = val;
                        }
                    }
                }
            }
        }
    }

    // Find max velocity
    let mut max_inbound: f32 = 0.0;
    let mut max_outbound: f32 = 0.0;
    for sweep in &l2.sweeps {
        for radial in &sweep.radials {
            for moment in &radial.moments {
                if moment.product == wx_radar::products::RadarProduct::Velocity {
                    for &val in &moment.data {
                        if !val.is_nan() {
                            if val < max_inbound { max_inbound = val; }
                            if val > max_outbound { max_outbound = val; }
                        }
                    }
                }
            }
        }
    }

    let max_gate_to_gate = (max_outbound - max_inbound).abs();

    json!({
        "site": site_id,
        "site_name": site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown"),
        "sweeps": l2.sweeps.len(),
        "max_reflectivity_dbz": if max_ref > f32::MIN { Some((max_ref * 10.0).round() / 10.0) } else { None },
        "max_inbound_velocity_ms": (max_inbound * 10.0).round() / 10.0,
        "max_outbound_velocity_ms": (max_outbound * 10.0).round() / 10.0,
        "max_gate_to_gate_ms": (max_gate_to_gate * 10.0).round() / 10.0,
    })
}

/// Generate a plain-text threat assessment summarizing all data sources.
fn generate_assessment(
    conditions: &serde_json::Value,
    alerts: &serde_json::Value,
    spc_outlook: &serde_json::Value,
    radar: &serde_json::Value,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // SPC outlook risk
    if let Some(risk) = spc_outlook["point_risk"].as_str() {
        let risk_upper = risk.to_uppercase();
        match risk_upper.as_str() {
            "TSTM" | "GENERAL THUNDER" => {
                parts.push("General thunderstorm risk per SPC Day 1 outlook.".to_string());
            }
            "MRGL" | "MARGINAL" => {
                parts.push("MARGINAL risk (1/5) for severe weather per SPC Day 1 outlook.".to_string());
            }
            "SLGT" | "SLIGHT" => {
                parts.push("SLIGHT risk (2/5) for severe weather per SPC Day 1 outlook.".to_string());
            }
            "ENH" | "ENHANCED" => {
                parts.push("ENHANCED risk (3/5) for severe weather per SPC Day 1 outlook. Elevated threat.".to_string());
            }
            "MDT" | "MODERATE" => {
                parts.push("MODERATE risk (4/5) for severe weather per SPC Day 1 outlook. Significant severe weather likely.".to_string());
            }
            "HIGH" => {
                parts.push("HIGH risk (5/5) for severe weather per SPC Day 1 outlook. PARTICULARLY DANGEROUS SITUATION.".to_string());
            }
            _ => {
                parts.push(format!("SPC Day 1 risk: {}.", risk));
            }
        }
    } else if spc_outlook.is_null() {
        parts.push("SPC outlook data unavailable.".to_string());
    } else {
        parts.push("No SPC risk categories for this point.".to_string());
    }

    // Active alerts
    let alert_count = alerts["total"].as_u64().unwrap_or(0);
    let severe_count = alerts["severe_related"].as_u64().unwrap_or(0);
    if alert_count > 0 {
        if severe_count > 0 {
            parts.push(format!("{} active alert(s), {} severe-weather related. Check alert details.", alert_count, severe_count));
        } else {
            parts.push(format!("{} active alert(s), none severe-weather related.", alert_count));
        }
    } else {
        parts.push("No active NWS alerts for this location.".to_string());
    }

    // Radar
    if let Some(max_dbz) = radar["max_reflectivity_dbz"].as_f64() {
        if max_dbz >= 60.0 {
            parts.push(format!("INTENSE radar returns (max {:.0} dBZ) — likely large hail and/or very heavy rain.", max_dbz));
        } else if max_dbz >= 50.0 {
            parts.push(format!("Strong radar returns (max {:.0} dBZ) — thunderstorms with heavy rain likely.", max_dbz));
        } else if max_dbz >= 40.0 {
            parts.push(format!("Moderate radar returns (max {:.0} dBZ) — showers/storms in the area.", max_dbz));
        } else if max_dbz >= 20.0 {
            parts.push(format!("Light radar returns (max {:.0} dBZ) — light precipitation in the area.", max_dbz));
        } else {
            parts.push("Radar clear or very light returns.".to_string());
        }
    }

    let gate_to_gate = radar["max_gate_to_gate_ms"].as_f64().unwrap_or(0.0);
    if gate_to_gate >= 46.0 {
        parts.push(format!("CRITICAL: Max gate-to-gate shear {:.0} m/s — TVS-level rotation detected on radar.", gate_to_gate));
    } else if gate_to_gate >= 25.0 {
        parts.push(format!("Notable velocity couplet detected ({:.0} m/s gate-to-gate) — possible mesocyclone.", gate_to_gate));
    }

    // Current conditions
    if let Some(temp) = conditions["temperature_c"].as_i64() {
        if let Some(dewpt) = conditions["dewpoint_c"].as_i64() {
            let spread = temp - dewpt;
            if spread <= 3 && temp > 15 {
                parts.push(format!("Surface temp/dew spread only {}C — very moist boundary layer.", spread));
            }
        }
    }

    if parts.is_empty() {
        "Insufficient data for assessment.".to_string()
    } else {
        parts.join(" ")
    }
}

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
