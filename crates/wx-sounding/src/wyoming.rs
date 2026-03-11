/// Fetch and parse upper-air soundings from the University of Wyoming archive.
///
/// URL format:
/// `http://weather.uwyo.edu/cgi-bin/sounding.py?region=naconf&TYPE=TEXT%3ALIST&YEAR={year}&MONTH={month:02}&FROM={day:02}{hour:02}&TO={day:02}{hour:02}&STNM={station}`

use crate::raob_stations::find_raob_station;
use crate::types::{Sounding, SoundingIndices, SoundingLevel, SurfaceObs};

/// Fetch a sounding from the University of Wyoming archive.
///
/// `station` can be a WMO number (e.g. "72357") or ICAO (e.g. "OUN").
/// `hour` is typically 0 or 12 (00Z or 12Z synoptic times).
pub fn fetch_sounding(
    station: &str,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
) -> Result<Sounding, String> {
    // Resolve station to WMO number for the URL
    let wmo = if let Some(s) = find_raob_station(station) {
        s.wmo.to_string()
    } else {
        // Assume it's already a WMO number or station ID the server understands
        station.to_string()
    };

    let url = format!(
        "https://weather.uwyo.edu/wsgi/sounding?datetime={}-{:02}-{:02}+{:02}%3A00%3A00&id={}&src=UNKNOWN&type=TEXT%3ALIST",
        year, month, day, hour, wmo
    );

    let body = http_get(&url)?;
    parse_wyoming_html(&body, station)
}

/// Fetch the most recent 12Z sounding for a station.
pub fn fetch_latest_12z(station: &str) -> Result<Sounding, String> {
    let now = chrono::Utc::now();
    let dt = if now.format("%H").to_string().parse::<u32>().unwrap_or(0) < 12 {
        // Before 12Z today, use yesterday's 12Z
        now - chrono::Duration::days(1)
    } else {
        now
    };
    let year = dt.format("%Y").to_string().parse::<i32>().unwrap_or(2024);
    let month = dt.format("%m").to_string().parse::<u32>().unwrap_or(1);
    let day = dt.format("%d").to_string().parse::<u32>().unwrap_or(1);
    fetch_sounding(station, year, month, day, 12)
}

/// Fetch the most recent 00Z sounding for a station.
pub fn fetch_latest_00z(station: &str) -> Result<Sounding, String> {
    let now = chrono::Utc::now();
    let dt = if now.format("%H").to_string().parse::<u32>().unwrap_or(0) < 3 {
        // Very early UTC, use previous day's 00Z
        now - chrono::Duration::days(1)
    } else {
        now
    };
    let year = dt.format("%Y").to_string().parse::<i32>().unwrap_or(2024);
    let month = dt.format("%m").to_string().parse::<u32>().unwrap_or(1);
    let day = dt.format("%d").to_string().parse::<u32>().unwrap_or(1);
    fetch_sounding(station, year, month, day, 0)
}

/// Perform an HTTP GET request and return the response body as a string.
fn http_get(url: &str) -> Result<String, String> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .build(),
    );
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if status != 200 {
        return Err(format!("HTTP {} from Wyoming server", status));
    }
    Ok(body)
}

/// Parse the HTML response from the University of Wyoming sounding server.
fn parse_wyoming_html(html: &str, station_id: &str) -> Result<Sounding, String> {
    // Extract <pre> block content
    let pre_content = extract_pre_block(html)?;

    // Parse station info from header
    let (station_number, obs_time, station_lat, station_lon, station_elev, station_name) =
        parse_station_info(&pre_content, station_id);

    // Parse the data table
    let levels = parse_data_table(&pre_content)?;

    if levels.is_empty() {
        return Err("No valid sounding levels parsed".to_string());
    }

    // Build surface observation from first level
    let surface = if !levels.is_empty() {
        Some(SurfaceObs {
            pressure: levels[0].pressure,
            temperature: levels[0].temperature,
            dewpoint: levels[0].dewpoint,
            wind_dir: levels[0].wind_dir,
            wind_speed: levels[0].wind_speed,
        })
    } else {
        None
    };

    Ok(Sounding {
        station: station_number,
        station_name,
        lat: station_lat,
        lon: station_lon,
        elevation_m: station_elev,
        time: obs_time,
        levels,
        surface,
        indices: SoundingIndices::default(),
    })
}

