/// Configuration and URL generation for WPC QPF (Weather Prediction Center
/// Quantitative Precipitation Forecasts).
///
/// WPC produces manually-analyzed QPF grids at 2.5km resolution over CONUS.
/// Products include 6-hour QPF accumulations and excessive rainfall outlook.
pub struct WpcConfig;

impl WpcConfig {
    /// FTP/HTTPS URL for WPC 2.5km QPF GRIB2 files.
    ///
    /// - `date`: format `"YYYYMMDD"` (e.g. `"20260310"`)
    /// - `hour`: issuance hour (0, 6, 12, 18)
    /// - `product`: `"6hr"` (6-hour QPF), `"day1"`, `"day2"`, `"day3"`
    /// - `fhour`: forecast hour for 6-hr period ending (e.g., 6, 12, 18, 24, ...)
    pub fn url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        match product {
            "6hr" | "6h" | "qpf" => format!(
                "https://ftp.wpc.ncep.noaa.gov/2p5km_qpf/p06m_{}{:02}f{:03}.grb",
                date, hour, fhour
            ),
            "day1" => format!(
                "https://ftp.wpc.ncep.noaa.gov/2p5km_qpf/d1_tl_{}12.grb",
                date
            ),
            "day2" => format!(
                "https://ftp.wpc.ncep.noaa.gov/2p5km_qpf/d2_tl_{}12.grb",
                date
            ),
            "day3" => format!(
                "https://ftp.wpc.ncep.noaa.gov/2p5km_qpf/d3_tl_{}12.grb",
                date
            ),
            _ => format!(
                "https://ftp.wpc.ncep.noaa.gov/2p5km_qpf/p06m_{}{:02}f{:03}.grb",
                date, hour, fhour
            ),
        }
    }

    /// NOMADS URL for WPC QPF products.
    pub fn nomads_url(date: &str, hour: u32, fhour: u32) -> String {
        format!(
            "https://nomads.ncep.noaa.gov/pub/data/nccf/com/wpc/prod/qpf/p06m_{}{:02}f{:03}.grb",
            date, hour, fhour
        )
    }

    /// WPC QPF files are typically small enough that .idx files are not used.
    /// Returns `None`.
    pub fn idx_url(_date: &str, _hour: u32, _product: &str, _fhour: u32) -> Option<String> {
        None
    }

    // --- Grid specifications (2.5km CONUS, NDFD grid) ---

    pub fn grid_nx() -> u32 { 2345 }
    pub fn grid_ny() -> u32 { 1597 }
    pub fn grid_dx() -> f64 { 2539.703 } // meters (Lambert conformal)
    pub fn grid_dy() -> f64 { 2539.703 }

    // --- Common variable patterns ---

    pub fn precip_6hr() -> &'static str { "APCP:surface" }
    pub fn precip_total() -> &'static str { "APCP:surface" }
}
