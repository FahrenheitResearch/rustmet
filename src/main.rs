// Core library (GRIB2 parser, downloader, models, projections)
use rustmet_core::{grib2, download, models, projection};
// Keep metfuncs/composite accessible if needed
#[allow(unused_imports)]
use rustmet_core::{metfuncs, composite};

// Local modules (products re-exports core + adds rendering types)
mod products;
mod render;
mod colormaps;

use clap::{Parser, Subcommand};
use chrono::Utc;

#[derive(Parser)]
#[command(name = "rustmet", version, about = "Pure Rust weather model processor")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download and plot model data
    Plot {
        /// Model name: hrrr, gfs, nam, rap
        model: String,
        /// Run time: YYYY-MM-DD/HHz or "latest"
        #[arg(long)]
        run: String,
        /// Forecast hours: "0-18" or "0,6,12,18"
        #[arg(long, default_value = "0")]
        fhours: String,
        /// Products to plot (comma-separated)
        #[arg(long)]
        products: Option<String>,
        /// Output directory
        #[arg(long, default_value = "./plots")]
        output: String,
        /// Overwrite existing files
        #[arg(long)]
        overwrite: bool,
    },
    /// Download model data only (for caching)
    Download {
        /// Model name: hrrr, gfs, nam, rap
        model: String,
        /// Run time: YYYY-MM-DD/HHz or "latest"
        #[arg(long)]
        run: String,
        /// Forecast hours: "0-18" or "0,6,12,18"
        #[arg(long, default_value = "0")]
        fhours: String,
        /// Variable patterns to download (comma-separated, e.g. "TMP:2 m,CAPE:surface,REFC")
        #[arg(long)]
        vars: Option<String>,
    },
    /// List available products
    Products,
    /// Show info about a GRIB2 file
    Info {
        /// Path to a .grib2 file
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Plot { model, run, fhours, products, output, overwrite } => {
            cmd_plot(&model, &run, &fhours, products.as_deref(), &output, overwrite)
        }
        Commands::Download { model, run, fhours, vars } => {
            cmd_download(&model, &run, &fhours, vars.as_deref())
        }
        Commands::Products => {
            cmd_products();
            Ok(())
        }
        Commands::Info { path } => {
            cmd_info(&path)
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

// ============================================================
// Command: plot
// ============================================================

fn cmd_plot(
    model: &str,
    run: &str,
    fhours: &str,
    products_filter: Option<&str>,
    output: &str,
    overwrite: bool,
) -> Result<(), String> {
    let (date, hour) = parse_run_time(run)?;
    let hours = parse_fhours(fhours)?;

    // Resolve which products to plot
    let selected_products: Vec<&products::GribProduct> = if let Some(filter) = products_filter {
        filter
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|name| {
                products::find_grib_product(name)
                    .ok_or_else(|| format!("Unknown product: '{}'. Run 'rustmet products' to list available.", name))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        // Default products depend on model
        match model {
            "hrrr" => vec![
                "Surface Temp", "Surface Dewpoint", "CAPE", "Composite Reflectivity",
                "10m Wind", "Wind Gust", "MSLP", "Visibility",
                "500mb Height", "500mb Wind", "250mb Wind",
                "Total Cloud Cover", "PBL Height", "Precipitable Water",
                "2m RH",
            ],
            "gfs" => vec!["Surface Temp", "500mb Height", "250mb Wind"],
            "nam" => vec!["Surface Temp", "CAPE", "Composite Reflectivity"],
            "rap" => vec!["Surface Temp", "CAPE"],
            _ => vec!["Surface Temp"],
        }
        .into_iter()
        .filter_map(|name| products::find_grib_product(name))
        .collect()
    };

    println!("rustmet plot");
    println!("  Model:    {}", model.to_uppercase());
    println!("  Run:      {}/{:02}z", date, hour);
    println!("  F-hours:  {:?}", hours);
    println!("  Products: {}", selected_products.iter().map(|p| p.name).collect::<Vec<_>>().join(", "));
    println!("  Output:   {}", output);
    println!();

    // Create output directory
    std::fs::create_dir_all(output)
        .map_err(|e| format!("Failed to create output directory '{}': {}", output, e))?;

    // Initialize download client and cache
    let client = download::DownloadClient::new().map_err(|e| e.to_string())?;
    let cache = download::Cache::new();

    // Collect all GRIB2 variable patterns needed across products
    let mut all_vars: Vec<&str> = Vec::new();
    for prod in &selected_products {
        for var in prod.grib_vars {
            if !all_vars.contains(var) {
                all_vars.push(var);
            }
        }
    }

    for fh in &hours {
        println!("[F{:03}] Processing...", fh);

        // Build URLs based on model
        let (idx_url, grib_url) = model_urls(model, &date, hour, *fh)?;

        // Try to get idx file
        println!("  Fetching index: {}", idx_url);
        let idx_text = client.get_text(&idx_url)
            .map_err(|e| format!("Failed to fetch .idx for F{:03}: {}", fh, e))?;
        let idx_entries = download::parse_idx(&idx_text);
        println!("  Index has {} entries", idx_entries.len());

        // Find matching entries for our variables
        let mut selected_entries: Vec<&download::IdxEntry> = Vec::new();
        for var_pat in &all_vars {
            let matches = download::find_entries(&idx_entries, var_pat);
            if matches.is_empty() {
                eprintln!("  Warning: no .idx match for '{}'", var_pat);
            }
            for m in matches {
                if !selected_entries.iter().any(|e| e.byte_offset == m.byte_offset) {
                    selected_entries.push(m);
                }
            }
        }

        if selected_entries.is_empty() {
            eprintln!("  No matching variables found, skipping F{:03}", fh);
            continue;
        }

        // Compute byte ranges for partial download
        let ranges = download::byte_ranges(&idx_entries, &selected_entries);
        println!("  Downloading {} variable(s) via byte ranges...", selected_entries.len());

        // Download each byte range and concatenate
        let mut grib_data: Vec<u8> = Vec::new();
        for (start, end) in &ranges {
            // Check cache first
            let range_url = format!("{}#bytes={}-{}", grib_url, start, end);
            if let Some(cached) = cache.get(&range_url) {
                grib_data.extend_from_slice(&cached);
                continue;
            }

            let chunk = if *end == u64::MAX {
                // Last entry - download from start to end of file
                // Use a large end value; the server will return what's available
                client.get_range(&grib_url, *start, *start + 50_000_000)
                    .map_err(|e| e.to_string())?
            } else {
                client.get_range(&grib_url, *start, *end)
                    .map_err(|e| e.to_string())?
            };
            cache.put(&range_url, &chunk);
            grib_data.extend_from_slice(&chunk);
        }

        println!("  Downloaded {} bytes total", grib_data.len());

        // Parse GRIB2 messages from the downloaded data
        match grib2::Grib2File::from_bytes(&grib_data) {
            Ok(grib) => {
                println!("  Parsed {} GRIB2 message(s)", grib.messages.len());
                for msg in &grib.messages {
                    let name = grib2::parameter_name(
                        msg.discipline,
                        msg.product.parameter_category,
                        msg.product.parameter_number,
                    );
                    let level = grib2::level_name(msg.product.level_type);
                    println!("    {} @ {} = {:.0}", name, level, msg.product.level_value);
                }

                // Build projection from the first message's grid definition
                let proj: Box<dyn projection::Projection> = if let Some(first_msg) = grib.messages.first() {
                    match build_projection(&first_msg.grid) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("  Warning: failed to build projection: {}", e);
                            continue;
                        }
                    }
                } else {
                    eprintln!("  Warning: no messages to build projection from");
                    continue;
                };

                // Build time strings for rendering
                let init_time_str = if let Some(first_msg) = grib.messages.first() {
                    format!("Init: {} UTC", first_msg.reference_time.format("%Y-%m-%d %H:%M"))
                } else {
                    format!("Init: {}/{:02}z", date, hour)
                };

                for prod in &selected_products {
                    let out_path = format!(
                        "{}/{}_{}_f{:03}_{}.png",
                        output,
                        model,
                        date,
                        fh,
                        prod.name.to_lowercase().replace(' ', "_")
                    );

                    if !overwrite && std::path::Path::new(&out_path).exists() {
                        println!("  [skip] {} (exists)", out_path);
                        continue;
                    }

                    // Extract and render the product
                    match render_grib_product(prod, &grib, &*proj, &init_time_str, *fh) {
                        Ok(png_bytes) => {
                            render::save_png(&out_path, &png_bytes)?;
                            println!("  [done] {} -> {} ({} bytes)", prod.name, out_path, png_bytes.len());
                        }
                        Err(e) => {
                            eprintln!("  [warn] {} skipped: {}", prod.name, e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("  Warning: GRIB2 parse error: {}", e);
                eprintln!("  (This is expected until the full parser is implemented)");
            }
        }

        println!();
    }

    println!("Done.");
    Ok(())
}

// ============================================================
// Command: download
// ============================================================

fn cmd_download(
    model: &str,
    run: &str,
    fhours: &str,
    vars: Option<&str>,
) -> Result<(), String> {
    let (date, hour) = parse_run_time(run)?;
    let hours = parse_fhours(fhours)?;

    println!("rustmet download");
    println!("  Model:   {}", model.to_uppercase());
    println!("  Run:     {}/{:02}z", date, hour);
    println!("  F-hours: {:?}", hours);
    if let Some(v) = vars {
        println!("  Vars:    {}", v);
    }
    println!();

    let client = download::DownloadClient::new().map_err(|e| e.to_string())?;
    let cache = download::Cache::new();

    // Parse variable filter patterns
    let var_patterns: Option<Vec<&str>> = vars.map(|v| {
        v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect()
    });

    for fh in &hours {
        let (idx_url, grib_url) = model_urls(model, &date, hour, *fh)?;

        // Check if full file is already cached
        if cache.contains(&grib_url) {
            println!("[F{:03}] Already cached", fh);
            continue;
        }

        println!("[F{:03}] Fetching index...", fh);
        let idx_text = client.get_text(&idx_url)
            .map_err(|e| format!("Failed to fetch .idx for F{:03}: {}", fh, e))?;
        let idx_entries = download::parse_idx(&idx_text);

        if let Some(ref patterns) = var_patterns {
            // Selective download via byte ranges
            let mut selected: Vec<&download::IdxEntry> = Vec::new();
            for pat in patterns {
                let matches = download::find_entries(&idx_entries, pat);
                for m in matches {
                    if !selected.iter().any(|e| e.byte_offset == m.byte_offset) {
                        selected.push(m);
                    }
                }
            }

            if selected.is_empty() {
                eprintln!("[F{:03}] No matching variables found", fh);
                continue;
            }

            let ranges = download::byte_ranges(&idx_entries, &selected);
            println!("[F{:03}] Downloading {} variable(s)...", fh, selected.len());

            let mut total_bytes: u64 = 0;
            for (i, (start, end)) in ranges.iter().enumerate() {
                let range_url = format!("{}#bytes={}-{}", grib_url, start, end);
                if cache.contains(&range_url) {
                    continue;
                }
                let chunk = if *end == u64::MAX {
                    client.get_range(&grib_url, *start, *start + 50_000_000)
                        .map_err(|e| e.to_string())?
                } else {
                    client.get_range(&grib_url, *start, *end)
                        .map_err(|e| e.to_string())?
                };
                total_bytes += chunk.len() as u64;
                cache.put(&range_url, &chunk);
                println!("  [{}/{}] {} @ {} ({} bytes)",
                    i + 1, ranges.len(),
                    selected[i].variable, selected[i].level,
                    chunk.len()
                );
            }
            println!("[F{:03}] Downloaded {} bytes", fh, total_bytes);
        } else {
            // Download full file
            println!("[F{:03}] Downloading full GRIB2 file...", fh);
            let data = client.get_bytes(&grib_url).map_err(|e| e.to_string())?;
            println!("[F{:03}] Downloaded {} bytes", fh, data.len());
            cache.put(&grib_url, &data);
        }
    }

    println!("Done. Files cached in {:?}", cache.dir());
    Ok(())
}

// ============================================================
// Command: products
// ============================================================

fn cmd_products() {
    products::list_grib_products();
}

// ============================================================
// Command: info
// ============================================================

fn cmd_info(path: &str) -> Result<(), String> {
    println!("GRIB2 File: {}", path);

    let file_size = std::fs::metadata(path)
        .map_err(|e| format!("Cannot access '{}': {}", path, e))?
        .len();
    println!("Size: {} bytes ({:.1} MB)", file_size, file_size as f64 / 1_048_576.0);
    println!();

    let grib = grib2::Grib2File::from_path(path).map_err(|e| e.to_string())?;

    if grib.messages.is_empty() {
        println!("No messages found (parser may be stubbed).");
        return Ok(());
    }

    println!("{} message(s):", grib.messages.len());
    println!();
    println!("{:<4} {:<35} {:<30} {:>10} {:>8}",
        "#", "Parameter", "Level", "Value", "F-hour");
    println!("{}", "-".repeat(90));

    for (i, msg) in grib.messages.iter().enumerate() {
        let name = grib2::parameter_name(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        );
        let units = grib2::parameter_units(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        );
        let level = grib2::level_name(msg.product.level_type);

        let level_str = if msg.product.level_value != 0.0 {
            format!("{} ({:.0})", level, msg.product.level_value)
        } else {
            level.to_string()
        };

        println!("{:<4} {:<35} {:<30} {:>10} {:>8}",
            i + 1,
            format!("{} [{}]", name, units),
            level_str,
            format!("{:.1}", msg.product.level_value),
            format!("F{:03}", msg.product.forecast_time),
        );
    }

    // Grid info from first message
    if let Some(msg) = grib.messages.first() {
        println!();
        println!("Grid: {}x{}, template {}",
            msg.grid.nx, msg.grid.ny, msg.grid.template);
        println!("  SW corner: ({:.4}, {:.4})", msg.grid.lat1, msg.grid.lon1);
        println!("  NE corner: ({:.4}, {:.4})", msg.grid.lat2, msg.grid.lon2);
        println!("  dx={:.4}, dy={:.4}", msg.grid.dx, msg.grid.dy);
    }

    Ok(())
}

// ============================================================
// Helper: parse run time
// ============================================================

fn parse_run_time(run: &str) -> Result<(String, u32), String> {
    if run.eq_ignore_ascii_case("latest") {
        // Compute most recent model run from current UTC time
        // Models typically available ~2 hours after init time
        let now = Utc::now();
        let available_hour = if now.format("%H").to_string().parse::<u32>().unwrap_or(0) >= 2 {
            now.format("%H").to_string().parse::<u32>().unwrap_or(0) - 2
        } else {
            // Roll back to previous day's late run
            22
        };
        // Round down to nearest model run (hourly for HRRR, 6-hourly for GFS)
        // For simplicity, just use the available hour directly (works for hourly models)
        let date = if available_hour > 20 && now.format("%H").to_string().parse::<u32>().unwrap_or(0) < 2 {
            (now - chrono::Duration::days(1)).format("%Y%m%d").to_string()
        } else {
            now.format("%Y%m%d").to_string()
        };
        Ok((date, available_hour))
    } else if run.contains('/') {
        // Format: YYYY-MM-DD/HHz
        let parts: Vec<&str> = run.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid run time format: '{}'. Expected YYYY-MM-DD/HHz or 'latest'", run));
        }
        let date = parts[0].replace('-', "");
        if date.len() != 8 {
            return Err(format!("Invalid date in run time: '{}'. Expected YYYY-MM-DD", parts[0]));
        }
        let hour_str = parts[1].trim_end_matches('z').trim_end_matches('Z');
        let hour: u32 = hour_str.parse()
            .map_err(|_| format!("Invalid hour in run time: '{}'. Expected e.g. '12z'", parts[1]))?;
        if hour > 23 {
            return Err(format!("Hour must be 0-23, got {}", hour));
        }
        Ok((date, hour))
    } else {
        Err(format!("Invalid run time: '{}'. Expected YYYY-MM-DD/HHz or 'latest'", run))
    }
}

// ============================================================
// Helper: parse forecast hours
// ============================================================

fn parse_fhours(s: &str) -> Result<Vec<u32>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(vec![0]);
    }

    if s.contains('-') {
        // Range: "0-18"
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid forecast hour range: '{}'. Expected START-END", s));
        }
        let start: u32 = parts[0].parse()
            .map_err(|_| format!("Invalid start hour: '{}'", parts[0]))?;
        let end: u32 = parts[1].parse()
            .map_err(|_| format!("Invalid end hour: '{}'", parts[1]))?;
        if end < start {
            return Err(format!("End hour ({}) must be >= start hour ({})", end, start));
        }
        Ok((start..=end).collect())
    } else if s.contains(',') {
        // List: "0,6,12,18"
        s.split(',')
            .map(|p| {
                p.trim().parse::<u32>()
                    .map_err(|_| format!("Invalid forecast hour: '{}'", p.trim()))
            })
            .collect()
    } else {
        // Single value
        let h: u32 = s.parse()
            .map_err(|_| format!("Invalid forecast hour: '{}'", s))?;
        Ok(vec![h])
    }
}

// ============================================================
// Helper: generate model URLs
// ============================================================

fn model_urls(model: &str, date: &str, hour: u32, fhour: u32) -> Result<(String, String), String> {
    match model {
        "hrrr" => {
            let idx = models::HrrrConfig::idx_url(date, hour, "sfc", fhour);
            let grib = models::HrrrConfig::aws_url(date, hour, "sfc", fhour);
            Ok((idx, grib))
        }
        "gfs" => {
            let idx = models::GfsConfig::idx_url(date, hour, fhour);
            let grib = models::GfsConfig::aws_url(date, hour, fhour);
            Ok((idx, grib))
        }
        "nam" => {
            let idx = models::NamConfig::idx_url(date, hour, fhour);
            let grib = models::NamConfig::aws_url(date, hour, fhour);
            Ok((idx, grib))
        }
        "rap" => {
            let idx = models::RapConfig::idx_url(date, hour, fhour);
            let grib = models::RapConfig::aws_url(date, hour, fhour);
            Ok((idx, grib))
        }
        _ => Err(format!("Unknown model: '{}'. Supported: hrrr, gfs, nam, rap", model)),
    }
}

// ============================================================
// Helper: build projection from GRIB2 grid definition
// ============================================================

fn build_projection(grid: &grib2::GridDefinition) -> Result<Box<dyn projection::Projection>, String> {
    match grid.template {
        30 => {
            // Lambert Conformal Conic (HRRR, NAM, RAP)
            // GRIB2 dx/dy for Lambert are in metres (stored as unsigned int)
            // lon1 may be 0-360; normalize to -180..180
            let lon1 = if grid.lon1 > 180.0 { grid.lon1 - 360.0 } else { grid.lon1 };
            let lov = if grid.lov > 180.0 { grid.lov - 360.0 } else { grid.lov };
            Ok(Box::new(projection::LambertProjection::new(
                grid.latin1, grid.latin2, lov,
                grid.lat1, lon1,
                grid.dx, grid.dy,
                grid.nx, grid.ny,
            )))
        }
        0 => {
            // Lat/Lon Equidistant Cylindrical (GFS)
            // GFS longitudes are 0-360; normalize to -180..180
            let lon1 = if grid.lon1 > 180.0 { grid.lon1 - 360.0 } else { grid.lon1 };
            let lon2 = if grid.lon2 > 180.0 { grid.lon2 - 360.0 } else { grid.lon2 };
            Ok(Box::new(projection::LatLonProjection::new(
                grid.lat1, lon1,
                grid.lat2, lon2,
                grid.nx, grid.ny,
            )))
        }
        _ => Err(format!("Unsupported grid template: {}", grid.template)),
    }
}

// ============================================================
// Helper: render a GribProduct from parsed GRIB2 messages
// ============================================================

/// Find a GRIB2 message by matching its variable pattern string (e.g. "TMP:2 m above ground").
fn find_msg_by_var<'a>(
    grib: &'a grib2::Grib2File,
    var_pattern: &str,
) -> Option<&'a grib2::Grib2Message> {
    // Parse the VAR:level pattern
    let parts: Vec<&str> = var_pattern.splitn(2, ':').collect();
    let var_name = parts[0].trim();
    let level_hint = if parts.len() > 1 { parts[1].trim() } else { "" };

    grib.messages.iter().find(|m| {
        let name = grib2::parameter_name(m.discipline, m.product.parameter_category, m.product.parameter_number);
        let level = grib2::level_name(m.product.level_type);

        // Check if variable name matches (case-insensitive substring)
        let name_match = name.to_uppercase().contains(&var_name.to_uppercase())
            || var_name.to_uppercase() == abbrev_for_param(m.discipline, m.product.parameter_category, m.product.parameter_number);

        if !name_match {
            return false;
        }

        if level_hint.is_empty() {
            return true;
        }

        // Check if the level description matches the hint
        let level_str = format!("{} ({:.0})", level, m.product.level_value);
        let full_level = format!("{}", level_hint);
        level_str.to_lowercase().contains(&full_level.to_lowercase())
            || level.to_lowercase().contains(&full_level.to_lowercase())
            || match_level_hint(m.product.level_type, m.product.level_value, level_hint)
    })
}

