use crate::output::{print_json, print_error};
use serde_json::json;
use std::path::PathBuf;

use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2;
use rustmet_core::render::colormap::{get_colormap, interpolate_color};
use rustmet_core::render::encode::write_png;
use wx_field::projection::{LambertProjection, LatLonProjection, Projection};

const TILE_SIZE: usize = 256;

// ── Tile directory ──────────────────────────────────────────────────

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn tile_dir() -> PathBuf {
    let dir = dirs_home().join(".wx-pro").join("tiles");
    std::fs::create_dir_all(&dir).ok();
    dir
}

// ── Web Mercator tile math ──────────────────────────────────────────

/// Convert tile (z, x, y) to lat/lon bounding box.
/// Returns (south, west, north, east) in degrees.
fn tile_bounds(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = (1u64 << z) as f64;
    let lon_min = x as f64 / n * 360.0 - 180.0;
    let lon_max = (x as f64 + 1.0) / n * 360.0 - 180.0;
    let lat_max = (std::f64::consts::PI * (1.0 - 2.0 * y as f64 / n))
        .sinh()
        .atan()
        .to_degrees();
    let lat_min = (std::f64::consts::PI * (1.0 - 2.0 * (y as f64 + 1.0) / n))
        .sinh()
        .atan()
        .to_degrees();
    (lat_min, lon_min, lat_max, lon_max)
}

/// Get all tiles at zoom level z that intersect a bounding box.
/// bbox = (min_lat, min_lon, max_lat, max_lon).
fn tiles_in_bbox(z: u32, bbox: (f64, f64, f64, f64)) -> Vec<(u32, u32)> {
    let n = 1u64 << z;
    let (min_lat, min_lon, max_lat, max_lon) = bbox;

    let x_min = ((min_lon + 180.0) / 360.0 * n as f64)
        .floor()
        .max(0.0) as u32;
    let x_max = ((max_lon + 180.0) / 360.0 * n as f64)
        .ceil()
        .min(n as f64 - 1.0) as u32;

    let y_min = ((1.0
        - (max_lat.to_radians().tan() + 1.0 / max_lat.to_radians().cos()).ln()
            / std::f64::consts::PI)
        / 2.0
        * n as f64)
        .floor()
        .max(0.0) as u32;
    let y_max = ((1.0
        - (min_lat.to_radians().tan() + 1.0 / min_lat.to_radians().cos()).ln()
            / std::f64::consts::PI)
        / 2.0
        * n as f64)
        .ceil()
        .min(n as f64 - 1.0) as u32;

    let mut tiles = Vec::new();
    for x in x_min..=x_max {
        for y in y_min..=y_max {
            tiles.push((x, y));
        }
    }
    tiles
}

// ── Bilinear interpolation ──────────────────────────────────────────

fn sample_bilinear(values: &[f64], nx: usize, ny: usize, gi: f64, gj: f64) -> Option<f64> {
    let i0 = gi.floor() as isize;
    let j0 = gj.floor() as isize;
    let i1 = i0 + 1;
    let j1 = j0 + 1;

    if i0 < 0 || j0 < 0 || i1 >= nx as isize || j1 >= ny as isize {
        return None;
    }

    let fi = gi - i0 as f64;
    let fj = gj - j0 as f64;

    let v00 = values[j0 as usize * nx + i0 as usize];
    let v10 = values[j0 as usize * nx + i1 as usize];
    let v01 = values[j1 as usize * nx + i0 as usize];
    let v11 = values[j1 as usize * nx + i1 as usize];

    if !v00.is_finite() || !v10.is_finite() || !v01.is_finite() || !v11.is_finite() {
        return None;
    }

    let val = v00 * (1.0 - fi) * (1.0 - fj)
        + v10 * fi * (1.0 - fj)
        + v01 * (1.0 - fi) * fj
        + v11 * fi * fj;
    Some(val)
}

// ── Mercator latitude interpolation ─────────────────────────────────

/// Interpolate latitude using mercator projection (not linear).
/// t=0 corresponds to lat_max (top), t=1 to lat_min (bottom).
fn mercator_lat(lat_max: f64, lat_min: f64, t: f64) -> f64 {
    let y_max = lat_max.to_radians().tan().asinh();
    let y_min = lat_min.to_radians().tan().asinh();
    let y = y_max + t * (y_min - y_max);
    y.sinh().atan().to_degrees()
}

