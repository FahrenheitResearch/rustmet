//! Contour line rendering via marching squares for meteorological overlays.
//!
//! Produces transparent PNG tiles with contour lines (isobars, height contours, etc.)
//! suitable for Leaflet overlay on top of filled color tiles or base maps.

use crate::tiles::{CachedField, tile_bounds, mercator_lat, sample_bilinear, TILE_SIZE};
use rustmet_core::render::encode::encode_png;

/// Sample grid resolution for marching squares (higher = smoother lines, more CPU).
const SAMPLE_RES: usize = 512;

/// Default contour settings per variable.
pub fn default_contour_settings(var: &str) -> (f64, (u8, u8, u8), f64) {
    // Returns (interval, color_rgb, line_width)
    match var.to_lowercase().as_str() {
        "height" | "hgt" => (60.0, (255, 255, 255), 2.0),   // 500mb heights: 60m
        "mslp" => (4.0, (255, 255, 255), 2.0),              // MSLP: 4 hPa
        "temperature" | "temp" | "t" => (5.0, (255, 255, 255), 1.5), // 5K
        "dewpoint" | "td" | "dew" => (5.0, (200, 200, 255), 1.5),   // 5K
        "reflectivity" | "refl" | "refc" | "refd" => (10.0, (100, 100, 100), 1.5), // 10 dBZ
        "cape" => (500.0, (255, 255, 255), 1.5),            // 500 J/kg
        "cin" => (50.0, (255, 200, 200), 1.5),              // 50 J/kg
        "rh" | "relative_humidity" => (10.0, (200, 255, 200), 1.5), // 10%
        "wind" | "wind_speed" | "wspd" => (10.0, (255, 255, 255), 1.5), // 10 kt
        "gust" => (10.0, (255, 255, 255), 1.5),             // 10 kt
        "precip" | "precipitation" | "apcp" => (5.0, (200, 200, 255), 1.5), // 5 mm
        "helicity" | "hlcy" | "srh" => (100.0, (255, 255, 255), 1.5), // 100 m2/s2
        "pressure" | "pres" => (4.0, (255, 255, 255), 2.0), // 4 hPa
        "pwat" | "precipitable_water" => (10.0, (200, 200, 255), 1.5), // 10 mm
        "vis" | "visibility" => (1000.0, (255, 255, 200), 1.5), // 1 km
        _ => (10.0, (255, 255, 255), 1.5),
    }
}

/// Render contour lines on a 256x256 transparent tile.
///
/// Returns PNG bytes. The tile is fully transparent except where contour lines are drawn.
pub fn render_contour_tile(
    field: &CachedField,
    z: u32,
    x: u32,
    y: u32,
    interval: f64,
    color: (u8, u8, u8),
    line_width: f64,
) -> Result<Vec<u8>, String> {
    // 1) Sample field values on a SAMPLE_RES x SAMPLE_RES grid
    let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, x, y);
    let grid_is_0_360 =
        field.proj.bounding_box().1 >= 0.0 && field.proj.bounding_box().3 > 180.0;

    let gs = SAMPLE_RES + 1; // grid nodes (one more than cells)
    let mut samples = vec![f64::NAN; gs * gs];

    for sy in 0..gs {
        for sx in 0..gs {
            let tx = sx as f64 / SAMPLE_RES as f64;
            let ty = sy as f64 / SAMPLE_RES as f64;
            let mut lon = lon_min + tx * (lon_max - lon_min);
            let lat = mercator_lat(lat_max, lat_min, ty);
            if grid_is_0_360 && lon < 0.0 {
                lon += 360.0;
            }
            let (gi, gj) = field.proj.latlon_to_grid(lat, lon);
            if let Some(val) = sample_bilinear(&field.values, field.nx, field.ny, gi, gj) {
                if val.is_finite() && val.abs() < 1e15 && val > -900.0 {
                    samples[sy * gs + sx] = val;
                }
            }
        }
    }

    // 2) Determine contour levels from data range
    let (data_min, data_max) = samples.iter().fold((f64::MAX, f64::MIN), |(mn, mx), &v| {
        if v.is_finite() {
            (mn.min(v), mx.max(v))
        } else {
            (mn, mx)
        }
    });

    if !data_min.is_finite() || !data_max.is_finite() || interval <= 0.0 {
        // No valid data in tile, return transparent PNG
        let pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];
        return encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32);
    }

    let first_level = (data_min / interval).floor() * interval;
    let last_level = (data_max / interval).ceil() * interval;
    let num_levels = ((last_level - first_level) / interval).round() as usize + 1;
    // Safety cap: don't generate absurd number of levels
    let num_levels = num_levels.min(500);

    let levels: Vec<f64> = (0..num_levels)
        .map(|i| first_level + i as f64 * interval)
        .filter(|&l| l >= data_min && l <= data_max)
        .collect();

    // 3) Pixel buffer (RGBA, transparent background)
    let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];

    // Scale factor from sample grid to tile pixels
    let scale = TILE_SIZE as f64 / SAMPLE_RES as f64;

    // 4) Marching squares for each contour level
    for &level in &levels {
        march_level(&samples, gs, level, scale, &mut pixels, color, line_width);
    }

    // 5) Draw contour labels for prominent levels
    // Labels every 2nd level to avoid clutter
    for (i, &level) in levels.iter().enumerate() {
        if i % 2 == 0 {
            draw_contour_labels(&samples, gs, level, scale, &mut pixels, color);
        }
    }

    encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32)
}