/// Map GRIB2 discipline/category/number to common abbreviation (TMP, DPT, UGRD, etc.)
fn abbrev_for_param(discipline: u8, category: u8, number: u8) -> String {
    match (discipline, category, number) {
        (0, 0, 0) => "TMP",
        (0, 0, 6) => "DPT",
        (0, 1, 1) => "RH",
        (0, 2, 2) => "UGRD",
        (0, 2, 3) => "VGRD",
        (0, 2, 22) => "GUST",
        (0, 3, 0) => "PRES",
        (0, 3, 1) => "PRMSL",
        (0, 3, 198) => "MSLMA",
        (0, 3, 5) => "HGT",
        (0, 7, 6) => "CAPE",
        (0, 7, 7) => "CIN",
        (0, 7, 8) => "HLCY",
        (0, 16, 196) | (0, 16, 195) => "REFC",
        (0, 6, 1) => "TCDC",
        (0, 1, 3) => "PWAT",
        (0, 19, 0) => "VIS",
        (0, 3, 18) => "HPBL",
        _ => "",
    }.into()
}

/// Check if a message's level matches a human-readable hint like "2 m above ground" or "500 mb"
fn match_level_hint(level_type: u8, level_value: f64, hint: &str) -> bool {
    let hint_lower = hint.to_lowercase();

    // "surface" level
    if hint_lower.contains("surface") {
        return level_type == 1;
    }
    // "entire atmosphere"
    if hint_lower.contains("entire atmosphere") {
        return level_type == 10 || level_type == 200;
    }
    // "mean sea level"
    if hint_lower.contains("mean sea level") {
        return level_type == 101;
    }
    // "X m above ground" -> level_type 103
    if hint_lower.contains("above ground") {
        if let Some(height) = extract_number(&hint_lower) {
            return level_type == 103 && (level_value - height).abs() < 0.5;
        }
        return level_type == 103;
    }
    // "X mb" or "X hPa" -> isobaric surface (level_type 100), value in Pa
    if hint_lower.contains("mb") || hint_lower.contains("hpa") {
        if let Some(pressure_mb) = extract_number(&hint_lower) {
            // GRIB2 isobaric level_value is in Pa; our parser already scales it
            return level_type == 100
                && ((level_value - pressure_mb).abs() < 1.0
                    || (level_value - pressure_mb * 100.0).abs() < 50.0);
        }
    }

    false
}

