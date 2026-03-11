/// Rendering engine for weather map plots.
/// Ported from wrf-solar with trait-based projection support.
///
/// Features:
/// - 11" x 8.5" at 100 DPI = 1100 x 850 pixels
/// - White background
/// - Product name centered at top (bold, 12pt equivalent)
/// - Valid time left, credit center, Init time right
/// - State boundaries in black (0.5px)
/// - Horizontal colorbar at bottom (2% height)
/// - Map fills 98% height, full width
/// - Target aspect ratio 1.5:1 (width:height) for map area
///
/// Visual features:
/// - Wu anti-aliased lines
/// - Marching squares contours with inline labels
/// - 3x3 supersampled fill for smooth contour boundaries
/// - Discrete banded colorbar with edge lines
/// - Filled wind barb pennants and calm circles
/// - TTF text rendering via fontdue

use std::sync::OnceLock;
use crate::colormaps;
use crate::products::Product;
use rustmet_core::projection::Projection;

/// Figure dimensions matching solarpower07's matplotlib: 11x8.5" at 100dpi
pub const FIG_WIDTH: u32 = 1100;
pub const FIG_HEIGHT: u32 = 850;

/// Layout zones (in pixels from top)
const TITLE_HEIGHT: u32 = 40;
const SUBTITLE_HEIGHT: u32 = 18;
const COLORBAR_HEIGHT: u32 = 20;
const COLORBAR_MARGIN: u32 = 14; // space above/below colorbar
const BOTTOM_MARGIN: u32 = 4;

/// Map area dimensions
pub fn map_area() -> (u32, u32, u32, u32) {
    // (x_start, y_start, width, height)
    let y_start = TITLE_HEIGHT + SUBTITLE_HEIGHT;
    let map_height = FIG_HEIGHT - y_start - COLORBAR_HEIGHT - COLORBAR_MARGIN * 2 - BOTTOM_MARGIN;
    (0, y_start, FIG_WIDTH, map_height)
}

// ============================================================
// GEODATA LOADING (matching wrf-render's approach)
// ============================================================

static GEO_DATA: OnceLock<Option<rustmaps::geo::GeoData>> = OnceLock::new();

fn find_geodata_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    for env_var in &["WRF_GEODATA", "HRRR_GEODATA"] {
        if let Ok(val) = std::env::var(env_var) {
            let p = PathBuf::from(&val);
            if p.exists() {
                return Some(p);
            }
        }
    }

    // Check next to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("geodata");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // Check current directory
    let cwd = PathBuf::from("geodata");
    if cwd.exists() {
        return Some(cwd);
    }

    // Common locations
    for path in &[
        "/usr/share/rustmaps/geodata",
        "/usr/local/share/rustmaps/geodata",
    ] {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

fn get_geodata() -> Option<&'static rustmaps::geo::GeoData> {
    GEO_DATA.get_or_init(|| {
        if let Some(dir) = find_geodata_dir() {
            eprintln!("[render] Loading geodata from {}", dir.display());
            match rustmaps::geo::GeoData::load(&dir) {
                Ok(data) => Some(data),
                Err(e) => {
                    eprintln!("[render] Warning: failed to load geodata: {}", e);
                    None
                }
            }
        } else {
            eprintln!("[render] Warning: no geodata directory found. Set WRF_GEODATA or HRRR_GEODATA.");
            None
        }
    }).as_ref()
}

// ============================================================
// COORDINATE CONVERSIONS
// ============================================================

/// Convert lat/lon to pixel coordinates within the map area
fn latlon_to_px(lat: f64, lon: f64, proj: &dyn Projection, map_w: u32, map_h: u32) -> (f32, f32) {
    let (gi, gj) = proj.latlon_to_grid(lat, lon);
    let sx = map_w as f64 / proj.nx() as f64;
    let sy = map_h as f64 / proj.ny() as f64;
    (
        (gi * sx) as f32,
        (map_h as f64 - 1.0 - gj * sy) as f32,
    )
}

/// Convert map-area pixel (px, py) to grid coordinates (gx, gy)
#[inline]
#[allow(dead_code)]
fn px_to_grid(px: u32, py: u32, map_w: u32, map_h: u32, nx: usize, ny: usize) -> (f32, f32) {
    let gx = px as f32 * nx as f32 / map_w as f32;
    let gy = (map_h - 1 - py) as f32 * ny as f32 / map_h as f32;
    (gx, gy)
}

// ============================================================
// MAIN RENDER ENTRY POINT
// ============================================================

