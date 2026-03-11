use crate::state::AppState;
use crate::theme;
use egui::{Color32, FontId, Pos2, Stroke};
use std::collections::BTreeMap;

// ── Local data ─────────────────────────────────────────────────────

struct HodographData {
    pressure: Vec<f64>, // hPa, descending
    u_wind: Vec<f64>,   // knots (east component)
    v_wind: Vec<f64>,   // knots (north component)
}

// ── Height-layer colors (SHARPpy style) ────────────────────────────

const LAYER_COLORS: &[(f64, f64, Color32, &str)] = &[
    (0.0,     1000.0,  Color32::from_rgb(255, 0, 0),       "Sfc-1 km"),
    (1000.0,  3000.0,  Color32::from_rgb(255, 130, 0),     "1-3 km"),
    (3000.0,  6000.0,  Color32::from_rgb(0, 200, 0),       "3-6 km"),
    (6000.0,  9000.0,  Color32::from_rgb(0, 200, 255),     "6-9 km"),
    (9000.0,  12000.0, Color32::from_rgb(80, 80, 255),     "9-12 km"),
    (12000.0, 99999.0, Color32::from_rgb(180, 0, 255),     "12+ km"),
];

fn layer_color(h: f64) -> Color32 {
    for &(lo, hi, c, _) in LAYER_COLORS {
        if h >= lo && h < hi { return c; }
    }
    Color32::from_rgb(180, 0, 255)
}

// ── Helpers ────────────────────────────────────────────────────────

fn is_pressure_level(lt: u8) -> bool { lt == 100 || lt == 105 }

fn level_to_pressure_hpa(lt: u8, lv: f64) -> f64 {
    match lt {
        100 => if lv > 2000.0 { lv / 100.0 } else { lv },
        105 => {
            if lv <= 0.0 { 1013.0 }
            else if lv >= 50.0 { 50.0 }
            else { 1013.0 * (-lv * 0.06).exp() }
        }
        _ => lv,
    }
}

fn p_to_h_agl(p: f64, p_sfc: f64) -> f64 {
    7400.0 * (p_sfc / p).ln()
}

fn uv_to_screen(u: f64, v: f64, center: Pos2, scale: f32) -> Pos2 {
    Pos2::new(center.x + u as f32 * scale, center.y - v as f32 * scale)
}

fn interp_at_h(heights: &[f64], vals: &[f64], target: f64) -> Option<f64> {
    for i in 0..heights.len().saturating_sub(1) {
        if heights[i] <= target && heights[i + 1] >= target {
            let f = (target - heights[i]) / (heights[i + 1] - heights[i]);
            return Some(vals[i] + f * (vals[i + 1] - vals[i]));
        }
    }
    None
}

fn mean_wind(h: &[f64], u: &[f64], v: &[f64], h0: f64, h1: f64) -> Option<(f64, f64)> {
    let n = ((h1 - h0) / 250.0).ceil() as usize;
    if n == 0 { return None; }
    let (mut su, mut sv, mut c) = (0.0, 0.0, 0);
    for i in 0..=n {
        let hh = h0 + (h1 - h0) * i as f64 / n as f64;
        if let (Some(ui), Some(vi)) = (interp_at_h(h, u, hh), interp_at_h(h, v, hh)) {
            su += ui; sv += vi; c += 1;
        }
    }
    if c == 0 { None } else { Some((su / c as f64, sv / c as f64)) }
}

fn bulk_shear(h: &[f64], u: &[f64], v: &[f64], h0: f64, h1: f64) -> Option<f64> {
    let (u0, v0) = (interp_at_h(h, u, h0)?, interp_at_h(h, v, h0)?);
    let (u1, v1) = (interp_at_h(h, u, h1)?, interp_at_h(h, v, h1)?);
    Some(((u1 - u0).powi(2) + (v1 - v0).powi(2)).sqrt())
}

fn bunkers(h: &[f64], u: &[f64], v: &[f64]) -> Option<(f64, f64)> {
    let (mu, mv) = mean_wind(h, u, v, 0.0, 6000.0)?;
    let (u0, v0) = (interp_at_h(h, u, 0.0)?, interp_at_h(h, v, 0.0)?);
    let (u6, v6) = (interp_at_h(h, u, 6000.0)?, interp_at_h(h, v, 6000.0)?);
    let (su, sv) = (u6 - u0, v6 - v0);
    let sm = (su * su + sv * sv).sqrt();
    if sm < 0.5 { return Some((mu, mv)); }
    // 7.5 kt deviation perpendicular to shear, to the right
    Some((mu + sv / sm * 7.5, mv - su / sm * 7.5))
}

