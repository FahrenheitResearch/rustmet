//! Derived meteorological parameters computed from 3D model fields.
//!
//! Generic versions that accept data slices instead of WrfFile references,
//! making them usable with any data source (WRF, HRRR GRIB2, GFS, etc.).
//!
//! Uses the metfuncs module for thermodynamic calculations and rayon
//! for parallel computation across grid points.

use crate::metfuncs;
use rayon::prelude::*;

/// Physical constants
const RD: f64 = 287.058;
const G: f64 = 9.80665;
const ZEROCNK: f64 = 273.15;
const ROCP: f64 = 0.28571426;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Linearly interpolate a value at target_h from height/value profiles.
/// Profiles must be ordered with increasing height.
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
    values[values.len() - 1]
}

/// Extract a column (nz values) from a flattened 3D array [nz][ny][nx] at grid point (j, i).
fn extract_column(data: &[f64], nz: usize, ny: usize, nx: usize, j: usize, i: usize) -> Vec<f64> {
    let mut col = Vec::with_capacity(nz);
    for k in 0..nz {
        col.push(data[k * ny * nx + j * nx + i]);
    }
    col
}

/// Compute dewpoint (Celsius) from mixing ratio (kg/kg) and pressure (hPa).
pub fn dewpoint_from_q(q: f64, p_hpa: f64) -> f64 {
    let q = q.max(1.0e-10); // avoid log(0)
    let e = q * p_hpa / (0.622 + q); // vapor pressure in hPa
    let e = e.max(1.0e-10);
    let ln_e = (e / 6.112).ln();
    (243.5 * ln_e) / (17.67 - ln_e)
}

// ---------------------------------------------------------------------------
// CAPE / CIN
// ---------------------------------------------------------------------------

/// Compute CAPE/CIN for every grid point (parallelized with rayon).
///
/// All 3D arrays are flattened [nz][ny][nx]. 2D arrays are [ny][nx].
///
/// `parcel_type`: `"sb"` (surface-based), `"ml"` (mixed-layer), `"mu"` (most-unstable).
///
/// Inputs:
/// - `pressure_3d`: Full pressure in Pa, shape [nz][ny][nx]
/// - `temperature_c_3d`: Temperature in Celsius, shape [nz][ny][nx]
/// - `qvapor_3d`: Water vapor mixing ratio in kg/kg, shape [nz][ny][nx]
/// - `height_agl_3d`: Height AGL in meters, shape [nz][ny][nx]
/// - `psfc`: Surface pressure in Pa, shape [ny][nx]
/// - `t2`: 2-meter temperature in K, shape [ny][nx]
/// - `q2`: 2-meter mixing ratio in kg/kg, shape [ny][nx]
///
/// Returns `(cape_2d, cin_2d, lcl_2d, lfc_2d)` each `Vec<f64>` of size `ny * nx`.
pub fn compute_cape_cin(
    pressure_3d: &[f64],
    temperature_c_3d: &[f64],
    qvapor_3d: &[f64],
    height_agl_3d: &[f64],
    psfc: &[f64],
    t2: &[f64],
    q2: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
    parcel_type: &str,
) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let n2d = ny * nx;
    let parcel_type_owned = parcel_type.to_string();

    // Parallel computation over all grid points
    let results: Vec<(f64, f64, f64, f64)> = (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;

            // Extract column profiles
            let p_col = extract_column(pressure_3d, nz, ny, nx, j, i);
            let t_col = extract_column(temperature_c_3d, nz, ny, nx, j, i);
            let q_col = extract_column(qvapor_3d, nz, ny, nx, j, i);
            let h_col = extract_column(height_agl_3d, nz, ny, nx, j, i);

            // Convert to hPa for metfuncs (temperature already in Celsius)
            let mut p_hpa: Vec<f64> = p_col.iter().map(|&p| p / 100.0).collect();
            let t_c: Vec<f64> = t_col;
            let mut td_c: Vec<f64> = Vec::with_capacity(nz);
            for k in 0..nz {
                td_c.push(dewpoint_from_q(q_col[k], p_hpa[k]));
            }
            let mut h_agl: Vec<f64> = h_col;

            // Ensure profiles are ordered surface-to-top (decreasing pressure)
            if p_hpa.len() > 1 && p_hpa[0] < p_hpa[p_hpa.len() - 1] {
                p_hpa.reverse();
                let mut t_c = t_c;
                t_c.reverse();
                td_c.reverse();
                h_agl.reverse();
                // Surface values
                let psfc_hpa = psfc[idx] / 100.0;
                let t2m_c = t2[idx] - ZEROCNK;
                let td2m_c = dewpoint_from_q(q2[idx], psfc_hpa);
                return metfuncs::cape_cin_core(
                    &p_hpa, &t_c, &td_c, &h_agl,
                    psfc_hpa, t2m_c, td2m_c,
                    &parcel_type_owned, 100.0, 300.0, None,
                );
            }

            // Surface values
            let psfc_hpa = psfc[idx] / 100.0;
            let t2m_c = t2[idx] - ZEROCNK;
            let td2m_c = dewpoint_from_q(q2[idx], psfc_hpa);

            metfuncs::cape_cin_core(
                &p_hpa, &t_c, &td_c, &h_agl,
                psfc_hpa, t2m_c, td2m_c,
                &parcel_type_owned, 100.0, 300.0, None,
            )
        })
        .collect();

    let mut cape_2d = Vec::with_capacity(n2d);
    let mut cin_2d = Vec::with_capacity(n2d);
    let mut lcl_2d = Vec::with_capacity(n2d);
    let mut lfc_2d = Vec::with_capacity(n2d);

    for (cape, cin, lcl, lfc) in results {
        cape_2d.push(cape);
        cin_2d.push(cin);
        lcl_2d.push(lcl);
        lfc_2d.push(lfc);
    }

    (cape_2d, cin_2d, lcl_2d, lfc_2d)
}