fn extract_number(s: &str) -> Option<f64> {
    let mut num_str = String::new();
    let mut found_digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' || (c == '-' && !found_digit) {
            num_str.push(c);
            found_digit = true;
        } else if found_digit {
            break;
        }
    }
    num_str.parse::<f64>().ok()
}

/// Convert GribProduct definition into a render-ready Product and extract data.
fn render_grib_product(
    gprod: &products::GribProduct,
    grib: &grib2::Grib2File,
    proj: &dyn projection::Projection,
    init_time_str: &str,
    fhour: u32,
) -> Result<Vec<u8>, String> {
    let nx = proj.nx() as usize;
    let ny = proj.ny() as usize;

    // Determine valid time string
    let valid_time_str = if let Some(first_msg) = grib.messages.first() {
        let valid = first_msg.reference_time + chrono::Duration::hours(fhour as i64);
        format!("Valid: {} UTC (F{:03})", valid.format("%Y-%m-%d %H:%M"), fhour)
    } else {
        format!("F{:03}", fhour)
    };

    // Check if this is a wind speed product (needs U and V components)
    let is_wind_product = gprod.grib_vars.len() == 2
        && gprod.grib_vars.iter().any(|v| v.contains("UGRD"))
        && gprod.grib_vars.iter().any(|v| v.contains("VGRD"));

    let (values, wind_u_data, wind_v_data) = if is_wind_product {
        // Wind speed: compute from U and V
        let u_msg = find_msg_by_var(grib, gprod.grib_vars[0])
            .ok_or_else(|| format!("GRIB message not found for '{}'", gprod.grib_vars[0]))?;
        let v_msg = find_msg_by_var(grib, gprod.grib_vars[1])
            .ok_or_else(|| format!("GRIB message not found for '{}'", gprod.grib_vars[1]))?;

        let u_vals = grib2::unpack_message(u_msg).map_err(|e| e.to_string())?;
        let v_vals = grib2::unpack_message(v_msg).map_err(|e| e.to_string())?;

        // Truncate to expected grid size if needed (spatial differencing artifacts)
        let expected = nx * ny;
        let u_vals = if u_vals.len() > expected { u_vals[..expected].to_vec() } else { u_vals };
        let v_vals = if v_vals.len() > expected { v_vals[..expected].to_vec() } else { v_vals };

        // Compute wind speed in display units
        let speed: Vec<f64> = u_vals.iter().zip(v_vals.iter())
            .map(|(u, v)| {
                let spd = (u * u + v * v).sqrt();
                convert_wind(spd, gprod.units)
            })
            .collect();

        // Convert U/V from m/s to knots for wind barbs
        let u_kt: Vec<f64> = u_vals.iter().map(|v| v * 1.94384).collect();
        let v_kt: Vec<f64> = v_vals.iter().map(|v| v * 1.94384).collect();

        (speed, Some(u_kt), Some(v_kt))
    } else {
        // Single-variable product
        let msg = find_msg_by_var(grib, gprod.grib_vars[0])
            .ok_or_else(|| format!("GRIB message not found for '{}'", gprod.grib_vars[0]))?;
        let raw_values = grib2::unpack_message(msg).map_err(|e| e.to_string())?;

        // Unit conversion
        let converted = convert_values(&raw_values, gprod.name, gprod.units);
        (converted, None, None)
    };

    // Validate data dimensions - spatial differencing may produce a few extra values
    let expected = nx * ny;
    let values = if values.len() > expected {
        values[..expected].to_vec()
    } else if values.len() < expected {
        return Err(format!(
            "Data size mismatch: got {} values, expected {}x{}={}",
            values.len(), nx, ny, expected
        ));
    } else {
        values
    };


    // Build a render Product from the GribProduct
    let render_product = grib_product_to_render_product(gprod);

    // Render
    let png_bytes = render::render_plot(
        &values,
        nx,
        ny,
        &render_product,
        proj,
        init_time_str,
        &valid_time_str,
        wind_u_data.as_deref(),
        wind_v_data.as_deref(),
        None,  // contour_data overlay
        None,  // contour_interval
    );

    Ok(png_bytes)
}

