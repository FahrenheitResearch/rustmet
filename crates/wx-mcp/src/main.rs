use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Framing {
    JsonLines,
    ContentLength,
}

fn detect_framing<R: BufRead>(reader: &mut R) -> io::Result<Option<Framing>> {
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(None);
        }

        let leading_ws = buf
            .iter()
            .take_while(|b| matches!(**b, b' ' | b'\t' | b'\r' | b'\n'))
            .count();
        if leading_ws > 0 {
            reader.consume(leading_ws);
            continue;
        }

        if buf.len() >= 15 && buf[..15].eq_ignore_ascii_case(b"Content-Length:") {
            return Ok(Some(Framing::ContentLength));
        }

        return Ok(Some(Framing::JsonLines));
    }
}

fn read_jsonl_message<R: BufRead>(reader: &mut R) -> io::Result<Option<String>> {
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if line.trim().is_empty() {
            continue;
        }
        return Ok(Some(line));
    }
}

fn read_content_length_message<R: BufRead>(reader: &mut R) -> io::Result<Option<String>> {
    let mut saw_header = false;
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            if saw_header {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF while reading MCP headers",
                ));
            }
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        saw_header = true;

        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = Some(value.trim().parse::<usize>().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "Invalid Content-Length header")
                })?);
            }
        }
    }

    let content_length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Missing Content-Length header in MCP request",
        )
    })?;

    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;

    String::from_utf8(body)
        .map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "MCP body was not valid UTF-8"))
}

fn read_request<R: BufRead>(
    reader: &mut R,
    framing: &mut Option<Framing>,
) -> io::Result<Option<Value>> {
    let mode = match framing {
        Some(mode) => *mode,
        None => match detect_framing(reader)? {
            Some(mode) => {
                *framing = Some(mode);
                mode
            }
            None => return Ok(None),
        },
    };

    let raw = match mode {
        Framing::JsonLines => read_jsonl_message(reader)?,
        Framing::ContentLength => read_content_length_message(reader)?,
    };

    match raw {
        Some(raw) => serde_json::from_str(&raw).map(Some).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Parse error: {}", e))
        }),
        None => Ok(None),
    }
}

fn write_response<W: Write>(writer: &mut W, framing: Framing, response: &Value) -> io::Result<()> {
    let payload = serde_json::to_string(response).unwrap();

    match framing {
        Framing::JsonLines => {
            writeln!(writer, "{}", payload)?;
        }
        Framing::ContentLength => {
            write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
            writer.write_all(payload.as_bytes())?;
        }
    }

    writer.flush()
}

/// Find wx-pro binary
fn find_wx_pro() -> Option<PathBuf> {
    // 1. Same directory as our binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("wx-pro.exe");
            if candidate.exists() {
                return Some(candidate);
            }
            // Also check without .exe for unix
            let candidate = dir.join("wx-pro");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    // 2. PATH lookup
    which_in_path("wx-pro")
}

/// Find wx-lite binary
fn find_wx_lite() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("wx-lite.exe");
            if candidate.exists() {
                return Some(candidate);
            }
            let candidate = dir.join("wx-lite");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    which_in_path("wx-lite")
}

/// Simple PATH lookup
fn which_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(';').chain(path_var.split(':')) {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
        let candidate_exe = PathBuf::from(dir).join(format!("{}.exe", name));
        if candidate_exe.exists() {
            return Some(candidate_exe);
        }
    }
    None
}

