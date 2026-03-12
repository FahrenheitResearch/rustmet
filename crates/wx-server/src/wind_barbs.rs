//! Wind barb tile overlay renderer.
//!
//! Renders standard meteorological wind barbs onto a transparent PNG tile.
//! Barbs follow WMO convention: staff points INTO the wind direction.
//!
//! Speed decomposition (knots):
//!   - 50 kt = filled triangle (flag/pennant)
//!   - 10 kt = full barb (long line)
//!   -  5 kt = half barb (short line)
//!   - 1-2 kt = staff only (no barbs)
//!   - calm   = small circle

use rustmet_core::render::encode::encode_png;

use crate::tiles::{
    ensure_field_run, mercator_lat, sample_bilinear, tile_bounds, CachedField, FieldCache, TILE_SIZE,
};

// ── Barb geometry constants ──────────────────────────────────────

const STAFF_LEN: f64 = 25.0;
const FULL_BARB_LEN: f64 = 10.0;
const HALF_BARB_LEN: f64 = 5.0;
const BARB_ANGLE_DEG: f64 = 60.0;
const BARB_SPACING: f64 = 5.0; // pixels between barbs along the staff
const FLAG_WIDTH: f64 = 5.0; // pixels along the staff for a flag triangle
const CALM_RADIUS: f64 = 4.0;

// m/s to knots
const MS_TO_KT: f64 = 1.94384;

// ── Pixel buffer drawing primitives ──────────────────────────────

/// Set a pixel in the RGBA buffer with alpha-over compositing.
#[inline]
fn set_pixel(pixels: &mut [u8], x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if x < 0 || y < 0 || x >= TILE_SIZE as i32 || y >= TILE_SIZE as i32 {
        return;
    }
    let idx = (y as usize * TILE_SIZE + x as usize) * 4;
    let src_a = a as f32 / 255.0;
    let dst_a = pixels[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a < 0.001 {
        return;
    }
    pixels[idx] =
        ((r as f32 * src_a + pixels[idx] as f32 * dst_a * (1.0 - src_a)) / out_a) as u8;
    pixels[idx + 1] =
        ((g as f32 * src_a + pixels[idx + 1] as f32 * dst_a * (1.0 - src_a)) / out_a) as u8;
    pixels[idx + 2] =
        ((b as f32 * src_a + pixels[idx + 2] as f32 * dst_a * (1.0 - src_a)) / out_a) as u8;
    pixels[idx + 3] = (out_a * 255.0) as u8;
}

/// Bresenham line drawing.
fn bresenham(
    pixels: &mut [u8],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    let mut cx = x0;
    let mut cy = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        set_pixel(pixels, cx, cy, r, g, b, a);
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}

/// Draw a thick line by drawing parallel Bresenham lines offset perpendicular.
fn draw_line(
    pixels: &mut [u8],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    thickness: i32,
) {
    let half = thickness / 2;
    let dx = (x1 - x0) as f64;
    let dy = (y1 - y0) as f64;
    let len = (dx * dx + dy * dy).sqrt();
    for offset in -half..=half {
        let (ox, oy) = if len > 0.001 {
            let px = -dy / len;
            let py = dx / len;
            (
                (px * offset as f64).round() as i32,
                (py * offset as f64).round() as i32,
            )
        } else {
            (offset, 0)
        };
        bresenham(pixels, x0 + ox, y0 + oy, x1 + ox, y1 + oy, r, g, b, a);
    }
}

/// Draw a filled circle (for calm wind).
fn draw_circle_filled(
    pixels: &mut [u8],
    cx: i32,
    cy: i32,
    radius: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                set_pixel(pixels, cx + dx, cy + dy, r, g, b, a);
            }
        }
    }
}

/// Draw a circle outline.
fn draw_circle_outline(
    pixels: &mut [u8],
    cx: i32,
    cy: i32,
    radius: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    thickness: i32,
) {
    let r_outer = radius + thickness / 2;
    let r_inner = (radius - thickness / 2).max(0);
    for dy in -r_outer..=r_outer {
        for dx in -r_outer..=r_outer {
            let d2 = dx * dx + dy * dy;
            if d2 <= r_outer * r_outer && d2 >= r_inner * r_inner {
                set_pixel(pixels, cx + dx, cy + dy, r, g, b, a);
            }
        }
    }
}

