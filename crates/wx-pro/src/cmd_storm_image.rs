use crate::output::{print_json, print_error};
use crate::cmd_radar::{find_nearest_site, find_latest_file, http_get_bytes, maybe_decompress_gz};
use crate::basemap;
use serde_json::json;
use std::path::PathBuf;

use wx_radar::level2::Level2File;
use wx_radar::products::RadarProduct;
use wx_radar::render::render_ppi;
use rustmet_core::render::encode::write_png;

const NEXRAD_BASE_URL: &str = "https://unidata-nexrad-level2.s3.amazonaws.com";

fn image_dir() -> PathBuf {
    let dir = dirs_home().join(".wx-pro").join("images");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn run(
    site: &str,
    lat: Option<f64>,
    lon: Option<f64>,
    size: u32,
    ansi: bool,
    ansi_width: u32,
    ansi_mode: &str,
    pretty: bool,
) {
    let site_id = if let (Some(la), Some(lo)) = (lat, lon) {
        find_nearest_site(la, lo)
    } else if !site.is_empty() {
        site.to_uppercase()
    } else {
        print_error("Provide --site KTLX or --lat/--lon");
    };

    let site_info = wx_radar::sites::find_site(&site_id);
    let (site_lat, site_lon) = site_info.as_ref()
        .map(|s| (s.lat, s.lon))
        .unwrap_or((0.0, 0.0));

    // Download latest volume
    let now = chrono::Utc::now();
    let today = now.format("%Y/%m/%d").to_string();
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y/%m/%d").to_string();

    let key = find_latest_file(&site_id, &today)
        .or_else(|| find_latest_file(&site_id, &yesterday))
        .unwrap_or_else(|| print_error(&format!("No NEXRAD files found for {} in last 24h", site_id)));

    let filename = key.rsplit('/').next().unwrap_or(&key).to_string();

    let download_start = std::time::Instant::now();
    let url = format!("{}/{}", NEXRAD_BASE_URL, key);
    let raw_data = http_get_bytes(&url);
    let data = maybe_decompress_gz(raw_data);
    let download_ms = download_start.elapsed().as_millis();

    let l2 = match Level2File::parse(&data) {
        Ok(f) => f,
        Err(e) => print_error(&format!("Failed to parse Level 2: {}", e)),
    };

    let timestamp = l2.timestamp_string();

    // Find lowest reflectivity sweep
    let sweep_idx = l2.sweeps.iter().position(|s| {
        s.radials.iter().any(|r| {
            r.moments.iter().any(|m| m.product == RadarProduct::Reflectivity)
        })
    }).unwrap_or_else(|| print_error("No reflectivity data in volume scan"));

    let sweep = &l2.sweeps[sweep_idx];
    let elev = sweep.elevation_angle;

    // Render base PPI
    let render_start = std::time::Instant::now();
    let ppi = match render_ppi(sweep, RadarProduct::Reflectivity, size) {
        Some(p) => p,
        None => print_error("Failed to render PPI"),
    };
    let render_ms = render_start.elapsed().as_millis();

    let mut pixels = ppi.pixels;
    let img_size = ppi.size as usize;
    let range_km = ppi.range_km;

    // Draw basemap
    let center = img_size as f64 / 2.0;
    let px_per_km = center / range_km;

    // Range rings
    for ring_km in [50.0, 100.0, 150.0, 200.0] {
        let ring_px = ring_km * px_per_km;
        if ring_px > 0.0 && ring_px < center {
            draw_circle(&mut pixels, img_size, center, center, ring_px, [180, 180, 180, 100]);
        }
    }

    // Crosshairs
    for i in 0..img_size {
        blend_pixel(&mut pixels, img_size, img_size / 2, i, [120, 120, 120, 60]);
        blend_pixel(&mut pixels, img_size, i, img_size / 2, [120, 120, 120, 60]);
    }

    // Basemap overlays
    if site_info.is_some() {
        let km_per_deg_lat = 111.139;
        basemap::draw_basemap(&mut pixels, img_size, img_size, |blat, blon| {
            let dy_km = (blat - site_lat) * km_per_deg_lat;
            let dx_km = (blon - site_lon) * km_per_deg_lat * site_lat.to_radians().cos();
            if dx_km.abs() > range_km || dy_km.abs() > range_km {
                return None;
            }
            let px = center + dx_km * px_per_km;
            let py = center - dy_km * px_per_km;
            if px < -1.0 || px >= img_size as f64 + 1.0 || py < -1.0 || py >= img_size as f64 + 1.0 {
                return None;
            }
            Some((px, py))
        });
    }

    // Radar site dot
    {
        let cx = img_size / 2;
        let cy = img_size / 2;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let px = (cx as i32 + dx) as usize;
                let py = (cy as i32 + dy) as usize;
                if px < img_size && py < img_size {
                    set_pixel(&mut pixels, img_size, px, py, [255, 255, 255, 255]);
                }
            }
        }
    }

    // --- Cell identification ---
    let algo_start = std::time::Instant::now();
    let cells = wx_radar::cells::identify_cells(
        sweep, Some(site_lat), Some(site_lon),
    );

    // --- Rotation detection (simplified inline) ---
    let mesos = detect_mesos_for_overlay(&l2);

    // Associate mesos with cells
    let meso_azimuths: Vec<f32> = mesos.iter().map(|m| m.azimuth).collect();
    let meso_ranges: Vec<f32> = mesos.iter().map(|m| m.range_km).collect();
    let associations = wx_radar::cells::associate_mesos_with_cells(
        &cells, &meso_azimuths, &meso_ranges, 15.0,
    );
    let algo_ms = algo_start.elapsed().as_millis();

    // --- Draw cell overlays ---
    // Limit to top 20 cells for readability
    let display_cells = cells.iter().take(20);

    for cell in display_cells {
        let az_rad = (cell.centroid_azimuth as f64).to_radians();
        let px_x = center + (cell.centroid_range_km as f64 * az_rad.sin()) * px_per_km;
        let px_y = center - (cell.centroid_range_km as f64 * az_rad.cos()) * px_per_km;

        if px_x < 5.0 || px_x >= (img_size - 40) as f64 || px_y < 5.0 || px_y >= (img_size - 15) as f64 {
            continue;
        }

        let has_meso = associations.iter()
            .enumerate()
            .any(|(_, &assoc)| assoc == cell.label);

        // Draw cell marker
        if has_meso {
            // Red triangle for meso cells
            draw_meso_marker(&mut pixels, img_size, px_x as i32, px_y as i32);
        } else {
            // Cyan crosshair for regular cells
            draw_cell_marker(&mut pixels, img_size, px_x as i32, px_y as i32);
        }

        // Draw label "C1", "C2", etc.
        let label = format!("C{}", cell.label);
        let label_x = px_x as i32 + 6;
        let label_y = px_y as i32 - 4;

        // Background box for readability
        let text_w = label.len() as i32 * 6 + 2;
        draw_filled_rect(&mut pixels, img_size, label_x - 1, label_y - 1, text_w, 10, [0, 0, 0, 200]);

        // Text color: red for meso, cyan for regular
        let text_color = if has_meso {
            (255, 80, 80)
        } else {
            (0, 255, 255)
        };
        draw_text(&mut pixels, img_size, label_x, label_y, &label, text_color);

        // If meso, add rot velocity label
        if has_meso {
            if let Some(mi) = associations.iter()
                .enumerate()
                .find(|(_, &assoc)| assoc == cell.label)
                .map(|(mi, _)| mi)
            {
                if let Some(m) = mesos.get(mi) {
                    let rot_label = format!("{:.0}m/s", m.rot_vel);
                    let rot_x = label_x;
                    let rot_y = label_y + 10;
                    let rot_w = rot_label.len() as i32 * 6 + 2;
                    draw_filled_rect(&mut pixels, img_size, rot_x - 1, rot_y - 1, rot_w, 10, [0, 0, 0, 200]);
                    draw_text(&mut pixels, img_size, rot_x, rot_y, &rot_label, (255, 200, 0));
                }
            }
        }
    }

    // Draw standalone mesos not associated with any cell
    for (mi, m) in mesos.iter().enumerate() {
        if associations.get(mi).copied().unwrap_or(0) != 0 {
            continue; // already drawn with cell
        }
        let az_rad = (m.azimuth as f64).to_radians();
        let px_x = center + (m.range_km as f64 * az_rad.sin()) * px_per_km;
        let px_y = center - (m.range_km as f64 * az_rad.cos()) * px_per_km;
        if px_x < 5.0 || px_x >= (img_size - 20) as f64 || px_y < 5.0 || px_y >= (img_size - 15) as f64 {
            continue;
        }
        draw_meso_marker(&mut pixels, img_size, px_x as i32, px_y as i32);
    }

    // Info box in top-left
    let info_lines = [
        format!("{} — {}", site_id, site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown")),
        format!("{}", timestamp),
        format!("Cells: {}  Mesos: {}", cells.len(),
            associations.iter().filter(|&&a| a > 0).collect::<std::collections::HashSet<_>>().len()),
    ];
    let box_w = 260i32;
    let box_h = (info_lines.len() as i32) * 10 + 4;
    draw_filled_rect(&mut pixels, img_size, 4, 4, box_w, box_h, [0, 0, 0, 220]);
    for (i, line) in info_lines.iter().enumerate() {
        draw_text(&mut pixels, img_size, 7, 7 + i as i32 * 10, line, (255, 255, 255));
    }

    // Legend in bottom-left
    draw_filled_rect(&mut pixels, img_size, 4, img_size as i32 - 28, 140, 24, [0, 0, 0, 220]);
    draw_cell_marker(&mut pixels, img_size, 10, img_size as i32 - 20);
    draw_text(&mut pixels, img_size, 18, img_size as i32 - 24, "Cell", (0, 255, 255));
    draw_meso_marker(&mut pixels, img_size, 60, img_size as i32 - 20);
    draw_text(&mut pixels, img_size, 68, img_size as i32 - 24, "Meso", (255, 80, 80));

    // ANSI terminal output
    if ansi {
        let mode = rustmet_core::render::ansi::AnsiMode::from_str(ansi_mode);
        let ansi_str = rustmet_core::render::ansi::rgba_to_ansi_mode(&pixels, ppi.size, ppi.size, ansi_width, mode);
        eprint!("{}", ansi_str);
    }

    // Save
    let ts = now.format("%Y%m%d_%H%M%S").to_string();
    let out_path = image_dir().join(format!("{}_STORM_{}.png", site_id, ts));
    if let Err(e) = write_png(&pixels, ppi.size, ppi.size, &out_path) {
        print_error(&format!("Failed to write PNG: {}", e));
    }

    let out_path_str = out_path.to_string_lossy().to_string();

    // JSON with both the image path and cell data
    let cells_json: Vec<serde_json::Value> = cells.iter().take(20).map(|cell| {
        let has_meso = associations.iter()
            .enumerate()
            .any(|(_, &assoc)| assoc == cell.label);
        let mut obj = json!({
            "id": format!("C{}", cell.label),
            "lat": cell.lat,
            "lon": cell.lon,
            "max_reflectivity_dbz": cell.max_reflectivity,
            "area_km2": cell.area_km2,
        });
        if has_meso {
            if let Some(mi) = associations.iter()
                .enumerate()
                .find(|(_, &assoc)| assoc == cell.label)
                .map(|(mi, _)| mi)
            {
                if let Some(m) = mesos.get(mi) {
                    obj["mesocyclone"] = json!({
                        "rotational_velocity_ms": (m.rot_vel * 10.0).round() / 10.0,
                        "strength_rank": m.strength,
                    });
                }
            }
        }
        obj
    }).collect();

    let meso_count = cells_json.iter().filter(|c| c.get("mesocyclone").is_some()).count();

    print_json(&json!({
        "image_path": out_path_str,
        "site": site_id,
        "site_name": site_info.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown"),
        "scan_time": timestamp,
        "elevation_deg": (elev * 10.0).round() / 10.0,
        "image_size": ppi.size,
        "file": filename,
        "summary": {
            "total_cells": cells.len(),
            "displayed_cells": cells_json.len(),
            "cells_with_mesocyclone": meso_count,
        },
        "cells": cells_json,
        "performance": {
            "download_ms": download_ms,
            "render_ms": render_ms,
            "algorithm_ms": algo_ms,
            "file_size_bytes": data.len(),
        },
    }), pretty);
}

