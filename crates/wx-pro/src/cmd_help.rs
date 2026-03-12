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
    args: Vec<ArgDef>,
    example: &'static str,
}

#[derive(Serialize)]
struct HelpResponse {
    name: &'static str,
    version: &'static str,
    description: &'static str,
    output_format: &'static str,
    commands: Vec<CommandDef>,
}

pub fn run(pretty: bool) {
    let resp = HelpResponse {
        name: "wx-pro",
        version: env!("CARGO_PKG_VERSION"),
        description: "Super advanced AI meteorologist agent — full power, no bandwidth limits, maximum data. All output is valid JSON to stdout. Errors go to stderr as {\"error\": \"...\"}.",
        output_format: "JSON",
        commands: vec![
            CommandDef {
                name: "models",
                description: "List available NWP models with resolution, domain, and timing info",
                args: vec![],
                example: "wx-pro models",
            },
            CommandDef {
                name: "point",
                description: "Get model data at a geographic point for a specific variable and level",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude in degrees" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude in degrees" },
                    ArgDef { name: "--model", arg_type: "string", required: true, default: None, description: "Model ID (hrrr, gfs, nam, rap)" },
                    ArgDef { name: "--var", arg_type: "string", required: true, default: None, description: "Variable name (temperature, dewpoint, wind, cape, cin, rh, pressure, reflectivity, visibility, precipitation)" },
                    ArgDef { name: "--level", arg_type: "string", required: false, default: Some("surface"), description: "Level (surface, 500mb, 850mb, etc.)" },
                ],
                example: "wx-pro point --lat 35.2 --lon -97.4 --model hrrr --var temperature --level surface",
            },
            CommandDef {
                name: "sounding",
                description: "Get a model sounding (vertical profile) at a point with thermodynamic indices",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude in degrees" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude in degrees" },
                    ArgDef { name: "--model", arg_type: "string", required: false, default: Some("hrrr"), description: "Model ID" },
                ],
                example: "wx-pro sounding --lat 35.2 --lon -97.4 --model hrrr",
            },
            CommandDef {
                name: "severe",
                description: "Severe weather assessment — SPC outlook, watches, MDs, alerts",
                args: vec![
                    ArgDef { name: "--state", arg_type: "string", required: false, default: None, description: "US state abbreviation (e.g. OK, TX, KS)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Center latitude (use with --lon)" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Center longitude (use with --lat)" },
                    ArgDef { name: "--radius", arg_type: "float", required: false, default: Some("100"), description: "Radius in km" },
                ],
                example: "wx-pro severe --state OK",
            },
            CommandDef {
                name: "download",
                description: "Download model data from AWS/NOMADS",
                args: vec![
                    ArgDef { name: "--model", arg_type: "string", required: true, default: None, description: "Model ID (hrrr, gfs, nam, rap)" },
                    ArgDef { name: "--run", arg_type: "string", required: false, default: Some("latest"), description: "Run time (latest, 00, 06, 12, 18)" },
                    ArgDef { name: "--fhour", arg_type: "string", required: false, default: Some("0"), description: "Forecast hour" },
                    ArgDef { name: "--output", arg_type: "string", required: false, default: Some("./data"), description: "Output directory" },
                ],
                example: "wx-pro download --model hrrr --run latest --fhour 0 --output ./data/",
            },
            CommandDef {
                name: "decode",
                description: "Decode a local GRIB2 file — list messages or extract point values",
                args: vec![
                    ArgDef { name: "--file", arg_type: "string", required: true, default: None, description: "Path to GRIB2 file" },
                    ArgDef { name: "--list", arg_type: "flag", required: false, default: None, description: "List all messages in the file" },
                    ArgDef { name: "--message", arg_type: "int", required: false, default: None, description: "Message index to inspect or extract from" },
                    ArgDef { name: "--point", arg_type: "string", required: false, default: None, description: "Grid point as I,J (use with --message)" },
                ],
                example: "wx-pro decode --file data/hrrr.grib2 --list",
            },
            CommandDef {
                name: "metar",
                description: "Fetch current METAR observation(s) for an ICAO station",
                args: vec![
                    ArgDef { name: "--station", arg_type: "string", required: true, default: None, description: "ICAO station code (e.g., KOKC, KJFK)" },
                    ArgDef { name: "--hours", arg_type: "int", required: false, default: Some("1"), description: "Number of hours to look back" },
                ],
                example: "wx-pro metar --station KOKC",
            },
            CommandDef {
                name: "alerts",
                description: "Fetch active NWS weather alerts by state or lat/lon",
                args: vec![
                    ArgDef { name: "--state", arg_type: "string", required: false, default: None, description: "State code (e.g., OK, TX)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude" },
                    ArgDef { name: "--all", arg_type: "flag", required: false, default: None, description: "Fetch all national alerts" },
                ],
                example: "wx-pro alerts --state OK",
            },
            CommandDef {
                name: "station",
                description: "Look up weather station info by ICAO or find nearby stations",
                args: vec![
                    ArgDef { name: "--id", arg_type: "string", required: false, default: None, description: "ICAO station code" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude for nearby search" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude for nearby search" },
                    ArgDef { name: "--radius", arg_type: "float", required: false, default: Some("100"), description: "Search radius in km" },
                ],
                example: "wx-pro station --id KOKC",
            },
            CommandDef {
                name: "raob",
                description: "Fetch real radiosonde sounding from University of Wyoming with derived indices",
                args: vec![
                    ArgDef { name: "--station", arg_type: "string", required: false, default: None, description: "Station ID (WMO number or ICAO, e.g., OUN, 72357)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude (finds nearest RAOB site)" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude (finds nearest RAOB site)" },
                    ArgDef { name: "--hour", arg_type: "string", required: false, default: Some("12"), description: "Sounding time: 00 or 12 (Z)" },
                ],
                example: "wx-pro raob --station OUN --hour 12",
            },
            CommandDef {
                name: "forecast",
                description: "Get NWS 7-day or hourly forecast for a US location",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                    ArgDef { name: "--hourly", arg_type: "flag", required: false, default: None, description: "Fetch hourly forecast instead of 7-day" },
                ],
                example: "wx-pro forecast --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "conditions",
                description: "Unified current conditions — METAR observation + active alerts + nearest station in one call",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                ],
                example: "wx-pro conditions --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "history",
                description: "Observation history with min/max/trends — last N hours of METARs for a station",
                args: vec![
                    ArgDef { name: "--station", arg_type: "string", required: true, default: None, description: "ICAO station code" },
                    ArgDef { name: "--hours", arg_type: "int", required: false, default: Some("24"), description: "Number of hours to look back" },
                ],
                example: "wx-pro history --station KOKC --hours 24",
            },
            CommandDef {
                name: "hazards",
                description: "Unified natural hazard assessment — categorizes active threats (fire, wind, winter, flood, tornado, etc.)",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                ],
                example: "wx-pro hazards --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "radar",
                description: "Download + parse latest NEXRAD Level 2 radar volume scan — reflectivity, velocity summary",
                args: vec![
                    ArgDef { name: "--site", arg_type: "string", required: false, default: None, description: "NEXRAD site ID (e.g., KTLX, KFWS)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude (finds nearest radar)" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude (finds nearest radar)" },
                ],
                example: "wx-pro radar --site KTLX",
            },
            // === NEW wx-pro COMMANDS ===
            CommandDef {
                name: "mrms",
                description: "Download MRMS composite radar mosaic products (1km CONUS) — reflectivity, precip rate, QPE",
                args: vec![
                    ArgDef { name: "--product", arg_type: "string", required: false, default: Some("composite_refl"), description: "MRMS product (composite_refl, precip_rate, precip_flag, qpe_01h)" },
                    ArgDef { name: "--datetime", arg_type: "string", required: false, default: None, description: "Datetime YYYYMMDD-HHmmss (default: latest)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude for point extraction" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude for point extraction" },
                ],
                example: "wx-pro mrms --product composite_refl --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "rotation",
                description: "Run rotation detection algorithm on latest Level 2 radar volume — mesocyclone + TVS detection",
                args: vec![
                    ArgDef { name: "--site", arg_type: "string", required: false, default: None, description: "NEXRAD site ID (e.g., KTLX)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude (finds nearest radar)" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude (finds nearest radar)" },
                ],
                example: "wx-pro rotation --site KTLX",
            },
            CommandDef {
                name: "briefing",
                description: "Combined severe weather briefing — SPC outlook + alerts + radar + METAR in one call. The AI meteorologist command.",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Longitude" },
                ],
                example: "wx-pro briefing --lat 35.2 --lon -97.4",
            },
            CommandDef {
                name: "watch-box",
                description: "Monitor a geographic point for threshold exceedances — single-shot check of MRMS + alerts",
                args: vec![
                    ArgDef { name: "--lat", arg_type: "float", required: true, default: None, description: "Center latitude" },
                    ArgDef { name: "--lon", arg_type: "float", required: true, default: None, description: "Center longitude" },
                    ArgDef { name: "--radius-km", arg_type: "float", required: false, default: Some("50"), description: "Monitoring radius in km" },
                    ArgDef { name: "--interval-sec", arg_type: "int", required: false, default: Some("300"), description: "Check interval in seconds (future continuous mode)" },
                    ArgDef { name: "--threshold-dbz", arg_type: "float", required: false, default: Some("40"), description: "Reflectivity threshold in dBZ" },
                ],
                example: "wx-pro watch-box --lat 35.2 --lon -97.4 --threshold-dbz 40",
            },
            CommandDef {
                name: "storm-analysis",
                description: "Multi-frame storm cell tracking — SCIT-style identification, mesocyclone association, motion vectors, trend analysis",
                args: vec![
                    ArgDef { name: "--site", arg_type: "string", required: false, default: None, description: "NEXRAD site ID (e.g., KTLX)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude (finds nearest radar)" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude (finds nearest radar)" },
                    ArgDef { name: "--frames", arg_type: "int", required: false, default: Some("5"), description: "Number of radar volume scans to track (1-10)" },
                ],
                example: "wx-pro storm-analysis --site KTLX --frames 5",
            },
            CommandDef {
                name: "storm-image",
                description: "Render labeled storm cell PNG — reflectivity PPI with cell IDs, mesocyclone markers, info box, and legend",
                args: vec![
                    ArgDef { name: "--site", arg_type: "string", required: false, default: None, description: "NEXRAD site ID (e.g., KTLX)" },
                    ArgDef { name: "--lat", arg_type: "float", required: false, default: None, description: "Latitude (finds nearest radar)" },
                    ArgDef { name: "--lon", arg_type: "float", required: false, default: None, description: "Longitude (finds nearest radar)" },
                    ArgDef { name: "--size", arg_type: "int", required: false, default: Some("800"), description: "Image size in pixels" },
                ],
                example: "wx-pro storm-image --site KTLX --size 800",
            },
            CommandDef {
                name: "commands",
                description: "Show this help — describes all commands, arguments, and output format for agent discovery",
                args: vec![],
                example: "wx-pro commands",
            },
        ],
    };
    print_json(&resp, pretty);
}
