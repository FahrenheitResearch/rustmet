// cmd_radar_lite.rs — MRMS composite reflectivity (bandwidth-optimized radar)
//
// Expected bandwidth: ~1-2MB (MRMS grib2.gz vs ~15MB for Level 2)
//
// Downloads MRMS MergedReflectivityQCComposite at 00.50 level.
// Covers ALL of CONUS simultaneously (not just one radar site).
// Auto-computes datetime from current UTC, rounds to nearest 2 minutes.

use crate::output::{print_json, print_error};
use serde_json::json;
use std::io::Read;

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

/// Round minutes down to nearest 2-minute interval (MRMS updates every 2 min).
fn round_to_2min(minute: u32) -> u32 {
    (minute / 2) * 2
}

pub fn run(lat: Option<f64>, lon: Option<f64>, radius_km: f64, pretty: bool) {
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

    // Compute datetime string: YYYYMMDD-HHmmss, rounded to nearest 2 minutes
    let now = chrono::Utc::now();
    let rounded_min = round_to_2min(now.format("%M").to_string().parse::<u32>().unwrap_or(0));
    let datetime = format!(
        "{}-{}{:02}{:02}",
        now.format("%Y%m%d"),
        now.format("%H"),
        rounded_min,
        0 // seconds always 00
    );

    // Try current time first, then fall back to 2 and 4 minutes earlier
    let mut downloaded = false;
    let mut grib_data: Vec<u8> = Vec::new();
    let mut actual_datetime = datetime.clone();
    let mut file_size: usize = 0;
    let download_start = std::time::Instant::now();

    for offset in &[0u32, 2, 4, 6, 8, 10] {
        let total_min = now.format("%H").to_string().parse::<u32>().unwrap_or(0) * 60
            + rounded_min;
        let adjusted_min = if total_min >= *offset {
            total_min - offset
        } else {
            // Wrap to previous day would be complex; just try what we have
            continue;
        };
        let adj_hour = adjusted_min / 60;
        let adj_min = (adjusted_min % 60 / 2) * 2;
        let try_datetime = format!(
            "{}-{:02}{:02}{:02}",
            now.format("%Y%m%d"),
            adj_hour,
            adj_min,
            0
        );

        let url = rustmet_core::models::mrms::MrmsConfig::composite_reflectivity_url(&try_datetime);

        match ureq::get(&url)
            .header("User-Agent", "wx-lite/0.1 (Fahrenheit Research)")
            .call()
        {
            Ok(resp) => {
                if let Ok(compressed) = resp.into_body().read_to_vec() {
                    file_size = compressed.len();
                    // Decompress gzip
                    let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
                    let mut decompressed = Vec::new();
                    if decoder.read_to_end(&mut decompressed).is_ok() {
                        grib_data = decompressed;
                        actual_datetime = try_datetime;
                        downloaded = true;
                        break;
                    }
                }
            }
            Err(_) => continue,
        }
    }

    let download_time_ms = download_start.elapsed().as_millis();

    if !downloaded {
        print_error(&format!(
            "Failed to download MRMS composite reflectivity. Tried datetimes near {}. \
             MRMS may be temporarily unavailable.",
            datetime
        ));
    }

    // Parse GRIB2
    let grib = match rustmet_core::grib2::parser::Grib2File::from_bytes(&grib_data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("Failed to parse MRMS GRIB2: {}", e)),
    };

    if grib.messages.is_empty() {
        print_error("MRMS GRIB2 file contains no messages");
    }

    let msg = &grib.messages[0];

    // Unpack data values
    let values = match rustmet_core::grib2::unpack::unpack_message(msg) {
        Ok(v) => v,
        Err(e) => print_error(&format!("Failed to unpack MRMS data: {}", e)),
    };

    let grid = &msg.grid;
    let nx = grid.nx as usize;
    let ny = grid.ny as usize;

    // If lat/lon provided: extract point value and nearby max
    if let (Some(pt_lat), Some(pt_lon)) = (lat, lon) {
        // Find nearest grid point
        let (i_pt, j_pt) = latlon_to_grid_ij(pt_lat, pt_lon, grid);
        let pt_idx = j_pt * nx + i_pt;
        let value_at_point = if pt_idx < values.len() {
            let v = values[pt_idx];
            if v < -90.0 { None } else { Some(v) } // MRMS uses -999 for missing
        } else {
            None
        };

        // Find max within radius
        let mut max_val = f64::NEG_INFINITY;
        let mut max_lat = 0.0f64;
        let mut max_lon = 0.0f64;
        let mut points_checked = 0u32;

        // Compute grid search bounds to avoid scanning entire 7000x3500 grid
        let deg_radius = radius_km / 111.0; // rough conversion
        let lat_min = pt_lat - deg_radius;
        let lat_max = pt_lat + deg_radius;
        let lon_min = pt_lon - deg_radius;
        let lon_max = pt_lon + deg_radius;

        let j_start = ((lat_min - grid.lat1) / grid.dy).max(0.0) as usize;
        let j_end = (((lat_max - grid.lat1) / grid.dy) as usize + 1).min(ny);
        let i_start = ((lon_min - grid.lon1) / grid.dx).max(0.0) as usize;
        let i_end = (((lon_max - grid.lon1) / grid.dx) as usize + 1).min(nx);

        for j in j_start..j_end {
            let row_lat = grid.lat1 + j as f64 * grid.dy;
            for i in i_start..i_end {
                let col_lon = grid.lon1 + i as f64 * grid.dx;
                let dist = haversine_km(pt_lat, pt_lon, row_lat, col_lon);
                if dist <= radius_km {
                    let idx = j * nx + i;
                    if idx < values.len() {
                        let v = values[idx];
                        if v > -90.0 && v > max_val {
                            max_val = v;
                            max_lat = row_lat;
                            max_lon = col_lon;
                            points_checked += 1;
                        }
                    }
                }
            }
        }

        let max_within_radius = if max_val > f64::NEG_INFINITY {
            Some(json!({
                "dbz": (max_val * 10.0).round() / 10.0,
                "lat": (max_lat * 1000.0).round() / 1000.0,
                "lon": (max_lon * 1000.0).round() / 1000.0,
            }))
        } else {
            None
        };

        print_json(&json!({
            "product": "MRMS MergedReflectivityQCComposite",
            "datetime": actual_datetime,
            "grid": { "nx": nx, "ny": ny },
            "point": {
                "lat": pt_lat,
                "lon": pt_lon,
                "grid_i": i_pt,
                "grid_j": j_pt,
                "value_dbz": value_at_point.map(|v| (v * 10.0).round() / 10.0),
            },
            "max_within_radius": max_within_radius,
            "radius_km": radius_km,
            "points_checked": points_checked,
            "file_size_bytes": file_size,
            "download_time_ms": download_time_ms,
        }), pretty);
    } else {
        // No point specified: find overall max reflectivity
        let mut max_val = f64::NEG_INFINITY;
        let mut max_idx = 0usize;

        for (idx, &v) in values.iter().enumerate() {
            if v > -90.0 && v > max_val {
                max_val = v;
                max_idx = idx;
            }
        }

        let max_j = max_idx / nx;
        let max_i = max_idx % nx;
        let max_lat = grid.lat1 + max_j as f64 * grid.dy;
        let max_lon = grid.lon1 + max_i as f64 * grid.dx;

        print_json(&json!({
            "product": "MRMS MergedReflectivityQCComposite",
            "datetime": actual_datetime,
            "grid": { "nx": nx, "ny": ny },
            "conus_max": {
                "dbz": (max_val * 10.0).round() / 10.0,
                "lat": (max_lat * 1000.0).round() / 1000.0,
                "lon": (max_lon * 1000.0).round() / 1000.0,
                "grid_i": max_i,
                "grid_j": max_j,
            },
            "file_size_bytes": file_size,
            "download_time_ms": download_time_ms,
            "bandwidth_note": "MRMS composite ~1-2MB vs Level 2 ~15MB",
        }), pretty);
    }
}

/// Convert lat/lon to nearest grid index (i, j) for the MRMS lat/lon grid.
fn latlon_to_grid_ij(lat: f64, lon: f64, grid: &rustmet_core::grib2::parser::GridDefinition) -> (usize, usize) {
    let nx = grid.nx as usize;
    let ny = grid.ny as usize;

    // MRMS is a regular lat/lon grid
    let j = ((lat - grid.lat1) / grid.dy).round().max(0.0).min((ny - 1) as f64) as usize;
    let i = ((lon - grid.lon1) / grid.dx).round().max(0.0).min((nx - 1) as f64) as usize;
    (i, j)
}
