/// Meteorological thermodynamic functions ported from wrfsolar's metfuncs.py.
/// Pure math - no external dependencies. All functions are direct ports of the
/// SHARPpy-derived implementations used in the Python codebase.

// --- Physical Constants ---
pub const RD: f64 = 287.058;       // Dry air gas constant (J/(kg*K))
pub const CP: f64 = 1005.7;        // Specific heat at constant pressure (J/(kg*K))
pub const G: f64 = 9.80665;        // Gravitational acceleration (m/s^2)
pub const ROCP: f64 = 0.28571426;  // Rd/Cp
pub const ZEROCNK: f64 = 273.15;   // 0 Celsius in Kelvin
pub const MISSING: f64 = -9999.0;

// --- SHARPpy Thermodynamic Approximations ---

/// Wobus function for computing moist adiabats.
/// Input: temperature in Celsius.
pub fn wobf(t: f64) -> f64 {
    let t = t - 20.0;
    if t <= 0.0 {
        let npol = 1.0
            + t * (-8.841660499999999e-3
                + t * (1.4714143e-4
                    + t * (-9.671989000000001e-7
                        + t * (-3.2607217e-8 + t * (-3.8598073e-10)))));
        15.13 / (npol * npol * npol * npol)
    } else {
        let ppol = t
            * (4.9618922e-07
                + t * (-6.1059365e-09
                    + t * (3.9401551e-11
                        + t * (-1.2588129e-13 + t * (1.6688280e-16)))));
        let ppol = 1.0 + t * (3.6182989e-03 + t * (-1.3603273e-05 + ppol));
        (29.93 / (ppol * ppol * ppol * ppol)) + (0.96 * t) - 14.8
    }
}

/// Lifts a saturated parcel.
/// p: Pressure (hPa), thetam: Saturation Potential Temperature (Celsius).
/// Uses 7 Newton-Raphson iterations.
pub fn satlift(p: f64, thetam: f64) -> f64 {
    if p >= 1000.0 {
        return thetam;
    }

    let pwrp = (p / 1000.0_f64).powf(ROCP);
    let mut t1 = (thetam + ZEROCNK) * pwrp - ZEROCNK;
    let mut e1 = wobf(t1) - wobf(thetam);
    let mut rate = 1.0;

    for _ in 0..7 {
        if e1.abs() < 0.001 {
            break;
        }
        let t2 = t1 - (e1 * rate);
        let mut e2 = (t2 + ZEROCNK) / pwrp - ZEROCNK;
        e2 += wobf(t2) - wobf(e2) - thetam;
        rate = (t2 - t1) / (e2 - e1);
        t1 = t2;
        e1 = e2;
    }

    t1 - e1 * rate
}

/// LCL temperature from temperature and dewpoint (both Celsius).
pub fn lcltemp(t: f64, td: f64) -> f64 {
    let s = t - td;
    let dlt = s * (1.2185 + 0.001278 * t + s * (-0.00219 + 1.173e-5 * s - 0.0000052 * t));
    t - dlt
}

/// Dry lift to LCL. Returns (p_lcl, t_lcl) in (hPa, Celsius).
pub fn drylift(p: f64, t: f64, td: f64) -> (f64, f64) {
    let t_lcl = lcltemp(t, td);
    let p_lcl = 1000.0
        * (((t_lcl + ZEROCNK) / ((t + ZEROCNK) * ((1000.0 / p).powf(ROCP)))))
            .powf(1.0 / ROCP);
    (p_lcl, t_lcl)
}

/// Saturation vapor pressure (hPa) at given temperature (Celsius).
/// Uses the SHARPpy 8th-order polynomial approximation (Eschner).
pub fn vappres(t: f64) -> f64 {
    let pol = t * (1.1112018e-17 + (t * -3.0994571e-20));
    let pol = t * (2.1874425e-13 + (t * (-1.789232e-15 + pol)));
    let pol = t * (4.3884180e-09 + (t * (-2.988388e-11 + pol)));
    let pol = t * (7.8736169e-05 + (t * (-6.111796e-07 + pol)));
    let pol = 0.99999683 + (t * (-9.082695e-03 + pol));
    6.1078 / pol.powi(8)
}

