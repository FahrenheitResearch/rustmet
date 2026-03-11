use crate::output::{print_json, print_error};
use crate::basemap;
use serde_json::json;
use std::path::PathBuf;

use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2;
use rustmet_core::render::{render_raster_par, write_png};
use wx_field::projection::{LambertProjection, LatLonProjection, Projection};

/// Default image directory
fn image_dir() -> PathBuf {
    let dir = dirs_home().join(".wx-pro").join("images");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Render a HRRR/model field as a PNG image, save to disk, return file path.
pub fn run(
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    pretty: bool,
) {
    let model_lower = model.to_lowercase();
    if !["hrrr", "rap", "gfs", "nam"].contains(&model_lower.as_str()) {
        print_error(&format!(
            "Model '{}' not supported. Use: hrrr, rap, gfs, nam",
            model
        ));
    }

    // Build .idx pattern
    let pattern = build_pattern(var, level);

    // Determine product type
    let product = if level.contains("mb") || level.contains("hPa") {
        "prs"
    } else {
        "sfc"
    };

    // Create download client
    let client = match DownloadClient::new() {
        Ok(c) => c,
        Err(e) => print_error(&format!("Failed to create HTTP client: {}", e)),
    };

    // Find latest run
    eprintln!("Finding latest {} run...", model_lower);
    let (date, hour) = match rustmet_core::models::find_latest_run(&client, &model_lower) {
        Ok(r) => r,
        Err(e) => print_error(&format!("No model run found: {}", e)),
    };

    let run_label = format!("{}/{:02}z", date, hour);
    eprintln!("Downloading {} {} {} f{:02} [{}]...", model_lower, run_label, product, fhour, pattern);

    let patterns: Vec<&str> = vec![pattern.as_str()];
    let download_start = std::time::Instant::now();
    let result = match fetch_with_fallback(
        &client,
        &model_lower,
        &date,
        hour,
        product,
        fhour,
        Some(&patterns),
        None,
    ) {
        Ok(r) => r,
        Err(e) => print_error(&format!("Download failed: {}", e)),
    };
    let download_ms = download_start.elapsed().as_millis();
    let bytes_downloaded = result.data.len();

    eprintln!("Downloaded {} bytes from {} in {}ms", bytes_downloaded, result.source_name, download_ms);

    // Parse GRIB2
    let parse_start = std::time::Instant::now();
    let grib = match grib2::Grib2File::from_bytes(&result.data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("GRIB2 parse failed: {}", e)),
    };
    let parse_ms = parse_start.elapsed().as_millis();

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

    if values.len() != nx * ny {
        print_error(&format!(
            "Grid size mismatch: {} values, expected {}x{}={}",
            values.len(), nx, ny, nx * ny
        ));
    }

    // Get variable metadata
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

    // Select colormap and value range
    let (colormap, vmin, vmax) = select_colormap_and_range(var, param_units, &values);

    eprintln!("Rendering {}x{} grid with colormap '{}' ({} to {})...", nx, ny, colormap, vmin, vmax);

    // GRIB2 data may need row flipping (scan mode). Most HRRR data is top-to-bottom
    // but the renderer expects top-to-bottom, so flip if scan mode indicates bottom-to-top.
    let mut render_values = values;
    if msg.grid.scan_mode & 0x40 != 0 {
        // Bit 2 set = rows scan from bottom to top; flip for image rendering
        grib2::flip_rows(&mut render_values, nx, ny);
    }

    // Render
    let render_start = std::time::Instant::now();
    let mut pixels = render_raster_par(&render_values, nx, ny, colormap, vmin, vmax);

    // Draw basemap overlay (state lines, coastlines, country borders)
    let grid = &msg.grid;
    let flipped = msg.grid.scan_mode & 0x40 != 0;
    let proj = build_projection(grid);
    if let Some(ref proj) = proj {
        let ny_f = ny;
        basemap::draw_basemap(&mut pixels, nx, ny, |lat, lon| {
            let (gi, gj) = proj.latlon_to_grid(lat, lon);
            if gi < -0.5 || gi >= nx as f64 + 0.5 || gj < -0.5 || gj >= ny_f as f64 + 0.5 {
                return None;
            }
            // If rows were flipped for rendering, reverse j mapping
            let pj = if flipped { (ny_f as f64 - 1.0) - gj } else { gj };
            Some((gi, pj))
        });
    }

    let render_ms = render_start.elapsed().as_millis();

    // Save PNG
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    let var_safe = var.replace('/', "_").replace(':', "_").replace(' ', "_");
    let level_safe = level.replace(' ', "_");
    let out_path = image_dir().join(format!(
        "{}_{}_{}_{}_f{:02}_{}.png",
        model_lower, var_safe, level_safe, run_label.replace('/', "_"), fhour, timestamp
    ));

    if let Err(e) = write_png(&pixels, nx as u32, ny as u32, &out_path) {
        print_error(&format!("Failed to write PNG: {}", e));
    }

    let out_path_str = out_path.to_string_lossy().to_string();

    print_json(&json!({
        "image_path": out_path_str,
        "model": model_lower.to_uppercase(),
        "run": run_label,
        "forecast_hour": fhour,
        "variable": param_name,
        "variable_short": var,
        "level": level,
        "units": param_units,
        "colormap": colormap,
        "value_range": [vmin, vmax],
        "grid": {
            "nx": nx,
            "ny": ny,
        },
        "performance": {
            "download_ms": download_ms,
            "parse_ms": parse_ms,
            "render_ms": render_ms,
            "file_size_bytes": bytes_downloaded,
        },
    }), pretty);
}

