# Migrating from cfgrib to rustmet

This guide shows how to replace common cfgrib patterns with rustmet equivalents.
rustmet parses GRIB2 natively in Rust with no ecCodes dependency, making it
faster and easier to install.

## Opening a GRIB2 File

### cfgrib

```python
import cfgrib
import xarray as xr

# Open all datasets (cfgrib splits by level type automatically)
datasets = cfgrib.open_datasets("hrrr.grib2")

# Or via xarray
ds = xr.open_dataset("hrrr.grib2", engine="cfgrib",
                      backend_kwargs={"indexpath": ""})
```

### rustmet

```python
import rustmet

# Open and parse the entire file (returns a GribFile with all messages)
grib = rustmet.open("hrrr.grib2")

# See what's inside
print(f"{grib.num_messages} messages")
for line in grib.inventory():
    print(line)
```

**Key difference:** cfgrib creates xarray Datasets grouped by level type.
rustmet gives you a flat list of messages. You pick the ones you need with
`find()`, `search()`, or `filter()`.


## Finding Specific Fields

### cfgrib

```python
# Open only 2m temperature
ds = xr.open_dataset("hrrr.grib2", engine="cfgrib",
    backend_kwargs={
        "filter_by_keys": {
            "shortName": "2t",
            "typeOfLevel": "heightAboveGround",
            "level": 2,
        }
    })
t2m = ds["t2m"].values  # numpy array
```

### rustmet

```python
grib = rustmet.open("hrrr.grib2")

# Option 1: find() -- exact match on variable name, optional level string
msg = grib.find("TMP", "2 m above ground")

# Option 2: search() -- fuzzy human-readable query, returns ranked list
results = grib.search("temperature 2m")
msg = results[0]

# Option 3: filter() -- returns a new GribFile with matching messages
surface_temps = grib.filter("TMP", "above ground")

# Unpack the data
values = msg.values()      # 1D numpy array (f64)
values_2d = msg.values_2d()  # 2D numpy array shaped (ny, nx)
```


## Reading Specific Levels

### cfgrib

```python
# 500 hPa geopotential height
ds = xr.open_dataset("gfs.grib2", engine="cfgrib",
    backend_kwargs={
        "filter_by_keys": {
            "shortName": "gh",
            "typeOfLevel": "isobaricInhPa",
            "level": 500,
        }
    })
z500 = ds["gh"].values
```

### rustmet

```python
grib = rustmet.open("gfs.grib2")

# Fuzzy search understands pressure level shorthand
msg = grib.search("500mb height")[0]

# Or use find() with the level string
msg = grib.find("HGT", "500 isobaric")

# Access metadata
print(msg.variable)      # "HGT"
print(msg.level)         # "500 isobaric (Pa)"
print(msg.level_value)   # 500.0
print(msg.level_type)    # "isobaric (Pa)"
print(msg.units)         # "gpm"
print(msg.nx, msg.ny)    # grid dimensions
print(msg.forecast_time) # forecast hour
```


## Getting Lat/Lon Coordinates

### cfgrib

```python
ds = xr.open_dataset("hrrr.grib2", engine="cfgrib")
lats = ds.latitude.values
lons = ds.longitude.values
```

### rustmet

```python
grib = rustmet.open("hrrr.grib2")
msg = grib.messages[0]

lats = msg.lats()  # 1D numpy array
lons = msg.lons()  # 1D numpy array

# For 2D plotting, reshape to grid
lats_2d = lats.reshape(msg.ny, msg.nx)
lons_2d = lons.reshape(msg.ny, msg.nx)
```


## Downloading Model Data (No Local File Needed)

cfgrib requires you to download files separately. rustmet has a built-in
download client that fetches from AWS, Google, NOMADS, and Azure with
automatic fallback and caching.

```python
import rustmet

# Download latest HRRR 2m temperature (uses byte-range requests for speed)
data = rustmet.fetch("hrrr", "latest",
                     vars=["TMP:2 m above ground"])
msg = data.messages[0]
t2m = msg.values_2d()

# Download multiple forecast hours in parallel
series = rustmet.fetch("hrrr", "2026-03-09/12z",
                       fhour=[0, 1, 2, 3, 6, 12],
                       vars=["TMP:2 m above ground"])
# series is a list of GribFile objects, one per forecast hour
```


## Unit Conversion

### cfgrib

```python
# cfgrib returns SI units, manual conversion needed
t2m_celsius = ds["t2m"].values - 273.15
```

### rustmet

```python
# Built-in unit conversion
vals_k = msg.values()
vals_c = rustmet.convert_units(vals_k, "K", "C")
vals_f = rustmet.convert_units(vals_k, "K", "F")

# Supports: K/C/F, m/s/kt/mph/km_h, Pa/hPa/mb/inHg, m/ft/km, kg_m2/mm/in
```


## Field Statistics

### cfgrib

```python
import numpy as np
print(np.nanmin(data), np.nanmax(data), np.nanmean(data))
```

### rustmet

```python
vals = msg.values()
stats = rustmet.field_stats(vals)
print(f"min={stats['min']:.1f}  max={stats['max']:.1f}  mean={stats['mean']:.1f}")
print(f"std_dev={stats['std_dev']:.2f}  count={stats['count']}  nan={stats['nan_count']}")
```


## Performance Tips

1. **Byte-range downloads.** When using `rustmet.fetch()` with `vars=`,
   rustmet downloads only the matching GRIB2 messages using HTTP range
   requests against the .idx index file. A full HRRR surface file is ~100 MB;
   fetching just 2m temperature pulls ~2 MB.

2. **Parallel forecast hours.** Pass a list to `fhour=` and rustmet downloads
   all hours in parallel using Rayon threads:
   ```python
   files = rustmet.fetch("hrrr", "latest", fhour=list(range(49)))
   ```

3. **Caching.** The `Client` class caches downloads to disk automatically.
   Repeated fetches hit the cache instead of the network:
   ```python
   client = rustmet.Client()  # cache at default location
   data = client.fetch("hrrr", "2026-03-09/12z", fhour=0)
   # Second call is instant
   data = client.fetch("hrrr", "2026-03-09/12z", fhour=0)
   ```

4. **No index files.** Unlike cfgrib, rustmet does not write `.idx` files to
   disk. The GRIB2 index is built in memory during parsing.

5. **Parsing from bytes.** If you already have GRIB2 data in memory (e.g.,
   from an S3 download), skip the filesystem:
   ```python
   grib = rustmet.GribFile.from_bytes(raw_bytes)
   ```

6. **Smoothing in Rust.** Instead of scipy gaussian_filter, use the built-in:
   ```python
   smoothed = rustmet.smooth(vals, nx, ny, sigma=2.0)
   ```


## Quick Reference

| cfgrib / xarray | rustmet |
|---|---|
| `cfgrib.open_datasets(path)` | `rustmet.open(path)` |
| `xr.open_dataset(path, engine="cfgrib", ...)` | `rustmet.open(path)` then `.find()` or `.search()` |
| `filter_by_keys={"shortName": "2t", ...}` | `grib.search("temperature 2m")` |
| `ds["t2m"].values` | `msg.values_2d()` |
| `ds.latitude.values` | `msg.lats()` |
| `ds.longitude.values` | `msg.lons()` |
| Manual HTTP download + cfgrib | `rustmet.fetch("hrrr", "latest", ...)` |
| `data - 273.15` | `rustmet.convert_units(data, "K", "C")` |
