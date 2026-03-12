//! NEXRAD Level 2 radar tile renderer.
//!
//! Converts polar radar data into 256x256 web map tiles (PNG).
//! Downloads the latest Level 2 volume scan from AWS S3, caches
//! the decoded sweep data, and renders tiles on demand using
//! polar-to-cartesian conversion.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

use rustmet_core::download::DownloadClient;
use rustmet_core::render::colormap::{interpolate_color, ColorStop};
use rustmet_core::render::encode::encode_png;
use wx_radar::level2::Level2File;
use wx_radar::products::RadarProduct;
use wx_radar::sites::find_site;

const TILE_SIZE: usize = 256;
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Maximum radar range in km for reflectivity (460 km for super-res).
const MAX_RANGE_KM: f64 = 460.0;

/// Cache TTL for decoded radar volumes (2 minutes).
const RADAR_TTL: Duration = Duration::from_secs(120);

// ── Velocity colormap: green-white-red diverging ───────────────────

pub static VELOCITY: &[ColorStop] = &[
    (0.000, 0x00, 0x78, 0x00), // dark green (strong inbound)
    (0.125, 0x00, 0xA0, 0x00),
    (0.200, 0x00, 0xC8, 0x00), // green
    (0.300, 0x00, 0xEB, 0x00), // bright green
    (0.400, 0x90, 0xF0, 0x90), // light green
    (0.450, 0xD0, 0xF0, 0xD0), // very light green
    (0.500, 0xF0, 0xF0, 0xF0), // near-white (zero velocity)
    (0.550, 0xF0, 0xD0, 0xD0), // very light red
    (0.600, 0xF0, 0x90, 0x90), // light red
    (0.700, 0xEB, 0x00, 0x00), // bright red
    (0.800, 0xC8, 0x00, 0x00), // red
    (0.875, 0xA0, 0x00, 0x00),
    (1.000, 0x78, 0x00, 0x00), // dark red (strong outbound)
];

// ── Cached radar data ──────────────────────────────────────────────

/// A single decoded sweep ready for tile rendering.
/// Gates stored in polar form: radials x gates.
struct CachedSweep {
    /// Azimuth angle (degrees) for each radial, sorted.
    azimuths: Vec<f32>,
    /// Gate values for each radial.
    gates: Vec<Vec<f32>>,
    /// First gate range in meters.
    first_gate_range_m: f32,
    /// Gate spacing in meters.
    gate_size_m: f32,
    /// Number of gates per radial.
    gate_count: usize,
}

struct CachedRadar {
    /// Map from product short name (REF, VEL, etc.) to the lowest sweep.
    sweeps: HashMap<String, CachedSweep>,
    /// Radar site latitude.
    lat: f64,
    /// Radar site longitude.
    lon: f64,
    /// When this was fetched.
    fetched_at: Instant,
}

type RadarKey = String; // site ID, e.g. "KTLX" or "KTLX::filename.bz2"

pub struct RadarCache {
    data: RwLock<HashMap<RadarKey, Arc<CachedRadar>>>,
    inflight: Mutex<HashMap<RadarKey, Arc<tokio::sync::Notify>>>,
}

/// A single radar scan entry from the NOMADS directory listing.
#[derive(Clone, serde::Serialize)]
pub struct ScanEntry {
    pub filename: String,
    pub timestamp: String, // ISO-like from filename, e.g. "2026-03-12T02:09:06Z"
    pub modified: String,  // last-modified from directory listing
}

