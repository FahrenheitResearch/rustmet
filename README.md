# rustmet

[![CI](https://github.com/FahrenheitResearch/rustmet/actions/workflows/ci.yml/badge.svg)](https://github.com/FahrenheitResearch/rustmet/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Pure Rust weather platform — GRIB2 engine, NEXRAD radar, NWS integration, MCP tool server, and HTTP tile service. Zero C dependencies. Single static binaries.**

rustmet is a 50,000+ LOC workspace of 16 crates that covers the full stack from raw GRIB2 byte decoding to AI agent tool integration. It downloads operational NWP model data (HRRR, GFS, NAM, RAP), parses NEXRAD Level 2 radar, fetches NWS alerts/forecasts/METARs, renders 256x256 XYZ map tiles with bilinear interpolation, serves them over HTTP, and exposes 22 weather tools via the Model Context Protocol (MCP) for LLM agents.

## Binaries

Pre-built binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64), and Windows (x64) are attached to each [GitHub Release](https://github.com/FahrenheitResearch/rustmet/releases).

| Binary | Size | Description |
|--------|------|-------------|
| **wx** | 4.0 MB | Weather CLI — 17 commands, JSON output for agent pipelines |
| **wx-pro** | 6.0 MB | Full-power CLI — 23 commands including MRMS composites, rotation detection, storm cell tracking, imagery rendering, watch-box geometry |
| **wx-lite** | 3.6 MB | Bandwidth-optimized CLI — 12 commands, response caching, Open-Meteo global coverage |
| **wx-mcp** | 694 KB | MCP server — exposes 22 weather tools over stdio for Claude, Hermes, or any MCP client |
| **wx-server** | 5.8 MB | HTTP tile server — Axum-based REST API with in-process GRIB2 rendering, SSE streaming, and a self-contained web dashboard |

All binaries are statically linked with pure Rust TLS (rustls). No OpenSSL, no libc version requirements, no runtime dependencies. Copy the binary and run it.

### Build from source

```bash
git clone https://github.com/FahrenheitResearch/rustmet.git
cd rustmet
cargo build --release -p wx-agent -p wx-pro -p wx-lite -p wx-mcp -p wx-server
```

Binaries appear in `target/release/`. Requires Rust 1.75+.

## Core Library

rustmet-core is the foundation crate used by all binaries. It provides:

### GRIB2 Engine
- Zero-copy parser supporting grid templates 0 (Lat/Lon), 10 (Mercator), 20 (Polar Stereographic), 30 (Lambert Conformal), 1 (Rotated Lat/Lon)
- Data representation templates 5.0 (simple packing), 5.40 (JPEG2000), 5.41 (PNG)
- HTTP byte-range downloads — fetches only the GRIB2 messages matching your variable/level filter via `.idx` file parsing
- Connection pooling with retry, exponential backoff, and disk caching
- Complete WMO parameter tables for discipline 0 (meteorological) and 10 (oceanographic)

### Supported Models

| Model | Resolution | Coverage | Update Cycle | Source |
|-------|-----------|----------|-------------|--------|
| **HRRR** | 3 km | CONUS | Hourly | NOMADS |
| **GFS** | 0.25° | Global | 6-hourly | NOMADS |
| **NAM** | 12 km | North America | 6-hourly | NOMADS |
| **RAP** | 13 km | CONUS | Hourly | NOMADS |
| **MRMS** | 1 km | CONUS | 2-minute | MRMS server |

### Meteorological Calculations (45+)

**Thermodynamics:** potential temperature, equivalent potential temperature, wet-bulb temperature, virtual temperature, mixing ratio, vapor pressure, saturation vapor pressure, LCL, LFC, EL

**Stability:** CAPE (surface-based, mixed-layer, most-unstable), CIN, lifted index, Showalter index, K-index, Total Totals, Significant Tornado Parameter, Energy-Helicity Index, Supercell Composite Parameter

**Kinematics:** storm-relative helicity (0-1 km, 0-3 km), Bunkers storm motion, bulk wind shear, mean wind, storm-relative wind

**Grid math:** first/second derivatives, divergence, vorticity (relative, absolute, curvature, shear), advection, Laplacian, deformation, frontogenesis, Gaussian and N-point smoothing

**Projections:** Lambert Conformal Conic, Equidistant Cylindrical, Polar Stereographic, Mercator, Rotated Lat/Lon — forward and inverse transforms with grid-index mapping

### Rendering Engine
- Bilinear interpolation onto 256x256 XYZ tiles
- 14 colormaps (temperature, dewpoint, reflectivity, CAPE, wind, RH, STP, EHI, lapse rate, updraft helicity, precip, vorticity, simulated IR, geopotential anomaly)
- Marching-squares contour generation with anti-aliased lines
- Wind barb rendering with filled pennants
- Skew-T / Log-P sounding diagrams
- NEXRAD polar-to-cartesian radar rendering
- Near-range radar quality control (10 km minimum range, elevation filtering, spatial consistency checks)

## NEXRAD Radar

The `wx-radar` crate parses NEXRAD WSR-88D Level 2 archive files:

- All 6 dual-pol products: REF, VEL, SW, ZDR, RHO (CC), PHI (KDP)
- 141 operational radar sites with metadata (lat/lon, name, region)
- BZ2 decompression of compressed volume scans
- Polar-to-cartesian tile rendering for XYZ map overlays
- Quality-controlled max reflectivity and gate-to-gate velocity analysis
- Rotation detection type definitions for mesocyclone identification
- SCIT-style storm cell identification with multi-threshold watershed segmentation
- Cell tracking across volume scans with motion vector computation

Data sources: AWS S3 real-time archive and NOMADS historic scans.

## NWS Integration

| Data | Crate | Source |
|------|-------|--------|
| Active alerts/warnings | wx-alerts | api.weather.gov |
| SPC outlooks, watches, mesoscale discussions | wx-alerts | SPC RSS/API |
| 7-day and hourly forecasts | wx-obs | api.weather.gov |
| METAR/TAF observations | wx-obs | aviationweather.gov |
| Upper-air soundings | wx-sounding | weather.uwyo.edu (Wyoming) |
| Station metadata | wx-obs | api.weather.gov |

## MCP Tool Server (wx-mcp)

`wx-mcp` implements the [Model Context Protocol](https://modelcontextprotocol.io/) (version 2024-11-05) over stdio, exposing 22 weather tools to any MCP-compatible LLM agent. It auto-detects framing (Content-Length headers or JSON Lines).

`wx-mcp` is stateless — each tool call spawns `wx-pro` or `wx-lite` as a subprocess, captures the JSON output, and returns it as an MCP text result. This means tool execution inherits all the capabilities of the full CLI.

### Tool Reference

**Lightweight (< 50 KB)**

| Tool | Description |
|------|-------------|
| `wx_metar` | Raw/decoded METAR for any ICAO station |
| `wx_conditions` | Current obs + active alerts for a lat/lon |
| `wx_station` | Station metadata, nearest-station search |
| `wx_alerts` | Active NWS alerts by point or state |
| `wx_hazards` | Categorized hazard assessment (tornado, flood, fire, winter) |

**Medium (50-200 KB)**

| Tool | Description |
|------|-------------|
| `wx_brief` | Ultra-compact briefing — answers most weather questions |
| `wx_forecast` | NWS 7-day or hourly forecast |
| `wx_global` | International weather via Open-Meteo |
| `wx_history` | Historical observations with trends |
| `wx_severe` | SPC outlooks, watches, mesoscale discussions |
| `wx_evidence` | Multi-source confidence assessment (METAR vs HRRR vs NWS) |

**Heavy (1 MB+)**

| Tool | Description |
|------|-------------|
| `wx_sounding` | HRRR/RAP model-derived convective parameters (CAPE, SRH, shear) |
| `wx_radar` | Full NEXRAD volume scan analysis (~15 MB download) |
| `wx_briefing` | Complete severe weather briefing: METAR + alerts + SPC + radar |

**Visualization**

| Tool | Description |
|------|-------------|
| `wx_tiles` | XYZ PNG tiles for Leaflet/Mapbox overlays |
| `wx_radar_image` | Rendered NEXRAD PPI as PNG file |
| `wx_model_image` | Rendered model field (CAPE, temp, etc.) as PNG |
| `wx_point` | Single grid-point value extraction |
| `wx_scan` | Grid extrema search (max CAPE, min pressure, etc.) |
| `wx_timeseries` | Multi-hour trend for a variable at a point |

**Storm Analysis**

| Tool | Description |
|------|-------------|
| `wx_storm_analysis` | SCIT-style multi-frame cell tracking with mesocyclone association and motion vectors |
| `wx_storm_image` | Rendered storm cell PNG with labeled cells, meso markers, info box, and legend |

### MCP Integration Example

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools/call",
 "params": {"name": "wx_sounding", "arguments": {"lat": 35.2, "lon": -97.4}}}
```

Returns HRRR-derived SBCAPE, MLCAPE, CIN, 0-1km SRH, 0-3km SRH, bulk shear, lifted index, PWAT, surface obs, and storm motion vectors.

## HTTP Tile Server (wx-server)

`wx-server` is an Axum-based HTTP server that renders weather data as 256x256 PNG tiles on demand. It downloads GRIB2 data from NOMADS, caches decoded fields in memory, and renders tiles with bilinear interpolation using configurable colormaps.

### Endpoints

```
GET /tiles/{model}/{var}/{level}/f{hour}/{z}/{x}/{y}.png?style=nws&run=YYYYMMDD/HHz
GET /tiles/contour/{model}/{var}/{level}/f{hour}/{z}/{x}/{y}.png?interval=60
GET /tiles/wind/{model}/{level}/f{hour}/{z}/{x}/{y}.png
GET /tiles/radar/{site}/ref/{z}/{x}/{y}.png?scan=FILENAME
GET /tiles/surface/{z}/{x}/{y}.png

GET /api/value?model=hrrr&var=tmp&level=2m&fhour=f00&lat=35&lon=-97
GET /api/runs/{model}
GET /api/radar/scans/{site}
GET /api/sounding/{station}
GET /api/conditions?lat=35&lon=-97
GET /api/legend/{colormap}?style=nws&vmin=-40&vmax=120
GET /api/cache/stats
GET /api/status
GET /events?types=model_run,alert

GET /health
GET /                          # Self-contained web dashboard
```

### Features
- In-process GRIB2 download and decode with field-level caching
- Thundering herd prevention via `tokio::sync::Notify`
- Run cache with 2-minute TTL for latest model init times from NOMADS
- Historic model run selection via `?run=` query parameter
- Historic radar scan selection via `?scan=` parameter
- Smooth tile animation support: tiles are designed for `swapLayer()` pattern where new layers load before old ones are removed
- Transparent low-value pixels (near-black values get alpha=0 so the basemap shows through)
- CORS headers for cross-origin embedding
- SSE event stream for real-time model run and alert notifications

### Web Dashboard

`wx-server` serves a self-contained HTML dashboard at `/` with:
- 48 map products across 7 categories (surface, precipitation, instability, wind shear, composites, upper air, winter)
- 131 NEXRAD radar sites organized by 8 regions with search
- Model run selection (date + cycle dropdown) with real init times from NOMADS
- Radar scan time selection from NOMADS directory listing
- Forecast hour animation with prefetching
- Wind barb and contour overlays with adjustable interval
- Mouse readout with interpolated values and unit conversion
- SkewT sounding modal
- Leaflet map with dark basemap

The dashboard is a single `index.html` file — no build step, no npm, no bundler. Also available at `examples/web-dashboard/index.html`.

## Python Bindings

```bash
pip install rustmet
```

```python
import rustmet

# Download and decode HRRR 2m temperature
grib = rustmet.fetch("hrrr", "2026-03-09/00z", vars=["TMP:2 m above ground"])
data = grib.messages[0].values_2d()  # numpy array (ny, nx)

# Convert to xarray Dataset
ds = rustmet.to_xarray(grib)
```

78 `#[pyfunction]` exports cover all meteorological calculations. Built with PyO3 + maturin.

## Benchmarks

### GRIB2 Decoding

| Operation | rustmet | Comparison | Speedup |
|-----------|---------|------------|---------|
| Open/parse GRIB | 5.16 ms | ecCodes 5.75 ms | 1.1x |
| Unpack all messages | 72.00 ms | ecCodes 77.16 ms | 1.07x |
| Open/parse GRIB | 5.16 ms | cfgrib 195.42 ms | 37.9x |

### Meteorological Compute

| Function | rustmet | Comparison | Speedup |
|----------|---------|------------|---------|
| Gaussian smooth (sigma=2, 400x400) | 906.5 us | SciPy 1.32 ms | 1.45x |
| Equivalent potential temperature (10k pts) | 302.3 us | MetPy 752.4 us | 2.49x |
| Mixing ratio | — | MetPy | ~9x |
| Vapor pressure | — | MetPy | ~5x |
| Dewpoint from RH | — | MetPy | ~3x |

### End-to-End: Fetch + Decode

8 HRRR surface variables (15M grid points), median of 5 runs:

| Tool | Download | Decode | Total |
|------|----------|--------|-------|
| **rustmet** | **1.5 s** | **0.11 s** | **1.6 s** |
| requests + cfgrib | 7.4 s | 0.42 s | 7.8 s |
| herbie + cfgrib | 8.1 s | 0.75 s | 8.9 s |

Decoded values match ecCodes output (`numpy.allclose=True`) on all tested HRRR and synthetic cases.

## Validated Against

- **ecCodes** (ECMWF) — GRIB2 decode values via `numpy.allclose` on real HRRR and synthetic grids
- **cfgrib** — GRIB2 decode cross-check
- **MetPy** — thermodynamic functions (theta, theta-e, dewpoint, mixing ratio, vapor pressure)
- **SciPy** — Gaussian smoothing kernel
- **NumPy** — finite-difference vorticity and divergence

510+ tests across all modules. Run with `cargo test`.

## Workspace Structure

```
rustmet/
├── crates/
│   ├── rustmet-core/     # GRIB2 parser, download client, model configs, colormaps
│   ├── rustmet-py/       # Python bindings (PyO3 + maturin)
│   ├── wx-agent/         # wx CLI binary (17 commands)
│   ├── wx-pro/           # wx-pro CLI binary (23 commands)
│   ├── wx-lite/          # wx-lite CLI binary (12 commands)
│   ├── wx-mcp/           # MCP server binary (22 tools)
│   ├── wx-server/        # HTTP tile server binary
│   ├── wx-radar/         # NEXRAD Level 2 parser, 141 sites
│   ├── wx-alerts/        # NWS alerts, SPC outlooks
│   ├── wx-obs/           # METAR/TAF parser and fetcher
│   ├── wx-sounding/      # Upper-air sounding fetcher
│   ├── wx-field/         # Shared field/projection types
│   ├── wx-math/          # Meteorological computations
│   ├── wx-render/        # Visualization and rendering
│   ├── wx-io/            # GRIB2, NetCDF, NEXRAD I/O
│   └── wx-ui/            # Desktop GUI (egui)
├── examples/
│   ├── hermes-agent/     # AI weather agent deployment guide
│   └── web-dashboard/    # Self-contained weather dashboard
├── deploy/               # Docker, systemd, nginx configs
├── benchmark/            # Cross-library comparison benchmarks
└── docs/                 # Migration guides (cfgrib, MetPy)
```

## Deployment

### Docker

```bash
cd deploy
docker compose up -d
```

The Docker image builds `wx-server`, `wx-pro`, and `wx-lite` in a multi-stage build. Runtime image is `debian:bookworm-slim`. See `deploy/README.md` for nginx reverse proxy, systemd service files, and environment variable reference.

### AI Agent Integration

See `examples/hermes-agent/` for a complete deployment guide using Hermes Agent with Nemotron-3-Super-120B via OpenRouter (free tier). The agent uses `wx-mcp` to access all 22 weather tools, runs on a cron schedule, and pushes alerts via Telegram.

Architecture:
```
LLM (OpenRouter / any OpenAI-compatible endpoint)
  └─ Hermes Agent (memory, skills, cron)
       └─ wx-mcp (MCP stdio)
            └─ wx-pro / wx-lite (GRIB2, NEXRAD, NWS)
                 └─ NOAA / NWS / AWS S3
```

Any MCP-compatible agent framework works — the `wx-mcp` binary speaks standard MCP 2024-11-05 protocol.

## License

MIT — see [LICENSE](LICENSE).