// ── Simplified meso detection for overlay ───────────────────────────

struct OverlayMeso {
    azimuth: f32,
    range_km: f32,
    rot_vel: f32,
    strength: u8,
}

fn detect_mesos_for_overlay(l2: &Level2File) -> Vec<OverlayMeso> {
    const MIN_REF: f32 = 35.0;
    const MIN_ROT_VEL: f32 = 20.0;
    const MIN_CLUSTER_GATES: usize = 3;
    const MIN_VERTICAL_TILTS: usize = 3;
    const MAX_HORIZ_OFFSET_KM: f64 = 10.0;
    const STRENGTH_THRESHOLDS: [f32; 5] = [20.0, 25.0, 33.0, 46.0, 60.0];

    let mut sweep_elevs: Vec<(usize, f32)> = l2.sweeps.iter()
        .enumerate()
        .map(|(i, s)| (i, s.elevation_angle))
        .collect();
    sweep_elevs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    // Deduplicate SAILS/MESO-SAILS: keep only unique elevations (>0.3° apart)
    let mut unique_elevs: Vec<(usize, f32)> = Vec::new();
    for &(idx, elev) in &sweep_elevs {
        if unique_elevs.last().map_or(true, |&(_, prev)| (elev - prev).abs() > 0.3) {
            unique_elevs.push((idx, elev));
        }
    }

    // Phase 1: Scan each sweep independently, cluster per-sweep
    let mut per_sweep_clusters: Vec<Vec<OverlayMeso>> = Vec::new();

    for &(si, _) in unique_elevs.iter().take(4) {
        let sweep = &l2.sweeps[si];
        let n = sweep.radials.len();
        if n < 3 {
            per_sweep_clusters.push(Vec::new());
            continue;
        }

        let ref_data: Vec<Vec<f32>> = sweep.radials.iter().map(|r| {
            r.moments.iter()
                .find(|m| m.product == RadarProduct::Reflectivity)
                .map(|m| m.data.clone())
                .unwrap_or_default()
        }).collect();

        let mut candidates: Vec<(f32, f32, f32)> = Vec::new(); // (az, range_km, rot_vel)

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

                    let delta_v = v2 - v1; // positive = cyclonic (NH)
                    let rot_vel = delta_v / 2.0;
                    if rot_vel < MIN_ROT_VEL { continue; }

                    let range_km = first_gate_km + gi as f32 * gate_size_km;

                    // Reflectivity co-location
                    if i < ref_data.len() && !ref_data[i].is_empty() {
                        let ref_gs = sweep.radials[i].moments.iter()
                            .find(|m| m.product == RadarProduct::Reflectivity)
                            .map(|m| m.gate_size as f32 / 1000.0)
                            .unwrap_or(0.25);
                        let ref_fg = sweep.radials[i].moments.iter()
                            .find(|m| m.product == RadarProduct::Reflectivity)
                            .map(|m| m.first_gate_range as f32 / 1000.0)
                            .unwrap_or(0.0);
                        let ref_gi = if ref_gs > 0.0 {
                            ((range_km - ref_fg) / ref_gs).round() as usize
                        } else { 0 };
                        if ref_gi < ref_data[i].len() {
                            let rv = ref_data[i][ref_gi];
                            if rv.is_nan() || rv < MIN_REF { continue; }
                        } else { continue; }
                    } else { continue; }

                    let avg_az = {
                        let a1 = sweep.radials[i].azimuth;
                        let a2 = sweep.radials[next].azimuth;
                        if (a2 - a1).abs() > 180.0 {
                            ((a1 + a2 + 360.0) / 2.0) % 360.0
                        } else {
                            (a1 + a2) / 2.0
                        }
                    };

                    candidates.push((avg_az, range_km, rot_vel));
                }
            }
        }

        // Cluster within this sweep using union-find
        let cn = candidates.len();
        let mut parent: Vec<usize> = (0..cn).collect();
        let mut rank: Vec<usize> = vec![0; cn];

        fn uf_find(p: &mut [usize], x: usize) -> usize {
            if p[x] != x { p[x] = uf_find(p, p[x]); }
            p[x]
        }
        fn uf_union(p: &mut [usize], r: &mut [usize], a: usize, b: usize) {
            let ra = uf_find(p, a);
            let rb = uf_find(p, b);
            if ra == rb { return; }
            if r[ra] < r[rb] { p[ra] = rb; }
            else if r[ra] > r[rb] { p[rb] = ra; }
            else { p[rb] = ra; r[ra] += 1; }
        }

        for i in 0..cn {
            for j in (i+1)..cn {
                let az_d = {
                    let d = (candidates[i].0 - candidates[j].0).abs();
                    if d > 180.0 { 360.0 - d } else { d }
                };
                let rng_d = (candidates[i].1 - candidates[j].1).abs();
                if az_d <= 2.0 && rng_d <= 0.75 {
                    uf_union(&mut parent, &mut rank, i, j);
                }
            }
        }

        let mut components: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..cn {
            components.entry(uf_find(&mut parent, i)).or_default().push(i);
        }

        let mut sweep_mesos = Vec::new();
        for (_, indices) in &components {
            if indices.len() < MIN_CLUSTER_GATES { continue; }

            let mut sum_ax: f64 = 0.0;
            let mut sum_ay: f64 = 0.0;
            let mut sum_rng: f64 = 0.0;
            let mut max_rv: f32 = 0.0;
            for &idx in indices {
                let (az, rng, rv) = candidates[idx];
                let ar = (az as f64).to_radians();
                sum_ax += ar.cos();
                sum_ay += ar.sin();
                sum_rng += rng as f64;
                if rv > max_rv { max_rv = rv; }
            }
            let cnt = indices.len() as f64;
            let caz = (sum_ay.atan2(sum_ax).to_degrees() as f32 + 360.0) % 360.0;
            let crng = (sum_rng / cnt) as f32;

            let strength = if max_rv >= STRENGTH_THRESHOLDS[4] { 5 }
                else if max_rv >= STRENGTH_THRESHOLDS[3] { 4 }
                else if max_rv >= STRENGTH_THRESHOLDS[2] { 3 }
                else if max_rv >= STRENGTH_THRESHOLDS[1] { 2 }
                else { 1 };

            sweep_mesos.push(OverlayMeso {
                azimuth: caz,
                range_km: crng,
                rot_vel: max_rv,
                strength,
            });
        }

        per_sweep_clusters.push(sweep_mesos);
    }

    // Phase 2: Vertical continuity — require detection on >= 3 tilts within 10 km
    if per_sweep_clusters.is_empty() || per_sweep_clusters[0].is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for base in &per_sweep_clusters[0] {
        let baz = (base.azimuth as f64).to_radians();
        let bx = base.range_km as f64 * baz.sin();
        let by = base.range_km as f64 * baz.cos();

        let mut tilt_count = 1usize;
        let mut best_rv = base.rot_vel;

        for sweep_mesos in per_sweep_clusters.iter().skip(1) {
            let mut closest_dist = MAX_HORIZ_OFFSET_KM;
            let mut closest_rv: Option<f32> = None;
            for det in sweep_mesos {
                let daz = (det.azimuth as f64).to_radians();
                let dx = det.range_km as f64 * daz.sin();
                let dy = det.range_km as f64 * daz.cos();
                let dist = ((bx - dx).powi(2) + (by - dy).powi(2)).sqrt();
                if dist < closest_dist {
                    closest_dist = dist;
                    closest_rv = Some(det.rot_vel);
                }
            }
            if let Some(rv) = closest_rv {
                tilt_count += 1;
                if rv > best_rv { best_rv = rv; }
            }
        }

        if tilt_count >= MIN_VERTICAL_TILTS {
            let strength = if best_rv >= STRENGTH_THRESHOLDS[4] { 5 }
                else if best_rv >= STRENGTH_THRESHOLDS[3] { 4 }
                else if best_rv >= STRENGTH_THRESHOLDS[2] { 3 }
                else if best_rv >= STRENGTH_THRESHOLDS[1] { 2 }
                else { 1 };

            result.push(OverlayMeso {
                azimuth: base.azimuth,
                range_km: base.range_km,
                rot_vel: best_rv,
                strength,
            });
        }
    }

    result
}

