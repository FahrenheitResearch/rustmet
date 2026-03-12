use crate::output::{print_json, print_error};
use crate::cmd_radar;
use serde_json::json;

#[allow(dead_code)]
const NEXRAD_BASE_URL: &str = "https://unidata-nexrad-level2.s3.amazonaws.com";

/// Rotation detection thresholds.
const MIN_REFLECTIVITY_DBZ: f32 = 35.0;
const TVS_SHEAR_THRESHOLD: f32 = 46.0;
const MIN_CLUSTER_GATES: usize = 3;
const MIN_VERTICAL_TILTS: usize = 3;
const MAX_HORIZONTAL_OFFSET_KM: f64 = 10.0;
const NUM_LOW_ELEVATIONS: usize = 4;

/// Rotational velocity thresholds for strength ranking (m/s).
const STRENGTH_THRESHOLDS: [f32; 5] = [20.0, 25.0, 33.0, 46.0, 60.0];

/// A candidate rotation detection from a single sweep.
#[derive(Debug, Clone)]
struct SweepDetection {
    azimuth: f32,
    range_km: f32,
    elevation: f32,
    rotational_velocity: f32,
    max_shear: f32,
    gate_count: usize,
}

pub fn run(site: &str, lat: Option<f64>, lon: Option<f64>, pretty: bool) {
    // Resolve site
    let site_id = if let (Some(la), Some(lo)) = (lat, lon) {
        cmd_radar::find_nearest_site(la, lo)
    } else if !site.is_empty() {
        site.to_uppercase()
    } else {
        print_error("Provide --site KTLX or --lat/--lon");
    };

    let site_info = wx_radar::sites::find_site(&site_id);

    // Download latest Level 2
    let now = chrono::Utc::now();
    let today = now.format("%Y/%m/%d").to_string();
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y/%m/%d").to_string();

    let latest_key = cmd_radar::find_latest_file(&site_id, &today)
        .or_else(|| cmd_radar::find_latest_file(&site_id, &yesterday));

    let key = match latest_key {
        Some(k) => k,
        None => print_error(&format!("No NEXRAD files found for {} in last 24h", site_id)),
    };

    let filename = key.rsplit('/').next().unwrap_or(&key).to_string();

    let download_start = std::time::Instant::now();
    let url = format!("{}/{}", NEXRAD_BASE_URL, key);
    let raw_data = cmd_radar::http_get_bytes(&url);
    let download_ms = download_start.elapsed().as_millis();

    let data = cmd_radar::maybe_decompress_gz(raw_data);

    let parse_start = std::time::Instant::now();
    let l2 = match wx_radar::level2::Level2File::parse(&data) {
        Ok(f) => f,
        Err(e) => print_error(&format!("Failed to parse Level 2: {}", e)),
    };
    let parse_ms = parse_start.elapsed().as_millis();

    // Run rotation detection algorithm
    let algo_start = std::time::Instant::now();

    // Get lowest N elevation sweeps
    let mut sweep_elevations: Vec<(usize, f32)> = l2.sweeps.iter()
        .enumerate()
        .map(|(i, s)| (i, s.elevation_angle))
        .collect();
    sweep_elevations.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let low_sweep_indices: Vec<usize> = sweep_elevations.iter()
        .take(NUM_LOW_ELEVATIONS)
        .map(|(i, _)| *i)
        .collect();

    // For each low elevation sweep, scan for azimuthal shear
    let mut all_sweep_detections: Vec<Vec<SweepDetection>> = Vec::new();
    let mut total_raw_candidates = 0usize;
    for &si in &low_sweep_indices {
        let detections = scan_sweep(&l2.sweeps[si]);
        total_raw_candidates += detections.len();
        all_sweep_detections.push(detections);
    }

    // Cluster detections within each sweep
    let mut clustered: Vec<Vec<SweepDetection>> = Vec::new();
    for detections in &all_sweep_detections {
        let clusters = cluster_detections(detections);
        clustered.push(clusters);
    }

    // Vertical continuity check across tilts
    let final_detections = vertical_continuity(&clustered, &low_sweep_indices, &l2.sweeps);

    let algo_ms = algo_start.elapsed().as_millis();

    // Build output JSON
    let mesocyclones: Vec<serde_json::Value> = final_detections.iter().map(|d| {
        let strength = compute_strength_rank(d.rotational_velocity);

        let (det_lat, det_lon) = if let Some(ref si) = site_info {
            let az_rad = (d.azimuth as f64).to_radians();
            let la = si.lat + (d.range_km as f64 * az_rad.cos()) / 111.139;
            let lo = si.lon + (d.range_km as f64 * az_rad.sin())
                / (111.139 * si.lat.to_radians().cos());
            ((la * 1000.0).round() / 1000.0, (lo * 1000.0).round() / 1000.0)
        } else {
            (0.0, 0.0)
        };

        let is_tvs = d.max_shear >= TVS_SHEAR_THRESHOLD;

        json!({
            "lat": det_lat,
            "lon": det_lon,
            "azimuth": (d.azimuth * 10.0).round() / 10.0,
            "range_km": (d.range_km * 10.0).round() / 10.0,
            "elevation": (d.elevation * 10.0).round() / 10.0,
            "rotational_velocity_ms": (d.rotational_velocity * 10.0).round() / 10.0,
            "max_gate_to_gate_ms": (d.max_shear * 10.0).round() / 10.0,
            "strength_rank": strength,
            "is_tvs": is_tvs,
            "gate_count": d.gate_count,
        })
    }).collect();

    let tvs_count = final_detections.iter()
        .filter(|d| d.max_shear >= TVS_SHEAR_THRESHOLD)
        .count();

    // Per-sweep summary
    let sweep_summaries: Vec<serde_json::Value> = all_sweep_detections.iter()
        .enumerate()
        .map(|(i, dets)| {
            let elev = low_sweep_indices.get(i)
                .and_then(|&si| l2.sweeps.get(si))
                .map(|s| s.elevation_angle)
                .unwrap_or(0.0);
            if dets.is_empty() {
                json!({"sweep": i, "elevation": elev, "candidates": 0})
            } else {
                let max_rot = dets.iter()
                    .map(|d| d.rotational_velocity)
                    .fold(0.0f32, f32::max);
                json!({
                    "sweep": i,
                    "elevation": elev,
                    "candidates": dets.len(),
                    "max_rotational_velocity_ms": (max_rot * 10.0).round() / 10.0,
                })
            }
        })
        .collect();

    print_json(&json!({
        "site": site_id,
        "site_name": site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown"),
        "file": filename,
        "algorithm": {
            "min_reflectivity_dbz": MIN_REFLECTIVITY_DBZ,
            "tvs_shear_threshold_ms": TVS_SHEAR_THRESHOLD,
            "min_cluster_gates": MIN_CLUSTER_GATES,
            "min_vertical_tilts": MIN_VERTICAL_TILTS,
            "max_horizontal_offset_km": MAX_HORIZONTAL_OFFSET_KM,
            "elevations_scanned": low_sweep_indices.len(),
        },
        "raw_candidates": total_raw_candidates,
        "sweep_analysis": sweep_summaries,
        "detections": {
            "mesocyclone_count": mesocyclones.len(),
            "tvs_count": tvs_count,
            "items": mesocyclones,
        },
        "performance": {
            "download_ms": download_ms,
            "parse_ms": parse_ms,
            "algorithm_ms": algo_ms,
            "file_size_bytes": data.len(),
        },
    }), pretty);
}