// ─── Tool definitions ────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "wx_metar",
                "description": "Get current aviation weather observation (METAR) for any airport worldwide. Use for quick current conditions at a specific station.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "station": {
                            "type": "string",
                            "description": "ICAO station code (e.g., KOKC, KJFK, EGLL)"
                        },
                        "hours": {
                            "type": "number",
                            "description": "Number of hours to look back (default: 1)"
                        }
                    },
                    "required": ["station"]
                }
            },
            {
                "name": "wx_forecast",
                "description": "Get NWS 7-day or hourly forecast for a US location. Use for planning ahead. US only — use wx_global for international.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        },
                        "hourly": {
                            "type": "boolean",
                            "description": "Fetch hourly forecast instead of 7-day (default: false)"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_alerts",
                "description": "Get active NWS weather alerts/warnings. Use to check for dangerous weather. Can filter by point or state.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude (use with lon for point-based alerts)"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude (use with lat for point-based alerts)"
                        },
                        "state": {
                            "type": "string",
                            "description": "State code (e.g., OK, TX) for statewide alerts"
                        }
                    }
                }
            },
            {
                "name": "wx_conditions",
                "description": "Get current conditions combining METAR + alerts + station info. Best for 'what's the weather right now?' at a US location.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_hazards",
                "description": "Categorize active weather hazards by type (tornado, wind, flood, fire, winter, etc). Use for hazard assessment at a US location.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_severe",
                "description": "Full SPC severe weather assessment including outlooks, watches, mesoscale discussions. Use for storm potential analysis.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude (use with lon for point-based)"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude (use with lat for point-based)"
                        },
                        "state": {
                            "type": "string",
                            "description": "State code for regional assessment"
                        }
                    }
                }
            },
            {
                "name": "wx_radar",
                "description": "Download and analyze latest NEXRAD radar volume scan. Use for detailed storm analysis. WARNING: Downloads 10-15MB of radar data.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "site": {
                            "type": "string",
                            "description": "NEXRAD site ID (e.g., KTLX, KFWS)"
                        },
                        "lat": {
                            "type": "number",
                            "description": "Latitude (finds nearest radar site)"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude (finds nearest radar site)"
                        }
                    }
                }
            },
            {
                "name": "wx_history",
                "description": "Get observation history with trend analysis. Use for 'has weather been changing?' or 'how has temperature trended?'",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "station": {
                            "type": "string",
                            "description": "ICAO station code (e.g., KOKC, KJFK)"
                        },
                        "hours": {
                            "type": "number",
                            "description": "Number of hours to look back (default: 24)"
                        }
                    },
                    "required": ["station"]
                }
            },
            {
                "name": "wx_station",
                "description": "Look up weather station info by ID or find nearest stations to a location. No network required for ID lookup.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "ICAO station code to look up"
                        },
                        "lat": {
                            "type": "number",
                            "description": "Latitude for nearby station search"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude for nearby station search"
                        },
                        "radius": {
                            "type": "number",
                            "description": "Search radius in km (default: 100)"
                        }
                    }
                }
            },
            {
                "name": "wx_briefing",
                "description": "Complete severe weather briefing: METAR + alerts + SPC + radar in one call. Use when comprehensive storm analysis is needed. This is the most expensive call (~15MB download).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_global",
                "description": "Get weather for ANY location on Earth via Open-Meteo. Use for international locations outside the US. Very lightweight (~5KB). No API key needed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        },
                        "forecast": {
                            "type": "boolean",
                            "description": "Include 7-day daily forecast (~10KB total, default: false)"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_brief",
                "description": "Ultra-compact weather briefing: METAR + alert count + forecast summary. Use this FIRST for basic weather questions. Only ~50KB. US locations only.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_sounding",
                "description": "Get REAL model-derived convective parameters (CAPE, CIN, shear, SRH, updraft helicity, storm motion) from HRRR or RAP at a point. Downloads actual NWP model data via GRIB2. Use for convective risk assessment, tornado potential, storm environment analysis. Downloads ~2-3MB of model data.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        },
                        "model": {
                            "type": "string",
                            "description": "NWP model to use: 'hrrr' (3km, default) or 'rap' (13km)"
                        }
                    },
                    "required": ["lat", "lon"]
                }
            },
            {
                "name": "wx_radar_image",
                "description": "Render NEXRAD radar PPI to a PNG image file. Returns the file path — read the image with your file reader. Supports reflectivity, velocity, spectrum width, ZDR, CC, KDP. Downloads ~10MB radar data, renders in ~2ms.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude (finds nearest radar site)"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude (finds nearest radar site)"
                        },
                        "site": {
                            "type": "string",
                            "description": "NEXRAD site ID (e.g., KTLX, KJKL)"
                        },
                        "product": {
                            "type": "string",
                            "description": "Radar product: ref (reflectivity), vel (velocity), sw, zdr, rho, phi. Default: ref"
                        },
                        "size": {
                            "type": "number",
                            "description": "Image size in pixels (default: 800)"
                        }
                    }
                }
            },
            {
                "name": "wx_model_image",
                "description": "Render a full HRRR/GFS/RAP/NAM model field as a PNG image. Returns file path — read the image with your file reader. Supports CAPE, reflectivity, temperature, dewpoint, wind, helicity, RH, cloud cover, precipitation, etc. Downloads ~1-3MB of model data, renders full CONUS grid.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "model": {
                            "type": "string",
                            "description": "NWP model: hrrr (default), rap, gfs, nam"
                        },
                        "var": {
                            "type": "string",
                            "description": "Variable: cape, refc (reflectivity), temp, dewpoint, rh, gust, helicity, uh (updraft helicity), wind_u, wind_v, precip, cloud, snow, vis, pwat, mslp"
                        },
                        "level": {
                            "type": "string",
                            "description": "Level: surface (default), 2m, 10m, 500mb, 850mb, 0-3km, 0-1km, mixed_layer"
                        },
                        "fhour": {
                            "type": "number",
                            "description": "Forecast hour (default: 0 = analysis)"
                        }
                    },
                    "required": ["var"]
                }
            },
            {
                "name": "wx_point",
                "description": "Get a single model variable at a point from HRRR, RAP, GFS, or NAM. Use for specific model data queries (e.g., 500mb temperature, surface CAPE). Supports friendly names (temp, cape, wind_u) and GRIB2 names (TMP, CAPE, UGRD).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude"
                        },
                        "lon": {
                            "type": "number",
                            "description": "Longitude"
                        },
                        "model": {
                            "type": "string",
                            "description": "NWP model: hrrr, rap, gfs, nam"
                        },
                        "var": {
                            "type": "string",
                            "description": "Variable name (e.g., temp, cape, wind_u, rh, height, TMP, CAPE, UGRD)"
                        },
                        "level": {
                            "type": "string",
                            "description": "Level (e.g., surface, 2m, 500mb, 850mb). Default: surface"
                        }
                    },
                    "required": ["lat", "lon", "model", "var"]
                }
            },
            {
                "name": "wx_scan",
                "description": "Scan a full model grid (HRRR/GFS/RAP/NAM) for extreme values. Returns top N maxima, minima, or all points above a threshold with lat/lon coordinates. Use to discover where weather action is: highest CAPE, strongest gusts, lowest visibility, max reflectivity, etc.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string",
                            "description": "Data source: hrrr (default), rap, gfs, nam"
                        },
                        "var": {
                            "type": "string",
                            "description": "Variable: cape, refc, gust, vis, temp, dewpoint, rh, helicity, uh, wind_u, wind_v, precip, snow, cloud"
                        },
                        "level": {
                            "type": "string",
                            "description": "Level: surface (default), 2m, 10m, 500mb, 850mb, 0-3km"
                        },
                        "fhour": {
                            "type": "number",
                            "description": "Forecast hour (default: 0)"
                        },
                        "mode": {
                            "type": "string",
                            "description": "Scan mode: max (default), min, threshold"
                        },
                        "top_n": {
                            "type": "number",
                            "description": "Number of results (default: 10)"
                        },
                        "threshold": {
                            "type": "number",
                            "description": "Threshold value for threshold mode"
                        },
                        "separation_km": {
                            "type": "number",
                            "description": "Minimum distance between results in km (default: 30)"
                        },
                        "lat1": { "type": "number", "description": "Bounding box south latitude" },
                        "lon1": { "type": "number", "description": "Bounding box west longitude" },
                        "lat2": { "type": "number", "description": "Bounding box north latitude" },
                        "lon2": { "type": "number", "description": "Bounding box east longitude" }
                    },
                    "required": ["var"]
                }
            },
            {
                "name": "wx_timeseries",
                "description": "Get time evolution of a weather variable at a point. Downloads multiple forecast hours from HRRR/GFS/etc and returns the trend with event detection (precip onset, cold front, peak instability). Use to answer 'when will it rain?', 'when does the front arrive?', 'what's the CAPE trend?'",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": { "type": "number", "description": "Latitude" },
                        "lon": { "type": "number", "description": "Longitude" },
                        "var": { "type": "string", "description": "Variable: cape, refc, temp, gust, rh, dewpoint, wind_u, wind_v, vis, precip" },
                        "level": { "type": "string", "description": "Level: surface (default), 2m, 10m, 500mb" },
                        "model": { "type": "string", "description": "Model: hrrr (default), rap, gfs, nam" },
                        "hours": { "type": "number", "description": "Number of forecast hours (default: 18)" }
                    },
                    "required": ["lat", "lon", "var"]
                }
            },
            {
                "name": "wx_evidence",
                "description": "Multi-source weather evidence and confidence assessment. Compares METAR observations, HRRR model data, and NWS alerts for a location. Shows where sources agree or conflict, data freshness, and overall confidence level. Use when you need to justify or verify a weather assessment.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": { "type": "number", "description": "Latitude" },
                        "lon": { "type": "number", "description": "Longitude" }
                    },
                    "required": ["lat", "lon"]
                }
            }
        ]
    })
}

