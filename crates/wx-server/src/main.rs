mod cache;
mod contours;
mod radar_tiles;
mod sounding;
mod sse;
mod surface_obs;
mod tiles;
mod wind_barbs;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tower_http::cors::CorsLayer;

use cache::TileCache;
use radar_tiles::RadarCache;
use sounding::SoundingCache;
use sse::EventHub;
use surface_obs::MetarCache;
use tiles::FieldCache;

/// Shared application state
#[derive(Clone)]
struct AppState {
    cache: Arc<TileCache>,
    hub: Arc<EventHub>,
    fields: Arc<FieldCache>,
    radar: Arc<RadarCache>,
    sounding_cache: Arc<SoundingCache>,
    metar_cache: Arc<MetarCache>,
}

// ---------------------------------------------------------------------------
// Binary discovery
// ---------------------------------------------------------------------------

fn find_wx_pro() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(std::path::Path::new("."));
        let candidate = dir.join("wx-pro.exe");
        if candidate.exists() {
            return candidate;
        }
        let candidate = dir.join("wx-pro");
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("wx-pro")
}

async fn run_wx_pro(args: &[&str]) -> Result<String, String> {
    let bin = find_wx_pro();
    tracing::debug!("running {} {:?}", bin.display(), args);

    let output = Command::new(&bin)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to execute wx-pro: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("wx-pro failed: {}", stderr.trim()))
    }
}

// ---------------------------------------------------------------------------
// Generic JSON proxy — runs wx-pro with the given args and returns stdout
// as application/json.
// ---------------------------------------------------------------------------

async fn json_proxy(args: &[&str]) -> Response {
    match run_wx_pro(args).await {
        Ok(json_str) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "public, max-age=60"),
            ],
            json_str,
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    }
}

/// Same as json_proxy but returns the raw PNG bytes when `raw=true` is in the
/// query-string, otherwise falls back to JSON.
async fn image_or_json_proxy(
    args: &[&str],
    raw: bool,
) -> Response {
    match run_wx_pro(args).await {
        Ok(output) => {
            if raw {
                // wx-pro prints JSON with an "image_path" field
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&output) {
                    if let Some(path) = v.get("image_path").and_then(|p| p.as_str()) {
                        if let Ok(data) = tokio::fs::read(path).await {
                            return (
                                StatusCode::OK,
                                [
                                    (header::CONTENT_TYPE, "image/png"),
                                    (header::CACHE_CONTROL, "public, max-age=60"),
                                ],
                                data,
                            )
                                .into_response();
                        }
                    }
                }
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read image").into_response()
            } else {
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "application/json"),
                        (header::CACHE_CONTROL, "public, max-age=60"),
                    ],
                    output,
                )
                    .into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Tile handler
// ---------------------------------------------------------------------------

