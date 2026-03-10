/// Meteorological thermodynamic functions ported from wrfsolar's metfuncs.py.
/// Pure math - no external dependencies. All functions are direct ports of the
/// SHARPpy-derived implementations used in the Python codebase.

// --- Physical Constants ---
pub const RD: f64 = 287.058;       // Dry air gas constant (J/(kg*K))
pub const RV: f64 = 461.5;         // Water vapor gas constant (J/(kg*K))
pub const CP: f64 = 1005.7;        // Specific heat at constant pressure (J/(kg*K))
pub const G: f64 = 9.80665;        // Gravitational acceleration (m/s^2)
pub const ROCP: f64 = 0.28571426;  // Rd/Cp
pub const ZEROCNK: f64 = 273.15;   // 0 Celsius in Kelvin
pub const MISSING: f64 = -9999.0;
pub const EPS: f64 = 0.62197;      // Rd/Rv = Mw/Md (ratio of molecular weights)
pub const LAPSE_STD: f64 = 0.0065;  // Standard atmosphere lapse rate (K/m)
pub const P0_STD: f64 = 1013.25;    // Standard sea level pressure (hPa)
pub const T0_STD: f64 = 288.15;     // Standard sea level temperature (K)

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

// =============================================================================
// Temperature Conversions
// =============================================================================

/// Convert Celsius to Fahrenheit.
pub fn celsius_to_fahrenheit(t: f64) -> f64 {
    t * 9.0 / 5.0 + 32.0
}

/// Convert Fahrenheit to Celsius.
pub fn fahrenheit_to_celsius(t: f64) -> f64 {
    (t - 32.0) * 5.0 / 9.0
}

/// Convert Celsius to Kelvin.
pub fn celsius_to_kelvin(t: f64) -> f64 {
    t + ZEROCNK
}

/// Convert Kelvin to Celsius.
pub fn kelvin_to_celsius(t: f64) -> f64 {
    t - ZEROCNK
}

// =============================================================================
// Saturation / Moisture Functions
// =============================================================================

/// Saturation vapor pressure (hPa) using Bolton (1980) formula.
/// Input: temperature in Celsius.
pub fn saturation_vapor_pressure(t_c: f64) -> f64 {
    6.112 * ((17.67 * t_c) / (t_c + 243.5)).exp()
}

/// Dewpoint (Celsius) from temperature (Celsius) and relative humidity (%).
/// Uses the Magnus formula inverted.
pub fn dewpoint_from_rh(t_c: f64, rh: f64) -> f64 {
    let rh_frac = rh / 100.0;
    let es = saturation_vapor_pressure(t_c);
    let e = rh_frac * es;
    // Invert Bolton: Td = 243.5 * ln(e/6.112) / (17.67 - ln(e/6.112))
    let ln_ratio = (e / 6.112).ln();
    243.5 * ln_ratio / (17.67 - ln_ratio)
}

/// Relative humidity (%) from temperature and dewpoint (both Celsius).
pub fn rh_from_dewpoint(t_c: f64, td_c: f64) -> f64 {
    let es = saturation_vapor_pressure(t_c);
    let e = saturation_vapor_pressure(td_c);
    (e / es) * 100.0
}

/// Specific humidity (kg/kg) from pressure (hPa) and mixing ratio (g/kg).
pub fn specific_humidity(p_hpa: f64, w_gkg: f64) -> f64 {
    let _ = p_hpa; // pressure not needed for this conversion
    let w = w_gkg / 1000.0; // kg/kg
    w / (1.0 + w)
}

/// Mixing ratio (g/kg) from specific humidity (kg/kg).
pub fn mixing_ratio_from_specific_humidity(q: f64) -> f64 {
    (q / (1.0 - q)) * 1000.0
}

/// Saturation mixing ratio (g/kg) at given pressure (hPa) and temperature (Celsius).
/// Uses Bolton saturation vapor pressure.
pub fn saturation_mixing_ratio(p_hpa: f64, t_c: f64) -> f64 {
    let es = saturation_vapor_pressure(t_c);
    EPS * es / (p_hpa - es) * 1000.0
}

/// Vapor pressure (hPa) from dewpoint temperature (Celsius).
/// Uses Bolton (1980) formula.
pub fn vapor_pressure_from_dewpoint(td_c: f64) -> f64 {
    saturation_vapor_pressure(td_c)
}