fn srh(h: &[f64], u: &[f64], v: &[f64], su: f64, sv: f64, h0: f64, h1: f64) -> f64 {
    // Collect data points within [h0, h1]
    let mut pts: Vec<(f64, f64, f64)> = Vec::new();
    for i in 0..h.len() {
        if h[i] >= h0 && h[i] <= h1 {
            pts.push((h[i], u[i], v[i]));
        }
    }
    if let (Some(ui), Some(vi)) = (interp_at_h(h, u, h0), interp_at_h(h, v, h0)) {
        pts.push((h0, ui, vi));
    }
    if let (Some(ui), Some(vi)) = (interp_at_h(h, u, h1), interp_at_h(h, v, h1)) {
        pts.push((h1, ui, vi));
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0);

    let mut val = 0.0;
    let kt_to_ms = 0.514444;
    for i in 0..pts.len().saturating_sub(1) {
        let (_, u1, v1) = pts[i];
        let (_, u2, v2) = pts[i + 1];
        let (sr1u, sr1v) = (u1 - su, v1 - sv);
        let (sr2u, sr2v) = (u2 - su, v2 - sv);
        val += (sr2u - sr1u) * (sr2v + sr1v) - (sr2v - sr1v) * (sr2u + sr1u);
    }
    val * 0.5 * kt_to_ms * kt_to_ms
}

// ── Data extraction ────────────────────────────────────────────────

fn extract_hodograph(
    grib: &rustmet_core::grib2::Grib2File,
    gi: usize,
    gj: usize,
) -> Option<HodographData> {
    use rustmet_core::grib2;

    let mut u_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut v_map: BTreeMap<i64, f64> = BTreeMap::new();

    for msg in &grib.messages {
        let lt = msg.product.level_type;
        if !is_pressure_level(lt) { continue; }
        let nx = msg.grid.nx as usize;
        let p_hpa = level_to_pressure_hpa(lt, msg.product.level_value);
        let p_key = (p_hpa * 100.0) as i64;
        let cat = msg.product.parameter_category;
        let num = msg.product.parameter_number;

        let get_val = |msg: &grib2::Grib2Message| -> Option<f64> {
            let vals = grib2::unpack_message_normalized(msg).ok()?;
            vals.get(gj * nx + gi).copied()
        };

        match (cat, num) {
            (2, 2) => { if let Some(v) = get_val(msg) { u_map.insert(p_key, v * 1.94384); } }
            (2, 3) => { if let Some(v) = get_val(msg) { v_map.insert(p_key, v * 1.94384); } }
            _ => {}
        }
    }

    if u_map.len() < 3 { return None; }

    let mut pressure = Vec::new();
    let mut u_wind = Vec::new();
    let mut v_wind = Vec::new();

    let keys: Vec<i64> = u_map.keys().rev().cloned().collect();
    for &k in &keys {
        if let (Some(&u), Some(&v)) = (u_map.get(&k), v_map.get(&k)) {
            pressure.push(k as f64 / 100.0);
            u_wind.push(u);
            v_wind.push(v);
        }
    }

    if pressure.len() < 3 { return None; }
    Some(HodographData { pressure, u_wind, v_wind })
}

// ── Main view function ─────────────────────────────────────────────

