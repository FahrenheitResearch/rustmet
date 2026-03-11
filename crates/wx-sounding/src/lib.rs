//! wx-sounding — Upper-air observations: radiosonde sounding fetcher and parser.
//!
//! Fetches and parses radiosonde soundings from the University of Wyoming
//! archive, computes derived thermodynamic and kinematic indices using
//! `wx_math::thermo`.

pub mod types;
pub mod wyoming;
pub mod raob_stations;
pub mod derived;

pub use types::{Sounding, SoundingLevel, SurfaceObs, SoundingIndices};
pub use wyoming::{fetch_sounding, fetch_latest_12z, fetch_latest_00z};
pub use raob_stations::{RaobStation, find_raob_station, nearest_raob_station, all_raob_stations};
pub use derived::compute_indices;