// ---------------------------------------------------------------------------
// Storm Relative Helicity
// ---------------------------------------------------------------------------

/// Compute 0-X km Storm Relative Helicity using Bunkers storm motion.
///
/// `u_3d`, `v_3d`: Wind components in m/s, shape [nz][ny][nx]
/// `height_agl_3d`: Height AGL in meters, shape [nz][ny][nx]
/// `top_m`: height AGL in meters (typically 1000.0 or 3000.0)
pub fn compute_srh(
    u_3d: &[f64],
    v_3d: &[f64],
    height_agl_3d: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
    top_m: f64,
) -> Vec<f64> {
    let n2d = ny * nx;

    (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;

            let u_col = extract_column(u_3d, nz, ny, nx, j, i);
            let v_col = extract_column(v_3d, nz, ny, nx, j, i);
            let h_col = extract_column(height_agl_3d, nz, ny, nx, j, i);

            // Ensure ordered from surface upward
            let (h_prof, u_prof, v_prof) = if h_col.len() > 1 && h_col[0] > h_col[h_col.len() - 1] {
                let mut h = h_col;
                let mut u = u_col;
                let mut v = v_col;
                h.reverse();
                u.reverse();
                v.reverse();
                (h, u, v)
            } else {
                (h_col, u_col, v_col)
            };

            compute_srh_column(&h_prof, &u_prof, &v_prof, top_m)
        })
        .collect()
}

/// Compute SRH for a single column using Bunkers storm motion.
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

    // 1. Compute mean wind in 0-6 km layer
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

    // 2. Compute 0-6 km shear vector
    let u_sfc = u_prof[0];
    let v_sfc = v_prof[0];
    let u_6km = interp_at_height(mean_depth, heights, u_prof);
    let v_6km = interp_at_height(mean_depth, heights, v_prof);
    let shear_u = u_6km - u_sfc;
    let shear_v = v_6km - v_sfc;

    // 3. Bunkers deviation: rotate shear 90 degrees clockwise, scale to 7.5 m/s
    let shear_mag = (shear_u * shear_u + shear_v * shear_v).sqrt();
    let (dev_u, dev_v) = if shear_mag > 0.1 {
        let scale = 7.5 / shear_mag;
        // 90-degree clockwise rotation: (u, v) -> (v, -u)
        (shear_v * scale, -shear_u * scale)
    } else {
        (0.0, 0.0)
    };

    // Right-moving storm motion
    let storm_u = mean_u + dev_u;
    let storm_v = mean_v + dev_v;

    // 4. Compute SRH
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

// ---------------------------------------------------------------------------
// Bulk Wind Shear
// ---------------------------------------------------------------------------