/// Parse a NOMADS directory listing and return scan entries for a given site.
pub fn list_nomads_scans(site: &str) -> Result<Vec<ScanEntry>, String> {
    let client = DownloadClient::new().map_err(|e| format!("HTTP client error: {}", e))?;
    let site_upper = site.to_uppercase();

    let list_url = format!(
        "https://nomads.ncep.noaa.gov/pub/data/nccf/radar/nexrad_level2/{}/",
        site_upper
    );

    let html = client
        .get_text(&list_url)
        .map_err(|e| format!("Failed to list NOMADS radar dir: {}", e))?;

    // Parse the HTML directory listing.
    // Typical line: <a href="KTLX20260312_020906_V06">KTLX20260312_020906_V06</a>  2026-03-12 02:14  5.3M
    // Or with .gz/.bz2 suffix.
    let mut entries: Vec<ScanEntry> = Vec::new();

    for chunk in html.split("href=\"").skip(1) {
        let filename = match chunk.split('"').next() {
            Some(f) => f,
            None => continue,
        };
        // Filter to actual radar files for this site
        if filename.contains('/') {
            continue;
        }
        if !filename.contains(&site_upper) {
            continue;
        }
        // Must look like a radar file (has underscore-separated timestamp)
        if !(filename.ends_with(".bz2") || filename.ends_with(".gz") || filename.contains("_V0")) {
            continue;
        }

        // Try to extract timestamp from filename.
        // Formats: KTLX20260312_020906_V06 or KTLX_20260312_020906.bz2
        let timestamp = parse_scan_timestamp(filename, &site_upper);

        // Try to extract modification time from the HTML after the closing </a> tag.
        // The text after the link typically looks like: "</a>  2026-03-12 02:14  5.3M"
        let after_link = chunk.split("</a>").nth(1).unwrap_or("").trim();
        // First token pair should be date and time
        let parts: Vec<&str> = after_link.split_whitespace().collect();
        let modified = if parts.len() >= 2 {
            format!("{} {}", parts[0], parts[1])
        } else {
            String::new()
        };

        entries.push(ScanEntry {
            filename: filename.to_string(),
            timestamp,
            modified,
        });
    }

    entries.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(entries)
}

/// Extract an ISO timestamp from a NOMADS radar filename.
fn parse_scan_timestamp(filename: &str, site: &str) -> String {
    // Strip site prefix and any leading underscore
    let rest = filename.trim_start_matches(site).trim_start_matches('_');
    // rest might be: 20260312_020906_V06 or 20260312_020906.bz2
    // Extract first 15 chars which should be YYYYMMDD_HHMMSS
    if rest.len() >= 15 {
        let date_part = &rest[0..8];
        let time_part = &rest[9..15];
        if date_part.chars().all(|c| c.is_ascii_digit())
            && time_part.chars().all(|c| c.is_ascii_digit())
        {
            return format!(
                "{}-{}-{}T{}:{}:{}Z",
                &date_part[0..4],
                &date_part[4..6],
                &date_part[6..8],
                &time_part[0..2],
                &time_part[2..4],
                &time_part[4..6]
            );
        }
    }
    String::new()
}

impl RadarCache {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    async fn get(&self, site: &str) -> Option<Arc<CachedRadar>> {
        let data = self.data.read().await;
        if let Some(cached) = data.get(&site.to_uppercase()) {
            if cached.fetched_at.elapsed() < RADAR_TTL {
                return Some(cached.clone());
            }
        }
        None
    }

    /// Return status info for all cached radar sites: (site, age_secs).
    pub async fn get_status(&self) -> Vec<(String, u64)> {
        let data = self.data.read().await;
        data.iter()
            .filter(|(_, v)| v.fetched_at.elapsed() < RADAR_TTL)
            .map(|(site, v)| (site.clone(), v.fetched_at.elapsed().as_secs()))
            .collect()
    }

    async fn put(&self, site: &str, radar: CachedRadar) -> Arc<CachedRadar> {
        let arc = Arc::new(radar);
        let mut data = self.data.write().await;
        // Evict expired entries
        data.retain(|_, v| v.fetched_at.elapsed() < RADAR_TTL);
        data.insert(site.to_uppercase(), arc.clone());
        arc
    }
}

// ── Tile math (same as tiles.rs) ───────────────────────────────────

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

fn mercator_lat(lat_max: f64, lat_min: f64, t: f64) -> f64 {
    let y_max = lat_max.to_radians().tan().asinh();
    let y_min = lat_min.to_radians().tan().asinh();
    let y = y_max + t * (y_min - y_max);
    y.sinh().atan().to_degrees()
}

// ── Polar-to-cartesian math ────────────────────────────────────────