// ── Tile rendering ──────────────────────────────────────────────────

fn render_tile(
    values: &[f64],
    nx: usize,
    ny: usize,
    proj: &dyn Projection,
    z: u32,
    x: u32,
    y: u32,
    colormap_name: &str,
    vmin: f64,
    vmax: f64,
) -> Vec<u8> {
    let cmap = get_colormap(colormap_name)
        .unwrap_or_else(|| get_colormap("temperature").unwrap());
    let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, x, y);
    let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4]; // fully transparent

    for py in 0..TILE_SIZE {
        for px in 0..TILE_SIZE {
            // Map pixel to lat/lon (Web Mercator projection within tile)
            let tx = (px as f64 + 0.5) / TILE_SIZE as f64;
            let ty = (py as f64 + 0.5) / TILE_SIZE as f64;

            let lon = lon_min + tx * (lon_max - lon_min);
            // Lat needs mercator interpolation, not linear
            let lat = mercator_lat(lat_max, lat_min, ty);

            let (gi, gj) = proj.latlon_to_grid(lat, lon);

            if let Some(val) = sample_bilinear(values, nx, ny, gi, gj) {
                if val.is_finite() && val.abs() < 1e15 && val > -900.0 {
                    let norm = ((val - vmin) / (vmax - vmin)).clamp(0.0, 1.0);
                    let (r, g, b) = interpolate_color(cmap, norm);
                    let idx = (py * TILE_SIZE + px) * 4;
                    pixels[idx] = r;
                    pixels[idx + 1] = g;
                    pixels[idx + 2] = b;
                    pixels[idx + 3] = 200; // semi-transparent for overlay
                }
            }
        }
    }
    pixels
}

