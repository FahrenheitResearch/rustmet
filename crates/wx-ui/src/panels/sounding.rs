use crate::state::AppState;
use crate::theme;
use std::collections::BTreeMap;
use wx_math::thermo::{self, ROCP, ZEROCNK};

// ── Local sounding data ────────────────────────────────────────────

struct SoundingData {
    pressure: Vec<f64>,      // hPa, surface-first (descending)
    temperature: Vec<f64>,   // Celsius
    dewpoint: Vec<f64>,      // Celsius
    wind_speed: Vec<f64>,    // knots
    wind_dir: Vec<f64>,      // degrees (meteorological)
    wind_pressure: Vec<f64>, // hPa for wind levels
}

// ── Skew-T constants ───────────────────────────────────────────────

const P_BOT: f64 = 1050.0;
const P_TOP: f64 = 100.0;
const T_MIN: f64 = -40.0; // at bottom of plot
const T_MAX: f64 = 50.0;  // at bottom of plot
const T_RANGE: f64 = T_MAX - T_MIN; // 90

const ISOBAR_LEVELS: &[f64] = &[
    1000.0, 925.0, 850.0, 700.0, 500.0, 400.0, 300.0, 250.0, 200.0, 150.0, 100.0,
];

const MIXING_RATIOS: &[f64] = &[0.4, 1.0, 2.0, 4.0, 7.0, 10.0, 15.0, 20.0, 30.0];

// ── Colors (SHARPpy dark theme) ────────────────────────────────────

const BG_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 0, 0);
const ISOBAR_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 60, 60);
const ISOTHERM_COLOR: egui::Color32 = egui::Color32::from_rgb(50, 50, 55);
const ZERO_C_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 180, 255);
const DRY_ADIABAT_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 120, 50);
const MOIST_ADIABAT_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 130, 80);
const MIXING_LINE_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 100, 60);
const TEMP_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 0, 0);
const DEWP_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 220, 0);
const PARCEL_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
const WETBULB_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 180, 220);
const BARB_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 220, 220);
const CAPE_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(255, 0, 0, 50);
const CIN_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(0, 80, 255, 40);
const LABEL_COLOR: egui::Color32 = egui::Color32::from_rgb(160, 160, 170);

// ── Coordinate transform ───────────────────────────────────────────

/// Convert (T in Celsius, P in hPa) -> screen position.
/// Uses standard Skew-T projection: 45° rotation, log-pressure Y axis.
#[inline]
fn tp_to_screen(t_c: f64, p_hpa: f64, rect: &egui::Rect) -> egui::Pos2 {
    let log_bot = P_BOT.ln();
    let log_top = P_TOP.ln();
    let log_p = p_hpa.clamp(P_TOP, P_BOT).ln();

    // y: 0 at bottom (high pressure), 1 at top (low pressure)
    let y_norm = (log_bot - log_p) / (log_bot - log_top); // 0..1

    // Skew: at 45° rotation, isotherms shift right by T_RANGE * y_norm
    // This means an isotherm covers the full T-range width over the full height
    let t_shifted = t_c + T_RANGE * y_norm;
    let x_norm = (t_shifted - T_MIN) / T_RANGE;

    egui::pos2(
        rect.left() + x_norm as f32 * rect.width(),
        rect.bottom() - y_norm as f32 * rect.height(),
    )
}

/// Inverse: screen position -> (T in Celsius, P in hPa)
fn screen_to_tp(pos: egui::Pos2, rect: &egui::Rect) -> (f64, f64) {
    let log_bot = P_BOT.ln();
    let log_top = P_TOP.ln();

    let y_norm = (rect.bottom() - pos.y) as f64 / rect.height() as f64;
    let x_norm = (pos.x - rect.left()) as f64 / rect.width() as f64;

    let log_p = log_bot - y_norm * (log_bot - log_top);
    let p = log_p.exp();

    let t_shifted = T_MIN + x_norm * T_RANGE;
    let t = t_shifted - T_RANGE * y_norm;

    (t, p)
}

// ── Generate pressure steps (log-spaced) ───────────────────────────

fn pressure_steps(n: usize) -> Vec<f64> {
    let log_bot = P_BOT.ln();
    let log_top = P_TOP.ln();
    (0..=n)
        .map(|i| {
            let frac = i as f64 / n as f64;
            (log_bot * (1.0 - frac) + log_top * frac).exp()
        })
        .collect()
}

// ── Main view function ─────────────────────────────────────────────