/// Mixing ratio (g/kg) of a parcel at pressure p (hPa) and temperature t (Celsius).
/// Includes Wexler enhancement factor for non-ideal gas behavior.
pub fn mixratio(p: f64, t: f64) -> f64 {
    // Enhancement Factor (Wexler)
    let x = 0.02 * (t - 12.5 + (7500.0 / p));
    let wfw = 1.0 + (0.0000045 * p) + (0.0014 * x * x);

    // Saturation Vapor Pressure (with enhancement)
    let fwesw = wfw * vappres(t);

    // Mixing Ratio (g/kg)
    621.97 * (fwesw / (p - fwesw))
}

/// Virtual temperature. Inputs and output all in Celsius.
/// t: temperature (C), p: pressure (hPa), td: dewpoint (C).
pub fn virtual_temp(t: f64, p: f64, td: f64) -> f64 {
    let w = mixratio(p, td) / 1000.0;
    let tk = t + ZEROCNK;
    let vt = tk * (1.0 + 0.61 * w);
    vt - ZEROCNK
}

/// Equivalent potential temperature. Returns value in Celsius.
/// p (hPa), t (C), td (C).
pub fn thetae(p: f64, t: f64, td: f64) -> f64 {
    let (p_lcl, t_lcl) = drylift(p, t, td);
    let theta = (t_lcl + ZEROCNK) * ((1000.0 / p_lcl).powf(ROCP));
    let r = mixratio(p, td) / 1000.0;
    let lc = 2500.0 - 2.37 * t_lcl;
    let te_k = theta * ((lc * 1000.0 * r) / (CP * (t_lcl + ZEROCNK))).exp();
    te_k - ZEROCNK
}

/// Temperature (Celsius) of air at given mixing ratio (g/kg) and pressure (hPa).
/// Ported from SHARPpy params.py.
pub fn temp_at_mixrat(w: f64, p: f64) -> f64 {
    let c1: f64 = 0.0498646455;
    let c2: f64 = 2.4082965;
    let c3: f64 = 7.07475;
    let c4: f64 = 38.9114;
    let c5: f64 = 0.0915;
    let c6: f64 = 1.2035;

    let x = (w * p / (622.0 + w)).log10();
    (10.0_f64.powf(c1 * x + c2) - c3
        + c4 * (10.0_f64.powf(c5 * x) - c6).powi(2))
        - ZEROCNK
}

// --- Helper Functions ---

/// Linear interpolation: given x between x1 and x2, interpolate between y1 and y2.
pub fn interp_linear(x: f64, x1: f64, x2: f64, y1: f64, y2: f64) -> f64 {
    if x2 == x1 {
        return y1;
    }
    y1 + (x - x1) * (y2 - y1) / (x2 - x1)
}

/// Interpolate height at a target pressure from pressure and height profiles
/// (both in decreasing pressure order, i.e. surface first).
pub fn get_height_at_pres(target_p: f64, p_prof: &[f64], h_prof: &[f64]) -> f64 {
    for i in 0..p_prof.len() - 1 {
        if p_prof[i] >= target_p && target_p >= p_prof[i + 1] {
            return interp_linear(target_p, p_prof[i], p_prof[i + 1], h_prof[i], h_prof[i + 1]);
        }
    }
    // Bounds check
    if target_p > p_prof[0] {
        return h_prof[0];
    }
    if target_p < p_prof[p_prof.len() - 1] {
        return h_prof[h_prof.len() - 1];
    }
    f64::NAN
}