/// Convert raw GRIB values to display units based on product name/units.
fn convert_values(values: &[f64], product_name: &str, _units: &str) -> Vec<f64> {
    let name_lower = product_name.to_lowercase();

    if name_lower.contains("temp") || name_lower.contains("dewpoint") {
        // K -> F
        values.iter().map(|v| (v - 273.15) * 9.0 / 5.0 + 32.0).collect()
    } else if name_lower.contains("850") && name_lower.contains("temp") {
        // K -> F
        values.iter().map(|v| (v - 273.15) * 9.0 / 5.0 + 32.0).collect()
    } else if name_lower.contains("mslp") || name_lower.contains("pressure") {
        // Pa -> hPa (mb)
        values.iter().map(|v| {
            if *v > 10000.0 { v / 100.0 } else { *v }
        }).collect()
    } else if name_lower.contains("visibility") {
        // Already in m, keep as-is
        values.to_vec()
    } else if name_lower.contains("precip") {
        // kg/m² (mm) -> inches
        values.iter().map(|v| v / 25.4).collect()
    } else {
        // No conversion needed (CAPE in J/kg, reflectivity in dBZ, etc.)
        values.to_vec()
    }
}

/// Convert wind speed from m/s to target units.
fn convert_wind(speed_ms: f64, units: &str) -> f64 {
    match units {
        "kt" | "kts" | "knots" => speed_ms * 1.94384,
        "mph" => speed_ms * 2.23694,
        "km/h" | "kph" => speed_ms * 3.6,
        _ => speed_ms, // m/s
    }
}