pub fn sounding_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    let _ = ctx;

    if state.grib.is_none() {
        show_placeholder(ui, state, "Skew-T Log-P Diagram", "pressure-level data");
        return;
    }

    let grib = state.grib.as_ref().unwrap();
    let n_plevels = grib.messages.iter().filter(|m| {
        is_pressure_level(m.product.level_type)
            && m.product.parameter_category == 0
            && m.product.parameter_number == 0
    }).count();

    // ── Top controls ───────────────────────────────────────────
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
        ui.label(egui::RichText::new(format!("{n_plevels} pressure levels")).small().color(theme::TEXT_DIM));
    });
    ui.separator();

    if n_plevels < 3 {
        show_placeholder(ui, state, "Not enough data", "at least 3 pressure levels");
        return;
    }

    // ── Extract data ───────────────────────────────────────────
    let grib = state.grib.as_ref().unwrap();
    let Some(data) = extract_sounding(grib, state.sounding_grid_i, state.sounding_grid_j) else {
        ui.label(egui::RichText::new("Could not extract sounding at this grid point").color(theme::WARNING));
        return;
    };
    let derived = compute_derived(&data);

    // ── Layout: Skew-T (left ~70%), params (right ~30%) ────────
    let avail = ui.available_size();
    let (response, painter) = ui.allocate_painter(avail, egui::Sense::hover());
    let full_rect = response.rect;

    // Skew-T plot area
    let margin_l = 48.0_f32;
    let margin_r = 60.0_f32; // wind barbs
    let margin_t = 8.0_f32;
    let margin_b = 24.0_f32;
    let params_w = 185.0_f32;

    let plot_rect = egui::Rect::from_min_max(
        egui::pos2(full_rect.left() + margin_l, full_rect.top() + margin_t),
        egui::pos2(full_rect.right() - margin_r - params_w, full_rect.bottom() - margin_b),
    );

    // Fill backgrounds
    painter.rect_filled(full_rect, 0.0, BG_COLOR);
    painter.rect_filled(plot_rect, 0.0, BG_COLOR);

    // Create a clipped painter for the plot area — all background lines
    // and traces will be automatically clipped to the plot bounds
    let clip = painter.with_clip_rect(plot_rect);

    let pressures = pressure_steps(120);
    let label_font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let tiny_font = egui::FontId::new(8.5, egui::FontFamily::Monospace);

    // ── 1. Isobars ─────────────────────────────────────────────
    for &p in ISOBAR_LEVELS {
        let y = tp_to_screen(0.0, p, &plot_rect).y;
        clip.line_segment(
            [egui::pos2(plot_rect.left(), y), egui::pos2(plot_rect.right(), y)],
            egui::Stroke::new(0.7, ISOBAR_COLOR),
        );
        // Label left of plot
        painter.text(
            egui::pos2(plot_rect.left() - 3.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{:.0}", p),
            label_font.clone(),
            LABEL_COLOR,
        );
    }

    // ── 2. Isotherms (every 10°C) ─────────────────────────────
    {
        let mut t = -120.0_f64;
        while t <= 60.0 {
            let pts: Vec<egui::Pos2> = pressures.iter()
                .map(|&p| tp_to_screen(t, p, &plot_rect))
                .collect();
            let (color, width) = if (t - 0.0).abs() < 0.1 {
                (ZERO_C_COLOR, 1.2)
            } else if t % 20.0 == 0.0 {
                (ISOTHERM_COLOR, 0.6)
            } else {
                (ISOTHERM_COLOR, 0.35)
            };
            clip.add(egui::Shape::line(pts, egui::Stroke::new(width, color)));

            // Label at bottom
            let bp = tp_to_screen(t, P_BOT, &plot_rect);
            if bp.x > plot_rect.left() + 5.0 && bp.x < plot_rect.right() - 5.0 {
                painter.text(
                    egui::pos2(bp.x, plot_rect.bottom() + 2.0),
                    egui::Align2::CENTER_TOP,
                    format!("{:.0}", t),
                    tiny_font.clone(),
                    LABEL_COLOR,
                );
            }
            t += 10.0;
        }
    }

    // ── 3. Dry adiabats ───────────────────────────────────────
    {
        let mut theta_c = -30.0_f64;
        while theta_c <= 250.0 {
            let theta_k = theta_c + ZEROCNK;
            let pts: Vec<egui::Pos2> = pressures.iter()
                .map(|&p| {
                    let t = theta_k * (p / 1000.0).powf(ROCP) - ZEROCNK;
                    tp_to_screen(t, p, &plot_rect)
                })
                .collect();
            clip.add(egui::Shape::line(pts, egui::Stroke::new(0.4, DRY_ADIABAT_COLOR)));
            theta_c += 10.0;
        }
    }

    // ── 4. Moist adiabats ─────────────────────────────────────
    {
        let mut thetaw = -15.0_f64;
        while thetaw <= 35.0 {
            let pts: Vec<egui::Pos2> = pressures.iter()
                .map(|&p| {
                    let t = thermo::satlift(p, thetaw);
                    tp_to_screen(t, p, &plot_rect)
                })
                .collect();
            clip.add(egui::Shape::line(pts, egui::Stroke::new(0.4, MOIST_ADIABAT_COLOR)));
            thetaw += 5.0;
        }
    }

    // ── 5. Mixing ratio lines ─────────────────────────────────
    {
        let mr_pressures: Vec<f64> = pressure_steps(60).into_iter()
            .filter(|&p| p >= 400.0)
            .collect();
        for &w in MIXING_RATIOS {
            let pts: Vec<egui::Pos2> = mr_pressures.iter()
                .map(|&p| {
                    let t = thermo::temp_at_mixrat(w, p);
                    tp_to_screen(t, p, &plot_rect)
                })
                .collect();
            draw_dashed(&clip, &pts, egui::Stroke::new(0.3, MIXING_LINE_COLOR), 4.0, 3.0);
        }
    }

    // ── 6. CAPE/CIN shading ───────────────────────────────────
    if let Some(ref parcel) = derived.parcel_trace {
        draw_cape_cin(&clip, &data, parcel, &plot_rect);
    }

    // ── 7. Temperature & dewpoint traces ──────────────────────
    if data.pressure.len() >= 2 {
        let t_pts: Vec<egui::Pos2> = data.pressure.iter().zip(data.temperature.iter())
            .map(|(&p, &t)| tp_to_screen(t, p, &plot_rect))
            .collect();
        clip.add(egui::Shape::line(t_pts, egui::Stroke::new(2.5, TEMP_COLOR)));

        let td_pts: Vec<egui::Pos2> = data.pressure.iter().zip(data.dewpoint.iter())
            .map(|(&p, &td)| tp_to_screen(td, p, &plot_rect))
            .collect();
        clip.add(egui::Shape::line(td_pts, egui::Stroke::new(2.5, DEWP_COLOR)));
    }

    // ── 8. Wet-bulb temperature trace ─────────────────────────
    if data.pressure.len() >= 2 {
        let wb_pts: Vec<egui::Pos2> = data.pressure.iter()
            .zip(data.temperature.iter().zip(data.dewpoint.iter()))
            .map(|(&p, (&t, &td))| {
                let wb = wet_bulb(p, t, td);
                tp_to_screen(wb, p, &plot_rect)
            })
            .collect();
        clip.add(egui::Shape::line(wb_pts, egui::Stroke::new(1.0, WETBULB_COLOR)));
    }

    // ── 9. Parcel trace ───────────────────────────────────────
    if let Some(ref parcel) = derived.parcel_trace {
        let pts: Vec<egui::Pos2> = parcel.iter()
            .map(|&(p, t)| tp_to_screen(t, p, &plot_rect))
            .collect();
        draw_dashed(&clip, &pts, egui::Stroke::new(1.8, PARCEL_COLOR), 8.0, 5.0);
    }

    // ── 10. Wind barbs (right side, subsampled) ───────────────
    {
        let barb_x = plot_rect.right() + 28.0;
        let mut last_y = -999.0_f32;
        for i in 0..data.wind_pressure.len() {
            let p = data.wind_pressure[i];
            if p < P_TOP || p > P_BOT { continue; }
            let y = tp_to_screen(0.0, p, &plot_rect).y;
            if (y - last_y).abs() < 14.0 { continue; } // subsample
            last_y = y;
            draw_wind_barb(&painter, egui::pos2(barb_x, y), data.wind_speed[i], data.wind_dir[i], 20.0);
        }
    }

    // ── 11. Plot border ───────────────────────────────────────
    painter.rect_stroke(plot_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 80)));

    // ── 12. LCL / LFC markers on left edge ────────────────────
    if derived.lcl_hpa > P_TOP && derived.lcl_hpa < P_BOT {
        let y = tp_to_screen(0.0, derived.lcl_hpa, &plot_rect).y;
        let marker_x = plot_rect.left() - 2.0;
        painter.text(
            egui::pos2(marker_x, y),
            egui::Align2::RIGHT_CENTER,
            "LCL",
            tiny_font.clone(),
            egui::Color32::from_rgb(0, 200, 0),
        );
    }

    // ── 13. Parameters panel (right side) ─────────────────────
    draw_params_panel(&painter, &derived, &data, full_rect, plot_rect);

    // ── 14. Hover crosshair ───────────────────────────────────
    if let Some(hp) = response.hover_pos() {
        if plot_rect.contains(hp) {
            let (t_h, p_h) = screen_to_tp(hp, &plot_rect);
            let cross = egui::Color32::from_white_alpha(50);
            painter.line_segment(
                [egui::pos2(hp.x, plot_rect.top()), egui::pos2(hp.x, plot_rect.bottom())],
                egui::Stroke::new(0.5, cross),
            );
            painter.line_segment(
                [egui::pos2(plot_rect.left(), hp.y), egui::pos2(plot_rect.right(), hp.y)],
                egui::Stroke::new(0.5, cross),
            );
            let tip = format!("{:.0} hPa  {:.1}°C", p_h, t_h);
            let gal = painter.layout_no_wrap(tip.clone(), tiny_font.clone(), egui::Color32::WHITE);
            let tp = hp + egui::vec2(12.0, -14.0);
            painter.rect_filled(
                egui::Rect::from_min_size(tp - egui::vec2(2.0, 1.0), gal.size() + egui::vec2(4.0, 2.0)),
                2.0,
                egui::Color32::from_black_alpha(220),
            );
            painter.text(tp, egui::Align2::LEFT_TOP, &tip, tiny_font.clone(), egui::Color32::WHITE);
        }
    }
}