/// Interpolate environmental temperature and dewpoint at a target pressure.
/// Uses log-pressure interpolation. Returns (t_interp, td_interp) in Celsius.
pub fn get_env_at_pres(
    target_p: f64,
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
) -> (f64, f64) {
    for i in 0..p_prof.len() - 1 {
        if p_prof[i] >= target_p && target_p >= p_prof[i + 1] {
            let log_p = target_p.ln();
            let log_p1 = p_prof[i].ln();
            let log_p2 = p_prof[i + 1].ln();
            let t_interp = interp_linear(log_p, log_p1, log_p2, t_prof[i], t_prof[i + 1]);
            let td_interp = interp_linear(log_p, log_p1, log_p2, td_prof[i], td_prof[i + 1]);
            return (t_interp, td_interp);
        }
    }
    (
        t_prof[t_prof.len() - 1],
        td_prof[td_prof.len() - 1],
    )
}

// --- Parcel Selectors ---

/// Returns Mixed Layer Parcel matching SHARPpy's calculation method.
/// Uses 1-2-1 weighting scheme (surface and top weight 1, inner levels weight 2).
/// Returns (p_start, t_start, td_start) all in (hPa, Celsius, Celsius).
pub fn get_mixed_layer_parcel(
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
    depth: f64,
) -> (f64, f64, f64) {
    let sfc_p = p_prof[0];
    let top_p = sfc_p - depth;

    // Surface (Bottom Bound) - Weight 1
    let theta_sfc = (t_prof[0] + ZEROCNK) * ((1000.0 / sfc_p).powf(ROCP));
    let td_sfc = td_prof[0];

    // Top Bound (Interpolated) - Weight 1
    let (t_top, td_top) = get_env_at_pres(top_p, p_prof, t_prof, td_prof);
    let theta_top = (t_top + ZEROCNK) * ((1000.0 / top_p).powf(ROCP));

    // Accumulators
    let mut sum_theta = theta_sfc + theta_top;
    let mut sum_p = sfc_p + top_p;
    let mut sum_td = td_sfc + td_top;
    let mut count = 2.0;

    // Inner Layers - Weight 2
    for i in 1..p_prof.len() {
        let p = p_prof[i];
        if p <= top_p {
            break;
        }
        let t = t_prof[i];
        let td = td_prof[i];
        let th = (t + ZEROCNK) * ((1000.0 / p).powf(ROCP));

        sum_theta += 2.0 * th;
        sum_p += 2.0 * p;
        sum_td += 2.0 * td;
        count += 2.0;
    }

    // Averages
    let avg_theta = sum_theta / count;
    let avg_p = sum_p / count;
    let avg_td = sum_td / count;

    // Parcel T: Bring Mean Theta back to Surface Pressure
    let avg_t_k = avg_theta * ((sfc_p / 1000.0).powf(ROCP));
    let avg_t = avg_t_k - ZEROCNK;

    // Parcel Td: Calculate mixing ratio from (Mean P, Mean Td), get dewpoint at surface
    let avg_w = mixratio(avg_p, avg_td);
    let parcel_td = temp_at_mixrat(avg_w, sfc_p);

    (sfc_p, avg_t, parcel_td)
}

/// Returns Most Unstable Parcel (highest theta-e in the lowest `depth` hPa).
/// Returns (p, t, td) all in (hPa, Celsius, Celsius).
pub fn get_most_unstable_parcel(
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
    depth: f64,
) -> (f64, f64, f64) {
    let sfc_p = p_prof[0];
    let limit_p = sfc_p - depth;
    let mut max_thetae = -999.0_f64;
    let mut best_idx = 0_usize;

    for i in 0..p_prof.len() {
        if p_prof[i] < limit_p {
            break;
        }
        let te = thetae(p_prof[i], t_prof[i], td_prof[i]);
        if te > max_thetae {
            max_thetae = te;
            best_idx = i;
        }
    }

    (p_prof[best_idx], t_prof[best_idx], td_prof[best_idx])
}

// --- Core CAPE/CIN Computation ---

