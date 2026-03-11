use serde::Serialize;
use crate::output::print_json;

#[derive(Serialize)]
struct SoundingLevel {
    pressure_hpa: u32,
    height_m: u32,
    temperature_c: f64,
    dewpoint_c: f64,
    wind_dir: u32,
    wind_speed_kt: u32,
}

#[derive(Serialize)]
struct SoundingIndices {
    sbcape: f64,
    sbcin: f64,
    mlcape: f64,
    mlcin: f64,
    lcl_m: f64,
    lfc_m: f64,
    shear_01_kt: f64,
    shear_06_kt: f64,
    srh_01: f64,
    srh_03: f64,
    stp: f64,
}

#[derive(Serialize)]
struct SoundingResponse {
    lat: f64,
    lon: f64,
    model: String,
    levels: Vec<SoundingLevel>,
    indices: SoundingIndices,
}

pub fn run(lat: f64, lon: f64, model: &str, pretty: bool) {
    // Stub: return mock sounding data with correct structure.
    let resp = SoundingResponse {
        lat,
        lon,
        model: model.to_string(),
        levels: vec![
            SoundingLevel { pressure_hpa: 1000, height_m: 100, temperature_c: 28.0, dewpoint_c: 18.0, wind_dir: 180, wind_speed_kt: 15 },
            SoundingLevel { pressure_hpa: 925, height_m: 750, temperature_c: 22.0, dewpoint_c: 16.0, wind_dir: 190, wind_speed_kt: 20 },
            SoundingLevel { pressure_hpa: 850, height_m: 1500, temperature_c: 16.0, dewpoint_c: 10.0, wind_dir: 210, wind_speed_kt: 30 },
            SoundingLevel { pressure_hpa: 700, height_m: 3000, temperature_c: 6.0, dewpoint_c: -2.0, wind_dir: 230, wind_speed_kt: 40 },
            SoundingLevel { pressure_hpa: 500, height_m: 5500, temperature_c: -12.0, dewpoint_c: -22.0, wind_dir: 250, wind_speed_kt: 55 },
            SoundingLevel { pressure_hpa: 300, height_m: 9200, temperature_c: -38.0, dewpoint_c: -48.0, wind_dir: 260, wind_speed_kt: 80 },
            SoundingLevel { pressure_hpa: 250, height_m: 10500, temperature_c: -48.0, dewpoint_c: -58.0, wind_dir: 265, wind_speed_kt: 95 },
            SoundingLevel { pressure_hpa: 200, height_m: 12000, temperature_c: -55.0, dewpoint_c: -65.0, wind_dir: 270, wind_speed_kt: 70 },
        ],
        indices: SoundingIndices {
            sbcape: 2500.0,
            sbcin: -30.0,
            mlcape: 1800.0,
            mlcin: -50.0,
            lcl_m: 800.0,
            lfc_m: 1500.0,
            shear_01_kt: 25.0,
            shear_06_kt: 50.0,
            srh_01: 200.0,
            srh_03: 350.0,
            stp: 3.5,
        },
    };
    print_json(&resp, pretty);
}
