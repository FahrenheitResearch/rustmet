"""
rustmet — Fast GRIB2 processor for weather models
==================================================

Pure Rust GRIB2 parser with Python bindings. 5x faster than cfgrib/eccodes.

Quick start::

    import rustmet

    # Fetch HRRR 2m temperature (downloads only the bytes you need)
    grib = rustmet.fetch("hrrr", "2026-03-09/00z",
                         vars=["TMP:2 m above ground"])

    # Get the data as a numpy array
    msg = grib.messages[0]
    data = msg.values_2d()    # shape (ny, nx), dtype float64
    lats = msg.lats()
    lons = msg.lons()

    # Or get an xarray Dataset (requires xarray)
    ds = rustmet.to_xarray(grib)

Supported models: HRRR, GFS, NAM, RAP
"""

from rustmet._rustmet import (
    GribFile,
    GribMessage,
    Client,
    fetch,
    open,
    products,
    __version__,
    # Thermodynamic functions
    lcltemp,
    thetae,
    mixratio,
    dewpoint_from_q,
    # Composite severe weather parameters
    compute_cape_cin,
    compute_srh,
    compute_shear,
    compute_stp,
    compute_ehi,
    compute_scp,
    compute_lapse_rate,
    compute_pw,
)


def to_xarray(grib, name=None):
    """Convert a GribFile to an xarray Dataset.

    Args:
        grib: GribFile from fetch() or open()
        name: Optional dataset name

    Returns:
        xarray.Dataset with one data variable per GRIB message,
        plus lat/lon coordinates.

    Requires: pip install xarray
    """
    try:
        import xarray as xr
    except ImportError:
        raise ImportError(
            "xarray is required for to_xarray(). Install with: pip install xarray"
        )
    import numpy as np

    data_vars = {}
    coords = None
    attrs = {}

    for i, msg in enumerate(grib.messages):
        # Build a unique variable name
        var_name = msg.variable
        if var_name in data_vars:
            var_name = f"{msg.variable}_{msg.level_type}_{int(msg.level_value)}"

        values = msg.values_2d()

        # Set up coordinates from first message
        if coords is None:
            ny, nx = msg.ny, msg.nx
            lat_1d = msg.lats()
            lon_1d = msg.lons()

            if len(lat_1d) == ny * nx:
                # 2D lat/lon (Lambert Conformal etc.)
                lats_2d = lat_1d.reshape(ny, nx)
                lons_2d = lon_1d.reshape(ny, nx)
                coords = {
                    "lat": (["y", "x"], lats_2d),
                    "lon": (["y", "x"], lons_2d),
                }
                dims = ["y", "x"]
            else:
                # Regular lat/lon grid
                coords = {
                    "lat": (["y"], lat_1d[:ny]),
                    "lon": (["x"], lon_1d[:nx]),
                }
                dims = ["y", "x"]

            attrs["reference_time"] = msg.reference_time
            attrs["forecast_time"] = int(msg.forecast_time)

        msg_attrs = {
            "units": msg.units,
            "level": msg.level,
            "level_type": msg.level_type,
            "level_value": msg.level_value,
            "discipline": int(msg.discipline),
            "parameter_category": int(msg.parameter_category),
            "parameter_number": int(msg.parameter_number),
        }

        data_vars[var_name] = (dims, np.asarray(values), msg_attrs)

    ds = xr.Dataset(data_vars, coords=coords, attrs=attrs)
    if name:
        ds.attrs["name"] = name
    return ds


__all__ = [
    "GribFile",
    "GribMessage",
    "Client",
    "fetch",
    "open",
    "products",
    "to_xarray",
    "__version__",
    # Thermodynamic functions
    "lcltemp",
    "thetae",
    "mixratio",
    "dewpoint_from_q",
    # Composite severe weather parameters
    "compute_cape_cin",
    "compute_srh",
    "compute_shear",
    "compute_stp",
    "compute_ehi",
    "compute_scp",
    "compute_lapse_rate",
    "compute_pw",
]
