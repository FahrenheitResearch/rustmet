// GRIB2 writing and parsing round-trip demo
//
// Demonstrates rustmet-core's Grib2Writer:
// - Create a synthetic temperature field
// - Encode it as GRIB2 bytes
// - Parse the bytes back with Grib2File
// - Unpack and print statistics
//
// No network access or external files needed.
//
// Run with: cargo run --example write_grib2

use rustmet_core::grib2::{
    Grib2File, Grib2Writer, MessageBuilder, PackingMethod,
    GridDefinition, ProductDefinition, unpack_message,
};

fn main() {
    println!("=== rustmet-core: GRIB2 Write/Parse Round-Trip ===\n");

    // Define a 10x10 lat/lon grid over the central US
    let nx = 10;
    let ny = 10;
    let grid = GridDefinition {
        template: 0,  // lat/lon equidistant cylindrical
        nx: nx as u32,
        ny: ny as u32,
        lat1: 30.0,
        lon1: -100.0,
        lat2: 39.0,
        lon2: -91.0,
        dx: 1.0,
        dy: 1.0,
        scan_mode: 0,
        ..GridDefinition::default()
    };

    // Create a synthetic 2m temperature field (Kelvin)
    // Gradient from warm south to cool north with some east-west variation
    let mut values = vec![0.0f64; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let lat_frac = j as f64 / (ny - 1) as f64;   // 0 (south) to 1 (north)
            let lon_frac = i as f64 / (nx - 1) as f64;   // 0 (west) to 1 (east)
            // Temperature decreases with latitude, slight east-west variation
            let t_celsius = 35.0 - 15.0 * lat_frac + 3.0 * (lon_frac * std::f64::consts::PI).sin();
            values[j * nx + i] = t_celsius + 273.15;  // Convert to Kelvin
        }
    }

    // Product definition: 2m Temperature analysis
    let product = ProductDefinition {
        template: 0,
        parameter_category: 0,  // Temperature
        parameter_number: 0,    // Temperature
        generating_process: 2,  // Forecast
        forecast_time: 0,       // Analysis (fhr 0)
        time_range_unit: 1,     // Hour
        level_type: 103,        // Height above ground
        level_value: 2.0,       // 2 meters
    };

    // Set reference time
    let ref_time = chrono::NaiveDate::from_ymd_opt(2025, 7, 15)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();

    // Build the GRIB2 file
    let writer = Grib2Writer::new().add_message(
        MessageBuilder::new(0, values.clone())  // discipline 0 = Meteorological
            .grid(grid)
            .product(product)
            .reference_time(ref_time)
            .packing(PackingMethod::Simple { bits_per_value: 16 }),
    );

    // Encode to bytes
    let grib_bytes = writer.to_bytes().expect("Failed to encode GRIB2");
    println!("Encoded GRIB2: {} bytes", grib_bytes.len());
    println!("  Magic: {:?}", std::str::from_utf8(&grib_bytes[0..4]).unwrap());
    println!("  Edition: {}", grib_bytes[7]);
    println!("  End marker: {:?}", std::str::from_utf8(&grib_bytes[grib_bytes.len()-4..]).unwrap());

    // Parse the bytes back
    let grib = Grib2File::from_bytes(&grib_bytes).expect("Failed to parse GRIB2");
    println!("\nParsed {} message(s)", grib.messages.len());

    let msg = &grib.messages[0];
    println!("  Discipline: {} (Meteorological)", msg.discipline);
    println!("  Grid: {}x{}, template {}", msg.grid.nx, msg.grid.ny, msg.grid.template);
    println!("  Product: category={}, number={} (Temperature)",
        msg.product.parameter_category, msg.product.parameter_number);
    println!("  Level: type={} (height above ground), value={} m",
        msg.product.level_type, msg.product.level_value);
    println!("  Reference time: {}", msg.reference_time);

    // Unpack the data values
    let unpacked = unpack_message(msg).expect("Failed to unpack");
    println!("\nUnpacked {} values", unpacked.len());

    // Compute statistics
    let min = unpacked.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = unpacked.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = unpacked.iter().sum::<f64>() / unpacked.len() as f64;

    println!("  Min:  {:.2} K ({:.2} C)", min, min - 273.15);
    println!("  Max:  {:.2} K ({:.2} C)", max, max - 273.15);
    println!("  Mean: {:.2} K ({:.2} C)", mean, mean - 273.15);

    // Verify round-trip accuracy
    let max_error = values.iter()
        .zip(unpacked.iter())
        .map(|(orig, unpk)| (orig - unpk).abs())
        .fold(0.0f64, f64::max);
    println!("\nRound-trip max error: {:.6} K", max_error);
    println!("  (16-bit packing quantization is expected)");

    println!("\nDone.");
}