/// Compute CAPE, CIN, LCL height, and LFC height for a grid column.
///
/// Inputs:
/// - p_prof, t_prof, td_prof: Model level profiles (surface first, decreasing pressure).
///   May be in Pa or hPa; may be in K or C (auto-detected and converted).
/// - height_agl: Height AGL profile (meters) matching model levels.
/// - psfc: Surface pressure (Pa or hPa).
/// - t2m: 2-meter temperature (K or C).
/// - td2m: 2-meter dewpoint (K or C).
/// - parcel_type: "sb", "ml", or "mu".
/// - ml_depth: Mixed layer depth in hPa (default 100).
/// - mu_depth: Most unstable search depth in hPa (default 300).
/// - top_m: Optional cap on integration height (meters AGL).
///
/// Returns (cape, cin, h_lcl, h_lfc) in (J/kg, J/kg, m AGL, m AGL).
pub fn cape_cin_core(
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
    height_agl: &[f64],
    psfc: f64,
    t2m: f64,
    td2m: f64,
    parcel_type: &str,
    ml_depth: f64,
    mu_depth: f64,
    top_m: Option<f64>,
) -> (f64, f64, f64, f64) {
    // --- 0. Unit Standardization ---
    let mut p_prof = p_prof.to_vec();
    let mut t_prof = t_prof.to_vec();
    let mut td_prof = td_prof.to_vec();
    let mut psfc_val = psfc;
    let mut t2m_val = t2m;
    let mut td2m_val = td2m;

    if psfc_val > 2000.0 {
        for v in p_prof.iter_mut() {
            *v /= 100.0;
        }
        psfc_val /= 100.0;
    }

    if t2m_val > 150.0 {
        for v in t_prof.iter_mut() {
            *v -= ZEROCNK;
        }
        for v in td_prof.iter_mut() {
            *v -= ZEROCNK;
        }
        t2m_val -= ZEROCNK;
        td2m_val -= ZEROCNK;
    }

    // Ensure Td2m <= T2m
    if td2m_val > t2m_val {
        td2m_val = t2m_val;
    }

    // Prepend surface data to profiles
    let n = p_prof.len();
    let mut new_p = Vec::with_capacity(n + 1);
    let mut new_t = Vec::with_capacity(n + 1);
    let mut new_td = Vec::with_capacity(n + 1);
    let mut new_h = Vec::with_capacity(n + 1);

    new_p.push(psfc_val);
    new_t.push(t2m_val);
    new_td.push(td2m_val);
    new_h.push(0.0);

    for i in 0..n {
        new_p.push(p_prof[i]);
        new_t.push(t_prof[i]);
        new_td.push(if td_prof[i] <= t_prof[i] {
            td_prof[i]
        } else {
            t_prof[i]
        });
        new_h.push(height_agl[i]);
    }

    let p_prof = new_p;
    let t_prof = new_t;
    let td_prof = new_td;
    let height_agl = new_h;

    // --- 1. Select Parcel ---
    let (p_start, t_start, td_start) = match parcel_type {
        "ml" => get_mixed_layer_parcel(&p_prof, &t_prof, &td_prof, ml_depth),
        "mu" => get_most_unstable_parcel(&p_prof, &t_prof, &td_prof, mu_depth),
        _ => (psfc_val, t2m_val, td2m_val), // "sb" default
    };

    // --- 2. Find LCL (Analytic) ---
    let (p_lcl, t_lcl) = drylift(p_start, t_start, td_start);
    let h_lcl = get_height_at_pres(p_lcl, &p_prof, &height_agl);

    // Calculate Theta-M (constant for moist ascent)
    let theta_start_k = (t_lcl + ZEROCNK) * ((1000.0 / p_lcl).powf(ROCP));
    let theta_start_c = theta_start_k - ZEROCNK;
    let thetam = theta_start_c - wobf(theta_start_c) + wobf(t_lcl);

    // --- PASS 1: Geometric Scan for LFC and EL ---
    let mut el_p = p_lcl;
    let mut lfc_p = p_lcl;

    let mut found_positive_layer = false;
    let mut in_pos_layer = false;

    // Find start index (first level at or above LCL)
    let mut start_idx = 0;
    for i in 0..p_prof.len() {
        if p_prof[i] <= p_lcl {
            start_idx = i;
            break;
        }
    }

    for i in start_idx..p_prof.len() {
        let p_curr = p_prof[i];

        // Environmental Tv
        let tv_env = virtual_temp(t_prof[i], p_curr, td_prof[i]);
        // Parcel Tv
        let t_parc = satlift(p_curr, thetam);
        let tv_parc = virtual_temp(t_parc, p_curr, t_parc);

        let buoyancy = tv_parc - tv_env;

        if buoyancy > 0.0 {
            if !in_pos_layer {
                in_pos_layer = true;

                // Find crossing (LFC of this layer)
                let curr_pos_bottom = if i > 0 {
                    let p_prev = p_prof[i - 1];
                    let tv_env_prev = virtual_temp(t_prof[i - 1], p_prev, td_prof[i - 1]);
                    let t_parc_prev = satlift(p_prev, thetam);
                    let tv_parc_prev = virtual_temp(t_parc_prev, p_prev, t_parc_prev);
                    let buoy_prev = tv_parc_prev - tv_env_prev;

                    if buoyancy != buoy_prev {
                        let frac = (0.0 - buoy_prev) / (buoyancy - buoy_prev);
                        p_prev + frac * (p_curr - p_prev)
                    } else {
                        p_curr
                    }
                } else {
                    p_curr
                };

                lfc_p = curr_pos_bottom;
                el_p = p_prof[p_prof.len() - 1];
                found_positive_layer = true;
            }
        } else {
            // buoyancy <= 0
            if in_pos_layer {
                in_pos_layer = false;

                // Find crossing (EL)
                let p_prev = p_prof[i - 1];
                let tv_env_prev = virtual_temp(t_prof[i - 1], p_prev, td_prof[i - 1]);
                let t_parc_prev = satlift(p_prev, thetam);
                let tv_parc_prev = virtual_temp(t_parc_prev, p_prev, t_parc_prev);
                let buoy_prev = tv_parc_prev - tv_env_prev;

                let curr_pos_top = if buoyancy != buoy_prev {
                    let frac = (0.0 - buoy_prev) / (buoyancy - buoy_prev);
                    p_prev + frac * (p_curr - p_prev)
                } else {
                    p_curr
                };

                el_p = curr_pos_top;
            }
        }
    }

    if in_pos_layer {
        el_p = p_prof[p_prof.len() - 1];
    }

    // Return zeros if no instability found
    if !found_positive_layer {
        return (0.0, 0.0, h_lcl, f64::NAN);
    }

    // If LFC is below LCL, set to LCL
    if lfc_p.is_nan() || lfc_p > p_lcl {
        lfc_p = p_lcl;
    }
    let h_lfc = get_height_at_pres(lfc_p, &p_prof, &height_agl);

    // --- PASS 2: Integration ---
    let mut p_top_limit = el_p;
    if let Some(top_m_val) = top_m {
        // Reverse profiles for height->pressure lookup
        let h_rev: Vec<f64> = height_agl.iter().copied().rev().collect();
        let p_rev: Vec<f64> = p_prof.iter().copied().rev().collect();
        let p_top_m = get_height_at_pres(top_m_val, &h_rev, &p_rev);
        if p_top_m >= p_top_limit {
            p_top_limit = p_top_m.max(p_prof[p_prof.len() - 1]);
        }
    }

    let mut total_cape = 0.0_f64;
    let mut total_cin = 0.0_f64;

    // --- Integrate CIN from Surface (p_start) to LCL (dry adiabat) ---
    let mut curr_dry_p = p_start;
    let mut dry_idx = start_idx;

    while curr_dry_p > p_lcl {
        // Find next model level
        let mut next_p = -1.0_f64;
        let mut temp_idx = dry_idx;
        while temp_idx < p_prof.len() {
            if p_prof[temp_idx] < curr_dry_p - 0.01 {
                next_p = p_prof[temp_idx];
                dry_idx = temp_idx;
                break;
            }
            temp_idx += 1;
        }

        let target_dry_p = if next_p == -1.0 || next_p < p_lcl {
            p_lcl
        } else {
            next_p
        };

        // Standard sub-stepping for the dry layer
        let p1 = curr_dry_p;
        let p2 = target_dry_p;
        let p_mid = (p1 + p2) / 2.0;

        // Environment at p_mid
        let (t_env, td_env) = get_env_at_pres(p_mid, &p_prof, &t_prof, &td_prof);
        let tv_env = virtual_temp(t_env, p_mid, td_env);

        // Parcel temperature via dry adiabat
        let theta_start_k = (t_start + ZEROCNK) * ((1000.0 / p_start).powf(ROCP));
        let t_parc_k = theta_start_k * ((p_mid / 1000.0).powf(ROCP));
        let t_parc = t_parc_k - ZEROCNK;

        // Parcel mixing ratio is constant (from starting dewpoint)
        let r_parcel = mixratio(p_start, td_start);

        // Virtual Temp of Parcel with known W
        let tv_parc = (t_parc + ZEROCNK) * (1.0 + 0.61 * (r_parcel / 1000.0)) - ZEROCNK;

        let val = RD * (tv_parc - tv_env) * (p1 / p2).ln();

        // In the dry layer, only accumulate CIN
        if val < 0.0 {
            total_cin += val;
        }

        curr_dry_p = target_dry_p;
    }

    // --- Integrate from LCL to EL (moist adiabat) ---
    let mut curr_p = p_lcl;
    let mut idx = 0;
    while idx < p_prof.len() && p_prof[idx] > p_lcl {
        idx += 1;
    }

    while curr_p > p_top_limit {
        // Find next model level
        let mut next_model_p = -1.0_f64;
        let mut temp_idx = idx;
        while temp_idx < p_prof.len() {
            if p_prof[temp_idx] < curr_p - 0.01 {
                next_model_p = p_prof[temp_idx];
                idx = temp_idx;
                break;
            }
            temp_idx += 1;
        }

        let target_p = if next_model_p == -1.0 || next_model_p < p_top_limit {
            p_top_limit
        } else {
            next_model_p
        };

        let dp_total = curr_p - target_p;
        let n_steps = if dp_total > 10.0 {
            (dp_total / 10.0) as usize + 1
        } else {
            1
        };
        let step_size = dp_total / n_steps as f64;

        for k in 0..n_steps {
            let p1 = curr_p - k as f64 * step_size;
            let p2 = curr_p - (k + 1) as f64 * step_size;
            let p_mid = (p1 + p2) / 2.0;

            let (t_env, td_env) = get_env_at_pres(p_mid, &p_prof, &t_prof, &td_prof);
            let tv_env = virtual_temp(t_env, p_mid, td_env);

            let t_parc = satlift(p_mid, thetam);
            let tv_parc = virtual_temp(t_parc, p_mid, t_parc);

            let val = RD * (tv_parc - tv_env) * (p1 / p2).ln();

            if val > 0.0 {
                total_cape += val;
            } else {
                total_cin += val;
            }
        }

        curr_p = target_p;
    }

    (total_cape, total_cin, h_lcl, h_lfc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wobf_negative() {
        let result = wobf(-10.0);
        assert!(result > 0.0, "wobf(-10) should be positive");
    }

    #[test]
    fn test_wobf_positive() {
        let result = wobf(30.0);
        assert!(result > 0.0, "wobf(30) should be positive");
    }

    #[test]
    fn test_vappres_at_zero() {
        let es = vappres(0.0);
        // At 0C, saturation vapor pressure should be ~6.1 hPa
        assert!((es - 6.1078).abs() < 0.01);
    }

    #[test]
    fn test_mixratio() {
        let w = mixratio(1000.0, 20.0);
        // At 1000 hPa, 20C, mixing ratio should be roughly 14-15 g/kg
        assert!(w > 10.0 && w < 20.0);
    }

    #[test]
    fn test_lcltemp_saturated() {
        // When T == Td, LCL temp should equal T
        let t_lcl = lcltemp(20.0, 20.0);
        assert!((t_lcl - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_interp_linear() {
        let result = interp_linear(5.0, 0.0, 10.0, 0.0, 100.0);
        assert!((result - 50.0).abs() < 1e-10);
    }
}
