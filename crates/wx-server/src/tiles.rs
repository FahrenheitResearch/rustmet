use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2;
use rustmet_core::render::colormap::{get_colormap, interpolate_color};
use rustmet_core::render::encode::encode_png;
use wx_field::projection::{LambertProjection, LatLonProjection, Projection};

pub const TILE_SIZE: usize = 256;

// ── Run cache ──────────────────────────────────────────────────────
// Caches the latest model run (date, hour) per model so we only probe
// NOMADS once per model, not once per tile request.

const RUN_TTL: Duration = Duration::from_secs(120); // Re-check every 2 min

struct CachedRun {
    date: String,
    hour: u32,
    found_at: Instant,
}

// ── Field cache ────────────────────────────────────────────────────
// Caches decoded GRIB2 field data so multiple tile requests for the
// same model/var/level/fhour share a single download.

pub struct CachedField {
    pub values: Vec<f64>,
    pub nx: usize,
    pub ny: usize,
    pub proj: Box<dyn Projection>,
    pub colormap: &'static str,
    pub vmin: f64,
    pub vmax: f64,
    pub fetched_at: Instant,
}

const FIELD_TTL: Duration = Duration::from_secs(300);

type FieldKey = (String, String, String, u32); // (model, var, level, fhour)

pub struct FieldCache {
    runs: RwLock<HashMap<String, CachedRun>>,
    fields: RwLock<HashMap<FieldKey, Arc<CachedField>>>,
    // Per-key mutex to prevent thundering herd: if a field download is
    // in progress, other requests for the same key wait instead of
    // starting duplicate downloads.
    inflight: Mutex<HashMap<FieldKey, Arc<tokio::sync::Notify>>>,
}

