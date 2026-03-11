# rustmet

[![CI](https://github.com/FahrenheitResearch/rustmet/actions/workflows/ci.yml/badge.svg)](https://github.com/FahrenheitResearch/rustmet/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Pure Rust GRIB2 processor -- 5x faster than cfgrib.**

rustmet downloads and decodes GRIB2 data from operational weather models using pure Rust. It ships with Python bindings via PyO3, so you can drop it into any existing Python forecasting pipeline.

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

### End-to-end: fetch + decode

Downloading and decoding 8 HRRR surface variables (15M grid points), median of 5 runs:

| Tool | Download | Decode | Total |
|------|----------|--------|-------|
| **rustmet** | **1.5 s** | **0.11 s** | **1.6 s** |
| requests + cfgrib | 7.4 s | 0.42 s | 7.8 s |
| herbie + cfgrib | 8.1 s | 0.75 s | 8.9 s |
| xarray + cfgrib | -- | 0.39 s | -- |

rustmet is **5x faster end-to-end** than the standard Python stack. The download speedup comes from HTTP range requests with connection pooling; the decode speedup comes from a zero-copy Rust GRIB2 parser with JPEG2000 and deflate decompression.

### Competitive comparison

We also benchmark rustmet against cfgrib, MetPy, and scipy on equivalent operations (GRIB2 parsing, meteorological calculations, grid math). Results vary by system -- run the comparison yourself:

```bash
python benchmark/compare.py
```

The script handles missing libraries gracefully and prints a formatted table. MetPy's pint-based unit system adds overhead compared to raw numpy -- this is a deliberate design choice for dimensional safety, not a deficiency. See [benchmark/RESULTS.md](benchmark/RESULTS.md) for detailed Criterion results and comparative numbers.

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
- **NumPy integration** -- decoded grids are returned directly as numpy arrays
- **xarray support** -- `rustmet.to_xarray()` gives you a labeled Dataset with coordinates
- **Parallel decoding** -- multi-message files are decoded across threads with rayon
- **Cross-platform** -- Linux, macOS, Windows; x86_64 and aarch64

## Test Suite

rustmet ships with a comprehensive test suite of **510+ tests** covering every layer of the stack:

```bash
cargo test                    # Run all 510+ tests
cargo test -p rustmet-core    # Core library tests only
cargo bench                   # Criterion benchmarks
```

- **GRIB2 parser tests** -- section parsing, template decoding, bit-level unpacking, edge cases
- **Meteorological calculations** -- CAPE, CIN, helicity, storm motion, lapse rates, thermodynamic profiles validated against known values
- **Grid math** -- derivatives, divergence, vorticity, Laplacian on synthetic fields with analytic solutions
- **Projection tests** -- Lambert Conformal, Lat/Lon, Mercator, Polar Stereographic round-trip accuracy
- **Download/index tests** -- `.idx` parsing, byte-range computation, URL generation for all supported models
- **Rendering tests** -- colormap interpolation, contour generation, wind barb geometry, PNG output validation
- **Python bindings** -- PyO3 integration tests for fetch, decode, and xarray conversion

Benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) and cover GRIB2 decoding throughput, JPEG2000 decompression, grid interpolation, and colormap lookup performance.

## Project Structure

```
rustmet/
  crates/
    rustmet-core/    # Pure Rust library: GRIB2 parser, HTTP client, model definitions
    rustmet-py/      # Python bindings (PyO3 + maturin)
  src/               # CLI binary
  benchmark/         # Benchmark suite
```

## License

MIT