// ─── MCP Handlers ────────────────────────────────────────────────────────────

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "wx-tools",
            "version": "0.1.0"
        }
    })
}

fn handle_tools_list() -> Value {
    tool_definitions()
}

fn handle_tools_call(params: &Value) -> Value {
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return mcp_error("Missing 'name' in tools/call params");
        }
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match tool_name {
        "wx_metar" => call_wx_metar(&arguments),
        "wx_forecast" => call_wx_forecast(&arguments),
        "wx_alerts" => call_wx_alerts(&arguments),
        "wx_conditions" => call_wx_conditions(&arguments),
        "wx_hazards" => call_wx_hazards(&arguments),
        "wx_severe" => call_wx_severe(&arguments),
        "wx_radar" => call_wx_radar(&arguments),
        "wx_history" => call_wx_history(&arguments),
        "wx_station" => call_wx_station(&arguments),
        "wx_briefing" => call_wx_briefing(&arguments),
        "wx_global" => call_wx_global(&arguments),
        "wx_brief" => call_wx_brief(&arguments),
        "wx_sounding" => call_wx_sounding(&arguments),
        "wx_radar_image" => call_wx_radar_image(&arguments),
        "wx_model_image" => call_wx_model_image(&arguments),
        "wx_point" => call_wx_point(&arguments),
        "wx_scan" => call_wx_scan(&arguments),
        "wx_timeseries" => call_wx_timeseries(&arguments),
        "wx_evidence" => call_wx_evidence(&arguments),
        _ => mcp_error(&format!("Unknown tool: {}", tool_name)),
    }
}

