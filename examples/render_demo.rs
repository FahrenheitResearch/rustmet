// Pure-Rust rendering demo
//
// Demonstrates rustmet-core's rendering pipeline:
// - Create a synthetic 2D temperature field
// - Render it to an RGBA pixel buffer using a built-in colormap
// - Save the result as a PNG file
//
// No network access or external files needed.
// Output: render_demo_output.png in the current directory.
//
// Run with: cargo run --example render_demo

use rustmet_core::render::{render_raster, write_png, list_colormaps};

fn main() {
    println!("=== rustmet-core: Pure-Rust Rendering ===\n");

    // Grid dimensions
    let nx = 200;
    let ny = 150;

    // Create a synthetic temperature field (Celsius)
    // A warm-core low pressure system pattern
    let cx = nx as f64 / 2.0;
    let cy = ny as f64 / 2.0;
    let mut values = vec![0.0f64; nx * ny];

    for j in 0..ny {
        for i in 0..nx {
            let dx = (i as f64 - cx) / cx;
            let dy = (j as f64 - cy) / cy;
            let r = (dx * dx + dy * dy).sqrt();

            // Base temperature gradient (warm south, cool north)
            let base = 30.0 - 20.0 * (j as f64 / ny as f64);

            // Warm core anomaly
            let warm_core = 8.0 * (-r * r * 4.0).exp();

            // Some wave-like perturbation
            let wave = 3.0 * (dx * 6.0).sin() * (dy * 4.0).cos();

            values[j * nx + i] = base + warm_core + wave;
        }
    }

    // Print available colormaps
    let colormaps = list_colormaps();
    println!("Available colormaps ({}):", colormaps.len());
    for (i, name) in colormaps.iter().enumerate() {
        if i > 0 { print!(", "); }
        print!("{}", name);
    }
    println!("\n");

    // Compute data range
    let vmin = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let vmax = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    println!("Synthetic temperature field: {}x{} grid", nx, ny);
    println!("  Value range: {:.1} C to {:.1} C", vmin, vmax);

    // Render to RGBA pixels using the "temperature" colormap
    let pixels = render_raster(&values, nx, ny, "temperature", vmin, vmax);
    println!("  Rendered to {} RGBA pixels ({} bytes)", nx * ny, pixels.len());

    // Save as PNG
    let output_path = "render_demo_output.png";
    write_png(&pixels, nx as u32, ny as u32, output_path)
        .expect("Failed to write PNG");

    println!("\nSaved to: {}", output_path);
    println!("  Image size: {}x{} pixels", nx, ny);
    println!("  Colormap: temperature");
    println!("  Range: {:.1} to {:.1} C", vmin, vmax);

    println!("\nDone.");
}
