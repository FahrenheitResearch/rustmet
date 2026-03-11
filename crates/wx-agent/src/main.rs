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

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wx", about = "Agentic weather platform — JSON API for AI agents")]
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
        Commands::AgentHelp => cmd_help::run(pretty),
    }
}
