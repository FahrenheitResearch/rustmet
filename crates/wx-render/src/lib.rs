//! Unified rendering layer for meteorological visualizations.
//!
//! Provides colormaps, raster rendering, contours, overlays,
//! and specialty plots (Skew-T, hodograph, station, cross-section).
//! Future: GPU radar rendering, map compositor, tile generation.

// Re-export from rustmet-core's render module
pub use rustmet_core::render::*;

/// GPU radar rendering (from rustdar). Placeholder for future implementation.
pub mod radar {}

/// Map compositing layer. Placeholder for future implementation.
pub mod compositor {}

/// XYZ tile generation. Placeholder for future implementation.
pub mod tiles {}
