mod cache;
mod sse;

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
use sse::EventHub;

/// Shared application state
#[derive(Clone)]
struct AppState {
    cache: Arc<TileCache>,
    hub: Arc<EventHub>,
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
) -> Response {
    // Parse "12.png" -> 12
    let y: u32 = y_png.trim_end_matches(".png").parse().unwrap_or(0);
    let fhour: u32 = fhour_str.trim_start_matches('f').parse().unwrap_or(0);

    // Check cache first
    if let Some(data) = state.cache.get(&model, &var, &level, fhour, z, x, y).await {
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

    let fhour_s = fhour.to_string();
    let z_s = z.to_string();
    let x_s = x.to_string();
    let y_s = y.to_string();

    let output = run_wx_pro(&[
        "tiles", "--model", &model, "--var", &var, "--level", &level, "--fhour", &fhour_s, "--z",
        &z_s, "--x", &x_s, "--y", &y_s,
    ])
    .await;

    match output {
        Ok(json_str) => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(path) = v.get("tile_path").and_then(|p| p.as_str()) {
                    if let Ok(data) = tokio::fs::read(path).await {
                        // Cache the tile
                        state.cache.put(&model, &var, &level, fhour, z, x, y, data.clone()).await;
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
                }
            }
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate tile").into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
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

    // Shared state: tile cache (512MB, 5 min TTL) and event hub
    let cache = Arc::new(TileCache::new(512, 300));
    let hub = Arc::new(EventHub::new(512));
    let state = AppState {
        cache: cache.clone(),
        hub: hub.clone(),
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
        // JSON API
        .route("/api/conditions", get(handle_conditions))
        .route("/api/forecast", get(handle_forecast))
        .route("/api/alerts", get(handle_alerts))
        .route("/api/metar", get(handle_metar))
        .route("/api/radar", get(handle_radar))
        .route("/api/scan", get(handle_scan))
        .route("/api/point", get(handle_point))
        .route("/api/severe", get(handle_severe))
        .route("/api/radar-image", get(handle_radar_image))
        .route("/api/model-image", get(handle_model_image))
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
        // Health
        .route(
            "/health",
            get(|| async { Json(json!({"status": "ok"})) }),
        )
        .with_state(state)
        .layer(CorsLayer::permissive());

    println!("wx-server v0.1.0 starting on http://0.0.0.0:{}", port);
    println!("  Tiles:  GET /tiles/hrrr/cape/surface/f00/5/8/12.png");
    println!("  API:    GET /api/conditions?lat=35&lon=-97");
    println!("  SSE:    GET /events?types=model_run,alert");
    println!("  Cache:  GET /api/cache/stats");
    println!("  Health: GET /health");

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server error");
}
