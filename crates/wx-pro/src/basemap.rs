use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static GEODATA: OnceLock<Option<rustmaps::geo::GeoData>> = OnceLock::new();

/// Find the geodata directory (Natural Earth shapefiles).
fn find_geodata_dir() -> Option<PathBuf> {
    for var in ["WRF_GEODATA", "HRRR_GEODATA"] {
        if let Ok(p) = std::env::var(var) {
            let path = PathBuf::from(&p);
            if path.exists() { return Some(path); }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let c = dir.join("geodata");
            if c.exists() { return Some(c); }
        }
    }
    let candidates = [
        "C:/Users/drew/rustmaps/data",
        "../rustmaps/data",
        "../../rustmaps/data",
        "./geodata",
    ];
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() && p.is_dir() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Get the cached geodata singleton.
pub fn get_geodata() -> Option<&'static rustmaps::geo::GeoData> {
    GEODATA
        .get_or_init(|| {
            let dir = find_geodata_dir()?;
            match rustmaps::geo::GeoData::load(&dir) {
                Ok(g) => Some(g),
                Err(e) => {
                    eprintln!("Warning: failed to load geodata: {}", e);
                    None
                }
            }
        })
        .as_ref()
}

/// Draw basemap with dark outline + light core for visibility on any background.
///
/// `to_pixel` maps (lat, lon) -> Option<(x, y)> in pixel coordinates.
pub fn draw_basemap<F>(
    pixels: &mut [u8],
    img_width: usize,
    img_height: usize,
    to_pixel: F,
) where
    F: Fn(f64, f64) -> Option<(f64, f64)>,
{
    let geo = match get_geodata() {
        Some(g) => g,
        None => return,
    };

    // Pass 1: dark shadow (offset by 1px or thicker) for outline
    let shadow = [0, 0, 0, 140];

    for polyline in &geo.coastlines_50m {
        draw_polyline_thick(pixels, img_width, img_height, polyline, &to_pixel, shadow);
    }
    for polyline in &geo.country_borders {
        draw_polyline_thick(pixels, img_width, img_height, polyline, &to_pixel, shadow);
    }
    for polyline in &geo.state_borders {
        draw_polyline_thick(pixels, img_width, img_height, polyline, &to_pixel, shadow);
    }

    // Pass 2: bright core lines on top
    let coast_color = [255, 255, 255, 200];
    let border_color = [220, 220, 220, 200];
    let state_color = [200, 200, 200, 160];

    for polyline in &geo.coastlines_50m {
        draw_polyline_thin(pixels, img_width, img_height, polyline, &to_pixel, coast_color);
    }
    for polyline in &geo.country_borders {
        draw_polyline_thin(pixels, img_width, img_height, polyline, &to_pixel, border_color);
    }
    for polyline in &geo.state_borders {
        draw_polyline_thin(pixels, img_width, img_height, polyline, &to_pixel, state_color);
    }
}

/// Draw a polyline with 3px width (for shadow/outline pass).
fn draw_polyline_thick<F>(
    pixels: &mut [u8],
    w: usize,
    h: usize,
    points: &[(f64, f64)],
    to_pixel: &F,
    color: [u8; 4],
) where
    F: Fn(f64, f64) -> Option<(f64, f64)>,
{
    for pair in points.windows(2) {
        let (lon1, lat1) = pair[0];
        let (lon2, lat2) = pair[1];
        if (lon2 - lon1).abs() > 10.0 || (lat2 - lat1).abs() > 10.0 { continue; }

        if let (Some((x1, y1)), Some((x2, y2))) = (to_pixel(lat1, lon1), to_pixel(lat2, lon2)) {
            if out_of_frame(x1, y1, x2, y2, w, h) { continue; }
            // Draw 3 lines for thickness
            draw_line_aa(pixels, w, h, x1 - 1.0, y1, x2 - 1.0, y2, color);
            draw_line_aa(pixels, w, h, x1 + 1.0, y1, x2 + 1.0, y2, color);
            draw_line_aa(pixels, w, h, x1, y1 - 1.0, x2, y2 - 1.0, color);
            draw_line_aa(pixels, w, h, x1, y1 + 1.0, x2, y2 + 1.0, color);
        }
    }
}

/// Draw a polyline with 1px width (for core line pass).
fn draw_polyline_thin<F>(
    pixels: &mut [u8],
    w: usize,
    h: usize,
    points: &[(f64, f64)],
    to_pixel: &F,
    color: [u8; 4],
) where
    F: Fn(f64, f64) -> Option<(f64, f64)>,
{
    for pair in points.windows(2) {
        let (lon1, lat1) = pair[0];
        let (lon2, lat2) = pair[1];
        if (lon2 - lon1).abs() > 10.0 || (lat2 - lat1).abs() > 10.0 { continue; }

        if let (Some((x1, y1)), Some((x2, y2))) = (to_pixel(lat1, lon1), to_pixel(lat2, lon2)) {
            if out_of_frame(x1, y1, x2, y2, w, h) { continue; }
            draw_line_aa(pixels, w, h, x1, y1, x2, y2, color);
        }
    }
}

#[inline]
fn out_of_frame(x1: f64, y1: f64, x2: f64, y2: f64, w: usize, h: usize) -> bool {
    let margin = 50.0;
    (x1 < -margin && x2 < -margin)
        || (x1 > w as f64 + margin && x2 > w as f64 + margin)
        || (y1 < -margin && y2 < -margin)
        || (y1 > h as f64 + margin && y2 > h as f64 + margin)
}

/// Draw an anti-aliased line (Wu's algorithm).
fn draw_line_aa(
    pixels: &mut [u8],
    w: usize,
    h: usize,
    mut x0: f64,
    mut y0: f64,
    mut x1: f64,
    mut y1: f64,
    color: [u8; 4],
) {
    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    if steep {
        std::mem::swap(&mut x0, &mut y0);
        std::mem::swap(&mut x1, &mut y1);
    }
    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < 1e-10 { 1.0 } else { dy / dx };

    let xend = x0.round();
    let yend = y0 + gradient * (xend - x0);
    let xpxl1 = xend as i32;
    let mut intery = yend + gradient;

    let xend2 = x1.round();
    let xpxl2 = xend2 as i32;

    for x in xpxl1..=xpxl2 {
        let y = intery.floor() as i32;
        let frac = intery - intery.floor();

        if steep {
            plot_aa(pixels, w, h, y, x, color, 1.0 - frac);
            plot_aa(pixels, w, h, y + 1, x, color, frac);
        } else {
            plot_aa(pixels, w, h, x, y, color, 1.0 - frac);
            plot_aa(pixels, w, h, x, y + 1, color, frac);
        }
        intery += gradient;
    }
}

#[inline]
fn plot_aa(pixels: &mut [u8], w: usize, h: usize, x: i32, y: i32, color: [u8; 4], intensity: f64) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { return; }
    let idx = (y as usize * w + x as usize) * 4;
    if idx + 3 >= pixels.len() { return; }
    let alpha = (color[3] as f64 / 255.0) * intensity;
    let inv = 1.0 - alpha;
    pixels[idx] = (pixels[idx] as f64 * inv + color[0] as f64 * alpha) as u8;
    pixels[idx + 1] = (pixels[idx + 1] as f64 * inv + color[1] as f64 * alpha) as u8;
    pixels[idx + 2] = (pixels[idx + 2] as f64 * inv + color[2] as f64 * alpha) as u8;
    pixels[idx + 3] = pixels[idx + 3].max((color[3] as f64 * intensity) as u8);
}
