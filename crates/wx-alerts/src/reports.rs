//! SPC Storm Reports (tornado, hail, wind)

use serde::{Deserialize, Serialize};

const USER_AGENT: &str = "(wx-alerts, contact@example.com)";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StormReport {
    pub time: String,
    pub report_type: ReportType,
    pub magnitude: Option<f64>,
    pub location: String,
    pub state: String,
    pub lat: f64,
    pub lon: f64,
    pub comments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReportType {
    Tornado,
    Hail,
    Wind,
}

fn fetch_csv(url: &str) -> Result<String, String> {
    ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))
}

/// Parse SPC storm report CSV
///
/// CSV format varies slightly but generally:
/// Time,F_Scale/Speed/Size,Location,County,State,Lat,Lon,Comments
///
/// First line is a header row.
fn parse_csv(body: &str, report_type: ReportType) -> Vec<StormReport> {
    let mut reports = Vec::new();
    let mut lines = body.lines();

    // Skip header line
    if lines.next().is_none() {
        return reports;
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.splitn(8, ',').collect();
        if fields.len() < 7 {
            continue;
        }

        let time = fields[0].trim().to_string();

        let magnitude = fields[1].trim().parse::<f64>().ok().and_then(|v| {
            // Filter out sentinel values
            if v == 0.0 && report_type == ReportType::Tornado {
                Some(v) // EF0 is valid
            } else if v == 0.0 {
                None
            } else {
                Some(v)
            }
        });

        let location = fields[2].trim().to_string();
        // fields[3] is county — we skip it
        let state = fields[4].trim().to_string();

        let lat = match fields[5].trim().parse::<f64>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let lon = match fields[6].trim().parse::<f64>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let comments = if fields.len() > 7 {
            fields[7].trim().to_string()
        } else {
            String::new()
        };

        reports.push(StormReport {
            time,
            report_type: report_type.clone(),
            magnitude,
            location,
            state,
            lat,
            lon,
            comments,
        });
    }

    reports
}

fn fetch_reports(prefix: &str) -> Result<Vec<StormReport>, String> {
    let torn_url = format!("https://www.spc.noaa.gov/climo/reports/{}_torn.csv", prefix);
    let hail_url = format!("https://www.spc.noaa.gov/climo/reports/{}_hail.csv", prefix);
    let wind_url = format!("https://www.spc.noaa.gov/climo/reports/{}_wind.csv", prefix);

    let mut all_reports = Vec::new();

    // Fetch each type, tolerating failures (there may be no reports)
    if let Ok(body) = fetch_csv(&torn_url) {
        all_reports.extend(parse_csv(&body, ReportType::Tornado));
    }

    if let Ok(body) = fetch_csv(&hail_url) {
        all_reports.extend(parse_csv(&body, ReportType::Hail));
    }

    if let Ok(body) = fetch_csv(&wind_url) {
        all_reports.extend(parse_csv(&body, ReportType::Wind));
    }

    Ok(all_reports)
}

/// Fetch today's storm reports
pub fn fetch_today_reports() -> Result<Vec<StormReport>, String> {
    fetch_reports("today")
}

/// Fetch yesterday's storm reports
pub fn fetch_yesterday_reports() -> Result<Vec<StormReport>, String> {
    fetch_reports("yesterday")
}