// ── Wet-bulb temperature approximation ─────────────────────────────

fn wet_bulb(p: f64, t: f64, td: f64) -> f64 {
    // Iterative wet-bulb: find Tw where satlift(p, theta_w) = Tw
    // Quick approximation using the Stull formula
    let rh = 100.0 * thermo::vappres(td) / thermo::vappres(t);
    let rh = rh.clamp(0.0, 100.0);
    let tw = t * (0.151977 * (rh + 8.313659_f64).sqrt()).atan()
        + (t + rh).atan()
        - (rh - 1.676331).atan()
        + 0.00391838 * rh.powf(1.5) * (0.023101 * rh).atan()
        - 4.686035;
    tw
}

// ── Derived parameters ─────────────────────────────────────────────

struct DerivedParams {
    sbcape: f64,
    sbcin: f64,
    mlcape: f64,
    mlcin: f64,
    lcl_hpa: f64,
    lfc_h: f64,
    freezing_hpa: f64,
    pw_mm: f64,
    parcel_trace: Option<Vec<(f64, f64)>>,
    shear_01: Option<f64>,
    shear_06: Option<f64>,
    srh_01: f64,
    srh_03: f64,
}

fn compute_derived(data: &SoundingData) -> DerivedParams {
    let n = data.pressure.len();
    let defaults = DerivedParams {
        sbcape: 0.0, sbcin: 0.0, mlcape: 0.0, mlcin: 0.0,
        lcl_hpa: 0.0, lfc_h: f64::NAN, freezing_hpa: 0.0, pw_mm: 0.0,
        parcel_trace: None, shear_01: None, shear_06: None,
        srh_01: 0.0, srh_03: 0.0,
    };
    if n < 3 { return defaults; }

    let psfc = data.pressure[0];
    let t2m = data.temperature[0];
    let td2m = data.dewpoint[0];

    let height_agl = compute_height_agl(&data.pressure, &data.temperature);

    // CAPE/CIN
    let (sbcape, sbcin, _sb_lcl_h, sb_lfc_h) = thermo::cape_cin_core(
        &data.pressure, &data.temperature, &data.dewpoint, &height_agl,
        psfc, t2m, td2m, "sb", 100.0, 300.0, None,
    );
    let (mlcape, mlcin, _, _) = thermo::cape_cin_core(
        &data.pressure, &data.temperature, &data.dewpoint, &height_agl,
        psfc, t2m, td2m, "ml", 100.0, 300.0, None,
    );

    // LCL
    let (lcl_p, _lcl_t) = thermo::drylift(psfc, t2m, td2m);

    // Freezing level
    let mut freezing = 0.0;
    for i in 0..n - 1 {
        if data.temperature[i] > 0.0 && data.temperature[i + 1] <= 0.0 {
            let f = data.temperature[i] / (data.temperature[i] - data.temperature[i + 1]);
            freezing = data.pressure[i] + f * (data.pressure[i + 1] - data.pressure[i]);
            break;
        }
    }

    // PW
    let pw = compute_pw(&data.pressure, &data.temperature, &data.dewpoint, &height_agl);

    // Wind-derived params
    let (shear_01, shear_06, srh_01, srh_03) = compute_kinematic(data, &height_agl);

    let parcel_trace = Some(build_parcel_trace(psfc, t2m, td2m));

    DerivedParams {
        sbcape, sbcin, mlcape, mlcin,
        lcl_hpa: lcl_p, lfc_h: sb_lfc_h, freezing_hpa: freezing,
        pw_mm: pw, parcel_trace, shear_01, shear_06, srh_01, srh_03,
    }
}

