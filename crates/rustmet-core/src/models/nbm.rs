/// Configuration and URL generation for the NBM (National Blend of Models).
///
/// NBM is a statistically post-processed blend of multiple NWS and global models.
/// 2.5km CONUS grid, runs hourly, forecasts out to 264 hours.
pub struct NbmConfig;

impl NbmConfig {
    /// AWS Open Data URL for NBM GRIB2 files.
    ///
    /// - `date`: format `"YYYYMMDD"` (e.g. `"20260310"`)
    /// - `hour`: model initialization hour (0-23)
    /// - `product`: `"core"` or `"qmd"`
    /// - `fhour`: forecast hour (0-264)
    pub fn aws_url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        let product_code = Self::product_code(product);
        format!(
            "https://noaa-nbm-grib2-pds.s3.amazonaws.com/blend.{}/{:02}/{}/blend.t{:02}z.{}.f{:03}.co.grib2",
            date, hour, product_code, hour, product_code, fhour
        )
    }

    /// IDX file URL (GRIB2 URL + `.idx`).
    pub fn idx_url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        format!("{}.idx", Self::aws_url(date, hour, product, fhour))
    }

    /// NOMADS URL for NBM.
    pub fn nomads_url(date: &str, hour: u32, product: &str, fhour: u32) -> String {
        let product_code = Self::product_code(product);
        format!(
            "https://nomads.ncep.noaa.gov/pub/data/nccf/com/blend/prod/blend.{}/{:02}/{}/blend.t{:02}z.{}.f{:03}.co.grib2",
            date, hour, product_code, hour, product_code, fhour
        )
    }

    fn product_code(product: &str) -> &str {
        match product {
            "qmd" | "quantile" => "qmd",
            _ => "core",
        }
    }

    // --- Grid specifications (2.5km CONUS, NDFD grid) ---

    pub fn grid_nx() -> u32 { 2345 }
    pub fn grid_ny() -> u32 { 1597 }
    pub fn grid_dx() -> f64 { 2539.703 } // meters (Lambert conformal)
    pub fn grid_dy() -> f64 { 2539.703 }

    // --- Common variable patterns for .idx matching ---

    pub fn sfc_temp_2m() -> &'static str { "TMP:2 m above ground" }
    pub fn sfc_dewpoint_2m() -> &'static str { "DPT:2 m above ground" }
    pub fn sfc_rh_2m() -> &'static str { "RH:2 m above ground" }
    pub fn sfc_u_wind_10m() -> &'static str { "UGRD:10 m above ground" }
    pub fn sfc_v_wind_10m() -> &'static str { "VGRD:10 m above ground" }
    pub fn sfc_gust() -> &'static str { "GUST:surface" }
    pub fn sfc_pressure() -> &'static str { "PRES:surface" }
    pub fn sfc_precip() -> &'static str { "APCP:surface" }
    pub fn sfc_visibility() -> &'static str { "VIS:surface" }
    pub fn max_temp() -> &'static str { "TMAX:2 m above ground" }
    pub fn min_temp() -> &'static str { "TMIN:2 m above ground" }
    pub fn wind_speed() -> &'static str { "WIND:10 m above ground" }
    pub fn sky_cover() -> &'static str { "TCDC:entire atmosphere" }
    pub fn snow() -> &'static str { "ASNOW:surface" }
    pub fn precip_prob() -> &'static str { "APCP:surface:prob" }

    /// Build a pattern for a variable on a pressure level.
    pub fn prs_var(var: &str, level_mb: u32) -> String {
        format!("{}:{} mb", var, level_mb)
    }
}