// ─── Tool Implementations ────────────────────────────────────────────────────

fn call_wx_metar(args: &Value) -> Value {
    let Some(station) = args.get("station").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: station");
    };
    let mut cmd_args = vec!["metar", "--station", station];
    let hours_str;
    if let Some(hours) = args.get("hours").and_then(|v| v.as_f64()) {
        hours_str = format!("{}", hours as u32);
        cmd_args.extend(["--hours", &hours_str]);
    }
    run_wx_pro(&cmd_args)
}

fn call_wx_forecast(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    let mut cmd_args = vec!["forecast", "--lat", &lat_s, "--lon", &lon_s];
    if args.get("hourly").and_then(|v| v.as_bool()).unwrap_or(false) {
        cmd_args.push("--hourly");
    }
    run_wx_pro(&cmd_args)
}

fn call_wx_alerts(args: &Value) -> Value {
    let mut cmd_args = vec!["alerts"];
    let lat_s;
    let lon_s;
    let mut extra: Vec<&str> = Vec::new();
    if let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) {
        lat_s = format!("{}", lat);
        lon_s = format!("{}", lon);
        extra.extend(["--lat", lat_s.as_str(), "--lon", lon_s.as_str()]);
    }
    if let Some(state) = args.get("state").and_then(|v| v.as_str()) {
        // state needs to be owned to live long enough
        extra.extend(["--state", state]);
    }
    cmd_args.extend(extra.iter());
    run_wx_pro(&cmd_args)
}