fn compute_height_agl(pressure: &[f64], temperature: &[f64]) -> Vec<f64> {
    let n = pressure.len();
    let mut h = vec![0.0; n];
    for i in 1..n {
        let t_avg_k = ((temperature[i - 1] + temperature[i]) / 2.0) + ZEROCNK;
        let dp = (pressure[i - 1] / pressure[i]).ln();
        h[i] = h[i - 1] + (thermo::RD * t_avg_k / thermo::G) * dp;
    }
    h
}

fn compute_pw(p: &[f64], t: &[f64], td: &[f64], h: &[f64]) -> f64 {
    let mut pw = 0.0;
    for i in 0..p.len() - 1 {
        let w_avg = (thermo::mixratio(p[i], td[i]) + thermo::mixratio(p[i + 1], td[i + 1])) / 2.0 / 1000.0;
        let dz = h[i + 1] - h[i];
        let rho = (p[i] + p[i + 1]) / 2.0 * 100.0 / (thermo::RD * ((t[i] + t[i + 1]) / 2.0 + ZEROCNK));
        pw += w_avg * rho * dz;
    }
    pw
}

fn compute_kinematic(data: &SoundingData, height_agl: &[f64]) -> (Option<f64>, Option<f64>, f64, f64) {
    if data.wind_pressure.len() < 3 { return (None, None, 0.0, 0.0); }

    // Build wind height profile
    let p_sfc = data.pressure[0];
    let w_heights: Vec<f64> = data.wind_pressure.iter()
        .map(|&p| 7400.0 * (p_sfc / p).ln())
        .collect();

    let interp = |h: f64, vals: &[f64]| -> Option<f64> {
        for i in 0..w_heights.len() - 1 {
            if w_heights[i] <= h && w_heights[i + 1] >= h {
                let f = (h - w_heights[i]) / (w_heights[i + 1] - w_heights[i]);
                return Some(vals[i] + f * (vals[i + 1] - vals[i]));
            }
        }
        None
    };

    let shear = |h_top: f64| -> Option<f64> {
        let u0 = data.wind_speed.first().copied()?;
        let d0 = data.wind_dir.first().copied()?;
        let u_top = interp(h_top, &data.wind_speed)?;
        let d_top = interp(h_top, &data.wind_dir)?;
        // Convert to u/v then compute shear
        let (u0x, u0y) = spd_dir_to_uv(u0, d0);
        let (u1x, u1y) = spd_dir_to_uv(u_top, d_top);
        Some(((u1x - u0x).powi(2) + (u1y - u0y).powi(2)).sqrt())
    };

    let shear_01 = shear(1000.0);
    let shear_06 = shear(6000.0);

    // SRH (simplified)
    let srh_01 = 0.0; // TODO: proper SRH with Bunkers
    let srh_03 = 0.0;

    (shear_01, shear_06, srh_01, srh_03)
}