async fn handle_tile(
    State(state): State<AppState>,
    Path((model, var, level, fhour_str, z, x, y_png)): Path<(
        String,
        String,
        String,
        String,
        u32,
        u32,
        String,
    )>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let y: u32 = y_png.trim_end_matches(".png").parse().unwrap_or(0);
    let fhour: u32 = fhour_str.trim_start_matches('f').parse().unwrap_or(0);
    let style = params.get("style").map(|s| s.as_str());

    // When a style is specified, include it in the cache key to avoid serving
    // tiles rendered with a different colormap.
    let cache_var = if let Some(s) = style {
        format!("{}__style_{}", var, s)
    } else {
        var.clone()
    };

    // Check rendered tile cache first
    if let Some(data) = state.cache.get(&model, &cache_var, &level, fhour, z, x, y).await {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
            data,
        )
            .into_response();
    }

    // In-process rendering: field is downloaded once and cached,
    // then individual tiles are rendered from the cached field data.
    let run = params.get("run").map(|s| s.as_str());
    match tiles::generate_tile_styled(&state.fields, &model, &var, &level, fhour, z, x, y, style, run).await {
        Ok(png_bytes) => {
            // Cache the rendered tile (with style-aware key)
            state.cache.put(&model, &cache_var, &level, fhour, z, x, y, png_bytes.clone()).await;
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=300"),
                ],
                png_bytes,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Tile generation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Legend handler
// ---------------------------------------------------------------------------

async fn handle_legend(
    Path(var): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let style = params.get("style").map(|s| s.as_str());
    let vmin: f64 = params.get("vmin").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let vmax: f64 = params.get("vmax").and_then(|v| v.parse().ok()).unwrap_or(0.0);

    let var_clone = var.clone();
    let style_owned = style.map(|s| s.to_string());

    match tokio::task::spawn_blocking(move || {
        tiles::render_legend(&var_clone, style_owned.as_deref(), vmin, vmax)
    })
    .await
    {
        Ok(Ok(png_bytes)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            png_bytes,
        )
            .into_response(),
        Ok(Err(e)) => {
            tracing::error!("Legend render failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
        Err(e) => {
            tracing::error!("Legend task join error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {}", e)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Radar tile handler
// ---------------------------------------------------------------------------

async fn handle_radar_tile(
    State(state): State<AppState>,
    Path((site, product, z, x, y_png)): Path<(String, String, u32, u32, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let y: u32 = y_png.trim_end_matches(".png").parse().unwrap_or(0);
    let scan_file = params.get("scan").map(|s| s.as_str());

    // Radar tiles skip the rendered tile cache. The underlying RadarCache (120s TTL)
    // already caches decoded sweep data in memory, and rendering from cached sweeps
    // is pure CPU math (no I/O). Using the tile cache (300s TTL) caused stale tiles
    // to persist longer than the radar data they were rendered from.
    match radar_tiles::generate_radar_tile(&state.radar, &site, &product, z, x, y, scan_file).await {
        Ok(png_bytes) => {
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=120"),
                ],
                png_bytes,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Radar tile generation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

/// List available radar scans for a given site from NOMADS.
async fn handle_radar_scans(
    Path(site): Path<String>,
) -> Response {
    let site_upper = site.to_uppercase();
    match tokio::task::spawn_blocking(move || {
        radar_tiles::list_nomads_scans(&site_upper)
    })
    .await
    {
        Ok(Ok(entries)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "public, max-age=30"),
            ],
            serde_json::to_string(&json!({
                "site": site.to_uppercase(),
                "scans": entries,
            })).unwrap_or_default(),
        )
            .into_response(),
        Ok(Err(e)) => {
            tracing::error!("Radar scan listing failed for {}: {}", site, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response()
        }
        Err(e) => {
            tracing::error!("Radar scan listing task error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{}", e)}))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Contour tile handler
// ---------------------------------------------------------------------------

async fn handle_contour_tile(
    State(state): State<AppState>,
    Path((model, var, level, fhour_str, z, x, y_png)): Path<(
        String,
        String,
        String,
        String,
        u32,
        u32,
        String,
    )>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let y: u32 = y_png.trim_end_matches(".png").parse().unwrap_or(0);
    let fhour: u32 = fhour_str.trim_start_matches('f').parse().unwrap_or(0);

    // Get contour settings from query params or defaults
    let (default_interval, default_color, default_width) =
        contours::default_contour_settings(&var);

    let interval: f64 = params
        .get("interval")
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_interval);

    let color = if let Some(c) = params.get("color") {
        parse_color(c).unwrap_or(default_color)
    } else {
        default_color
    };

    let line_width: f64 = params
        .get("width")
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_width);

    // Check cache (use "contour_" prefix to separate from filled tiles)
    let cache_var = format!("contour_{}", var);
    if let Some(data) = state.cache.get(&model, &cache_var, &level, fhour, z, x, y).await {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
            data,
        )
            .into_response();
    }

    let run = params.get("run").map(|s| s.as_str());
    match tiles::generate_contour_tile(
        &state.fields, &model, &var, &level, fhour, z, x, y,
        interval, color, line_width, run,
    )
    .await
    {
        Ok(png_bytes) => {
            state.cache.put(&model, &cache_var, &level, fhour, z, x, y, png_bytes.clone()).await;
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=300"),
                ],
                png_bytes,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Contour tile generation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

/// Parse a hex color string like "ffffff" or "ff0000" to (r, g, b).
fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

// ---------------------------------------------------------------------------
// Wind barb tile handler
// ---------------------------------------------------------------------------

async fn handle_wind_barb_tile(
    State(state): State<AppState>,
    Path((model, level, fhour_str, z, x, y_png)): Path<(
        String,
        String,
        String,
        u32,
        u32,
        String,
    )>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let y: u32 = y_png.trim_end_matches(".png").parse().unwrap_or(0);
    let fhour: u32 = fhour_str.trim_start_matches('f').parse().unwrap_or(0);

    let cache_var = format!("wind_barbs_{}", level);
    if let Some(data) = state.cache.get(&model, &cache_var, &level, fhour, z, x, y).await {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
            data,
        )
            .into_response();
    }

    let run = params.get("run").map(|s| s.as_str());
    match wind_barbs::generate_wind_barb_tile(&state.fields, &model, &level, fhour, z, x, y, run).await
    {
        Ok(png_bytes) => {
            state
                .cache
                .put(&model, &cache_var, &level, fhour, z, x, y, png_bytes.clone())
                .await;
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=300"),
                ],
                png_bytes,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Wind barb tile generation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// JSON API handlers
// ---------------------------------------------------------------------------

async fn handle_conditions(Query(params): Query<HashMap<String, String>>) -> Response {
    let lat = params.get("lat").map(|s| s.as_str()).unwrap_or("");
    let lon = params.get("lon").map(|s| s.as_str()).unwrap_or("");
    if lat.is_empty() || lon.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "lat and lon required"})),
        )
            .into_response();
    }
    json_proxy(&["conditions", "--lat", lat, "--lon", lon]).await
}

async fn handle_forecast(Query(params): Query<HashMap<String, String>>) -> Response {
    let lat = params.get("lat").map(|s| s.as_str()).unwrap_or("");
    let lon = params.get("lon").map(|s| s.as_str()).unwrap_or("");
    if lat.is_empty() || lon.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "lat and lon required"})),
        )
            .into_response();
    }
    json_proxy(&["forecast", "--lat", lat, "--lon", lon]).await
}

