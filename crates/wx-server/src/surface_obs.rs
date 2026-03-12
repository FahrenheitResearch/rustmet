//! Surface observation (METAR) tile overlay renderer.
//!
//! Fetches bulk METARs from aviationweather.gov, caches them, and renders
//! station model plots on 256x256 transparent PNG tiles.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use rustmet_core::render::encode::encode_png;
use wx_obs::stations::{Station, STATIONS};
use wx_obs::metar::{SkyCoverage, Intensity};

const TILE_SIZE: usize = 256;
const METAR_TTL: Duration = Duration::from_secs(300); // 5-minute cache

// URL for bulk US METARs as JSON from Aviation Weather Center
const METAR_BULK_URL: &str =
    "https://aviationweather.gov/api/data/metar?ids=~us&format=json&hours=1";

// ── METAR observation struct (parsed from AWC JSON) ───────────────────

#[derive(Debug, Clone)]
pub struct SurfaceObs {
    pub station_id: String,
    pub lat: f64,
    pub lon: f64,
    pub temp_c: Option<f64>,
    pub dewpoint_c: Option<f64>,
    pub wind_dir: Option<u16>,
    pub wind_speed_kt: Option<u16>,
    pub wind_gust_kt: Option<u16>,
    pub visibility_sm: Option<f64>,
    pub altimeter_inhg: Option<f64>,
    pub wx_string: Option<String>,
    pub sky_cover: SkyCoverType,
    pub ceiling_ft: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkyCoverType {
    Clear,
    Few,
    Scattered,
    Broken,
    Overcast,
    Obscured,
}

// ── METAR cache ───────────────────────────────────────────────────────

struct CachedMetars {
    obs: Vec<SurfaceObs>,
    fetched_at: Instant,
}

pub struct MetarCache {
    data: RwLock<Option<CachedMetars>>,
}

impl MetarCache {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(None),
        }
    }

    pub async fn get_obs(&self) -> Result<Vec<SurfaceObs>, String> {
        // Check cache first
        {
            let guard = self.data.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.fetched_at.elapsed() < METAR_TTL {
                    return Ok(cached.obs.clone());
                }
            }
        }

        // Cache miss or expired - fetch new data
        let obs = tokio::task::spawn_blocking(|| fetch_bulk_metars())
            .await
            .map_err(|e| format!("Task join error: {}", e))??;

        let mut guard = self.data.write().await;
        *guard = Some(CachedMetars {
            obs: obs.clone(),
            fetched_at: Instant::now(),
        });

        Ok(obs)
    }
}

// ── Fetch and parse bulk METARs from AWC JSON API ─────────────────────

fn fetch_bulk_metars() -> Result<Vec<SurfaceObs>, String> {
    eprintln!("[surface] Fetching bulk METARs from aviationweather.gov...");

    let response = ureq::get(METAR_BULK_URL)
        .call()
        .map_err(|e| format!("METAR fetch failed: {}", e))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read METAR response: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("METAR JSON parse error: {}", e))?;

    let arr = json
        .as_array()
        .ok_or_else(|| "METAR response is not an array".to_string())?;

    let mut obs_list = Vec::with_capacity(arr.len());

    for item in arr {
        let station_id = match item.get("icaoId").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let lat = item.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let lon = item.get("lon").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if lat == 0.0 && lon == 0.0 {
            continue;
        }

        let temp_c = item.get("temp").and_then(|v| v.as_f64());
        let dewpoint_c = item.get("dewp").and_then(|v| v.as_f64());

        let wind_dir = item
            .get("wdir")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .map(|v| v as u16);
        let wind_speed_kt = item
            .get("wspd")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .map(|v| v as u16);
        let wind_gust_kt = item
            .get("wgst")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .map(|v| v as u16);

        let visibility_sm = item.get("visib").and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        });

        let altimeter_inhg = item.get("altim").and_then(|v| v.as_f64());

        let wx_string = item
            .get("wxString")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Parse sky cover from the clouds array
        let (sky_cover, ceiling_ft) = parse_sky_cover(item);

        obs_list.push(SurfaceObs {
            station_id,
            lat,
            lon,
            temp_c,
            dewpoint_c,
            wind_dir,
            wind_speed_kt,
            wind_gust_kt,
            visibility_sm,
            altimeter_inhg,
            wx_string,
            sky_cover,
            ceiling_ft,
        });
    }

    eprintln!("[surface] Parsed {} METARs", obs_list.len());
    Ok(obs_list)
}

