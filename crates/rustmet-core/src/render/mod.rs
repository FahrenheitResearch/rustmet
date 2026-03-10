//! Lightweight rendering module for weather maps.
//!
//! Provides zero-dependency plotting primitives:
//! - Colormaps: standard meteorological color scales
//! - Raster: grid-to-RGBA pixel rendering
//! - Contour: marching squares isopleths
//! - PNG: minimal PNG encoder for output
//!
//! This is a library-level module suitable for embedding in Python bindings
//! or any consumer of rustmet-core. No windowing, no GPU, no matplotlib.

pub mod colormap;
pub mod raster;
pub mod contour;
pub mod filled_contour;
pub mod overlay;
pub mod encode;
pub mod skewt;
pub mod hodograph;
pub mod station;
pub mod cross_section;