async fn handle_alerts(Query(params): Query<HashMap<String, String>>) -> Response {
    let lat = params.get("lat").map(|s| s.as_str()).unwrap_or("");
    let lon = params.get("lon").map(|s| s.as_str()).unwrap_or("");
    let state = params.get("state").map(|s| s.as_str()).unwrap_or("");

    if !state.is_empty() {
        return json_proxy(&["alerts", "--state", state]).await;
    }
    if !lat.is_empty() && !lon.is_empty() {
        return json_proxy(&["alerts", "--lat", lat, "--lon", lon]).await;
    }
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "lat/lon or state required"})),
    )
        .into_response()
}

async fn handle_metar(Query(params): Query<HashMap<String, String>>) -> Response {
    let station = params.get("station").map(|s| s.as_str()).unwrap_or("");
    if station.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "station required"})),
        )
            .into_response();
    }
    json_proxy(&["metar", "--station", station]).await
}

async fn handle_radar(Query(params): Query<HashMap<String, String>>) -> Response {
    let site = params.get("site").map(|s| s.as_str()).unwrap_or("");
    let lat = params.get("lat").map(|s| s.as_str()).unwrap_or("");
    let lon = params.get("lon").map(|s| s.as_str()).unwrap_or("");

    if !site.is_empty() {
        return json_proxy(&["radar", "--site", site]).await;
    }
    if !lat.is_empty() && !lon.is_empty() {
        return json_proxy(&["radar", "--lat", lat, "--lon", lon]).await;
    }
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "site or lat/lon required"})),
    )
        .into_response()
}

