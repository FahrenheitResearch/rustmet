mod output;
mod cmd_models;
mod cmd_point;
mod cmd_sounding;
mod cmd_severe;
mod cmd_download;
mod cmd_decode;
mod cmd_help;
mod cmd_metar;
mod cmd_alerts;
mod cmd_station;
mod cmd_raob;
mod cmd_forecast;
mod cmd_conditions;
mod cmd_history;
mod cmd_hazards;
mod cmd_radar;
mod cmd_mrms;
mod cmd_rotation;
mod cmd_briefing;
mod cmd_watch_box;
mod cmd_radar_image;
mod cmd_model_image;
mod basemap;
mod cmd_scan;
mod cmd_timeseries;
mod cmd_evidence;
mod cmd_tiles;
mod cmd_storm_analysis;
mod cmd_storm_image;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wx-pro", about = "Super advanced AI meteorologist agent — full power, no bandwidth limits, maximum data")]
struct Cli {
    /// Pretty-print JSON output for human readability
    #[arg(long, global = true)]
    pretty: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available NWP models
    Models,

    /// Fetch current METAR observation for a station
    Metar {
        /// ICAO station code (e.g., KOKC, KJFK)
        #[arg(long)]
        station: String,
        /// Number of hours to look back
        #[arg(long, default_value = "1")]
        hours: u32,
    },

    /// Fetch active NWS weather alerts
    #[command(allow_negative_numbers = true)]
    Alerts {
        /// State code (e.g., OK, TX)
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        lat: Option<f64>,
        #[arg(long)]
        lon: Option<f64>,
        /// Fetch ALL national alerts (no filter)
        #[arg(long)]
        all: bool,
    },

    /// Look up weather station info
    #[command(allow_negative_numbers = true)]
    Station {
        /// ICAO station code to look up
        #[arg(long, default_value = "")]
        id: String,
        /// Latitude for nearby search
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude for nearby search
        #[arg(long)]
        lon: Option<f64>,
        /// Search radius in km
        #[arg(long, default_value = "100")]
        radius: f64,
    },

    /// Get model data at a geographic point
    #[command(allow_negative_numbers = true)]
    Point {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        #[arg(long)]
        model: String,
        #[arg(long)]
        var: String,
        #[arg(long, default_value = "surface")]
        level: String,
    },

    /// Get a model sounding at a point
    #[command(allow_negative_numbers = true)]
    Sounding {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        #[arg(long, default_value = "hrrr")]
        model: String,
    },

    /// Severe weather assessment for a region
    #[command(allow_negative_numbers = true)]
    Severe {
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        lat: Option<f64>,
        #[arg(long)]
        lon: Option<f64>,
        #[arg(long, default_value = "100")]
        radius: f64,
    },