/// Wet bulb temperature (Celsius) using iterative Normand's rule.
/// p_hpa: pressure (hPa), t_c: temperature (C), td_c: dewpoint (C).
pub fn wet_bulb_temperature(p_hpa: f64, t_c: f64, td_c: f64) -> f64 {
    // Lift parcel to LCL, then descend moist adiabatically
    let (p_lcl, t_lcl) = drylift(p_hpa, t_c, td_c);
    // theta_m for the moist descent
    let theta_c = t_lcl + ZEROCNK;
    let theta_sfc = theta_c * ((1000.0 / p_lcl).powf(ROCP));
    let theta_start_c = theta_sfc - ZEROCNK;
    let thetam = theta_start_c - wobf(theta_start_c) + wobf(t_lcl);
    // Descend moist adiabatically from LCL to original pressure
    satlift(p_hpa, thetam)
}

/// Frost point temperature (Celsius) from temperature (C) and relative humidity (%).
/// Uses the Magnus formula over ice.
pub fn frost_point(t_c: f64, rh: f64) -> f64 {
    // Saturation vapor pressure over water
    let es_water = saturation_vapor_pressure(t_c);
    let e = (rh / 100.0) * es_water;
    // Invert Magnus formula over ice:
    // ei = 6.112 * exp(22.46 * T / (T + 272.62))
    // ln(e/6.112) = 22.46 * Tf / (Tf + 272.62)
    let ln_ratio = (e / 6.112).ln();
    272.62 * ln_ratio / (22.46 - ln_ratio)
}

/// Psychrometric vapor pressure (hPa) using the psychrometric equation.
/// t_c: dry bulb (C), tw_c: wet bulb (C), p_hpa: pressure (hPa).
pub fn psychrometric_vapor_pressure(t_c: f64, tw_c: f64, p_hpa: f64) -> f64 {
    let es_tw = saturation_vapor_pressure(tw_c);
    // Psychrometer constant for aspirated psychrometer: 6.6e-4
    let a = 6.6e-4;
    es_tw - a * p_hpa * (t_c - tw_c)
}

// =============================================================================
// Potential Temperature Functions
// =============================================================================

/// Potential temperature (K) from pressure (hPa) and temperature (Celsius).
/// Uses Poisson's equation: theta = T * (1000/p)^(Rd/Cp).
pub fn potential_temperature(p_hpa: f64, t_c: f64) -> f64 {
    let t_k = t_c + ZEROCNK;
    t_k * (1000.0 / p_hpa).powf(ROCP)
}

/// Equivalent potential temperature (K) using Bolton (1980) formula.
/// p_hpa: pressure (hPa), t_c: temperature (C), td_c: dewpoint (C).
pub fn equivalent_potential_temperature(p_hpa: f64, t_c: f64, td_c: f64) -> f64 {
    let t_k = t_c + ZEROCNK;
    let td_k = td_c + ZEROCNK;
    // Bolton LCL temperature
    let t_lcl = 56.0 + 1.0 / (1.0 / (td_k - 56.0) + (t_k / td_k).ln() / 800.0);
    // Mixing ratio (kg/kg)
    let e = saturation_vapor_pressure(td_c);
    let r = EPS * e / (p_hpa - e); // kg/kg
    // Bolton theta-e
    let theta_e = t_k * (1000.0 / p_hpa).powf(0.2854 * (1.0 - 0.28 * r))
        * (3036.0 / t_lcl - 1.78).exp().powf(r * (1.0 + 0.448 * r));
    theta_e
}

/// Wet bulb potential temperature (K) from pressure (hPa), temp (C), dewpoint (C).
/// Computed by finding the wet bulb temperature, then computing its potential temperature
/// along the moist adiabat to 1000 hPa.
pub fn wet_bulb_potential_temperature(p_hpa: f64, t_c: f64, td_c: f64) -> f64 {
    // Lift to LCL, then descend moist adiabatically to 1000 hPa
    let (p_lcl, t_lcl) = drylift(p_hpa, t_c, td_c);
    let theta_c = t_lcl + ZEROCNK;
    let theta_sfc = theta_c * ((1000.0 / p_lcl).powf(ROCP));
    let theta_start_c = theta_sfc - ZEROCNK;
    let thetam = theta_start_c - wobf(theta_start_c) + wobf(t_lcl);
    let tw_1000 = satlift(1000.0, thetam);
    tw_1000 + ZEROCNK
}

/// Virtual potential temperature (K) from pressure (hPa), temp (C), mixing ratio (g/kg).
pub fn virtual_potential_temperature(p_hpa: f64, t_c: f64, w_gkg: f64) -> f64 {
    let theta = potential_temperature(p_hpa, t_c);
    let w = w_gkg / 1000.0;
    theta * (1.0 + 0.61 * w)
}