// ── Drawing primitives ──────────────────────────────────────────────

fn draw_circle(pixels: &mut [u8], img_size: usize, cx: f64, cy: f64, radius: f64, color: [u8; 4]) {
    let steps = (radius * 4.0).max(360.0) as usize;
    for i in 0..steps {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
        let px = (cx + radius * angle.cos()) as usize;
        let py = (cy - radius * angle.sin()) as usize;
        if px < img_size && py < img_size {
            blend_pixel(pixels, img_size, px, py, color);
        }
    }
}

fn draw_cell_marker(pixels: &mut [u8], img_size: usize, cx: i32, cy: i32) {
    // Cyan crosshair +
    let color = [0, 255, 255, 255];
    for d in -4i32..=4 {
        let px = (cx + d) as usize;
        let py = cy as usize;
        if px < img_size && py < img_size {
            set_pixel(pixels, img_size, px, py, color);
        }
        let px2 = cx as usize;
        let py2 = (cy + d) as usize;
        if px2 < img_size && py2 < img_size {
            set_pixel(pixels, img_size, px2, py2, color);
        }
    }
}

fn draw_meso_marker(pixels: &mut [u8], img_size: usize, cx: i32, cy: i32) {
    // Red triangle pointing up + circle
    let color = [255, 80, 80, 255];

    // Circle (radius 5)
    for angle_i in 0..36 {
        let angle = (angle_i as f64) * 10.0 * std::f64::consts::PI / 180.0;
        let px = (cx as f64 + 5.0 * angle.cos()) as usize;
        let py = (cy as f64 - 5.0 * angle.sin()) as usize;
        if px < img_size && py < img_size {
            set_pixel(pixels, img_size, px, py, color);
        }
    }

    // Inner filled triangle
    for row in 0..5i32 {
        let y = cy - 3 + row;
        let half_w = (5 - row) / 2;
        for dx in -half_w..=half_w {
            let x = cx + dx;
            if x >= 0 && y >= 0 && (x as usize) < img_size && (y as usize) < img_size {
                set_pixel(pixels, img_size, x as usize, y as usize, color);
            }
        }
    }
}