fn spd_dir_to_uv(spd: f64, dir: f64) -> (f64, f64) {
    let rad = dir.to_radians();
    (-spd * rad.sin(), -spd * rad.cos())
}

fn build_parcel_trace(psfc: f64, t_sfc: f64, td_sfc: f64) -> Vec<(f64, f64)> {
    let (p_lcl, t_lcl) = thermo::drylift(psfc, t_sfc, td_sfc);

    // Theta-M for moist ascent
    let theta_k = (t_lcl + ZEROCNK) * (1000.0 / p_lcl).powf(ROCP);
    let theta_c = theta_k - ZEROCNK;
    let thetam = theta_c - thermo::wobf(theta_c) + thermo::wobf(t_lcl);

    let mut trace = Vec::with_capacity(200);

    // Dry adiabat: surface to LCL
    let theta_sfc_k = (t_sfc + ZEROCNK) * (1000.0 / psfc).powf(ROCP);
    let n_dry = 40;
    for i in 0..=n_dry {
        let f = i as f64 / n_dry as f64;
        let p = psfc + f * (p_lcl - psfc);
        let t = theta_sfc_k * (p / 1000.0).powf(ROCP) - ZEROCNK;
        trace.push((p, t));
    }

    // Moist adiabat: LCL to top
    let n_moist = 120;
    for i in 1..=n_moist {
        let f = i as f64 / n_moist as f64;
        let p = (p_lcl.ln() + f * (P_TOP.ln() - p_lcl.ln())).exp();
        let t = thermo::satlift(p, thetam);
        trace.push((p, t));
    }

    trace
}

