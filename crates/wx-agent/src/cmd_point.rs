use serde::Serialize;
use chrono::Utc;
use crate::output::print_json;

#[derive(Serialize)]
struct PointResponse {
    lat: f64,
    lon: f64,
    model: String,
    variable: String,
    level: String,
    value: f64,
    units: &'static str,
    valid_time: String,
    run_time: String,
}

pub fn run(lat: f64, lon: f64, model: &str, var: &str, level: &str, pretty: bool) {
    // Stub: return mock data with correct structure.
    // Will be wired to real GRIB2 download + extraction later.
    let now = Utc::now();
    let run_hour = (now.format("%H").to_string().parse::<u32>().unwrap_or(12) / 6) * 6;
    let run_time = now.format(&format!("%Y-%m-%dT{:02}:00:00Z", run_hour)).to_string();
    let valid_time = now.format("%Y-%m-%dT%H:00:00Z").to_string();

    let (value, units) = match var {
        "temperature" | "temp" | "t" => (28.5, "C"),
        "dewpoint" | "td" => (18.0, "C"),
        "wind" | "wind_speed" => (15.0, "kt"),
        "pressure" | "mslp" => (1013.25, "hPa"),
        "rh" | "relative_humidity" => (65.0, "%"),
        "cape" => (2500.0, "J/kg"),
        "cin" => (-30.0, "J/kg"),
        "reflectivity" | "refl" => (45.0, "dBZ"),
        "visibility" => (10.0, "km"),
        "precipitation" | "precip" => (2.5, "mm"),
        _ => (0.0, "unknown"),
    };

    let resp = PointResponse {
        lat,
        lon,
        model: model.to_string(),
        variable: var.to_string(),
        level: level.to_string(),
        value,
        units,
        valid_time,
        run_time,
    };
    print_json(&resp, pretty);
}