/// Extract the first `<pre>` block from the HTML.
fn extract_pre_block(html: &str) -> Result<String, String> {
    // Case-insensitive search for <pre> tags
    let lower = html.to_lowercase();
    let start = lower
        .find("<pre>")
        .ok_or_else(|| "No <pre> block found in Wyoming response".to_string())?;
    let end = lower[start..]
        .find("</pre>")
        .ok_or_else(|| "No closing </pre> found".to_string())?;

    // Extract the content between tags (using original case)
    let content = &html[start + 5..start + end];
    Ok(content.to_string())
}

/// Parse station information from the sounding header text.
fn parse_station_info(
    text: &str,
    fallback_id: &str,
) -> (String, String, f64, f64, f64, String) {
    let mut station_number = fallback_id.to_string();
    let mut obs_time = String::new();
    let mut lat = f64::NAN;
    let mut lon = f64::NAN;
    let mut elev = f64::NAN;
    let mut name = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Station number:") {
            station_number = trimmed
                .strip_prefix("Station number:")
                .unwrap_or("")
                .trim()
                .to_string();
        } else if trimmed.starts_with("Observation time:") {
            obs_time = trimmed
                .strip_prefix("Observation time:")
                .unwrap_or("")
                .trim()
                .to_string();
        } else if trimmed.starts_with("Station latitude:") {
            if let Ok(v) = trimmed
                .strip_prefix("Station latitude:")
                .unwrap_or("")
                .trim()
                .parse::<f64>()
            {
                lat = v;
            }
        } else if trimmed.starts_with("Station longitude:") {
            if let Ok(v) = trimmed
                .strip_prefix("Station longitude:")
                .unwrap_or("")
                .trim()
                .parse::<f64>()
            {
                lon = v;
            }
        } else if trimmed.starts_with("Station elevation:") {
            // May have units like "357.00"
            let val_str = trimmed
                .strip_prefix("Station elevation:")
                .unwrap_or("")
                .trim();
            // Strip trailing non-numeric characters
            let numeric: String = val_str
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            if let Ok(v) = numeric.parse::<f64>() {
                elev = v;
            }
        }
    }

    // Try to look up name from station database
    if let Some(stn) = find_raob_station(&station_number) {
        if name.is_empty() {
            name = stn.name.to_string();
        }
        if lat.is_nan() {
            lat = stn.lat;
        }
        if lon.is_nan() {
            lon = stn.lon;
        }
        if elev.is_nan() {
            elev = stn.elevation_m;
        }
    } else if let Some(stn) = find_raob_station(fallback_id) {
        if name.is_empty() {
            name = stn.name.to_string();
        }
        if lat.is_nan() {
            lat = stn.lat;
        }
        if lon.is_nan() {
            lon = stn.lon;
        }
        if elev.is_nan() {
            elev = stn.elevation_m;
        }
    }

    // Default NaN values to 0
    if lat.is_nan() {
        lat = 0.0;
    }
    if lon.is_nan() {
        lon = 0.0;
    }
    if elev.is_nan() {
        elev = 0.0;
    }

    (station_number, obs_time, lat, lon, elev, name)
}

/// Parse the fixed-width data table from the sounding text.
///
/// Format:
/// ```text
/// PRES   HGHT   TEMP   DWPT   RELH   MIXR   DRCT   SKNT   THTA   THTE   THTV
///  hPa     m      C      C      %    g/kg    deg   knot     K      K      K
/// ```
///
/// Note: Wyoming's newer endpoint may return `SPED` (m/s) instead of `SKNT` (knots).
/// We detect this from the header and convert to knots if needed.
fn parse_data_table(text: &str) -> Result<Vec<SoundingLevel>, String> {
    let mut levels = Vec::new();
    let mut in_data = false;
    let mut dash_count = 0;
    let mut wind_speed_is_ms = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            // If we were in data, a blank line means end of data table
            if in_data {
                break;
            }
            continue;
        }

        // Count separator lines (-----)
        if trimmed.starts_with("---") {
            dash_count += 1;
            if dash_count >= 2 {
                in_data = true;
            }
            continue;
        }

        // Detect wind speed column header before we enter data
        if !in_data {
            // Check for SPED (m/s) vs SKNT (knots) in the header row
            if trimmed.contains("SPED") {
                wind_speed_is_ms = true;
            }
            continue;
        }

        // If we hit non-data text (station info section below the table), stop
        if trimmed.contains("Station information")
            || trimmed.contains("Station number")
            || trimmed.contains("Showalter")
            || trimmed.contains("CAPE")
        {
            break;
        }

        // Parse data line, converting wind speed from m/s to knots if needed
        if let Some(mut level) = parse_data_line(trimmed) {
            if wind_speed_is_ms {
                level.wind_speed *= 1.94384;
            }
            levels.push(level);
        }
    }

    Ok(levels)
}

