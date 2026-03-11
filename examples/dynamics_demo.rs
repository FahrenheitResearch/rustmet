// Grid dynamics demo
//
// Demonstrates rustmet-core's 2D dynamics calculations:
// - Create synthetic u,v wind fields on a grid
// - Compute vorticity, divergence, total deformation
// - Compute wind speed and direction
// - Print statistics for each derived field
//
// No network access or external files needed.
//
// Run with: cargo run --example dynamics_demo

use rustmet_core::dynamics;

fn main() {
    println!("=== rustmet-core: Grid Dynamics Calculations ===\n");

    // Grid parameters
    let nx = 50;
    let ny = 50;
    let n = nx * ny;
    let dx = 3000.0;  // 3 km grid spacing (meters)
    let dy = 3000.0;

    println!("Grid: {}x{}, dx={:.0}m, dy={:.0}m\n", nx, ny, dx, dy);

    // Create synthetic wind fields
    // Pattern: cyclonic vortex centered on the grid + background westerly flow
    let cx = nx as f64 / 2.0;
    let cy = ny as f64 / 2.0;
    let mut u = vec![0.0f64; n];
    let mut v = vec![0.0f64; n];

    for j in 0..ny {
        for i in 0..nx {
            let x = (i as f64 - cx) * dx;
            let y = (j as f64 - cy) * dy;
            let r = (x * x + y * y).sqrt();
            let r_scale = 50000.0;  // Vortex radius of max wind (50 km)

            // Rankine-like vortex: tangential wind increases linearly inside r_scale,
            // decays as 1/r outside
            let v_tan = if r < 1.0 {
                0.0
            } else if r < r_scale {
                20.0 * r / r_scale  // Linear increase to 20 m/s
            } else {
                20.0 * r_scale / r  // 1/r decay
            };

            // Convert tangential wind to u,v (cyclonic = counterclockwise in NH)
            let cos_theta = if r > 1.0 { x / r } else { 0.0 };
            let sin_theta = if r > 1.0 { y / r } else { 0.0 };

            // Tangential: (-sin, cos) for counterclockwise rotation
            u[j * nx + i] = -v_tan * sin_theta + 10.0;  // Add 10 m/s westerly background
            v[j * nx + i] = v_tan * cos_theta;
        }
    }

    print_stats("U-wind (m/s)", &u);
    print_stats("V-wind (m/s)", &v);

    // --- Wind speed and direction ---
    let speed = dynamics::wind_speed(&u, &v);
    let direction = dynamics::wind_direction(&u, &v);
    print_stats("Wind speed (m/s)", &speed);
    print_stats("Wind direction (deg)", &direction);

    // --- Vorticity ---
    println!("\n--- Derived Dynamics Fields ---\n");

    let vort = dynamics::vorticity(&u, &v, nx, ny, dx, dy);
    print_stats("Relative vorticity (1/s)", &vort);
    // Scale to more readable units
    let vort_max = vort.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    println!("  Peak vorticity: {:.2e} 1/s ({:.1} x 10^-5)",
        vort_max, vort_max * 1e5);

    // --- Divergence ---
    let div = dynamics::divergence(&u, &v, nx, ny, dx, dy);
    print_stats("Divergence (1/s)", &div);

    // --- Deformation ---
    let stretch = dynamics::stretching_deformation(&u, &v, nx, ny, dx, dy);
    let shear = dynamics::shearing_deformation(&u, &v, nx, ny, dx, dy);
    let total_def = dynamics::total_deformation(&u, &v, nx, ny, dx, dy);
    print_stats("Stretching deformation (1/s)", &stretch);
    print_stats("Shearing deformation (1/s)", &shear);
    print_stats("Total deformation (1/s)", &total_def);

    // --- Coriolis parameter at 40N ---
    let lat = 40.0;
    let f = dynamics::coriolis_parameter(lat);
    println!("\nCoriolis parameter at {:.0}N: {:.6e} 1/s", lat, f);

    // --- Absolute vorticity ---
    // Create latitude array (40N +/- ~4 degrees for 50 grid points at 3km)
    let lats: Vec<f64> = (0..n)
        .map(|k| {
            let j = k / nx;
            let lat_offset = (j as f64 - cy) * dy / 111000.0;  // ~111 km per degree
            40.0 + lat_offset
        })
        .collect();

    let abs_vort = dynamics::absolute_vorticity(&u, &v, &lats, nx, ny, dx, dy);
    print_stats("Absolute vorticity (1/s)", &abs_vort);

    // --- Advection of a scalar (e.g., temperature) ---
    println!("\n--- Temperature Advection ---\n");

    // Create a simple temperature field: warm south, cool north
    let temp: Vec<f64> = (0..n)
        .map(|k| {
            let j = k / nx;
            300.0 - 10.0 * (j as f64 / ny as f64)  // 300K south, 290K north
        })
        .collect();

    let t_adv = dynamics::temperature_advection(&temp, &u, &v, nx, ny, dx, dy);
    print_stats("Temperature advection (K/s)", &t_adv);
    let max_adv = t_adv.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    println!("  Peak warming rate: {:.2} K/hr", max_adv * 3600.0);

    println!("\nDone.");
}

fn print_stats(name: &str, values: &[f64]) {
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    println!("{:>38}:  min={:>12.6}  max={:>12.6}  mean={:>12.6}", name, min, max, mean);
}