async fn handle_scan(Query(params): Query<HashMap<String, String>>) -> Response {
    let var = params.get("var").map(|s| s.as_str()).unwrap_or("");
    let mode = params.get("mode").map(|s| s.as_str()).unwrap_or("");
    if var.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "var required"})),
        )
            .into_response();
    }
    let mut args = vec!["scan", "--var", var];
    if !mode.is_empty() {
        args.push("--mode");
        args.push(mode);
    }
    json_proxy(&args).await
}

async fn handle_point(Query(params): Query<HashMap<String, String>>) -> Response {
    let lat = params.get("lat").map(|s| s.as_str()).unwrap_or("");
    let lon = params.get("lon").map(|s| s.as_str()).unwrap_or("");
    let model = params.get("model").map(|s| s.as_str()).unwrap_or("");
    let var = params.get("var").map(|s| s.as_str()).unwrap_or("");

    if lat.is_empty() || lon.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "lat and lon required"})),
        )
            .into_response();
    }
    let mut args = vec!["point", "--lat", lat, "--lon", lon];
    if !model.is_empty() {
        args.push("--model");
        args.push(model);
    }
    if !var.is_empty() {
        args.push("--var");
        args.push(var);
    }
    json_proxy(&args).await
}

/// Fast point-value query that samples from the already-cached field data.
/// No subprocess spawn — this returns in microseconds when the field is cached.
///
/// Query params: lat, lon, model, var, level, fhour
/// Returns JSON: { "value": 42.5, "units": "F", "lat": 35.0, "lon": -97.0 }
async fn handle_value(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let lat: f64 = match params.get("lat").and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, Json(json!({"error":"lat required"}))).into_response(),
    };
    let lon: f64 = match params.get("lon").and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, Json(json!({"error":"lon required"}))).into_response(),
    };
    let model = params.get("model").map(|s| s.as_str()).unwrap_or("hrrr");
    let var = params.get("var").map(|s| s.as_str()).unwrap_or("temperature");
    let level = params.get("level").map(|s| s.as_str()).unwrap_or("2m");
    let fhour: u32 = params.get("fhour").and_then(|s| s.parse().ok()).unwrap_or(0);

    let run = params.get("run").map(|s| s.as_str());

    // Try to get the field from cache (don't trigger a download)
    let key = (
        model.to_lowercase(),
        var.to_lowercase(),
        level.to_lowercase(),
        fhour,
    );
    let field = match state.fields.get_field(&key).await {
        Some(f) => f,
        None => {
            // Field not cached yet — try to load it (this will download if needed)
            match tiles::ensure_field_run(&state.fields, model, var, level, fhour, run).await {
                Ok(f) => f,
                Err(e) => return (StatusCode::NOT_FOUND, Json(json!({"error": e}))).into_response(),
            }
        }
    };

    // Convert lon to grid coordinate system if needed
    let grid_is_0_360 = field.proj.bounding_box().1 >= 0.0 && field.proj.bounding_box().3 > 180.0;
    let mut sample_lon = lon;
    if grid_is_0_360 && sample_lon < 0.0 {
        sample_lon += 360.0;
    }

    let (gi, gj) = field.proj.latlon_to_grid(lat, sample_lon);
    let raw_value = tiles::sample_bilinear(&field.values, field.nx, field.ny, gi, gj);

    match raw_value {
        Some(val) if val.is_finite() && val.abs() < 1e15 && val > -900.0 => {
            // Convert to display units based on variable
            let (display_val, units) = convert_to_display(var, level, val);
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-cache")],
                Json(json!({
                    "value": (display_val * 100.0).round() / 100.0,
                    "raw": (val * 100.0).round() / 100.0,
                    "units": units,
                    "lat": lat,
                    "lon": lon,
                })),
            )
                .into_response()
        }
        _ => (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-cache")],
            Json(json!({
                "value": null,
                "units": "",
                "lat": lat,
                "lon": lon,
                "error": "no data at this point"
            })),
        )
            .into_response(),
    }
}