pub fn hodograph_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    let _ = ctx;

    if state.grib.is_none() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("Wind Hodograph").heading().color(theme::ACCENT));
                ui.add_space(16.0);
                ui.label(egui::RichText::new("Need wind data at pressure levels").color(theme::TEXT_DIM));
                ui.add_space(16.0);
                if ui.add(egui::Button::new("Go to Download").fill(theme::ACCENT)).clicked() {
                    state.active_view = crate::state::View::Download;
                }
            });
        });
        return;
    }

    let grib = state.grib.as_ref().unwrap();
    let n_wind = grib.messages.iter().filter(|m| {
        is_pressure_level(m.product.level_type) && m.product.parameter_category == 2 && m.product.parameter_number == 2
    }).count();

    // Controls
    ui.horizontal(|ui| {
        ui.label("Grid point:");
        ui.label("i:");
        let max_i = state.field_nx.max(1).saturating_sub(1) as f32;
        let mut gi = state.sounding_grid_i as f32;
        if ui.add(egui::DragValue::new(&mut gi).speed(1.0).range(0.0..=max_i)).changed() {
            state.sounding_grid_i = gi as usize;
        }
        ui.label("j:");
        let max_j = state.field_ny.max(1).saturating_sub(1) as f32;
        let mut gj = state.sounding_grid_j as f32;
        if ui.add(egui::DragValue::new(&mut gj).speed(1.0).range(0.0..=max_j)).changed() {
            state.sounding_grid_j = gj as usize;
        }
        ui.separator();
        ui.label(egui::RichText::new(format!("{n_wind} wind levels")).small().color(theme::TEXT_DIM));
    });
    ui.separator();

    if n_wind < 3 {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Not enough wind levels (need 3+). Download HRRR Full F000.").color(theme::WARNING));
        });
        return;
    }

    let hodo = match extract_hodograph(grib, state.sounding_grid_i, state.sounding_grid_j) {
        Some(h) => h,
        None => {
            ui.label(egui::RichText::new("Failed to extract wind profile").color(theme::WARNING));
            return;
        }
    };

    let p_sfc = hodo.pressure.first().copied().unwrap_or(1013.0);
    let heights: Vec<f64> = hodo.pressure.iter().map(|&p| p_to_h_agl(p, p_sfc)).collect();

    // ── Layout ─────────────────────────────────────────────────
    let avail = ui.available_size();
    // Hodograph square on left, params on right
    let params_w = 185.0_f32;
    let side = (avail.x - params_w - 20.0).min(avail.y).max(200.0);

    let total_w = side + params_w + 20.0;
    let (response, painter) = ui.allocate_painter(egui::vec2(total_w.min(avail.x), side.min(avail.y)), egui::Sense::hover());
    let full_rect = response.rect;

    let plot_rect = egui::Rect::from_min_size(
        full_rect.min,
        egui::vec2(side.min(full_rect.width() - params_w - 10.0), side.min(full_rect.height())),
    );
    let center = plot_rect.center();
    let radius = plot_rect.width().min(plot_rect.height()) * 0.42;

    // Auto-scale
    let mut max_wind = 60.0_f64;
    for i in 0..hodo.u_wind.len() {
        max_wind = max_wind.max((hodo.u_wind[i].powi(2) + hodo.v_wind[i].powi(2)).sqrt());
    }
    let max_ring = ((max_wind / 20.0).ceil() * 20.0).max(60.0);
    let scale = radius / max_ring as f32;

    // ── Background ─────────────────────────────────────────────
    painter.rect_filled(full_rect, 0.0, Color32::from_rgb(0, 0, 0));

    // ── Speed rings ────────────────────────────────────────────
    let ring_stroke = Stroke::new(0.5, Color32::from_rgb(50, 50, 60));
    let ring_font = FontId::new(9.0, egui::FontFamily::Monospace);
    let mut spd = 20.0_f64;
    while spd <= max_ring {
        let r = spd as f32 * scale;
        painter.circle_stroke(center, r, ring_stroke);
        painter.text(
            Pos2::new(center.x + 3.0, center.y - r - 1.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{}", spd as i32),
            ring_font.clone(),
            Color32::from_rgb(80, 80, 100),
        );
        spd += 20.0;
    }

    // ── Axes ───────────────────────────────────────────────────
    let axis_stroke = Stroke::new(0.6, Color32::from_rgb(60, 60, 75));
    painter.line_segment([Pos2::new(plot_rect.left() + 2.0, center.y), Pos2::new(plot_rect.right() - 2.0, center.y)], axis_stroke);
    painter.line_segment([Pos2::new(center.x, plot_rect.top() + 2.0), Pos2::new(center.x, plot_rect.bottom() - 2.0)], axis_stroke);

    // Cardinal labels
    let dir_font = FontId::new(12.0, egui::FontFamily::Proportional);
    let dc = Color32::from_rgb(140, 140, 160);
    let m = 12.0;
    painter.text(Pos2::new(center.x, plot_rect.top() + m), egui::Align2::CENTER_TOP, "N", dir_font.clone(), dc);
    painter.text(Pos2::new(center.x, plot_rect.bottom() - m), egui::Align2::CENTER_BOTTOM, "S", dir_font.clone(), dc);
    painter.text(Pos2::new(plot_rect.right() - m, center.y), egui::Align2::RIGHT_CENTER, "E", dir_font.clone(), dc);
    painter.text(Pos2::new(plot_rect.left() + m, center.y), egui::Align2::LEFT_CENTER, "W", dir_font.clone(), dc);

    // ── Wind trace (colored by height) ─────────────────────────
    if hodo.u_wind.len() >= 2 {
        // Draw thick trace segments colored by height layer
        for i in 0..hodo.u_wind.len() - 1 {
            let h_mid = (heights[i] + heights[i + 1]) / 2.0;
            let color = layer_color(h_mid);
            let p0 = uv_to_screen(hodo.u_wind[i], hodo.v_wind[i], center, scale);
            let p1 = uv_to_screen(hodo.u_wind[i + 1], hodo.v_wind[i + 1], center, scale);
            painter.line_segment([p0, p1], Stroke::new(2.5, color));
        }
        // Dots at data points
        for i in 0..hodo.u_wind.len() {
            let c = layer_color(heights[i]);
            let p = uv_to_screen(hodo.u_wind[i], hodo.v_wind[i], center, scale);
            painter.circle_filled(p, 3.0, c);
        }
    }

    // ── Bunkers storm motion ───────────────────────────────────
    let bunk = bunkers(&heights, &hodo.u_wind, &hodo.v_wind);
    if let Some((ru, rv)) = bunk {
        let pos = uv_to_screen(ru, rv, center, scale);
        painter.circle_filled(pos, 5.0, Color32::WHITE);
        painter.circle_stroke(pos, 5.0, Stroke::new(1.5, Color32::from_rgb(200, 200, 200)));
        painter.text(Pos2::new(pos.x + 8.0, pos.y - 2.0), egui::Align2::LEFT_CENTER, "RM",
            FontId::new(9.0, egui::FontFamily::Monospace), Color32::WHITE);
    }

    // ── Height layer legend ────────────────────────────────────
    let leg_x = plot_rect.left() + 6.0;
    let mut leg_y = plot_rect.bottom() - 6.0 - (LAYER_COLORS.len() as f32 * 13.0);
    let leg_font = FontId::new(9.0, egui::FontFamily::Monospace);
    // Legend background
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(leg_x - 3.0, leg_y - 3.0), egui::vec2(80.0, LAYER_COLORS.len() as f32 * 13.0 + 6.0)),
        3.0, Color32::from_rgba_premultiplied(0, 0, 0, 180),
    );
    for &(_, _, color, label) in LAYER_COLORS {
        let sw = egui::Rect::from_min_size(Pos2::new(leg_x, leg_y + 1.0), egui::vec2(8.0, 8.0));
        painter.rect_filled(sw, 1.0, color);
        painter.text(Pos2::new(leg_x + 12.0, leg_y + 5.0), egui::Align2::LEFT_CENTER, label, leg_font.clone(), Color32::from_rgb(180, 180, 200));
        leg_y += 13.0;
    }

    // ── Parameters panel (right of hodograph) ──────────────────
    {
        let font = FontId::new(11.0, egui::FontFamily::Monospace);
        let hdr_font = FontId::new(10.0, egui::FontFamily::Monospace);
        let px = plot_rect.right() + 16.0;
        let mut py = full_rect.top() + 8.0;
        let lh = 15.0_f32;

        // Background
        let panel_rect = egui::Rect::from_min_max(
            egui::pos2(px - 4.0, py - 4.0),
            egui::pos2(full_rect.right(), full_rect.bottom()),
        );
        painter.rect_filled(panel_rect, 4.0, Color32::from_rgba_premultiplied(0, 0, 0, 180));

        let txt = |py: &mut f32, text: &str, color: Color32| {
            painter.text(egui::pos2(px, *py), egui::Align2::LEFT_TOP, text, font.clone(), color);
            *py += lh;
        };
        let hdr = |py: &mut f32, text: &str| {
            *py += 4.0;
            painter.text(egui::pos2(px, *py), egui::Align2::LEFT_TOP, text, hdr_font.clone(), Color32::from_rgb(180, 180, 200));
            *py += lh + 2.0;
        };

        let lc = Color32::from_rgb(160, 160, 170);

        hdr(&mut py, "── SHEAR ──");
        if let Some(s) = bulk_shear(&heights, &hodo.u_wind, &hodo.v_wind, 0.0, 1000.0) {
            txt(&mut py, &format!("0-1km  {:>4.0} kt", s), lc);
        }
        if let Some(s) = bulk_shear(&heights, &hodo.u_wind, &hodo.v_wind, 0.0, 3000.0) {
            txt(&mut py, &format!("0-3km  {:>4.0} kt", s), lc);
        }
        if let Some(s) = bulk_shear(&heights, &hodo.u_wind, &hodo.v_wind, 0.0, 6000.0) {
            txt(&mut py, &format!("0-6km  {:>4.0} kt", s), lc);
        }

        if let Some((su, sv)) = bunk {
            hdr(&mut py, "── SRH ──");
            let s01 = srh(&heights, &hodo.u_wind, &hodo.v_wind, su, sv, 0.0, 1000.0);
            txt(&mut py, &format!("0-1km {:>5.0} m²/s²", s01),
                if s01.abs() > 150.0 { Color32::from_rgb(255, 200, 0) } else { lc });
            let s03 = srh(&heights, &hodo.u_wind, &hodo.v_wind, su, sv, 0.0, 3000.0);
            txt(&mut py, &format!("0-3km {:>5.0} m²/s²", s03),
                if s03.abs() > 250.0 { Color32::from_rgb(255, 100, 0) } else { lc });

            hdr(&mut py, "── STORM MOTION ──");
            let rm_spd = (su * su + sv * sv).sqrt();
            let rm_dir = (270.0 - sv.atan2(su).to_degrees()).rem_euclid(360.0);
            txt(&mut py, &format!("RM {:>3.0}°/{:.0} kt", rm_dir, rm_spd), Color32::WHITE);
        }

        // Mean winds
        hdr(&mut py, "── MEAN WIND ──");
        if let Some((mu, mv)) = mean_wind(&heights, &hodo.u_wind, &hodo.v_wind, 0.0, 6000.0) {
            let ms = (mu * mu + mv * mv).sqrt();
            let md = (270.0 - mv.atan2(mu).to_degrees()).rem_euclid(360.0);
            txt(&mut py, &format!("0-6km {:>3.0}°/{:.0} kt", md, ms), lc);
        }

        // Surface wind
        if !hodo.u_wind.is_empty() {
            hdr(&mut py, "── SURFACE ──");
            let su = hodo.u_wind[0];
            let sv = hodo.v_wind[0];
            let ss = (su * su + sv * sv).sqrt();
            let sd = (270.0 - sv.atan2(su).to_degrees()).rem_euclid(360.0);
            txt(&mut py, &format!("{:>3.0}° / {:.0} kt", sd, ss), lc);
        }
    }

    // ── Hover tooltip ──────────────────────────────────────────
    if let Some(hp) = response.hover_pos() {
        if plot_rect.contains(hp) {
            let u_h = ((hp.x - center.x) / scale) as f64;
            let v_h = ((center.y - hp.y) / scale) as f64;
            let spd = (u_h * u_h + v_h * v_h).sqrt();
            let dir = (270.0 - v_h.atan2(u_h).to_degrees()).rem_euclid(360.0);
            let tip = format!("{:.0}°/{:.0} kt", dir, spd);
            let tf = FontId::new(9.0, egui::FontFamily::Monospace);
            let gal = painter.layout_no_wrap(tip.clone(), tf.clone(), Color32::WHITE);
            let tp = hp + egui::vec2(12.0, -14.0);
            painter.rect_filled(
                egui::Rect::from_min_size(tp - egui::vec2(2.0, 1.0), gal.size() + egui::vec2(4.0, 2.0)),
                2.0, Color32::from_black_alpha(220),
            );
            painter.text(tp, egui::Align2::LEFT_TOP, &tip, tf, Color32::WHITE);

            // Crosshair
            let ch = Color32::from_white_alpha(30);
            painter.line_segment([Pos2::new(hp.x, plot_rect.top()), Pos2::new(hp.x, plot_rect.bottom())], Stroke::new(0.4, ch));
            painter.line_segment([Pos2::new(plot_rect.left(), hp.y), Pos2::new(plot_rect.right(), hp.y)], Stroke::new(0.4, ch));
        }
    }
}