fn call_wx_conditions(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    run_wx_pro(&["conditions", "--lat", &lat_s, "--lon", &lon_s])
}

fn call_wx_hazards(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    run_wx_pro(&["hazards", "--lat", &lat_s, "--lon", &lon_s])
}

fn call_wx_severe(args: &Value) -> Value {
    let mut cmd_args = vec!["severe"];
    let lat_s;
    let lon_s;
    let mut extra: Vec<&str> = Vec::new();
    if let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) {
        lat_s = format!("{}", lat);
        lon_s = format!("{}", lon);
        extra.extend(["--lat", lat_s.as_str(), "--lon", lon_s.as_str()]);
    }
    if let Some(state) = args.get("state").and_then(|v| v.as_str()) {
        extra.extend(["--state", state]);
    }
    cmd_args.extend(extra.iter());
    run_wx_pro(&cmd_args)
}

fn call_wx_radar(args: &Value) -> Value {
    let mut cmd_args = vec!["radar"];
    let lat_s;
    let lon_s;
    let mut extra: Vec<&str> = Vec::new();
    if let Some(site) = args.get("site").and_then(|v| v.as_str()) {
        if !site.is_empty() {
            extra.extend(["--site", site]);
        }
    }
    if let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) {
        lat_s = format!("{}", lat);
        lon_s = format!("{}", lon);
        extra.extend(["--lat", lat_s.as_str(), "--lon", lon_s.as_str()]);
    }
    cmd_args.extend(extra.iter());
    run_wx_pro(&cmd_args)
}

fn call_wx_history(args: &Value) -> Value {
    let Some(station) = args.get("station").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: station");
    };
    let mut cmd_args = vec!["history", "--station", station];
    let hours_str;
    if let Some(hours) = args.get("hours").and_then(|v| v.as_f64()) {
        hours_str = format!("{}", hours as u32);
        cmd_args.extend(["--hours", &hours_str]);
    }
    run_wx_pro(&cmd_args)
}

fn call_wx_station(args: &Value) -> Value {
    let mut cmd_args = vec!["station"];
    let lat_s;
    let lon_s;
    let radius_s;
    let mut extra: Vec<&str> = Vec::new();
    if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
        if !id.is_empty() {
            extra.extend(["--id", id]);
        }
    }
    if let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) {
        lat_s = format!("{}", lat);
        lon_s = format!("{}", lon);
        extra.extend(["--lat", lat_s.as_str(), "--lon", lon_s.as_str()]);
    }
    if let Some(radius) = args.get("radius").and_then(|v| v.as_f64()) {
        radius_s = format!("{}", radius);
        extra.extend(["--radius", &radius_s]);
    }
    cmd_args.extend(extra.iter());
    run_wx_pro(&cmd_args)
}

fn call_wx_briefing(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    run_wx_pro(&["briefing", "--lat", &lat_s, "--lon", &lon_s])
}

fn call_wx_global(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    let mut cmd_args = vec!["global", "--lat", &*lat_s, "--lon", &*lon_s];
    if args
        .get("forecast")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        cmd_args.push("--forecast");
    }
    run_wx_lite(&cmd_args)
}

fn call_wx_brief(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    run_wx_lite(&["brief", "--lat", &lat_s, "--lon", &lon_s])
}

fn call_wx_radar_image(args: &Value) -> Value {
    let mut cmd_args = vec!["radar-image"];
    let lat_s;
    let lon_s;
    let size_s;
    let mut extra: Vec<&str> = Vec::new();
    if let Some(site) = args.get("site").and_then(|v| v.as_str()) {
        if !site.is_empty() {
            extra.extend(["--site", site]);
        }
    }
    if let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) {
        lat_s = format!("{}", lat);
        lon_s = format!("{}", lon);
        extra.extend(["--lat", lat_s.as_str(), "--lon", lon_s.as_str()]);
    }
    if let Some(product) = args.get("product").and_then(|v| v.as_str()) {
        extra.extend(["--product", product]);
    }
    if let Some(size) = args.get("size").and_then(|v| v.as_f64()) {
        size_s = format!("{}", size as u32);
        extra.extend(["--size", &size_s]);
    }
    if extra.is_empty() {
        return mcp_error("Provide site or lat/lon for radar-image");
    }
    cmd_args.extend(extra.iter());
    run_wx_pro(&cmd_args)
}