fn draw_filled_rect(pixels: &mut [u8], img_size: usize, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
    for dy in 0..h {
        for dx in 0..w {
            let px = (x + dx) as usize;
            let py = (y + dy) as usize;
            if px < img_size && py < img_size {
                blend_pixel(pixels, img_size, px, py, color);
            }
        }
    }
}

fn blend_pixel(pixels: &mut [u8], img_size: usize, x: usize, y: usize, color: [u8; 4]) {
    let idx = (y * img_size + x) * 4;
    if idx + 3 >= pixels.len() { return; }
    let alpha = color[3] as f32 / 255.0;
    let inv = 1.0 - alpha;
    pixels[idx] = (pixels[idx] as f32 * inv + color[0] as f32 * alpha) as u8;
    pixels[idx + 1] = (pixels[idx + 1] as f32 * inv + color[1] as f32 * alpha) as u8;
    pixels[idx + 2] = (pixels[idx + 2] as f32 * inv + color[2] as f32 * alpha) as u8;
    pixels[idx + 3] = pixels[idx + 3].max(color[3]);
}

fn set_pixel(pixels: &mut [u8], img_size: usize, x: usize, y: usize, color: [u8; 4]) {
    let idx = (y * img_size + x) * 4;
    if idx + 3 >= pixels.len() { return; }
    pixels[idx] = color[0];
    pixels[idx + 1] = color[1];
    pixels[idx + 2] = color[2];
    pixels[idx + 3] = color[3];
}

