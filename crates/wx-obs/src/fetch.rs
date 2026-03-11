use crate::metar::{Metar, parse_metar};
use crate::taf::{Taf, parse_taf};

const METAR_URL: &str = "https://aviationweather.gov/api/data/metar";
const TAF_URL: &str = "https://aviationweather.gov/api/data/taf";

/// Fetch current METAR for a station from aviationweather.gov.
pub fn fetch_metar(station: &str) -> Result<Metar, String> {
    let metars = fetch_recent_metars(station, 1)?;
    metars.into_iter().next().ok_or_else(|| format!("no METAR found for {}", station))
}

/// Fetch recent METARs (last N hours) for a station.
pub fn fetch_recent_metars(station: &str, hours: u32) -> Result<Vec<Metar>, String> {
    let station = station.trim().to_uppercase();
    if station.len() != 4 {
        return Err("station must be a 4-letter ICAO code".into());
    }

    let url = format!("{}?ids={}&format=raw&hours={}", METAR_URL, station, hours);
    let body = http_get(&url)?;

    let mut results = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("No ") {
            continue;
        }
        match parse_metar(line) {
            Ok(m) => results.push(m),
            Err(_) => {
                // Skip unparseable lines (could be headers, etc.)
            }
        }
    }

    Ok(results)
}

/// Fetch current TAF for a station.
pub fn fetch_taf(station: &str) -> Result<Taf, String> {
    let station = station.trim().to_uppercase();
    if station.len() != 4 {
        return Err("station must be a 4-letter ICAO code".into());
    }

    let url = format!("{}?ids={}&format=raw", TAF_URL, station);
    let body = http_get(&url)?;

    // TAF can span multiple lines — join into one.
    let joined = body.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("No "))
        .collect::<Vec<_>>()
        .join(" ");

    if joined.is_empty() {
        return Err(format!("no TAF found for {}", station));
    }

    parse_taf(&joined)
}

/// Fetch METARs for multiple stations at once (comma-separated query).
pub fn fetch_metars_bulk(stations: &[&str]) -> Result<Vec<Metar>, String> {
    if stations.is_empty() {
        return Ok(Vec::new());
    }

    let ids = stations.iter()
        .map(|s| s.trim().to_uppercase())
        .collect::<Vec<_>>()
        .join(",");

    let url = format!("{}?ids={}&format=raw&hours=1", METAR_URL, ids);
    let body = http_get(&url)?;

    let mut results = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("No ") {
            continue;
        }
        match parse_metar(line) {
            Ok(m) => results.push(m),
            Err(_) => {}
        }
    }

    Ok(results)
}

/// Simple blocking HTTP GET using ureq.
fn http_get(url: &str) -> Result<String, String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("failed to read response body: {}", e))?;

    Ok(body)
}
