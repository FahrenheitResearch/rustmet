# rustmet

[![CI](https://github.com/FahrenheitResearch/rustmet/actions/workflows/ci.yml/badge.svg)](https://github.com/FahrenheitResearch/rustmet/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Pure Rust GRIB2 decoder and meteorological compute library with Python bindings.**

rustmet decodes GRIB2 data from operational weather models and provides fast meteorological calculations, all in pure Rust. Python bindings via PyO3 let you drop it into existing forecasting pipelines.

> **Status:** Active development. APIs may change between releases.

## Install

**Python**
```bash
pip install rustmet
```

**Rust**
```bash
cargo add rustmet-core
```

## Quick Start

### Python

```python
import rustmet

# Fetch HRRR 2m temperature -- downloads only the bytes you need
grib = rustmet.fetch("hrrr", "2026-03-09/00z",
                     vars=["TMP:2 m above ground"])

msg = grib.messages[0]
data = msg.values_2d()    # numpy array, shape (ny, nx)
lats = msg.lats()
lons = msg.lons()

# Or convert to xarray (pip install xarray)
ds = rustmet.to_xarray(grib)
```

### Rust

```rust
use rustmet_core::{Client, ModelSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let grib = client.fetch(
        ModelSpec::hrrr("2026-03-09/00z"),
        &["TMP:2 m above ground"],
    )?;

    for msg in &grib.messages {
        println!("{}: {} values", msg.variable, msg.values.len());
    }
    Ok(())
}
```

## Benchmarks

All timings below were measured on a single machine. Results will vary by hardware and OS. Run `python benchmark/compare.py` to reproduce.

### GRIB2 Decoding (real HRRR file)

| Operation | rustmet | Comparison | Speedup |
|-----------|---------|------------|---------|
| Open/parse GRIB | 5.16 ms | ecCodes 5.75 ms | 1.1x |
| Unpack all messages | 72.00 ms | ecCodes 77.16 ms | 1.07x |
| Open/parse GRIB | 5.16 ms | cfgrib 195.42 ms | 37.9x |

Decoded values match ecCodes output (`numpy.allclose=True`) on all tested HRRR and synthetic cases. The large gap vs cfgrib reflects that cfgrib's `open_datasets` does substantially more work (xarray integration, coordinate construction) than a raw GRIB parse.

### Meteorological Compute

| Function | rustmet | Comparison | Speedup |
|----------|---------|------------|---------|
| Gaussian smooth (sigma=2, 400x400) | 906.5 us | SciPy 1.32 ms | 1.45x |
| Equivalent potential temperature (10k points) | 302.3 us | MetPy 752.4 us | 2.49x |
| Potential temperature | -- | MetPy | ~2x |
| Dewpoint from RH | -- | MetPy | ~3x |
| Mixing ratio | -- | MetPy | ~9x |
| Vapor pressure | -- | MetPy | ~5x |
| Vorticity (finite difference) | -- | NumPy | ~2.3x |
| Divergence (finite difference) | -- | NumPy | ~2.3x |

MetPy uses pint-based unit tracking, which adds overhead by design for dimensional safety. The comparison reflects wall-clock time, not algorithmic differences.

### End-to-End: Fetch + Decode

Downloading and decoding 8 HRRR surface variables (15M grid points), median of 5 runs:

| Tool | Download | Decode | Total |
|------|----------|--------|-------|
| **rustmet** | **1.5 s** | **0.11 s** | **1.6 s** |
| requests + cfgrib | 7.4 s | 0.42 s | 7.8 s |
| herbie + cfgrib | 8.1 s | 0.75 s | 8.9 s |

The end-to-end speedup comes from combining HTTP range requests with Rust-native decoding. The exact ratio depends on network conditions, data source, and comparison stack.

## Validated Against

rustmet decode and compute outputs are tested for numerical agreement with:

- **ecCodes** (ECMWF) -- GRIB2 decode values via `numpy.allclose` on real HRRR and synthetic grids
- **cfgrib** -- GRIB2 decode cross-check
- **MetPy** -- thermodynamic functions (theta, theta-e, dewpoint, mixing ratio, vapor pressure)
- **SciPy** -- Gaussian smoothing kernel
- **NumPy** -- finite-difference vorticity and divergence

## Supported Models

| Model | Resolution | Coverage | Update Cycle |
|-------|-----------|----------|-------------|
| **HRRR** | 3 km | CONUS | Hourly |
| **GFS** | 0.25 deg | Global | 6-hourly |
| **NAM** | 12 km | North America | 6-hourly |
| **RAP** | 13 km | CONUS | Hourly |

## Features

- **Byte-range downloads** -- fetches only the GRIB2 messages you need, not the entire file
- **Pure Rust TLS** -- no OpenSSL dependency, builds anywhere
- **Zero-copy GRIB2 parser** -- supports templates 5.0 (simple), 5.40 (JPEG2000), 5.41 (PNG)
- **NumPy integration** -- decoded grids returned directly as numpy arrays
- **xarray support** -- `rustmet.to_xarray()` returns a labeled Dataset with coordinates
- **Parallel decoding** -- multi-message files decoded across threads with rayon
- **Cross-platform** -- Linux, macOS, Windows; x86_64 and aarch64

## Test Suite

rustmet ships with **510+ tests** covering every layer of the stack:

```bash
cargo test                    # Run all 510+ tests
cargo test -p rustmet-core    # Core library tests only
cargo bench                   # Criterion benchmarks
```

- **GRIB2 parser** -- section parsing, template decoding, bit-level unpacking, edge cases
- **Meteorological calculations** -- CAPE, CIN, helicity, storm motion, lapse rates, thermodynamic profiles validated against known values
- **Grid math** -- derivatives, divergence, vorticity, Laplacian on synthetic fields with analytic solutions
- **Projections** -- Lambert Conformal, Lat/Lon, Mercator, Polar Stereographic round-trip accuracy
- **Download/index** -- `.idx` parsing, byte-range computation, URL generation for all supported models
- **Rendering** -- colormap interpolation, contour generation, wind barb geometry, PNG output validation
- **Python bindings** -- PyO3 integration tests for fetch, decode, and xarray conversion

Benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) for GRIB2 decoding throughput, JPEG2000 decompression, grid interpolation, and colormap lookup.

## Project Structure

```
rustmet/
  crates/
    rustmet-core/    # Pure Rust library: GRIB2 parser, HTTP client, model definitions
    rustmet-py/      # Python bindings (PyO3 + maturin)
    rustmet-wasm/    # WebAssembly bindings
  src/               # CLI binary
  benchmark/         # Cross-library comparison benchmarks
```

## License

MIT
