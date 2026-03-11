# Migrating from MetPy to rustmet

This guide maps MetPy's thermodynamic and calculation functions to their
rustmet equivalents. Both libraries use the Bolton (1980) formulation for
saturation vapor pressure (equation 10) and equivalent potential temperature
(equation 39), so results match closely.

## Scalar Thermodynamic Functions

These operate on single float values. All temperatures are in Celsius,
pressures in hPa, unless noted.

| MetPy | rustmet | Notes |
|---|---|---|
| `mpcalc.potential_temperature(p, T)` | `rustmet.potential_temperature(p_hpa, t_c)` | Returns K |
| `mpcalc.equivalent_potential_temperature(p, T, Td)` | `rustmet.equivalent_potential_temperature(p_hpa, t_c, td_c)` | Returns K. Bolton (1980) eq 39 |
| `mpcalc.saturation_vapor_pressure(T)` | `rustmet.saturation_vapor_pressure(t_c)` | Returns hPa. Bolton (1980) eq 10 |
| `mpcalc.dewpoint_from_relative_humidity(T, rh)` | `rustmet.dewpoint_from_rh(t_c, rh)` | rh in percent (0-100) |
| `mpcalc.relative_humidity_from_dewpoint(T, Td)` | `rustmet.rh_from_dewpoint(t_c, td_c)` | Returns percent |
| `mpcalc.mixing_ratio(e, p)` | `rustmet.mixratio(p_hpa, t_c)` | Returns g/kg. Takes T not vapor pressure |
| `mpcalc.saturation_mixing_ratio(p, T)` | `rustmet.saturation_mixing_ratio(p_hpa, t_c)` | Returns g/kg |
| `mpcalc.vapor_pressure(p, w)` | `rustmet.vapor_pressure_from_dewpoint(td_c)` | Different interface |
| `mpcalc.wet_bulb_temperature(p, T, Td)` | `rustmet.wet_bulb_temperature(p_hpa, t_c, td_c)` | Returns Celsius |
| `mpcalc.virtual_temperature(T, w)` | `rustmet.virtual_temperature_from_dewpoint(t_c, td_c, p_hpa)` | Returns Celsius |
| `mpcalc.lcl(p, T, Td)` | `rustmet.lcltemp(t_c, td_c)` | Returns LCL temperature in Celsius |
| `mpcalc.specific_humidity_from_dewpoint(p, Td)` | `rustmet.specific_humidity_from_dewpoint(p_hpa, td_c)` | Returns kg/kg |
| `mpcalc.dewpoint_from_specific_humidity(p, q)` | `rustmet.dewpoint_from_specific_humidity(p_hpa, q)` | Returns Celsius |
| `mpcalc.thickness_hydrostatic(p, T)` | `rustmet.thickness_hypsometric(p_bottom, p_top, t_mean_k)` | Returns meters |

### Additional rustmet scalar functions

```python
# Equivalent potential temperature (returns Celsius, legacy interface)
theta_e_c = rustmet.thetae(p_hpa, t_c, td_c)

# Dewpoint from mixing ratio (q in kg/kg)
td = rustmet.dewpoint_from_q(q_kgkg, p_hpa)

# Frost point temperature
t_frost = rustmet.frost_point(t_c, rh)

# Wet-bulb potential temperature (K)
theta_w = rustmet.wet_bulb_potential_temperature(p_hpa, t_c, td_c)

# Relative humidity from mixing ratio (w in g/kg)
rh = rustmet.relative_humidity_from_mixing_ratio(p_hpa, t_c, w_gkg)

# Relative humidity from specific humidity (q in kg/kg)
rh = rustmet.relative_humidity_from_specific_humidity(p_hpa, t_c, q)
```


## Vectorized Array Functions (_arr variants)

For processing entire fields or profiles at once, rustmet provides `_arr`
versions that accept and return numpy arrays. These avoid Python loops and
are implemented in compiled Rust.

