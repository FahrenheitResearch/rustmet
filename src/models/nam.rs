/// Configuration and URL generation for the NAM (North American Mesoscale) model.
///
/// NAM runs at 12km over North America, initialized every 6 hours (00/06/12/18z)
/// with forecasts out to 84 hours.
pub struct NamConfig;

impl NamConfig {
    /// Base URL for NAM GRIB2 files on the NOMADS server (12km CONUS nest).
    ///
    /// - `date`: format `"YYYYMMDD"` (e.g. `"20260310"`)
    /// - `hour`: model initialization hour (0, 6, 12, 18)
    /// - `fhour`: forecast hour (0-84)
    pub fn nomads_url(date: &str, hour: u32, fhour: u32) -> String {
        format!(
            "https://nomads.ncep.noaa.gov/pub/data/nccf/com/nam/prod/nam.{}/nam.t{:02}z.awphys{:02}.tm00.grib2",
            date, hour, fhour
        )
    }

    /// IDX file URL.
    pub fn idx_url(date: &str, hour: u32, fhour: u32) -> String {
        format!("{}.idx", Self::nomads_url(date, hour, fhour))
    }

    /// AWS Open Data URL for NAM.
    pub fn aws_url(date: &str, hour: u32, fhour: u32) -> String {
        format!(
            "https://noaa-nam-bdp-pds.s3.amazonaws.com/nam.{}/nam.t{:02}z.awphys{:02}.tm00.grib2",
            date, hour, fhour
        )
    }

    // --- Grid specifications (12km CONUS) ---

    pub fn grid_nx() -> u32 { 614 }
    pub fn grid_ny() -> u32 { 428 }
    pub fn grid_dx() -> f64 { 12190.58 } // meters (Lambert conformal)
    pub fn grid_dy() -> f64 { 12190.58 }

    // --- Common variable patterns for .idx matching ---

    pub fn sfc_temp_2m() -> &'static str { "TMP:2 m above ground" }
    pub fn sfc_dewpoint_2m() -> &'static str { "DPT:2 m above ground" }
    pub fn sfc_rh_2m() -> &'static str { "RH:2 m above ground" }
    pub fn sfc_u_wind_10m() -> &'static str { "UGRD:10 m above ground" }
    pub fn sfc_v_wind_10m() -> &'static str { "VGRD:10 m above ground" }
    pub fn sfc_gust() -> &'static str { "GUST:surface" }
    pub fn sfc_mslp() -> &'static str { "PRMSL:mean sea level" }
    pub fn sfc_pressure() -> &'static str { "PRES:surface" }
    pub fn sfc_cape() -> &'static str { "CAPE:surface" }
    pub fn sfc_cin() -> &'static str { "CIN:surface" }
    pub fn composite_refl() -> &'static str { "REFC:entire atmosphere" }
    pub fn sfc_precip() -> &'static str { "APCP:surface" }
    pub fn sfc_visibility() -> &'static str { "VIS:surface" }
    pub fn sfc_hgt() -> &'static str { "HGT:surface" }

    /// Build a pattern for a variable on a pressure level.
    pub fn prs_var(var: &str, level_mb: u32) -> String {
        format!("{}:{} mb", var, level_mb)
    }
}