/// Build an .idx search pattern from user-friendly var/level names.
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
            // Default to 0-3km SRH
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

/// Select colormap name and value range based on variable type.
fn select_colormap_and_range<'a>(var: &str, units: &str, values: &[f64]) -> (&'a str, f64, f64) {
    match var.to_lowercase().as_str() {
        "cape" => ("cape", 0.0, 5000.0),
        "cin" => ("cape", -500.0, 0.0),
        "reflectivity" | "refl" | "refc" => ("nws_reflectivity", -10.0, 75.0),
        "temperature" | "temp" | "t" => {
            if units.contains("K") {
                // Convert range to Celsius for display
                ("temperature", 233.15, 323.15) // -40 to 50°C in K
            } else {
                ("temperature", -40.0, 50.0)
            }
        }
        "dewpoint" | "td" | "dew" => {
            if units.contains("K") {
                ("dewpoint", 243.15, 303.15) // -30 to 30°C in K
            } else {
                ("dewpoint", -30.0, 30.0)
            }
        }
        "wind_u" | "u" | "ugrd" | "wind_v" | "v" | "vgrd" => ("wind", -50.0, 50.0),
        "gust" | "wind_speed" | "wspd" | "wind" => ("wind", 0.0, 50.0),
        "helicity" | "hlcy" | "srh" => ("helicity", 0.0, 500.0),
        "updraft_helicity" | "uh" | "mxuphl" => ("helicity", 0.0, 200.0),
        "rh" | "relative_humidity" => ("relative_humidity", 0.0, 100.0),
        "pressure" | "pres" => ("pressure", 95000.0, 105000.0),
        "mslp" => ("pressure", 980.0, 1040.0),
        "visibility" | "vis" => ("visibility", 0.0, 30000.0),
        "precip" | "precipitation" | "apcp" => ("precipitation", 0.0, 50.0),
        "pwat" | "precipitable_water" => ("precipitation", 0.0, 75.0),
        "cloud" | "cloud_cover" | "tcc" | "tcdc" => ("cloud_cover", 0.0, 100.0),
        "snow" | "snowfall" | "weasd" => ("snow", 0.0, 50.0),
        "height" | "hgt" => {
            // Auto-range from data
            let (dmin, dmax) = data_range(values);
            ("temperature", dmin, dmax)
        }
        _ => {
            // Auto-range from data
            let (dmin, dmax) = data_range(values);
            ("temperature", dmin, dmax)
        }
    }
}

/// Build a map projection from a GRIB2 grid definition.
fn build_projection(grid: &grib2::GridDefinition) -> Option<Box<dyn Projection>> {
    match grid.template {
        // Lambert Conformal Conic (HRRR, NAM, RAP)
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
        // Lat/Lon (GFS)
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
        _ => {
            eprintln!("Warning: unsupported grid template {} for basemap projection", grid.template);
            None
        }
    }
}

/// Compute min/max from data, ignoring NaN.
fn data_range(values: &[f64]) -> (f64, f64) {
    let mut vmin = f64::INFINITY;
    let mut vmax = f64::NEG_INFINITY;
    for &v in values {
        if v.is_finite() && v.abs() < 1e15 {
            if v < vmin { vmin = v; }
            if v > vmax { vmax = v; }
        }
    }
    if !vmin.is_finite() || !vmax.is_finite() {
        (0.0, 1.0)
    } else if (vmax - vmin).abs() < 1e-10 {
        (vmin - 1.0, vmax + 1.0)
    } else {
        (vmin, vmax)
    }
}