/// Scan a single sweep for azimuthal shear patterns.
///
/// For each pair of adjacent radials, compare velocity values at the same range
/// to find velocity couplets (inbound/outbound pairs) that indicate rotation.
/// Only consider gates where co-located reflectivity >= MIN_REFLECTIVITY_DBZ.
fn scan_sweep(sweep: &wx_radar::level2::Level2Sweep) -> Vec<SweepDetection> {
    let mut candidates = Vec::new();

    // Build reflectivity lookup per radial for co-location check
    let mut ref_data: Vec<Vec<f32>> = Vec::new();
    let mut ref_gate_size: f32 = 0.25;
    let mut ref_first_gate: f32 = 0.0;
    for radial in &sweep.radials {
        let mut found = false;
        for moment in &radial.moments {
            if moment.product == wx_radar::products::RadarProduct::Reflectivity {
                ref_gate_size = moment.gate_size as f32 / 1000.0;
                ref_first_gate = moment.first_gate_range as f32 / 1000.0;
                ref_data.push(moment.data.clone());
                found = true;
                break;
            }
        }
        if !found {
            ref_data.push(Vec::new());
        }
    }

    let n = sweep.radials.len();
    if n < 3 {
        return candidates;
    }

    // Compare adjacent radials for velocity shear
    for i in 0..n {
        let next = (i + 1) % n;

        let vel_i = sweep.radials[i].moments.iter()
            .find(|m| m.product == wx_radar::products::RadarProduct::Velocity);
        let vel_next = sweep.radials[next].moments.iter()
            .find(|m| m.product == wx_radar::products::RadarProduct::Velocity);

        if let (Some(vi), Some(vn)) = (vel_i, vel_next) {
            let gate_size_km = vi.gate_size as f32 / 1000.0;
            let first_gate_km = vi.first_gate_range as f32 / 1000.0;
            let gate_count = vi.data.len().min(vn.data.len());

            for gi in 0..gate_count {
                let v1 = vi.data[gi];
                let v2 = vn.data[gi];
                if v1.is_nan() || v2.is_nan() {
                    continue;
                }

                let delta_v = v2 - v1; // positive = cyclonic rotation (NH)
                let rot_vel = delta_v / 2.0;

                // Minimum rotational velocity: 15 m/s
                if rot_vel < STRENGTH_THRESHOLDS[0] {
                    continue;
                }

                let range_km = first_gate_km + gi as f32 * gate_size_km;

                // Co-location check: reflectivity at this gate must be >= threshold
                let ref_ok = if i < ref_data.len() && !ref_data[i].is_empty() {
                    let ref_gi = if ref_gate_size > 0.0 {
                        ((range_km - ref_first_gate) / ref_gate_size).round() as usize
                    } else {
                        0
                    };
                    if ref_gi < ref_data[i].len() {
                        let rv = ref_data[i][ref_gi];
                        !rv.is_nan() && rv >= MIN_REFLECTIVITY_DBZ
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !ref_ok {
                    continue;
                }

                let avg_az = {
                    let a1 = sweep.radials[i].azimuth;
                    let a2 = sweep.radials[next].azimuth;
                    // Handle 360/0 wraparound
                    if (a2 - a1).abs() > 180.0 {
                        let sum = a1 + a2 + 360.0;
                        (sum / 2.0) % 360.0
                    } else {
                        (a1 + a2) / 2.0
                    }
                };

                candidates.push(SweepDetection {
                    azimuth: avg_az,
                    range_km,
                    elevation: sweep.elevation_angle,
                    rotational_velocity: rot_vel,
                    max_shear: delta_v,
                    gate_count: 1,
                });
            }
        }
    }

    candidates
}

/// Cluster nearby shear detections using union-find.
///
/// Merge adjacent gates (within 2° azimuthally, 0.75 km in range) into
/// connected components. Discard clusters with fewer than MIN_CLUSTER_GATES gates.
/// Return the centroid detection for each surviving cluster.
fn cluster_detections(detections: &[SweepDetection]) -> Vec<SweepDetection> {
    if detections.is_empty() {
        return Vec::new();
    }

    let n = detections.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank: Vec<usize> = vec![0; n];

    // Union-find helpers (inline to avoid extra struct)
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    fn union(parent: &mut [usize], rank: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra == rb { return; }
        if rank[ra] < rank[rb] {
            parent[ra] = rb;
        } else if rank[ra] > rank[rb] {
            parent[rb] = ra;
        } else {
            parent[rb] = ra;
            rank[ra] += 1;
        }
    }

    // Adjacency thresholds
    let az_threshold = 2.0f32;   // degrees
    let range_threshold = 0.75f32; // km (~3 gates at 0.25 km)

    // O(n²) pairwise — fine for typical detection counts (<1000)
    for i in 0..n {
        for j in (i + 1)..n {
            let az_diff = {
                let d = (detections[i].azimuth - detections[j].azimuth).abs();
                if d > 180.0 { 360.0 - d } else { d }
            };
            let range_diff = (detections[i].range_km - detections[j].range_km).abs();

            if az_diff <= az_threshold && range_diff <= range_threshold {
                union(&mut parent, &mut rank, i, j);
            }
        }
    }

    // Group by component
    let mut components: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        components.entry(root).or_default().push(i);
    }

    // Build centroid detection for each qualifying cluster
    let mut result = Vec::new();
    for (_root, indices) in &components {
        if indices.len() < MIN_CLUSTER_GATES {
            continue;
        }

        let mut sum_az_x: f64 = 0.0;
        let mut sum_az_y: f64 = 0.0;
        let mut sum_range: f64 = 0.0;
        let mut max_rot_vel: f32 = 0.0;
        let mut max_shear: f32 = 0.0;
        let mut total_gates: usize = 0;

        for &idx in indices {
            let d = &detections[idx];
            let az_rad = (d.azimuth as f64).to_radians();
            sum_az_x += az_rad.cos();
            sum_az_y += az_rad.sin();
            sum_range += d.range_km as f64;
            if d.rotational_velocity > max_rot_vel {
                max_rot_vel = d.rotational_velocity;
            }
            if d.max_shear > max_shear {
                max_shear = d.max_shear;
            }
            total_gates += d.gate_count;
        }

        let count = indices.len() as f64;
        let centroid_az = (sum_az_y.atan2(sum_az_x).to_degrees() as f32 + 360.0) % 360.0;
        let centroid_range = (sum_range / count) as f32;
        let elevation = detections[indices[0]].elevation;

        result.push(SweepDetection {
            azimuth: centroid_az,
            range_km: centroid_range,
            elevation,
            rotational_velocity: max_rot_vel,
            max_shear,
            gate_count: total_gates,
        });
    }

    result
}

/// Check vertical continuity across multiple elevation tilts.
///
/// A real mesocyclone should appear on at least MIN_VERTICAL_TILTS elevation
/// scans, with the horizontal position within MAX_HORIZONTAL_OFFSET_KM between
/// adjacent tilts. This eliminates single-scan noise detections.
fn vertical_continuity(
    clustered_by_sweep: &[Vec<SweepDetection>],
    _sweep_indices: &[usize],
    _sweeps: &[wx_radar::level2::Level2Sweep],
) -> Vec<SweepDetection> {
    if clustered_by_sweep.is_empty() {
        return Vec::new();
    }

    // Convert detection to (x, y) in km for distance calculations
    fn to_xy(d: &SweepDetection) -> (f64, f64) {
        let az_rad = (d.azimuth as f64).to_radians();
        let x = d.range_km as f64 * az_rad.sin();
        let y = d.range_km as f64 * az_rad.cos();
        (x, y)
    }

    // Start from lowest tilt detections
    let mut result = Vec::new();

    for base_det in &clustered_by_sweep[0] {
        let (bx, by) = to_xy(base_det);
        let mut tilt_count = 1usize;
        let mut best_rot_vel = base_det.rotational_velocity;
        let mut best_shear = base_det.max_shear;

        // Try to find matches on higher tilts
        for sweep_dets in clustered_by_sweep.iter().skip(1) {
            let mut closest_dist = MAX_HORIZONTAL_OFFSET_KM;
            let mut closest_det: Option<&SweepDetection> = None;
            for det in sweep_dets {
                let (dx, dy) = to_xy(det);
                let dist = ((bx - dx).powi(2) + (by - dy).powi(2)).sqrt();
                if dist < closest_dist {
                    closest_dist = dist;
                    closest_det = Some(det);
                }
            }
            if let Some(det) = closest_det {
                tilt_count += 1;
                if det.rotational_velocity > best_rot_vel {
                    best_rot_vel = det.rotational_velocity;
                }
                if det.max_shear > best_shear {
                    best_shear = det.max_shear;
                }
            }
        }

        if tilt_count >= MIN_VERTICAL_TILTS {
            result.push(SweepDetection {
                azimuth: base_det.azimuth,
                range_km: base_det.range_km,
                elevation: base_det.elevation,
                rotational_velocity: best_rot_vel,
                max_shear: best_shear,
                gate_count: base_det.gate_count,
            });
        }
    }

    result
}

/// Compute strength rank (1-5) from rotational velocity.
fn compute_strength_rank(rot_vel: f32) -> u8 {
    let abs_vel = rot_vel.abs();
    if abs_vel >= STRENGTH_THRESHOLDS[4] { 5 }
    else if abs_vel >= STRENGTH_THRESHOLDS[3] { 4 }
    else if abs_vel >= STRENGTH_THRESHOLDS[2] { 3 }
    else if abs_vel >= STRENGTH_THRESHOLDS[1] { 2 }
    else if abs_vel >= STRENGTH_THRESHOLDS[0] { 1 }
    else { 0 }
}