| MetPy (with units) | rustmet array function |
|---|---|
| `mpcalc.potential_temperature(p_arr, t_arr)` | `rustmet.potential_temperature_arr(p_hpa, t_c)` |
| `mpcalc.equivalent_potential_temperature(p, t, td)` | `rustmet.thetae_arr(p_hpa, t_c, td_c)` |
| `mpcalc.saturation_vapor_pressure(t_arr)` | `rustmet.vappres_arr(t_c)` |
| `mpcalc.dewpoint_from_relative_humidity(t, rh)` | `rustmet.dewpoint_from_rh_arr(t_c, rh)` |
| `mpcalc.saturation_mixing_ratio(p, t)` | `rustmet.mixratio_arr(p_hpa, t_c)` |
| `mpcalc.wet_bulb_temperature(p, t, td)` | `rustmet.wet_bulb_temperature_arr(p_hpa, t_c, td_c)` |

### Example: Computing theta-e for a full pressure-level grid

```python
import rustmet
import numpy as np

grib = rustmet.open("gfs_prs.grib2")

# Get all temperature messages on pressure levels
temps = grib.filter("TMP", "isobaric")
rh_msgs = grib.filter("RH", "isobaric")

for t_msg, rh_msg in zip(temps.messages, rh_msgs.messages):
    p = t_msg.level_value  # e.g. 500.0 hPa
    t_vals = t_msg.values()
    t_c = rustmet.convert_units(t_vals, "K", "C")
    rh_vals = rh_msg.values()

    # Compute dewpoint for the whole field
    td_c = rustmet.dewpoint_from_rh_arr(t_c, rh_vals)

    # Compute theta-e for the whole field
    p_arr = np.full_like(t_c, p)
    theta_e = rustmet.thetae_arr(p_arr, t_c, td_c)
    print(f"{p:.0f} hPa: theta_e range [{theta_e.min():.1f}, {theta_e.max():.1f}] K")
```


## Sounding / Profile Functions

### MetPy

```python
import metpy.calc as mpcalc
from metpy.units import units

p = [1000, 925, 850, 700, 500, 300] * units.hPa
T = [25, 20, 15, 5, -15, -40] * units.degC
Td = [20, 15, 10, -5, -25, -50] * units.degC

# Parcel profile
prof = mpcalc.parcel_profile(p, T[0], Td[0])

# CAPE/CIN
cape, cin = mpcalc.cape_cin(p, T, Td, prof)

# Surface-based CAPE/CIN
sb_cape, sb_cin = mpcalc.surface_based_cape_cin(p, T, Td)
```

### rustmet

```python
import rustmet
import numpy as np

p = np.array([1000, 925, 850, 700, 500, 300], dtype=np.float64)
T = np.array([25, 20, 15, 5, -15, -40], dtype=np.float64)
Td = np.array([20, 15, 10, -5, -25, -50], dtype=np.float64)

# Parcel profile (dry adiabat to LCL, moist above)
prof = rustmet.parcel_profile(p, T[0], Td[0])

# Dry and moist adiabats separately
dry_profile = rustmet.dry_lapse(p, T[0])
moist_profile = rustmet.moist_lapse(p, T[0])

# Surface-based CAPE/CIN
sb_cape, sb_cin = rustmet.py_surface_based_cape_cin(p, T, Td)

# Mixed-layer CAPE/CIN (100 hPa mixed layer)
ml_cape, ml_cin = rustmet.py_mixed_layer_cape_cin(p, T, Td, 100.0)

# Most-unstable CAPE/CIN
mu_cape, mu_cin = rustmet.py_most_unstable_cape_cin(p, T, Td)

# Downdraft CAPE
dcape = rustmet.py_downdraft_cape(p, T, Td)

# Bunkers storm motion: returns ((u_rm, v_rm), (u_lm, v_lm))
z = np.array([0, 750, 1500, 3000, 5600, 9500], dtype=np.float64)
u = np.array([5, 10, 15, 20, 25, 30], dtype=np.float64)
v = np.array([0, 5, 10, 10, 5, 0], dtype=np.float64)
(u_rm, v_rm), (u_lm, v_lm) = rustmet.py_bunkers_storm_motion(p, u, v, z)
```

**Note on units:** rustmet does not use Pint units. All inputs are plain
floats or numpy arrays. Pressure is always hPa, temperature is Celsius,
wind is m/s, height is meters.


## Stability Indices

MetPy computes these from full sounding profiles. rustmet provides direct
single-value functions:

```python
# K-Index: (T850 - T500) + Td850 - (T700 - Td700)
ki = rustmet.k_index(t850, t700, t500, td850, td700)

# Total Totals: (T850 - T500) + (Td850 - T500)
tt = rustmet.total_totals(t850, t500, td850)

# Lifted Index: surface parcel lifted to 500 hPa
li = rustmet.lifted_index(p_profile, t_profile, td_profile)

# Showalter Index: 850 hPa parcel lifted to 500 hPa
si = rustmet.showalter_index(p_profile, t_profile, td_profile)

# SWEAT Index
sweat = rustmet.sweat_index(tt, td850, wspd850_kt, wdir850, wspd500_kt, wdir500)
```


## Composite Severe Weather Parameters

rustmet includes gridded (numpy array) implementations for operational use.
These process entire 2D or 3D grids at once using Rayon parallelism.

```python
# Significant Tornado Parameter
stp = rustmet.compute_stp(cape_2d, lcl_2d, srh_1km_2d, shear_6km_2d)

# Supercell Composite Parameter
scp = rustmet.compute_scp(mucape_2d, srh_3km_2d, shear_6km_2d)

# Energy-Helicity Index
ehi = rustmet.compute_ehi(cape_2d, srh_2d)

# Significant Hail Parameter (SHIP)
ship = rustmet.significant_hail_parameter(cape, shear06, t500, lr_700_500, mr, nx, ny)

# Bulk Richardson Number
brn = rustmet.bulk_richardson_number(cape, shear_06_ms)
```


## Dynamics / Kinematics

### MetPy

```python
div = mpcalc.divergence(u, v, dx=dx, dy=dy)
vort = mpcalc.vorticity(u, v, dx=dx, dy=dy)
```

### rustmet

```python
# All dynamics functions take flattened 1D arrays + grid dimensions
div = rustmet.divergence(u_flat, v_flat, nx, ny, dx, dy)
vort = rustmet.vorticity(u_flat, v_flat, nx, ny, dx, dy)

# Additional dynamics
fronto = rustmet.frontogenesis_2d(theta_flat, u_flat, v_flat, nx, ny, dx, dy)
qvec = rustmet.q_vector(t_flat, u_geo_flat, v_geo_flat, p_hpa, nx, ny, dx, dy)
# qvec is a dict with 'q1' and 'q2' keys
```


## Wind Calculations

### MetPy

```python
speed = mpcalc.wind_speed(u, v)
direction = mpcalc.wind_direction(u, v)
```

### rustmet

```python
# Returns (speed, direction) tuple of numpy arrays
# Direction follows meteorological convention (0 = from north)
speed, direction = rustmet.wind_speed_dir(u_arr, v_arr)
```


## Heat Index and Wind Chill

```python
# Heat index (Rothfusz regression). Inputs in Fahrenheit.
hi = rustmet.py_heat_index(t_f, rh_percent)

# Wind chill (NWS formula). Inputs in Fahrenheit and mph.
wc = rustmet.py_windchill(t_f, wind_mph)

# Australian apparent temperature. Inputs in Celsius, %, m/s.
at = rustmet.py_apparent_temperature(t_c, rh, wind_ms, solar_wm2=None)
```


## Key Differences from MetPy

1. **No Pint units.** rustmet uses plain floats. Pressure is always hPa,
   temperature is Celsius, wind is m/s, heights in meters. This avoids
   unit-conversion overhead but means you need to know your input units.

2. **No xarray integration.** rustmet returns numpy arrays. If you need
   xarray, wrap them yourself:
   ```python
   import xarray as xr
   da = xr.DataArray(msg.values_2d(), dims=["y", "x"])
   ```

3. **Bolton (1980) throughout.** Both MetPy and rustmet use Bolton's
   formulation for saturation vapor pressure and theta-e, so results
   should match to numerical precision.

4. **Compiled Rust.** The `_arr` functions and grid computations run in
   compiled Rust, typically 5-20x faster than MetPy's NumPy implementations
   for large grids. Composite parameter functions use Rayon for automatic
   multi-core parallelism.

5. **Built-in rendering.** rustmet can render Skew-T diagrams, hodographs,
   filled contours, wind barbs, and streamlines without matplotlib:
   ```python
   pixels = rustmet.render_skewt_py(p, t, td, wind_speed=ws, wind_dir=wd)
   rustmet.save_png(pixels, 800, 800, "skewt.png")
   ```
