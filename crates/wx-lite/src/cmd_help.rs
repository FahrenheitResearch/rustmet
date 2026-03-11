// cmd_help.rs — Agent help / command discovery for wx-lite
//
// Expected bandwidth: 0B (generated locally)

use serde::Serialize;
use crate::output::print_json;

#[derive(Serialize)]
struct ArgDef {
    name: &'static str,
    #[serde(rename = "type")]
    arg_type: &'static str,
    required: bool,
    default: Option<&'static str>,
    description: &'static str,
}

#[derive(Serialize)]
struct CommandDef {
    name: &'static str,
    description: &'static str,
    bandwidth: &'static str,
    args: Vec<ArgDef>,
    example: &'static str,
}

#[derive(Serialize)]
struct HelpResponse {
    name: &'static str,
    version: &'static str,
    description: &'static str,
    output_format: &'static str,
    design_philosophy: &'static str,
    commands: Vec<CommandDef>,
}

pub fn run(pretty: bool) {
    let resp = HelpResponse {
        name: "wx-lite",
        version: env!("CARGO_PKG_VERSION"),
        description: "Bandwidth-optimized weather CLI — minimum bytes, maximum coverage. All output is valid JSON to stdout. Errors go to stderr as {\"error\": \"...\"}.",
        output_format: "JSON",
        design_philosophy: "Every byte downloaded must earn its keep. Target <500KB per command except explicit radar download. Aggressive caching, text over binary, MRMS over Level 2.",
        commands: vec![
            CommandDef {
                name: "conditions",
                description: "Current conditions — METAR only by default, --with-alerts to add alerts",
                bandwidth: "~500B without alerts, ~50KB with alerts",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                    ArgDef { name: "--with-alerts", arg_type: "flag", required: false, default: None, description: "Include active alerts (adds ~50KB)" },
                ],
                example: "wx-lite conditions --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "forecast",
                description: "NWS 7-day or hourly forecast (US only, cached /points lookup)",
                bandwidth: "~50KB (first call), ~45KB cached",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                    ArgDef { name: "--hourly", arg_type: "flag", required: false, default: None, description: "Fetch hourly forecast instead of 7-day" },
                ],
                example: "wx-lite forecast --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "alerts",
                description: "NWS weather alerts by point, state, or national feed",
                bandwidth: "~50-200KB",
                args: vec![
                    ArgDef { name: "--state", arg_type: "string", required: false, default: None, description: "State code (e.g., OK, TX)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude" },
                    ArgDef { name: "--all", arg_type: "flag", required: false, default: None, description: "Fetch ALL national alerts" },
                ],
                example: "wx-lite alerts --state OK",
            },
            CommandDef {
                name: "radar-lite",
                description: "MRMS composite reflectivity — full CONUS radar at 1/10th Level 2 bandwidth",
                bandwidth: "~1-2MB (vs ~15MB Level 2)",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude for point extraction" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude for point extraction" },
                    ArgDef { name: "--radius", arg_type: "float", required: false, default: Some("50"), description: "Search radius in km" },
                ],
                example: "wx-lite radar-lite --lat 35.2 --lon -97.4 --radius 100",
            },
            CommandDef {
                name: "metar",
                description: "Fetch current METAR observation for an ICAO station",
                bandwidth: "~500B",
                args: vec![
                    ArgDef { name: "--station", arg_type: "string", required: true, default: None, description: "ICAO station code (e.g., KOKC)" },
                    ArgDef { name: "--hours", arg_type: "int", required: false, default: Some("1"), description: "Hours to look back" },
                ],
                example: "wx-lite metar --station KOKC",
            },
            CommandDef {
                name: "global",
                description: "Open-Meteo worldwide weather — works ANYWHERE, no NWS dependency, no auth",
                bandwidth: "~3-5KB current, ~10KB with --forecast",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                    ArgDef { name: "--forecast", arg_type: "flag", required: false, default: None, description: "Include 7-day daily forecast" },
                ],
                example: "wx-lite global --lat 48.8566 --lon 2.3522",
            },
            CommandDef {
                name: "station",
                description: "Local station lookup — no network required",
                bandwidth: "0B (compiled into binary)",
                args: vec![
                    ArgDef { name: "--id", arg_type: "string", required: false, default: None, description: "ICAO station code" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude for nearby search" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude for nearby search" },
                    ArgDef { name: "--radius", arg_type: "float", required: false, default: Some("100"), description: "Search radius in km" },
                ],
                example: "wx-lite station --id KOKC",
            },
            CommandDef {
                name: "hazards",
                description: "Point-based alert categorization (fire, flood, tornado, winter, etc.)",
                bandwidth: "~50KB",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                ],
                example: "wx-lite hazards --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "severe-lite",
                description: "Lightweight severe weather check — SPC outlook only, skips MDs and watch details",
                bandwidth: "~200KB (vs ~800KB full severe)",
                args: vec![
                    ArgDef { name: "--state", arg_type: "string", required: false, default: None, description: "State code" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude" },
                ],
                example: "wx-lite severe-lite --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "brief",
                description: "Ultra-compact briefing — METAR + alert count + forecast summary in one call",
                bandwidth: "~50KB total",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                ],
                example: "wx-lite brief --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "history",
                description: "Observation history with min/max/trends for a station",
                bandwidth: "~2-5KB",
                args: vec![
                    ArgDef { name: "--station", arg_type: "string", required: true, default: None, description: "ICAO station code" },
                    ArgDef { name: "--hours", arg_type: "int", required: false, default: Some("24"), description: "Hours to look back" },
                ],
                example: "wx-lite history --station KOKC --hours 24",
            },
            CommandDef {
                name: "commands",
                description: "Show this help — describes all commands with bandwidth estimates for agent discovery",
                bandwidth: "0B",
                args: vec![],
                example: "wx-lite commands",
            },
        ],
    };
    print_json(&resp, pretty);
}
