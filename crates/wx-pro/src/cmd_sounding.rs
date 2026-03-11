use serde::Serialize;
use crate::output::{print_json, print_error};

use chrono::{Utc, Timelike};
use rustmet_core::download::{DownloadClient, fetch_with_fallback};
use rustmet_core::grib2::{self, Grib2Message, grid_latlon};
use rustmet_core::grib2::tables;

/// Convective / severe weather parameters extracted from HRRR or RAP at a point.
#[derive(Serialize)]
struct SoundingResponse {
    lat: f64,
    lon: f64,
    model: String,
    run: String,
    source: String,
    surface: SurfaceParams,
    instability: InstabilityParams,
    shear: ShearParams,
    helicity: HelicityParams,
    composite: CompositeParams,
    storm_motion: Option<StormMotion>,
    performance: PerfStats,
}

#[derive(Serialize)]
struct SurfaceParams {
    temperature_c: Option<f64>,
    dewpoint_c: Option<f64>,
    wind_speed_kt: Option<f64>,
    wind_dir_deg: Option<f64>,
    wind_gust_kt: Option<f64>,
    pressure_hpa: Option<f64>,
    visibility_km: Option<f64>,
    pwat_mm: Option<f64>,
}

#[derive(Serialize)]
struct InstabilityParams {
    sbcape: Option<f64>,
    sbcin: Option<f64>,
    mlcape: Option<f64>,
    mlcin: Option<f64>,
    lifted_index: Option<f64>,
    best_lifted_index: Option<f64>,
}

#[derive(Serialize)]
struct ShearParams {
    /// Bulk shear 0-1km (kt)
    shear_01_kt: Option<f64>,
    /// Bulk shear 0-6km (kt)
    shear_06_kt: Option<f64>,
}

#[derive(Serialize)]
struct HelicityParams {
    /// Storm-relative helicity 0-1km (m²/s²)
    srh_01: Option<f64>,
    /// Storm-relative helicity 0-3km (m²/s²)
    srh_03: Option<f64>,
    /// Max updraft helicity (m²/s²)
    max_updraft_helicity: Option<f64>,
}

#[derive(Serialize)]
struct CompositeParams {
    /// Composite reflectivity (dBZ)
    composite_reflectivity: Option<f64>,
}

#[derive(Serialize)]
struct StormMotion {
    u_ms: f64,
    v_ms: f64,
    speed_kt: f64,
    direction_deg: f64,
}

#[derive(Serialize)]
struct PerfStats {
    download_ms: u128,
    parse_ms: u128,
    messages_downloaded: usize,
    bytes_downloaded: usize,
}

/// Variable patterns to download from HRRR/RAP sfc product.
/// These cover all the key convective parameters.
const CONVECTIVE_VARS: &[&str] = &[
    // Surface obs
    "TMP:2 m above ground",
    "DPT:2 m above ground",
    "UGRD:10 m above ground",
    "VGRD:10 m above ground",
    "GUST:surface",
    "PRES:surface",
    "VIS:surface",
    // Moisture
    "PWAT:entire atmosphere",
    // Instability
    "CAPE:surface",
    "CIN:surface",
    "CAPE:255-0 mb above ground",
    "CIN:255-0 mb above ground",
    "LFTX:500-1000 mb",
    "4LFTX:180-0 mb above ground",
    // Reflectivity
    "REFC:entire atmosphere",
    // Shear (u and v components)
    "VUCSH:0-1000 m above ground",
    "VVCSH:0-1000 m above ground",
    "VUCSH:0-6000 m above ground",
    "VVCSH:0-6000 m above ground",
    // Helicity
    "HLCY:3000-0 m above ground",
    "HLCY:1000-0 m above ground",
    // Updraft helicity
    "MXUPHL:5000-2000 m above ground",
    // Storm motion
    "USTM:6000-0 m above ground",
    "VSTM:6000-0 m above ground",
];