/// Convert raw GRIB2 values to human-readable display units.
fn convert_to_display(var: &str, level: &str, val: f64) -> (f64, &'static str) {
    match var.to_lowercase().as_str() {
        "temperature" | "temp" | "t" => {
            // GRIB2 temps are in Kelvin → convert to Fahrenheit
            let f = (val - 273.15) * 9.0 / 5.0 + 32.0;
            (f, "°F")
        }
        "dewpoint" | "td" | "dew" => {
            let f = (val - 273.15) * 9.0 / 5.0 + 32.0;
            (f, "°F")
        }
        "wind" | "wind_speed" | "wspd" => {
            // m/s → knots
            (val * 1.94384, "kt")
        }
        "gust" => {
            (val * 1.94384, "kt")
        }
        "mslp" => {
            // Pa → mb (hPa)
            if val > 10000.0 { (val / 100.0, "mb") } else { (val, "mb") }
        }
        "refc" | "reflectivity" | "refl" => (val, "dBZ"),
        "cape" => (val, "J/kg"),
        "cin" => (val, "J/kg"),
        "rh" | "relative_humidity" => (val, "%"),
        "helicity" | "hlcy" | "srh" => (val, "m²/s²"),
        "mxuphl" | "updraft_helicity" | "uh" => (val, "m²/s²"),
        "height" | "hgt" => {
            // Geopotential height in meters → decameters for upper air
            if level.contains("mb") || level.contains("hPa") {
                (val / 10.0, "dam")
            } else {
                (val, "m")
            }
        }
        "precip" | "precipitation" | "apcp" => {
            // kg/m² ≈ mm → inches
            (val / 25.4, "in")
        }
        "pwat" | "precipitable_water" => (val, "mm"),
        "snow" | "snowfall" | "weasd" => {
            // kg/m² → inches (rough: 1 kg/m² ≈ 0.04 in of snow)
            (val / 25.4, "in")
        }
        "cloud" | "cloud_cover" | "tcc" | "tcdc" => (val, "%"),
        "visibility" | "vis" => (val / 1609.34, "mi"),
        _ => (val, ""),
    }
}

// ---------------------------------------------------------------------------
// Available model runs endpoint
// ---------------------------------------------------------------------------

/// Probe NOMADS for available model runs, returning which ones exist.
///
/// For HRRR/RAP (hourly cycles): probes the last 24 hours.
/// For GFS/NAM (6-hourly cycles): probes the last 4 cycles.
async fn handle_runs(Path(model): Path<String>) -> Response {
    let model_lower = model.to_lowercase();

    // Determine which cycles to probe
    let valid_hours: Vec<u32> = match model_lower.as_str() {
        "hrrr" | "rap" => (0..24).collect(),
        "gfs" | "nam" => vec![0, 6, 12, 18],
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Unknown model '{}'. Supported: hrrr, gfs, nam, rap", model)})),
            )
                .into_response();
        }
    };

    let max_lookback: i64 = match model_lower.as_str() {
        "hrrr" | "rap" => 24,
        _ => 48, // 4 cycles of 6-hourly = up to 24h, but give some buffer
    };

    let model_clone = model_lower.clone();
    let result = tokio::task::spawn_blocking(move || {
        probe_available_runs(&model_clone, &valid_hours, max_lookback)
    })
    .await;

    match result {
        Ok(runs) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "public, max-age=120"),
            ],
            serde_json::to_string(&json!({
                "model": model_lower,
                "runs": runs,
            }))
            .unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Task error: {}", e)})),
        )
            .into_response(),
    }
}