/// Fill a triangle using barycentric coordinates.
fn fill_triangle(
    pixels: &mut [u8],
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    let min_x = x0.min(x1).min(x2).floor() as i32;
    let max_x = x0.max(x1).max(x2).ceil() as i32;
    let min_y = y0.min(y1).min(y2).floor() as i32;
    let max_y = y0.max(y1).max(y2).ceil() as i32;

    let d = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
    if d.abs() < 0.001 {
        return;
    }

    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let pxf = px as f64 + 0.5;
            let pyf = py as f64 + 0.5;
            let wa = ((y1 - y2) * (pxf - x2) + (x2 - x1) * (pyf - y2)) / d;
            let wb = ((y2 - y0) * (pxf - x2) + (x0 - x2) * (pyf - y2)) / d;
            let wc = 1.0 - wa - wb;
            if wa >= -0.01 && wb >= -0.01 && wc >= -0.01 {
                set_pixel(pixels, px, py, r, g, b, a);
            }
        }
    }
}

// ── Wind barb glyph rendering ────────────────────────────────────

/// Draw a single wind barb at pixel position (cx, cy).
///
/// `speed_kt`: wind speed in knots.
/// `dir_rad`: meteorological direction in radians (0=N, pi/2=E, pi=S, 3pi/2=W).
///            This is the direction the wind comes FROM.
fn draw_wind_barb(pixels: &mut [u8], cx: f64, cy: f64, speed_kt: f64, dir_rad: f64) {
    // Calm wind: circle
    if speed_kt < 1.0 {
        draw_circle_outline(pixels, cx as i32, cy as i32, CALM_RADIUS as i32, 0, 0, 0, 220, 2);
        draw_circle_filled(
            pixels,
            cx as i32,
            cy as i32,
            (CALM_RADIUS - 1.0) as i32,
            255,
            255,
            255,
            220,
        );
        return;
    }

    // Decompose speed: round to nearest 5 kt
    let rounded = (((speed_kt + 2.5) as i32) / 5) * 5;
    let flags = rounded / 50;
    let full_barbs = (rounded % 50) / 10;
    let half_barbs = ((rounded % 50) % 10) / 5;

    // Staff direction in screen coordinates.
    // Meteorological direction = where wind comes FROM.
    // Staff points from barb center toward the "from" direction.
    // Screen: north = -Y, east = +X.
    // So staff_dx = sin(dir), staff_dy = -cos(dir).
    let sin_a = dir_rad.sin();
    let cos_a = dir_rad.cos();

    let staff_dx = STAFF_LEN * sin_a;
    let staff_dy = -STAFF_LEN * cos_a;

    let staff_end_x = cx + staff_dx;
    let staff_end_y = cy + staff_dy;

    // Draw staff: black outline (3px) then white core (1px)
    draw_line(
        pixels,
        cx as i32,
        cy as i32,
        staff_end_x as i32,
        staff_end_y as i32,
        0, 0, 0, 220,
        3,
    );
    draw_line(
        pixels,
        cx as i32,
        cy as i32,
        staff_end_x as i32,
        staff_end_y as i32,
        255, 255, 255, 255,
        1,
    );

    // Unit vector along staff (center -> tip)
    let su_x = sin_a;
    let su_y = -cos_a;

    // Perpendicular to left when looking from center to tip
    let perp_x = -su_y; // = cos_a
    let perp_y = su_x; // = sin_a

    // Barb direction: 60 degrees from staff toward left-perpendicular
    let barb_angle_rad = BARB_ANGLE_DEG * std::f64::consts::PI / 180.0;
    let bd_x = -su_x * barb_angle_rad.cos() + perp_x * barb_angle_rad.sin();
    let bd_y = -su_y * barb_angle_rad.cos() + perp_y * barb_angle_rad.sin();

    // Position along staff from tip toward center
    let mut pos = 0.0_f64;

    // Flags (50 kt pennants) -- filled triangles
    for _ in 0..flags {
        let bx = staff_end_x - su_x * pos;
        let by = staff_end_y - su_y * pos;

        let tip_x = bx + perp_x * FULL_BARB_LEN;
        let tip_y = by + perp_y * FULL_BARB_LEN;

        let nx = staff_end_x - su_x * (pos + FLAG_WIDTH);
        let ny = staff_end_y - su_y * (pos + FLAG_WIDTH);

        // Filled black triangle with white edge lines
        fill_triangle(pixels, bx, by, tip_x, tip_y, nx, ny, 0, 0, 0, 220);
        draw_line(pixels, bx as i32, by as i32, tip_x as i32, tip_y as i32, 255, 255, 255, 200, 1);
        draw_line(
            pixels,
            tip_x as i32,
            tip_y as i32,
            nx as i32,
            ny as i32,
            255, 255, 255, 200,
            1,
        );

        pos += FLAG_WIDTH;
    }

    if flags > 0 {
        pos += BARB_SPACING * 0.5;
    }

    // Full barbs (10 kt)
    for _ in 0..full_barbs {
        let bx = staff_end_x - su_x * pos;
        let by = staff_end_y - su_y * pos;
        let tip_x = bx + bd_x * FULL_BARB_LEN;
        let tip_y = by + bd_y * FULL_BARB_LEN;

        draw_line(pixels, bx as i32, by as i32, tip_x as i32, tip_y as i32, 0, 0, 0, 220, 3);
        draw_line(
            pixels,
            bx as i32,
            by as i32,
            tip_x as i32,
            tip_y as i32,
            255, 255, 255, 255,
            1,
        );
        pos += BARB_SPACING;
    }

    // Half barbs (5 kt)
    for i in 0..half_barbs {
        // If this is the only feature, offset from tip to avoid ambiguity
        if flags == 0 && full_barbs == 0 && i == 0 {
            pos += BARB_SPACING;
        }

        let bx = staff_end_x - su_x * pos;
        let by = staff_end_y - su_y * pos;
        let tip_x = bx + bd_x * HALF_BARB_LEN;
        let tip_y = by + bd_y * HALF_BARB_LEN;

        draw_line(pixels, bx as i32, by as i32, tip_x as i32, tip_y as i32, 0, 0, 0, 220, 3);
        draw_line(
            pixels,
            bx as i32,
            by as i32,
            tip_x as i32,
            tip_y as i32,
            255, 255, 255, 255,
            1,
        );
        pos += BARB_SPACING;
    }
}

