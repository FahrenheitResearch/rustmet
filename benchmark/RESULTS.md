# Rustmet Criterion Benchmark Results

Run date: 2026-03-10
Platform: Windows 11, release profile (opt-level=3, LTO, codegen-units=1)

## Meteorological Functions (1,000-element arrays)

| Benchmark | Time (median) | Per-element | Notes |
|-----------|---------------|-------------|-------|
| thetae | 54.31 us | 54.3 ns | Equivalent potential temperature |
| mixratio | 3.63 us | 3.6 ns | Mixing ratio from P and T |
| potential_temperature | 9.21 us | 9.2 ns | Theta from P and T |
| wet_bulb_temperature | 103.91 us | 103.9 ns | Iterative wet-bulb solve |

## Dynamics (100x100 grid = 10,000 points, dx=dy=3km)

| Benchmark | Time (median) | Per-gridpoint | Notes |
|-----------|---------------|---------------|-------|
| vorticity | 8.30 us | 0.83 ns | Relative vorticity from u,v |
| divergence | 8.27 us | 0.83 ns | Horizontal divergence from u,v |
| advection | 9.91 us | 0.99 ns | Scalar advection (T by u,v) |
| laplacian | 15.82 us | 1.58 ns | 2D Laplacian of scalar field |
| total_deformation | 24.60 us | 2.46 ns | Stretching + shearing deformation |

## Smoothing (200x200 grid = 40,000 points)

| Benchmark | Time (median) | Per-gridpoint | Notes |
|-----------|---------------|---------------|-------|
| gaussian sigma=2 | 699.76 us | 17.5 ns | Gaussian smoother, sigma=2 |
| gaussian sigma=5 | 1.488 ms | 37.2 ns | Gaussian smoother, sigma=5 |
| 9-point 1-pass | 145.98 us | 3.6 ns | 9-point stencil, 1 pass |
| 5-point 3-pass | 226.64 us | 5.7 ns | 5-point stencil, 3 passes |

## GRIB2 Round-trip (50x50 grid = 2,500 values, simple packing 16-bit)

| Benchmark | Time (median) | Throughput | Notes |
|-----------|---------------|------------|-------|
| write | 57.22 us | 43.7 M vals/s | Encode 2500 values to GRIB2 bytes |
| parse | 142.93 ns | 17.5 G vals/s | Parse headers only (no unpack) |
| parse + unpack | 20.87 us | 119.8 M vals/s | Parse + unpack data values |
| 5-msg roundtrip | 102.29 us | 122.2 M vals/s | Parse + unpack 5 messages (12,500 vals) |

## Search (10 GRIB2 messages)

| Benchmark | Time (median) | Notes |
|-----------|---------------|-------|
| search "temperature" | 2.71 us | Matches by parameter name |
| search "wind" | 3.00 us | Matches wind-related fields |
| search "500 mb" | 2.89 us | Matches by pressure level |
| search (no match) | 2.13 us | No matching messages |
