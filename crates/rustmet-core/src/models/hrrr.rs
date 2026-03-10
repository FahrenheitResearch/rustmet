/// Configuration and URL generation for the HRRR (High-Resolution Rapid Refresh) model.
///
/// HRRR is a 3km CONUS model run hourly with forecasts out to 18h (48h for 00/06/12/18z).
pub struct HrrrConfig;

impl HrrrConfig {
    /// Base URL for HRRR GRIB2 files on the AWS Open Data bucket.
    ///
    /// - `date`: format `"YYYYMMDD"` (e.g. `"20260310"`)
    /// - `hour`: model initialization hour (0-23)
    /// - `product`: `"sfc"`, `"prs"`, `"nat"`, or `"subh"`
    /// - `fhour`: forecast hour (0-48)
    pub fn aws_url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        let product_code = Self::product_code(product);
        format!(
            "https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{}/conus/hrrr.t{:02}z.{}f{:02}.grib2",
            date, hour, product_code, fhour
        )
    }

    /// IDX file URL (GRIB2 URL + `.idx`).
    pub fn idx_url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        format!("{}.idx", Self::aws_url(date, hour, product, fhour))
    }

    /// NOMADS URL (NCEP operational server, rolling ~2 day availability).
    pub fn nomads_url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        let product_code = Self::product_code(product);
        format!(
            "https://nomads.ncep.noaa.gov/pub/data/nccf/com/hrrr/prod/hrrr.{}/conus/hrrr.t{:02}z.{}f{:02}.grib2",
            date, hour, product_code, fhour
        )
    }

    fn product_code(product: &str) -> &str {
        match product {
            "sfc" | "surface" => "wrfsfc",
            "prs" | "pressure" => "wrfprs",
            "nat" | "native" => "wrfnat",
            "subh" | "subhourly" => "wrfsubh",
            _ => "wrfsfc",
        }
    }

    // --- Grid specifications ---

    pub fn grid_nx() -> u32 { 1799 }
    pub fn grid_ny() -> u32 { 1059 }
    pub fn grid_dx() -> f64 { 3000.0 } // meters
    pub fn grid_dy() -> f64 { 3000.0 }

    // --- Common variable patterns for .idx matching ---

    pub fn sfc_temp_2m() -> &'static str { "TMP:2 m above ground" }
    pub fn sfc_dewpoint_2m() -> &'static str { "DPT:2 m above ground" }
    pub fn sfc_rh_2m() -> &'static str { "RH:2 m above ground" }
    pub fn sfc_u_wind_10m() -> &'static str { "UGRD:10 m above ground" }
    pub fn sfc_v_wind_10m() -> &'static str { "VGRD:10 m above ground" }
    pub fn sfc_gust() -> &'static str { "GUST:surface" }
    pub fn sfc_mslp() -> &'static str { "MSLMA:mean sea level" }
    pub fn sfc_pressure() -> &'static str { "PRES:surface" }
    pub fn sfc_cape() -> &'static str { "CAPE:surface" }
    pub fn sfc_cin() -> &'static str { "CIN:surface" }
    pub fn composite_refl() -> &'static str { "REFC:entire atmosphere" }
    pub fn sfc_precip() -> &'static str { "APCP:surface" }
    pub fn sfc_visibility() -> &'static str { "VIS:surface" }
    pub fn updraft_helicity() -> &'static str { "MXUPHL" }
    pub fn sfc_hgt() -> &'static str { "HGT:surface" }

    /// Build a pattern for a variable on a pressure level (e.g., `"TMP:500 mb"`).
    pub fn prs_var(var: &str, level_mb: u32) -> String {
        format!("{}:{} mb", var, level_mb)
    }
}
