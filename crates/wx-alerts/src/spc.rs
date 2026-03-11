//! SPC Products: Convective Outlooks, Mesoscale Discussions, Watches

use serde::{Deserialize, Serialize};
use serde_json::Value;

const USER_AGENT: &str = "(wx-alerts, contact@example.com)";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvectiveOutlook {
    pub day: u8,
    pub valid_time: String,
    pub categories: Vec<OutlookCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlookCategory {
    pub label: String,
    pub risk_level: u8,
    pub polygons: Vec<Vec<(f64, f64)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MesoscaleDiscussion {
    pub number: u32,
    pub issued: String,
    pub expires: String,
    pub concerning: String,
    pub summary: String,
    pub polygon: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watch {
    pub number: u32,
    pub watch_type: WatchType,
    pub issued: String,
    pub expires: String,
    pub text: String,
    pub states: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WatchType {
    Tornado,
    SevereThunderstorm,
}

fn risk_label_to_level(label: &str) -> u8 {
    match label.to_uppercase().as_str() {
        "TSTM" | "THUNDERSTORM" | "THUNDER" => 0,
        "MRGL" | "MARGINAL" => 1,
        "SLGT" | "SLIGHT" => 2,
        "ENH" | "ENHANCED" => 3,
        "MDT" | "MODERATE" => 4,
        "HIGH" => 5,
        _ => 0,
    }
}

fn extract_polygons_from_geometry(geometry: &Value) -> Vec<Vec<(f64, f64)>> {
    let mut polygons = Vec::new();

    let geo_type = geometry.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match geo_type {
        "Polygon" => {
            if let Some(coords) = geometry.get("coordinates").and_then(|v| v.as_array()) {
                for ring in coords {
                    if let Some(points) = ring.as_array() {
                        let poly: Vec<(f64, f64)> = points.iter()
                            .filter_map(|p| {
                                let arr = p.as_array()?;
                                let lon = arr.first()?.as_f64()?;
                                let lat = arr.get(1)?.as_f64()?;
                                Some((lat, lon))
                            })
                            .collect();
                        if !poly.is_empty() {
                            polygons.push(poly);
                        }
                    }
                }
            }
        }
        "MultiPolygon" => {
            if let Some(multi) = geometry.get("coordinates").and_then(|v| v.as_array()) {
                for polygon in multi {
                    if let Some(rings) = polygon.as_array() {
                        for ring in rings {
                            if let Some(points) = ring.as_array() {
                                let poly: Vec<(f64, f64)> = points.iter()
                                    .filter_map(|p| {
                                        let arr = p.as_array()?;
                                        let lon = arr.first()?.as_f64()?;
                                        let lat = arr.get(1)?.as_f64()?;
                                        Some((lat, lon))
                                    })
                                    .collect();
                                if !poly.is_empty() {
                                    polygons.push(poly);
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    polygons
}

fn fetch_outlook(url: &str, day: u8) -> Result<ConvectiveOutlook, String> {
    let body: String = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let json: Value = serde_json::from_str(&body)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let features = json.get("features")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "No features array in outlook response".to_string())?;

    // Group by category label
    let mut category_map: std::collections::HashMap<String, Vec<Vec<(f64, f64)>>> =
        std::collections::HashMap::new();

    for feature in features {
        let props = match feature.get("properties") {
            Some(p) => p,
            None => continue,
        };

        // The label field varies — try LABEL, LABEL2, dn, cat
        let label = props.get("LABEL")
            .or_else(|| props.get("LABEL2"))
            .or_else(|| props.get("dn"))
            .or_else(|| props.get("cat"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if label.is_empty() {
            continue;
        }

        if let Some(geometry) = feature.get("geometry") {
            let polys = extract_polygons_from_geometry(geometry);
            category_map.entry(label).or_default().extend(polys);
        }
    }

    let mut categories: Vec<OutlookCategory> = category_map.into_iter()
        .map(|(label, polygons)| {
            let risk_level = risk_label_to_level(&label);
            OutlookCategory {
                label,
                risk_level,
                polygons,
            }
        })
        .collect();

    // Sort by risk level ascending
    categories.sort_by_key(|c| c.risk_level);

    // Try to extract valid time from the response
    let valid_time = json.get("properties")
        .and_then(|p| p.get("valid"))
        .or_else(|| json.get("valid_time"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(ConvectiveOutlook {
        day,
        valid_time,
        categories,
    })
}

/// Fetch current Day 1 convective outlook
pub fn fetch_day1_outlook() -> Result<ConvectiveOutlook, String> {
    fetch_outlook(
        "https://www.spc.noaa.gov/products/outlook/day1otlk_cat.lyr.geojson",
        1,
    )
}

/// Fetch current Day 2 convective outlook
pub fn fetch_day2_outlook() -> Result<ConvectiveOutlook, String> {
    fetch_outlook(
        "https://www.spc.noaa.gov/products/outlook/day2otlk_cat.lyr.geojson",
        2,
    )
}

/// Fetch active mesoscale discussions
///
/// Parses the SPC MD page to find active MDs, then fetches each one.
pub fn fetch_mesoscale_discussions() -> Result<Vec<MesoscaleDiscussion>, String> {
    let url = "https://www.spc.noaa.gov/products/md/";

    let body: String = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse MD numbers from the HTML page
    // Links look like: href="md1234.html"
    let mut md_numbers: Vec<u32> = Vec::new();
    for segment in body.split("md") {
        if let Some(dot_pos) = segment.find(".html") {
            let num_str = &segment[..dot_pos];
            if let Ok(num) = num_str.parse::<u32>() {
                if !md_numbers.contains(&num) {
                    md_numbers.push(num);
                }
            }
        }
    }

    let mut mds = Vec::new();
    for num in md_numbers {
        // Try to fetch the MD text
        let md_url = format!("https://www.spc.noaa.gov/products/md/md{:04}.html", num);
        let md_body = match ureq::get(&md_url)
            .header("User-Agent", USER_AGENT)
            .call()
        {
            Ok(mut resp) => match resp.body_mut().read_to_string() {
                Ok(s) => s,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let md = parse_md_html(&md_body, num);
        mds.push(md);
    }

    Ok(mds)
}

fn parse_md_html(html: &str, number: u32) -> MesoscaleDiscussion {
    // Extract polygon from "LAT...LON" lines
    let mut polygon = Vec::new();
    let mut in_latlon = false;

    for line in html.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("LAT...LON") {
            in_latlon = true;
            // Parse coords on this line after "LAT...LON"
            let after = trimmed.trim_start_matches("LAT...LON").trim();
            parse_latlon_pairs(after, &mut polygon);
            continue;
        }
        if in_latlon {
            // Continue reading coord pairs until we hit an empty line or non-numeric
            let stripped = trimmed.replace("&nbsp;", " ");
            let stripped = stripped.trim();
            if stripped.is_empty() || (!stripped.chars().next().unwrap_or(' ').is_ascii_digit() && stripped != "/") {
                in_latlon = false;
                continue;
            }
            parse_latlon_pairs(stripped, &mut polygon);
        }
    }

    // Extract concerning/summary from text
    let concerning = extract_between(html, "CONCERNING...", "\n")
        .unwrap_or_default();
    let summary = extract_between(html, "SUMMARY...", "\n\n")
        .or_else(|| extract_between(html, "DISCUSSION...", "\n\n"))
        .unwrap_or_default();

    // Extract times
    let issued = extract_between(html, "ISSUED:", "\n").unwrap_or_default();
    let expires = extract_between(html, "VALID UNTIL", "\n")
        .or_else(|| extract_between(html, "EXPIRES:", "\n"))
        .unwrap_or_default();

    MesoscaleDiscussion {
        number,
        issued: issued.trim().to_string(),
        expires: expires.trim().to_string(),
        concerning: concerning.trim().to_string(),
        summary: summary.trim().to_string(),
        polygon,
    }
}

fn parse_latlon_pairs(s: &str, out: &mut Vec<(f64, f64)>) {
    // Pairs are 4-digit numbers: LLLL OOOO where lat=LL.LL lon=-OOO.O or similar
    // SPC format: lat is ddmm (hundredths of degrees), lon is dddmm
    // Actually: values like "3892 10142" => lat 38.92, lon -101.42
    let nums: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    while i + 1 < nums.len() {
        if let (Ok(lat_raw), Ok(lon_raw)) = (nums[i].parse::<f64>(), nums[i + 1].parse::<f64>()) {
            let lat = lat_raw / 100.0;
            let lon = -(lon_raw / 100.0);
            out.push((lat, lon));
            i += 2;
        } else {
            i += 1;
        }
    }
}

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let start_idx = text.find(start)? + start.len();
    let remaining = &text[start_idx..];
    let end_idx = remaining.find(end).unwrap_or(remaining.len());
    Some(remaining[..end_idx].to_string())
}

/// Fetch active watches from SPC
pub fn fetch_active_watches() -> Result<Vec<Watch>, String> {
    // Try the JSON endpoint first
    let url = "https://www.spc.noaa.gov/products/watch/ActiveWW.json";

    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let body: String = resp.body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Try parsing as JSON
    if let Ok(json) = serde_json::from_str::<Value>(&body) {
        return parse_watches_json(&json);
    }

    // Fallback: parse the HTML watch page
    let html_url = "https://www.spc.noaa.gov/products/watch/";
    let html_body: String = ureq::get(html_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    parse_watches_html(&html_body)
}

fn parse_watches_json(json: &Value) -> Result<Vec<Watch>, String> {
    let mut watches = Vec::new();

    // The JSON may have a "features" array (GeoJSON) or be a direct array
    let items = json.get("features")
        .and_then(|v| v.as_array())
        .or_else(|| json.as_array());

    let items = match items {
        Some(arr) => arr,
        None => return Ok(watches),
    };

    for item in items {
        let props = item.get("properties").unwrap_or(item);

        let number = props.get("WATCHNUMBER")
            .or_else(|| props.get("number"))
            .or_else(|| props.get("ww"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0) as u32;

        let wtype_str = props.get("WATCHTYPE")
            .or_else(|| props.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let watch_type = if wtype_str.to_lowercase().contains("tornado") {
            WatchType::Tornado
        } else {
            WatchType::SevereThunderstorm
        };

        let issued = props.get("issued")
            .or_else(|| props.get("ISSUED"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let expires = props.get("expires")
            .or_else(|| props.get("EXPIRES"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let text = props.get("text")
            .or_else(|| props.get("TEXT"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let states = props.get("states")
            .or_else(|| props.get("STATES"))
            .and_then(|v| v.as_str())
            .map(|s| s.split_whitespace().map(|st| st.to_string()).collect())
            .unwrap_or_default();

        if number > 0 {
            watches.push(Watch {
                number,
                watch_type,
                issued,
                expires,
                text,
                states,
            });
        }
    }

    Ok(watches)
}

fn parse_watches_html(html: &str) -> Result<Vec<Watch>, String> {
    let mut watches = Vec::new();

    // Look for watch links like "ww0123.html"
    for segment in html.split("ww") {
        if let Some(dot_pos) = segment.find(".html") {
            let num_str = &segment[..dot_pos];
            if let Ok(num) = num_str.parse::<u32>() {
                // Determine type from surrounding text
                // Look backwards in the original for "Tornado" or "Severe"
                let watch_type = if html.contains(&format!("Tornado Watch {}", num))
                    || html.contains(&format!("TORNADO WATCH {}", num))
                {
                    WatchType::Tornado
                } else {
                    WatchType::SevereThunderstorm
                };

                // Avoid duplicates
                if !watches.iter().any(|w: &Watch| w.number == num) {
                    watches.push(Watch {
                        number: num,
                        watch_type,
                        issued: String::new(),
                        expires: String::new(),
                        text: String::new(),
                        states: Vec::new(),
                    });
                }
            }
        }
    }

    Ok(watches)
}

/// Check if a point (lat, lon) is inside any outlook risk area.
/// Returns the highest risk category containing the point.
pub fn point_risk_level<'a>(lat: f64, lon: f64, outlook: &'a ConvectiveOutlook) -> Option<&'a OutlookCategory> {
    let mut best: Option<&OutlookCategory> = None;

    for cat in &outlook.categories {
        for polygon in &cat.polygons {
            if point_in_polygon(lat, lon, polygon) {
                match best {
                    Some(b) if b.risk_level >= cat.risk_level => {}
                    _ => best = Some(cat),
                }
            }
        }
    }

    best
}

/// Ray casting algorithm for point-in-polygon test
fn point_in_polygon(lat: f64, lon: f64, polygon: &[(f64, f64)]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }

    let mut inside = false;
    let mut j = n - 1;

    for i in 0..n {
        let (yi, xi) = polygon[i];
        let (yj, xj) = polygon[j];

        if ((yi > lat) != (yj > lat))
            && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi)
        {
            inside = !inside;
        }
        j = i;
    }

    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_in_polygon() {
        // Simple square polygon
        let poly = vec![
            (0.0, 0.0),
            (0.0, 10.0),
            (10.0, 10.0),
            (10.0, 0.0),
            (0.0, 0.0),
        ];

        assert!(point_in_polygon(5.0, 5.0, &poly));
        assert!(!point_in_polygon(15.0, 5.0, &poly));
        assert!(!point_in_polygon(5.0, 15.0, &poly));
    }

    #[test]
    fn test_risk_label_to_level() {
        assert_eq!(risk_label_to_level("TSTM"), 0);
        assert_eq!(risk_label_to_level("MRGL"), 1);
        assert_eq!(risk_label_to_level("SLGT"), 2);
        assert_eq!(risk_label_to_level("ENH"), 3);
        assert_eq!(risk_label_to_level("MDT"), 4);
        assert_eq!(risk_label_to_level("HIGH"), 5);
    }
}
