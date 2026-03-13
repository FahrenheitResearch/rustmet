use crate::output::{print_json, print_error};
use crate::cmd_radar;
use serde_json::json;

const NEXRAD_BASE_URL: &str = "https://unidata-nexrad-level2.s3.amazonaws.com";

/// Maximum distance (km) for matching cells across frames.
const CELL_TRACKING_RADIUS_KM: f32 = 20.0;

/// A tracked cell across multiple frames.
struct TrackedCell {
    /// Current label (from latest frame).
    label: usize,
    /// History: (timestamp, lat, lon, max_ref, area_km2).
    history: Vec<CellSnapshot>,
    /// Motion vector: degrees (from-direction).
    motion_direction: Option<f32>,
    /// Motion speed: km/h.
    motion_speed: Option<f32>,
    /// Associated mesocyclone? (from latest frame).
    has_meso: bool,
    /// Associated meso rotational velocity (m/s).
    meso_rot_vel: Option<f32>,
    /// Associated meso strength rank.
    meso_strength: Option<u8>,
}

struct CellSnapshot {
    timestamp: String,
    lat: f64,
    lon: f64,
    max_reflectivity: f32,
    mean_reflectivity: f32,
    area_km2: f32,
    gate_count: usize,
    centroid_azimuth: f32,
    centroid_range_km: f32,
}

pub fn run(
    site: &str,
    lat: Option<f64>,
    lon: Option<f64>,
    frames: usize,
    pretty: bool,
) {
    // Resolve site
    let site_id = if let (Some(la), Some(lo)) = (lat, lon) {
        cmd_radar::find_nearest_site(la, lo)
    } else if !site.is_empty() {
        site.to_uppercase()
    } else {
        print_error("Provide --site KTLX or --lat/--lon");
    };

    let site_info = wx_radar::sites::find_site(&site_id);
    let (site_lat, site_lon) = site_info.as_ref()
        .map(|s| (Some(s.lat), Some(s.lon)))
        .unwrap_or((None, None));

    let frame_count = frames.min(10).max(1);

    // Find available files for this site
    let now = chrono::Utc::now();
    let today = now.format("%Y/%m/%d").to_string();
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y/%m/%d").to_string();

    // Get file listing from S3
    let file_keys = get_file_listing(&site_id, &today, &yesterday, frame_count);
    if file_keys.is_empty() {
        print_error(&format!("No NEXRAD files found for {} in last 24h", site_id));
    }

    let total_start = std::time::Instant::now();

    // Download and parse all frames
    let mut frame_results: Vec<FrameResult> = Vec::new();
    for key in &file_keys {
        let filename = key.rsplit('/').next().unwrap_or(key).to_string();
        let url = format!("{}/{}", NEXRAD_BASE_URL, key);
        let raw_data = cmd_radar::http_get_bytes(&url);
        let data = cmd_radar::maybe_decompress_gz(raw_data);

        match wx_radar::level2::Level2File::parse(&data) {
            Ok(l2) => {
                let timestamp = l2.timestamp_string();

                // Get lowest elevation sweep for cell identification
                let lowest_sweep_idx = l2.sweeps.iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.elevation_angle.partial_cmp(&b.elevation_angle)
                        .unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i);

                if let Some(si) = lowest_sweep_idx {
                    let cells = wx_radar::cells::identify_cells(
                        &l2.sweeps[si], site_lat, site_lon,
                    );

                    // Run rotation detection on this volume
                    let mesos = run_rotation_on_volume(&l2);

                    // Associate mesos with cells
                    let meso_azimuths: Vec<f32> = mesos.iter().map(|m| m.azimuth).collect();
                    let meso_ranges: Vec<f32> = mesos.iter().map(|m| m.range_km).collect();
                    let associations = wx_radar::cells::associate_mesos_with_cells(
                        &cells, &meso_azimuths, &meso_ranges, 15.0,
                    );

                    frame_results.push(FrameResult {
                        filename,
                        timestamp,
                        cells,
                        mesos,
                        meso_cell_associations: associations,
                    });
                }
            }
            Err(_) => continue,
        }
    }

    if frame_results.is_empty() {
        print_error("Failed to parse any radar frames");
    }

    // Track cells across frames (newest first → oldest)
    frame_results.reverse(); // now oldest first
    let tracked = track_cells(&frame_results, site_lat, site_lon);
    let total_ms = total_start.elapsed().as_millis();

    // Build output JSON
    let latest_frame = frame_results.last().unwrap();

    let cells_json: Vec<serde_json::Value> = tracked.iter().map(|tc| {
        let latest = tc.history.last().unwrap();
        let mut cell_obj = json!({
            "id": format!("C{}", tc.label),
            "lat": latest.lat,
            "lon": latest.lon,
            "centroid_azimuth": latest.centroid_azimuth,
            "centroid_range_km": latest.centroid_range_km,
            "max_reflectivity_dbz": latest.max_reflectivity,
            "mean_reflectivity_dbz": latest.mean_reflectivity,
            "area_km2": latest.area_km2,
            "gate_count": latest.gate_count,
            "frames_tracked": tc.history.len(),
        });

        if let Some(dir) = tc.motion_direction {
            cell_obj["motion_direction_deg"] = json!((dir * 10.0).round() / 10.0);
        }
        if let Some(spd) = tc.motion_speed {
            cell_obj["motion_speed_kmh"] = json!((spd * 10.0).round() / 10.0);
            // Also in knots
            cell_obj["motion_speed_kt"] = json!((spd * 0.539957 * 10.0).round() / 10.0);
        }

        if tc.has_meso {
            cell_obj["mesocyclone"] = json!({
                "detected": true,
                "rotational_velocity_ms": tc.meso_rot_vel.map(|v| (v * 10.0).round() / 10.0),
                "strength_rank": tc.meso_strength,
            });
        }

        // Add time series if we have multiple frames
        if tc.history.len() > 1 {
            let trend: Vec<serde_json::Value> = tc.history.iter().map(|snap| {
                json!({
                    "time": snap.timestamp,
                    "max_ref_dbz": snap.max_reflectivity,
                    "area_km2": snap.area_km2,
                    "lat": snap.lat,
                    "lon": snap.lon,
                })
            }).collect();
            cell_obj["trend"] = json!(trend);
        }

        cell_obj
    }).collect();

    let total_mesos = tracked.iter().filter(|t| t.has_meso).count();

    print_json(&json!({
        "site": site_id,
        "site_name": site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown"),
        "frames_analyzed": frame_results.len(),
        "latest_scan": latest_frame.timestamp,
        "summary": {
            "total_cells": cells_json.len(),
            "cells_with_mesocyclone": total_mesos,
            "strongest_cell_dbz": cells_json.first()
                .and_then(|c| c.get("max_reflectivity_dbz"))
                .and_then(|v| v.as_f64()),
        },
        "cells": cells_json,
        "performance": {
            "total_ms": total_ms,
            "frames_downloaded": file_keys.len(),
        },
    }), pretty);
}