fn parse_sky_cover(item: &serde_json::Value) -> (SkyCoverType, Option<u32>) {
    let clouds = match item.get("clouds").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return (SkyCoverType::Clear, None),
    };

    if clouds.is_empty() {
        return (SkyCoverType::Clear, None);
    }

    let mut worst = SkyCoverType::Clear;
    let mut ceiling: Option<u32> = None;

    for cloud in clouds {
        let cover = cloud.get("cover").and_then(|v| v.as_str()).unwrap_or("");
        let base = cloud
            .get("base")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .map(|v| v as u32);

        let sc = match cover {
            "CLR" | "SKC" | "CAVOK" => SkyCoverType::Clear,
            "FEW" => SkyCoverType::Few,
            "SCT" => SkyCoverType::Scattered,
            "BKN" => SkyCoverType::Broken,
            "OVC" => SkyCoverType::Overcast,
            "VV" | "OVX" => SkyCoverType::Obscured,
            _ => continue,
        };

        // Track worst coverage
        if (sc as u8) > (worst as u8) {
            worst = sc;
        }

        // Ceiling = lowest BKN/OVC/VV layer
        if matches!(
            sc,
            SkyCoverType::Broken | SkyCoverType::Overcast | SkyCoverType::Obscured
        ) {
            if let Some(b) = base {
                ceiling = Some(ceiling.map_or(b, |c: u32| c.min(b)));
            }
        }
    }

    (worst, ceiling)
}

// ── Major airport filter (for low zoom) ───────────────────────────────

const MAJOR_AIRPORTS: &[&str] = &[
    "KATL", "KBOS", "KBWI", "KCLE", "KCLT", "KCVG", "KDAL", "KDCA", "KDEN", "KDFW", "KDTW",
    "KEWR", "KFLL", "KHOU", "KIAD", "KIAH", "KJFK", "KLAS", "KLAX", "KLGA", "KMCI", "KMCO",
    "KMDW", "KMEM", "KMIA", "KMKE", "KMIN", "KMSP", "KMSY", "KOAK", "KOKC", "KOMA", "KORD",
    "KPBI", "KPDX", "KPHL", "KPHX", "KPIT", "KRDU", "KRNO", "KSAN", "KSAT", "KSDF", "KSEA",
    "KSFO", "KSJC", "KSLC", "KSMF", "KSTL", "KTPA", "KTUL", "KTUS", "PANC", "PAFA", "PHNL",
];

fn is_major_airport(id: &str) -> bool {
    MAJOR_AIRPORTS.contains(&id)
}

// ── Tile math (duplicated from tiles.rs for independence) ─────────────

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

/// Convert lat/lon to pixel coordinates within a tile.
fn latlon_to_pixel(
    lat: f64,
    lon: f64,
    lat_min: f64,
    lon_min: f64,
    lat_max: f64,
    lon_max: f64,
) -> (i32, i32) {
    // X is linear in longitude
    let px = ((lon - lon_min) / (lon_max - lon_min) * TILE_SIZE as f64) as i32;

    // Y uses Mercator projection
    let y_max = lat_max.to_radians().tan().asinh();
    let y_min = lat_min.to_radians().tan().asinh();
    let y_pt = lat.to_radians().tan().asinh();

    let py = ((y_max - y_pt) / (y_max - y_min) * TILE_SIZE as f64) as i32;

    (px, py)
}

