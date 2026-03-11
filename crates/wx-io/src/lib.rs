//! Unified I/O layer for weather data formats.
//!
//! Currently wraps rustmet-core's GRIB2 implementation.
//! Future: NEXRAD Level II, NetCDF/HDF5, shapefiles.

pub mod grib2 {
    pub use rustmet_core::grib2::*;
}

/// NEXRAD Level II radar data (future)
pub mod nexrad {
    // Will be populated from rustdar
}

/// NetCDF/HDF5 reader (future)
pub mod netcdf {
    // Will be populated from wrf-solar/wrf-render
}
