# ML Mesocyclone Detector — Training Data Pipeline

Context doc for the ML agent. This explains how to use rustmet's existing infrastructure to gather and label training data for a lightweight mesocyclone classification model.

## Goal

Replace hand-tuned thresholds in our rotation detection with a small MLP (8-16 features → 16 → 8 → 1 sigmoid) that scores mesocyclone candidates. The algorithmic detector stays as the candidate generator; the MLP replaces the threshold-based classification step.

## Architecture

```
rustmet/
├── crates/
│   ├── wx-radar/src/cells.rs        # SCIT cell identification (pure Rust)
│   ├── wx-radar/src/level2.rs       # NEXRAD Level 2 parser (pure Rust)
│   ├── wx-radar/src/detection.rs    # Mesocyclone/TVS type definitions
│   ├── wx-pro/src/cmd_rotation.rs   # Current algorithmic meso detector
│   └── wx-pro/src/cmd_storm_analysis.rs  # Multi-frame cell tracking
├── target/release/
│   ├── wx-pro.exe                   # CLI binary (all commands below)
│   └── wx-mcp.exe                   # MCP server (exposes same tools to LLMs)
```

All binaries are pure Rust, statically linked, ~5MB. No Python, no C deps.

## Data Sources

### 1. NEXRAD Level 2 Archive (AWS S3, free, no auth)

Public bucket: `s3://unidata-nexrad-level2/` (also HTTPS)

URL pattern:
```
https://unidata-nexrad-level2.s3.amazonaws.com/{YYYY}/{MM}/{DD}/{SITE}/{SITE}{YYYYMMDD}_{HHMMSS}_V06
```

Example:
```
https://unidata-nexrad-level2.s3.amazonaws.com/2024/04/26/KTLX/KTLX20240426_220315_V06
```

List files for a site/date:
```
https://unidata-nexrad-level2.s3.amazonaws.com?list-type=2&prefix=2024/04/26/KTLX
```

Returns XML with `<Key>` elements. Filter out `_MDM` (metadata) files. Volume scans are ~5-15 MB each, one every ~4-5 minutes.

141 operational NEXRAD sites — full list with lat/lon at `crates/wx-radar/src/sites.rs`.

### 2. SPC Storm Reports (tornado, hail, wind)

CSV files by date, one row per report:
```
https://www.spc.noaa.gov/climo/reports/{YYMMDD}_rpts_filtered_torn.csv
https://www.spc.noaa.gov/climo/reports/{YYMMDD}_rpts_filtered_hail.csv
https://www.spc.noaa.gov/climo/reports/{YYMMDD}_rpts_filtered_wind.csv
```

Tornado CSV columns: `Time,F_Scale,Location,County,State,Lat,Lon,...`
- Time is in local (CST/CDT typically), needs timezone conversion
- Lat/Lon gives the tornado report location
- F_Scale gives intensity (EF0-EF5)

### 3. SPC Mesoscale Discussions & Watches

Available through `wx-pro severe --state XX` but less useful for point-level labeling.

## Using wx-pro to Extract Features

### Download + parse a radar volume
```bash
wx-pro radar --site KTLX
# Returns JSON: sweeps, max_ref, velocity stats, file metadata
```

### Run rotation detection (current algorithmic approach)
```bash
wx-pro rotation --site KTLX --pretty
# Returns JSON:
# {
#   "raw_candidates": 847,          # gates passing initial shear threshold
#   "sweep_analysis": [...],         # per-tilt detection counts
#   "detections": {
#     "mesocyclone_count": 2,
#     "tvs_count": 0,
#     "items": [{
#       "lat": 35.12, "lon": -97.44,
#       "azimuth": 242.3, "range_km": 45.2,
#       "rotational_velocity_ms": 28.5,
#       "max_gate_to_gate_ms": 57.0,
#       "strength_rank": 3,
#       "is_tvs": true,
#       "gate_count": 12
#     }]
#   }
# }
```

### Run cell identification + tracking across frames
```bash
wx-pro storm-analysis --site KTLX --frames 5 --pretty
# Returns JSON: cells with motion vectors, meso associations, time series
```

### Render labeled image
```bash
wx-pro storm-image --site KTLX --size 800
# Returns JSON with image_path + cell/meso data
```

## Feature Extraction for ML

The rotation detector at `cmd_rotation.rs` already computes per-candidate features. For each detection that survives clustering + vertical continuity, we have:

| Feature | Source | Notes |
|---------|--------|-------|
| `rotational_velocity` | max Vrot in cluster | m/s, higher = stronger |
| `max_shear` | max gate-to-gate delta V | m/s, signed (positive = cyclonic NH) |
| `gate_count` | cluster size | # of adjacent shear gates |
| `range_km` | distance from radar | affects beam width, resolution |
| `azimuth` | compass bearing | not directly useful, but affects beam geometry |
| `elevation` | tilt angle | lower = closer to ground |
| `tilt_count` | vertical continuity depth | # of elevation tilts detection appears on (3-4 typical) |
| `co-located reflectivity` | max dBZ at detection location | from REF product |