/// Probe NOMADS .idx files to find available runs.
fn probe_available_runs(model: &str, valid_hours: &[u32], max_lookback: i64) -> Vec<serde_json::Value> {
    use chrono::{Utc, TimeDelta};
    use rustmet_core::download::DownloadClient;
    use rustmet_core::models::{HrrrConfig, GfsConfig, NamConfig, RapConfig};

    let client = match DownloadClient::new() {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let now = Utc::now();
    let mut runs = Vec::new();
    let max_runs = match model {
        "hrrr" | "rap" => 24,
        _ => 6,
    };

    for lookback in 0..max_lookback {
        if runs.len() >= max_runs {
            break;
        }
        let candidate = now - TimeDelta::hours(lookback);
        let hour: u32 = candidate.format("%H").to_string().parse().unwrap();

        if !valid_hours.contains(&hour) {
            continue;
        }

        let date_str = candidate.format("%Y%m%d").to_string();

        let idx_url = match model {
            "hrrr" => format!("{}.idx", HrrrConfig::nomads_url(&date_str, hour, "sfc", 0)),
            "gfs" => format!("{}.idx", GfsConfig::nomads_url(&date_str, hour, 0)),
            "nam" => format!("{}.idx", NamConfig::nomads_url(&date_str, hour, 0)),
            "rap" => format!("{}.idx", RapConfig::nomads_url(&date_str, hour, 0)),
            _ => continue,
        };

        if client.head_ok(&idx_url) {
            runs.push(json!({
                "date": date_str,
                "hour": hour,
                "run": format!("{}/{:02}z", date_str, hour),
            }));
        }
    }

    runs
}

async fn handle_severe(Query(params): Query<HashMap<String, String>>) -> Response {
    let lat = params.get("lat").map(|s| s.as_str()).unwrap_or("");
    let lon = params.get("lon").map(|s| s.as_str()).unwrap_or("");
    if lat.is_empty() || lon.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "lat and lon required"})),
        )
            .into_response();
    }
    json_proxy(&["severe", "--lat", lat, "--lon", lon]).await
}

async fn handle_radar_image(Query(params): Query<HashMap<String, String>>) -> Response {
    let site = params.get("site").map(|s| s.as_str()).unwrap_or("");
    let raw = params
        .get("raw")
        .map(|s| s == "true" || s == "1")
        .unwrap_or(false);

    if site.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "site required"})),
        )
            .into_response();
    }
    image_or_json_proxy(&["radar", "--site", site, "--image"], raw).await
}

async fn handle_model_image(Query(params): Query<HashMap<String, String>>) -> Response {
    let var = params.get("var").map(|s| s.as_str()).unwrap_or("");
    let raw = params
        .get("raw")
        .map(|s| s == "true" || s == "1")
        .unwrap_or(false);

    if var.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "var required"})),
        )
            .into_response();
    }
    image_or_json_proxy(&["model-image", "--var", var], raw).await
}

// ---------------------------------------------------------------------------
// Sounding handler — SkewT-LogP SVG or JSON
// ---------------------------------------------------------------------------

/// Serve the demo page from the `demo/` directory adjacent to the binary or repo root.
async fn serve_demo_page() -> Response {
    // Try multiple locations for demo/index.html
    let candidates = [
        std::path::PathBuf::from("demo/index.html"),
        std::path::PathBuf::from("../demo/index.html"),
        std::path::PathBuf::from("../../demo/index.html"),
    ];
    for path in &candidates {
        if let Ok(contents) = tokio::fs::read_to_string(path).await {
            // Replace localhost API URL with relative path so it works from any origin
            let html = contents.replace("http://localhost:8080", "");
            return Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(axum::body::Body::from(html))
                .unwrap();
        }
    }
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(axum::body::Body::from("demo/index.html not found"))
        .unwrap()
}