// =============================================================================
// Lifted / Parcel Functions
// =============================================================================

/// LCL pressure (hPa) from surface pressure (hPa), temp (C), dewpoint (C).
pub fn lcl_pressure(p_hpa: f64, t_c: f64, td_c: f64) -> f64 {
    let (p_lcl, _t_lcl) = drylift(p_hpa, t_c, td_c);
    p_lcl
}

/// Lift a parcel and compute parcel temperature at each level.
/// Returns parcel virtual temperature profile above LCL via moist adiabat.
fn lift_parcel_profile(
    p_prof: &[f64],
    t_prof: &[f64],
    td_prof: &[f64],
) -> (f64, f64, Vec<f64>) {
    // Use surface-based parcel
    let p_sfc = p_prof[0];
    let t_sfc = t_prof[0];
    let td_sfc = td_prof[0];

    let (p_lcl, t_lcl) = drylift(p_sfc, t_sfc, td_sfc);

    // Compute thetam for moist ascent
    let theta_k = (t_lcl + ZEROCNK) * ((1000.0 / p_lcl).powf(ROCP));
    let theta_c = theta_k - ZEROCNK;
    let thetam = theta_c - wobf(theta_c) + wobf(t_lcl);

    // Compute parcel Tv at each level
    let mut parcel_tv = Vec::with_capacity(p_prof.len());
    let theta_dry_k = (t_sfc + ZEROCNK) * ((1000.0 / p_sfc).powf(ROCP));
    let r_parcel = mixratio(p_sfc, td_sfc);

    for i in 0..p_prof.len() {
        let p = p_prof[i];
        if p > p_lcl {
            // Below LCL: dry adiabat
            let t_parc_k = theta_dry_k * ((p / 1000.0).powf(ROCP));
            let t_parc = t_parc_k - ZEROCNK;
            let tv = (t_parc + ZEROCNK) * (1.0 + 0.61 * (r_parcel / 1000.0)) - ZEROCNK;
            parcel_tv.push(tv);
        } else {
            // Above LCL: moist adiabat
            let t_parc = satlift(p, thetam);
            let tv = virtual_temp(t_parc, p, t_parc);
            parcel_tv.push(tv);
        }
    }

    (p_lcl, t_lcl, parcel_tv)
}

/// Level of Free Convection (LFC).
/// Returns Option<(pressure_hPa, temperature_C)> of the LFC.
/// Profiles should be surface-first, decreasing pressure.
pub fn lfc(
    p_profile: &[f64],
    t_profile: &[f64],
    td_profile: &[f64],
) -> Option<(f64, f64)> {
    let (p_lcl, _t_lcl, parcel_tv) = lift_parcel_profile(p_profile, t_profile, td_profile);

    // Search above LCL for first crossing where parcel becomes warmer than environment
    for i in 1..p_profile.len() {
        if p_profile[i] > p_lcl {
            continue;
        }
        let tv_env_prev = virtual_temp(t_profile[i - 1], p_profile[i - 1], td_profile[i - 1]);
        let tv_env = virtual_temp(t_profile[i], p_profile[i], td_profile[i]);
        let buoy_prev = parcel_tv[i - 1] - tv_env_prev;
        let buoy = parcel_tv[i] - tv_env;

        if buoy_prev <= 0.0 && buoy > 0.0 {
            // Interpolate crossing
            let frac = (0.0 - buoy_prev) / (buoy - buoy_prev);
            let p_lfc = p_profile[i - 1] + frac * (p_profile[i] - p_profile[i - 1]);
            let t_lfc = t_profile[i - 1] + frac * (t_profile[i] - t_profile[i - 1]);
            return Some((p_lfc, t_lfc));
        }

        // If parcel is already warmer right at LCL
        if buoy > 0.0 && p_profile[i] <= p_lcl && (i == 0 || p_profile[i - 1] > p_lcl) {
            return Some((p_profile[i], t_profile[i]));
        }
    }

    None
}