// ── Tile rendering ───────────────────────────────────────────────

/// Barb spacing in pixels for a given zoom level.
fn barb_spacing_for_zoom(z: u32) -> usize {
    match z {
        0..=2 => 64,
        3 => 56,
        4 => 48,
        5 => 40,
        6 => 36,
        7 => 32,
        8 => 28,
        _ => 24,
    }
}

/// Render wind barb overlay from U/V fields. Returns raw RGBA pixels.
fn render_wind_barb_pixels(
    u_field: &CachedField,
    v_field: &CachedField,
    z: u32,
    x: u32,
    y: u32,
) -> Vec<u8> {
    let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, x, y);
    let grid_is_0_360 =
        u_field.proj.bounding_box().1 >= 0.0 && u_field.proj.bounding_box().3 > 180.0;

    let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];
    let spacing = barb_spacing_for_zoom(z);
    let offset = spacing / 2;

    let mut py = offset;
    while py < TILE_SIZE {
        let mut px = offset;
        while px < TILE_SIZE {
            let tx = (px as f64 + 0.5) / TILE_SIZE as f64;
            let ty = (py as f64 + 0.5) / TILE_SIZE as f64;
            let mut lon = lon_min + tx * (lon_max - lon_min);
            let lat = mercator_lat(lat_max, lat_min, ty);

            if grid_is_0_360 && lon < 0.0 {
                lon += 360.0;
            }

            let (gi, gj) = u_field.proj.latlon_to_grid(lat, lon);

            let u_val = sample_bilinear(&u_field.values, u_field.nx, u_field.ny, gi, gj);
            let v_val = sample_bilinear(&v_field.values, v_field.nx, v_field.ny, gi, gj);

            if let (Some(u), Some(v)) = (u_val, v_val) {
                if u.is_finite() && v.is_finite() && u.abs() < 1e10 && v.abs() < 1e10 {
                    let speed_ms = (u * u + v * v).sqrt();
                    let speed_kt = speed_ms * MS_TO_KT;
                    // Meteorological direction (FROM): atan2(-u, -v)
                    let dir_rad = (-u).atan2(-v);
                    draw_wind_barb(&mut pixels, px as f64, py as f64, speed_kt, dir_rad);
                }
            }

            px += spacing;
        }
        py += spacing;
    }

    pixels
}

// ── Public API ───────────────────────────────────────────────────

/// Generate a wind barb overlay tile as a transparent PNG.
///
/// Downloads both UGRD and VGRD fields via the shared FieldCache,
/// then renders wind barbs spaced evenly across the tile.
pub async fn generate_wind_barb_tile(
    field_cache: &FieldCache,
    model: &str,
    level: &str,
    fhour: u32,
    z: u32,
    x: u32,
    y: u32,
    run: Option<&str>,
) -> Result<Vec<u8>, String> {
    // Download/cache both wind component fields
    let u_field = ensure_field_run(field_cache, model, "ugrd", level, fhour, run).await?;
    let v_field = ensure_field_run(field_cache, model, "vgrd", level, fhour, run).await?;

    let png_bytes = tokio::task::spawn_blocking(move || {
        let pixels = render_wind_barb_pixels(&u_field, &v_field, z, x, y);
        encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("PNG encode error: {}", e))?;

    Ok(png_bytes)
}