/// Create a render::Product from a GribProduct definition.
fn grib_product_to_render_product(gprod: &products::GribProduct) -> products::Product {
    let (cmin, cmax, cstep, cbar_step) = product_contour_params(gprod);

    // Pick a render style - reflectivity can be raster, most others contour
    let render_style = if gprod.name.to_lowercase().contains("reflectivity") {
        products::RenderStyle::FilledContour
    } else {
        products::RenderStyle::FilledContour
    };

    products::Product {
        name: gprod.name,
        product_name_fn: match gprod.name {
            "Surface Temp" => |_| "2m Temperature (\u{00b0}F)".into(),
            "Surface Dewpoint" => |_| "2m Dewpoint (\u{00b0}F)".into(),
            "CAPE" => |_| "Surface-Based CAPE (J/kg)".into(),
            "Composite Reflectivity" => |_| "Composite Reflectivity (dBZ)".into(),
            "10m Wind" => |_| "10m Wind Speed (m/s)".into(),
            "Wind Gust" => |_| "Surface Wind Gust (m/s)".into(),
            "MSLP" => |_| "Mean Sea Level Pressure (hPa)".into(),
            "500mb Height" => |_| "500mb Geopotential Height (m)".into(),
            "500mb Wind" => |_| "500mb Wind Speed (m/s)".into(),
            "250mb Wind" => |_| "250mb Wind Speed (m/s)".into(),
            "Total Cloud Cover" => |_| "Total Cloud Cover (%)".into(),
            "PBL Height" => |_| "Planetary Boundary Layer Height (m)".into(),
            "Precipitable Water" => |_| "Precipitable Water (mm)".into(),
            "Visibility" => |_| "Surface Visibility (m)".into(),
            "2m RH" => |_| "2m Relative Humidity (%)".into(),
            "Total Precip" => |_| "Total Precipitation (in)".into(),
            "Max Updraft Helicity" => |_| "Max Updraft Helicity 2-5km (m\u{00b2}/s\u{00b2})".into(),
            "850mb Temp" => |_| "850mb Temperature (\u{00b0}F)".into(),
            "700mb RH" => |_| "700mb Relative Humidity (%)".into(),
            "Surface Pressure" => |_| "Surface Pressure (hPa)".into(),
            "Lifted Index" => |_| "Surface Lifted Index (K)".into(),
            "CIN" => |_| "Convective Inhibition (J/kg)".into(),
            "Storm Motion" => |_| "Bunkers Storm Motion (m/s)".into(),
            _ => |_| "Unknown Product".into(),
        },
        data: product_data_type(gprod),
        render_style,
        contour_min: cmin,
        contour_max: cmax,
        contour_step: cstep,
        cbar_min: cmin,
        cbar_max: cmax,
        cbar_step: cbar_step,
        colormap_id: grib_colormap_to_render(gprod.colormap),
        overlays: vec![],
        custom_levels: None,
        custom_cbar_ticks: None,
    }
}