/// Run marching squares for a single contour level and draw line segments.
fn march_level(
    samples: &[f64],
    gs: usize, // grid size (SAMPLE_RES + 1)
    level: f64,
    scale: f64,
    pixels: &mut [u8],
    color: (u8, u8, u8),
    line_width: f64,
) {
    let cells = gs - 1;
    for cy in 0..cells {
        for cx in 0..cells {
            let v00 = samples[cy * gs + cx];
            let v10 = samples[cy * gs + cx + 1];
            let v01 = samples[(cy + 1) * gs + cx];
            let v11 = samples[(cy + 1) * gs + cx + 1];

            // Skip cells with NaN
            if !v00.is_finite() || !v10.is_finite() || !v01.is_finite() || !v11.is_finite() {
                continue;
            }

            // Classify corners: 1 if >= level, 0 if < level
            let case = ((v00 >= level) as u8)
                | (((v10 >= level) as u8) << 1)
                | (((v01 >= level) as u8) << 2)
                | (((v11 >= level) as u8) << 3);

            // Cases 0 and 15: entirely above or below, no contour
            if case == 0 || case == 15 {
                continue;
            }

            // Interpolation helpers — find where the contour crosses each edge
            // Edge positions in cell-local coordinates (0..1):
            //   top:    (cx, cy)   to (cx+1, cy)
            //   bottom: (cx, cy+1) to (cx+1, cy+1)
            //   left:   (cx, cy)   to (cx, cy+1)
            //   right:  (cx+1, cy) to (cx+1, cy+1)

            let lerp = |a: f64, b: f64| -> f64 {
                if (b - a).abs() < 1e-10 {
                    0.5
                } else {
                    ((level - a) / (b - a)).clamp(0.0, 1.0)
                }
            };

            let top = (cx as f64 + lerp(v00, v10), cy as f64);
            let bottom = (cx as f64 + lerp(v01, v11), (cy + 1) as f64);
            let left = (cx as f64, cy as f64 + lerp(v00, v01));
            let right = ((cx + 1) as f64, cy as f64 + lerp(v10, v11));

            // Marching squares case table.
            // Corner bits: v00=bit0(TL), v10=bit1(TR), v01=bit2(BL), v11=bit3(BR)
            // Edges: top(TL-TR), bottom(BL-BR), left(TL-BL), right(TR-BR)
            //
            // Each case lists which edges the contour crosses between.
            let segments: Vec<((f64, f64), (f64, f64))> = match case {
                // Single corner cases
                1  => vec![(top, left)],          // TL above
                2  => vec![(top, right)],         // TR above
                4  => vec![(left, bottom)],       // BL above
                8  => vec![(right, bottom)],      // BR above
                // Complement cases (3 corners above = 1 corner below)
                14 => vec![(top, left)],          // ~1: only TL below
                13 => vec![(top, right)],         // ~2: only TR below
                11 => vec![(left, bottom)],       // ~4: only BL below
                7  => vec![(right, bottom)],      // ~8: only BR below
                // Two adjacent corners on same side
                3  => vec![(left, right)],        // TL+TR above (top row)
                12 => vec![(left, right)],        // BL+BR above (bottom row)
                5  => vec![(top, bottom)],        // TL+BL above (left column)
                10 => vec![(top, bottom)],        // TR+BR above (right column)
                // Saddle points (two diagonal corners)
                6 => {
                    // TR+BL above (diagonal)
                    let center = (v00 + v10 + v01 + v11) / 4.0;
                    if center >= level {
                        vec![(top, left), (right, bottom)]
                    } else {
                        vec![(top, right), (left, bottom)]
                    }
                }
                9 => {
                    // TL+BR above (diagonal)
                    let center = (v00 + v10 + v01 + v11) / 4.0;
                    if center >= level {
                        vec![(top, right), (left, bottom)]
                    } else {
                        vec![(top, left), (right, bottom)]
                    }
                }
                _ => continue,
            };

            // Draw each segment, scaled to tile pixel coordinates
            for (p0, p1) in segments {
                let x0 = p0.0 * scale;
                let y0 = p0.1 * scale;
                let x1 = p1.0 * scale;
                let y1 = p1.1 * scale;
                draw_line_aa(pixels, x0, y0, x1, y1, color, line_width);
            }
        }
    }
}

