// GRIB2 fuzzy search demo
//
// Demonstrates rustmet-core's search_messages function:
// - Create a multi-message GRIB2 file with different meteorological fields
// - Use fuzzy search to find specific variables
// - Print an inventory and search results
//
// No network access or external files needed.
//
// Run with: cargo run --example search_demo

use rustmet_core::grib2::{
    Grib2File, Grib2Writer, MessageBuilder, PackingMethod,
    GridDefinition, ProductDefinition, search_messages,
    parameter_name, level_name,
};

fn main() {
    println!("=== rustmet-core: GRIB2 Fuzzy Search ===\n");

    // Define a small grid
    let nx: u32 = 5;
    let ny: u32 = 5;
    let n = (nx * ny) as usize;
    let grid = GridDefinition {
        template: 0,
        nx,
        ny,
        lat1: 35.0,
        lon1: -100.0,
        lat2: 39.0,
        lon2: -96.0,
        dx: 1.0,
        dy: 1.0,
        scan_mode: 0,
        ..GridDefinition::default()
    };

    // Build a multi-message GRIB2 file with several fields
    let fields: Vec<(u8, u8, u8, u8, f64, &str)> = vec![
        // (discipline, category, number, level_type, level_value, description)
        (0, 0, 0, 103, 2.0,     "2m Temperature"),
        (0, 0, 6, 103, 2.0,     "2m Dewpoint"),
        (0, 2, 2, 103, 10.0,    "10m U-Wind"),
        (0, 2, 3, 103, 10.0,    "10m V-Wind"),
        (0, 3, 5, 100, 500.0,   "500mb Geopotential Height"),
        (0, 3, 5, 100, 850.0,   "850mb Geopotential Height"),
        (0, 0, 0, 100, 500.0,   "500mb Temperature"),
        (0, 0, 0, 100, 850.0,   "850mb Temperature"),
        (0, 1, 1, 103, 2.0,     "2m Relative Humidity"),
        (0, 7, 6, 1, 0.0,       "CAPE"),
    ];

    let mut writer = Grib2Writer::new();
    for (disc, cat, num, ltype, lval, _desc) in &fields {
        let values: Vec<f64> = (0..n).map(|i| (i as f64) * 0.1 + *lval).collect();
        writer = writer.add_message(
            MessageBuilder::new(*disc, values)
                .grid(grid.clone())
                .product(ProductDefinition {
                    template: 0,
                    parameter_category: *cat,
                    parameter_number: *num,
                    generating_process: 2,
                    forecast_time: 0,
                    time_range_unit: 1,
                    level_type: *ltype,
                    level_value: *lval,
                })
                .packing(PackingMethod::Simple { bits_per_value: 16 }),
        );
    }

    // Encode and parse back
    let grib_bytes = writer.to_bytes().expect("Failed to encode GRIB2");
    let grib = Grib2File::from_bytes(&grib_bytes).expect("Failed to parse GRIB2");
    println!("Created GRIB2 with {} messages ({} bytes)\n", grib.messages.len(), grib_bytes.len());

    // Print full inventory
    println!("--- Full Inventory ---");
    for (i, msg) in grib.messages.iter().enumerate() {
        let name = parameter_name(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        );
        let level = level_name(msg.product.level_type);
        println!("  [{}] {} @ {} {}", i, name, msg.product.level_value, level);
    }

    // Run fuzzy searches
    let queries = vec![
        "temperature 2m",
        "wind 10m",
        "500mb height",
        "cape",
        "rh",
        "850 temperature",
        "dewpoint",
    ];

    println!("\n--- Search Results ---");
    for query in queries {
        let results = search_messages(&grib.messages, query);
        println!("\nQuery: \"{}\" -> {} result(s)", query, results.len());
        for (i, msg) in results.iter().enumerate().take(3) {
            let name = parameter_name(
                msg.discipline,
                msg.product.parameter_category,
                msg.product.parameter_number,
            );
            let level = level_name(msg.product.level_type);
            println!("  [{}] {} @ {} {}", i, name, msg.product.level_value, level);
        }
        if results.len() > 3 {
            println!("  ... and {} more", results.len() - 3);
        }
    }

    println!("\nDone.");
}