async fn handle_sounding(
    State(state): State<AppState>,
    Path(station): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = params.get("format").map(|s| s.as_str()).unwrap_or("svg");
    let hour: Option<u32> = params.get("hour").and_then(|s| s.parse().ok());
    let width: u32 = params.get("width").and_then(|s| s.parse().ok()).unwrap_or(800);
    let height: u32 = params.get("height").and_then(|s| s.parse().ok()).unwrap_or(800);

    // Normalize station: strip leading K for ICAO 4-letter codes
    let station_clean = if station.len() == 4 && station.starts_with('K') {
        station[1..].to_string()
    } else {
        station.to_uppercase()
    };

    match sounding::fetch_sounding_cached(&state.sounding_cache, &station_clean, hour).await {
        Ok(snd) => {
            if format == "json" {
                let json_val = sounding::sounding_to_json(&snd);
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "application/json"),
                        (header::CACHE_CONTROL, "public, max-age=300"),
                    ],
                    serde_json::to_string_pretty(&json_val).unwrap_or_default(),
                )
                    .into_response()
            } else {
                let svg = sounding::render_skewt_svg(&snd, width, height);
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "image/svg+xml"),
                        (header::CACHE_CONTROL, "public, max-age=300"),
                    ],
                    svg,
                )
                    .into_response()
            }
        }
        Err(e) => {
            tracing::error!("Sounding fetch failed for {}: {}", station, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e, "station": station})),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Surface observation tile handler
// ---------------------------------------------------------------------------