/// Get a listing of the most recent N files from S3.
fn get_file_listing(site_id: &str, today: &str, yesterday: &str, count: usize) -> Vec<String> {
    let mut keys = Vec::new();

    // Try today first
    if let Some(listing) = fetch_s3_listing(site_id, today) {
        keys.extend(listing);
    }

    // If we don't have enough, try yesterday
    if keys.len() < count {
        if let Some(listing) = fetch_s3_listing(site_id, yesterday) {
            keys.extend(listing);
        }
    }

    // Sort chronologically (filenames contain timestamps), take most recent N
    keys.sort();
    if keys.len() > count {
        keys = keys[keys.len() - count..].to_vec();
    }

    keys
}

/// Fetch S3 directory listing for a NEXRAD site on a given date.
fn fetch_s3_listing(site_id: &str, date: &str) -> Option<Vec<String>> {
    let prefix = format!("{}/{}", date, site_id);
    let url = format!(
        "{}?list-type=2&prefix={}",
        NEXRAD_BASE_URL, prefix
    );

    let body = match ureq::get(&url).call() {
        Ok(resp) => match resp.into_body().read_to_string() {
            Ok(s) => s,
            Err(_) => return None,
        },
        Err(_) => return None,
    };

    // Parse simple XML for <Key> elements
    let mut keys = Vec::new();
    for segment in body.split("<Key>").skip(1) {
        if let Some(end) = segment.find("</Key>") {
            let key = &segment[..end];
            // Skip MDM (metadata) files, only want volume scans
            if !key.ends_with("_MDM") && !key.contains("_MDM.") {
                keys.push(key.to_string());
            }
        }
    }

    if keys.is_empty() { None } else { Some(keys) }
}

struct FrameResult {
    filename: String,
    timestamp: String,
    cells: Vec<wx_radar::cells::StormCell>,
    mesos: Vec<MesoDetection>,
    meso_cell_associations: Vec<usize>,
}

struct MesoDetection {
    azimuth: f32,
    range_km: f32,
    rotational_velocity: f32,
    max_shear: f32,
    strength_rank: u8,
}

