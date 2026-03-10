//! Rustmet benchmark binary — times download + decode of GRIB2 data.
//!
//! Usage:
//!   rustmet-bench [--iterations N] [--skip-download] [--run YYYY-MM-DD/HHz]
//!
//! Outputs JSON timing results to stdout for consumption by the comparison script.

use rustmet_core::{download, grib2};
use std::time::Instant;

/// The 5 benchmark variables — same ones used in the Python comparison
const BENCH_VARS: &[&str] = &[
    "TMP:2 m above ground",
    "DPT:2 m above ground",
    "UGRD:10 m above ground",
    "VGRD:10 m above ground",
    "CAPE:surface",
    "REFC:entire atmosphere",
    "MSLMA:mean sea level",
    "HGT:500 mb",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut iterations: u32 = 5;
    let mut skip_download = false;
    let mut run_time = String::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--iterations" | "-n" => {
                i += 1;
                iterations = args[i].parse().expect("invalid iteration count");
            }
            "--skip-download" => skip_download = true,
            "--run" => {
                i += 1;
                run_time = args[i].clone();
            }
            "--help" | "-h" => {
                eprintln!("Usage: rustmet-bench [--iterations N] [--skip-download] [--run YYYY-MM-DD/HHz]");
                eprintln!("  --iterations N     Number of decode iterations (default: 5)");
                eprintln!("  --skip-download    Skip download timing (use cached data)");
                eprintln!("  --run TIME         Model run time (default: latest available)");
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Resolve run time
    let (date, hour) = if run_time.is_empty() {
        // Use yesterday 00z as a safe default (always available)
        let now = chrono::Utc::now();
        let yesterday = now - chrono::Duration::hours(24);
        (yesterday.format("%Y%m%d").to_string(), 0u32)
    } else {
        parse_run(&run_time)
    };

    eprintln!("rustmet-bench");
    eprintln!("  Run:        {}/{:02}z", date, hour);
    eprintln!("  Variables:  {} patterns", BENCH_VARS.len());
    eprintln!("  Iterations: {} (decode)", iterations);
    eprintln!();

    let client = download::DownloadClient::new().expect("Failed to create HTTP client");

    // Build HRRR URLs for f000
    let idx_url = format!(
        "https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{}/conus/hrrr.t{:02}z.wrfprsf00.grib2.idx",
        date, hour
    );
    let grib_url = format!(
        "https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{}/conus/hrrr.t{:02}z.wrfprsf00.grib2",
        date, hour
    );

    // ── Phase 1: Download ──────────────────────────────────────────
    let mut download_times_ms: Vec<f64> = Vec::new();
    let mut grib_bytes: Vec<u8> = Vec::new();
    let mut total_download_bytes: usize = 0;

    if !skip_download {
        // Clear cache for fair timing
        let cache = download::Cache::new();

        for iter in 0..3 {
            // Remove cached data
            cache.remove(&grib_url);

            let t0 = Instant::now();

            // Fetch index
            let idx_text = client.get_text(&idx_url)
                .expect("Failed to fetch .idx");
            let idx_entries = download::parse_idx(&idx_text);

            // Find matching entries
            let mut selected: Vec<&download::IdxEntry> = Vec::new();
            for var in BENCH_VARS {
                let matches = download::find_entries(&idx_entries, var);
                for m in matches {
                    if !selected.iter().any(|e| e.byte_offset == m.byte_offset) {
                        selected.push(m);
                    }
                }
            }

            // Compute byte ranges and download
            let ranges = download::byte_ranges(&idx_entries, &selected);
            let data = client.get_ranges(&grib_url, &ranges)
                .expect("Failed to download GRIB2 data");

            let elapsed = t0.elapsed();
            let ms = elapsed.as_secs_f64() * 1000.0;
            download_times_ms.push(ms);
            total_download_bytes = data.len();
            grib_bytes = data;

            eprintln!("  Download iter {}: {:.0}ms ({:.2} MB)", iter + 1, ms,
                total_download_bytes as f64 / 1048576.0);
        }
    } else {
        // Load from cache or do single download
        eprintln!("  Downloading once (not timed)...");
        let idx_text = client.get_text(&idx_url).expect("Failed to fetch .idx");
        let idx_entries = download::parse_idx(&idx_text);
        let mut selected: Vec<&download::IdxEntry> = Vec::new();
        for var in BENCH_VARS {
            let matches = download::find_entries(&idx_entries, var);
            for m in matches {
                if !selected.iter().any(|e| e.byte_offset == m.byte_offset) {
                    selected.push(m);
                }
            }
        }
        let ranges = download::byte_ranges(&idx_entries, &selected);
        grib_bytes = client.get_ranges(&grib_url, &ranges)
            .expect("Failed to download GRIB2 data");
        total_download_bytes = grib_bytes.len();
        eprintln!("  Downloaded {:.2} MB", total_download_bytes as f64 / 1048576.0);
    }

    // ── Phase 2: Decode ────────────────────────────────────────────
    let mut decode_times_ms: Vec<f64> = Vec::new();
    let mut num_messages = 0usize;
    let mut total_values = 0usize;

    for iter in 0..iterations {
        let t0 = Instant::now();

        // Parse GRIB2 from bytes
        let grib = grib2::Grib2File::from_bytes(&grib_bytes)
            .expect("Failed to parse GRIB2");

        // Unpack every message
        let mut values_count = 0usize;
        for msg in &grib.messages {
            let values = grib2::unpack_message(msg)
                .expect("Failed to unpack message");
            values_count += values.len();
        }

        let elapsed = t0.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        decode_times_ms.push(ms);
        num_messages = grib.messages.len();
        total_values = values_count;

        eprintln!("  Decode iter {}: {:.0}ms ({} messages, {} values)",
            iter + 1, ms, num_messages, total_values);
    }

    // ── Output JSON results ────────────────────────────────────────
    let download_median = if download_times_ms.is_empty() {
        0.0
    } else {
        median(&mut download_times_ms)
    };
    let decode_median = median(&mut decode_times_ms);

    println!("{{");
    println!("  \"tool\": \"rustmet\",");
    println!("  \"run\": \"{}/{:02}z\",", date, hour);
    println!("  \"variables\": {},", BENCH_VARS.len());
    println!("  \"download_bytes\": {},", total_download_bytes);
    println!("  \"num_messages\": {},", num_messages);
    println!("  \"total_values\": {},", total_values);
    println!("  \"download_median_ms\": {:.1},", download_median);
    println!("  \"decode_median_ms\": {:.1},", decode_median);
    println!("  \"download_times_ms\": {:?},", download_times_ms);
    println!("  \"decode_times_ms\": {:?}", decode_times_ms);
    println!("}}");
}

fn median(v: &mut Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = v.len() / 2;
    if v.len() % 2 == 0 {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    }
}

fn parse_run(s: &str) -> (String, u32) {
    // "YYYY-MM-DD/HHz" or "YYYYMMDD/HHz"
    let parts: Vec<&str> = s.split('/').collect();
    let date_str = parts[0].replace('-', "");
    let hour: u32 = parts.get(1)
        .map(|h| h.trim_end_matches('z').trim_end_matches('Z').parse().unwrap_or(0))
        .unwrap_or(0);
    (date_str, hour)
}