// ══════════════════════════════════════════════════════════════════════
// Bitmap font — 5x7 pixel glyphs for digits, letters, minus, period
// ══════════════════════════════════════════════════════════════════════

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// Each glyph is 7 rows of 5 bits, stored as u8 per row (MSB = leftmost pixel).
/// bit layout: 0bABCDE_000 where A is leftmost column.
const FONT: &[(char, [u8; 7])] = &[
    (
        '0',
        [
            0b01110_000,
            0b10001_000,
            0b10011_000,
            0b10101_000,
            0b11001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        '1',
        [
            0b00100_000,
            0b01100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b01110_000,
        ],
    ),
    (
        '2',
        [
            0b01110_000,
            0b10001_000,
            0b00001_000,
            0b00110_000,
            0b01000_000,
            0b10000_000,
            0b11111_000,
        ],
    ),
    (
        '3',
        [
            0b01110_000,
            0b10001_000,
            0b00001_000,
            0b00110_000,
            0b00001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        '4',
        [
            0b00010_000,
            0b00110_000,
            0b01010_000,
            0b10010_000,
            0b11111_000,
            0b00010_000,
            0b00010_000,
        ],
    ),
    (
        '5',
        [
            0b11111_000,
            0b10000_000,
            0b11110_000,
            0b00001_000,
            0b00001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        '6',
        [
            0b00110_000,
            0b01000_000,
            0b10000_000,
            0b11110_000,
            0b10001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        '7',
        [
            0b11111_000,
            0b00001_000,
            0b00010_000,
            0b00100_000,
            0b01000_000,
            0b01000_000,
            0b01000_000,
        ],
    ),
    (
        '8',
        [
            0b01110_000,
            0b10001_000,
            0b10001_000,
            0b01110_000,
            0b10001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        '9',
        [
            0b01110_000,
            0b10001_000,
            0b10001_000,
            0b01111_000,
            0b00001_000,
            0b00010_000,
            0b01100_000,
        ],
    ),
    (
        '-',
        [
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b11111_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
        ],
    ),
    (
        '.',
        [
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b01100_000,
            0b01100_000,
        ],
    ),
    (
        '/',
        [
            0b00001_000,
            0b00010_000,
            0b00010_000,
            0b00100_000,
            0b01000_000,
            0b01000_000,
            0b10000_000,
        ],
    ),
    (
        'M',
        [
            0b10001_000,
            0b11011_000,
            0b10101_000,
            0b10101_000,
            0b10001_000,
            0b10001_000,
            0b10001_000,
        ],
    ),
    // Additional weather symbol letters
    (
        'R',
        [
            0b11110_000,
            0b10001_000,
            0b10001_000,
            0b11110_000,
            0b10100_000,
            0b10010_000,
            0b10001_000,
        ],
    ),
    (
        'S',
        [
            0b01110_000,
            0b10001_000,
            0b10000_000,
            0b01110_000,
            0b00001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        'N',
        [
            0b10001_000,
            0b11001_000,
            0b10101_000,
            0b10011_000,
            0b10001_000,
            0b10001_000,
            0b10001_000,
        ],
    ),
    (
        'F',
        [
            0b11111_000,
            0b10000_000,
            0b10000_000,
            0b11110_000,
            0b10000_000,
            0b10000_000,
            0b10000_000,
        ],
    ),
    (
        'G',
        [
            0b01110_000,
            0b10001_000,
            0b10000_000,
            0b10111_000,
            0b10001_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        'Z',
        [
            0b11111_000,
            0b00001_000,
            0b00010_000,
            0b00100_000,
            0b01000_000,
            0b10000_000,
            0b11111_000,
        ],
    ),
    (
        'T',
        [
            0b11111_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
        ],
    ),
    (
        'H',
        [
            0b10001_000,
            0b10001_000,
            0b10001_000,
            0b11111_000,
            0b10001_000,
            0b10001_000,
            0b10001_000,
        ],
    ),
    (
        'A',
        [
            0b01110_000,
            0b10001_000,
            0b10001_000,
            0b11111_000,
            0b10001_000,
            0b10001_000,
            0b10001_000,
        ],
    ),
    (
        'I',
        [
            0b01110_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b00100_000,
            0b01110_000,
        ],
    ),
    (
        'C',
        [
            0b01110_000,
            0b10001_000,
            0b10000_000,
            0b10000_000,
            0b10000_000,
            0b10001_000,
            0b01110_000,
        ],
    ),
    (
        ' ',
        [
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
            0b00000_000,
        ],
    ),
];

fn get_glyph(ch: char) -> Option<&'static [u8; 7]> {
    let ch_upper = ch.to_ascii_uppercase();
    FONT.iter().find(|(c, _)| *c == ch_upper).map(|(_, g)| g)
}

// ══════════════════════════════════════════════════════════════════════
// Primitive drawing on RGBA pixel buffer
// ══════════════════════════════════════════════════════════════════════

type Color = [u8; 4]; // RGBA

const RED: Color = [220, 40, 40, 255];
const GREEN: Color = [0, 160, 60, 255];
const BLUE: Color = [30, 80, 200, 255];
const WHITE: Color = [255, 255, 255, 255];
const BLACK: Color = [0, 0, 0, 255];
const DARK_GRAY: Color = [60, 60, 60, 255];
const CYAN: Color = [0, 180, 220, 255];
const YELLOW: Color = [220, 200, 0, 255];
const MAGENTA: Color = [180, 0, 180, 255];

/// Set a single pixel with alpha blending.
fn set_pixel(pixels: &mut [u8], width: usize, x: i32, y: i32, color: Color) {
    if x < 0 || y < 0 || x >= width as i32 || y >= TILE_SIZE as i32 {
        return;
    }
    let idx = (y as usize * width + x as usize) * 4;
    if idx + 3 >= pixels.len() {
        return;
    }
    let alpha = color[3] as u16;
    if alpha == 255 {
        pixels[idx] = color[0];
        pixels[idx + 1] = color[1];
        pixels[idx + 2] = color[2];
        pixels[idx + 3] = 255;
    } else if alpha > 0 {
        let inv = 255 - alpha;
        pixels[idx] = ((color[0] as u16 * alpha + pixels[idx] as u16 * inv) / 255) as u8;
        pixels[idx + 1] =
            ((color[1] as u16 * alpha + pixels[idx + 1] as u16 * inv) / 255) as u8;
        pixels[idx + 2] =
            ((color[2] as u16 * alpha + pixels[idx + 2] as u16 * inv) / 255) as u8;
        pixels[idx + 3] = (pixels[idx + 3] as u16 + alpha
            - (pixels[idx + 3] as u16 * alpha / 255))
            .min(255) as u8;
    }
}

/// Draw text using bitmap font.
fn draw_text(pixels: &mut [u8], width: usize, x: i32, y: i32, text: &str, color: Color) {
    let mut cx = x;
    for ch in text.chars() {
        if let Some(glyph) = get_glyph(ch) {
            for row in 0..GLYPH_H {
                let bits = glyph[row];
                for col in 0..GLYPH_W {
                    if bits & (0b10000000 >> col) != 0 {
                        set_pixel(pixels, width, cx + col as i32, y + row as i32, color);
                    }
                }
            }
        }
        cx += (GLYPH_W + 1) as i32; // 1 pixel spacing between chars
    }
}

/// Draw text with a dark outline/shadow for readability.
fn draw_text_outlined(
    pixels: &mut [u8],
    width: usize,
    x: i32,
    y: i32,
    text: &str,
    color: Color,
) {
    let bg = [0, 0, 0, 160];
    // Draw outline in 8 directions
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx != 0 || dy != 0 {
                draw_text(pixels, width, x + dx, y + dy, text, bg);
            }
        }
    }
    draw_text(pixels, width, x, y, text, color);
}

/// Draw a circle using midpoint algorithm.
fn draw_circle(
    pixels: &mut [u8],
    width: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Color,
    filled: bool,
) {
    if filled {
        // Filled circle using scanlines
        for dy in -radius..=radius {
            let dx_max = ((radius * radius - dy * dy) as f64).sqrt() as i32;
            for dx in -dx_max..=dx_max {
                set_pixel(pixels, width, cx + dx, cy + dy, color);
            }
        }
    } else {
        // Outline using midpoint circle
        let mut x = radius;
        let mut y = 0i32;
        let mut err = 1 - radius;

        while x >= y {
            set_pixel(pixels, width, cx + x, cy + y, color);
            set_pixel(pixels, width, cx - x, cy + y, color);
            set_pixel(pixels, width, cx + x, cy - y, color);
            set_pixel(pixels, width, cx - x, cy - y, color);
            set_pixel(pixels, width, cx + y, cy + x, color);
            set_pixel(pixels, width, cx - y, cy + x, color);
            set_pixel(pixels, width, cx + y, cy - x, color);
            set_pixel(pixels, width, cx - y, cy - x, color);

            y += 1;
            if err < 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }
}

/// Draw a line using Bresenham's algorithm.
fn draw_line(
    pixels: &mut [u8],
    width: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Color,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x = x0;
    let mut y = y0;

    loop {
        set_pixel(pixels, width, x, y, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            if x == x1 {
                break;
            }
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            if y == y1 {
                break;
            }
            err += dx;
            y += sy;
        }
    }
}

/// Draw a thick line (2px).
fn draw_line_thick(
    pixels: &mut [u8],
    width: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Color,
) {
    draw_line(pixels, width, x0, y0, x1, y1, color);
    draw_line(pixels, width, x0 + 1, y0, x1 + 1, y1, color);
    draw_line(pixels, width, x0, y0 + 1, x1, y1 + 1, color);
}

// ══════════════════════════════════════════════════════════════════════
// Sky cover circle rendering
// ══════════════════════════════════════════════════════════════════════

fn draw_sky_cover(
    pixels: &mut [u8],
    width: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    cover: SkyCoverType,
) {
    match cover {
        SkyCoverType::Clear => {
            // Empty circle
            draw_circle(pixels, width, cx, cy, radius, BLACK, false);
        }
        SkyCoverType::Few => {
            // Circle with small filled wedge (lower-right quadrant line)
            draw_circle(pixels, width, cx, cy, radius, BLACK, false);
            // Draw two diagonal lines in lower-right to indicate FEW
            for dy in 0..=radius {
                let dx_max = ((radius * radius - dy * dy) as f64).sqrt() as i32;
                // Fill only a small wedge (roughly 1/8 of circle)
                for dx in 0..=dx_max {
                    if dx >= dy {
                        set_pixel(pixels, width, cx + dx, cy + dy, BLACK);
                    }
                }
            }
        }
        SkyCoverType::Scattered => {
            // Half filled (left half)
            draw_circle(pixels, width, cx, cy, radius, BLACK, false);
            for dy in -radius..=radius {
                let dx_max = ((radius * radius - dy * dy) as f64).sqrt() as i32;
                for dx in -dx_max..=0 {
                    set_pixel(pixels, width, cx + dx, cy + dy, BLACK);
                }
            }
        }
        SkyCoverType::Broken => {
            // 3/4 filled (all but upper-right quadrant wedge)
            draw_circle(pixels, width, cx, cy, radius, BLACK, false);
            for dy in -radius..=radius {
                let dx_max = ((radius * radius - dy * dy) as f64).sqrt() as i32;
                for dx in -dx_max..=dx_max {
                    // Leave upper-right unfilled
                    if dx > 0 && dy < 0 {
                        continue;
                    }
                    set_pixel(pixels, width, cx + dx, cy + dy, BLACK);
                }
            }
        }
        SkyCoverType::Overcast | SkyCoverType::Obscured => {
            // Fully filled circle
            draw_circle(pixels, width, cx, cy, radius, BLACK, true);
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Wind barb rendering
// ══════════════════════════════════════════════════════════════════════

fn draw_wind_barb(
    pixels: &mut [u8],
    width: usize,
    cx: i32,
    cy: i32,
    dir_deg: u16,
    speed_kt: u16,
    barb_len: i32,
) {
    // Wind direction: direction FROM which wind blows
    // Barb points into the wind (from station toward wind origin)
    let dir_rad = (dir_deg as f64).to_radians();

    // Staff endpoint (points into the wind)
    let staff_x = cx + (barb_len as f64 * dir_rad.sin()) as i32;
    let staff_y = cy - (barb_len as f64 * dir_rad.cos()) as i32;

    // Draw staff from center to endpoint
    draw_line_thick(pixels, width, cx, cy, staff_x, staff_y, BLACK);

    if speed_kt < 3 {
        // Calm - just draw the circle (already drawn as sky cover)
        return;
    }

    // Calculate barbs: flags (50kt), full barbs (10kt), half barbs (5kt)
    let mut remaining = speed_kt;
    let flags = remaining / 50;
    remaining %= 50;
    let full_barbs = remaining / 10;
    remaining %= 10;
    let half_barbs = if remaining >= 3 { 1 } else { 0 };

    // Barb perpendicular direction (to the left when looking toward wind origin)
    let perp_x = dir_rad.cos();
    let perp_y = dir_rad.sin();
    let barb_tick_len = (barb_len as f64 * 0.4) as i32;

    let dx = dir_rad.sin();
    let dy = -dir_rad.cos();

    let mut pos = 0; // Distance along staff from outer end
    let spacing = barb_len as f64 * 0.15;

    // Draw flags (triangular pennants for 50kt)
    for _ in 0..flags {
        let base_x = staff_x - (pos as f64 * dx) as i32;
        let base_y = staff_y - (pos as f64 * dy) as i32;
        let next_x = staff_x - ((pos as f64 + spacing * 1.5) * dx) as i32;
        let next_y = staff_y - ((pos as f64 + spacing * 1.5) * dy) as i32;
        let tip_x = base_x + (barb_tick_len as f64 * perp_x) as i32;
        let tip_y = base_y + (barb_tick_len as f64 * perp_y) as i32;

        draw_line(pixels, width, base_x, base_y, tip_x, tip_y, BLACK);
        draw_line(pixels, width, tip_x, tip_y, next_x, next_y, BLACK);
        draw_line(pixels, width, next_x, next_y, base_x, base_y, BLACK);
        // Fill triangle
        let mid_x = (base_x + next_x + tip_x) / 3;
        let mid_y = (base_y + next_y + tip_y) / 3;
        set_pixel(pixels, width, mid_x, mid_y, BLACK);

        pos += (spacing * 1.8) as i32;
    }

    // Draw full barbs (long ticks for 10kt)
    for _ in 0..full_barbs {
        let base_x = staff_x - (pos as f64 * dx) as i32;
        let base_y = staff_y - (pos as f64 * dy) as i32;
        let tip_x = base_x + (barb_tick_len as f64 * perp_x) as i32;
        let tip_y = base_y + (barb_tick_len as f64 * perp_y) as i32;

        draw_line_thick(pixels, width, base_x, base_y, tip_x, tip_y, BLACK);
        pos += spacing as i32;
    }

    // Draw half barb (short tick for 5kt)
    if half_barbs > 0 {
        // If no full barbs, push half barb slightly inward
        if full_barbs == 0 && flags == 0 {
            pos += spacing as i32;
        }
        let base_x = staff_x - (pos as f64 * dx) as i32;
        let base_y = staff_y - (pos as f64 * dy) as i32;
        let half_len = barb_tick_len / 2;
        let tip_x = base_x + (half_len as f64 * perp_x) as i32;
        let tip_y = base_y + (half_len as f64 * perp_y) as i32;

        draw_line(pixels, width, base_x, base_y, tip_x, tip_y, BLACK);
    }
}

// ══════════════════════════════════════════════════════════════════════
// Weather symbol rendering
// ══════════════════════════════════════════════════════════════════════

fn draw_weather_symbol(
    pixels: &mut [u8],
    width: usize,
    x: i32,
    y: i32,
    wx: &str,
) {
    // Simplified weather symbols - draw abbreviated text
    let (text, color) = if wx.contains("TS") {
        ("TS", MAGENTA)
    } else if wx.contains("SN") {
        ("SN", CYAN)
    } else if wx.contains("FZ") && wx.contains("RA") {
        ("ZR", MAGENTA)
    } else if wx.contains("FZ") && wx.contains("DZ") {
        ("ZR", MAGENTA)
    } else if wx.contains("RA") {
        ("RA", GREEN)
    } else if wx.contains("DZ") {
        ("DZ", GREEN)
    } else if wx.contains("FG") {
        ("FG", YELLOW)
    } else if wx.contains("BR") {
        ("BR", YELLOW)
    } else if wx.contains("HZ") {
        ("HZ", YELLOW)
    } else if wx.contains("FU") || wx.contains("SMOKE") {
        ("FU", [160, 160, 160, 255])
    } else if wx.contains("PL") || wx.contains("IC") {
        ("IC", CYAN)
    } else {
        return; // Unknown wx, skip
    };

    draw_text_outlined(pixels, width, x, y, text, color);
}

// ══════════════════════════════════════════════════════════════════════
// Station model plot
// ══════════════════════════════════════════════════════════════════════

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn draw_station_model(
    pixels: &mut [u8],
    width: usize,
    cx: i32,
    cy: i32,
    obs: &SurfaceObs,
    zoom: u32,
) {
    let radius: i32 = if zoom > 7 { 7 } else { 5 };

    // --- Sky cover circle (center) ---
    // Draw white background behind circle for contrast
    draw_circle(pixels, width, cx, cy, radius + 1, WHITE, true);
    draw_sky_cover(pixels, width, cx, cy, radius, obs.sky_cover);

    if zoom < 5 {
        // Low zoom: just the dot
        return;
    }

    if zoom <= 7 {
        // Medium zoom: simplified - temp and wind only
        // Temperature upper-left
        if let Some(tc) = obs.temp_c {
            let tf = c_to_f(tc).round() as i32;
            let text = format!("{}", tf);
            draw_text_outlined(
                pixels,
                width,
                cx - radius - 6 * text.len() as i32,
                cy - radius - 4,
                &text,
                RED,
            );
        }

        // Wind barb
        if let (Some(dir), Some(spd)) = (obs.wind_dir, obs.wind_speed_kt) {
            if spd >= 3 {
                draw_wind_barb(pixels, width, cx, cy, dir, spd, 20);
            }
        }

        return;
    }

    // --- High zoom (z > 7): full station model ---

    let text_offset = radius + 4;

    // Temperature (upper-left, red, in Fahrenheit)
    if let Some(tc) = obs.temp_c {
        let tf = c_to_f(tc).round() as i32;
        let text = format!("{}", tf);
        let tw = text.len() as i32 * (GLYPH_W as i32 + 1);
        draw_text_outlined(
            pixels,
            width,
            cx - text_offset - tw,
            cy - text_offset - GLYPH_H as i32 + 2,
            &text,
            RED,
        );
    }

    // Dewpoint (lower-left, green, in Fahrenheit)
    if let Some(dc) = obs.dewpoint_c {
        let df = c_to_f(dc).round() as i32;
        let text = format!("{}", df);
        let tw = text.len() as i32 * (GLYPH_W as i32 + 1);
        draw_text_outlined(
            pixels,
            width,
            cx - text_offset - tw,
            cy + text_offset - 2,
            &text,
            GREEN,
        );
    }

    // Pressure (upper-right, last 3 digits of altimeter)
    if let Some(alt) = obs.altimeter_inhg {
        // Convert to tens of mb: e.g. 29.92 inHg = 1013.2 hPa, display "132"
        let hpa = alt * 33.8639;
        let coded = ((hpa * 10.0).round() as i32) % 1000;
        let text = format!("{:03}", coded);
        draw_text_outlined(
            pixels,
            width,
            cx + text_offset,
            cy - text_offset - GLYPH_H as i32 + 2,
            &text,
            BLUE,
        );
    }

    // Wind barb (extending from center in wind-from direction)
    if let (Some(dir), Some(spd)) = (obs.wind_dir, obs.wind_speed_kt) {
        let barb_len = 28;
        if spd >= 3 {
            draw_wind_barb(pixels, width, cx, cy, dir, spd, barb_len);
        }
    }

    // Weather symbol (right of station, below pressure)
    if let Some(ref wx) = obs.wx_string {
        draw_weather_symbol(
            pixels,
            width,
            cx + text_offset,
            cy + text_offset - 2,
            wx,
        );
    }
}

// ══════════════════════════════════════════════════════════════════════
// Public tile generation
// ══════════════════════════════════════════════════════════════════════

pub async fn generate_surface_tile(
    cache: &MetarCache,
    z: u32,
    x: u32,
    y: u32,
) -> Result<Vec<u8>, String> {
    let all_obs = cache.get_obs().await?;

    let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, x, y);

    // Expand bounds slightly so stations near edges still render
    let lat_margin = (lat_max - lat_min) * 0.15;
    let lon_margin = (lon_max - lon_min) * 0.15;

    // Filter stations to those within this tile (with margin)
    let tile_obs: Vec<&SurfaceObs> = all_obs
        .iter()
        .filter(|obs| {
            obs.lat >= lat_min - lat_margin
                && obs.lat <= lat_max + lat_margin
                && obs.lon >= lon_min - lon_margin
                && obs.lon <= lon_max + lon_margin
        })
        .filter(|obs| {
            // At low zoom, only show major airports
            if z < 5 {
                is_major_airport(&obs.station_id)
            } else {
                true
            }
        })
        .collect();

    // Render on a background thread since it's CPU work
    let tile_obs_owned: Vec<SurfaceObs> = tile_obs.into_iter().cloned().collect();
    let png_bytes = tokio::task::spawn_blocking(move || {
        let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4]; // transparent

        for obs in &tile_obs_owned {
            let (px, py) = latlon_to_pixel(
                obs.lat, obs.lon, lat_min, lon_min, lat_max, lon_max,
            );

            // Only draw if center is roughly within tile (with some margin for barbs/text)
            if px >= -40 && px < TILE_SIZE as i32 + 40 && py >= -40 && py < TILE_SIZE as i32 + 40
            {
                draw_station_model(&mut pixels, TILE_SIZE, px, py, obs, z);
            }
        }

        encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("PNG encode error: {}", e))?;

    Ok(png_bytes)
}
