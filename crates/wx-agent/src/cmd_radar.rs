use crate::output::{print_json, print_error};
use serde_json::json;
use std::io::Read;

const NEXRAD_BASE_URL: &str = "https://unidata-nexrad-level2.s3.amazonaws.com";
const USER_AGENT: &str = "wx-agent/0.1 (Fahrenheit Research)";

/// Download latest NEXRAD Level 2 volume scan, parse it, return JSON summary.
pub fn run(site: &str, lat: Option<f64>, lon: Option<f64>, pretty: bool) {
    // Resolve site: if lat/lon given, find nearest
    let site_id = if let (Some(la), Some(lo)) = (lat, lon) {
        find_nearest_site(la, lo)
    } else if !site.is_empty() {
        site.to_uppercase()
    } else {
        print_error("Provide --site KTLX or --lat/--lon");
    };

    // Look up site info
    let site_info = wx_radar::sites::find_site(&site_id);

    // Step 1: Find latest file on S3
    let now = chrono::Utc::now();
    let today = now.format("%Y/%m/%d").to_string();
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y/%m/%d").to_string();

    let latest_key = find_latest_file(&site_id, &today)
        .or_else(|| find_latest_file(&site_id, &yesterday));

    let key = match latest_key {
        Some(k) => k,
        None => print_error(&format!("No NEXRAD files found for {} in last 24h", site_id)),
    };

    let filename = key.rsplit('/').next().unwrap_or(&key).to_string();

    // Step 2: Download the file
    let download_start = std::time::Instant::now();
    let url = format!("{}/{}", NEXRAD_BASE_URL, key);
    let raw_data = http_get_bytes(&url);
    let download_ms = download_start.elapsed().as_millis();

    // Decompress if gzipped
    let data = maybe_decompress_gz(raw_data);

    // Step 3: Parse Level 2
    let parse_start = std::time::Instant::now();
    let l2 = match wx_radar::level2::Level2File::parse(&data) {
        Ok(f) => f,
        Err(e) => print_error(&format!("Failed to parse Level 2: {}", e)),
    };
    let parse_ms = parse_start.elapsed().as_millis();

    // Step 4: Extract summary data
    let num_sweeps = l2.sweeps.len();
    let total_radials: usize = l2.sweeps.iter().map(|s| s.radials.len()).sum();

    // Find max reflectivity across all sweeps
    let mut max_ref: f32 = f32::MIN;
    let mut max_ref_az: f32 = 0.0;
    let mut max_ref_range: f32 = 0.0;
    let mut max_ref_elev: f32 = 0.0;

    for sweep in &l2.sweeps {
        for radial in &sweep.radials {
            for moment in &radial.moments {
                if moment.product == wx_radar::products::RadarProduct::Reflectivity {
                    let gate_size_km = moment.gate_size as f32 / 1000.0;
                    let first_gate_km = moment.first_gate_range as f32 / 1000.0;
                    for (gi, &val) in moment.data.iter().enumerate() {
                        if !val.is_nan() && val > max_ref {
                            max_ref = val;
                            max_ref_az = radial.azimuth;
                            max_ref_range = first_gate_km + gi as f32 * gate_size_km;
                            max_ref_elev = sweep.elevation_angle;
                        }
                    }
                }
            }
        }
    }

    // Collect sweep elevation angles
    let elevations: Vec<f32> = l2.sweeps.iter().map(|s| s.elevation_angle).collect();

    // Find max velocity (for severe weather indication)
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

    // Compute max reflectivity lat/lon if we have site info
    let max_ref_location = site_info.as_ref().map(|si| {
        let az_rad = (max_ref_az as f64).to_radians();
        let lat = si.lat + (max_ref_range as f64 * az_rad.cos()) / 111.139;
        let lon = si.lon + (max_ref_range as f64 * az_rad.sin()) / (111.139 * si.lat.to_radians().cos());
        json!({"lat": (lat * 1000.0).round() / 1000.0, "lon": (lon * 1000.0).round() / 1000.0})
    }).unwrap_or(json!(null));

    // Products available
    let mut products_found = std::collections::HashSet::new();
    for sweep in &l2.sweeps {
        for radial in &sweep.radials {
            for moment in &radial.moments {
                products_found.insert(moment.product.short_name().to_string());
            }
        }
    }
    let mut products_list: Vec<String> = products_found.into_iter().collect();
    products_list.sort();

    let max_gate_to_gate = (max_outbound - max_inbound).abs();

    print_json(&json!({
        "site": site_id,
        "site_name": site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown"),
        "site_lat": site_info.as_ref().map(|s| s.lat),
        "site_lon": site_info.as_ref().map(|s| s.lon),
        "file": filename,
        "volume": {
            "sweeps": num_sweeps,
            "total_radials": total_radials,
            "elevations": elevations,
            "products": products_list,
        },
        "reflectivity": {
            "max_dbz": if max_ref > f32::MIN { Some((max_ref * 10.0).round() / 10.0) } else { None },
            "max_location": max_ref_location,
            "max_azimuth": (max_ref_az * 10.0).round() / 10.0,
            "max_range_km": (max_ref_range * 10.0).round() / 10.0,
            "max_elevation": (max_ref_elev * 10.0).round() / 10.0,
        },
        "velocity": {
            "max_inbound_ms": (max_inbound * 10.0).round() / 10.0,
            "max_outbound_ms": (max_outbound * 10.0).round() / 10.0,
            "max_gate_to_gate_ms": (max_gate_to_gate * 10.0).round() / 10.0,
        },
        "performance": {
            "download_ms": download_ms,
            "parse_ms": parse_ms,
            "file_size_bytes": data.len(),
        },
    }), pretty);
}

