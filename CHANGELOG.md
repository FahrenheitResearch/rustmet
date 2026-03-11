# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-10

### Added

#### Core GRIB2 Engine
- Pure Rust GRIB2 parser with zero-copy decoding
- Support for grid definition templates 0 (Lat/Lon), 30 (Lambert Conformal), 20 (Polar Stereographic), 10 (Mercator), 1 (Rotated Lat/Lon)
- Data representation templates 5.0 (simple packing), 5.40 (JPEG2000), 5.41 (PNG)
- Complete WMO parameter tables for discipline 0 (meteorological) and 10 (oceanographic)
- BitReader for sub-byte field extraction
- GRIB2 writer for round-trip encoding

#### Download Client
- HTTP byte-range downloads fetching only needed GRIB2 messages
- `.idx` file parsing for variable-level message selection
- Connection pooling with pure Rust TLS (rustls, no OpenSSL dependency)
- Configurable retry with exponential backoff
- Disk-based caching with content-addressable storage
- Parallel chunk downloads via rayon
- Latest model run auto-detection

#### Supported Models (16 configurations)
- **HRRR** -- 3 km CONUS, hourly, surface/pressure/subhourly products
- **GFS** -- 0.25 deg global, 6-hourly
- **NAM** -- 12 km North America, 6-hourly, CONUS/Alaska nests
- **RAP** -- 13 km CONUS, hourly
- URL generation for AWS, NOMADS, and Google Cloud sources

#### Meteorological Calculations (45+)
- Thermodynamics: potential temperature, equivalent potential temperature, wet-bulb temperature, virtual temperature, mixing ratio, vapor pressure, LCL, LFC, EL
- Stability: CAPE (surface-based, mixed-layer, most unstable), CIN, lifted index, Showalter index, K-index, Total Totals
- Kinematics: storm-relative helicity (0-1 km, 0-3 km), Bunkers storm motion, bulk wind shear, mean wind
- Composite indices: Significant Tornado Parameter (STP), Energy-Helicity Index (EHI), Supercell Composite Parameter
- Boundary layer: PBL height estimation, lapse rates (height-based and pressure-based)
- Sounding analysis: full parcel trace, DCAPE, 0-3 km MLCAPE
- Isentropic analysis and apparent temperature (heat index, wind chill)

#### Grid Mathematics
- First and second derivatives on regular grids
- Divergence, vorticity (relative, absolute, curvature, shear), advection
- Laplacian, deformation, frontogenesis
- Gaussian and N-point smoothing
- Geospatial gradient with latitude-aware scaling

#### Map Projections
- Lambert Conformal Conic (HRRR, NAM, RAP grids)
- Equidistant Cylindrical / Lat-Lon (GFS grids)
- Polar Stereographic, Mercator, Rotated Lat/Lon
- Forward and inverse transforms with grid-index mapping

#### Rendering Engine
- Filled-contour plotting with marching-squares contouring
- Wu anti-aliased line drawing for boundaries and contours
- 3x3 supersampled fill for smooth contour edges
- Wind barb rendering with filled pennants and calm circles
- Inline contour labels
- Discrete banded colorbar with tick marks
- TTF text rendering via fontdue
- State/country boundary overlays
- 14 custom colormaps ported from solarpower07 (temperature, dewpoint, reflectivity, CAPE, winds, RH, STP, EHI, lapse rate, UH, precip, vorticity, sim IR, geopotential anomaly)
- Skew-T / Log-P diagram rendering
- Vertical cross-section rendering
- PNG output at 1100x850 px (11x8.5 in at 100 DPI)

#### Python Bindings (PyO3)
- 78 `#[pyfunction]` exports covering all meteorological calculations
- `rustmet.fetch()` for downloading and decoding GRIB2 data
- NumPy array integration for decoded grids
- `rustmet.to_xarray()` for labeled xarray Dataset output
- `rustmet.search()` for GRIB2 message querying
- Multi-forecast-hour and DataFrame support
- Maturin-based build for PyPI distribution

#### CLI Binary
- `rustmet plot` -- download and render model products to PNG
- `rustmet download` -- cache GRIB2 data for offline use
- `rustmet products` -- list available plotting products
- `rustmet info` -- inspect a local GRIB2 file

#### Testing and CI
- 510+ unit and integration tests across all modules
- Criterion benchmarks for decoding, dynamics, smoothing, and GRIB2 round-trip
- GitHub Actions CI workflow with build, test, clippy, and format checks
- Benchmark tracking workflow

#### Error Handling
- Typed error hierarchy (`GribError`, `DownloadError`, `ProjectionError`)
- `Send + Sync` for all error types
- Descriptive `Display` implementations

[0.1.0]: https://github.com/FahrenheitResearch/rustmet/releases/tag/v0.1.0