/// Draw contour labels (value text) at sparse positions along the contour.
fn draw_contour_labels(
    samples: &[f64],
    gs: usize,
    level: f64,
    scale: f64,
    pixels: &mut [u8],
    color: (u8, u8, u8),
) {
    // Find a few good label positions by scanning for contour crossings at
    // regular intervals. We label at most 3 positions per tile.
    let cells = gs - 1;
    let step = cells / 4; // Check every quarter
    if step == 0 {
        return;
    }

    let mut label_count = 0;
    let label_text = format_contour_label(level);

    for cy in (step / 2..cells).step_by(step.max(1)) {
        for cx in (step / 2..cells).step_by(step.max(1)) {
            if label_count >= 3 {
                return;
            }

            let v00 = samples[cy * gs + cx];
            let v10 = samples[cy * gs + cx + 1];

            if !v00.is_finite() || !v10.is_finite() {
                continue;
            }

            // Check if contour crosses this top edge
            if (v00 < level) != (v10 < level) {
                let t = if (v10 - v00).abs() < 1e-10 {
                    0.5
                } else {
                    ((level - v00) / (v10 - v00)).clamp(0.0, 1.0)
                };
                let px = ((cx as f64 + t) * scale) as i32;
                let py = (cy as f64 * scale) as i32;

                // Draw simple bitmap text
                draw_text(pixels, px, py, &label_text, color);
                label_count += 1;
            }
        }
    }
}

/// Format a contour level as a compact label.
fn format_contour_label(level: f64) -> String {
    if level.abs() >= 1000.0 {
        // For heights like 5400m, show as integer
        format!("{}", level as i64)
    } else if level.fract().abs() < 0.01 {
        format!("{}", level as i64)
    } else {
        format!("{:.1}", level)
    }
}

/// Wu's anti-aliased line algorithm with configurable width.
fn draw_line_aa(
    pixels: &mut [u8],
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    color: (u8, u8, u8),
    width: f64,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return;
    }

    // Number of steps for line drawing
    let steps = (len * 2.0).ceil() as usize;
    if steps == 0 {
        return;
    }

    let half_w = width / 2.0;

    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        let cx = x0 + t * dx;
        let cy = y0 + t * dy;

        // For width > 1, stamp a small circle at each point
        let r = half_w.ceil() as i32;
        for oy in -r..=r {
            for ox in -r..=r {
                let px = (cx + ox as f64) as i32;
                let py = (cy + oy as f64) as i32;

                if px < 0 || py < 0 || px >= TILE_SIZE as i32 || py >= TILE_SIZE as i32 {
                    continue;
                }

                // Distance from center
                let dist = ((ox as f64 - (cx - cx.floor())) * (ox as f64 - (cx - cx.floor()))
                    + (oy as f64 - (cy - cy.floor())) * (oy as f64 - (cy - cy.floor())))
                    .sqrt();

                if dist > half_w + 0.5 {
                    continue;
                }

                // Anti-alias: fade at the edge
                let alpha = if dist < half_w - 0.5 {
                    1.0
                } else {
                    (half_w + 0.5 - dist).clamp(0.0, 1.0)
                };

                let a = (alpha * 220.0) as u8;
                if a == 0 {
                    continue;
                }

                let idx = (py as usize * TILE_SIZE + px as usize) * 4;
                // Alpha-composite over existing pixel
                let existing_a = pixels[idx + 3] as u16;
                let new_a = a as u16;
                let out_a = new_a + existing_a * (255 - new_a) / 255;
                if out_a > 0 {
                    pixels[idx] = ((color.0 as u16 * new_a
                        + pixels[idx] as u16 * existing_a * (255 - new_a) / 255)
                        / out_a) as u8;
                    pixels[idx + 1] = ((color.1 as u16 * new_a
                        + pixels[idx + 1] as u16 * existing_a * (255 - new_a) / 255)
                        / out_a) as u8;
                    pixels[idx + 2] = ((color.2 as u16 * new_a
                        + pixels[idx + 2] as u16 * existing_a * (255 - new_a) / 255)
                        / out_a) as u8;
                    pixels[idx + 3] = out_a.min(255) as u8;
                }
            }
        }
    }
}