/// Map GribProduct ranges to contour parameters, accounting for unit conversions.
fn product_contour_params(gprod: &products::GribProduct) -> (f64, f64, f64, f64) {
    let name_lower = gprod.name.to_lowercase();
    if name_lower.contains("temp") || name_lower.contains("dewpoint") {
        // Display in Fahrenheit
        (-60.0, 120.0, 1.0, 10.0)
    } else if name_lower == "cape" {
        (0.0, 5000.0, 100.0, 500.0)
    } else if name_lower.contains("reflectivity") {
        (5.0, 70.0, 2.5, 5.0)
    } else if name_lower.contains("wind") && !name_lower.contains("gust") {
        (0.0, gprod.range.1, 1.0, 5.0)
    } else if name_lower.contains("gust") {
        (0.0, gprod.range.1, 1.0, 5.0)
    } else if name_lower.contains("mslp") {
        (980.0, 1040.0, 2.0, 4.0)
    } else if name_lower.contains("500mb height") {
        (4800.0, 6000.0, 30.0, 60.0)
    } else if name_lower.contains("rh") {
        (0.0, 100.0, 5.0, 10.0)
    } else if name_lower.contains("cloud") {
        (0.0, 100.0, 5.0, 10.0)
    } else if name_lower.contains("pbl") {
        (0.0, 4000.0, 100.0, 500.0)
    } else if name_lower.contains("precipitable") {
        (0.0, 70.0, 2.0, 10.0)
    } else if name_lower.contains("precip") {
        (0.0, 2.0, 0.05, 0.25)
    } else if name_lower.contains("helicity") {
        (0.0, 200.0, 10.0, 25.0)
    } else if name_lower.contains("visibility") {
        (0.0, 16000.0, 500.0, 2000.0)
    } else if name_lower.contains("850") && name_lower.contains("temp") {
        (-40.0, 100.0, 1.0, 10.0)
    } else if name_lower.contains("lifted") {
        (-10.0, 10.0, 1.0, 2.0)
    } else if name_lower.contains("cin") {
        (-300.0, 0.0, 10.0, 50.0)
    } else if name_lower.contains("pressure") && !name_lower.contains("msl") {
        (500.0, 1050.0, 5.0, 50.0)
    } else {
        // Generic: use the range from GribProduct
        let range = gprod.range.1 - gprod.range.0;
        let step = nice_step(range, 100);
        let cbar_step = nice_step(range, 10);
        (gprod.range.0, gprod.range.1, step, cbar_step)
    }
}