// ── Helpers copied from cmd_model_image (private in that module) ────

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
            let lvl = level.to_lowercase();
            if lvl.contains("1") {
                return "HLCY:1000-0 m above ground".to_string();
            }
            return "HLCY:3000-0 m above ground".to_string();
        }
        "pwat" | "precipitable_water" => return "PWAT:entire atmosphere".to_string(),
        "updraft_helicity" | "uh" | "mxuphl" => {
            return "MXUPHL:5000-2000 m above ground".to_string()
        }
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
                ("temperature", 233.15, 323.15)
            } else {
                ("temperature", -40.0, 50.0)
            }
        }
        "dewpoint" | "td" | "dew" => {
            if units.contains("K") {
                ("dewpoint", 243.15, 303.15)
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
            let (dmin, dmax) = data_range(values);
            ("temperature", dmin, dmax)
        }
        _ => {
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
            if lo1 > 180.0 {
                lo1 -= 360.0;
            }
            let mut lov = grid.lov;
            if lov > 180.0 {
                lov -= 360.0;
            }
            Some(Box::new(LambertProjection::grib2(
                grid.latin1, grid.latin2, lov, grid.lat1, lo1, grid.dx, grid.dy, grid.nx, grid.ny,
            )))
        }
        // Lat/Lon (GFS)
        0 => {
            let mut lo1 = grid.lon1;
            let mut lo2 = grid.lon2;
            if lo1 > 180.0 {
                lo1 -= 360.0;
            }
            if lo2 > 180.0 {
                lo2 -= 360.0;
            }
            Some(Box::new(LatLonProjection::new(
                grid.lat1, lo1, grid.lat2, lo2, grid.nx, grid.ny,
            )))
        }
        _ => {
            eprintln!(
                "Warning: unsupported grid template {} for tile projection",
                grid.template
            );
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
            if v < vmin {
                vmin = v;
            }
            if v > vmax {
                vmax = v;
            }
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

// ── Public entry point ──────────────────────────────────────────────

/// Generate XYZ map tiles (256x256 transparent PNGs) from NWP model data.
///
/// Supports two modes:
/// - Single tile: provide z, x, y to render one tile.
/// - Tile set: provide z only (x=None, y=None) to render all tiles at that
///   zoom level that intersect the model domain.
pub fn run(
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    z: u32,
    x: Option<u32>,
    y: Option<u32>,
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
    eprintln!(
        "Downloading {} {} {} f{:02} [{}]...",
        model_lower, run_label, product, fhour, pattern
    );

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

    eprintln!(
        "Downloaded {} bytes from {} in {}ms",
        result.data.len(),
        result.source_name,
        download_ms
    );

    // Parse GRIB2
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

    if values.len() != nx * ny {
        print_error(&format!(
            "Grid size mismatch: {} values, expected {}x{}={}",
            values.len(),
            nx,
            ny,
            nx * ny
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

    // Build projection for lat/lon <-> grid mapping
    let proj = match build_projection(&msg.grid) {
        Some(p) => p,
        None => print_error("Cannot build projection for this grid type"),
    };

    // GRIB2 data may need row flipping (scan mode)
    let mut render_values = values;
    if msg.grid.scan_mode & 0x40 != 0 {
        grib2::flip_rows(&mut render_values, nx, ny);
    }

    // Determine which tiles to generate
    let tile_coords: Vec<(u32, u32)> = match (x, y) {
        (Some(tx), Some(ty)) => vec![(tx, ty)],
        (None, None) => {
            // Tile set mode: all tiles intersecting model domain
            let bbox = proj.bounding_box();
            let coords = tiles_in_bbox(z, bbox);
            eprintln!(
                "Tile set mode: {} tiles at z={} intersecting model domain",
                coords.len(),
                z
            );
            coords
        }
        _ => print_error("Provide both --x and --y for single tile, or neither for tile set"),
    };

    // Render tiles
    let render_start = std::time::Instant::now();
    let mut tile_results = Vec::new();

    for &(tx, ty) in &tile_coords {
        let pixels = render_tile(
            &render_values,
            nx,
            ny,
            proj.as_ref(),
            z,
            tx,
            ty,
            colormap,
            vmin,
            vmax,
        );

        // Build output path: ~/.wx-pro/tiles/{model}/{var}/{level}/f{fhour}/{z}/{x}/{y}.png
        let var_safe = var.replace('/', "_").replace(':', "_").replace(' ', "_");
        let level_safe = level.replace(' ', "_");
        let out_dir = tile_dir()
            .join(&model_lower)
            .join(&var_safe)
            .join(&level_safe)
            .join(format!("f{:02}", fhour))
            .join(z.to_string())
            .join(tx.to_string());

        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            print_error(&format!("Failed to create tile directory: {}", e));
        }

        let out_path = out_dir.join(format!("{}.png", ty));

        if let Err(e) = write_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32, &out_path) {
            print_error(&format!("Failed to write tile PNG: {}", e));
        }

        let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, tx, ty);
        tile_results.push(json!({
            "tile_path": out_path.to_string_lossy(),
            "tile": { "z": z, "x": tx, "y": ty },
            "bounds": {
                "south": lat_min,
                "west": lon_min,
                "north": lat_max,
                "east": lon_max,
            },
        }));
    }

    let render_ms = render_start.elapsed().as_millis();
    let tiles_generated = tile_results.len();

    eprintln!(
        "Rendered {} tile(s) with colormap '{}' in {}ms",
        tiles_generated, colormap, render_ms
    );

    // Output JSON
    if tile_results.len() == 1 {
        // Single tile output
        let t = &tile_results[0];
        print_json(
            &json!({
                "tile_path": t["tile_path"],
                "tile": t["tile"],
                "bounds": t["bounds"],
                "model": model_lower.to_uppercase(),
                "run": run_label,
                "variable": param_name,
                "units": param_units,
                "colormap": colormap,
                "value_range": [vmin, vmax],
                "format": "png",
                "tile_size": TILE_SIZE,
                "transparent": true,
                "performance": {
                    "download_ms": download_ms,
                    "render_ms": render_ms,
                    "tiles_generated": tiles_generated,
                },
            }),
            pretty,
        );
    } else {
        // Tile set output
        print_json(
            &json!({
                "tiles": tile_results,
                "model": model_lower.to_uppercase(),
                "run": run_label,
                "variable": param_name,
                "units": param_units,
                "colormap": colormap,
                "value_range": [vmin, vmax],
                "zoom": z,
                "format": "png",
                "tile_size": TILE_SIZE,
                "transparent": true,
                "performance": {
                    "download_ms": download_ms,
                    "render_ms": render_ms,
                    "tiles_generated": tiles_generated,
                },
            }),
            pretty,
        );
    }
}