fn call_wx_sounding(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("hrrr");
    let model_s = model.to_string();
    run_wx_pro(&["sounding", "--lat", &lat_s, "--lon", &lon_s, "--model", &model_s])
}

fn call_wx_model_image(args: &Value) -> Value {
    let Some(var) = args.get("var").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: var");
    };
    let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("hrrr");
    let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("surface");
    let fhour_s;
    let mut cmd_args = vec!["model-image", "--model", model, "--var", var, "--level", level];
    if let Some(fhour) = args.get("fhour").and_then(|v| v.as_f64()) {
        fhour_s = format!("{}", fhour as u32);
        cmd_args.extend(["--fhour", &fhour_s]);
    }
    run_wx_pro(&cmd_args)
}

fn call_wx_point(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let Some(model) = args.get("model").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: model");
    };
    let Some(var) = args.get("var").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: var");
    };
    let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("surface");
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    run_wx_pro(&["point", "--lat", &lat_s, "--lon", &lon_s,
               "--model", model, "--var", var, "--level", level])
}

fn call_wx_scan(args: &Value) -> Value {
    let Some(var) = args.get("var").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: var");
    };
    let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("hrrr");
    let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("surface");
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("max");
    let fhour_s;
    let top_n_s;
    let threshold_s;
    let separation_km_s;
    let lat1_s;
    let lon1_s;
    let lat2_s;
    let lon2_s;
    let mut cmd_args = vec!["scan", "--source", source, "--var", var, "--level", level, "--mode", mode];
    if let Some(fhour) = args.get("fhour").and_then(|v| v.as_f64()) {
        fhour_s = format!("{}", fhour as u32);
        cmd_args.extend(["--fhour", &fhour_s]);
    }
    if let Some(top_n) = args.get("top_n").and_then(|v| v.as_f64()) {
        top_n_s = format!("{}", top_n as u32);
        cmd_args.extend(["--top-n", &top_n_s]);
    }
    if let Some(threshold) = args.get("threshold").and_then(|v| v.as_f64()) {
        threshold_s = format!("{}", threshold);
        cmd_args.extend(["--threshold", &threshold_s]);
    }
    if let Some(separation_km) = args.get("separation_km").and_then(|v| v.as_f64()) {
        separation_km_s = format!("{}", separation_km);
        cmd_args.extend(["--separation-km", &separation_km_s]);
    }
    if let (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) = (
        args.get("lat1").and_then(|v| v.as_f64()),
        args.get("lon1").and_then(|v| v.as_f64()),
        args.get("lat2").and_then(|v| v.as_f64()),
        args.get("lon2").and_then(|v| v.as_f64()),
    ) {
        lat1_s = format!("{}", lat1);
        lon1_s = format!("{}", lon1);
        lat2_s = format!("{}", lat2);
        lon2_s = format!("{}", lon2);
        cmd_args.extend(["--lat1", &lat1_s, "--lon1", &lon1_s, "--lat2", &lat2_s, "--lon2", &lon2_s]);
    }
    run_wx_pro(&cmd_args)
}

fn call_wx_timeseries(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let Some(var) = args.get("var").and_then(|v| v.as_str()) else {
        return mcp_error("Missing required parameter: var");
    };
    let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("surface");
    let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("hrrr");
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    let mut cmd_args = vec!["timeseries", "--lat", &lat_s, "--lon", &lon_s, "--var", var, "--level", level, "--model", model];
    let hours_s;
    if let Some(hours) = args.get("hours").and_then(|v| v.as_f64()) {
        hours_s = format!("{}", hours as u32);
        cmd_args.push("--hours");
        cmd_args.push(&hours_s);
    }
    run_wx_pro(&cmd_args)
}