/// Run simplified rotation detection on a volume scan.
fn run_rotation_on_volume(l2: &wx_radar::level2::Level2File) -> Vec<MesoDetection> {
    use wx_radar::products::RadarProduct;

    const MIN_REF: f32 = 35.0;
    const MIN_ROT_VEL: f32 = 20.0;
    const TVS_THRESHOLD: f32 = 46.0;
    const STRENGTH_THRESHOLDS: [f32; 5] = [15.0, 25.0, 33.0, 46.0, 60.0];

    const MIN_VERTICAL_TILTS: usize = 3;
    const MAX_HORIZ_OFFSET_KM: f64 = 10.0;
    const MIN_CLUSTER_GATES: usize = 3;

    // Scan lowest 4 unique elevations (deduplicate SAILS)
    let mut sweep_elevs: Vec<(usize, f32)> = l2.sweeps.iter()
        .enumerate()
        .map(|(i, s)| (i, s.elevation_angle))
        .collect();
    sweep_elevs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut unique_elevs: Vec<(usize, f32)> = Vec::new();
    for &(idx, elev) in &sweep_elevs {
        if unique_elevs.last().map_or(true, |&(_, prev)| (elev - prev).abs() > 0.3) {
            unique_elevs.push((idx, elev));
        }
    }
    let low_sweeps: Vec<(usize, f32)> = unique_elevs.into_iter().take(4).collect();

    // Phase 1: Per-sweep independent scanning
    let mut per_sweep_detections: Vec<Vec<MesoDetection>> = Vec::new();

    for &(si, _) in &low_sweeps {
        let sweep = &l2.sweeps[si];
        let n = sweep.radials.len();
        if n < 3 { continue; }
        let mut all_detections: Vec<MesoDetection> = Vec::new();

        // Build ref lookup
        let ref_data: Vec<Vec<f32>> = sweep.radials.iter().map(|r| {
            r.moments.iter()
                .find(|m| m.product == RadarProduct::Reflectivity)
                .map(|m| m.data.clone())
                .unwrap_or_default()
        }).collect();

        for i in 0..n {
            let next = (i + 1) % n;
            let vel_i = sweep.radials[i].moments.iter()
                .find(|m| m.product == RadarProduct::Velocity);
            let vel_next = sweep.radials[next].moments.iter()
                .find(|m| m.product == RadarProduct::Velocity);

            if let (Some(vi), Some(vn)) = (vel_i, vel_next) {
                let gate_size_km = vi.gate_size as f32 / 1000.0;
                let first_gate_km = vi.first_gate_range as f32 / 1000.0;
                let gate_count = vi.data.len().min(vn.data.len());

                for gi in 0..gate_count {
                    let v1 = vi.data[gi];
                    let v2 = vn.data[gi];
                    if v1.is_nan() || v2.is_nan() { continue; }

                    let delta_v = v2 - v1; // positive = cyclonic rotation (NH)
                    let rot_vel = delta_v / 2.0;
                    if rot_vel < MIN_ROT_VEL { continue; }

                    let range_km = first_gate_km + gi as f32 * gate_size_km;

                    // Check reflectivity co-location
                    if i < ref_data.len() && !ref_data[i].is_empty() {
                        let ref_gate_size = sweep.radials[i].moments.iter()
                            .find(|m| m.product == RadarProduct::Reflectivity)
                            .map(|m| m.gate_size as f32 / 1000.0)
                            .unwrap_or(0.25);
                        let ref_first = sweep.radials[i].moments.iter()
                            .find(|m| m.product == RadarProduct::Reflectivity)
                            .map(|m| m.first_gate_range as f32 / 1000.0)
                            .unwrap_or(0.0);
                        let ref_gi = if ref_gate_size > 0.0 {
                            ((range_km - ref_first) / ref_gate_size).round() as usize
                        } else { 0 };
                        if ref_gi < ref_data[i].len() {
                            let rv = ref_data[i][ref_gi];
                            if rv.is_nan() || rv < MIN_REF { continue; }
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    let avg_az = {
                        let a1 = sweep.radials[i].azimuth;
                        let a2 = sweep.radials[next].azimuth;
                        if (a2 - a1).abs() > 180.0 {
                            ((a1 + a2 + 360.0) / 2.0) % 360.0
                        } else {
                            (a1 + a2) / 2.0
                        }
                    };

                    let strength = if rot_vel >= STRENGTH_THRESHOLDS[4] { 5 }
                        else if rot_vel >= STRENGTH_THRESHOLDS[3] { 4 }
                        else if rot_vel >= STRENGTH_THRESHOLDS[2] { 3 }
                        else if rot_vel >= STRENGTH_THRESHOLDS[1] { 2 }
                        else { 1 };

                    all_detections.push(MesoDetection {
                        azimuth: avg_az,
                        range_km,
                        rotational_velocity: rot_vel,
                        max_shear: delta_v,
                        strength_rank: strength,
                    });
                }
            }
        }

        // Cluster this sweep's detections: merge within 2°/0.75km
        let mut sweep_clustered: Vec<MesoDetection> = Vec::new();
        'cluster: for det in &all_detections {
            for existing in &mut sweep_clustered {
                let az_diff = {
                    let d = (det.azimuth - existing.azimuth).abs();
                    if d > 180.0 { 360.0 - d } else { d }
                };
                let range_diff = (det.range_km - existing.range_km).abs();
                if az_diff < 2.0 && range_diff < 0.75 {
                    if det.rotational_velocity > existing.rotational_velocity {
                        existing.rotational_velocity = det.rotational_velocity;
                        existing.max_shear = det.max_shear;
                        existing.strength_rank = det.strength_rank;
                    }
                    continue 'cluster;
                }
            }
            sweep_clustered.push(MesoDetection {
                azimuth: det.azimuth,
                range_km: det.range_km,
                rotational_velocity: det.rotational_velocity,
                max_shear: det.max_shear,
                strength_rank: det.strength_rank,
            });
        }
        // Only keep clusters with enough gates
        let sweep_filtered: Vec<MesoDetection> = sweep_clustered;
        per_sweep_detections.push(sweep_filtered);
    }

    // Phase 2: Vertical continuity — require MIN_VERTICAL_TILTS
    let mut final_mesos: Vec<MesoDetection> = Vec::new();

    if let Some(base_dets) = per_sweep_detections.first() {
        for base in base_dets {
            let bx = (base.azimuth as f64).to_radians().sin() * base.range_km as f64;
            let by = (base.azimuth as f64).to_radians().cos() * base.range_km as f64;
            let mut tilt_count = 1usize;
            let mut best_rot = base.rotational_velocity;
            let mut best_shear = base.max_shear;

            for sweep_dets in per_sweep_detections.iter().skip(1) {
                let mut closest_dist = MAX_HORIZ_OFFSET_KM;
                let mut closest_det: Option<&MesoDetection> = None;
                for det in sweep_dets {
                    let dx = (det.azimuth as f64).to_radians().sin() * det.range_km as f64;
                    let dy = (det.azimuth as f64).to_radians().cos() * det.range_km as f64;
                    let dist = ((bx - dx).powi(2) + (by - dy).powi(2)).sqrt();
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_det = Some(det);
                    }
                }
                if let Some(det) = closest_det {
                    tilt_count += 1;
                    if det.rotational_velocity > best_rot {
                        best_rot = det.rotational_velocity;
                    }
                    if det.max_shear > best_shear {
                        best_shear = det.max_shear;
                    }
                }
            }

            if tilt_count >= MIN_VERTICAL_TILTS {
                let strength = if best_rot >= STRENGTH_THRESHOLDS[4] { 5 }
                    else if best_rot >= STRENGTH_THRESHOLDS[3] { 4 }
                    else if best_rot >= STRENGTH_THRESHOLDS[2] { 3 }
                    else if best_rot >= STRENGTH_THRESHOLDS[1] { 2 }
                    else { 1 };
                final_mesos.push(MesoDetection {
                    azimuth: base.azimuth,
                    range_km: base.range_km,
                    rotational_velocity: best_rot,
                    max_shear: best_shear,
                    strength_rank: strength,
                });
            }
        }
    }

    let _ = TVS_THRESHOLD; // used for classification if needed
    final_mesos
}