// ── CAPE/CIN shading ───────────────────────────────────────────────

fn draw_cape_cin(
    painter: &egui::Painter,
    data: &SoundingData,
    parcel: &[(f64, f64)],
    rect: &egui::Rect,
) {
    let n_steps = 300;
    let p_top_data = data.pressure.last().copied().unwrap_or(P_TOP).max(P_TOP);
    let p_bot_data = data.pressure[0].min(P_BOT);

    for i in 0..n_steps {
        let f0 = i as f64 / n_steps as f64;
        let f1 = (i + 1) as f64 / n_steps as f64;
        let p0 = (p_bot_data.ln() * (1.0 - f0) + p_top_data.ln() * f0).exp();
        let p1 = (p_bot_data.ln() * (1.0 - f1) + p_top_data.ln() * f1).exp();

        let (t_env0, _) = thermo::get_env_at_pres(p0, &data.pressure, &data.temperature, &data.dewpoint);
        let (t_env1, _) = thermo::get_env_at_pres(p1, &data.pressure, &data.temperature, &data.dewpoint);
        let Some(t_par0) = interp_parcel(p0, parcel) else { continue; };
        let Some(t_par1) = interp_parcel(p1, parcel) else { continue; };

        let color = if t_par0 > t_env0 { CAPE_FILL } else { CIN_FILL };

        let e0 = tp_to_screen(t_env0, p0, rect);
        let e1 = tp_to_screen(t_env1, p1, rect);
        let p0s = tp_to_screen(t_par0, p0, rect);
        let p1s = tp_to_screen(t_par1, p1, rect);

        let sep = ((p0s.x - e0.x).abs() + (p1s.x - e1.x).abs()) / 2.0;
        if sep < 0.5 { continue; }

        painter.add(egui::Shape::convex_polygon(
            vec![e0, p0s, p1s, e1],
            color,
            egui::Stroke::NONE,
        ));
    }
}

fn interp_parcel(p: f64, trace: &[(f64, f64)]) -> Option<f64> {
    if trace.is_empty() { return None; }
    if p > trace[0].0 || p < trace.last()?.0 { return None; }
    for i in 0..trace.len() - 1 {
        let (p0, t0) = trace[i];
        let (p1, t1) = trace[i + 1];
        if p0 >= p && p >= p1 {
            let f = (p0.ln() - p.ln()) / (p0.ln() - p1.ln());
            return Some(t0 + f * (t1 - t0));
        }
    }
    None
}

// ── Wind barbs ─────────────────────────────────────────────────────

fn draw_wind_barb(painter: &egui::Painter, center: egui::Pos2, speed_kt: f64, dir_deg: f64, length: f32) {
    if speed_kt < 2.0 {
        painter.circle_stroke(center, 3.0, egui::Stroke::new(1.0, BARB_COLOR));
        return;
    }

    let dir_rad = dir_deg.to_radians() as f32;
    let dx = -dir_rad.sin();
    let dy = dir_rad.cos();
    let tip = egui::pos2(center.x + dx * length, center.y + dy * length);

    painter.line_segment([center, tip], egui::Stroke::new(1.2, BARB_COLOR));

    let mut rem = speed_kt;
    let sp = length / 5.5;
    let blen = length * 0.42;
    let mut pos = 0.0_f32;

    // Perpendicular (right side when looking into wind)
    let px = dy;
    let py = -dx;

    // Pennants (50 kt)
    while rem >= 47.5 {
        let b = egui::pos2(tip.x - dx * pos, tip.y - dy * pos);
        let e = egui::pos2(b.x + px * blen, b.y + py * blen);
        let n = egui::pos2(tip.x - dx * (pos + sp), tip.y - dy * (pos + sp));
        painter.add(egui::Shape::convex_polygon(vec![b, e, n], BARB_COLOR, egui::Stroke::NONE));
        rem -= 50.0;
        pos += sp;
    }
    // Long barbs (10 kt)
    while rem >= 7.5 {
        let b = egui::pos2(tip.x - dx * pos, tip.y - dy * pos);
        let e = egui::pos2(b.x + px * blen, b.y + py * blen);
        painter.line_segment([b, e], egui::Stroke::new(1.2, BARB_COLOR));
        rem -= 10.0;
        pos += sp;
    }
    // Short barb (5 kt)
    if rem >= 2.5 {
        let b = egui::pos2(tip.x - dx * pos, tip.y - dy * pos);
        let e = egui::pos2(b.x + px * blen * 0.5, b.y + py * blen * 0.5);
        painter.line_segment([b, e], egui::Stroke::new(1.0, BARB_COLOR));
    }
}

