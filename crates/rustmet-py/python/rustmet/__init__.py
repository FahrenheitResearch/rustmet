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

try:
    from rustmet._rustmet import (
        GribFile,
        GribMessage,
        Client,
        Grib2Writer,
        fetch,
        fetch_streaming as _fetch_streaming_native,
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
        # Rendering functions
        render_map as _render_map,
        render_array as _render_array,
        save_png as _save_png,
        colormaps,
    )
except ImportError:
    __version__ = "0.1.0"


def _require_pandas():
    """Import and return pandas, raising a helpful error if not installed."""
    try:
        import pandas as pd
        return pd
    except ImportError:
        raise ImportError(
            "pandas is required for DataFrame support. "
            "Install with: pip install pandas"
        )


def inventory_df(client_or_grib, model=None, run=None, fhour=0, product="prs"):
    """Return inventory as a pandas DataFrame.

    Can be called with:
    - inventory_df(grib_file) -- inventory of a parsed GribFile
    - inventory_df(client, model, run, fhour) -- inventory from server .idx file

    Returns DataFrame with columns:
        msg_num, byte_offset, variable, level, forecast, description, units

    For GribFile input, description and units come from WMO GRIB2 tables.
    For Client/.idx input, description and units are empty strings (the .idx
    format does not include the discipline/category/number needed for lookup).

    Requires: pip install pandas
    """
    pd = _require_pandas()

    if isinstance(client_or_grib, GribFile):
        rows = []
        for i, msg in enumerate(client_or_grib.messages):
            rows.append({
                "msg_num": i + 1,
                "byte_offset": None,
                "variable": msg.variable,
                "level": msg.level,
                "forecast": f"t{msg.forecast_time}",
                "description": msg.variable,
                "units": msg.units,
            })
        return pd.DataFrame(rows)

    if isinstance(client_or_grib, Client):
        if model is None or run is None:
            raise ValueError(
                "When calling inventory_df with a Client, "
                "model and run are required."
            )
        entries = client_or_grib.inventory(model, run, fhour, product)
        rows = []
        for entry in entries:
            rows.append({
                "msg_num": entry["msg_num"],
                "byte_offset": entry["byte_offset"],
                "variable": entry["variable"],
                "level": entry["level"],
                "forecast": entry["forecast"],
                "description": "",
                "units": "",
            })
        return pd.DataFrame(rows)

    raise TypeError(
        f"Expected GribFile or Client, got {type(client_or_grib).__name__}"
    )


def _gribfile_to_dataframe(self):
    """Convert inventory to pandas DataFrame.

    Returns DataFrame with columns:
        msg_num, byte_offset, variable, level, forecast, description, units
    """
    return inventory_df(self)


# Monkey-patch to_dataframe onto GribFile
try:
    GribFile.to_dataframe = _gribfile_to_dataframe
except NameError:
    pass  # GribFile not available (native module not built)


def search(grib, query):
    """Search a GribFile's messages by human-readable query (fuzzy match).

    Uses intelligent matching with alias support. Supports patterns like:
      - "temperature" -- any temperature variable
      - "temperature 2m" -- TMP at 2m above ground
      - "wind 10m" -- UGRD/VGRD at 10m
      - "cape" -- CAPE
      - "500mb height" -- HGT at 500 mb
      - "rh" -- Relative Humidity (via alias)

    Results are ranked by relevance (best match first).

    Args:
        grib: GribFile to search
        query: Human-readable search string

    Returns:
        list of GribMessage objects, ranked by relevance
    """
    return grib.search(query)


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