fn call_wx_evidence(args: &Value) -> Value {
    let (Some(lat), Some(lon)) = (
        args.get("lat").and_then(|v| v.as_f64()),
        args.get("lon").and_then(|v| v.as_f64()),
    ) else {
        return mcp_error("Missing required parameters: lat, lon");
    };
    let lat_s = format!("{}", lat);
    let lon_s = format!("{}", lon);
    run_wx_pro(&["evidence", "--lat", &lat_s, "--lon", &lon_s])
}

// ─── Execution helpers ───────────────────────────────────────────────────────

fn run_wx_pro(args: &[&str]) -> Value {
    let bin = match find_wx_pro() {
        Some(p) => p,
        None => {
            return mcp_error(
                "wx-pro binary not found. Ensure wx-pro.exe is in the same directory as wx-mcp or on PATH.",
            );
        }
    };
    run_command(&bin, args)
}

fn run_wx_lite(args: &[&str]) -> Value {
    let bin = match find_wx_lite() {
        Some(p) => p,
        None => {
            return mcp_error(
                "wx-lite binary not found. Ensure wx-lite.exe is in the same directory as wx-mcp or on PATH.",
            );
        }
    };
    run_command(&bin, args)
}

fn run_command(bin: &PathBuf, args: &[&str]) -> Value {
    let result = Command::new(bin).args(args).output();

    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                mcp_text_result(&stdout)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let msg = if stderr.is_empty() {
                    stdout
                } else {
                    stderr
                };
                mcp_error(&format!(
                    "Command failed (exit {}): {}",
                    output.status.code().unwrap_or(-1),
                    msg.trim()
                ))
            }
        }
        Err(e) => mcp_error(&format!("Failed to execute {}: {}", bin.display(), e)),
    }
}

fn mcp_text_result(text: &str) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

fn mcp_error(message: &str) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": message
            }
        ],
        "isError": true
    })
}

// ─── Main loop ───────────────────────────────────────────────────────────────

fn main() {
    // Log to stderr so it doesn't interfere with JSON-RPC on stdout
    eprintln!("[wx-mcp] Starting MCP server v0.1.0");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    let mut framing = None;

    loop {
        let request = match read_request(&mut stdin, &mut framing) {
            Ok(Some(v)) => v,
            Ok(None) => break,
            Err(e) => {
                // JSON parse error — send error response
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": e.to_string()
                    }
                });
                let mode = framing.unwrap_or(Framing::JsonLines);
                let _ = write_response(&mut stdout, mode, &resp);
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        eprintln!("[wx-mcp] method={} id={:?}", method, id);

        let response = match method {
            "initialize" => {
                let result = handle_initialize();
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                })
            }
            "notifications/initialized" => {
                // Client acknowledgement, no response needed
                continue;
            }
            "tools/list" => {
                let result = handle_tools_list();
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                })
            }
            "tools/call" => {
                let result = handle_tools_call(&params);
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                })
            }
            _ => {
                // Check if it's a notification (no id) — skip silently
                if id.is_none() || id.as_ref() == Some(&Value::Null) {
                    continue;
                }
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {}", method)
                    }
                })
            }
        };

        let mode = framing.unwrap_or(Framing::JsonLines);
        write_response(&mut stdout, mode, &response).unwrap();
    }

    eprintln!("[wx-mcp] Shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_jsonl_requests() {
        let mut framing = None;
        let mut cursor = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n");

        let request = read_request(&mut cursor, &mut framing).unwrap().unwrap();

        assert_eq!(framing, Some(Framing::JsonLines));
        assert_eq!(request["method"], "initialize");
    }

    #[test]
    fn reads_content_length_requests() {
        let mut framing = None;
        let body = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}";
        let request = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut cursor = Cursor::new(request.into_bytes());

        let parsed = read_request(&mut cursor, &mut framing).unwrap().unwrap();

        assert_eq!(framing, Some(Framing::ContentLength));
        assert_eq!(parsed["method"], "tools/list");
    }

    #[test]
    fn writes_content_length_responses() {
        let mut output = Vec::new();
        let response = json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}});

        write_response(&mut output, Framing::ContentLength, &response).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("Content-Length: "));
        assert!(text.contains("\r\n\r\n"));

        let body = text.split_once("\r\n\r\n").unwrap().1;
        let parsed: Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["result"]["ok"], true);
    }
}