// ── Bitmap font (5x7) ──────────────────────────────────────────────

fn draw_text(pixels: &mut [u8], img_size: usize, x: i32, y: i32, text: &str, color: (u8, u8, u8)) {
    let mut cx = x;
    for ch in text.chars() {
        let glyph = get_glyph(ch);
        for row in 0..7 {
            for col in 0..5 {
                if glyph[row] & (1 << (4 - col)) != 0 {
                    let px = cx + col as i32;
                    let py = y + row as i32;
                    if px >= 0 && py >= 0 && (px as usize) < img_size && (py as usize) < img_size {
                        set_pixel(pixels, img_size, px as usize, py as usize,
                            [color.0, color.1, color.2, 255]);
                    }
                }
            }
        }
        cx += 6;
    }
}

fn get_glyph(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111],
        '3' => [0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'm' => [0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        's' => [0b00000, 0b00000, 0b01110, 0b10000, 0b01110, 0b00001, 0b11110],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        ' ' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        ':' => [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'N' => [0b10001, 0b11001, 0b10101, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        'a' => [0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111],
        'c' => [0b00000, 0b00000, 0b01110, 0b10000, 0b10000, 0b10001, 0b01110],
        'e' => [0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110],
        'i' => [0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110],
        'l' => [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'n' => [0b00000, 0b00000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001],
        'o' => [0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        'r' => [0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000],
        't' => [0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110],
        'u' => [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101],
        'w' => [0b00000, 0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010],
        'y' => [0b00000, 0b00000, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110],
        _ => [0b01110, 0b01010, 0b01010, 0b01010, 0b01010, 0b00000, 0b01010], // '?'
    }
}