impl FieldCache {
    pub fn new() -> Self {
        Self {
            runs: RwLock::new(HashMap::new()),
            fields: RwLock::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    async fn get_run(&self, model: &str) -> Option<(String, u32)> {
        let runs = self.runs.read().await;
        if let Some(run) = runs.get(model) {
            if run.found_at.elapsed() < RUN_TTL {
                return Some((run.date.clone(), run.hour));
            }
        }
        None
    }

    async fn put_run(&self, model: &str, date: String, hour: u32) {
        let mut runs = self.runs.write().await;
        runs.insert(model.to_string(), CachedRun {
            date,
            hour,
            found_at: Instant::now(),
        });
    }

    /// Return all cached model runs as (model, date, hour) triples.
    pub async fn get_all_runs(&self) -> Vec<(String, String, u32)> {
        let runs = self.runs.read().await;
        runs.iter()
            .filter(|(_, r)| r.found_at.elapsed() < RUN_TTL)
            .map(|(model, r)| (model.clone(), r.date.clone(), r.hour))
            .collect()
    }

    pub async fn get_field(&self, key: &FieldKey) -> Option<Arc<CachedField>> {
        let fields = self.fields.read().await;
        if let Some(field) = fields.get(key) {
            if field.fetched_at.elapsed() < FIELD_TTL {
                return Some(field.clone());
            }
        }
        None
    }

    async fn put_field(&self, key: FieldKey, field: CachedField) -> Arc<CachedField> {
        let arc = Arc::new(field);
        let mut fields = self.fields.write().await;
        fields.retain(|_, v| v.fetched_at.elapsed() < FIELD_TTL);
        fields.insert(key, arc.clone());
        arc
    }
}

// ── Tile math ──────────────────────────────────────────────────────

pub fn tile_bounds(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
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

pub fn mercator_lat(lat_max: f64, lat_min: f64, t: f64) -> f64 {
    let y_max = lat_max.to_radians().tan().asinh();
    let y_min = lat_min.to_radians().tan().asinh();
    let y = y_max + t * (y_min - y_max);
    y.sinh().atan().to_degrees()
}

pub fn sample_bilinear(values: &[f64], nx: usize, ny: usize, gi: f64, gj: f64) -> Option<f64> {
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
    Some(v00 * (1.0 - fi) * (1.0 - fj) + v10 * fi * (1.0 - fj) + v01 * (1.0 - fi) * fj + v11 * fi * fj)
}

// ── Variable mapping ───────────────────────────────────────────────

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

fn select_colormap_and_range(var: &str, units: &str) -> (&'static str, f64, f64) {
    match var.to_lowercase().as_str() {
        "cape" => ("cape", 0.0, 5000.0),
        "cin" => ("cape", -500.0, 0.0),
        "reflectivity" | "refl" | "refc" => ("nws_reflectivity", -10.0, 75.0),
        "temperature" | "temp" | "t" => {
            if units.contains("K") { ("temperature", 233.15, 323.15) }
            else { ("temperature", -40.0, 50.0) }
        }
        "dewpoint" | "td" | "dew" => {
            if units.contains("K") { ("dewpoint", 243.15, 303.15) }
            else { ("dewpoint", -30.0, 30.0) }
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
        _ => ("temperature", 0.0, 1.0),
    }
}

fn build_projection(grid: &grib2::GridDefinition) -> Option<Box<dyn Projection>> {
    match grid.template {
        30 => {
            let mut lo1 = grid.lon1;
            if lo1 > 180.0 { lo1 -= 360.0; }
            let mut lov = grid.lov;
            if lov > 180.0 { lov -= 360.0; }
            Some(Box::new(LambertProjection::grib2(
                grid.latin1, grid.latin2, lov, grid.lat1, lo1,
                grid.dx, grid.dy, grid.nx, grid.ny,
            )))
        }
        // Lat/Lon grid (GFS). GRIB2 stores longitudes as 0-360.
        // Keep them in 0-360 space for the projection — we'll convert
        // tile longitudes to match in the render step.
        0 => {
            Some(Box::new(LatLonProjection::new(
                grid.lat1, grid.lon1, grid.lat2, grid.lon2, grid.nx, grid.ny,
            )))
        }
        _ => None,
    }
}

// ── Style resolution ──────────────────────────────────────────────

/// Resolve a colormap name given the base variable colormap and an optional style.
///
/// Style overrides let users request alternate color schemes:
/// - `?style=nws`      → NWS-style colors (e.g., temperature_nws)
/// - `?style=pivotal`  → Professional meteorological style (e.g., temperature_pivotal)
/// - `?style=clean`    → Clean variant for dark backgrounds (e.g., reflectivity_clean)
///
/// If the styled variant doesn't exist, falls back to the base colormap.
pub fn resolve_colormap_static(base: &'static str, style: Option<&str>) -> &'static str {
    let style = match style {
        Some(s) if !s.is_empty() => s,
        _ => return base,
    };

    match (base, style) {
        ("temperature" | "temp" | "temperature_f" | "temperature_c"
         | "temperature_250" | "temperature_500" | "temperature_700", "nws") => "temperature_nws",
        ("temperature" | "temp" | "temperature_f" | "temperature_c"
         | "temperature_250" | "temperature_500" | "temperature_700", "pivotal") => "temperature_pivotal",
        ("cape", "pivotal") => "cape_pivotal",
        ("wind" | "winds" | "winds_sfc", "pivotal") => "wind_pivotal",
        ("nws_reflectivity" | "reflectivity" | "refl", "clean") => "reflectivity_clean",
        _ => base,
    }
}

// ── Tile rendering ─────────────────────────────────────────────────

fn render_tile_pixels(field: &CachedField, z: u32, x: u32, y: u32) -> Vec<u8> {
    render_tile_pixels_styled(field, z, x, y, None)
}

fn render_tile_pixels_styled(field: &CachedField, z: u32, x: u32, y: u32, style: Option<&str>) -> Vec<u8> {
    let cmap_name = resolve_colormap_static(field.colormap, style);
    let cmap = get_colormap(cmap_name)
        .unwrap_or_else(|| get_colormap("temperature").unwrap());
    let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, x, y);
    // Detect if grid uses 0-360 longitude (GFS) vs -180..180 (Lambert)
    let grid_is_0_360 = field.proj.bounding_box().1 >= 0.0 && field.proj.bounding_box().3 > 180.0;
    let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];

    for py in 0..TILE_SIZE {
        for px in 0..TILE_SIZE {
            let tx = (px as f64 + 0.5) / TILE_SIZE as f64;
            let ty = (py as f64 + 0.5) / TILE_SIZE as f64;
            let mut lon = lon_min + tx * (lon_max - lon_min);
            let lat = mercator_lat(lat_max, lat_min, ty);
            // Convert tile longitude to grid's coordinate system
            if grid_is_0_360 && lon < 0.0 {
                lon += 360.0;
            }
            let (gi, gj) = field.proj.latlon_to_grid(lat, lon);

            if let Some(val) = sample_bilinear(&field.values, field.nx, field.ny, gi, gj) {
                if val.is_finite() && val.abs() < 1e15 && val > -900.0 {
                    let norm = ((val - field.vmin) / (field.vmax - field.vmin)).clamp(0.0, 1.0);
                    let (r, g, b) = interpolate_color(cmap, norm);
                    // Make near-black pixels transparent (e.g. reflectivity below threshold)
                    let alpha = if r < 3 && g < 3 && b < 3 { 0 } else { 200 };
                    let idx = (py * TILE_SIZE + px) * 4;
                    pixels[idx] = r;
                    pixels[idx + 1] = g;
                    pixels[idx + 2] = b;
                    pixels[idx + 3] = alpha;
                }
            }
        }
    }
    pixels
}

// ── Public API ─────────────────────────────────────────────────────

pub async fn generate_tile(
    field_cache: &FieldCache,
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    z: u32,
    x: u32,
    y: u32,
) -> Result<Vec<u8>, String> {
    generate_tile_styled(field_cache, model, var, level, fhour, z, x, y, None, None).await
}

/// Generate a tile with an optional colormap style override and optional run.
///
/// The `style` parameter selects an alternate colormap:
/// - `Some("nws")` → NWS-style colors
/// - `Some("pivotal")` → Professional meteorological style
/// - `Some("clean")` → Clean variant for dark backgrounds
/// - `None` → default colormap
///
/// The `run` parameter selects a specific model run:
/// - `Some("20260312/02z")` → use this specific run
/// - `None` → use the latest available run
pub async fn generate_tile_styled(
    field_cache: &FieldCache,
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    z: u32,
    x: u32,
    y: u32,
    style: Option<&str>,
    run: Option<&str>,
) -> Result<Vec<u8>, String> {
    let field = ensure_field_run(field_cache, model, var, level, fhour, run).await?;
    let style_owned = style.map(|s| s.to_string());

    let png_bytes = tokio::task::spawn_blocking(move || {
        let pixels = render_tile_pixels_styled(&field, z, x, y, style_owned.as_deref());
        encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("PNG encode error: {}", e))?;

    Ok(png_bytes)
}

/// Generate a contour-only tile (transparent PNG with contour lines).
///
/// Reuses the same field download + cache pipeline as filled tiles.
pub async fn generate_contour_tile(
    field_cache: &FieldCache,
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    z: u32,
    x: u32,
    y: u32,
    interval: f64,
    color: (u8, u8, u8),
    line_width: f64,
    run: Option<&str>,
) -> Result<Vec<u8>, String> {
    // Use the same field fetch logic as generate_tile
    let field = ensure_field_run(field_cache, model, var, level, fhour, run).await?;

    let png_bytes = tokio::task::spawn_blocking(move || {
        crate::contours::render_contour_tile(&field, z, x, y, interval, color, line_width)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("Contour render error: {}", e))?;

    Ok(png_bytes)
}

/// Parse a run string like "20260312/02z" into (date, hour).
pub fn parse_run_param(run: &str) -> Result<(String, u32), String> {
    // Expected format: YYYYMMDD/HHz
    let parts: Vec<&str> = run.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid run format '{}'. Expected YYYYMMDD/HHz", run));
    }
    let date = parts[0].to_string();
    if date.len() != 8 || !date.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid date '{}'. Expected YYYYMMDD", date));
    }
    let hour_str = parts[1].trim_end_matches('z').trim_end_matches('Z');
    let hour: u32 = hour_str.parse().map_err(|_| format!("Invalid hour '{}'", parts[1]))?;
    if hour > 23 {
        return Err(format!("Hour {} out of range (0-23)", hour));
    }
    Ok((date, hour))
}

/// Ensure a field is downloaded and cached, returning an Arc to it.
/// Extracted from generate_tile to share with contour and wind barb generation.
///
/// When `run` is Some, uses the specified run (date/hour) instead of probing
/// for the latest. Format: "YYYYMMDD/HHz" (e.g., "20260312/02z").
pub(crate) async fn ensure_field(
    field_cache: &FieldCache,
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
) -> Result<Arc<CachedField>, String> {
    ensure_field_run(field_cache, model, var, level, fhour, None).await
}

pub(crate) async fn ensure_field_run(
    field_cache: &FieldCache,
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    run: Option<&str>,
) -> Result<Arc<CachedField>, String> {
    let key: FieldKey = (
        model.to_lowercase(),
        var.to_lowercase(),
        level.to_lowercase(),
        fhour,
    );

    // When a specific run is requested, use a run-qualified cache key
    // so different runs don't collide in the field cache.
    let key = if let Some(run_str) = run {
        let (date, hour) = parse_run_param(run_str)?;
        (
            format!("{}_{}_{:02}z", model.to_lowercase(), date, hour),
            var.to_lowercase(),
            level.to_lowercase(),
            fhour,
        )
    } else {
        key
    };

    // Fast path
    if let Some(field) = field_cache.get_field(&key).await {
        return Ok(field);
    }

    // Slow path with inflight dedup
    let notify = {
        let mut inflight = field_cache.inflight.lock().await;
        if let Some(notify) = inflight.get(&key) {
            let notify = notify.clone();
            drop(inflight);
            notify.notified().await;
            if let Some(field) = field_cache.get_field(&key).await {
                return Ok(field);
            }
            return Err("Field download completed but not found in cache".to_string());
        }
        let notify = Arc::new(tokio::sync::Notify::new());
        inflight.insert(key.clone(), notify.clone());
        notify
    };

    let model_lower = model.to_lowercase();

    // If a specific run was requested, use it directly; otherwise probe for latest
    let (date, hour) = if let Some(run_str) = run {
        parse_run_param(run_str)?
    } else {
        let cached_run = field_cache.get_run(&model_lower).await;
        if let Some(run) = cached_run {
            run
        } else {
            let m = model_lower.clone();
            let run = tokio::task::spawn_blocking(move || {
                let client = DownloadClient::new().map_err(|e| format!("HTTP client error: {}", e))?;
                rustmet_core::models::find_latest_run(&client, &m)
                    .map_err(|e| format!("No model run found: {}", e))
            })
            .await
            .map_err(|e| format!("Task join error: {}", e))??;
            field_cache.put_run(&model_lower, run.0.clone(), run.1).await;
            run
        }
    };

    let var_owned = var.to_string();
    let level_owned = level.to_string();
    let model_for_dl = model.to_lowercase();
    let key_clone = key.clone();

    let result = tokio::task::spawn_blocking(move || {
        download_field(&model_for_dl, &var_owned, &level_owned, fhour, &date, hour)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    {
        let mut inflight = field_cache.inflight.lock().await;
        inflight.remove(&key_clone);
    }
    notify.notify_waiters();

    let cached = field_cache.put_field(key_clone, result?).await;
    Ok(cached)
}

fn download_field(
    model: &str,
    var: &str,
    level: &str,
    fhour: u32,
    date: &str,
    hour: u32,
) -> Result<CachedField, String> {
    let client = DownloadClient::new().map_err(|e| format!("HTTP client error: {}", e))?;

    let pattern = build_pattern(var, level);
    let product = if level.contains("mb") || level.contains("hPa") { "prs" } else { "sfc" };
    let patterns: Vec<&str> = vec![pattern.as_str()];

    eprintln!(
        "[tiles] Downloading {} {}/{:02}z {} f{:02} [{}]",
        model, date, hour, product, fhour, pattern
    );

    let result = fetch_with_fallback(
        &client, model, date, hour, product, fhour,
        Some(&patterns), None,
    )
    .map_err(|e| format!("Download failed: {}", e))?;

    eprintln!(
        "[tiles] Got {} bytes from {} for {}",
        result.data.len(), result.source_name, pattern
    );

    let grib = grib2::Grib2File::from_bytes(&result.data)
        .map_err(|e| format!("GRIB2 parse error: {}", e))?;

    if grib.messages.is_empty() {
        return Err("No matching GRIB2 messages found".to_string());
    }

    let msg = &grib.messages[0];
    let mut values = grib2::unpack_message(msg)
        .map_err(|e| format!("Unpack error: {}", e))?;

    let nx = msg.grid.nx as usize;
    let ny = msg.grid.ny as usize;

    let proj = build_projection(&msg.grid)
        .ok_or_else(|| format!("Unsupported grid template {}", msg.grid.template))?;

    let param_units = grib2::tables::parameter_units(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );

    let (colormap, vmin, vmax) = select_colormap_and_range(var, param_units);

    Ok(CachedField {
        values,
        nx,
        ny,
        proj,
        colormap,
        vmin,
        vmax,
        fetched_at: Instant::now(),
    })
}

// ── Legend rendering ──────────────────────────────────────────────

/// 5x7 bitmap font for digits 0-9, minus sign, and decimal point.
/// Each character is stored as 7 rows of 5-bit patterns (MSB = leftmost pixel).
static FONT_5X7: [[u8; 7]; 12] = [
    // '0'
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
    // '1'
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
    // '2'
    [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111],
    // '3'
    [0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110],
    // '4'
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
    // '5'
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
    // '6'
    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
    // '7'
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
    // '8'
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    // '9'
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
    // '-' (index 10)
    [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
    // '.' (index 11)
    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
];

fn char_to_font_index(c: char) -> Option<usize> {
    match c {
        '0'..='9' => Some((c as u8 - b'0') as usize),
        '-' => Some(10),
        '.' => Some(11),
        _ => None,
    }
}

/// Draw a single character at (cx, cy) into an RGBA pixel buffer.
fn draw_char(pixels: &mut [u8], width: usize, _height: usize, cx: usize, cy: usize, ch: char, color: (u8, u8, u8, u8)) {
    let idx = match char_to_font_index(ch) {
        Some(i) => i,
        None => return,
    };
    let glyph = &FONT_5X7[idx];
    for row in 0..7 {
        let py = cy + row;
        if py >= _height { break; }
        for col in 0..5 {
            let px = cx + col;
            if px >= width { break; }
            if (glyph[row] >> (4 - col)) & 1 == 1 {
                let off = (py * width + px) * 4;
                pixels[off]     = color.0;
                pixels[off + 1] = color.1;
                pixels[off + 2] = color.2;
                pixels[off + 3] = color.3;
            }
        }
    }
}

/// Draw a string of digits at (x, y) with 6px character spacing.
fn draw_text(pixels: &mut [u8], width: usize, height: usize, x: usize, y: usize, text: &str, color: (u8, u8, u8, u8)) {
    let mut cx = x;
    for ch in text.chars() {
        draw_char(pixels, width, height, cx, y, ch, color);
        cx += 6; // 5px char + 1px gap
    }
}

/// Format a floating-point value for the legend, keeping it compact.
fn format_legend_value(val: f64) -> String {
    if val.abs() >= 1000.0 {
        format!("{:.0}", val)
    } else if val.abs() >= 100.0 {
        format!("{:.0}", val)
    } else if val.abs() >= 10.0 {
        format!("{:.0}", val)
    } else if val.abs() >= 1.0 {
        format!("{:.1}", val)
    } else {
        format!("{:.2}", val)
    }
}

/// Render a colormap legend as a horizontal PNG image (300x40 px).
///
/// The legend shows the gradient from vmin to vmax with tick marks and labels.
/// Returns RGBA PNG bytes.
pub fn render_legend(
    var: &str,
    style: Option<&str>,
    vmin: f64,
    vmax: f64,
) -> Result<Vec<u8>, String> {
    let width: usize = 300;
    let height: usize = 40;
    let bar_top: usize = 2;
    let bar_bottom: usize = 18;
    let bar_left: usize = 10;
    let bar_right: usize = width - 10;
    let bar_width = bar_right - bar_left;

    // Determine colormap name
    let (base_cmap, default_vmin, default_vmax) = select_colormap_and_range(var, "");
    let cmap_name = resolve_colormap_static(base_cmap, style);
    let cmap = get_colormap(cmap_name)
        .unwrap_or_else(|| get_colormap("temperature").unwrap());

    let use_vmin = if vmin == vmax { default_vmin } else { vmin };
    let use_vmax = if vmin == vmax { default_vmax } else { vmax };

    let mut pixels = vec![0u8; width * height * 4];

    // Draw the gradient bar
    for py in bar_top..bar_bottom {
        for px in bar_left..bar_right {
            let t = (px - bar_left) as f64 / bar_width as f64;
            let (r, g, b) = interpolate_color(cmap, t);
            let off = (py * width + px) * 4;
            pixels[off]     = r;
            pixels[off + 1] = g;
            pixels[off + 2] = b;
            pixels[off + 3] = 255;
        }
    }

    // Draw border around gradient bar
    let border_color: (u8, u8, u8, u8) = (200, 200, 200, 255);
    for px in bar_left..bar_right {
        // Top border
        let off = ((bar_top.saturating_sub(1)) * width + px) * 4;
        pixels[off] = border_color.0; pixels[off+1] = border_color.1;
        pixels[off+2] = border_color.2; pixels[off+3] = border_color.3;
        // Bottom border
        let off = (bar_bottom * width + px) * 4;
        pixels[off] = border_color.0; pixels[off+1] = border_color.1;
        pixels[off+2] = border_color.2; pixels[off+3] = border_color.3;
    }
    for py in bar_top..=bar_bottom {
        // Left border
        let off = (py * width + bar_left.saturating_sub(1)) * 4;
        pixels[off] = border_color.0; pixels[off+1] = border_color.1;
        pixels[off+2] = border_color.2; pixels[off+3] = border_color.3;
        // Right border
        let off = (py * width + bar_right) * 4;
        pixels[off] = border_color.0; pixels[off+1] = border_color.1;
        pixels[off+2] = border_color.2; pixels[off+3] = border_color.3;
    }

    // Draw tick marks and value labels
    let num_ticks = 5;
    let text_color: (u8, u8, u8, u8) = (220, 220, 220, 255);

    for i in 0..=num_ticks {
        let frac = i as f64 / num_ticks as f64;
        let px = bar_left + (frac * bar_width as f64) as usize;
        let val = use_vmin + frac * (use_vmax - use_vmin);

        // Tick mark
        for py in bar_bottom..bar_bottom + 3 {
            if px < width && py < height {
                let off = (py * width + px) * 4;
                pixels[off] = text_color.0; pixels[off+1] = text_color.1;
                pixels[off+2] = text_color.2; pixels[off+3] = text_color.3;
            }
        }

        // Value label
        let label = format_legend_value(val);
        let label_width = label.len() * 6;
        let label_x = if i == 0 {
            px
        } else if i == num_ticks {
            px.saturating_sub(label_width)
        } else {
            px.saturating_sub(label_width / 2)
        };

        draw_text(&mut pixels, width, height, label_x, bar_bottom + 5, &label, text_color);
    }

    encode_png(&pixels, width as u32, height as u32)
}