/// Equilibrium Level (EL).
/// Returns Option<(pressure_hPa, temperature_C)> of the EL.
/// Profiles should be surface-first, decreasing pressure.
pub fn el(
    p_profile: &[f64],
    t_profile: &[f64],
    td_profile: &[f64],
) -> Option<(f64, f64)> {
    let (p_lcl, _t_lcl, parcel_tv) = lift_parcel_profile(p_profile, t_profile, td_profile);

    let mut found_positive = false;
    let mut last_el: Option<(f64, f64)> = None;

    for i in 1..p_profile.len() {
        if p_profile[i] > p_lcl {
            continue;
        }
        let tv_env_prev = virtual_temp(t_profile[i - 1], p_profile[i - 1], td_profile[i - 1]);
        let tv_env = virtual_temp(t_profile[i], p_profile[i], td_profile[i]);
        let buoy_prev = parcel_tv[i - 1] - tv_env_prev;
        let buoy = parcel_tv[i] - tv_env;

        if buoy > 0.0 {
            found_positive = true;
        }

        if found_positive && buoy_prev > 0.0 && buoy <= 0.0 {
            let frac = (0.0 - buoy_prev) / (buoy - buoy_prev);
            let p_el = p_profile[i - 1] + frac * (p_profile[i] - p_profile[i - 1]);
            let t_el = t_profile[i - 1] + frac * (t_profile[i] - t_profile[i - 1]);
            last_el = Some((p_el, t_el));
        }
    }

    last_el
}

/// Lifted Index: temperature difference between environment and parcel at 500 hPa.
/// Positive values indicate stable conditions, negative values indicate instability.
pub fn lifted_index(
    p_profile: &[f64],
    t_profile: &[f64],
    td_profile: &[f64],
) -> f64 {
    let p_sfc = p_profile[0];
    let t_sfc = t_profile[0];
    let td_sfc = td_profile[0];

    let (p_lcl, t_lcl) = drylift(p_sfc, t_sfc, td_sfc);

    // Get parcel temperature at 500 hPa
    let t_parcel_500 = if 500.0 >= p_lcl {
        // 500 hPa is below LCL (unlikely but handle it)
        let theta_k = (t_sfc + ZEROCNK) * ((1000.0 / p_sfc).powf(ROCP));
        theta_k * ((500.0_f64 / 1000.0).powf(ROCP)) - ZEROCNK
    } else {
        let theta_k = (t_lcl + ZEROCNK) * ((1000.0 / p_lcl).powf(ROCP));
        let theta_c = theta_k - ZEROCNK;
        let thetam = theta_c - wobf(theta_c) + wobf(t_lcl);
        satlift(500.0, thetam)
    };

    // Interpolate environment temperature at 500 hPa
    let (t_env_500, _td_env_500) = get_env_at_pres(500.0, p_profile, t_profile, td_profile);

    t_env_500 - t_parcel_500
}

/// Convective Condensation Level (CCL).
/// The level where the saturation mixing ratio equals the surface mixing ratio.
/// Returns Option<(pressure_hPa, temperature_C)>.
pub fn ccl(
    p_profile: &[f64],
    t_profile: &[f64],
    td_profile: &[f64],
) -> Option<(f64, f64)> {
    let w_sfc = mixratio(p_profile[0], td_profile[0]);

    // Search upward for where saturation mixing ratio equals surface mixing ratio
    for i in 1..p_profile.len() {
        let ws_prev = mixratio(p_profile[i - 1], t_profile[i - 1]);
        let ws_curr = mixratio(p_profile[i], t_profile[i]);

        if ws_prev >= w_sfc && ws_curr < w_sfc {
            // Interpolate
            let frac = (w_sfc - ws_prev) / (ws_curr - ws_prev);
            let p_ccl = p_profile[i - 1] + frac * (p_profile[i] - p_profile[i - 1]);
            let t_ccl = t_profile[i - 1] + frac * (t_profile[i] - t_profile[i - 1]);
            return Some((p_ccl, t_ccl));
        }
    }

    None
}

/// Convective temperature (Celsius).
/// The surface temperature needed to produce convection (reach CCL via dry adiabat).
pub fn convective_temperature(
    p_profile: &[f64],
    t_profile: &[f64],
    td_profile: &[f64],
) -> f64 {
    if let Some((p_ccl, t_ccl)) = ccl(p_profile, t_profile, td_profile) {
        // Bring CCL temperature down dry-adiabatically to surface
        let theta_k = (t_ccl + ZEROCNK) * ((1000.0 / p_ccl).powf(ROCP));
        theta_k * ((p_profile[0] / 1000.0).powf(ROCP)) - ZEROCNK
    } else {
        MISSING
    }
}

// =============================================================================
// Density / Height Functions
// =============================================================================

/// Air density (kg/m^3) from pressure (hPa), temperature (C), mixing ratio (g/kg).
/// Uses virtual temperature for moist air density.
pub fn density(p_hpa: f64, t_c: f64, w_gkg: f64) -> f64 {
    let p_pa = p_hpa * 100.0;
    let t_k = t_c + ZEROCNK;
    let w = w_gkg / 1000.0;
    let tv_k = t_k * (1.0 + 0.61 * w);
    p_pa / (RD * tv_k)
}

