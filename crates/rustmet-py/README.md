# rustmet

[![PyPI](https://img.shields.io/pypi/v/rustmet.svg)](https://pypi.org/project/rustmet/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/FahrenheitResearch/rustmet/blob/main/LICENSE)

Fast GRIB2 processor for weather models -- pure Rust with Python bindings, 5x faster than cfgrib.

No system dependencies required. No eccodes, no libgrib. Just `pip install rustmet`.

## Install

```bash
pip install rustmet
```

## Quick Start

```python
import rustmet

# Fetch HRRR 2m temperature (downloads only the bytes you need)
grib = rustmet.fetch("hrrr", "2026-03-09/00z",
                     vars=["TMP:2 m above ground"])

# Get numpy arrays
msg = grib.messages[0]
data = msg.values_2d()    # shape (ny, nx)
lats = msg.lats()
lons = msg.lons()

# Or convert to xarray Dataset
ds = rustmet.to_xarray(grib)
```

## Supported Models

- **HRRR** — 3km CONUS, hourly
- **GFS** — 0.25° global, 6-hourly
- **NAM** — 12km CONUS, 6-hourly
- **RAP** — 13km CONUS, hourly

## API

### Module-level functions

- `rustmet.fetch(model, run, fhour=0, product="prs", vars=None)` — Download and parse GRIB2 data
- `rustmet.open(path)` — Parse a local GRIB2 file
- `rustmet.products()` — List available product definitions
- `rustmet.to_xarray(grib)` — Convert GribFile to xarray Dataset

### Classes

- `rustmet.Client(cache_dir=None)` — HTTP client with caching
  - `.fetch(model, run, fhour, product, vars)` — Download and parse
  - `.inventory(model, run, fhour)` — List available variables
  - `.url(model, run, fhour, product)` — Get download URL
- `rustmet.GribFile` — Parsed GRIB2 file
  - `.messages` — List of GribMessage objects
  - `.find(variable, level=None)` — Search messages
  - `.inventory()` — Summary of all messages
- `rustmet.GribMessage` — Single decoded field
  - `.values()` — 1D numpy array
  - `.values_2d()` — 2D numpy array (ny, nx)
  - `.lats()`, `.lons()` — Coordinate arrays
  - `.variable`, `.level`, `.units` — Metadata

## Benchmark

```
FULL PIPELINE (download + decode)
Tool                        Download     Decode      Total    Speedup
rustmet (Rust)                 1.5s       90ms      1.6s       5.5x
herbie+cfgrib                  8.1s      747ms      8.9s  1.0x (base)
```

## License

MIT
