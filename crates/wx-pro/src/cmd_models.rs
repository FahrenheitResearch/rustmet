use serde::Serialize;
use crate::output::print_json;

#[derive(Serialize)]
struct ModelInfo {
    id: &'static str,
    name: &'static str,
    resolution: &'static str,
    domain: &'static str,
    interval_hours: u32,
    forecast_hours: u32,
}

#[derive(Serialize)]
struct ModelsResponse {
    models: Vec<ModelInfo>,
}

pub fn run(pretty: bool) {
    let resp = ModelsResponse {
        models: vec![
            ModelInfo {
                id: "hrrr",
                name: "HRRR",
                resolution: "3km",
                domain: "CONUS",
                interval_hours: 1,
                forecast_hours: 48,
            },
            ModelInfo {
                id: "gfs",
                name: "GFS",
                resolution: "0.25deg",
                domain: "Global",
                interval_hours: 6,
                forecast_hours: 384,
            },
            ModelInfo {
                id: "nam",
                name: "NAM",
                resolution: "12km",
                domain: "North America",
                interval_hours: 6,
                forecast_hours: 84,
            },
            ModelInfo {
                id: "rap",
                name: "RAP",
                resolution: "13km",
                domain: "North America",
                interval_hours: 1,
                forecast_hours: 21,
            },
            ModelInfo {
                id: "mrms",
                name: "MRMS",
                resolution: "1km",
                domain: "CONUS",
                interval_hours: 0, // 2-minute updates
                forecast_hours: 0, // analysis only
            },
        ],
    };
    print_json(&resp, pretty);
}