fn nice_step(range: f64, target_steps: usize) -> f64 {
    let raw = range / target_steps as f64;
    let magnitude = 10.0_f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

fn product_data_type(gprod: &products::GribProduct) -> products::ProductData {
    let name_lower = gprod.name.to_lowercase();
    if name_lower.contains("temp") && !name_lower.contains("dew") {
        products::ProductData::SurfaceTemperature { unit: products::TempUnit::Fahrenheit }
    } else if name_lower.contains("dewpoint") {
        products::ProductData::SurfaceDewpoint { unit: products::TempUnit::Fahrenheit }
    } else if name_lower.contains("cape") {
        products::ProductData::Cape { parcel: products::ParcelType::SB }
    } else if name_lower.contains("reflectivity") {
        products::ProductData::CompositeReflectivity
    } else if name_lower.contains("wind") {
        products::ProductData::SurfaceWindSpeed
    } else {
        // Fallback
        products::ProductData::SurfaceTemperature { unit: products::TempUnit::Fahrenheit }
    }
}

fn grib_colormap_to_render(colormap: &str) -> &'static str {
    match colormap {
        "temperature" => "temperature_f",
        "cape" => "cape",
        "reflectivity" => "reflectivity",
        "wind" => "winds_sfc",
        "rh" => "rh",
        "dewpoint_f" => "dewpoint_f",
        "precip_in" => "precip_in",
        "uh" => "uh",
        "jet" => "temperature_f",
        _ => "temperature_f",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fhours_range() {
        assert_eq!(parse_fhours("0-3").unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(parse_fhours("5-5").unwrap(), vec![5]);
    }

    #[test]
    fn test_parse_fhours_list() {
        assert_eq!(parse_fhours("0,6,12,18").unwrap(), vec![0, 6, 12, 18]);
        assert_eq!(parse_fhours("0, 24, 48").unwrap(), vec![0, 24, 48]);
    }

    #[test]
    fn test_parse_fhours_single() {
        assert_eq!(parse_fhours("0").unwrap(), vec![0]);
        assert_eq!(parse_fhours("12").unwrap(), vec![12]);
    }

    #[test]
    fn test_parse_run_time_explicit() {
        let (date, hour) = parse_run_time("2026-03-10/12z").unwrap();
        assert_eq!(date, "20260310");
        assert_eq!(hour, 12);
    }

    #[test]
    fn test_parse_run_time_explicit_uppercase() {
        let (date, hour) = parse_run_time("2026-03-10/00Z").unwrap();
        assert_eq!(date, "20260310");
        assert_eq!(hour, 0);
    }

    #[test]
    fn test_parse_run_time_latest() {
        // Just verify it doesn't error
        let (date, hour) = parse_run_time("latest").unwrap();
        assert_eq!(date.len(), 8);
        assert!(hour <= 23);
    }

    #[test]
    fn test_parse_run_time_errors() {
        assert!(parse_run_time("bad").is_err());
        assert!(parse_run_time("2026-03-10/25z").is_err());
        // "20260310/12z" is valid - no dashes is fine since it's already YYYYMMDD
        let (date, hour) = parse_run_time("20260310/12z").unwrap();
        assert_eq!(date, "20260310");
        assert_eq!(hour, 12);
    }
}