// ── Dashed line ────────────────────────────────────────────────────

fn draw_dashed(painter: &egui::Painter, points: &[egui::Pos2], stroke: egui::Stroke, dash: f32, gap: f32) {
    if points.len() < 2 { return; }
    let cycle = dash + gap;
    let mut acc = 0.0_f32;
    for i in 0..points.len() - 1 {
        let a = points[i];
        let b = points[i + 1];
        let seg = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        if seg < 0.1 { continue; }
        let ux = (b.x - a.x) / seg;
        let uy = (b.y - a.y) / seg;
        let mut d = 0.0_f32;
        while d < seg {
            let phase = acc % cycle;
            if phase < dash {
                let draw = (dash - phase).min(seg - d);
                let p0 = egui::pos2(a.x + ux * d, a.y + uy * d);
                let p1 = egui::pos2(a.x + ux * (d + draw), a.y + uy * (d + draw));
                painter.line_segment([p0, p1], stroke);
                d += draw;
                acc += draw;
            } else {
                let skip = (cycle - phase).min(seg - d);
                d += skip;
                acc += skip;
            }
        }
    }
}

// ── Parameters panel ───────────────────────────────────────────────

fn draw_params_panel(
    painter: &egui::Painter,
    d: &DerivedParams,
    data: &SoundingData,
    full_rect: egui::Rect,
    plot_rect: egui::Rect,
) {
    let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
    let hdr_font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let x = plot_rect.right() + 70.0 + 12.0;
    let mut y = full_rect.top() + 12.0;
    let lh = 15.0_f32;
    let w = 170.0_f32;

    // Background
    let panel_rect = egui::Rect::from_min_max(
        egui::pos2(x - 6.0, y - 4.0),
        egui::pos2(x + w, full_rect.bottom() - 8.0),
    );
    painter.rect_filled(panel_rect, 4.0, egui::Color32::from_rgba_premultiplied(0, 0, 0, 180));

    let txt = |p: &egui::Painter, y: &mut f32, text: &str, color: egui::Color32| {
        p.text(egui::pos2(x, *y), egui::Align2::LEFT_TOP, text, font.clone(), color);
        *y += lh;
    };
    let hdr = |p: &egui::Painter, y: &mut f32, text: &str| {
        *y += 4.0;
        p.text(egui::pos2(x, *y), egui::Align2::LEFT_TOP, text, hdr_font.clone(),
            egui::Color32::from_rgb(180, 180, 200));
        *y += lh + 2.0;
    };

    // Thermodynamic
    hdr(painter, &mut y, "── THERMODYNAMIC ──");
    txt(painter, &mut y, &format!("SBCAPE {:>6.0} J/kg", d.sbcape),
        cape_color(d.sbcape));
    txt(painter, &mut y, &format!("SBCIN  {:>6.0} J/kg", d.sbcin),
        cin_color(d.sbcin));
    txt(painter, &mut y, &format!("MLCAPE {:>6.0} J/kg", d.mlcape),
        cape_color(d.mlcape));
    txt(painter, &mut y, &format!("MLCIN  {:>6.0} J/kg", d.mlcin),
        cin_color(d.mlcin));
    txt(painter, &mut y, &format!("LCL    {:>6.0} hPa", d.lcl_hpa),
        LABEL_COLOR);
    if !d.lfc_h.is_nan() {
        txt(painter, &mut y, &format!("LFC    {:>6.0} m AGL", d.lfc_h), LABEL_COLOR);
    }
    if d.freezing_hpa > 0.0 {
        txt(painter, &mut y, &format!("Frz Lvl {:>5.0} hPa", d.freezing_hpa), ZERO_C_COLOR);
    }
    txt(painter, &mut y, &format!("PW     {:>6.1} mm", d.pw_mm), LABEL_COLOR);

    // Kinematic
    hdr(painter, &mut y, "── KINEMATIC ──");
    if let Some(s) = d.shear_01 {
        txt(painter, &mut y, &format!("0-1km Shear {:>3.0} kt", s), LABEL_COLOR);
    }
    if let Some(s) = d.shear_06 {
        txt(painter, &mut y, &format!("0-6km Shear {:>3.0} kt", s), LABEL_COLOR);
    }

    // Surface data
    if !data.pressure.is_empty() {
        hdr(painter, &mut y, "── SURFACE ──");
        txt(painter, &mut y, &format!("P_sfc  {:>6.0} hPa", data.pressure[0]), LABEL_COLOR);
        txt(painter, &mut y, &format!("T_sfc  {:>6.1}°C", data.temperature[0]), TEMP_COLOR);
        txt(painter, &mut y, &format!("Td_sfc {:>6.1}°C", data.dewpoint[0]), DEWP_COLOR);
        if !data.wind_speed.is_empty() {
            txt(painter, &mut y, &format!("Wind {:>3.0}°/{:.0}kt",
                data.wind_dir[0], data.wind_speed[0]), BARB_COLOR);
        }
    }
}