/// Virtual temperature (Celsius) from temperature (C), dewpoint (C), pressure (hPa).
/// Computes mixing ratio from dewpoint and pressure.
pub fn virtual_temperature_from_dewpoint(t_c: f64, td_c: f64, p_hpa: f64) -> f64 {
    virtual_temp(t_c, p_hpa, td_c)
}

/// Hypsometric thickness (meters) of a layer between two pressure levels.
/// p_bottom, p_top in hPa, t_mean_k in Kelvin.
pub fn thickness_hypsometric(p_bottom: f64, p_top: f64, t_mean_k: f64) -> f64 {
    (RD * t_mean_k / G) * (p_bottom / p_top).ln()
}

/// Standard atmosphere: pressure (hPa) to geopotential height (meters).
/// Valid for troposphere (below ~11 km).
pub fn pressure_to_height_std(p_hpa: f64) -> f64 {
    (T0_STD / LAPSE_STD) * (1.0 - (p_hpa / P0_STD).powf((RD * LAPSE_STD) / G))
}

/// Standard atmosphere: height (meters) to pressure (hPa).
/// Valid for troposphere (below ~11 km).
pub fn height_to_pressure_std(h_m: f64) -> f64 {
    P0_STD * (1.0 - LAPSE_STD * h_m / T0_STD).powf(G / (RD * LAPSE_STD))
}

/// Convert altimeter setting (hPa) to station pressure (hPa).
/// elevation_m: station elevation in meters.
pub fn altimeter_to_station_pressure(alt_hpa: f64, elevation_m: f64) -> f64 {
    // From the altimeter setting equation (NWS)
    let k = ROCP; // Rd/Cp
    let t0 = T0_STD;
    let _p0 = P0_STD;
    let l = LAPSE_STD;

    // Station pressure from altimeter equation:
    // alt = p_stn * (1 + (p0/p_stn)^k * (l*elev/t0))^(1/k)
    // Iterative approach: p_stn ≈ alt * (1 - l*elev/t0)^(1/k)
    // More precise: use the standard relationship
    let ratio = 1.0 - (l * elevation_m) / (t0 + l * elevation_m);
    alt_hpa * ratio.powf(1.0 / k)
}

