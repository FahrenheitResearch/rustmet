use crate::output::{print_json, print_error};
use serde_json::json;
use std::time::Instant;
use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2;
use wx_field::projection::{LambertProjection, LatLonProjection, Projection};

struct ScanHit {
    lat: f64,
    lon: f64,
    value: f64,
    grid_i: usize,
    grid_j: usize,
}

pub fn run(
    source: &str,
    var: &str,
    level: &str,
    fhour: u32,
    mode: &str,
    top_n: usize,
    threshold: Option<f64>,
    separation_km: f64,
    lat1: Option<f64>,
    lon1: Option<f64>,
    lat2: Option<f64>,
    lon2: Option<f64>,
    pretty: bool,
) {
    // Validate mode
    if !["max", "min", "threshold"].contains(&mode) {
        print_error(&format!("Invalid mode '{}'. Use: max, min, threshold", mode));
    }
    if mode == "threshold" && threshold.is_none() {
        print_error("threshold mode requires --threshold value");
    }

    // Validate bbox: all or none
    let bbox_params = [lat1, lon1, lat2, lon2];
    let bbox_count = bbox_params.iter().filter(|p| p.is_some()).count();
    if bbox_count > 0 && bbox_count < 4 {
        print_error("Bounding box requires all 4 parameters: lat1, lon1, lat2, lon2");
    }

    let source_lower = source.to_lowercase();

    // MRMS not yet supported
    if source_lower == "mrms" {
        print_error("MRMS scan coming soon, use hrrr/gfs/rap/nam");
    }

    if !["hrrr", "rap", "gfs", "nam"].contains(&source_lower.as_str()) {
        print_error(&format!(
            "Source '{}' not supported. Use: hrrr, rap, gfs, nam",
            source
        ));
    }

    let pattern = build_pattern(var, level);

    let product_type = if level.contains("mb") || level.contains("hPa") { "prs" } else { "sfc" };

    let client = match DownloadClient::new() {
        Ok(c) => c,
        Err(e) => print_error(&format!("Failed to create HTTP client: {}", e)),
    };

    eprintln!("Finding latest {} run...", source_lower);
    let (date, hour) = match rustmet_core::models::find_latest_run(&client, &source_lower) {
        Ok(r) => r,
        Err(e) => print_error(&format!("No model run found: {}", e)),
    };

    let run_label = format!("{}/{:02}z", date, hour);
    eprintln!("Downloading {} {} {} f{:02} [{}]...", source_lower, run_label, product_type, fhour, pattern);

    let patterns: Vec<&str> = vec![pattern.as_str()];
    let download_start = Instant::now();
    let result = match fetch_with_fallback(
        &client,
        &source_lower,
        &date,
        hour,
        product_type,
        fhour,
        Some(&patterns),
        None,
    ) {
        Ok(r) => r,
        Err(e) => print_error(&format!("Download failed: {}", e)),
    };
    let download_ms = download_start.elapsed().as_millis();

    eprintln!("Downloaded {} bytes from {} in {}ms", result.data.len(), result.source_name, download_ms);

    let parse_start = Instant::now();
    let grib = match grib2::Grib2File::from_bytes(&result.data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("GRIB2 parse failed: {}", e)),
    };

    if grib.messages.is_empty() {
        print_error("No matching GRIB2 messages found");
    }

    let msg = &grib.messages[0];
    let values = match grib2::unpack_message(msg) {
        Ok(v) => v,
        Err(e) => print_error(&format!("Failed to unpack data: {}", e)),
    };

    let nx = msg.grid.nx as usize;
    let ny = msg.grid.ny as usize;
    let parse_ms = parse_start.elapsed().as_millis();

    if values.len() != nx * ny {
        print_error(&format!(
            "Grid size mismatch: {} values, expected {}x{}={}",
            values.len(), nx, ny, nx * ny
        ));
    }

    let param_name = grib2::tables::parameter_name(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );
    let param_units = grib2::tables::parameter_units(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );

    let proj = match build_projection(&msg.grid) {
        Some(p) => p,
        None => print_error(&format!("Unsupported grid template {}", msg.grid.template)),
    };

    // Compute grid index bounds for bbox filtering
    let grid_bounds: Option<(usize, usize, usize, usize)> = if bbox_count == 4 {
        let b_lat1 = lat1.unwrap();
        let b_lon1 = lon1.unwrap();
        let b_lat2 = lat2.unwrap();
        let b_lon2 = lon2.unwrap();
        let (gi1, gj1) = proj.latlon_to_grid(b_lat1, b_lon1);
        let (gi2, gj2) = proj.latlon_to_grid(b_lat2, b_lon2);
        let i_min = (gi1.min(gi2).floor().max(0.0)) as usize;
        let i_max = (gi1.max(gi2).ceil().min(nx as f64 - 1.0)) as usize;
        let j_min = (gj1.min(gj2).floor().max(0.0)) as usize;
        let j_max = (gj1.max(gj2).ceil().min(ny as f64 - 1.0)) as usize;
        Some((i_min, i_max, j_min, j_max))
    } else {
        None
    };

    // Scan
    let scan_start = Instant::now();

    let mut domain_min = f64::INFINITY;
    let mut domain_max = f64::NEG_INFINITY;
    let mut domain_sum = 0.0;
    let mut valid_count = 0usize;
    let mut points_above_threshold = 0usize;

    let mut candidates: Vec<(usize, f64)> = Vec::new();

    for (idx, &val) in values.iter().enumerate() {
        if !val.is_finite() || val.abs() > 1e15 || val < -900.0 {
            continue;
        }

        let i = idx % nx;
        let j = idx / nx;

        if let Some((i_min, i_max, j_min, j_max)) = grid_bounds {
            if i < i_min || i > i_max || j < j_min || j > j_max {
                continue;
            }
        }

        if val < domain_min { domain_min = val; }
        if val > domain_max { domain_max = val; }
        domain_sum += val;
        valid_count += 1;

        if let Some(thresh) = threshold {
            if val >= thresh {
                points_above_threshold += 1;
            }
        }

        match mode {
            "max" | "min" => candidates.push((idx, val)),
            "threshold" => {
                if let Some(thresh) = threshold {
                    if val >= thresh {
                        candidates.push((idx, val));
                    }
                }
            }
            _ => {}
        }
    }

    match mode {
        "max" | "threshold" => candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)),
        "min" => candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
        _ => {}
    }

    let domain_mean = if valid_count > 0 { domain_sum / valid_count as f64 } else { 0.0 };

    // Cluster with separation enforcement
    let mut results: Vec<ScanHit> = Vec::new();
    let limit = if mode == "threshold" { 500 } else { top_n };

    for (idx, val) in &candidates {
        let i = idx % nx;
        let j = idx / nx;
        let (lat, lon) = proj.grid_to_latlon(i as f64, j as f64);
        let lon = if lon > 180.0 { lon - 360.0 } else { lon };

        let too_close = results.iter().any(|r| haversine_km(lat, lon, r.lat, r.lon) < separation_km);
        if !too_close {
            results.push(ScanHit { lat, lon, value: *val, grid_i: i, grid_j: j });
            if results.len() >= limit { break; }
        }
    }

    let scan_ms = scan_start.elapsed().as_millis();
    let total_ms = download_ms + parse_ms + scan_ms;

    let grid_template_name = match msg.grid.template {
        0 => "latitude_longitude",
        30 => "lambert_conformal",
        _ => "unknown",
    };

    let results_json: Vec<serde_json::Value> = results.iter().enumerate().map(|(rank, hit)| {
        json!({
            "rank": rank + 1,
            "lat": (hit.lat * 1000.0).round() / 1000.0,
            "lon": (hit.lon * 1000.0).round() / 1000.0,
            "value": (hit.value * 10.0).round() / 10.0,
            "grid_i": hit.grid_i,
            "grid_j": hit.grid_j,
        })
    }).collect();

    let bbox_json = if bbox_count == 4 {
        json!({
            "lat1": lat1.unwrap(),
            "lon1": lon1.unwrap(),
            "lat2": lat2.unwrap(),
            "lon2": lon2.unwrap(),
        })
    } else {
        json!(null)
    };

    let mut stats = json!({
        "domain_min": (domain_min * 10.0).round() / 10.0,
        "domain_max": (domain_max * 10.0).round() / 10.0,
        "domain_mean": (domain_mean * 10.0).round() / 10.0,
        "points_scanned": valid_count,
    });
    if mode == "threshold" {
        stats["points_above_threshold"] = json!(points_above_threshold);
    }

    print_json(&json!({
        "source": source_lower.to_uppercase(),
        "run": run_label,
        "forecast_hour": fhour,
        "variable": param_name,
        "variable_short": var,
        "level": level,
        "units": param_units,
        "scan_mode": mode,
        "top_n": top_n,
        "threshold": threshold,
        "separation_km": separation_km,
        "bbox": bbox_json,
        "grid": {
            "nx": nx,
            "ny": ny,
            "template": grid_template_name,
        },
        "results": results_json,
        "statistics": stats,
        "performance": {
            "download_ms": download_ms,
            "parse_ms": parse_ms,
            "scan_ms": scan_ms,
            "total_ms": total_ms,
        },
    }), pretty);
}