    /// Download model data
    Download {
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "latest")]
        run: String,
        #[arg(long, default_value = "0")]
        fhour: String,
        #[arg(long, default_value = "./data")]
        output: String,
    },

    /// Decode a local GRIB2 file
    Decode {
        #[arg(long)]
        file: String,
        #[arg(long)]
        list: bool,
        #[arg(long)]
        message: Option<usize>,
        #[arg(long)]
        point: Option<String>,
    },

    /// Fetch real radiosonde sounding from University of Wyoming
    #[command(allow_negative_numbers = true)]
    Raob {
        /// Station ID (WMO number or ICAO, e.g., OUN, 72357)
        #[arg(long, default_value = "")]
        station: String,
        /// Latitude (finds nearest RAOB site)
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude (finds nearest RAOB site)
        #[arg(long)]
        lon: Option<f64>,
        /// Sounding time: 00 or 12 (Z)
        #[arg(long, default_value = "12")]
        hour: String,
    },

    /// Get NWS forecast for a location (7-day or hourly)
    #[command(allow_negative_numbers = true)]
    Forecast {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        /// Fetch hourly forecast instead of 7-day
        #[arg(long)]
        hourly: bool,
    },

    /// Unified current conditions (METAR + alerts + station in one call)
    #[command(allow_negative_numbers = true)]
    Conditions {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },

    /// Observation history with trends (last N hours of METARs)
    History {
        /// ICAO station code
        #[arg(long)]
        station: String,
        /// Number of hours to look back
        #[arg(long, default_value = "24")]
        hours: u32,
    },

    /// Download + parse latest NEXRAD Level 2 radar volume scan
    #[command(allow_negative_numbers = true)]
    Radar {
        /// NEXRAD site ID (e.g., KTLX, KFWS)
        #[arg(long, default_value = "")]
        site: String,
        /// Latitude (finds nearest radar site)
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude (finds nearest radar site)
        #[arg(long)]
        lon: Option<f64>,
    },

    /// Unified natural hazard assessment for a location
    #[command(allow_negative_numbers = true)]
    Hazards {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },

    /// Download MRMS composite radar products
    #[command(allow_negative_numbers = true)]
    Mrms {
        /// MRMS product (composite_refl, precip_rate, precip_flag, qpe_01h)
        #[arg(long, default_value = "composite_refl")]
        product: String,
        /// Datetime in YYYYMMDD-HHmmss format (default: latest)
        #[arg(long)]
        datetime: Option<String>,
        /// Latitude for point extraction
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude for point extraction
        #[arg(long)]
        lon: Option<f64>,
    },

    /// Run rotation detection on latest Level 2 radar volume
    #[command(allow_negative_numbers = true)]
    Rotation {
        /// NEXRAD site ID (e.g., KTLX)
        #[arg(long, default_value = "")]
        site: String,
        /// Latitude (finds nearest radar site)
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude (finds nearest radar site)
        #[arg(long)]
        lon: Option<f64>,
    },

    /// Combined severe weather briefing — SPC + alerts + radar + conditions in one call
    #[command(allow_negative_numbers = true)]
    Briefing {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },

    /// Monitor a geographic box for threshold exceedances (single-shot check)
    #[command(allow_negative_numbers = true, name = "watch-box")]
    WatchBox {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        /// Monitoring radius in km
        #[arg(long, default_value = "50")]
        radius_km: f64,
        /// Check interval in seconds (for future continuous mode)
        #[arg(long, default_value = "300")]
        interval_sec: u64,
        /// Reflectivity threshold in dBZ
        #[arg(long, default_value = "40")]
        threshold_dbz: f64,
    },

    /// Render NEXRAD radar PPI to PNG image
    #[command(allow_negative_numbers = true, name = "radar-image")]
    RadarImage {
        /// NEXRAD site ID (e.g., KTLX)
        #[arg(long, default_value = "")]
        site: String,
        /// Latitude (finds nearest radar site)
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude (finds nearest radar site)
        #[arg(long)]
        lon: Option<f64>,
        /// Radar product: ref, vel, sw, zdr, rho, phi (default: ref)
        #[arg(long, default_value = "ref")]
        product: String,
        /// Image size in pixels (default: 800)
        #[arg(long, default_value = "800")]
        size: u32,
        /// Raw data layer only — no basemap, no overlays (for map tile compositing)
        #[arg(long)]
        raw: bool,
    },

    /// Render a model field (HRRR/GFS/etc) as a PNG image
    #[command(allow_negative_numbers = true, name = "model-image")]
    ModelImage {
        /// Model name (hrrr, rap, gfs, nam)
        #[arg(long, default_value = "hrrr")]
        model: String,
        /// Variable (cape, refc, temp, dewpoint, rh, helicity, uh, gust, wind_u, precip, etc.)
        #[arg(long)]
        var: String,
        /// Level (surface, 2m, 10m, 500mb, 0-3km, etc.)
        #[arg(long, default_value = "surface")]
        level: String,
        /// Forecast hour
        #[arg(long, default_value = "0")]
        fhour: u32,
        /// Raw data layer only — no basemap, no overlays (for map tile compositing)
        #[arg(long)]
        raw: bool,
    },

    /// Time series of a model variable at a point (forecast evolution + event detection)
    #[command(allow_negative_numbers = true)]
    Timeseries {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        /// Variable (cape, refc, temp, gust, rh, etc.)
        #[arg(long)]
        var: String,
        /// Level (surface, 2m, 10m, 500mb, etc.)
        #[arg(long, default_value = "surface")]
        level: String,
        /// Model (hrrr, rap, gfs, nam)
        #[arg(long, default_value = "hrrr")]
        model: String,
        /// Number of forecast hours
        #[arg(long, default_value = "18")]
        hours: u32,
    },

    /// Multi-source weather evidence and confidence assessment
    #[command(allow_negative_numbers = true)]
    Evidence {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },

    /// Scan a model grid for extreme values (top N max/min, threshold exceedances)
    #[command(allow_negative_numbers = true)]
    Scan {
        /// Data source: hrrr, rap, gfs, nam
        #[arg(long, default_value = "hrrr")]
        source: String,
        /// Variable (cape, refc, gust, vis, temp, rh, helicity, uh, etc.)
        #[arg(long)]
        var: String,
        /// Level (surface, 2m, 10m, 500mb, etc.)
        #[arg(long, default_value = "surface")]
        level: String,
        /// Forecast hour
        #[arg(long, default_value = "0")]
        fhour: u32,
        /// Scan mode: max, min, threshold
        #[arg(long, default_value = "max")]
        mode: String,
        /// Number of results to return
        #[arg(long, default_value = "10")]
        top_n: usize,
        /// Threshold value (required for threshold mode)
        #[arg(long)]
        threshold: Option<f64>,
        /// Minimum separation between results in km
        #[arg(long, default_value = "30")]
        separation_km: f64,
        /// Bounding box: south latitude
        #[arg(long)]
        lat1: Option<f64>,
        /// Bounding box: west longitude
        #[arg(long)]
        lon1: Option<f64>,
        /// Bounding box: north latitude
        #[arg(long)]
        lat2: Option<f64>,
        /// Bounding box: east longitude
        #[arg(long)]
        lon2: Option<f64>,
    },

    /// Generate XYZ map tiles (256x256 transparent PNGs) from model data for web maps
    #[command(allow_negative_numbers = true)]
    Tiles {
        /// Model name (hrrr, rap, gfs, nam)
        #[arg(long, default_value = "hrrr")]
        model: String,
        /// Variable (cape, refc, temp, dewpoint, rh, helicity, uh, gust, wind_u, precip, etc.)
        #[arg(long)]
        var: String,
        /// Level (surface, 2m, 10m, 500mb, 0-3km, etc.)
        #[arg(long, default_value = "surface")]
        level: String,
        /// Forecast hour
        #[arg(long, default_value = "0")]
        fhour: u32,
        /// Zoom level (0-18)
        #[arg(long)]
        z: u32,
        /// Tile X coordinate (omit with y for tile set mode)
        #[arg(long)]
        x: Option<u32>,
        /// Tile Y coordinate (omit with x for tile set mode)
        #[arg(long)]
        y: Option<u32>,
    },

    /// Storm cell analysis with cell identification, mesocyclone detection, and multi-frame tracking
    #[command(allow_negative_numbers = true, name = "storm-analysis")]
    StormAnalysis {
        /// NEXRAD site ID (e.g., KTLX)
        #[arg(long, default_value = "")]
        site: String,
        /// Latitude (finds nearest radar site)
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude (finds nearest radar site)
        #[arg(long)]
        lon: Option<f64>,
        /// Number of frames to analyze (1-10, default: 3)
        #[arg(long, default_value = "3")]
        frames: usize,
    },

    /// Render storm cell analysis as a labeled PNG image (reflectivity + cell IDs + meso markers)
    #[command(allow_negative_numbers = true, name = "storm-image")]
    StormImage {
        /// NEXRAD site ID (e.g., KTLX)
        #[arg(long, default_value = "")]
        site: String,
        /// Latitude (finds nearest radar site)
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude (finds nearest radar site)
        #[arg(long)]
        lon: Option<f64>,
        /// Image size in pixels (default: 800)
        #[arg(long, default_value = "800")]
        size: u32,
    },

    /// Describe all commands for agent discovery
    #[command(name = "commands")]
    AgentHelp,
}

