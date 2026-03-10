/// Configuration and URL generation for the RTMA (Real-Time Mesoscale Analysis).
///
/// RTMA is a 2.5km CONUS analysis product — no forecast hours, just analysis.
/// Updated hourly with observations blended onto a high-resolution grid.
pub struct RtmaConfig;

impl RtmaConfig {
    /// AWS Open Data URL for RTMA GRIB2 files.
    ///
    /// - `date`: format `"YYYYMMDD"` (e.g. `"20260310"`)
    /// - `hour`: analysis hour (0-23)
    /// - `product`: `"2dvaranl"` (2D analysis) or `"2dvarges"` (2D guess)
    pub fn aws_url(date: &str, hour: u32, product: &str) -> String {
        let product_code = Self::product_code(product);
        format!(
            "https://noaa-rtma-pds.s3.amazonaws.com/rtma2p5.{}/rtma2p5.t{:02}z.{}_ndfd.grb2_wexp",
            date, hour, product_code
        )
    }

    /// IDX file URL.
    pub fn idx_url(date: &str, hour: u32, product: &str) -> String {
        format!("{}.idx", Self::aws_url(date, hour, product))
    }

    /// NOMADS URL for RTMA.
    pub fn nomads_url(date: &str, hour: u32, product: &str) -> String {
        let product_code = Self::product_code(product);
        format!(
            "https://nomads.ncep.noaa.gov/pub/data/nccf/com/rtma/prod/rtma2p5.{}/rtma2p5.t{:02}z.{}_ndfd.grb2_wexp",
            date, hour, product_code
        )
    }

    fn product_code(product: &str) -> &str {
        match product {
            "guess" | "ges" | "2dvarges" => "2dvarges",
            _ => "2dvaranl",
        }
    }

    // --- Grid specifications (2.5km CONUS, NDFD grid) ---

    pub fn grid_nx() -> u32 { 2345 }
    pub fn grid_ny() -> u32 { 1597 }
    pub fn grid_dx() -> f64 { 2539.703 } // meters (Lambert conformal)
    pub fn grid_dy() -> f64 { 2539.703 }

    // --- Common variable patterns for .idx matching ---
    // RTMA is analysis-only so variables are observed/analyzed fields

    pub fn sfc_temp_2m() -> &'static str { "TMP:2 m above ground" }
    pub fn sfc_dewpoint_2m() -> &'static str { "DPT:2 m above ground" }
    pub fn sfc_u_wind_10m() -> &'static str { "UGRD:10 m above ground" }
    pub fn sfc_v_wind_10m() -> &'static str { "VGRD:10 m above ground" }
    pub fn sfc_gust() -> &'static str { "GUST:surface" }
    pub fn sfc_pressure() -> &'static str { "PRES:surface" }
    pub fn sfc_visibility() -> &'static str { "VIS:surface" }
    pub fn sfc_ceiling() -> &'static str { "HGT:cloud ceiling" }
    pub fn wind_speed() -> &'static str { "WIND:10 m above ground" }
    pub fn wind_direction() -> &'static str { "WDIR:10 m above ground" }
}