fn build_pattern(var: &str, level: &str) -> String {
    let grib_var = match var.to_lowercase().as_str() {
        "temperature" | "temp" | "t" => "TMP",
        "dewpoint" | "td" | "dew" => "DPT",
        "wind_u" | "u" | "ugrd" => "UGRD",
        "wind_v" | "v" | "vgrd" => "VGRD",
        "gust" => "GUST",
        "pressure" | "pres" => "PRES",
        "mslp" => "MSLMA",
        "cape" => "CAPE",
        "cin" => "CIN",
        "reflectivity" | "refl" | "refc" => return "REFC:entire atmosphere".to_string(),
        "visibility" | "vis" => "VIS",
        "rh" | "relative_humidity" => "RH",
        "height" | "hgt" => "HGT",
        "precip" | "precipitation" | "apcp" => "APCP",
        "helicity" | "hlcy" | "srh" => {
            let lvl = level.to_lowercase();
            if lvl.contains("1") { return "HLCY:1000-0 m above ground".to_string(); }
            return "HLCY:3000-0 m above ground".to_string();
        }
        "pwat" | "precipitable_water" => return "PWAT:entire atmosphere".to_string(),
        "updraft_helicity" | "uh" | "mxuphl" => return "MXUPHL:5000-2000 m above ground".to_string(),
        "wind_speed" | "wspd" | "wind" => "WIND",
        "snow" | "snowfall" | "weasd" => "WEASD",
        "cloud" | "cloud_cover" | "tcc" | "tcdc" => "TCDC",
        _ => var,
    };

    let grib_level = match level.to_lowercase().as_str() {
        "surface" | "sfc" => "surface",
        "2m" => "2 m above ground",
        "10m" => "10 m above ground",
        "atmosphere" | "entire" => "entire atmosphere",
        "0-3km" | "3000-0m" => "3000-0 m above ground",
        "0-1km" | "1000-0m" => "1000-0 m above ground",
        "0-6km" | "6000-0m" => "6000-0 m above ground",
        "2-5km" | "5000-2000m" => "5000-2000 m above ground",
        "255-0mb" | "ml" | "mixed_layer" => "255-0 mb above ground",
        l if l.ends_with("mb") || l.ends_with("hpa") => {
            let num = l.trim_end_matches("mb").trim_end_matches("hpa");
            return format!("{}:{} mb", grib_var, num);
        }
        _ => level,
    };

    format!("{}:{}", grib_var, grib_level)
}

fn build_projection(grid: &grib2::GridDefinition) -> Option<Box<dyn Projection>> {
    match grid.template {
        30 => {
            let mut lo1 = grid.lon1;
            if lo1 > 180.0 { lo1 -= 360.0; }
            let mut lov = grid.lov;
            if lov > 180.0 { lov -= 360.0; }
            Some(Box::new(LambertProjection::grib2(
                grid.latin1, grid.latin2, lov,
                grid.lat1, lo1,
                grid.dx, grid.dy,
                grid.nx, grid.ny,
            )))
        }
        0 => {
            let mut lo1 = grid.lon1;
            let mut lo2 = grid.lon2;
            if lo1 > 180.0 { lo1 -= 360.0; }
            if lo2 > 180.0 { lo2 -= 360.0; }
            Some(Box::new(LatLonProjection::new(
                grid.lat1, lo1, grid.lat2, lo2,
                grid.nx, grid.ny,
            )))
        }
        _ => None,
    }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
          + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().asin()
}