/// Find nearest NEXRAD site to given lat/lon
fn find_nearest_site(lat: f64, lon: f64) -> String {
    let mut best_id = "KTLX";
    let mut best_dist = f64::MAX;
    for &(id, _, slat, slon, _) in wx_radar::sites::SITES {
        let dlat = (slat - lat) * 111.139;
        let dlon = (slon - lon) * 111.139 * lat.to_radians().cos();
        let dist = (dlat * dlat + dlon * dlon).sqrt();
        if dist < best_dist {
            best_dist = dist;
            best_id = id;
        }
    }
    best_id.to_string()
}

/// List S3 bucket and find latest file for a site on a given date
fn find_latest_file(site: &str, date: &str) -> Option<String> {
    let prefix = format!("{}/{}/", date, site);
    let url = format!("{}?list-type=2&prefix={}", NEXRAD_BASE_URL, prefix);

    let response = ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .ok()?;

    let body = response.into_body().read_to_string().ok()?;

    // Parse S3 XML response
    let mut files: Vec<String> = Vec::new();
    for contents in body.split("<Contents>").skip(1) {
        let end = contents.find("</Contents>").unwrap_or(contents.len());
        let block = &contents[..end];
        if let Some(key) = extract_xml_tag(block, "Key") {
            let filename = key.rsplit('/').next().unwrap_or(&key);
            // Skip MDM metadata files
            if !filename.ends_with("_MDM") && !filename.ends_with(".md") && !key.is_empty() {
                files.push(key);
            }
        }
    }

    files.sort();
    files.last().cloned()
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml.find(&close)?;
    Some(xml[start..end].to_string())
}

/// Blocking HTTP GET returning raw bytes
fn http_get_bytes(url: &str) -> Vec<u8> {
    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call();

    match response {
        Ok(resp) => {
            let mut bytes = Vec::new();
            resp.into_body().as_reader().read_to_end(&mut bytes)
                .unwrap_or_else(|e| print_error(&format!("Failed to read response: {}", e)));
            bytes
        }
        Err(e) => print_error(&format!("Failed to download {}: {}", url, e)),
    }
}

/// Decompress gzip if the data starts with gzip magic bytes
fn maybe_decompress_gz(data: Vec<u8>) -> Vec<u8> {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        let mut decoder = flate2::read::GzDecoder::new(&data[..]);
        let mut decompressed = Vec::new();
        match decoder.read_to_end(&mut decompressed) {
            Ok(_) => decompressed,
            Err(_) => data, // Not actually gzip, use raw
        }
    } else {
        data
    }
}