pub fn run(lat: f64, lon: f64, model: &str, pretty: bool) {
    let model_lower = model.to_lowercase();
    if !["hrrr", "rap"].contains(&model_lower.as_str()) {
        print_error(&format!(
            "Model '{}' not supported for sounding. Use 'hrrr' or 'rap'.",
            model
        ));
    }

    // Create download client
    let client = match DownloadClient::new() {
        Ok(c) => c,
        Err(e) => print_error(&format!("Failed to create HTTP client: {}", e)),
    };

    // Find latest model run
    eprintln!("Finding latest {} run...", model_lower);
    let (date, hour) = match rustmet_core::models::find_latest_run(&client, &model_lower) {
        Ok(r) => r,
        Err(e) => {
            // Fallback: guess current run
            eprintln!("Warning: latest run detection failed ({}), trying current hour", e);
            let now = Utc::now();
            let h = now.hour();
            // Use 2 hours ago to allow for processing delay
            let lookback = if h >= 2 { h - 2 } else { 22 + h };
            (now.format("%Y%m%d").to_string(), lookback)
        }
    };

    let run_label = format!("{}/{:02}z", date, hour);
    eprintln!("Downloading {} {} sfc f00 convective params for ({}, {})...", model_lower, run_label, lat, lon);

    // Download with selective .idx byte ranges
    let download_start = std::time::Instant::now();
    let result = match fetch_with_fallback(
        &client,
        &model_lower,
        &date,
        hour,
        "sfc",
        0,
        Some(CONVECTIVE_VARS),
        None,
    ) {
        Ok(r) => r,
        Err(e) => print_error(&format!("Download failed: {}", e)),
    };
    let download_ms = download_start.elapsed().as_millis();
    let bytes_downloaded = result.data.len();
    let source = result.source_name.clone();

    eprintln!("Downloaded {} bytes from {} in {}ms", bytes_downloaded, source, download_ms);

    // Parse GRIB2
    let parse_start = std::time::Instant::now();
    let grib = match grib2::Grib2File::from_bytes(&result.data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("GRIB2 parse failed: {}", e)),
    };
    let parse_ms = parse_start.elapsed().as_millis();
    let num_messages = grib.messages.len();

    eprintln!("Parsed {} GRIB2 messages in {}ms", num_messages, parse_ms);

    // Extract values at point
    let mut temperature_c = None;
    let mut dewpoint_c = None;
    let mut u_wind = None;
    let mut v_wind = None;
    let mut gust_ms = None;
    let mut pressure_pa = None;
    let mut visibility_m = None;
    let mut pwat_kgm2 = None;
    let mut sbcape = None;
    let mut sbcin = None;
    let mut mlcape = None;
    let mut mlcin = None;
    let mut lifted_index = None;
    let mut best_lifted_index = None;
    let mut refc = None;
    let mut vucsh_01 = None;
    let mut vvcsh_01 = None;
    let mut vucsh_06 = None;
    let mut vvcsh_06 = None;
    let mut hlcy_01 = None;
    let mut hlcy_03 = None;
    let mut mxuphl = None;
    let mut ustm = None;
    let mut vstm = None;

    for msg in &grib.messages {
        let val = match extract_point_value(msg, lat, lon) {
            Some(v) => v,
            None => continue,
        };

        // Identify variable from GRIB2 metadata
        let param_name = tables::parameter_name(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        );
        let level_type = msg.product.level_type;
        let level_value = msg.product.level_value;

        // Match by (discipline, category, number, level_type, level_value)
        match (msg.discipline, msg.product.parameter_category, msg.product.parameter_number, level_type) {
            // Temperature at 2m (disc=0, cat=0, num=0, level=103/2m)
            (0, 0, 0, 103) if level_value_near(level_value, 2.0) => {
                temperature_c = Some(val - 273.15);
            }
            // Dewpoint at 2m (disc=0, cat=0, num=6, level=103/2m)
            (0, 0, 6, 103) if level_value_near(level_value, 2.0) => {
                dewpoint_c = Some(val - 273.15);
            }
            // U-wind 10m (disc=0, cat=2, num=2, level=103/10m)
            (0, 2, 2, 103) if level_value_near(level_value, 10.0) => {
                u_wind = Some(val);
            }
            // V-wind 10m (disc=0, cat=2, num=3, level=103/10m)
            (0, 2, 3, 103) if level_value_near(level_value, 10.0) => {
                v_wind = Some(val);
            }
            // Wind gust (disc=0, cat=2, num=22, level=1/surface)
            (0, 2, 22, 1) => {
                gust_ms = Some(val);
            }
            // Pressure surface (disc=0, cat=3, num=0, level=1/surface)
            (0, 3, 0, 1) => {
                pressure_pa = Some(val);
            }
            // Visibility (disc=0, cat=19, num=0, level=1/surface)
            (0, 19, 0, 1) => {
                visibility_m = Some(val);
            }
            // PWAT (disc=0, cat=1, num=3, level=200/entire atmosphere)
            (0, 1, 3, 200) => {
                pwat_kgm2 = Some(val);
            }
            // CAPE surface (disc=0, cat=7, num=6, level=1/surface)
            (0, 7, 6, 1) => {
                sbcape = Some(val);
            }
            // CIN surface (disc=0, cat=7, num=7, level=1/surface)
            (0, 7, 7, 1) => {
                sbcin = Some(val);
            }
            // CAPE mixed-layer (disc=0, cat=7, num=6, level=108/specified height above ground)
            (0, 7, 6, 108) => {
                mlcape = Some(val);
            }
            // CIN mixed-layer
            (0, 7, 7, 108) => {
                mlcin = Some(val);
            }
            // Lifted index (disc=0, cat=7, num=192 or specific)
            // LFTX is typically disc=0, cat=7, num=10 or NCEP local use
            (0, 7, 10, _) => {
                lifted_index = Some(val);
            }
            // Best lifted index (4LFTX) — NCEP local (num=11 or 193)
            // level_value may be 18000 (encoding 180-0 mb layer)
            (0, 7, 11, _) | (0, 7, 193, 108) => {
                best_lifted_index = Some(val);
            }
            // Composite reflectivity (disc=0, cat=16, num=196 NCEP local for REFC)
            (0, 16, 196, _) => {
                refc = Some(val);
            }
            // Also check standard reflectivity
            (0, 16, 195, _) if refc.is_none() => {
                refc = Some(val);
            }
            // U-component of vertical shear (VUCSH)
            // Standard: disc=0, cat=2, num=15; NCEP local: num=192
            // HRRR encodes both 0-1km and 0-6km with level_value=0, level_type=103
            // We get them in .idx order: first is 0-1km, second is 0-6km
            (0, 2, 15, _) | (0, 2, 192, _) => {
                if vucsh_01.is_none() {
                    vucsh_01 = Some(val);
                } else if vucsh_06.is_none() {
                    vucsh_06 = Some(val);
                }
            }
            // V-component of vertical shear (VVCSH)
            (0, 2, 16, _) | (0, 2, 193, _) => {
                if vvcsh_01.is_none() {
                    vvcsh_01 = Some(val);
                } else if vvcsh_06.is_none() {
                    vvcsh_06 = Some(val);
                }
            }
            // Storm-relative helicity (disc=0, cat=7, num=8)
            (0, 7, 8, 103) if level_value_near(level_value, 3000.0) => {
                hlcy_03 = Some(val);
            }
            (0, 7, 8, 103) if level_value_near(level_value, 1000.0) => {
                hlcy_01 = Some(val);
            }
            // Max updraft helicity (disc=0, cat=7, num=199 NCEP)
            (0, 7, 199, _) => {
                mxuphl = Some(val);
            }
            // Storm motion U (disc=0, cat=2, num=27 or NCEP variants)
            (0, 2, 27, _) => {
                ustm = Some(val);
            }
            // Storm motion V (disc=0, cat=2, num=28 or NCEP variants)
            (0, 2, 28, _) => {
                vstm = Some(val);
            }
            _ => {
                // Log unmatched for debugging
                eprintln!("  Unmatched: {} (d={}, c={}, n={}, lt={}, lv={})",
                    param_name,
                    msg.discipline,
                    msg.product.parameter_category,
                    msg.product.parameter_number,
                    level_type,
                    level_value
                );
            }
        }
    }

    // Compute wind speed/direction from u/v components
    let (wind_speed_kt, wind_dir_deg) = match (u_wind, v_wind) {
        (Some(u), Some(v)) => {
            let speed_ms = (u * u + v * v).sqrt();
            let speed_kt = speed_ms * 1.94384;
            let dir = (270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0);
            (Some(round2(speed_kt)), Some(round0(dir)))
        }
        _ => (None, None),
    };

    // Compute shear magnitudes from u/v components
    let shear_01_kt = match (vucsh_01, vvcsh_01) {
        (Some(u), Some(v)) => Some(round1((u * u + v * v).sqrt() * 1.94384)),
        _ => None,
    };
    let shear_06_kt = match (vucsh_06, vvcsh_06) {
        (Some(u), Some(v)) => Some(round1((u * u + v * v).sqrt() * 1.94384)),
        _ => None,
    };

    // Storm motion
    let storm_motion = match (ustm, vstm) {
        (Some(u), Some(v)) => {
            let speed_ms = (u * u + v * v).sqrt();
            Some(StormMotion {
                u_ms: round1(u),
                v_ms: round1(v),
                speed_kt: round1(speed_ms * 1.94384),
                direction_deg: round0((270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0)),
            })
        }
        _ => None,
    };

    let resp = SoundingResponse {
        lat,
        lon,
        model: model_lower.to_uppercase(),
        run: run_label,
        source,
        surface: SurfaceParams {
            temperature_c: temperature_c.map(round1),
            dewpoint_c: dewpoint_c.map(round1),
            wind_speed_kt,
            wind_dir_deg,
            wind_gust_kt: gust_ms.map(|g| round1(g * 1.94384)),
            pressure_hpa: pressure_pa.map(|p| round1(p / 100.0)),
            visibility_km: visibility_m.map(|v| round1(v / 1000.0)),
            pwat_mm: pwat_kgm2.map(round1), // kg/m² ≈ mm
        },
        instability: InstabilityParams {
            sbcape: sbcape.map(round0),
            sbcin: sbcin.map(round0),
            mlcape: mlcape.map(round0),
            mlcin: mlcin.map(round0),
            lifted_index: lifted_index.map(round1),
            best_lifted_index: best_lifted_index.map(round1),
        },
        shear: ShearParams {
            shear_01_kt,
            shear_06_kt,
        },
        helicity: HelicityParams {
            srh_01: hlcy_01.map(round0),
            srh_03: hlcy_03.map(round0),
            max_updraft_helicity: mxuphl.map(round0),
        },
        composite: CompositeParams {
            composite_reflectivity: refc.map(round1),
        },
        storm_motion,
        performance: PerfStats {
            download_ms,
            parse_ms,
            messages_downloaded: num_messages,
            bytes_downloaded,
        },
    };

    print_json(&resp, pretty);
}