fn cape_color(cape: f64) -> egui::Color32 {
    if cape >= 3000.0 { egui::Color32::from_rgb(255, 0, 0) }
    else if cape >= 2000.0 { egui::Color32::from_rgb(255, 100, 0) }
    else if cape >= 1000.0 { egui::Color32::from_rgb(255, 200, 0) }
    else if cape >= 500.0 { egui::Color32::from_rgb(200, 200, 100) }
    else { LABEL_COLOR }
}

fn cin_color(cin: f64) -> egui::Color32 {
    if cin < -200.0 { egui::Color32::from_rgb(100, 100, 255) }
    else if cin < -50.0 { egui::Color32::from_rgb(80, 80, 200) }
    else { LABEL_COLOR }
}

// ── Placeholder view ───────────────────────────────────────────────

fn show_placeholder(ui: &mut egui::Ui, state: &mut AppState, title: &str, needs: &str) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 4.0);
            ui.label(egui::RichText::new(title).heading().color(theme::ACCENT));
            ui.add_space(16.0);
            ui.label(egui::RichText::new(format!("Need {needs}. Download HRRR Full F000.")).color(theme::TEXT_DIM));
            ui.add_space(16.0);
            if ui.add(egui::Button::new("Go to Download").fill(theme::ACCENT)).clicked() {
                state.active_view = crate::state::View::Download;
            }
        });
    });
}

// ── Data extraction (from GRIB2) ───────────────────────────────────

fn is_pressure_level(level_type: u8) -> bool {
    level_type == 100 || level_type == 105
}

fn level_to_pressure_hpa(level_type: u8, level_value: f64) -> f64 {
    match level_type {
        100 => if level_value > 2000.0 { level_value / 100.0 } else { level_value },
        105 => {
            let l = level_value;
            if l <= 0.0 { return 1013.0; }
            if l >= 50.0 { return 50.0; }
            1013.0 * (-l * 0.06).exp()
        }
        _ => level_value,
    }
}

fn extract_sounding(
    grib: &rustmet_core::grib2::Grib2File,
    gi: usize,
    gj: usize,
) -> Option<SoundingData> {
    use rustmet_core::grib2;

    let mut temp_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut dew_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut rh_map: BTreeMap<i64, f64> = BTreeMap::new();
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
            (0, 0) => { if let Some(v) = get_val(msg) { temp_map.insert(p_key, v - 273.15); } }
            (0, 6) => { if let Some(v) = get_val(msg) { dew_map.insert(p_key, v - 273.15); } }
            (0, 1) => { if let Some(v) = get_val(msg) { rh_map.insert(p_key, v); } }
            (2, 2) => { if let Some(v) = get_val(msg) { u_map.insert(p_key, v); } }
            (2, 3) => { if let Some(v) = get_val(msg) { v_map.insert(p_key, v); } }
            _ => {}
        }
    }

    if temp_map.len() < 3 { return None; }

    let mut pressure = Vec::new();
    let mut temperature = Vec::new();
    let mut dewpoint = Vec::new();
    let mut wind_speed = Vec::new();
    let mut wind_dir = Vec::new();
    let mut wind_pressure = Vec::new();

    let keys: Vec<i64> = temp_map.keys().rev().cloned().collect();
    for &k in &keys {
        let p = k as f64 / 100.0;
        let t = *temp_map.get(&k)?;
        let td = if let Some(&td) = dew_map.get(&k) {
            td
        } else if let Some(&rh) = rh_map.get(&k) {
            let rh_frac = if rh > 2.0 { rh / 100.0 } else { rh };
            let a = 17.625;
            let b = 243.04;
            let gamma = rh_frac.max(0.01).ln() + (a * t) / (b + t);
            (b * gamma) / (a - gamma)
        } else {
            t - 15.0
        };

        pressure.push(p);
        temperature.push(t);
        dewpoint.push(td);

        if let (Some(&u), Some(&v)) = (u_map.get(&k), v_map.get(&k)) {
            let spd = (u * u + v * v).sqrt() * 1.94384;
            let dir = (270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0);
            wind_speed.push(spd);
            wind_dir.push(dir);
            wind_pressure.push(p);
        }
    }

    Some(SoundingData { pressure, temperature, dewpoint, wind_speed, wind_dir, wind_pressure })
}