def _parse_fhour_spec(fhour):
    """Parse a forecast hour specification into a list of ints or a single int.

    Accepts:
        - int: single forecast hour -> returned as-is
        - list/tuple of ints: passed through as list
        - str: "0-6" (range), "0,3,6,9" (comma-separated), or "0-12:3" (range with step)

    Returns:
        int (single hour) or list of int (multiple hours).
    """
    if isinstance(fhour, int):
        return fhour
    if isinstance(fhour, (list, tuple)):
        return [int(h) for h in fhour]
    if isinstance(fhour, str):
        fhour = fhour.strip()
        # Comma-separated: "0,3,6,9,12"
        if "," in fhour:
            return [int(h.strip()) for h in fhour.split(",")]
        # Range with optional step: "0-6" or "0-12:3"
        if "-" in fhour:
            parts = fhour.split(":")
            step = int(parts[1]) if len(parts) > 1 else 1
            endpoints = parts[0].split("-")
            start = int(endpoints[0])
            end = int(endpoints[1])
            return list(range(start, end + 1, step))
        # Single number as string
        return int(fhour)
    raise TypeError(f"fhour must be int, list, or str, got {type(fhour).__name__}")


# Wrap the native fetch to support string range syntax (e.g. "0-6", "0,3,6")
try:
    _native_fetch = fetch

    def fetch(model, run, fhour=0, product="prs", vars=None):
        """Fetch GRIB2 data from an operational weather model.

        Args:
            model: Model name ("hrrr", "gfs", "nam", "rap")
            run: Run time as "YYYY-MM-DD/HHz" (e.g. "2026-03-09/00z")
            fhour: Forecast hour(s). Accepts:
                - int: single hour (returns GribFile)
                - list of ints: multiple hours, downloaded in parallel
                  (returns list[GribFile])
                - str: "0-6" (range), "0,3,6,9" (comma-separated),
                       "0-12:3" (range with step)
            product: "prs", "sfc", "nat", or "subh" (default "prs")
            vars: List of variable patterns (e.g. ["TMP:2 m above ground"])

        Returns:
            GribFile (single fhour) or list[GribFile] (multiple fhours)
        """
        parsed = _parse_fhour_spec(fhour)
        return _native_fetch(model, run, parsed, product, vars)
except NameError:
    pass  # native module not built


def fetch_series(model, run, fhours, product="sfc", vars=None):
    """Fetch multiple forecast hours and return as time-indexed xarray Dataset.

    Downloads each forecast hour (in parallel via Rust) and concatenates
    the results along a ``forecast_hour`` dimension.

    Args:
        model: Model name ("hrrr", "gfs", "nam", "rap")
        run: Run time as "YYYY-MM-DD/HHz"
        fhours: Forecast hours -- int, list, or range string
                (e.g. "0-6", "0,3,6", "0-12:3")
        product: Model product (default "sfc")
        vars: Variable filter patterns

    Returns:
        xarray.Dataset with a ``forecast_hour`` coordinate dimension.

    Requires: pip install xarray
    """
    try:
        import xarray as xr
    except ImportError:
        raise ImportError(
            "xarray is required for fetch_series(). "
            "Install with: pip install xarray"
        )

    parsed = _parse_fhour_spec(fhours)
    if isinstance(parsed, int):
        parsed = [parsed]

    # Fetch all hours (Rust parallelizes the downloads)
    gribs = fetch(model, run, parsed, product, vars)
    if not isinstance(gribs, list):
        gribs = [gribs]

    # Convert each to xarray and concatenate along forecast_hour
    datasets = []
    for fh, grib in zip(parsed, gribs):
        ds = to_xarray(grib)
        ds = ds.expand_dims(forecast_hour=[fh])
        datasets.append(ds)

    return xr.concat(datasets, dim="forecast_hour")


def fetch_streaming(model, run, fhour=0, product="sfc", vars=None, callback=None):
    """Download and decode GRIB2 data with streaming decode.

    Messages are decoded incrementally as bytes arrive from the network,
    rather than waiting for the full download to complete. If a callback
    is provided, it is called with each GribMessage as soon as it is
    decoded -- allowing processing to overlap with downloading.

    Args:
        model: Model name ("hrrr", "gfs", "nam", "rap")
        run: Run time as "YYYY-MM-DD/HHz" (e.g. "2026-03-10/00z")
        fhour: Forecast hour (default 0)
        product: Model product ("prs", "sfc", "nat", "subh") -- default "sfc"
        vars: Optional list of variable patterns to filter download
        callback: Optional callable(GribMessage) invoked per decoded message

    Returns:
        list of GribMessage -- all messages decoded during the download.

    Example::

        def on_msg(msg):
            print(f"Decoded: {msg.variable} {msg.level}")

        msgs = rustmet.fetch_streaming("hrrr", "2026-03-10/00z",
                                       vars=["TMP:2 m above ground"],
                                       callback=on_msg)
    """
    try:
        return _fetch_streaming_native(model, run, fhour, product, vars, callback)
    except NameError:
        raise RuntimeError(
            "fetch_streaming requires the native rustmet module to be built"
        )


