use super::parser::GridDefinition;

/// Compute latitude and longitude arrays for every grid point.
/// Returns (lats, lons) with length nx*ny, stored in row-major order (j * nx + i).
pub fn grid_latlon(grid: &GridDefinition) -> (Vec<f64>, Vec<f64>) {
    let nx = grid.nx as usize;
    let ny = grid.ny as usize;
    let n = nx * ny;

    match grid.template {
        0 => latlon_grid(grid, nx, ny, n),
        30 => lambert_grid(grid, nx, ny, n),
        _ => {
            // Return empty vectors for unknown templates
            (Vec::new(), Vec::new())
        }
    }
}

/// Template 3.0: Regular latitude/longitude grid.
fn latlon_grid(grid: &GridDefinition, nx: usize, ny: usize, n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut lats = Vec::with_capacity(n);
    let mut lons = Vec::with_capacity(n);

    // Determine direction from scan_mode
    // Bit 2 (0x40): 0 = points in +i direction, 1 = -i
    // Bit 3 (0x80): 0 = points in -j direction, 1 = +j
    let dlat = if ny > 1 {
        (grid.lat2 - grid.lat1) / (ny as f64 - 1.0)
    } else {
        0.0
    };
    let dlon = if nx > 1 {
        (grid.lon2 - grid.lon1) / (nx as f64 - 1.0)
    } else {
        0.0
    };

    for j in 0..ny {
        let lat = grid.lat1 + j as f64 * dlat;
        for i in 0..nx {
            let lon = grid.lon1 + i as f64 * dlon;
            lats.push(lat);
            lons.push(lon);
        }
    }

    (lats, lons)
}

/// Template 3.30: Lambert Conformal Conic projection.
/// Inverse projection from grid (i, j) to (lat, lon).
fn lambert_grid(grid: &GridDefinition, nx: usize, ny: usize, n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut lats = Vec::with_capacity(n);
    let mut lons = Vec::with_capacity(n);

    let deg2rad = std::f64::consts::PI / 180.0;
    let rad2deg = 180.0 / std::f64::consts::PI;

    // Earth radius (6371.229 km is WMO standard)
    let r = 6_371_229.0_f64;

    let lat1_rad = grid.latin1 * deg2rad;
    let lat2_rad = grid.latin2 * deg2rad;
    let lov_rad = grid.lov * deg2rad;

    // Compute n (cone constant)
    let n = if (grid.latin1 - grid.latin2).abs() < 1.0e-6 {
        lat1_rad.sin()
    } else {
        let num = (lat1_rad.cos()).ln() - (lat2_rad.cos()).ln();
        let den = ((std::f64::consts::PI / 4.0 + lat2_rad / 2.0).tan()).ln()
            - ((std::f64::consts::PI / 4.0 + lat1_rad / 2.0).tan()).ln();
        num / den
    };

    // F factor
    let f_val = (lat1_rad.cos() * (std::f64::consts::PI / 4.0 + lat1_rad / 2.0).tan().powf(n))
        / n;

    // rho0 - distance from pole for the first grid point's latitude
    let lat1_pt_rad = grid.lat1 * deg2rad;
    let rho0 = r * f_val / (std::f64::consts::PI / 4.0 + lat1_pt_rad / 2.0).tan().powf(n);

    let lon1_rad = grid.lon1 * deg2rad;
    let theta0 = n * (lon1_rad - lov_rad);

    // Grid origin in projected coordinates
    // The first grid point (0,0) maps to (lat1, lon1)
    // x0, y0 are the projected coordinates of the first grid point
    let x0 = rho0 * theta0.sin();
    let y0 = rho0 - rho0 * theta0.cos();

    // Note: dx, dy are in meters for Lambert grids
    let dx = grid.dx;
    let dy = grid.dy;

    for j in 0..ny {
        for i in 0..nx {
            let x = x0 + i as f64 * dx;
            // y increases upward in projection space
            let y = y0 + j as f64 * dy;

            // Inverse Lambert conformal
            let rho0_full = rho0;
            let xp = x;
            let yp = rho0_full - y;
            let rho = if n > 0.0 {
                (xp * xp + yp * yp).sqrt()
            } else {
                -(xp * xp + yp * yp).sqrt()
            };

            let theta = xp.atan2(yp);

            let lat = if rho.abs() < 1.0e-10 {
                if n > 0.0 { 90.0 } else { -90.0 }
            } else {
                (2.0 * ((r * f_val / rho.abs()).powf(1.0 / n)).atan()
                    - std::f64::consts::PI / 2.0)
                    * rad2deg
            };

            let lon = (lov_rad + theta / n) * rad2deg;

            // Normalize longitude to [-180, 360)
            let lon = if lon > 360.0 {
                lon - 360.0
            } else if lon < -180.0 {
                lon + 360.0
            } else {
                lon
            };

            lats.push(lat);
            lons.push(lon);
        }
    }

    (lats, lons)
}