/// Parse a single fixed-width data line.
///
/// Columns: PRES HGHT TEMP DWPT RELH MIXR DRCT SKNT THTA THTE THTV
/// We extract: PRES(0), HGHT(1), TEMP(2), DWPT(3), DRCT(6), SKNT(7)
fn parse_data_line(line: &str) -> Option<SoundingLevel> {
    let fields: Vec<&str> = line.split_whitespace().collect();

    // Need at least 8 fields for PRES HGHT TEMP DWPT RELH MIXR DRCT SKNT
    if fields.len() < 8 {
        return None;
    }

    let pres = parse_field(fields[0])?;
    let hght = parse_field(fields[1])?;
    let temp = parse_field(fields[2])?;
    let dwpt = parse_field(fields[3])?;
    let drct = parse_field(fields[6]).unwrap_or(0.0);
    let sknt = parse_field(fields[7]).unwrap_or(0.0);

    // Filter out missing/invalid data
    // Wyoming uses very large numbers or blanks for missing data
    if pres < 1.0 || pres > 1100.0 {
        return None;
    }
    if hght < -500.0 || hght > 50000.0 {
        return None;
    }
    if temp < -120.0 || temp > 60.0 {
        return None;
    }
    if dwpt < -120.0 || dwpt > 60.0 {
        return None;
    }

    // Ensure dewpoint does not exceed temperature
    let dewpoint = if dwpt > temp { temp } else { dwpt };

    Some(SoundingLevel {
        pressure: pres,
        height: hght,
        temperature: temp,
        dewpoint,
        wind_dir: drct,
        wind_speed: sknt,
    })
}

/// Parse a numeric field, returning None for missing/invalid values.
fn parse_field(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "////" || trimmed == "****" || trimmed == "99999" {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_data_line() {
        let line = "  965.0    357   22.4    8.4     42   7.16    180     10  297.5  319.1  298.7";
        let level = parse_data_line(line).expect("should parse");
        assert!((level.pressure - 965.0).abs() < 0.01);
        assert!((level.height - 357.0).abs() < 0.01);
        assert!((level.temperature - 22.4).abs() < 0.01);
        assert!((level.dewpoint - 8.4).abs() < 0.01);
        assert!((level.wind_dir - 180.0).abs() < 0.01);
        assert!((level.wind_speed - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_data_table() {
        let text = r#"
Some header text

-----------------------------------------------------------------------------
   PRES   HGHT   TEMP   DWPT   RELH   MIXR   DRCT   SKNT   THTA   THTE   THTV
    hPa     m      C      C      %    g/kg    deg   knot     K      K      K
-----------------------------------------------------------------------------
  965.0    357   22.4    8.4     42   7.16    180     10  297.5  319.1  298.7
  957.4    427   21.8    8.3     42   7.17    185     14  297.5  319.2  298.8
  925.0    727   19.4    7.4     43   6.98    200     18  297.5  318.7  298.7

Station information and calculation results
"#;
        let levels = parse_data_table(text).unwrap();
        assert_eq!(levels.len(), 3);
        assert!((levels[0].pressure - 965.0).abs() < 0.01);
        assert!((levels[2].pressure - 925.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_pre_block() {
        let html = "<html><body><pre>Hello\nWorld</pre></body></html>";
        let content = extract_pre_block(html).unwrap();
        assert_eq!(content, "Hello\nWorld");
    }

    #[test]
    fn test_parse_field_missing() {
        assert!(parse_field("////").is_none());
        assert!(parse_field("****").is_none());
        assert!(parse_field("99999").is_none());
        assert!(parse_field("").is_none());
        assert!((parse_field("42.5").unwrap() - 42.5).abs() < 0.01);
    }
}