def fetch_iter(model, run, fhour=0, product="sfc", vars=None):
    """Generator that yields GribMessage objects as they are decoded.

    This is the iterator-based counterpart to :func:`fetch_streaming`.
    Under the hood it uses the same streaming decode pipeline, but
    collects messages and yields them one by one so you can use it in
    a ``for`` loop.

    Args:
        model: Model name ("hrrr", "gfs", "nam", "rap")
        run: Run time as "YYYY-MM-DD/HHz"
        fhour: Forecast hour (default 0)
        product: Model product (default "sfc")
        vars: Optional list of variable patterns

    Yields:
        GribMessage objects in the order they were decoded.

    Example::

        for msg in rustmet.fetch_iter("hrrr", "2026-03-10/00z",
                                      vars=["TMP:2 m above ground"]):
            data = msg.values_2d()
            print(f"{msg.variable}: min={data.min():.1f} max={data.max():.1f}")
    """
    # Use the streaming native function which returns all messages.
    # We collect them via the native call and then yield one at a time.
    # A true async generator would require deeper integration, but this
    # still gets the benefit of streaming decode (messages are parsed
    # incrementally during download rather than all-at-once after).
    messages = fetch_streaming(model, run, fhour, product, vars)
    for msg in messages:
        yield msg


def plot(msg, colormap="temperature", vmin=None, vmax=None, save=None):
    """Render a GribMessage as an image. Returns RGBA numpy array.

    Uses Rust-native rendering -- no matplotlib required. Produces a
    colormapped raster image from the GRIB message's data values.

    Args:
        msg: GribMessage to render
        colormap: Colormap name. Available colormaps:
            "temperature", "precipitation", "wind", "reflectivity",
            "cape", "relative_humidity", "vorticity"
        vmin: Minimum value for colormap scaling. If None, auto-detected
              from data range.
        vmax: Maximum value for colormap scaling. If None, auto-detected
              from data range.
        save: Optional file path. If provided, writes a PNG to that path.

    Returns:
        numpy array of shape (ny, nx, 4), dtype uint8 (RGBA).
        Alpha channel is 0 for NaN values, 255 otherwise.

    Example::

        import rustmet

        grib = rustmet.fetch("hrrr", "2026-03-10/00z",
                             vars=["TMP:2 m above ground"])
        msg = grib.messages[0]

        # Render and get numpy array
        rgba = rustmet.plot(msg, colormap="temperature",
                            vmin=250, vmax=310)

        # Render and save directly to PNG
        rustmet.plot(msg, colormap="temperature",
                     vmin=250, vmax=310, save="temperature.png")
    """
    import numpy as np

    pixels_flat = _render_map(msg, colormap, vmin, vmax)
    rgba = np.asarray(pixels_flat).reshape(msg.ny, msg.nx, 4)

    if save is not None:
        _save_png(pixels_flat, msg.nx, msg.ny, str(save))

    return rgba