async fn handle_surface_tile(
    State(state): State<AppState>,
    Path((z, x, y_png)): Path<(u32, u32, String)>,
) -> Response {
    let y: u32 = y_png.trim_end_matches(".png").parse().unwrap_or(0);

    // Check rendered tile cache
    let cache_model = "surface_obs";
    if let Some(data) = state
        .cache
        .get(cache_model, "metar", "sfc", 0, z, x, y)
        .await
    {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
            data,
        )
            .into_response();
    }

    match surface_obs::generate_surface_tile(&state.metar_cache, z, x, y).await {
        Ok(png_bytes) => {
            state
                .cache
                .put(cache_model, "metar", "sfc", 0, z, x, y, png_bytes.clone())
                .await;
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "public, max-age=300"),
                ],
                png_bytes,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Surface obs tile generation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port = std::env::args()
        .skip_while(|a| a != "--port")
        .nth(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    // Shared state: tile cache (512MB, 5 min TTL), event hub, and field cache
    let cache = Arc::new(TileCache::new(512, 300));
    let hub = Arc::new(EventHub::new(512));
    let fields = Arc::new(FieldCache::new());
    let radar = Arc::new(RadarCache::new());
    let sounding_cache = Arc::new(SoundingCache::new(3600)); // 1 hour TTL
    let metar_cache = Arc::new(MetarCache::new());
    let state = AppState {
        cache: cache.clone(),
        hub: hub.clone(),
        fields,
        radar,
        sounding_cache,
        metar_cache,
    };

    // Spawn background model run poller (checks every 60s)
    let poller_hub = hub.clone();
    let wx_pro_path = find_wx_pro();
    tokio::spawn(async move {
        sse::model_run_poller(poller_hub, std::time::Duration::from_secs(60), wx_pro_path).await;
    });

    let app = Router::new()
        // Tile endpoint (uses cache)
        .route(
            "/tiles/:model/:var/:level/:fhour_str/:z/:x/:y_png",
            get(handle_tile),
        )
        // Contour tile endpoint (transparent overlay with contour lines)
        .route(
            "/tiles/contour/:model/:var/:level/:fhour_str/:z/:x/:y_png",
            get(handle_contour_tile),
        )
        // Surface observation tile endpoint (METAR station models)
        .route(
            "/tiles/surface/:z/:x/:y_png",
            get(handle_surface_tile),
        )
        // Radar tile endpoint (NEXRAD Level 2)
        .route(
            "/tiles/radar/:site/:product/:z/:x/:y_png",
            get(handle_radar_tile),
        )
        // Radar scan listing endpoint (available scans from NOMADS)
        .route("/api/radar/scans/:site", get(handle_radar_scans))
        // Wind barb tile endpoint (transparent overlay with wind barbs)
        .route(
            "/tiles/wind/:model/:level/:fhour_str/:z/:x/:y_png",
            get(handle_wind_barb_tile),
        )
        // Legend endpoint (colorbar PNG)
        .route("/api/legend/:var", get(handle_legend))
        // Available model runs endpoint
        .route("/api/runs/:model", get(handle_runs))
        // JSON API
        .route("/api/conditions", get(handle_conditions))
        .route("/api/forecast", get(handle_forecast))
        .route("/api/alerts", get(handle_alerts))
        .route("/api/metar", get(handle_metar))
        .route("/api/radar", get(handle_radar))
        .route("/api/scan", get(handle_scan))
        .route("/api/point", get(handle_point))
        // Fast point-value readout (samples cached field data, no subprocess)
        .route("/api/value", get(handle_value))
        .route("/api/severe", get(handle_severe))
        .route("/api/radar-image", get(handle_radar_image))
        .route("/api/model-image", get(handle_model_image))
        // Sounding SkewT-LogP endpoint
        .route("/api/sounding/:station", get(handle_sounding))
        // SSE event stream
        .route("/events", get({
            let hub = hub.clone();
            move |query| sse::handle_events(query, hub)
        }))
        // Cache management
        .route("/api/cache/stats", get({
            let cache = cache.clone();
            move || async move { Json(cache.stats().await) }
        }))
        .route("/api/cache/clear", get({
            let cache = cache.clone();
            move || async move {
                cache.clear().await;
                Json(json!({"status": "cleared"}))
            }
        }))
        // Status: real model run times + radar data age
        .route("/api/status", get({
            let fields = state.fields.clone();
            let radar = state.radar.clone();
            move || {
                let fields = fields.clone();
                let radar = radar.clone();
                async move {
                    let runs = fields.get_all_runs().await;
                    let radar_status = radar.get_status().await;

                    let mut models = serde_json::Map::new();
                    for (model, date, hour) in &runs {
                        models.insert(model.clone(), json!({
                            "date": date,
                            "hour": hour,
                            "init": format!("{} {:02}z", date, hour),
                        }));
                    }

                    let mut radar_map = serde_json::Map::new();
                    for (site, age_secs) in &radar_status {
                        radar_map.insert(site.clone(), json!({
                            "age_secs": age_secs,
                        }));
                    }

                    Json(json!({
                        "models": models,
                        "radar": radar_map,
                    }))
                }
            }
        }))
        // Health
        .route(
            "/health",
            get(|| async { Json(json!({"status": "ok"})) }),
        )
        // Serve demo page at root
        .route("/", get(serve_demo_page))
        .with_state(state)
        .layer(CorsLayer::permissive());

    println!("wx-server v0.1.0 starting on http://0.0.0.0:{}", port);
    println!("  Demo:   http://localhost:{}/", port);
    println!("  Tiles:  GET /tiles/hrrr/cape/surface/f00/5/8/12.png?style=nws");
    println!("  Legend: GET /api/legend/temperature?style=nws&vmin=-40&vmax=120");
    println!("  Contour:GET /tiles/contour/gfs/hgt/500mb/f00/4/3/5.png?interval=60");
    println!("  Surface:GET /tiles/surface/7/28/49.png");
    println!("  Radar:  GET /tiles/radar/KTLX/ref/7/28/49.png?scan=FILENAME");
    println!("  Scans:  GET /api/radar/scans/KTLX");
    println!("  Wind:   GET /tiles/wind/hrrr/10m/f00/5/8/12.png");
    println!("  Sndg:   GET /api/sounding/OUN");
    println!("  API:    GET /api/conditions?lat=35&lon=-97");
    println!("  Value:  GET /api/value?lat=35&lon=-97&model=hrrr&var=temperature&level=2m&fhour=0");
    println!("  SSE:    GET /events?types=model_run,alert");
    println!("  Cache:  GET /api/cache/stats");
    println!("  Runs:   GET /api/runs/hrrr");
    println!("  Status: GET /api/status");
    println!("  Health: GET /health");

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server error");
}
