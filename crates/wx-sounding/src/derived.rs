/// Derived thermodynamic and kinematic calculations for soundings.
///
/// Uses `wx_math::thermo` for core thermodynamic computations.

use crate::types::Sounding;
use wx_math::thermo;

/// Compute all derived indices for a sounding in-place.
///
/// Requires the sounding to have valid levels with pressure, height,
/// temperature, dewpoint, and wind data.
pub fn compute_indices(sounding: &mut Sounding) {
    if sounding.levels.len() < 3 {
        return;
    }

    // Build profile arrays (surface first, decreasing pressure)
    let n = sounding.levels.len();
    let mut p_prof = Vec::with_capacity(n);
    let mut t_prof = Vec::with_capacity(n);
    let mut td_prof = Vec::with_capacity(n);
    let mut h_prof = Vec::with_capacity(n);      // MSL
    let mut h_agl_prof = Vec::with_capacity(n);  // AGL
    let mut wdir_prof = Vec::with_capacity(n);
    let mut wspd_prof = Vec::with_capacity(n);

    let sfc_elev = sounding.elevation_m;

    for lev in &sounding.levels {
        p_prof.push(lev.pressure);
        t_prof.push(lev.temperature);
        td_prof.push(lev.dewpoint);
        h_prof.push(lev.height);
        h_agl_prof.push(lev.height - sfc_elev);
        wdir_prof.push(lev.wind_dir);
        wspd_prof.push(lev.wind_speed);
    }

    // Surface values
    let psfc = p_prof[0];
    let t2m = t_prof[0];
    let td2m = td_prof[0];

    // --- CAPE/CIN computations ---
    // Surface-based
    let (sbcape, sbcin, lcl_h, lfc_h) = thermo::cape_cin_core(
        &p_prof[1..],
        &t_prof[1..],
        &td_prof[1..],
        &h_agl_prof[1..],
        psfc,
        t2m,
        td2m,
        "sb",
        100.0,
        300.0,
        None,
    );
    sounding.indices.sbcape = sbcape;
    sounding.indices.sbcin = sbcin;
    sounding.indices.lcl_m = lcl_h;
    sounding.indices.lfc_m = lfc_h;

    // Mixed-layer
    let (mlcape, mlcin, ml_lcl, _ml_lfc) = thermo::cape_cin_core(
        &p_prof[1..],
        &t_prof[1..],
        &td_prof[1..],
        &h_agl_prof[1..],
        psfc,
        t2m,
        td2m,
        "ml",
        100.0,
        300.0,
        None,
    );
    sounding.indices.mlcape = mlcape;
    sounding.indices.mlcin = mlcin;

    // Most-unstable
    let (mucape, mucin, _mu_lcl, _mu_lfc) = thermo::cape_cin_core(
        &p_prof[1..],
        &t_prof[1..],
        &td_prof[1..],
        &h_agl_prof[1..],
        psfc,
        t2m,
        td2m,
        "mu",
        100.0,
        300.0,
        None,
    );
    sounding.indices.mucape = mucape;
    sounding.indices.mucin = mucin;

    // --- EL height ---
    // Approximate: find highest level where parcel is still warmer than environment
    // For now, use a simple approach based on CAPE integration bounds
    sounding.indices.el_m = estimate_el(&p_prof, &t_prof, &td_prof, &h_agl_prof, psfc, t2m, td2m);

    // --- Lifted Index ---
    sounding.indices.li = compute_lifted_index(&p_prof, &t_prof, &td_prof);

    // --- Total Totals ---
    let t850 = interp_temp_at_pres(850.0, &p_prof, &t_prof);
    let t700 = interp_temp_at_pres(700.0, &p_prof, &t_prof);
    let t500 = interp_temp_at_pres(500.0, &p_prof, &t_prof);
    let td850 = interp_temp_at_pres(850.0, &p_prof, &td_prof);
    let td700 = interp_temp_at_pres(700.0, &p_prof, &td_prof);

    if !t850.is_nan() && !t500.is_nan() && !td850.is_nan() {
        sounding.indices.total_totals = (t850 - t500) + (td850 - t500);
    }

    // --- K-Index ---
    if !t850.is_nan() && !t700.is_nan() && !t500.is_nan() && !td850.is_nan() && !td700.is_nan() {
        sounding.indices.k_index = (t850 - t500) + td850 - (t700 - td700);
    }

    // --- SWEAT Index ---
    sounding.indices.sweat = compute_sweat(&p_prof, &t_prof, &td_prof, &wdir_prof, &wspd_prof);

    // --- Bulk Shear ---
    // Convert wind dir/speed (degrees, knots) to u/v (m/s)
    let (u_prof, v_prof) = wind_components(&wdir_prof, &wspd_prof);

    sounding.indices.bulk_shear_01 = compute_bulk_shear(&h_agl_prof, &u_prof, &v_prof, 0.0, 1000.0);
    sounding.indices.bulk_shear_06 = compute_bulk_shear(&h_agl_prof, &u_prof, &v_prof, 0.0, 6000.0);

    // --- SRH ---
    sounding.indices.srh_01 = compute_srh_column(&h_agl_prof, &u_prof, &v_prof, 1000.0);
    sounding.indices.srh_03 = compute_srh_column(&h_agl_prof, &u_prof, &v_prof, 3000.0);

    // --- Precipitable Water ---
    sounding.indices.pw_mm = compute_pw(&p_prof, &td_prof);

    // --- STP (Significant Tornado Parameter) ---
    // STP = (MLCAPE/1500) * (SRH_01/150) * (Shear_06/20) * ((2000 - MLLCL)/1000)
    let shear_06_ms = sounding.indices.bulk_shear_06 * 0.51444; // knots to m/s
    let lcl_term = ((2000.0 - ml_lcl) / 1000.0).clamp(0.0, 1.0);
    let shear_term = (shear_06_ms / 20.0).min(1.5);
    sounding.indices.stp = (mlcape / 1500.0)
        * (sounding.indices.srh_01 / 150.0)
        * shear_term
        * lcl_term;

    // Clamp STP to non-negative
    if sounding.indices.stp < 0.0 {
        sounding.indices.stp = 0.0;
    }
}

