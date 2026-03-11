use crate::output::{print_json, print_error};
use serde_json::json;
use std::io::Read;

use chrono::{Datelike, Timelike};
use rustmet_core::models::mrms::MrmsConfig;

const USER_AGENT: &str = "wx-pro/0.1 (Fahrenheit Research)";

/// Download MRMS composite reflectivity, parse GRIB2, optionally extract point value.
pub fn run(product: &str, datetime: Option<&str>, lat: Option<f64>, lon: Option<f64>, pretty: bool) {
    let mrms_product = match product {
        "composite_refl" => MrmsConfig::product_composite_refl(),
        "precip_rate" => MrmsConfig::product_precip_rate(),
        "precip_flag" => MrmsConfig::product_precip_flag(),
        "qpe_01h" => MrmsConfig::product_qpe_01h(),
        "qpe_03h" => MrmsConfig::product_qpe_03h(),
        "qpe_06h" => MrmsConfig::product_qpe_06h(),
        "qpe_12h" => MrmsConfig::product_qpe_12h(),
        "qpe_24h" => MrmsConfig::product_qpe_24h(),
        other => print_error(&format!(
            "Unknown MRMS product '{}'. Options: composite_refl, precip_rate, precip_flag, qpe_01h-24h",
            other
        )),
    };

    let dt_string = match datetime {
        Some(dt) => dt.to_string(),
        None => {
            let now = chrono::Utc::now();
            let rounded = (now.minute() / 2) * 2;
            format!(
                "{:04}{:02}{:02}-{:02}{:02}00",
                now.year(), now.month(), now.day(),
                now.hour(), rounded,
            )
        }
    };

    let level = if mrms_product == MrmsConfig::product_composite_refl() {
        "00.50"
    } else {
        "00.00"
    };

    let url = MrmsConfig::aws_url(mrms_product, level, &dt_string);

    let download_start = std::time::Instant::now();
    let raw_data = http_get_bytes(&url);
    let download_ms = download_start.elapsed().as_millis();
    let compressed_size = raw_data.len();

    let data = decompress_gz(raw_data);

    let grib = match rustmet_core::grib2::Grib2File::from_bytes(&data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("Failed to parse GRIB2: {}", e)),
    };

    let num_messages = grib.messages.len();

    let point_value = if let (Some(la), Some(lo)) = (lat, lon) {
        if la < MrmsConfig::lat_min() || la > MrmsConfig::lat_max()
            || lo < MrmsConfig::lon_min() || lo > MrmsConfig::lon_max()
        {
            print_error(&format!(
                "Point ({}, {}) is outside MRMS domain (lat {}-{}, lon {}-{})",
                la, lo,
                MrmsConfig::lat_min(), MrmsConfig::lat_max(),
                MrmsConfig::lon_min(), MrmsConfig::lon_max(),
            ));
        }

        let ix = ((lo - MrmsConfig::lon_min()) / MrmsConfig::grid_dx()).round() as usize;
        let iy = ((la - MrmsConfig::lat_min()) / MrmsConfig::grid_dy()).round() as usize;
        let nx = MrmsConfig::grid_nx() as usize;

        if let Some(msg) = grib.messages.first() {
            match rustmet_core::grib2::unpack_message(msg) {
                Ok(values) => {
                    let idx = iy * nx + ix;
                    if idx < values.len() {
                        let val = values[idx];
                        if val > -900.0 {
                            Some(json!({
                                "lat": la,
                                "lon": lo,
                                "grid_i": ix,
                                "grid_j": iy,
                                "value": (val * 10.0).round() / 10.0,
                            }))
                        } else {
                            Some(json!({
                                "lat": la,
                                "lon": lo,
                                "grid_i": ix,
                                "grid_j": iy,
                                "value": null,
                                "note": "missing/no-data at this grid point",
                            }))
                        }
                    } else {
                        Some(json!({
                            "lat": la,
                            "lon": lo,
                            "error": format!("grid index {} out of range ({})", idx, values.len()),
                        }))
                    }
                }
                Err(e) => Some(json!({
                    "error": format!("Failed to unpack GRIB2 message: {}", e),
                })),
            }
        } else {
            Some(json!({"error": "No messages in GRIB2 file"}))
        }
    } else {
        None
    };

    print_json(&json!({
        "product": product,
        "mrms_product": mrms_product,
        "datetime": dt_string,
        "url": url,
        "grid_size": format!("{}x{}", MrmsConfig::grid_nx(), MrmsConfig::grid_ny()),
        "messages": num_messages,
        "value_at_point": point_value,
        "performance": {
            "download_ms": download_ms,
            "compressed_bytes": compressed_size,
            "decompressed_bytes": data.len(),
        },
    }), pretty);
}

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

fn decompress_gz(data: Vec<u8>) -> Vec<u8> {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        let mut decoder = flate2::read::GzDecoder::new(&data[..]);
        let mut decompressed = Vec::new();
        match decoder.read_to_end(&mut decompressed) {
            Ok(_) => decompressed,
            Err(e) => print_error(&format!("Failed to decompress gzip: {}", e)),
        }
    } else {
        data
    }
}
