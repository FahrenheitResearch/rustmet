use crate::output::{print_json, print_error};
use serde_json::json;

pub fn run(station: &str, lat: Option<f64>, lon: Option<f64>, hour: &str, pretty: bool) {
    let stn_id = if let (Some(la), Some(lo)) = (lat, lon) {
        let nearest = wx_sounding::nearest_raob_station(la, lo);
        nearest.icao.to_string()
    } else if !station.is_empty() {
        station.to_string()
    } else {
        print_error("Provide --station or --lat/--lon");
    };

    let result = match hour {
        "12" | "12z" | "12Z" => wx_sounding::fetch_latest_12z(&stn_id),
        "00" | "00z" | "00Z" | "0" => wx_sounding::fetch_latest_00z(&stn_id),
        _ => wx_sounding::fetch_latest_12z(&stn_id),
    };

    match result {
        Ok(mut sounding) => {
            wx_sounding::compute_indices(&mut sounding);

            let levels: Vec<serde_json::Value> = sounding.levels.iter().map(|l| {
                json!({
                    "pressure_hpa": l.pressure,
                    "height_m": l.height,
                    "temperature_c": l.temperature,
                    "dewpoint_c": l.dewpoint,
                    "wind_dir": l.wind_dir,
                    "wind_speed_kt": l.wind_speed,
                })
            }).collect();

            let idx = &sounding.indices;
            print_json(&json!({
                "station": sounding.station,
                "station_name": sounding.station_name,
                "lat": sounding.lat,
                "lon": sounding.lon,
                "elevation_m": sounding.elevation_m,
                "time": sounding.time,
                "num_levels": levels.len(),
                "levels": levels,
                "indices": {
                    "sbcape": idx.sbcape,
                    "sbcin": idx.sbcin,
                    "mlcape": idx.mlcape,
                    "mlcin": idx.mlcin,
                    "mucape": idx.mucape,
                    "mucin": idx.mucin,
                    "lcl_m": idx.lcl_m,
                    "lfc_m": idx.lfc_m,
                    "el_m": idx.el_m,
                    "lifted_index": idx.li,
                    "total_totals": idx.total_totals,
                    "k_index": idx.k_index,
                    "sweat": idx.sweat,
                    "bulk_shear_01_kt": idx.bulk_shear_01,
                    "bulk_shear_06_kt": idx.bulk_shear_06,
                    "srh_01": idx.srh_01,
                    "srh_03": idx.srh_03,
                    "stp": idx.stp,
                    "pw_mm": idx.pw_mm,
                },
            }), pretty);
        }
        Err(e) => print_error(&e),
    }
}