/// Compute bulk wind shear magnitude (m/s) between `bottom_m` and `top_m` AGL.
///
/// `u_3d`, `v_3d`: Wind components in m/s, shape [nz][ny][nx]
/// `height_agl_3d`: Height AGL in meters, shape [nz][ny][nx]
pub fn compute_shear(
    u_3d: &[f64],
    v_3d: &[f64],
    height_agl_3d: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
    bottom_m: f64,
    top_m: f64,
) -> Vec<f64> {
    let n2d = ny * nx;

    (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;

            let u_col = extract_column(u_3d, nz, ny, nx, j, i);
            let v_col = extract_column(v_3d, nz, ny, nx, j, i);
            let h_col = extract_column(height_agl_3d, nz, ny, nx, j, i);

            // Ensure ordered from surface upward
            let (h_prof, u_prof, v_prof) = if h_col.len() > 1 && h_col[0] > h_col[h_col.len() - 1]
            {
                let mut h = h_col;
                let mut u = u_col;
                let mut v = v_col;
                h.reverse();
                u.reverse();
                v.reverse();
                (h, u, v)
            } else {
                (h_col, u_col, v_col)
            };

            let u_bot = interp_at_height(bottom_m, &h_prof, &u_prof);
            let v_bot = interp_at_height(bottom_m, &h_prof, &v_prof);
            let u_top = interp_at_height(top_m, &h_prof, &u_prof);
            let v_top = interp_at_height(top_m, &h_prof, &v_prof);

            let du = u_top - u_bot;
            let dv = v_top - v_bot;
            (du * du + dv * dv).sqrt()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Significant Tornado Parameter
// ---------------------------------------------------------------------------

/// Significant Tornado Parameter (STP).
///
/// STP = (CAPE/1500) * ((2000 - LCL)/1000) * (SRH_1km/150) * min(SHEAR_6km/20, 1.5)
///
/// Inputs are pre-computed 2D fields, each of size `n` (ny * nx).
pub fn compute_stp(
    cape: &[f64],
    lcl: &[f64],
    srh_1km: &[f64],
    shear_6km: &[f64],
) -> Vec<f64> {
    let n = cape.len();
    let mut stp = Vec::with_capacity(n);

    for idx in 0..n {
        let cape_term = (cape[idx] / 1500.0).max(0.0);
        let lcl_term = ((2000.0 - lcl[idx]) / 1000.0).clamp(0.0, 2.0);
        let srh_term = (srh_1km[idx] / 150.0).max(0.0);
        let shear_term = (shear_6km[idx] / 20.0).min(1.5).max(0.0);

        stp.push(cape_term * lcl_term * srh_term * shear_term);
    }

    stp
}

// ---------------------------------------------------------------------------
// Energy Helicity Index
// ---------------------------------------------------------------------------

/// Energy Helicity Index: EHI = (CAPE * SRH) / 160000
///
/// Inputs are pre-computed 2D fields.
pub fn compute_ehi(
    cape: &[f64],
    srh: &[f64],
) -> Vec<f64> {
    let n = cape.len();
    let mut ehi = Vec::with_capacity(n);

    for idx in 0..n {
        ehi.push((cape[idx] * srh[idx]) / 160000.0);
    }

    ehi
}

// ---------------------------------------------------------------------------
// Supercell Composite Parameter
// ---------------------------------------------------------------------------

/// Supercell Composite Parameter: SCP = (MUCAPE/1000) * (SRH_3km/50) * (SHEAR_6km/40)
///
/// Inputs are pre-computed 2D fields.
pub fn compute_scp(
    mucape: &[f64],
    srh_3km: &[f64],
    shear_6km: &[f64],
) -> Vec<f64> {
    let n = mucape.len();
    let mut scp = Vec::with_capacity(n);

    for idx in 0..n {
        let cape_term = (mucape[idx] / 1000.0).max(0.0);
        let srh_term = (srh_3km[idx] / 50.0).max(0.0);
        let shear_term = (shear_6km[idx] / 40.0).max(0.0);
        scp.push(cape_term * srh_term * shear_term);
    }

    scp
}

// ---------------------------------------------------------------------------
// Lapse Rate
// ---------------------------------------------------------------------------

/// Lapse rate (C/km) between two heights in km AGL.
///
/// `temperature_c_3d`: Temperature in Celsius, shape [nz][ny][nx]
/// `qvapor_3d`: Water vapor mixing ratio in kg/kg, shape [nz][ny][nx]
/// `height_agl_3d`: Height AGL in meters, shape [nz][ny][nx]
///
/// Uses virtual temperature for accuracy.
/// Positive values indicate temperature decreasing with height (conditionally unstable).
pub fn compute_lapse_rate(
    temperature_c_3d: &[f64],
    qvapor_3d: &[f64],
    height_agl_3d: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
    bottom_km: f64,
    top_km: f64,
) -> Vec<f64> {
    let n2d = ny * nx;
    let bottom_m = bottom_km * 1000.0;
    let top_m_val = top_km * 1000.0;

    (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;

            let t_col = extract_column(temperature_c_3d, nz, ny, nx, j, i);
            let q_col = extract_column(qvapor_3d, nz, ny, nx, j, i);
            let h_col = extract_column(height_agl_3d, nz, ny, nx, j, i);

            // Ensure ordered from surface upward
            let (h_prof, t_prof, q_prof) =
                if h_col.len() > 1 && h_col[0] > h_col[h_col.len() - 1] {
                    let mut h = h_col;
                    let mut t = t_col;
                    let mut q = q_col;
                    h.reverse();
                    t.reverse();
                    q.reverse();
                    (h, t, q)
                } else {
                    (h_col, t_col, q_col)
                };

            // Compute virtual temperature profile (Celsius)
            let tv_prof: Vec<f64> = (0..t_prof.len())
                .map(|k| {
                    let w = q_prof[k].max(0.0); // mixing ratio in kg/kg
                    let t_k = t_prof[k] + ZEROCNK;
                    t_k * (1.0 + 0.61 * w) - ZEROCNK // back to Celsius
                })
                .collect();

            let tv_bot = interp_at_height(bottom_m, &h_prof, &tv_prof);
            let tv_top = interp_at_height(top_m_val, &h_prof, &tv_prof);
            let dz_km = top_km - bottom_km;

            if dz_km.abs() < 0.001 {
                return 0.0;
            }

            (tv_bot - tv_top) / dz_km
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Precipitable Water
// ---------------------------------------------------------------------------

/// Precipitable water (mm).
///
/// PW = (1/g) * integral(QVAPOR * dp) from surface to top of model.
///
/// `qvapor_3d`: Mixing ratio in kg/kg, shape [nz][ny][nx]
/// `pressure_3d`: Full pressure in Pa, shape [nz][ny][nx]
pub fn compute_pw(
    qvapor_3d: &[f64],
    pressure_3d: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<f64> {
    let n2d = ny * nx;

    (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;

            let q_col = extract_column(qvapor_3d, nz, ny, nx, j, i);
            let p_col = extract_column(pressure_3d, nz, ny, nx, j, i);

            // Ensure ordered from surface (high pressure) upward (low pressure)
            let (p_prof, q_prof) = if p_col.len() > 1 && p_col[0] < p_col[p_col.len() - 1] {
                let mut p = p_col;
                let mut q = q_col;
                p.reverse();
                q.reverse();
                (p, q)
            } else {
                (p_col, q_col)
            };

            let mut pw_val = 0.0;
            for k in 0..p_prof.len() - 1 {
                let dp = (p_prof[k] - p_prof[k + 1]).abs(); // Pa
                let q_avg = 0.5 * (q_prof[k].max(0.0) + q_prof[k + 1].max(0.0));
                pw_val += q_avg * dp;
            }
            pw_val / G // kg/m^2 = mm
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Composite Reflectivity
// ---------------------------------------------------------------------------

/// Composite reflectivity (max in column) in dBZ from REFL_10CM field.
///
/// `refl_3d`: Reflectivity in dBZ, shape [nz][ny][nx]
pub fn composite_reflectivity_from_refl(
    refl_3d: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<f64> {
    let n2d = ny * nx;

    (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;
            let mut max_dbz = -999.0_f64;
            for k in 0..nz {
                let val = refl_3d[k * n2d + j * nx + i];
                if val > max_dbz {
                    max_dbz = val;
                }
            }
            max_dbz.max(-30.0)
        })
        .collect()
}

/// Composite reflectivity (max in column) in dBZ from hydrometeor mixing ratios.
/// Uses Smith (1984) empirical approximation.
///
/// All 3D fields shape [nz][ny][nx]:
/// - `pressure_3d`: Pa
/// - `temperature_c_3d`: Celsius
/// - `qrain_3d`, `qsnow_3d`, `qgraup_3d`: kg/kg
pub fn composite_reflectivity_from_hydrometeors(
    pressure_3d: &[f64],
    temperature_c_3d: &[f64],
    qrain_3d: &[f64],
    qsnow_3d: &[f64],
    qgraup_3d: &[f64],
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<f64> {
    let n2d = ny * nx;

    (0..n2d)
        .into_par_iter()
        .map(|idx| {
            let j = idx / nx;
            let i = idx % nx;
            let mut max_dbz = -999.0_f64;

            for k in 0..nz {
                let flat_idx = k * n2d + j * nx + i;
                let p = pressure_3d[flat_idx]; // Pa
                let t_k = temperature_c_3d[flat_idx] + ZEROCNK; // K

                // Air density from ideal gas law
                let rho = p / (RD * t_k);

                let qr = qrain_3d[flat_idx].max(0.0);
                let qs = qsnow_3d[flat_idx].max(0.0);
                let qg = qgraup_3d[flat_idx].max(0.0);

                // Smith (1984) reflectivity factors
                let z_rain = 3.63e9 * (rho * qr).powf(1.75);
                let z_snow = 9.80e8 * (rho * qs).powf(1.75);
                let z_graupel = 4.33e9 * (rho * qg).powf(1.75);

                let z_total = z_rain + z_snow + z_graupel;
                let dbz = if z_total > 0.0 {
                    10.0 * z_total.log10()
                } else {
                    -999.0
                };

                if dbz > max_dbz {
                    max_dbz = dbz;
                }
            }

            max_dbz.max(-30.0)
        })
        .collect()
}
