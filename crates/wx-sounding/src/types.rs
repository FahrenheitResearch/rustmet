use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sounding {
    pub station: String,
    pub station_name: String,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
    pub time: String,
    pub levels: Vec<SoundingLevel>,
    pub surface: Option<SurfaceObs>,
    pub indices: SoundingIndices,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundingLevel {
    pub pressure: f64,    // hPa
    pub height: f64,      // meters MSL
    pub temperature: f64, // Celsius
    pub dewpoint: f64,    // Celsius
    pub wind_dir: f64,    // degrees
    pub wind_speed: f64,  // knots
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceObs {
    pub pressure: f64,
    pub temperature: f64,
    pub dewpoint: f64,
    pub wind_dir: f64,
    pub wind_speed: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SoundingIndices {
    pub sbcape: f64,
    pub sbcin: f64,
    pub mlcape: f64,
    pub mlcin: f64,
    pub mucape: f64,
    pub mucin: f64,
    pub lcl_m: f64,
    pub lfc_m: f64,
    pub el_m: f64,
    pub li: f64,
    pub total_totals: f64,
    pub k_index: f64,
    pub sweat: f64,
    pub bulk_shear_01: f64,
    pub bulk_shear_06: f64,
    pub srh_01: f64,
    pub srh_03: f64,
    pub stp: f64,
    pub pw_mm: f64,
}