/// Track cells across multiple frames by proximity matching.
fn track_cells(frames: &[FrameResult], site_lat: Option<f64>, site_lon: Option<f64>) -> Vec<TrackedCell> {
    if frames.is_empty() {
        return Vec::new();
    }

    // Start with the latest frame's cells
    let latest = frames.last().unwrap();
    let mut tracked: Vec<TrackedCell> = latest.cells.iter().enumerate().map(|(i, cell)| {
        let has_meso = latest.meso_cell_associations.iter()
            .enumerate()
            .any(|(mi, &assoc)| assoc == cell.label);
        let meso_idx = latest.meso_cell_associations.iter()
            .enumerate()
            .find(|(_, &assoc)| assoc == cell.label)
            .map(|(mi, _)| mi);

        TrackedCell {
            label: cell.label,
            history: vec![CellSnapshot {
                timestamp: latest.timestamp.clone(),
                lat: cell.lat,
                lon: cell.lon,
                max_reflectivity: cell.max_reflectivity,
                mean_reflectivity: cell.mean_reflectivity,
                area_km2: cell.area_km2,
                gate_count: cell.gate_count,
                centroid_azimuth: cell.centroid_azimuth,
                centroid_range_km: cell.centroid_range_km,
            }],
            motion_direction: None,
            motion_speed: None,
            has_meso,
            meso_rot_vel: meso_idx.and_then(|mi| latest.mesos.get(mi))
                .map(|m| m.rotational_velocity),
            meso_strength: meso_idx.and_then(|mi| latest.mesos.get(mi))
                .map(|m| m.strength_rank),
        }
    }).collect();

    // Match backwards through older frames with collision resolution
    for frame_idx in (0..frames.len() - 1).rev() {
        let frame = &frames[frame_idx];

        // Build distance matrix: (track_idx, cell_idx, distance)
        let mut candidates: Vec<(usize, usize, f32)> = Vec::new();
        for (ti, tc) in tracked.iter().enumerate() {
            let latest_snap = tc.history.last().unwrap();
            for (ci, cell) in frame.cells.iter().enumerate() {
                let dist = haversine_km(latest_snap.lat, latest_snap.lon, cell.lat, cell.lon);
                if dist < CELL_TRACKING_RADIUS_KM {
                    candidates.push((ti, ci, dist));
                }
            }
        }

        // Sort by distance, assign greedily ensuring 1:1 mapping
        candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut used_tracks = std::collections::HashSet::new();
        let mut used_cells = std::collections::HashSet::new();

        for &(ti, ci, _) in &candidates {
            if used_tracks.contains(&ti) || used_cells.contains(&ci) {
                continue;
            }
            used_tracks.insert(ti);
            used_cells.insert(ci);

            let cell = &frame.cells[ci];
            tracked[ti].history.push(CellSnapshot {
                timestamp: frame.timestamp.clone(),
                lat: cell.lat,
                lon: cell.lon,
                max_reflectivity: cell.max_reflectivity,
                mean_reflectivity: cell.mean_reflectivity,
                area_km2: cell.area_km2,
                gate_count: cell.gate_count,
                centroid_azimuth: cell.centroid_azimuth,
                centroid_range_km: cell.centroid_range_km,
            });
        }
    }

    // Reverse history so it's chronological (oldest first)
    for tc in &mut tracked {
        tc.history.reverse();
    }

    // Compute motion vectors from first and last position
    for tc in &mut tracked {
        if tc.history.len() >= 2 {
            let first = &tc.history[0];
            let last = tc.history.last().unwrap();

            let dist_km = haversine_km(first.lat, first.lon, last.lat, last.lon);

            if dist_km > 1.0 {
                // Great-circle bearing
                let lat1_r = first.lat.to_radians();
                let lat2_r = last.lat.to_radians();
                let dlon_r = (last.lon - first.lon).to_radians();
                let x = dlon_r.sin() * lat2_r.cos();
                let y = lat1_r.cos() * lat2_r.sin()
                    - lat1_r.sin() * lat2_r.cos() * dlon_r.cos();
                let bearing = (x.atan2(y).to_degrees() + 360.0) % 360.0;
                let from_direction = (bearing + 180.0) % 360.0;

                // Compute time from actual timestamps, fall back to ~5 min estimate
                let time_hours = parse_time_diff(&first.timestamp, &last.timestamp)
                    .unwrap_or((tc.history.len() - 1) as f64 * (5.0 / 60.0));
                let speed_kmh = if time_hours > 0.0 {
                    dist_km as f64 / time_hours
                } else {
                    0.0
                };

                tc.motion_direction = Some(from_direction as f32);
                tc.motion_speed = Some(speed_kmh as f32);
            }
        }
    }

    tracked
}

/// Parse time difference between two timestamp strings in hours.
fn parse_time_diff(ts1: &str, ts2: &str) -> Option<f64> {
    let parse = |s: &str| -> Option<i64> {
        chrono::DateTime::parse_from_rfc3339(s).ok()
            .map(|dt| dt.timestamp())
            .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok().map(|ndt| ndt.and_utc().timestamp()))
            .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                .ok().map(|ndt| ndt.and_utc().timestamp()))
    };
    let t1 = parse(ts1)?;
    let t2 = parse(ts2)?;
    let secs = (t2 - t1).unsigned_abs();
    if secs > 0 { Some(secs as f64 / 3600.0) } else { None }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f32 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) +
        lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    (6371.0 * c) as f32
}