/// Compute azimuth (degrees CW from north, 0-360) and range (km) from
/// a radar site to a target lat/lon.
fn latlon_to_az_range(radar_lat: f64, radar_lon: f64, target_lat: f64, target_lon: f64) -> (f64, f64) {
    let rlat = radar_lat.to_radians();
    let tlat = target_lat.to_radians();
    let dlat = (target_lat - radar_lat).to_radians();
    let dlon = (target_lon - radar_lon).to_radians();

    // Haversine distance
    let a = (dlat / 2.0).sin().powi(2) + rlat.cos() * tlat.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    let range_km = EARTH_RADIUS_KM * c;

    // Bearing (azimuth)
    let y = dlon.sin() * tlat.cos();
    let x = rlat.cos() * tlat.sin() - rlat.sin() * tlat.cos() * dlon.cos();
    let bearing = y.atan2(x).to_degrees();
    let azimuth = (bearing + 360.0) % 360.0;

    (azimuth, range_km)
}

/// Look up the gate value from a cached sweep using nearest-neighbor.
fn sample_sweep(sweep: &CachedSweep, azimuth_deg: f64, range_km: f64) -> Option<f32> {
    let range_m = range_km * 1000.0;
    let first = sweep.first_gate_range_m as f64;
    let size = sweep.gate_size_m as f64;

    // Gate index
    let gate_idx = ((range_m - first) / size).round() as isize;
    if gate_idx < 0 || gate_idx >= sweep.gate_count as isize {
        return None;
    }
    let gate_idx = gate_idx as usize;

    // Find nearest radial by azimuth (binary search on sorted azimuths)
    let az = azimuth_deg as f32;
    let radial_idx = match sweep
        .azimuths
        .binary_search_by(|a| a.partial_cmp(&az).unwrap_or(std::cmp::Ordering::Equal))
    {
        Ok(i) => i,
        Err(i) => {
            // Pick the closer neighbor, wrapping around
            let n = sweep.azimuths.len();
            if n == 0 {
                return None;
            }
            let prev = if i == 0 { n - 1 } else { i - 1 };
            let next = i % n;
            let d_prev = az_diff(az, sweep.azimuths[prev]);
            let d_next = az_diff(az, sweep.azimuths[next]);
            if d_prev < d_next {
                prev
            } else {
                next
            }
        }
    };

    let val = sweep.gates[radial_idx][gate_idx];
    if val.is_nan() {
        None
    } else {
        Some(val)
    }
}

/// Absolute azimuth difference accounting for 360/0 wrap.
fn az_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

// ── NOMADS download ──────────────────────────────────────────────

/// Download a Level 2 file for a given site from NOMADS.
/// If `scan_file` is Some, download that specific file; otherwise pick the
/// second-to-last (the latest fully-uploaded scan).
fn download_l2(site: &str, scan_file: Option<&str>) -> Result<Vec<u8>, String> {
    let client = DownloadClient::new().map_err(|e| format!("HTTP client error: {}", e))?;
    let site_upper = site.to_uppercase();

    let chosen_file = if let Some(f) = scan_file {
        f.to_string()
    } else {
        // NOMADS keeps recent Level 2 files at this path
        let list_url = format!(
            "https://nomads.ncep.noaa.gov/pub/data/nccf/radar/nexrad_level2/{}/",
            site_upper
        );

        eprintln!("[radar_tiles] Listing: {}", list_url);

        let html = client
            .get_text(&list_url)
            .map_err(|e| format!("Failed to list NOMADS radar dir: {}", e))?;

        // Parse href="KTLX_20260312_020906.bz2" from the HTML directory listing
        let mut files: Vec<&str> = html
            .split("href=\"")
            .skip(1)
            .filter_map(|s| s.split('"').next())
            .filter(|f| f.ends_with(".bz2") || f.ends_with(".gz") || f.contains(&site_upper))
            .filter(|f| !f.contains('/')) // skip parent dir links
            .collect();

        files.sort(); // chronological by filename convention

        if files.is_empty() {
            return Err(format!("No Level 2 files found for {} on NOMADS", site_upper));
        }

        // Use second-to-last file: the latest may still be uploading (partial)
        let latest_file = if files.len() >= 2 {
            &files[files.len() - 2]
        } else {
            files.last().unwrap()
        };
        latest_file.to_string()
    };

    let data_url = format!(
        "https://nomads.ncep.noaa.gov/pub/data/nccf/radar/nexrad_level2/{}/{}",
        site_upper, chosen_file
    );
    eprintln!("[radar_tiles] Downloading: {}", data_url);

    let data = client
        .get_bytes(&data_url)
        .map_err(|e| format!("Download failed: {}", e))?;

    Ok(data)
}

