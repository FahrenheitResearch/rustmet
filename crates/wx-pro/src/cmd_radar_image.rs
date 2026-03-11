use crate::output::{print_json, print_error};
use crate::cmd_radar::{find_nearest_site, find_latest_file, http_get_bytes, maybe_decompress_gz};
use crate::basemap;
use serde_json::json;
use std::path::PathBuf;

use wx_radar::level2::Level2File;
use wx_radar::products::RadarProduct;
use wx_radar::render::render_ppi;
use rustmet_core::render::encode::write_png;

const NEXRAD_BASE_URL: &str = "https://unidata-nexrad-level2.s3.amazonaws.com";

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

/// Render NEXRAD radar PPI to PNG, save to disk, return file path.
pub fn run(
    site: &str,
    lat: Option<f64>,
    lon: Option<f64>,
    product: &str,
    size: u32,
    raw: bool,
    pretty: bool,
) {
    // Resolve site
    let site_id = if let (Some(la), Some(lo)) = (lat, lon) {
        find_nearest_site(la, lo)
    } else if !site.is_empty() {
        site.to_uppercase()
    } else {
        print_error("Provide --site KTLX or --lat/--lon");
    };

    let site_info = wx_radar::sites::find_site(&site_id);

    // Find and download latest file
    let now = chrono::Utc::now();
    let today = now.format("%Y/%m/%d").to_string();
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y/%m/%d").to_string();

    let key = find_latest_file(&site_id, &today)
        .or_else(|| find_latest_file(&site_id, &yesterday))
        .unwrap_or_else(|| print_error(&format!("No NEXRAD files found for {} in last 24h", site_id)));

    let filename = key.rsplit('/').next().unwrap_or(&key).to_string();

    let download_start = std::time::Instant::now();
    let url = format!("{}/{}", NEXRAD_BASE_URL, key);
    let raw_data = http_get_bytes(&url);
    let data = maybe_decompress_gz(raw_data);
    let download_ms = download_start.elapsed().as_millis();

    // Parse
    let l2 = match Level2File::parse(&data) {
        Ok(f) => f,
        Err(e) => print_error(&format!("Failed to parse Level 2: {}", e)),
    };

    // Select radar product
    let radar_product = match product.to_lowercase().as_str() {
        "ref" | "reflectivity" | "refl" => RadarProduct::Reflectivity,
        "vel" | "velocity" => RadarProduct::Velocity,
        "sw" | "spectrum_width" => RadarProduct::SpectrumWidth,
        "zdr" | "differential_reflectivity" => RadarProduct::DifferentialReflectivity,
        "rho" | "cc" | "correlation_coefficient" => RadarProduct::CorrelationCoefficient,
        "phi" | "kdp" | "specific_differential_phase" => RadarProduct::SpecificDifferentialPhase,
        _ => RadarProduct::Reflectivity,
    };

    // Find lowest elevation sweep with this product
    let sweep_idx = l2.sweeps.iter().position(|s| {
        s.radials.iter().any(|r| {
            r.moments.iter().any(|m| m.product == radar_product)
        })
    });

    let sweep_idx = match sweep_idx {
        Some(i) => i,
        None => print_error(&format!("Product {} not found in volume scan", product)),
    };

    let sweep = &l2.sweeps[sweep_idx];
    let elev = sweep.elevation_angle;

    // Render PPI
    let render_start = std::time::Instant::now();
    let ppi = match render_ppi(sweep, radar_product, size) {
        Some(p) => p,
        None => print_error("Failed to render PPI — no data for this product/sweep"),
    };
    let render_ms = render_start.elapsed().as_millis();

    let mut pixels = ppi.pixels;
    let img_size = ppi.size as usize;

    if !raw {
        // Draw range rings (50km, 100km, 150km, 200km)
        let center = img_size as f64 / 2.0;
        let px_per_km = center / ppi.range_km;
        for ring_km in [50.0, 100.0, 150.0, 200.0] {
            let ring_px = ring_km * px_per_km;
            if ring_px > 0.0 && ring_px < center {
                draw_circle(&mut pixels, img_size, center, center, ring_px, [180, 180, 180, 160]);
            }
        }

        // Draw crosshairs (N-S, E-W lines through center)
        for i in 0..img_size {
            // Vertical line (N-S)
            let cx = img_size / 2;
            blend_pixel(&mut pixels, img_size, cx, i, [120, 120, 120, 80]);
            // Horizontal line (E-W)
            blend_pixel(&mut pixels, img_size, i, img_size / 2, [120, 120, 120, 80]);
        }

        // Draw basemap (state lines, coastlines, country borders)
        if let Some(si) = site_info.as_ref() {
            let site_lat = si.lat;
            let site_lon = si.lon;
            let range = ppi.range_km;
            let half = img_size as f64 / 2.0;
            let km_per_deg_lat = 111.139;

            basemap::draw_basemap(&mut pixels, img_size, img_size, |lat, lon| {
                // Convert lat/lon offset from site to km
                let dy_km = (lat - site_lat) * km_per_deg_lat;
                let dx_km = (lon - site_lon) * km_per_deg_lat * site_lat.to_radians().cos();

                // Check range
                if dx_km.abs() > range || dy_km.abs() > range {
                    return None;
                }

                // Convert km to pixels (center = site, +x = east, +y = south in image)
                let px = half + dx_km * (half / range);
                let py = half - dy_km * (half / range); // y is inverted (north = up)

                if px < -1.0 || px >= img_size as f64 + 1.0 || py < -1.0 || py >= img_size as f64 + 1.0 {
                    return None;
                }
                Some((px, py))
            });
        }

        // Draw center dot (radar site)
        let cx = img_size / 2;
        let cy = img_size / 2;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let px = (cx as i32 + dx) as usize;
                let py = (cy as i32 + dy) as usize;
                if px < img_size && py < img_size {
                    set_pixel(&mut pixels, img_size, px, py, [255, 255, 255, 255]);
                }
            }
        }
    }

    // Save PNG
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    let product_short = radar_product.short_name();
    let out_path = image_dir().join(format!("{}_{}_{}_{}.png", site_id, product_short, elev, timestamp));

    if let Err(e) = write_png(&pixels, ppi.size, ppi.size, &out_path) {
        print_error(&format!("Failed to write PNG: {}", e));
    }

    let out_path_str = out_path.to_string_lossy().to_string();

    print_json(&json!({
        "image_path": out_path_str,
        "site": site_id,
        "site_name": site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown"),
        "product": radar_product.short_name(),
        "product_full": radar_product.display_name(),
        "elevation_deg": (elev * 10.0).round() / 10.0,
        "range_km": (ppi.range_km * 10.0).round() / 10.0,
        "raw": raw,
        "image_size": ppi.size,
        "file": filename,
        "performance": {
            "download_ms": download_ms,
            "render_ms": render_ms,
            "file_size_bytes": data.len(),
        },
    }), pretty);
}