/// Extract a single f64 value from a GRIB2 message at the nearest grid point to (lat, lon).
fn extract_point_value(msg: &Grib2Message, target_lat: f64, target_lon: f64) -> Option<f64> {
    let values = match grib2::unpack_message(msg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  Unpack error: {}", e);
            return None;
        }
    };

    if values.is_empty() {
        return None;
    }

    let (lats, lons) = grid_latlon(&msg.grid);
    if lats.is_empty() {
        return None;
    }

    // Normalize target longitude to match GRIB2 convention (0-360 or -180 to 180)
    let target_lon_norm = if lons.iter().any(|&lo| lo > 180.0) {
        // Grid uses 0-360
        if target_lon < 0.0 { target_lon + 360.0 } else { target_lon }
    } else {
        target_lon
    };

    // Find nearest grid point
    let mut min_dist = f64::INFINITY;
    let mut nearest_idx = 0;
    for (i, (&la, &lo)) in lats.iter().zip(lons.iter()).enumerate() {
        let dlat = la - target_lat;
        let dlon = lo - target_lon_norm;
        let dist = dlat * dlat + dlon * dlon;
        if dist < min_dist {
            min_dist = dist;
            nearest_idx = i;
        }
    }

    if nearest_idx < values.len() {
        let val = values[nearest_idx];
        // Filter out missing/fill values
        if val.is_finite() && val.abs() < 1e15 {
            Some(val)
        } else {
            None
        }
    } else {
        None
    }
}

fn level_value_near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0
}

fn round0(v: f64) -> f64 { v.round() }
fn round1(v: f64) -> f64 { (v * 10.0).round() / 10.0 }
fn round2(v: f64) -> f64 { (v * 100.0).round() / 100.0 }