/// Parse a Level 2 file and extract the lowest elevation sweep for each product.
fn parse_to_cached_radar(
    raw_data: &[u8],
    site_lat: f64,
    site_lon: f64,
) -> Result<CachedRadar, String> {
    let l2 = Level2File::parse(raw_data)?;

    eprintln!(
        "[radar_tiles] Parsed {} sweeps, station={}, time={}",
        l2.sweeps.len(),
        l2.station_id,
        l2.timestamp_string()
    );

    let mut sweeps: HashMap<String, CachedSweep> = HashMap::new();

    // For each product, find the lowest elevation sweep that has data
    let products_of_interest = [
        RadarProduct::Reflectivity,
        RadarProduct::Velocity,
    ];

    for target_product in &products_of_interest {
        let short = target_product.short_name().to_string();

        // Find the lowest-elevation sweep that has this product
        for sweep in &l2.sweeps {
            let has_product = sweep.radials.iter().any(|r| {
                r.moments.iter().any(|m| m.product == *target_product)
            });
            if !has_product {
                continue;
            }

            // Extract radials sorted by azimuth
            let mut radial_data: Vec<(f32, Vec<f32>, f32, u16, u16)> = Vec::new();

            for radial in &sweep.radials {
                for moment in &radial.moments {
                    if moment.product == *target_product {
                        radial_data.push((
                            radial.azimuth,
                            moment.data.clone(),
                            moment.first_gate_range as f32,
                            moment.gate_size,
                            moment.gate_count,
                        ));
                        break;
                    }
                }
            }

            if radial_data.is_empty() {
                continue;
            }

            radial_data.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            let first_gate_range_m = radial_data[0].2;
            let gate_size_m = radial_data[0].3 as f32;
            let gate_count = radial_data[0].4 as usize;

            let azimuths: Vec<f32> = radial_data.iter().map(|r| r.0).collect();
            let gates: Vec<Vec<f32>> = radial_data.into_iter().map(|r| r.1).collect();

            sweeps.insert(
                short.clone(),
                CachedSweep {
                    azimuths,
                    gates,
                    first_gate_range_m,
                    gate_size_m,
                    gate_count,
                },
            );

            break; // Use the lowest sweep
        }
    }

    if sweeps.is_empty() {
        return Err("No reflectivity or velocity data found in Level 2 file".to_string());
    }

    Ok(CachedRadar {
        sweeps,
        lat: site_lat,
        lon: site_lon,
        fetched_at: Instant::now(),
    })
}

// ── Tile rendering ─────────────────────────────────────────────────

fn render_radar_tile(
    radar: &CachedRadar,
    product: &str,
    z: u32,
    x: u32,
    y: u32,
) -> Result<Vec<u8>, String> {
    let product_key = match product.to_lowercase().as_str() {
        "reflectivity" | "ref" => "REF",
        "velocity" | "vel" => "VEL",
        _ => return Err(format!("Unknown radar product: {}", product)),
    };

    let sweep = radar
        .sweeps
        .get(product_key)
        .ok_or_else(|| format!("Product {} not available in cached radar data", product_key))?;

    let (vmin, vmax, colormap): (f64, f64, &[ColorStop]) = match product_key {
        "REF" => (-10.0, 75.0, rustmet_core::render::colormap::NWS_REFLECTIVITY),
        "VEL" => (-64.0, 64.0, VELOCITY),
        _ => unreachable!(),
    };

    let (lat_min, lon_min, lat_max, lon_max) = tile_bounds(z, x, y);

    // Quick reject: if tile is entirely outside radar range, return transparent
    let (_, center_range) = latlon_to_az_range(
        radar.lat,
        radar.lon,
        (lat_min + lat_max) / 2.0,
        (lon_min + lon_max) / 2.0,
    );
    // Tile diagonal is roughly the tile span; if center is way beyond range, skip
    let tile_span_km = {
        let dlat = (lat_max - lat_min).to_radians() * EARTH_RADIUS_KM;
        let dlon = (lon_max - lon_min).to_radians()
            * EARTH_RADIUS_KM
            * ((lat_min + lat_max) / 2.0).to_radians().cos();
        (dlat * dlat + dlon * dlon).sqrt()
    };
    if center_range > MAX_RANGE_KM + tile_span_km {
        // All transparent
        let pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];
        return encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32)
            .map_err(|e| format!("PNG encode error: {}", e));
    }

    let mut pixels = vec![0u8; TILE_SIZE * TILE_SIZE * 4];

    for py in 0..TILE_SIZE {
        for px in 0..TILE_SIZE {
            let tx = (px as f64 + 0.5) / TILE_SIZE as f64;
            let ty = (py as f64 + 0.5) / TILE_SIZE as f64;
            let lon = lon_min + tx * (lon_max - lon_min);
            let lat = mercator_lat(lat_max, lat_min, ty);

            let (az, range_km) = latlon_to_az_range(radar.lat, radar.lon, lat, lon);

            if range_km > MAX_RANGE_KM {
                continue;
            }

            if let Some(val) = sample_sweep(sweep, az, range_km) {
                let fval = val as f64;

                // For reflectivity, skip very low values (no echo)
                if product_key == "REF" && fval < 5.0 {
                    continue;
                }

                let norm = ((fval - vmin) / (vmax - vmin)).clamp(0.0, 1.0);
                let (r, g, b) = interpolate_color(colormap, norm);
                let idx = (py * TILE_SIZE + px) * 4;
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
                pixels[idx + 3] = 200; // semi-transparent for map overlay
            }
        }
    }

    encode_png(&pixels, TILE_SIZE as u32, TILE_SIZE as u32)
        .map_err(|e| format!("PNG encode error: {}", e))
}