/// Render a complete plot to PNG bytes.
pub fn render_plot(
    values: &[f64],
    nx: usize,
    ny: usize,
    product: &Product,
    proj: &dyn Projection,
    init_time_str: &str,
    valid_time_str: &str,
    wind_u: Option<&[f64]>,
    wind_v: Option<&[f64]>,
    contour_data: Option<&[f64]>,
    contour_interval: Option<f64>,
) -> Vec<u8> {
    let w = FIG_WIDTH;
    let h = FIG_HEIGHT;
    let mut pixels = vec![[255u8, 255, 255, 255]; (w * h) as usize]; // White background

    let (map_x, map_y, map_w, map_h) = map_area();

    // 1. Render base map (land/water fill)
    render_base_map(&mut pixels, w, map_x, map_y, map_w, map_h, proj);

    // 2. Render weather data overlay
    render_data_overlay(
        &mut pixels, w, map_x, map_y, map_w, map_h,
        values, nx, ny, product, proj,
    );

    // 3. Render contour overlay lines (height contours, MSLP)
    if let Some(cdata) = contour_data {
        if let Some(interval) = contour_interval {
            render_contour_lines(
                &mut pixels, w, map_x, map_y, map_w, map_h,
                cdata, nx, ny, interval,
            );
        }
    }

    // 5. Render wind barbs
    if let (Some(u), Some(v)) = (wind_u, wind_v) {
        render_wind_barbs(
            &mut pixels, w, map_x, map_y, map_w, map_h,
            u, v, nx, ny,
        );
    }

    // 6. Re-draw borders ON TOP of data (matching solarpower07 z-order)
    render_borders(&mut pixels, w, map_x, map_y, map_w, map_h, proj);

    // 7. Render title bar
    render_title(&mut pixels, w, product, valid_time_str, init_time_str);

    // 9. Render colorbar
    render_colorbar(&mut pixels, w, h, product);

    // 10. Encode to PNG
    encode_png(&pixels, w, h)
}

// ============================================================
// BASE MAP RENDERING
// ============================================================

fn render_base_map(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    proj: &dyn Projection,
) {
    let ocean_color = [200u8, 220, 240, 255];
    let land_color = [245u8, 245, 240, 255];

    // Fill entire map area with ocean color
    for py in map_y..(map_y + map_h) {
        for px in map_x..(map_x + map_w) {
            let idx = (py * img_w + px) as usize;
            if idx < pixels.len() {
                pixels[idx] = ocean_color;
            }
        }
    }

    let geo = match get_geodata() {
        Some(g) => g,
        None => return,
    };

    // Fill land polygons
    for poly in geo.land_for_zoom(5) {
        fill_polygon(pixels, img_w, map_x, map_y, map_w, map_h, proj, poly, land_color);
    }

    // Fill lakes
    for poly in geo.lakes_for_zoom(5) {
        fill_polygon(pixels, img_w, map_x, map_y, map_w, map_h, proj, poly, ocean_color);
    }
}

/// Render borders on top of everything
fn render_borders(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    proj: &dyn Projection,
) {
    let geo = match get_geodata() {
        Some(g) => g,
        None => return,
    };

    let (min_lat, min_lon, max_lat, max_lon) = proj.bounding_box();
    let margin = 2.0;

    // State boundaries (black, thin)
    for line in &geo.state_borders {
        if !polyline_in_bounds(line, min_lat - margin, min_lon - margin, max_lat + margin, max_lon + margin) { continue; }
        draw_polyline(pixels, img_w, map_x, map_y, map_w, map_h, proj, line, [0, 0, 0, 255]);
    }

    // Coastlines (black)
    for line in geo.coastlines_for_zoom(5) {
        if !polyline_in_bounds(line, min_lat - margin, min_lon - margin, max_lat + margin, max_lon + margin) { continue; }
        draw_polyline(pixels, img_w, map_x, map_y, map_w, map_h, proj, line, [0, 0, 0, 255]);
    }

    // Country borders
    for line in &geo.country_borders {
        if !polyline_in_bounds(line, min_lat - margin, min_lon - margin, max_lat + margin, max_lon + margin) { continue; }
        draw_polyline(pixels, img_w, map_x, map_y, map_w, map_h, proj, line, [0, 0, 0, 255]);
    }
}

fn polyline_in_bounds(line: &[(f64, f64)], min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) -> bool {
    // geodata stores (lon, lat) pairs
    line.iter().any(|&(lon, lat)| {
        lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon
    })
}

// ============================================================
// POLYGON & POLYLINE RENDERING
// ============================================================

/// Fill a geographic polygon on the pixel buffer
/// Coords are (lon, lat) pairs from rustmaps geodata
fn fill_polygon(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    proj: &dyn Projection,
    coords: &[(f64, f64)],
    color: [u8; 4],
) {
    if coords.len() < 3 { return; }

    // Project all coordinates to pixel space
    let mut screen_pts: Vec<(f32, f32)> = Vec::with_capacity(coords.len());
    for &(lon, lat) in coords {
        let (sx, sy) = latlon_to_px(lat, lon, proj, map_w, map_h);
        screen_pts.push((map_x as f32 + sx, map_y as f32 + sy));
    }

    // Find bounding box
    let min_y = screen_pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).max(map_y as f32) as u32;
    let max_y = screen_pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).min((map_y + map_h - 1) as f32) as u32;

    // Scanline fill
    for y in min_y..=max_y {
        let yf = y as f32 + 0.5;
        let mut intersections = Vec::new();
        let n = screen_pts.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let (_, y0) = screen_pts[i];
            let (_, y1) = screen_pts[j];
            if (y0 <= yf && y1 > yf) || (y1 <= yf && y0 > yf) {
                let t = (yf - y0) / (y1 - y0);
                let x = screen_pts[i].0 + t * (screen_pts[j].0 - screen_pts[i].0);
                intersections.push(x);
            }
        }
        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in intersections.chunks(2) {
            if pair.len() == 2 {
                let x_start = (pair[0].max(map_x as f32) as u32).min(map_x + map_w);
                let x_end = (pair[1].min((map_x + map_w) as f32) as u32).min(map_x + map_w);
                for x in x_start..x_end {
                    let idx = (y * img_w + x) as usize;
                    if idx < pixels.len() {
                        pixels[idx] = color;
                    }
                }
            }
        }
    }
}