/// Draw simple 5x7 bitmap text on the pixel buffer.
/// Each character is defined as a 5-wide x 7-tall bitmask.
fn draw_text(
    pixels: &mut [u8],
    x: i32,
    y: i32,
    text: &str,
    color: (u8, u8, u8),
) {
    // Offset so text is centered roughly on the point
    let start_x = x - (text.len() as i32 * 3);
    let start_y = y - 3;

    for (ci, ch) in text.chars().enumerate() {
        let glyph = char_glyph(ch);
        let ox = start_x + ci as i32 * 6;
        for row in 0..7i32 {
            for col in 0..5i32 {
                if glyph[row as usize] & (1 << (4 - col)) != 0 {
                    let px = ox + col;
                    let py = start_y + row;
                    if px >= 0
                        && py >= 0
                        && px < TILE_SIZE as i32
                        && py < TILE_SIZE as i32
                    {
                        // Draw with a dark outline for readability
                        // First pass: outline (black)
                        for dy in -1i32..=1 {
                            for dx in -1i32..=1 {
                                if dx == 0 && dy == 0 {
                                    continue;
                                }
                                let ox2 = px + dx;
                                let oy2 = py + dy;
                                if ox2 >= 0
                                    && oy2 >= 0
                                    && ox2 < TILE_SIZE as i32
                                    && oy2 < TILE_SIZE as i32
                                {
                                    let idx = (oy2 as usize * TILE_SIZE + ox2 as usize) * 4;
                                    if pixels[idx + 3] < 160 {
                                        pixels[idx] = 0;
                                        pixels[idx + 1] = 0;
                                        pixels[idx + 2] = 0;
                                        pixels[idx + 3] = 180;
                                    }
                                }
                            }
                        }
                        // Foreground
                        let idx = (py as usize * TILE_SIZE + px as usize) * 4;
                        pixels[idx] = color.0;
                        pixels[idx + 1] = color.1;
                        pixels[idx + 2] = color.2;
                        pixels[idx + 3] = 240;
                    }
                }
            }
        }
    }
}

/// 5x7 bitmap font glyphs for digits, minus sign, and period.
fn char_glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [
            0b01110,
            0b10001,
            0b10011,
            0b10101,
            0b11001,
            0b10001,
            0b01110,
        ],
        '1' => [
            0b00100,
            0b01100,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
            0b01110,
        ],
        '2' => [
            0b01110,
            0b10001,
            0b00001,
            0b00110,
            0b01000,
            0b10000,
            0b11111,
        ],
        '3' => [
            0b01110,
            0b10001,
            0b00001,
            0b00110,
            0b00001,
            0b10001,
            0b01110,
        ],
        '4' => [
            0b00010,
            0b00110,
            0b01010,
            0b10010,
            0b11111,
            0b00010,
            0b00010,
        ],
        '5' => [
            0b11111,
            0b10000,
            0b11110,
            0b00001,
            0b00001,
            0b10001,
            0b01110,
        ],
        '6' => [
            0b01110,
            0b10000,
            0b11110,
            0b10001,
            0b10001,
            0b10001,
            0b01110,
        ],
        '7' => [
            0b11111,
            0b00001,
            0b00010,
            0b00100,
            0b01000,
            0b01000,
            0b01000,
        ],
        '8' => [
            0b01110,
            0b10001,
            0b10001,
            0b01110,
            0b10001,
            0b10001,
            0b01110,
        ],
        '9' => [
            0b01110,
            0b10001,
            0b10001,
            0b01111,
            0b00001,
            0b00001,
            0b01110,
        ],
        '-' => [
            0b00000,
            0b00000,
            0b00000,
            0b11111,
            0b00000,
            0b00000,
            0b00000,
        ],
        '.' => [
            0b00000,
            0b00000,
            0b00000,
            0b00000,
            0b00000,
            0b01100,
            0b01100,
        ],
        _ => [0; 7], // space / unknown
    }
}