/// Convert station pressure to sea level pressure (hPa).
/// p_station (hPa), elevation_m (meters), t_c: station temperature (Celsius).
pub fn station_to_sea_level_pressure(p_station: f64, elevation_m: f64, t_c: f64) -> f64 {
    let t_k = t_c + ZEROCNK;
    // Use the hypsometric equation to extrapolate to sea level
    // Mean virtual temperature of the fictitious column below the station
    let t_mean = t_k + LAPSE_STD * elevation_m / 2.0;
    p_station * (G * elevation_m / (RD * t_mean)).exp()
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

    // =========================================================================
    // Temperature Conversion Tests
    // =========================================================================

    #[test]
    fn test_celsius_to_fahrenheit() {
        assert!((celsius_to_fahrenheit(0.0) - 32.0).abs() < 1e-10);
        assert!((celsius_to_fahrenheit(100.0) - 212.0).abs() < 1e-10);
        assert!((celsius_to_fahrenheit(-40.0) - (-40.0)).abs() < 1e-10);
    }

    #[test]
    fn test_fahrenheit_to_celsius() {
        assert!((fahrenheit_to_celsius(32.0) - 0.0).abs() < 1e-10);
        assert!((fahrenheit_to_celsius(212.0) - 100.0).abs() < 1e-10);
        assert!((fahrenheit_to_celsius(-40.0) - (-40.0)).abs() < 1e-10);
    }

    #[test]
    fn test_celsius_to_kelvin() {
        assert!((celsius_to_kelvin(0.0) - 273.15).abs() < 1e-10);
        assert!((celsius_to_kelvin(-273.15) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_kelvin_to_celsius() {
        assert!((kelvin_to_celsius(273.15) - 0.0).abs() < 1e-10);
        assert!((kelvin_to_celsius(0.0) - (-273.15)).abs() < 1e-10);
    }

    #[test]
    fn test_roundtrip_temp_conversions() {
        let t = 25.0;
        assert!((fahrenheit_to_celsius(celsius_to_fahrenheit(t)) - t).abs() < 1e-10);
        assert!((kelvin_to_celsius(celsius_to_kelvin(t)) - t).abs() < 1e-10);
    }

    // =========================================================================
    // Saturation / Moisture Tests
    // =========================================================================

    #[test]
    fn test_saturation_vapor_pressure_at_0c() {
        let es = saturation_vapor_pressure(0.0);
        // Bolton at 0C: 6.112 * exp(0) = 6.112 hPa
        assert!((es - 6.112).abs() < 0.01);
    }

    #[test]
    fn test_saturation_vapor_pressure_at_20c() {
        let es = saturation_vapor_pressure(20.0);
        // At 20C, es should be ~23.4 hPa
        assert!((es - 23.4).abs() < 0.5);
    }

    #[test]
    fn test_saturation_vapor_pressure_at_100c() {
        let es = saturation_vapor_pressure(100.0);
        // Bolton formula gives ~1014-1020 at 100C (slightly differs from exact 1013.25)
        assert!((es - 1013.0).abs() < 50.0, "es={es}");
    }

    #[test]
    fn test_dewpoint_from_rh_saturated() {
        // At 100% RH, dewpoint == temperature
        let td = dewpoint_from_rh(20.0, 100.0);
        assert!((td - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_dewpoint_from_rh_typical() {
        // At 50% RH and 20C, dewpoint should be ~9.3C
        let td = dewpoint_from_rh(20.0, 50.0);
        assert!((td - 9.3).abs() < 0.5);
    }

    #[test]
    fn test_rh_from_dewpoint_saturated() {
        let rh = rh_from_dewpoint(20.0, 20.0);
        assert!((rh - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_rh_from_dewpoint_roundtrip() {
        let t = 25.0;
        let rh_orig = 65.0;
        let td = dewpoint_from_rh(t, rh_orig);
        let rh_back = rh_from_dewpoint(t, td);
        assert!((rh_back - rh_orig).abs() < 0.1);
    }

    #[test]
    fn test_specific_humidity() {
        // 10 g/kg mixing ratio -> q ≈ 0.0099 kg/kg
        let q = specific_humidity(1000.0, 10.0);
        assert!((q - 0.009901).abs() < 0.001);
    }

    #[test]
    fn test_mixing_ratio_from_specific_humidity_roundtrip() {
        let w_orig = 10.0; // g/kg
        let q = specific_humidity(1000.0, w_orig);
        let w_back = mixing_ratio_from_specific_humidity(q);
        assert!((w_back - w_orig).abs() < 0.01);
    }

    #[test]
    fn test_saturation_mixing_ratio_at_20c() {
        let ws = saturation_mixing_ratio(1000.0, 20.0);
        // At 1000 hPa, 20C, ws should be ~14.7 g/kg
        assert!(ws > 13.0 && ws < 16.0);
    }

    #[test]
    fn test_vapor_pressure_from_dewpoint() {
        let e = vapor_pressure_from_dewpoint(10.0);
        let es = saturation_vapor_pressure(10.0);
        assert!((e - es).abs() < 1e-10);
    }

    #[test]
    fn test_wet_bulb_temperature_saturated() {
        // When saturated (T == Td), wet bulb should equal T
        let tw = wet_bulb_temperature(1000.0, 20.0, 20.0);
        assert!((tw - 20.0).abs() < 0.5);
    }

    #[test]
    fn test_wet_bulb_temperature_between_t_and_td() {
        // Wet bulb should be between Td and T
        let tw = wet_bulb_temperature(1000.0, 30.0, 15.0);
        assert!(tw >= 15.0 && tw <= 30.0, "Tw={tw} should be between 15 and 30");
    }

    #[test]
    fn test_frost_point_below_zero() {
        // Frost point at -10C, 80% RH should be below dewpoint
        let fp = frost_point(-10.0, 80.0);
        let td = dewpoint_from_rh(-10.0, 80.0);
        // Frost point should be close to but slightly above dewpoint at sub-zero temps
        assert!((fp - td).abs() < 3.0, "frost_point={fp}, dewpoint={td}");
    }

    #[test]
    fn test_psychrometric_vapor_pressure() {
        // At saturation (T == Tw), psychrometric e should equal es(T)
        let e = psychrometric_vapor_pressure(20.0, 20.0, 1000.0);
        let es = saturation_vapor_pressure(20.0);
        assert!((e - es).abs() < 0.01);
    }

    // =========================================================================
    // Potential Temperature Tests
    // =========================================================================

    #[test]
    fn test_potential_temperature_at_1000hpa() {
        // At 1000 hPa, theta == T (in K)
        let theta = potential_temperature(1000.0, 20.0);
        assert!((theta - 293.15).abs() < 0.01);
    }

    #[test]
    fn test_potential_temperature_at_850hpa() {
        // At 850 hPa, 10C, theta should be ~25C (~298K)
        let theta = potential_temperature(850.0, 10.0);
        assert!(theta > 296.0 && theta < 300.0);
    }

    #[test]
    fn test_potential_temperature_at_500hpa() {
        // At 500 hPa, -20C, theta should be significantly higher
        let theta = potential_temperature(500.0, -20.0);
        assert!(theta > 300.0 && theta < 320.0);
    }

    #[test]
    fn test_equivalent_potential_temperature() {
        // Theta-e should be >= theta
        let theta = potential_temperature(1000.0, 20.0);
        let theta_e = equivalent_potential_temperature(1000.0, 20.0, 15.0);
        assert!(theta_e > theta, "theta_e={theta_e} should exceed theta={theta}");
    }

    #[test]
    fn test_equivalent_potential_temperature_typical() {
        // At 1000 hPa, 25C, Td=20C, theta-e should be ~340-350K
        let theta_e = equivalent_potential_temperature(1000.0, 25.0, 20.0);
        assert!(theta_e > 335.0 && theta_e < 360.0, "theta_e={theta_e}");
    }

    #[test]
    fn test_wet_bulb_potential_temperature() {
        // Theta-w is the temperature of a saturated parcel brought to 1000 hPa
        // along a moist adiabat. It should be less than theta (dry) because
        // moist adiabats are steeper. It should be a reasonable temperature.
        let theta_w = wet_bulb_potential_temperature(1000.0, 25.0, 15.0);
        // Theta-w should be a reasonable value (250-310 K range)
        assert!(
            theta_w > 270.0 && theta_w < 310.0,
            "theta_w={theta_w} should be in reasonable range"
        );
    }

    #[test]
    fn test_virtual_potential_temperature() {
        let theta = potential_temperature(1000.0, 20.0);
        let theta_v = virtual_potential_temperature(1000.0, 20.0, 10.0);
        // Virtual potential temperature should be slightly higher than theta
        assert!(theta_v > theta);
        assert!((theta_v - theta).abs() < 5.0);
    }

    // =========================================================================
    // Lifted / Parcel Tests
    // =========================================================================

    #[test]
    fn test_lcl_pressure_saturated() {
        // When saturated, LCL should be at surface
        let p_lcl = lcl_pressure(1000.0, 20.0, 20.0);
        assert!((p_lcl - 1000.0).abs() < 1.0);
    }

    #[test]
    fn test_lcl_pressure_unsaturated() {
        // Unsaturated: LCL should be above surface (lower pressure)
        let p_lcl = lcl_pressure(1000.0, 25.0, 10.0);
        assert!(p_lcl < 1000.0);
        assert!(p_lcl > 500.0);
    }

    // Helper function to create a typical unstable sounding for profile tests
    fn make_unstable_sounding() -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        // Pressure levels from surface upward (hPa)
        let p = vec![1000.0, 925.0, 850.0, 700.0, 500.0, 400.0, 300.0, 200.0];
        // Temperature (C) - moderately unstable
        let t = vec![30.0, 22.0, 16.0, 4.0, -15.0, -28.0, -44.0, -60.0];
        // Dewpoint (C)
        let td = vec![22.0, 18.0, 12.0, -2.0, -25.0, -38.0, -54.0, -70.0];
        (p, t, td)
    }

    #[test]
    fn test_lifted_index_unstable() {
        let (p, t, td) = make_unstable_sounding();
        let li = lifted_index(&p, &t, &td);
        // Unstable sounding should have negative LI
        assert!(li < 5.0, "LI={li} should be moderately negative for unstable sounding");
    }

    #[test]
    fn test_lfc_exists_for_unstable() {
        let (p, t, td) = make_unstable_sounding();
        let result = lfc(&p, &t, &td);
        // May or may not find LFC depending on the exact profile,
        // but the function should not panic
        if let Some((p_lfc, _t_lfc)) = result {
            assert!(p_lfc < 1000.0 && p_lfc > 100.0, "LFC pressure={p_lfc} should be reasonable");
        }
    }

    #[test]
    fn test_el_exists_for_unstable() {
        let (p, t, td) = make_unstable_sounding();
        let result = el(&p, &t, &td);
        if let Some((p_el, _t_el)) = result {
            assert!(p_el < 1000.0 && p_el > 100.0, "EL pressure={p_el} should be reasonable");
        }
    }

    #[test]
    fn test_ccl_exists() {
        let (p, t, td) = make_unstable_sounding();
        let result = ccl(&p, &t, &td);
        if let Some((p_ccl, t_ccl)) = result {
            assert!(p_ccl < 1000.0 && p_ccl > 200.0, "CCL pressure={p_ccl}");
            assert!(t_ccl < 30.0, "CCL temp={t_ccl} should be below surface temp");
        }
    }

    #[test]
    fn test_convective_temperature() {
        let (p, t, td) = make_unstable_sounding();
        let t_conv = convective_temperature(&p, &t, &td);
        if t_conv != MISSING {
            // Convective temperature should be >= surface temperature
            assert!(t_conv >= t[0] - 5.0, "Tconv={t_conv} should be near or above sfc T={}", t[0]);
        }
    }

    // =========================================================================
    // Density / Height Tests
    // =========================================================================

    #[test]
    fn test_density_sea_level() {
        // Standard sea level density ~1.225 kg/m^3
        let rho = density(1013.25, 15.0, 0.0);
        assert!((rho - 1.225).abs() < 0.01, "density={rho}");
    }

    #[test]
    fn test_density_moist_less_than_dry() {
        // Moist air is less dense than dry air at same T, P
        let rho_dry = density(1000.0, 20.0, 0.0);
        let rho_moist = density(1000.0, 20.0, 15.0);
        assert!(rho_moist < rho_dry, "moist={rho_moist} should be < dry={rho_dry}");
    }

    #[test]
    fn test_virtual_temperature_from_dewpoint_matches() {
        let tv1 = virtual_temp(20.0, 1000.0, 15.0);
        let tv2 = virtual_temperature_from_dewpoint(20.0, 15.0, 1000.0);
        assert!((tv1 - tv2).abs() < 1e-10);
    }

    #[test]
    fn test_thickness_hypsometric() {
        // 1000-500 hPa thickness at 255K mean temperature should be ~5280m
        let dz = thickness_hypsometric(1000.0, 500.0, 255.0);
        assert!((dz - 5180.0).abs() < 200.0, "thickness={dz}m");
    }

    #[test]
    fn test_pressure_to_height_std_sea_level() {
        let h = pressure_to_height_std(1013.25);
        assert!(h.abs() < 1.0, "sea level height={h} should be ~0m");
    }

    #[test]
    fn test_pressure_to_height_std_500hpa() {
        let h = pressure_to_height_std(500.0);
        // 500 hPa is approximately 5500m in standard atmosphere
        assert!((h - 5574.0).abs() < 100.0, "500hPa height={h}");
    }

    #[test]
    fn test_height_to_pressure_std_sea_level() {
        let p = height_to_pressure_std(0.0);
        assert!((p - 1013.25).abs() < 0.01, "sea level pressure={p}");
    }

    #[test]
    fn test_height_to_pressure_roundtrip() {
        let p_orig = 700.0;
        let h = pressure_to_height_std(p_orig);
        let p_back = height_to_pressure_std(h);
        assert!((p_back - p_orig).abs() < 0.1, "roundtrip: {p_orig} -> {h}m -> {p_back}");
    }

    #[test]
    fn test_altimeter_to_station_pressure_sea_level() {
        // At sea level, station pressure == altimeter setting
        let p_stn = altimeter_to_station_pressure(1013.25, 0.0);
        assert!((p_stn - 1013.25).abs() < 0.1, "p_stn={p_stn}");
    }

    #[test]
    fn test_altimeter_to_station_pressure_elevated() {
        // At 1000m elevation, station pressure should be less than altimeter
        let p_stn = altimeter_to_station_pressure(1013.25, 1000.0);
        assert!(p_stn < 1013.25, "p_stn={p_stn} should be < 1013.25");
        // Should be roughly 890-940 hPa
        assert!((p_stn - 900.0).abs() < 50.0, "p_stn={p_stn}");
    }

    #[test]
    fn test_station_to_sea_level_pressure_sea_level() {
        // At sea level, SLP == station pressure
        let slp = station_to_sea_level_pressure(1013.25, 0.0, 15.0);
        assert!((slp - 1013.25).abs() < 0.1, "slp={slp}");
    }

    #[test]
    fn test_station_to_sea_level_pressure_elevated() {
        // At 500m elevation with 950 hPa station pressure, SLP should be higher
        let slp = station_to_sea_level_pressure(950.0, 500.0, 15.0);
        assert!(slp > 950.0, "slp={slp} should be > 950");
        // Should be roughly 1010 hPa
        assert!((slp - 1010.0).abs() < 15.0, "slp={slp}");
    }
}