/// Draw a geographic polyline on the pixel buffer
/// Coords are (lon, lat) pairs from rustmaps geodata
fn draw_polyline(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    proj: &dyn Projection,
    coords: &[(f64, f64)],
    color: [u8; 4],
) {
    if coords.len() < 2 { return; }

    for i in 0..coords.len() - 1 {
        let (lon0, lat0) = coords[i];
        let (lon1, lat1) = coords[i + 1];

        let (sx0, sy0) = latlon_to_px(lat0, lon0, proj, map_w, map_h);
        let (sx1, sy1) = latlon_to_px(lat1, lon1, proj, map_w, map_h);

        draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
            map_x as f32 + sx0, map_y as f32 + sy0,
            map_x as f32 + sx1, map_y as f32 + sy1,
            color);
    }
}

/// Xiaolin Wu's anti-aliased line drawing algorithm
fn draw_line(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    x0: f32, y0: f32, x1: f32, y1: f32,
    color: [u8; 4],
) {
    let blend_px = |pixels: &mut Vec<[u8; 4]>, x: i32, y: i32, brightness: f32| {
        if x < map_x as i32 || x >= (map_x + map_w) as i32
            || y < map_y as i32 || y >= (map_y + map_h) as i32
        { return; }
        let idx = (y as u32 * img_w + x as u32) as usize;
        if idx >= pixels.len() { return; }
        let a = (color[3] as f32 / 255.0) * brightness;
        let bg = pixels[idx];
        pixels[idx] = [
            (color[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8,
            (color[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8,
            (color[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8,
            255,
        ];
    };

    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    let (mut x0, mut y0, mut x1, mut y1) = if steep {
        (y0, x0, y1, x1)
    } else {
        (x0, y0, x1, y1)
    };
    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < 0.001 { 1.0 } else { dy / dx };

    // First endpoint
    let xend = x0.round();
    let yend = y0 + gradient * (xend - x0);
    let xpxl1 = xend as i32;
    let mut intery = yend + gradient;

    // Second endpoint
    let xend2 = x1.round();
    let xpxl2 = xend2 as i32;

    // Main loop
    for x in xpxl1..=xpxl2 {
        let fpart = intery - intery.floor();
        let ipart = intery.floor() as i32;
        if steep {
            blend_px(pixels, ipart, x, 1.0 - fpart);
            blend_px(pixels, ipart + 1, x, fpart);
        } else {
            blend_px(pixels, x, ipart, 1.0 - fpart);
            blend_px(pixels, x, ipart + 1, fpart);
        }
        intery += gradient;
    }
}

// ============================================================
// DATA OVERLAY
// ============================================================

/// Render weather data as filled contours on the map
fn render_data_overlay(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    values: &[f64],
    nx: usize, ny: usize,
    product: &Product,
    _proj: &dyn Projection,
) {
    let vmin = product.contour_min;
    let vmax = product.contour_max;
    let inv_range = if vmax > vmin { 1.0 / (vmax - vmin) } else { 1.0 };

    // 3x3 supersampling offsets within each pixel for anti-aliased contour boundaries
    const SS: usize = 3;
    const SS_COUNT: f32 = (SS * SS) as f32;
    let ss_offsets: Vec<(f32, f32)> = {
        let mut v = Vec::with_capacity(SS * SS);
        for sy in 0..SS {
            for sx in 0..SS {
                v.push((
                    (sx as f32 + 0.5) / SS as f32,
                    (sy as f32 + 0.5) / SS as f32,
                ));
            }
        }
        v
    };

    let nx_f = nx as f32;
    let ny_f = ny as f32;
    let map_w_f = map_w as f32;
    let map_h_f = map_h as f32;
    let nx_lim = (nx - 1) as f32;
    let ny_lim = (ny - 1) as f32;

    for py in 0..map_h {
        for px in 0..map_w {
            // Accumulate color from sub-pixel samples
            let mut r_sum: f32 = 0.0;
            let mut g_sum: f32 = 0.0;
            let mut b_sum: f32 = 0.0;
            let mut a_sum: f32 = 0.0;

            for &(sox, soy) in &ss_offsets {
                let spx = px as f32 + sox;
                let spy = py as f32 + soy;
                let gx = spx * nx_f / map_w_f;
                let gy = (map_h_f - 1.0 - spy) * ny_f / map_h_f;

                if gx < 0.0 || gx >= nx_lim || gy < 0.0 || gy >= ny_lim {
                    continue;
                }

                let ix = gx as usize;
                let iy = gy as usize;
                let fx = (gx - ix as f32) as f64;
                let fy = (gy - iy as f32) as f64;

                let v00 = values[iy * nx + ix];
                let v10 = values[iy * nx + (ix + 1).min(nx - 1)];
                let v01 = values[(iy + 1).min(ny - 1) * nx + ix];
                let v11 = values[(iy + 1).min(ny - 1) * nx + (ix + 1).min(nx - 1)];

                if v00.is_nan() || v10.is_nan() || v01.is_nan() || v11.is_nan() {
                    continue;
                }

                let val = v00 * (1.0 - fx) * (1.0 - fy)
                    + v10 * fx * (1.0 - fy)
                    + v01 * (1.0 - fx) * fy
                    + v11 * fx * fy;

                let t = ((val - vmin) * inv_range).clamp(0.0, 1.0);
                let c = colormap_lookup_f32(product.colormap_id, t);
                if c[3] > 0.0 {
                    r_sum += c[0];
                    g_sum += c[1];
                    b_sum += c[2];
                    a_sum += c[3];
                }
            }

            if a_sum > 0.0 {
                let color = [
                    (r_sum / SS_COUNT) as u8,
                    (g_sum / SS_COUNT) as u8,
                    (b_sum / SS_COUNT) as u8,
                    (a_sum / SS_COUNT) as u8,
                ];
                let idx = ((map_y + py) * img_w + (map_x + px)) as usize;
                if idx < pixels.len() {
                    let a = color[3] as f32 / 255.0;
                    let bg = pixels[idx];
                    pixels[idx] = [
                        (color[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8,
                        (color[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8,
                        (color[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8,
                        255,
                    ];
                }
            }
        }
    }
}

/// Look up a color from a named colormap at normalized position t, returning f32 values
pub fn colormap_lookup_f32(cmap_id: &str, t: f64) -> [f32; 4] {
    let c = colormap_lookup(cmap_id, t);
    [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]
}

/// Look up a color from a named colormap at normalized position t
pub fn colormap_lookup(cmap_id: &str, t: f64) -> [u8; 4] {
    match cmap_id {
        "winds" | "winds_sfc" => colormaps::winds_color(t, 60),
        "temperature_f" => colormaps::temperature_color(t, 180),
        "temperature_c" | "temperature_500" | "temperature_700" => {
            colormaps::temperature_color_cropped(t, 55, -40.0, 70.0)
        }
        "temperature_250" => colormaps::temperature_color_cropped(t, 40, -40.0, 70.0),
        "dewpoint_f" => colormaps::dewpoint_color(t, 80, 50),
        "rh" => colormaps::rh_color(t),
        "relvort" => colormaps::relvort_color(t, 100),
        "sim_ir" => colormaps::sim_ir_color(t),
        "cape" => {
            let cmap = colormaps::cape_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "three_cape" => {
            let cmap = colormaps::three_cape_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "ehi" => {
            let cmap = colormaps::ehi_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "srh" => {
            let cmap = colormaps::srh_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "stp" => {
            let cmap = colormaps::stp_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "lapse_rate" => {
            let cmap = colormaps::lr_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "uh" => {
            let cmap = colormaps::uh_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        "reflectivity" => colormaps::reflectivity_color(t),
        "geopot_anomaly" => colormaps::geopot_anomaly_color(t, 80),
        "precip_in" => colormaps::precip_color_in(t),
        "ml_metric" => {
            let cmap = colormaps::ml_metric_colormap();
            colormaps::lookup_quantized_ext(&cmap, t)
        }
        _ => [128, 128, 128, 255], // gray fallback
    }
}

// ============================================================
// CONTOUR LINES
// ============================================================

/// Render contour lines (for MSLP or geopotential height)
fn render_contour_lines(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    values: &[f64],
    nx: usize, ny: usize,
    interval: f64,
) {
    let color = [0u8, 0, 0, 180]; // dark semi-transparent (thin, subtle like matplotlib)

    // Marching squares on the DATA grid, then project crossing points to pixel space.
    // This produces smooth curved contour lines identical in approach to filled contours.

    // Linear interpolation fraction: where `level` falls between a and b
    let lerp_frac = |a: f64, b: f64, level: f64| -> f32 {
        let denom = b - a;
        if denom.abs() < 1e-12 { 0.5 } else { ((level - a) / denom) as f32 }
    };

    // Convert grid coords to pixel coords (matching px_to_grid inverse)
    let grid_to_px = |gx: f32, gy: f32| -> (f32, f32) {
        let px = gx * map_w as f32 / nx as f32 + map_x as f32;
        let py = map_y as f32 + (map_h as f32 - 1.0) - gy * map_h as f32 / ny as f32;
        (px, py)
    };

    // Marching squares edge table: for each of the 16 cases, which edges to connect.
    // Edges: 0=bottom(01), 1=right(12), 2=top(23), 3=left(30)
    // Corner layout:  3---2
    //                 |   |
    //                 0---1
    // Each entry is a list of (edge_a, edge_b) line segments to draw.
    // Saddle cases (5,10) use center value to disambiguate.
    let edge_point = |edge: u8, v: [f64; 4], level: f64, gx: usize, gy: usize| -> (f32, f32) {
        let (x, y) = match edge {
            0 => { // bottom edge: corner 0 -> corner 1
                let t = lerp_frac(v[0], v[1], level);
                (gx as f32 + t, gy as f32)
            }
            1 => { // right edge: corner 1 -> corner 2
                let t = lerp_frac(v[1], v[2], level);
                (gx as f32 + 1.0, gy as f32 + t)
            }
            2 => { // top edge: corner 3 -> corner 2
                let t = lerp_frac(v[3], v[2], level);
                (gx as f32 + t, gy as f32 + 1.0)
            }
            3 => { // left edge: corner 0 -> corner 3
                let t = lerp_frac(v[0], v[3], level);
                (gx as f32, gy as f32 + t)
            }
            _ => (gx as f32, gy as f32),
        };
        grid_to_px(x, y)
    };

    // Grid-based label tracking: coarse grid to avoid clutter
    // ~8x6 cells = labels spaced ~140x125 pixels apart
    let label_grid_cols: usize = 8;
    let label_grid_rows: usize = 6;
    let cell_w = map_w as f32 / label_grid_cols as f32;
    let cell_h = map_h as f32 / label_grid_rows as f32;
    // Track which cells already have ANY label (regardless of level)
    let mut label_placed: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    // Iterate over grid cells
    for gy in 0..(ny - 1) {
        for gx in 0..(nx - 1) {
            let v00 = values[gy * nx + gx];
            let v10 = values[gy * nx + gx + 1];
            let v11 = values[(gy + 1) * nx + gx + 1];
            let v01 = values[(gy + 1) * nx + gx];

            if v00.is_nan() || v10.is_nan() || v11.is_nan() || v01.is_nan() { continue; }

            let v = [v00, v10, v11, v01]; // corners: BL, BR, TR, TL

            let min_val = v00.min(v10).min(v11).min(v01);
            let max_val = v00.max(v10).max(v11).max(v01);

            // Iterate contour levels that cross this cell
            let first_level = ((min_val / interval).floor() as i64) * interval as i64;
            let mut level = first_level as f64;
            // Safety: limit iterations
            for _ in 0..200 {
                if level > max_val { break; }
                if level >= min_val && level <= max_val {
                    // Marching squares case index
                    let case = ((v[0] >= level) as u8)
                        | (((v[1] >= level) as u8) << 1)
                        | (((v[2] >= level) as u8) << 2)
                        | (((v[3] >= level) as u8) << 3);

                    // Segments to draw for each case (edge pairs)
                    let segments: &[(u8, u8)] = match case {
                        0 | 15 => &[],
                        1 | 14 => &[(0, 3)],
                        2 | 13 => &[(0, 1)],
                        3 | 12 => &[(3, 1)],
                        4 | 11 => &[(1, 2)],
                        6 | 9  => &[(0, 2)],
                        7 | 8  => &[(2, 3)],
                        5 => {
                            // Saddle: disambiguate with center value
                            let center = (v[0] + v[1] + v[2] + v[3]) * 0.25;
                            if center >= level { &[(0, 3), (1, 2)] } else { &[(0, 1), (2, 3)] }
                        }
                        10 => {
                            let center = (v[0] + v[1] + v[2] + v[3]) * 0.25;
                            if center >= level { &[(0, 1), (2, 3)] } else { &[(0, 3), (1, 2)] }
                        }
                        _ => &[],
                    };

                    for &(e0, e1) in segments {
                        let (px0, py0) = edge_point(e0, v, level, gx, gy);
                        let (px1, py1) = edge_point(e1, v, level, gx, gy);

                        draw_line(pixels, img_w, map_x, map_y, map_w, map_h, px0, py0, px1, py1, color);

                        // Check if we should place a label at the midpoint
                        let mid_x = (px0 + px1) * 0.5;
                        let mid_y = (py0 + py1) * 0.5;
                        let rel_x = mid_x - map_x as f32;
                        let rel_y = mid_y - map_y as f32;
                        if rel_x >= 0.0 && rel_x < map_w as f32 && rel_y >= 0.0 && rel_y < map_h as f32 {
                            let cell_col = (rel_x / cell_w) as usize;
                            let cell_row = (rel_y / cell_h) as usize;
                            if cell_col < label_grid_cols && cell_row < label_grid_rows
                                && !label_placed.contains(&(cell_col, cell_row))
                            {
                                // Block this cell and neighbors to ensure spacing
                                label_placed.insert((cell_col, cell_row));
                                if cell_col > 0 { label_placed.insert((cell_col - 1, cell_row)); }
                                if cell_col + 1 < label_grid_cols { label_placed.insert((cell_col + 1, cell_row)); }
                                if cell_row > 0 { label_placed.insert((cell_col, cell_row - 1)); }
                                if cell_row + 1 < label_grid_rows { label_placed.insert((cell_col, cell_row + 1)); }
                                // Format label
                                let label = if level.abs() >= 1.0 {
                                    format!("{}", level as i64)
                                } else {
                                    format!("{:.1}", level)
                                };
                                let label_font_size = 9.0f32;
                                let lw = text_width_px(&label, label_font_size);
                                let lh = label_font_size as u32;
                                let lx = (mid_x - lw as f32 / 2.0) as i32;
                                let ly = (mid_y - lh as f32 / 2.0) as i32;
                                // Draw inline label (no box -- matches matplotlib clabel style)
                                if lx >= 0 && ly >= 0 {
                                    draw_text_ttf(pixels, img_w, lx as u32, ly as u32, &label, [0, 0, 0, 220], label_font_size, true);
                                }
                            }
                        }
                    }
                }
                level += interval;
            }
        }
    }
}

// ============================================================
// WIND BARBS
// ============================================================

/// Fill a triangle with solid color (for wind barb pennants)
fn fill_triangle(
    pixels: &mut Vec<[u8; 4]>, img_w: u32,
    x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    color: [u8; 4],
) {
    let min_x = x0.min(x1).min(x2).floor() as i32;
    let max_x = x0.max(x1).max(x2).ceil() as i32;
    let min_y = y0.min(y1).min(y2).floor() as i32;
    let max_y = y0.max(y1).max(y2).ceil() as i32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            if px < map_x as i32 || px >= (map_x + map_w) as i32
                || py < map_y as i32 || py >= (map_y + map_h) as i32 { continue; }
            let (fpx, fpy) = (px as f32 + 0.5, py as f32 + 0.5);
            let d1 = (fpx - x1) * (y0 - y1) - (x0 - x1) * (fpy - y1);
            let d2 = (fpx - x2) * (y1 - y2) - (x1 - x2) * (fpy - y2);
            let d3 = (fpx - x0) * (y2 - y0) - (x2 - x0) * (fpy - y0);
            let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
            let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
            if !(has_neg && has_pos) {
                let idx = (py as u32 * img_w + px as u32) as usize;
                if idx < pixels.len() { pixels[idx] = color; }
            }
        }
    }
}

/// Render wind barbs on the map
fn render_wind_barbs(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    u_wind: &[f64], v_wind: &[f64],
    nx: usize, ny: usize,
) {
    // Subsample: ~30 barbs across, ~18 down (matching solarpower07's n_lon=30, n_lat=18)
    let skip_x = (nx / 30).max(1);
    let skip_y = (ny / 18).max(1);
    let barb_color = [0u8, 0, 0, 255]; // black
    let barb_len = 12.0f32;

    for j in (0..ny).step_by(skip_y) {
        for i in (0..nx).step_by(skip_x) {
            let idx = j * nx + i;
            let u = u_wind[idx]; // m/s
            let v = v_wind[idx];
            if u.is_nan() || v.is_nan() { continue; }

            // Convert to knots
            let u_kt = u * 1.94384;
            let v_kt = v * 1.94384;
            let speed = (u_kt * u_kt + v_kt * v_kt).sqrt();

            // Project grid point to screen (direct grid-to-pixel mapping)
            let sx = map_x as f32 + i as f32 * map_w as f32 / nx as f32;
            let sy = map_y as f32 + (ny - 1 - j) as f32 * map_h as f32 / ny as f32;

            // Wind direction (meteorological: from direction)
            let dir = (-u_kt).atan2(-v_kt) as f32;

            // Draw barb shaft
            let dx = -dir.sin() * barb_len;
            let dy = dir.cos() * barb_len;
            draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
                     sx, sy, sx + dx, sy + dy, barb_color);

            // Draw flags/barbs based on speed
            if speed < 2.5 {
                // Draw calm wind circle
                let r = 3.0f32;
                for angle_i in 0..32 {
                    let a0 = angle_i as f32 * std::f32::consts::TAU / 32.0;
                    let a1 = (angle_i + 1) as f32 * std::f32::consts::TAU / 32.0;
                    draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
                        sx + r * a0.cos(), sy + r * a0.sin(),
                        sx + r * a1.cos(), sy + r * a1.sin(),
                        barb_color);
                }
                continue;
            }

            let mut remaining = speed;
            let mut pos = 1.0f32; // position along shaft (1.0 = tip)

            // 50-kt flags (filled pennants)
            while remaining >= 47.5 {
                let bx = sx + dx * pos;
                let by = sy + dy * pos;
                let perp_x = dy * 0.3;
                let perp_y = -dx * 0.3;
                let next_pos = pos - 0.15;
                let bx2 = sx + dx * next_pos;
                let by2 = sy + dy * next_pos;
                fill_triangle(pixels, img_w, bx, by, bx + perp_x, by + perp_y, bx2, by2,
                    map_x, map_y, map_w, map_h, barb_color);
                remaining -= 50.0;
                pos -= 0.2;
            }

            // 10-kt long barbs
            while remaining >= 7.5 {
                let bx = sx + dx * pos;
                let by = sy + dy * pos;
                let perp_x = dy * 0.4;
                let perp_y = -dx * 0.4;
                draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
                         bx, by, bx + perp_x, by + perp_y, barb_color);
                remaining -= 10.0;
                pos -= 0.12;
            }

            // 5-kt short barbs
            if remaining >= 2.5 {
                let bx = sx + dx * pos;
                let by = sy + dy * pos;
                let perp_x = dy * 0.2;
                let perp_y = -dx * 0.2;
                draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
                         bx, by, bx + perp_x, by + perp_y, barb_color);
            }
        }
    }
}

// ============================================================
// TITLE & COLORBAR
// ============================================================

/// Render the title bar
fn render_title(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    product: &Product,
    valid_time_str: &str,
    init_time_str: &str,
) {
    let title_text = (product.product_name_fn)(0.0);

    // Title row: product name centered, bold
    let title_size = 16.0;
    let title_w = text_width_px(&title_text, title_size);
    let title_x = ((img_w as i32 - title_w as i32) / 2).max(0) as u32;
    draw_text_ttf(pixels, img_w, title_x, 2, &title_text, [0, 0, 0, 255], title_size, true);

    // Subtitle row: valid_time left, credit center, init_time right
    let sub_size = 9.0;
    let subtitle_y = TITLE_HEIGHT - 2;

    // Left: valid time
    draw_text_ttf(pixels, img_w, 14, subtitle_y, valid_time_str, [40, 40, 40, 255], sub_size, false);

    // Center: credit
    let credit = "RUSTMET";
    let credit_w = text_width_px(credit, sub_size);
    let credit_x = ((img_w as i32 - credit_w as i32) / 2).max(0) as u32;
    draw_text_ttf(pixels, img_w, credit_x, subtitle_y, credit, [40, 40, 40, 255], sub_size, false);

    // Right: init time
    let init_w = text_width_px(init_time_str, sub_size);
    let init_x = (img_w - init_w - 14).max(0);
    draw_text_ttf(pixels, img_w, init_x, subtitle_y, init_time_str, [40, 40, 40, 255], sub_size, false);
}

/// Render the horizontal colorbar at the bottom of the figure
fn render_colorbar(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    img_h: u32,
    product: &Product,
) {
    let cbar_y = img_h - COLORBAR_HEIGHT - COLORBAR_MARGIN - BOTTOM_MARGIN;
    let cbar_width = (img_w as f32 * 0.82) as u32;
    let cbar_x = (img_w - cbar_width) / 2;
    let bar_height = 12u32;

    // Draw discrete banded colorbar with edge lines (matching matplotlib drawedges=True)
    let n_bands = ((product.contour_max - product.contour_min) / product.contour_step).round() as usize;
    let n_bands = n_bands.max(1).min(500);
    let border = [0u8, 0, 0, 255];

    for band in 0..n_bands {
        let t0 = band as f64 / n_bands as f64;
        let t1 = (band + 1) as f64 / n_bands as f64;
        let t_mid = (t0 + t1) * 0.5;
        let color = colormap_lookup(product.colormap_id, t_mid);

        let px_start = (t0 * cbar_width as f64) as u32;
        let px_end = (t1 * cbar_width as f64) as u32;

        // Fill this band
        for px in px_start..px_end {
            for dy in 0..bar_height {
                let idx = ((cbar_y + dy) * img_w + (cbar_x + px)) as usize;
                if idx < pixels.len() { pixels[idx] = color; }
            }
        }

        // Draw thin vertical edge line at band boundary
        let edge_x = cbar_x + px_end;
        if edge_x < cbar_x + cbar_width {
            for dy in 0..bar_height {
                let idx = ((cbar_y + dy) * img_w + edge_x) as usize;
                if idx < pixels.len() { pixels[idx] = border; }
            }
        }
    }

    // Draw border around colorbar
    for px in 0..cbar_width {
        let top_idx = (cbar_y * img_w + cbar_x + px) as usize;
        let bot_idx = ((cbar_y + bar_height - 1) * img_w + cbar_x + px) as usize;
        if top_idx < pixels.len() { pixels[top_idx] = border; }
        if bot_idx < pixels.len() { pixels[bot_idx] = border; }
    }
    for dy in 0..bar_height {
        let left_idx = ((cbar_y + dy) * img_w + cbar_x) as usize;
        let right_idx = ((cbar_y + dy) * img_w + cbar_x + cbar_width - 1) as usize;
        if left_idx < pixels.len() { pixels[left_idx] = border; }
        if right_idx < pixels.len() { pixels[right_idx] = border; }
    }

    // Draw tick marks and labels (on top of the bar)
    let cbar_ticks = if let Some(ref ticks) = product.custom_cbar_ticks {
        ticks.clone()
    } else {
        let mut ticks = Vec::new();
        let mut v = product.cbar_min;
        while v <= product.cbar_max + product.cbar_step * 0.01 {
            ticks.push(v);
            v += product.cbar_step;
        }
        ticks
    };

    let vmin = product.contour_min;
    let vmax = product.contour_max;

    for &tick_val in &cbar_ticks {
        let t = ((tick_val - vmin) / (vmax - vmin)).clamp(0.0, 1.0);
        let tick_x = cbar_x + (t * cbar_width as f64) as u32;

        // Tick mark (3px below bar)
        for dy in 0..3 {
            let idx = ((cbar_y + bar_height + dy) * img_w + tick_x) as usize;
            if idx < pixels.len() { pixels[idx] = border; }
        }

        // Label (below tick)
        let label = if tick_val.abs() < 0.01 && tick_val != 0.0 {
            format!("{:.2}", tick_val)
        } else if tick_val.fract().abs() > 0.001 {
            format!("{:.1}", tick_val)
        } else {
            format!("{}", tick_val as i64)
        };

        let label_w = text_width(&label);
        let label_x = tick_x.saturating_sub(label_w / 2);
        draw_text(pixels, img_w, label_x, cbar_y + bar_height + 4, &label, [0, 0, 0, 255]);
    }
}

// ============================================================
// LAT/LON GRIDLINES
// ============================================================

/// Render subtle gray dashed lat/lon gridlines on the map
#[allow(dead_code)]
fn render_gridlines(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    map_x: u32, map_y: u32, map_w: u32, map_h: u32,
    proj: &dyn Projection,
) {
    let grid_color = [160u8, 160, 160, 80];
    let (min_lat, min_lon, max_lat, max_lon) = proj.bounding_box();

    // Choose gridline spacing
    let lat_span = max_lat - min_lat;
    let lon_span = max_lon - min_lon;
    let lat_spacing = if lat_span < 20.0 { 2.0 } else { 5.0 };
    let lon_spacing = if lon_span < 20.0 { 2.0 } else { 5.0 };

    let n_samples = 200usize;

    // Latitude gridlines
    let first_lat = (min_lat / lat_spacing).ceil() * lat_spacing;
    let mut lat = first_lat;
    while lat <= max_lat {
        let mut prev: Option<(f32, f32)> = None;
        let mut dash_accum = 0.0f32;
        let mut drawing = true;
        for i in 0..=n_samples {
            let lon = min_lon + (max_lon - min_lon) * i as f64 / n_samples as f64;
            let (px, py) = latlon_to_px(lat, lon, proj, map_w, map_h);
            let screen_x = map_x as f32 + px;
            let screen_y = map_y as f32 + py;
            if let Some((prev_x, prev_y)) = prev {
                let seg_len = ((screen_x - prev_x).powi(2) + (screen_y - prev_y).powi(2)).sqrt();
                dash_accum += seg_len;
                if dash_accum > 8.0 {
                    dash_accum = 0.0;
                    drawing = !drawing;
                }
                if drawing {
                    draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
                        prev_x, prev_y, screen_x, screen_y, grid_color);
                }
            }
            prev = Some((screen_x, screen_y));
        }
        lat += lat_spacing;
    }

    // Longitude gridlines
    let first_lon = (min_lon / lon_spacing).ceil() * lon_spacing;
    let mut lon = first_lon;
    while lon <= max_lon {
        let mut prev: Option<(f32, f32)> = None;
        let mut dash_accum = 0.0f32;
        let mut drawing = true;
        for i in 0..=n_samples {
            let lat_pt = min_lat + (max_lat - min_lat) * i as f64 / n_samples as f64;
            let (px, py) = latlon_to_px(lat_pt, lon, proj, map_w, map_h);
            let screen_x = map_x as f32 + px;
            let screen_y = map_y as f32 + py;
            if let Some((prev_x, prev_y)) = prev {
                let seg_len = ((screen_x - prev_x).powi(2) + (screen_y - prev_y).powi(2)).sqrt();
                dash_accum += seg_len;
                if dash_accum > 8.0 {
                    dash_accum = 0.0;
                    drawing = !drawing;
                }
                if drawing {
                    draw_line(pixels, img_w, map_x, map_y, map_w, map_h,
                        prev_x, prev_y, screen_x, screen_y, grid_color);
                }
            }
            prev = Some((screen_x, screen_y));
        }
        lon += lon_spacing;
    }
}

// ============================================================
// TTF TEXT RENDERING (DejaVu Sans via fontdue)
// ============================================================

use fontdue::{Font, FontSettings};

static FONT_REGULAR: OnceLock<Font> = OnceLock::new();
static FONT_BOLD: OnceLock<Font> = OnceLock::new();

fn get_font() -> &'static Font {
    FONT_REGULAR.get_or_init(|| {
        let data = include_bytes!("../fonts/DejaVuSans.ttf");
        Font::from_bytes(data as &[u8], FontSettings::default()).expect("Failed to load font")
    })
}

fn get_font_bold() -> &'static Font {
    FONT_BOLD.get_or_init(|| {
        let data = include_bytes!("../fonts/DejaVuSans-Bold.ttf");
        Font::from_bytes(data as &[u8], FontSettings::default()).expect("Failed to load bold font")
    })
}

fn text_width(s: &str) -> u32 {
    text_width_px(s, 10.0)
}

fn text_width_px(s: &str, size: f32) -> u32 {
    let font = get_font();
    let mut w = 0.0f32;
    for ch in s.chars() {
        let metrics = font.metrics(ch, size);
        w += metrics.advance_width;
    }
    w.ceil() as u32
}

fn draw_text(pixels: &mut Vec<[u8; 4]>, img_w: u32, x: u32, y: u32, text: &str, color: [u8; 4]) {
    draw_text_ttf(pixels, img_w, x, y, text, color, 10.0, false);
}

#[allow(dead_code)]
fn draw_text_scaled(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    x: u32, y: u32,
    text: &str,
    color: [u8; 4],
    scale: u32,
) {
    let size = match scale {
        1 => 10.0,
        2 => 18.0,
        3 => 24.0,
        _ => 10.0 * scale as f32,
    };
    draw_text_ttf(pixels, img_w, x, y, text, color, size, scale >= 2);
}

fn draw_text_ttf(
    pixels: &mut Vec<[u8; 4]>,
    img_w: u32,
    x: u32, y: u32,
    text: &str,
    color: [u8; 4],
    size: f32,
    bold: bool,
) {
    let font = if bold { get_font_bold() } else { get_font() };
    let mut cursor_x = x as f32;

    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);
        if metrics.width == 0 || metrics.height == 0 {
            cursor_x += metrics.advance_width;
            continue;
        }

        // ymin is the offset from the baseline to the top of the glyph
        let glyph_x = cursor_x as i32 + metrics.xmin;
        let glyph_y = y as i32 + (size as i32) - metrics.height as i32 - metrics.ymin;

        for gy in 0..metrics.height {
            for gx in 0..metrics.width {
                let coverage = bitmap[gy * metrics.width + gx];
                if coverage == 0 { continue; }

                let px = glyph_x + gx as i32;
                let py = glyph_y + gy as i32;
                if px < 0 || py < 0 || px >= img_w as i32 { continue; }

                let idx = (py as u32 * img_w + px as u32) as usize;
                if idx >= pixels.len() { continue; }

                let a = (color[3] as f32 / 255.0) * (coverage as f32 / 255.0);
                let bg = pixels[idx];
                pixels[idx] = [
                    (color[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8,
                    (color[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8,
                    (color[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8,
                    255,
                ];
            }
        }

        cursor_x += metrics.advance_width;
    }
}

// ============================================================
// PNG ENCODING
// ============================================================

pub fn encode_png(pixels: &[[u8; 4]], width: u32, height: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("PNG header");
        let flat: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
        writer.write_image_data(&flat).expect("PNG data");
    }
    buf
}

/// Write PNG bytes to a file
pub fn save_png(path: &str, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data)
        .map_err(|e| format!("Failed to write PNG to {}: {}", path, e))
}