fn main() {
    let cli = Cli::parse();
    let pretty = cli.pretty;

    match cli.command {
        Commands::Models => cmd_models::run(pretty),
        Commands::Metar { station, hours } => cmd_metar::run(&station, hours, pretty),
        Commands::Alerts { state, lat, lon, all } => cmd_alerts::run(state.as_deref(), lat, lon, all, pretty),
        Commands::Station { id, lat, lon, radius } => cmd_station::run(&id, lat, lon, radius, pretty),
        Commands::Point { lat, lon, model, var, level } => {
            cmd_point::run(lat, lon, &model, &var, &level, pretty);
        }
        Commands::Sounding { lat, lon, model } => {
            cmd_sounding::run(lat, lon, &model, pretty);
        }
        Commands::Severe { state, lat, lon, radius } => {
            cmd_severe::run(state.as_deref(), lat, lon, radius, pretty);
        }
        Commands::Download { model, run, fhour, output } => {
            cmd_download::run(&model, &run, &fhour, &output, pretty);
        }
        Commands::Decode { file, list, message, point } => {
            cmd_decode::run(&file, list, message, point.as_deref(), pretty);
        }
        Commands::Raob { station, lat, lon, hour } => {
            cmd_raob::run(&station, lat, lon, &hour, pretty);
        }
        Commands::Forecast { lat, lon, hourly } => {
            cmd_forecast::run(lat, lon, hourly, pretty);
        }
        Commands::Conditions { lat, lon } => {
            cmd_conditions::run(lat, lon, pretty);
        }
        Commands::Radar { site, lat, lon } => {
            cmd_radar::run(&site, lat, lon, pretty);
        }
        Commands::History { station, hours } => {
            cmd_history::run(&station, hours, pretty);
        }
        Commands::Hazards { lat, lon } => {
            cmd_hazards::run(lat, lon, pretty);
        }
        Commands::Mrms { product, datetime, lat, lon } => {
            cmd_mrms::run(&product, datetime.as_deref(), lat, lon, pretty);
        }
        Commands::Rotation { site, lat, lon } => {
            cmd_rotation::run(&site, lat, lon, pretty);
        }
        Commands::Briefing { lat, lon } => {
            cmd_briefing::run(lat, lon, pretty);
        }
        Commands::WatchBox { lat, lon, radius_km, interval_sec, threshold_dbz } => {
            cmd_watch_box::run(lat, lon, radius_km, interval_sec, threshold_dbz, pretty);
        }
        Commands::RadarImage { site, lat, lon, product, size, raw } => {
            cmd_radar_image::run(&site, lat, lon, &product, size, raw, pretty);
        }
        Commands::ModelImage { model, var, level, fhour, raw } => {
            cmd_model_image::run(&model, &var, &level, fhour, raw, pretty);
        }
        Commands::Timeseries { lat, lon, var, level, model, hours } => {
            cmd_timeseries::run(lat, lon, &var, &level, &model, hours, pretty);
        }
        Commands::Evidence { lat, lon } => {
            cmd_evidence::run(lat, lon, pretty);
        }
        Commands::Scan { source, var, level, fhour, mode, top_n, threshold, separation_km, lat1, lon1, lat2, lon2 } => {
            cmd_scan::run(&source, &var, &level, fhour, &mode, top_n, threshold, separation_km, lat1, lon1, lat2, lon2, pretty);
        }
        Commands::Tiles { model, var, level, fhour, z, x, y } => {
            cmd_tiles::run(&model, &var, &level, fhour, z, x, y, pretty);
        }
        Commands::StormAnalysis { site, lat, lon, frames } => {
            cmd_storm_analysis::run(&site, lat, lon, frames, pretty);
        }
        Commands::StormImage { site, lat, lon, size } => {
            cmd_storm_image::run(&site, lat, lon, size, pretty);
        }
        Commands::AgentHelp => cmd_help::run(pretty),
    }
}