**Additional features we could extract** (require minor code changes to `cmd_rotation.rs`):
- `spectrum_width` — from SW product at same gate; high SW = turbulent
- `correlation_coefficient` (rhoHV) — from RHO product; low = non-meteorological
- `differential_reflectivity` (ZDR) — from ZDR product; rain vs hail discrimination
- `azimuthal_extent_deg` — angular width of shear signature
- `range_extent_km` — radial depth of shear signature
- `max_ref_in_cell` — if associated with a storm cell
- `cell_area_km2` — size of parent cell
- `distance_to_cell_centroid_km` — meso proximity to cell core

All 6 dual-pol products are already parsed by `wx-radar/src/level2.rs`: REF, VEL, SW, ZDR, RHO (CC), PHI (KDP).

## Training Data Labeling Strategy

### Positive labels (confirmed mesocyclones)

1. **SPC tornado reports** — download CSV for high-impact severe days
2. For each tornado report (lat, lon, time):
   - Find nearest NEXRAD site (our `sites.rs` has all 141 with coordinates)
   - Download the volume scan closest in time (±5 min) from S3
   - Run rotation detection on that volume
   - Any detection within 15 km of the tornado report location = **positive**
3. For EF2+ tornadoes, also grab ±2 volume scans for temporal context

### Negative labels (false positives)

1. **Non-severe days** — pick random dates with no SPC reports in the region
2. Download volume scans from the same sites, run rotation detection
3. Any detection that survives the algorithm = **negative** (no tornado/meso occurred)
4. Also use detections from severe days that are >50 km from any report = **negative**

### High-value severe weather dates (many tornado reports, good data quality)

| Date | Event | Key Sites |
|------|-------|-----------|
| 2024-04-26 | Massive OK/NE outbreak | KTLX, KVNX, KINX, KUEX |
| 2024-05-07 | West TN tornadoes | KNQA, KOHX |
| 2024-12-10 | MS/AL tornadoes | KGWX, KBMX |
| 2023-03-31 | Midwest outbreak | KLSX, KILX, KIND |
| 2023-03-24 | MS rolling fork EF4 | KDGX, KJAN |
| 2022-04-05 | GA/SC tornadoes | KFFC, KCAE |
| 2022-03-30 | KS/OK outbreak | KICT, KTLX |
| 2021-12-10 | Quad-state tornado | KPAH, KNQA, KLZK |
| 2019-05-28 | Dayton OH EF4 | KIND, KILN |
| 2019-05-20 | Mangum OK EF3 | KFDR, KTLX |

### Sample pipeline (pseudocode)

```python
for date in severe_dates:
    # 1. Get tornado reports
    reports = fetch_spc_csv(date, type="torn")

    for report in reports:
        # 2. Find nearest radar
        site = nearest_nexrad(report.lat, report.lon)  # use sites.rs data

        # 3. List S3 files around report time
        files = list_s3(site, date)
        closest = find_closest_to_time(files, report.time_utc)

        # 4. Run wx-pro rotation on that file
        # (or parse the Level 2 directly in Python/Rust)
        result = subprocess.run(
            ["wx-pro", "rotation", "--site", site, "--pretty"],
            capture_output=True
        )
        detections = json.loads(result.stdout)["detections"]["items"]

        # 5. Label: any detection within 15 km of report = positive
        for det in detections:
            dist = haversine(det["lat"], det["lon"], report.lat, report.lon)
            label = 1 if dist < 15.0 else 0
            features = extract_features(det)
            dataset.append((features, label))
```

## Deployment Constraints

The trained model must be deployable as **pure Rust with zero dependencies**:
- Hardcoded weight matrices (f32 arrays)
- ReLU activation: `x.max(0.0)`
- Sigmoid output: `1.0 / (1.0 + (-x).exp())`
- No ONNX, no libtorch, no external files
- Inference must complete in <100 microseconds per candidate
- Total binary size increase: <50 KB of weights

The model gets compiled directly into `wx-pro` and `wx-mcp` binaries. Weights are `const` arrays in a Rust source file.

## Rust Level 2 Parser API (for direct feature extraction)

If the ML agent wants to write Rust code that directly extracts features from radar data instead of shelling out to `wx-pro`:

```rust
use wx_radar::level2::Level2File;
use wx_radar::products::RadarProduct;

let data = std::fs::read("KTLX20240426_220315_V06")?;
let l2 = Level2File::parse(&data)?;

for sweep in &l2.sweeps {
    // sweep.elevation_angle: f32
    // sweep.radials: Vec<RadialData>

    for radial in &sweep.radials {
        // radial.azimuth: f32 (degrees)
        // radial.elevation: f32

        for moment in &radial.moments {
            // moment.product: RadarProduct (Reflectivity, Velocity, SpectrumWidth, ZDR, RHO, PHI)
            // moment.gate_count: usize
            // moment.first_gate_range: u32 (meters)
            // moment.gate_size: u32 (meters)
            // moment.data: Vec<f32> (values per gate, NaN = no data)
        }
    }
}
```

Cell identification:
```rust
use wx_radar::cells::identify_cells;
let cells = identify_cells(&sweep, Some(site_lat), Some(site_lon));
// Returns Vec<StormCell> with centroid, area, max_ref, etc.
```

## Summary

- NEXRAD data: free on S3, easy HTTP fetch, our parser handles it
- SPC reports: free CSVs, give lat/lon/time for tornado labels
- Feature extraction: `wx-pro rotation` already outputs everything needed as JSON
- Dual-pol features: parsed but not yet surfaced in rotation output (easy to add)
- Deployment: hardcoded weights as const arrays in Rust, <100us inference