// ── Public API ─────────────────────────────────────────────────────

pub async fn generate_radar_tile(
    cache: &RadarCache,
    site: &str,
    product: &str,
    z: u32,
    x: u32,
    y: u32,
    scan_file: Option<&str>,
) -> Result<Vec<u8>, String> {
    let site_upper = site.to_uppercase();

    // Cache key: "KTLX" for latest, "KTLX::filename" for specific scan
    let cache_key = if let Some(f) = scan_file {
        format!("{}::{}", site_upper, f)
    } else {
        site_upper.clone()
    };

    // Fast path: cached data
    if let Some(radar) = cache.get(&cache_key).await {
        let product = product.to_string();
        let png_bytes = tokio::task::spawn_blocking(move || {
            render_radar_tile(&radar, &product, z, x, y)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))??;
        return Ok(png_bytes);
    }

    // Slow path: download. Use inflight lock to prevent thundering herd.
    let notify = {
        let mut inflight = cache.inflight.lock().await;
        if let Some(notify) = inflight.get(&cache_key) {
            let notify = notify.clone();
            drop(inflight);
            notify.notified().await;
            // Should now be cached
            if let Some(radar) = cache.get(&cache_key).await {
                let product = product.to_string();
                let png_bytes = tokio::task::spawn_blocking(move || {
                    render_radar_tile(&radar, &product, z, x, y)
                })
                .await
                .map_err(|e| format!("Task join error: {}", e))??;
                return Ok(png_bytes);
            }
            return Err("Radar download completed but not found in cache".to_string());
        }
        let notify = Arc::new(tokio::sync::Notify::new());
        inflight.insert(cache_key.clone(), notify.clone());
        notify
    };

    // Look up site coordinates
    let radar_site = find_site(&site_upper)
        .ok_or_else(|| format!("Unknown radar site: {}", site_upper))?;

    let site_for_dl = site_upper.clone();
    let site_lat = radar_site.lat;
    let site_lon = radar_site.lon;
    let scan_owned = scan_file.map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        let raw_data = download_l2(&site_for_dl, scan_owned.as_deref())?;
        eprintln!(
            "[radar_tiles] Downloaded {} bytes for {}",
            raw_data.len(),
            site_for_dl
        );
        parse_to_cached_radar(&raw_data, site_lat, site_lon)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    // Remove inflight entry and notify waiters
    {
        let mut inflight = cache.inflight.lock().await;
        inflight.remove(&cache_key);
    }
    notify.notify_waiters();

    let cached = cache.put(&cache_key, result?).await;

    let product = product.to_string();
    let png_bytes = tokio::task::spawn_blocking(move || {
        render_radar_tile(&cached, &product, z, x, y)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    Ok(png_bytes)
}