/// Draw a circle outline on an RGBA pixel buffer.
fn draw_circle(pixels: &mut [u8], img_size: usize, cx: f64, cy: f64, radius: f64, color: [u8; 4]) {
    let steps = (radius * 4.0).max(360.0) as usize;
    for i in 0..steps {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
        let px = (cx + radius * angle.cos()) as usize;
        let py = (cy - radius * angle.sin()) as usize;
        if px < img_size && py < img_size {
            blend_pixel(pixels, img_size, px, py, color);
        }
    }
}

/// Blend a pixel with alpha compositing.
fn blend_pixel(pixels: &mut [u8], img_size: usize, x: usize, y: usize, color: [u8; 4]) {
    let idx = (y * img_size + x) * 4;
    if idx + 3 >= pixels.len() { return; }
    let alpha = color[3] as f32 / 255.0;
    let inv = 1.0 - alpha;
    pixels[idx] = (pixels[idx] as f32 * inv + color[0] as f32 * alpha) as u8;
    pixels[idx + 1] = (pixels[idx + 1] as f32 * inv + color[1] as f32 * alpha) as u8;
    pixels[idx + 2] = (pixels[idx + 2] as f32 * inv + color[2] as f32 * alpha) as u8;
    pixels[idx + 3] = pixels[idx + 3].max(color[3]);
}

/// Set a pixel to a solid color.
fn set_pixel(pixels: &mut [u8], img_size: usize, x: usize, y: usize, color: [u8; 4]) {
    let idx = (y * img_size + x) * 4;
    if idx + 3 >= pixels.len() { return; }
    pixels[idx] = color[0];
    pixels[idx + 1] = color[1];
    pixels[idx + 2] = color[2];
    pixels[idx + 3] = color[3];
}