/// Interpolate temperature (or dewpoint) at a target pressure level.
fn interp_temp_at_pres(target_p: f64, p_prof: &[f64], t_prof: &[f64]) -> f64 {
    if p_prof.len() < 2 {
        return f64::NAN;
    }
    // Profile is surface-first (decreasing pressure)
    for i in 0..p_prof.len() - 1 {
        if p_prof[i] >= target_p && target_p >= p_prof[i + 1] {
            let log_p = target_p.ln();
            let log_p1 = p_prof[i].ln();
            let log_p2 = p_prof[i + 1].ln();
            return thermo::interp_linear(log_p, log_p1, log_p2, t_prof[i], t_prof[i + 1]);
        }
    }
    f64::NAN
}

/// Compute Lifted Index: T_env(500) - T_parcel(500).
fn compute_lifted_index(p_prof: &[f64], t_prof: &[f64], td_prof: &[f64]) -> f64 {
    if p_prof.is_empty() {
        return f64::NAN;
    }
    let p_sfc = p_prof[0];
    let t_sfc = t_prof[0];
    let td_sfc = td_prof[0];

    // Lift parcel to 500 hPa
    let (p_lcl, t_lcl) = thermo::drylift(p_sfc, t_sfc, td_sfc);

    // Theta-M for moist adiabat
    let theta_c = (t_lcl + thermo::ZEROCNK) * ((1000.0 / p_lcl).powf(thermo::ROCP)) - thermo::ZEROCNK;
    let thetam = theta_c - thermo::wobf(theta_c) + thermo::wobf(t_lcl);

    let t_parcel_500 = thermo::satlift(500.0, thetam);
    let t_env_500 = interp_temp_at_pres(500.0, p_prof, t_prof);

    if t_env_500.is_nan() {
        return f64::NAN;
    }

    t_env_500 - t_parcel_500
}

/// Estimate EL height (meters AGL) from a surface-based parcel ascent.
fn estimate_el(
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
    h_agl_prof: &[f64],
    psfc: f64,
    t2m: f64,
    td2m: f64,
) -> f64 {
    let (p_lcl, t_lcl) = thermo::drylift(psfc, t2m, td2m);
    let theta_c = (t_lcl + thermo::ZEROCNK) * ((1000.0 / p_lcl).powf(thermo::ROCP)) - thermo::ZEROCNK;
    let thetam = theta_c - thermo::wobf(theta_c) + thermo::wobf(t_lcl);

    let mut last_positive_h = 0.0_f64;
    let mut found_positive = false;

    for i in 0..p_prof.len() {
        if p_prof[i] > p_lcl {
            continue;
        }
        let t_parc = thermo::satlift(p_prof[i], thetam);
        let tv_parc = thermo::virtual_temp(t_parc, p_prof[i], t_parc);
        let tv_env = thermo::virtual_temp(t_prof[i], p_prof[i], td_prof[i]);

        if tv_parc > tv_env {
            found_positive = true;
            last_positive_h = h_agl_prof[i];
        } else if found_positive {
            // We just crossed the EL — interpolate
            if i > 0 {
                let t_parc_prev = thermo::satlift(p_prof[i - 1], thetam);
                let tv_parc_prev = thermo::virtual_temp(t_parc_prev, p_prof[i - 1], t_parc_prev);
                let tv_env_prev = thermo::virtual_temp(t_prof[i - 1], p_prof[i - 1], td_prof[i - 1]);
                let buoy_prev = tv_parc_prev - tv_env_prev;
                let buoy_curr = tv_parc - tv_env;
                if (buoy_curr - buoy_prev).abs() > 0.001 {
                    let frac = (0.0 - buoy_prev) / (buoy_curr - buoy_prev);
                    return h_agl_prof[i - 1] + frac * (h_agl_prof[i] - h_agl_prof[i - 1]);
                }
            }
            return h_agl_prof[i];
        }
    }

    last_positive_h
}

