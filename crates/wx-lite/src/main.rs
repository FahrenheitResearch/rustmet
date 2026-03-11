mod output;
mod cache;
mod cmd_metar;
mod cmd_forecast;
mod cmd_alerts;
mod cmd_station;
mod cmd_conditions_lite;
mod cmd_hazards;
mod cmd_history;
mod cmd_help;
mod cmd_global;
mod cmd_radar_lite;
mod cmd_severe_lite;
mod cmd_brief;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wx-lite", about = "Bandwidth-optimized weather CLI — minimum bytes, maximum coverage")]
struct Cli {
    /// Pretty-print JSON output for human readability
    #[arg(long, global = true)]
    pretty: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Current conditions (METAR only by default, --with-alerts to add alerts) ~500B
    #[command(allow_negative_numbers = true)]
    Conditions {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        /// Include active alerts (adds ~50KB bandwidth)
        #[arg(long)]
        with_alerts: bool,
    },

    /// NWS 7-day or hourly forecast (US only) ~50KB
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

    /// NWS weather alerts by point or state ~50-200KB
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

    /// MRMS composite reflectivity (~2MB vs 15MB Level 2)
    #[command(name = "radar-lite", allow_negative_numbers = true)]
    RadarLite {
        /// Latitude for point extraction
        #[arg(long)]
        lat: Option<f64>,
        /// Longitude for point extraction
        #[arg(long)]
        lon: Option<f64>,
        /// Search radius in km (default 50)
        #[arg(long, default_value = "50")]
        radius: f64,
    },

    /// Fetch current METAR observation ~500B
    Metar {
        /// ICAO station code (e.g., KOKC, KJFK)
        #[arg(long)]
        station: String,
        /// Number of hours to look back
        #[arg(long, default_value = "1")]
        hours: u32,
    },

    /// Open-Meteo global weather (works ANYWHERE, no NWS dependency) ~5KB
    #[command(allow_negative_numbers = true)]
    Global {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        /// Include 7-day daily forecast (~10KB total)
        #[arg(long)]
        forecast: bool,
    },

    /// Local station lookup (no network) ~0B
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

    /// Point-based alert categorization ~50KB
    #[command(allow_negative_numbers = true)]
    Hazards {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },

    /// SPC outlook only — lightweight severe check ~200KB
    #[command(name = "severe-lite", allow_negative_numbers = true)]
    SevereLite {
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        lat: Option<f64>,
        #[arg(long)]
        lon: Option<f64>,
    },

    /// Ultra-compact briefing: METAR + alert count + forecast summary ~50KB
    #[command(allow_negative_numbers = true)]
    Brief {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },

    /// Observation history with trends
    History {
        /// ICAO station code
        #[arg(long)]
        station: String,
        /// Number of hours to look back
        #[arg(long, default_value = "24")]
        hours: u32,
    },

    /// Describe all commands for agent discovery
    #[command(name = "commands")]
    AgentHelp,
}

fn main() {
    let cli = Cli::parse();
    let pretty = cli.pretty;

    match cli.command {
        Commands::Conditions { lat, lon, with_alerts } => {
            cmd_conditions_lite::run(lat, lon, with_alerts, pretty);
        }
        Commands::Forecast { lat, lon, hourly } => {
            cmd_forecast::run(lat, lon, hourly, pretty);
        }
        Commands::Alerts { state, lat, lon, all } => {
            cmd_alerts::run(state.as_deref(), lat, lon, all, pretty);
        }
        Commands::RadarLite { lat, lon, radius } => {
            cmd_radar_lite::run(lat, lon, radius, pretty);
        }
        Commands::Metar { station, hours } => {
            cmd_metar::run(&station, hours, pretty);
        }
        Commands::Global { lat, lon, forecast } => {
            cmd_global::run(lat, lon, forecast, pretty);
        }
        Commands::Station { id, lat, lon, radius } => {
            cmd_station::run(&id, lat, lon, radius, pretty);
        }
        Commands::Hazards { lat, lon } => {
            cmd_hazards::run(lat, lon, pretty);
        }
        Commands::SevereLite { state, lat, lon } => {
            cmd_severe_lite::run(state.as_deref(), lat, lon, pretty);
        }
        Commands::Brief { lat, lon } => {
            cmd_brief::run(lat, lon, pretty);
        }
        Commands::History { station, hours } => {
            cmd_history::run(&station, hours, pretty);
        }
        Commands::AgentHelp => cmd_help::run(pretty),
    }
}