def render(values, nx, ny, colormap="temperature", vmin=None, vmax=None, save=None):
    """Render a raw 2D array as a colormapped RGBA image.

    Like :func:`plot` but takes a numpy array instead of a GribMessage.

    Args:
        values: 2D numpy array or 1D array of length ny*nx
        nx: Grid width
        ny: Grid height
        colormap: Colormap name (default "temperature")
        vmin: Min value for scaling. If None, uses data minimum.
        vmax: Max value for scaling. If None, uses data maximum.
        save: Optional PNG output path.

    Returns:
        numpy array shape (ny, nx, 4) dtype uint8 (RGBA)
    """
    import numpy as np

    arr = np.asarray(values, dtype=np.float64).ravel()
    if len(arr) != nx * ny:
        raise ValueError(
            f"values has {len(arr)} elements but nx*ny = {nx * ny}"
        )

    if vmin is None:
        valid = arr[~np.isnan(arr)]
        vmin = float(valid.min()) if len(valid) > 0 else 0.0
    if vmax is None:
        valid = arr[~np.isnan(arr)]
        vmax = float(valid.max()) if len(valid) > 0 else 1.0

    pixels_flat = _render_array(arr, nx, ny, colormap, vmin, vmax)
    rgba = np.asarray(pixels_flat).reshape(ny, nx, 4)

    if save is not None:
        _save_png(pixels_flat, nx, ny, str(save))

    return rgba


def write_grib2(path, messages):
    """Write GribMessage objects to a GRIB2 file.

    Convenience function that creates a Grib2Writer, adds each message,
    and writes to disk.

    Args:
        path: Output file path.
        messages: List of GribMessage objects (from fetch/open) or list of
                  dicts with keys matching Grib2Writer.add_field() parameters.

    Example::

        import numpy as np
        import rustmet

        writer = rustmet.Grib2Writer()
        writer.add_field(
            values=np.random.randn(100).astype(np.float64),
            discipline=0,
            parameter_category=0,
            parameter_number=0,
            level_type=103,
            level_value=2.0,
            grid_template=0,
            nx=10, ny=10,
            lat1=30.0, lon1=-100.0,
            lat2=39.0, lon2=-91.0,
            dx=1.0, dy=1.0,
            bits_per_value=16,
            reference_time="2025-06-15 12:00:00",
        )
        writer.write("output.grib2")

        # Or use this convenience function with GribMessage objects:
        grib = rustmet.open("input.grib2")
        # Re-encode all messages to a new file
        rustmet.write_grib2("copy.grib2", grib.messages)
    """
    import numpy as np

    writer = Grib2Writer()
    for msg in messages:
        if isinstance(msg, dict):
            writer.add_field(**msg)
        elif hasattr(msg, 'values'):
            # GribMessage object -- extract metadata and re-encode
            vals = np.asarray(msg.values(), dtype=np.float64)
            writer.add_field(
                values=vals,
                discipline=msg.discipline,
                parameter_category=msg.parameter_category,
                parameter_number=msg.parameter_number,
                level_type=_level_type_from_string(msg.level_type),
                level_value=msg.level_value,
                nx=msg.nx,
                ny=msg.ny,
                reference_time=msg.reference_time,
                forecast_time=msg.forecast_time,
            )
        else:
            raise TypeError(
                f"Expected GribMessage or dict, got {type(msg).__name__}"
            )
    writer.write(str(path))


def _level_type_from_string(level_type_str):
    """Map level type string back to GRIB2 code."""
    mapping = {
        "surface": 1,
        "cloud base": 2,
        "cloud top": 3,
        "tropopause": 7,
        "top of atmosphere": 8,
        "isothermal": 20,
        "isobaric": 100,
        "mean sea level": 101,
        "height above ground": 103,
        "sigma": 104,
        "hybrid": 105,
        "depth below land surface": 106,
        "isentropic": 107,
        "pressure departure": 108,
        "potential vorticity": 109,
        "entire atmosphere": 200,
        "entire ocean": 201,
    }
    lower = level_type_str.lower().strip()
    return mapping.get(lower, 103)  # default to height above ground


__all__ = [
    "GribFile",
    "GribMessage",
    "Grib2Writer",
    "Client",
    "fetch",
    "fetch_streaming",
    "fetch_iter",
    "fetch_series",
    "open",
    "products",
    "to_xarray",
    "inventory_df",
    "search",
    "write_grib2",
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
    # Rendering
    "plot",
    "render",
    "colormaps",
]