/// Convert wind direction (degrees) and speed (knots) to u/v components (m/s).
fn wind_components(wdir: &[f64], wspd: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let kt_to_ms = 0.51444;
    let mut u = Vec::with_capacity(wdir.len());
    let mut v = Vec::with_capacity(wdir.len());
    for i in 0..wdir.len() {
        let spd_ms = wspd[i] * kt_to_ms;
        let dir_rad = wdir[i].to_radians();
        u.push(-spd_ms * dir_rad.sin());
        v.push(-spd_ms * dir_rad.cos());
    }
    (u, v)
}

/// Interpolate a value at a target height AGL from a height profile.
fn interp_at_height(target_h: f64, heights: &[f64], values: &[f64]) -> f64 {
    if heights.is_empty() {
        return f64::NAN;
    }
    if target_h <= heights[0] {
        return values[0];
    }
    if target_h >= heights[heights.len() - 1] {
        return values[values.len() - 1];
    }
    for k in 0..heights.len() - 1 {
        if heights[k] <= target_h && heights[k + 1] >= target_h {
            let frac = (target_h - heights[k]) / (heights[k + 1] - heights[k]);
            return values[k] + frac * (values[k + 1] - values[k]);
        }
    }
    f64::NAN
}

/// Compute bulk wind shear magnitude (knots) between two AGL heights.
fn compute_bulk_shear(
    h_agl: &[f64],
    u_prof: &[f64],
    v_prof: &[f64],
    bottom_m: f64,
    top_m: f64,
) -> f64 {
    let u_bot = interp_at_height(bottom_m, h_agl, u_prof);
    let v_bot = interp_at_height(bottom_m, h_agl, v_prof);
    let u_top = interp_at_height(top_m, h_agl, u_prof);
    let v_top = interp_at_height(top_m, h_agl, v_prof);

    let du = u_top - u_bot;
    let dv = v_top - v_bot;
    let shear_ms = (du * du + dv * dv).sqrt();
    // Convert m/s back to knots for output
    shear_ms / 0.51444
}

/// Compute SRH for a single column using Bunkers storm motion.
/// Heights are AGL (meters), u/v in m/s.
/// Returns SRH in m^2/s^2.
fn compute_srh_column(
    heights: &[f64],
    u_prof: &[f64],
    v_prof: &[f64],
    top_m: f64,
) -> f64 {
    let nz = heights.len();
    if nz < 2 {
        return 0.0;
    }

    // Mean wind in 0-6km layer
    let mean_depth = 6000.0;
    let mut sum_u = 0.0;
    let mut sum_v = 0.0;
    let mut sum_dz = 0.0;

    for k in 0..nz - 1 {
        if heights[k] >= mean_depth {
            break;
        }
        let h_bot = heights[k];
        let h_top = heights[k + 1].min(mean_depth);
        let dz = h_top - h_bot;
        if dz <= 0.0 {
            continue;
        }
        let u_mid = 0.5 * (u_prof[k] + u_prof[k + 1]);
        let v_mid = 0.5 * (v_prof[k] + v_prof[k + 1]);
        sum_u += u_mid * dz;
        sum_v += v_mid * dz;
        sum_dz += dz;
    }

    if sum_dz <= 0.0 {
        return 0.0;
    }

    let mean_u = sum_u / sum_dz;
    let mean_v = sum_v / sum_dz;

    // 0-6km shear vector
    let u_sfc = u_prof[0];
    let v_sfc = v_prof[0];
    let u_6km = interp_at_height(mean_depth, heights, u_prof);
    let v_6km = interp_at_height(mean_depth, heights, v_prof);
    let shear_u = u_6km - u_sfc;
    let shear_v = v_6km - v_sfc;

    // Bunkers deviation: rotate shear 90deg clockwise, scale to 7.5 m/s
    let shear_mag = (shear_u * shear_u + shear_v * shear_v).sqrt();
    let (dev_u, dev_v) = if shear_mag > 0.1 {
        let scale = 7.5 / shear_mag;
        (shear_v * scale, -shear_u * scale)
    } else {
        (0.0, 0.0)
    };

    let storm_u = mean_u + dev_u;
    let storm_v = mean_v + dev_v;

    // Integrate SRH
    let mut srh = 0.0;
    for k in 0..nz - 1 {
        if heights[k] >= top_m {
            break;
        }
        let h_bot = heights[k];
        let h_top = heights[k + 1].min(top_m);
        if h_top <= h_bot {
            continue;
        }

        let u_bot = u_prof[k];
        let v_bot = v_prof[k];
        let (u_top_val, v_top_val) = if h_top < heights[k + 1] {
            let frac = (h_top - heights[k]) / (heights[k + 1] - heights[k]);
            (
                u_prof[k] + frac * (u_prof[k + 1] - u_prof[k]),
                v_prof[k] + frac * (v_prof[k + 1] - v_prof[k]),
            )
        } else {
            (u_prof[k + 1], v_prof[k + 1])
        };

        let sr_u_bot = u_bot - storm_u;
        let sr_v_bot = v_bot - storm_v;
        let sr_u_top = u_top_val - storm_u;
        let sr_v_top = v_top_val - storm_v;

        let du = u_top_val - u_bot;
        let dv = v_top_val - v_bot;
        let avg_sr_u = 0.5 * (sr_u_bot + sr_u_top);
        let avg_sr_v = 0.5 * (sr_v_bot + sr_v_top);

        srh += avg_sr_u * dv - avg_sr_v * du;
    }

    srh
}

