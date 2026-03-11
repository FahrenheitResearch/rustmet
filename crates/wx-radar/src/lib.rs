//! Radar-specific processing for NEXRAD and other weather radars.
//!
//! Provides radar product definitions, derived products (VIL, echo tops),
//! rotation detection (mesocyclone, TVS), and storm-relative velocity.
//! Future: full implementation migrated from rustdar.

pub mod products;
pub mod derived;
pub mod detection;
pub mod sites;

pub use wx_field::{RadialField, RadialSweep, Radial, RadarSite};