/// Compute precipitable water (mm) by integrating mixing ratio through the column.
fn compute_pw(p_prof: &[f64], td_prof: &[f64]) -> f64 {
    let mut pw = 0.0;
    for i in 0..p_prof.len() - 1 {
        let w1 = thermo::mixratio(p_prof[i], td_prof[i]).max(0.0) / 1000.0; // g/kg -> kg/kg
        let w2 = thermo::mixratio(p_prof[i + 1], td_prof[i + 1]).max(0.0) / 1000.0;
        let w_avg = 0.5 * (w1 + w2);
        let dp = (p_prof[i] - p_prof[i + 1]).abs() * 100.0; // hPa -> Pa
        pw += w_avg * dp;
    }
    // PW in kg/m^2 = mm
    pw / thermo::G
}

/// Compute SWEAT index.
fn compute_sweat(
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
    wdir_prof: &[f64],
    wspd_prof: &[f64],
) -> f64 {
    let td850 = interp_temp_at_pres(850.0, p_prof, td_prof);
    let t500 = interp_temp_at_pres(500.0, p_prof, t_prof);
    let t850 = interp_temp_at_pres(850.0, p_prof, t_prof);

    if td850.is_nan() || t500.is_nan() || t850.is_nan() {
        return 0.0;
    }

    // Interpolate winds at 850 and 500 hPa
    let h_prof: Vec<f64> = p_prof.iter().map(|p| thermo::pressure_to_height_std(*p)).collect();
    let h850 = thermo::pressure_to_height_std(850.0);
    let h500 = thermo::pressure_to_height_std(500.0);
    let dir850 = interp_at_height(h850, &h_prof, wdir_prof);
    let dir500 = interp_at_height(h500, &h_prof, wdir_prof);
    let spd850 = interp_at_height(h850, &h_prof, wspd_prof);
    let spd500 = interp_at_height(h500, &h_prof, wspd_prof);

    let tt = (t850 - t500) + (td850 - t500);
    let td_term = if td850 > 0.0 { 12.0 * td850 } else { 0.0 };
    let tt_term = if tt > 49.0 { 20.0 * (tt - 49.0) } else { 0.0 };

    // Shear term
    let s = (dir500 - dir850).to_radians().sin();
    let shear_term = if dir850 >= 130.0
        && dir850 <= 250.0
        && dir500 >= 210.0
        && dir500 <= 310.0
        && dir500 > dir850
        && spd850 >= 15.0
        && spd500 >= 15.0
    {
        125.0 * (s + 0.2)
    } else {
        0.0
    };

    td_term + tt_term + 2.0 * spd850 + spd500 + shear_term
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wind_components() {
        // 270 degrees at 10 knots = pure westerly
        let (u, v) = wind_components(&[270.0], &[10.0]);
        let spd_ms = 10.0 * 0.51444;
        assert!((u[0] - spd_ms).abs() < 0.01); // u should be positive (from west)
        assert!(v[0].abs() < 0.01);             // v should be ~0
    }

    #[test]
    fn test_interp_at_height() {
        let h = vec![0.0, 1000.0, 2000.0, 3000.0];
        let v = vec![10.0, 20.0, 30.0, 40.0];
        assert!((interp_at_height(500.0, &h, &v) - 15.0).abs() < 0.01);
        assert!((interp_at_height(0.0, &h, &v) - 10.0).abs() < 0.01);
        assert!((interp_at_height(3000.0, &h, &v) - 40.0).abs() < 0.01);
    }
}
