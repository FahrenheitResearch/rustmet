"""
rustmet_xarray -- xarray integration helpers for rustmet
=========================================================

Provides convenience functions for converting GRIB2 files opened via
rustmet into xarray Datasets, with proper coordinates, attributes, and
time metadata.

Optional dependency: requires ``xarray`` (``pip install xarray``).

Example::

    from rustmet.rustmet_xarray import open_grib2

    ds = open_grib2("hrrr.t00z.wrfsfcf01.grib2",
                     filter={"variable": "TMP", "level": "2 m"})
    print(ds)
    ds["TMP"].plot()
"""

from __future__ import annotations

import os
from typing import Any, Dict, List, Optional, Sequence, Union


def _require_xarray():
    """Import xarray, raising a clear error if unavailable."""
    try:
        import xarray as xr
        return xr
    except ImportError:
        raise ImportError(
            "xarray is required for rustmet_xarray. "
            "Install with: pip install xarray  "
            "or: pip install rustmet[xarray]"
        )


def _require_numpy():
    """Import numpy."""
    import numpy as np
    return np


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _match_filter(msg, filt: Optional[Dict[str, Any]]) -> bool:
    """Return True if *msg* passes *filt*.

    *filt* is a dict whose keys are attribute names on GribMessage
    (``variable``, ``level``, ``level_type``, ``level_value``,
    ``forecast_time``, ``discipline``, ``parameter_category``,
    ``parameter_number``, ``units``).

    String values are matched case-insensitively using substring containment
    (the same logic as ``GribFile.filter``). Numeric values must match
    exactly.
    """
    if filt is None:
        return True
    for key, want in filt.items():
        actual = getattr(msg, key, None)
        if actual is None:
            return False
        if isinstance(want, str):
            if want.lower() not in str(actual).lower():
                return False
        else:
            if actual != want:
                return False
    return True


def _unique_var_name(base: str, existing: dict) -> str:
    """Return a variable name that does not collide with *existing* keys."""
    if base not in existing:
        return base
    # Append a numeric suffix
    idx = 2
    while f"{base}_{idx}" in existing:
        idx += 1
    return f"{base}_{idx}"


def _build_coords(msg, np):
    """Build coordinate arrays and dimension names from a GribMessage.

    Returns (coords_dict, dims_list, is_2d_latlon).
    """
    ny, nx = int(msg.ny), int(msg.nx)
    lat_1d = np.asarray(msg.lats(), dtype=np.float64)
    lon_1d = np.asarray(msg.lons(), dtype=np.float64)

    if len(lat_1d) == ny * nx:
        # Projected / curvilinear grid (e.g. Lambert Conformal)
        lats_2d = lat_1d.reshape(ny, nx)
        lons_2d = lon_1d.reshape(ny, nx)
        coords = {
            "lat": (["y", "x"], lats_2d),
            "lon": (["y", "x"], lons_2d),
        }
        return coords, ["y", "x"], True
    else:
        # Regular lat/lon grid
        coords = {
            "lat": (["y"], lat_1d[:ny]),
            "lon": (["x"], lon_1d[:nx]),
        }
        return coords, ["y", "x"], False


def _msg_attrs(msg) -> dict:
    """Extract per-variable attributes from a GribMessage."""
    return {
        "units": msg.units,
        "level": msg.level,
        "level_type": msg.level_type,
        "level_value": float(msg.level_value),
        "discipline": int(msg.discipline),
        "parameter_category": int(msg.parameter_category),
        "parameter_number": int(msg.parameter_number),
        "forecast_time": int(msg.forecast_time),
        "reference_time": msg.reference_time,
        "GRIB_shortName": msg.variable,
    }


def _parse_reference_time(ref_str: str):
    """Best-effort parse of a reference_time string into a datetime object.

    Returns None on failure rather than raising.
    """
    from datetime import datetime
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y%m%d%H%M%S"):
        try:
            return datetime.strptime(ref_str.strip(), fmt)
        except (ValueError, AttributeError):
            continue
    return None


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def messages_to_dataset(
    messages: Sequence,
    *,
    filter: Optional[Dict[str, Any]] = None,
    name: Optional[str] = None,
) -> "xarray.Dataset":
    """Convert a sequence of rustmet ``GribMessage`` objects to an xarray Dataset.

    Each message becomes a data variable named by its ``variable`` attribute
    (e.g. ``TMP``, ``UGRD``).  When multiple messages share the same variable
    name, a suffix is appended using the level type and value to disambiguate.

    Grid coordinates (``lat``, ``lon``) are taken from the first message.
    Projected grids (Lambert Conformal, etc.) produce 2-D coordinate arrays
    on ``(y, x)`` dimensions.

    Parameters
    ----------
    messages : sequence of GribMessage
        Messages to include. Typically ``grib.messages`` from a GribFile.
    filter : dict, optional
        If provided, only messages matching all key/value pairs are included.
        Keys correspond to GribMessage attributes (``variable``, ``level``,
        ``level_type``, ``level_value``, ``forecast_time``, etc.).
        String values use case-insensitive substring matching.
    name : str, optional
        If given, stored in ``ds.attrs["name"]``.

    Returns
    -------
    xarray.Dataset
    """
    xr = _require_xarray()
    np = _require_numpy()

    data_vars: Dict[str, Any] = {}
    coords = None
    dims = None
    global_attrs: Dict[str, Any] = {}

    for msg in messages:
        if not _match_filter(msg, filter):
            continue

        # Unpack data
        try:
            values = np.asarray(msg.values_2d(), dtype=np.float64)
        except Exception:
            # Fall back to 1-D unpack and reshape manually
            vals_1d = np.asarray(msg.values(), dtype=np.float64)
            ny, nx = int(msg.ny), int(msg.nx)
            if len(vals_1d) == ny * nx:
                values = vals_1d.reshape(ny, nx)
            else:
                # Cannot reshape -- skip this message
                continue

        # Replace GRIB missing-value sentinels with NaN.
        # Common sentinel is 9.999e20 (WMO convention).
        values = np.where(np.abs(values) > 1e15, np.nan, values)

        # Coordinates from first usable message
        if coords is None:
            coords, dims, _ = _build_coords(msg, np)
            ref = _parse_reference_time(msg.reference_time)
            global_attrs["reference_time"] = msg.reference_time
            global_attrs["forecast_time"] = int(msg.forecast_time)
            if ref is not None:
                global_attrs["reference_datetime"] = str(ref)

        # Variable naming: prefer short name, disambiguate on collision
        base_name = msg.variable
        var_name = _unique_var_name(base_name, data_vars)
        # If we had to disambiguate, use a more informative name
        if var_name != base_name:
            descriptive = f"{base_name}_{msg.level_type}_{int(msg.level_value)}"
            descriptive = descriptive.replace(" ", "_")
            var_name = _unique_var_name(descriptive, data_vars)

        data_vars[var_name] = (dims, values, _msg_attrs(msg))

    if not data_vars:
        # Return an empty dataset rather than crashing
        return xr.Dataset(attrs={"note": "no matching messages"})

    ds = xr.Dataset(data_vars, coords=coords, attrs=global_attrs)
    if name:
        ds.attrs["name"] = name
    return ds


def open_grib2(
    path: str,
    *,
    filter: Optional[Dict[str, Any]] = None,
    name: Optional[str] = None,
) -> "xarray.Dataset":
    """Open a GRIB2 file and return an xarray Dataset.

    This is the primary entry point. It opens the file with rustmet's native
    Rust parser, optionally filters messages, and returns an xarray Dataset
    with proper coordinates and per-variable attributes.

    Parameters
    ----------
    path : str
        Filesystem path to the GRIB2 file.
    filter : dict, optional
        Restrict which GRIB messages become data variables.  Keys are
        GribMessage attribute names; values are matched using the same rules
        as ``GribFile.filter``.

        Common filter keys:

        - ``"variable"``: parameter short name (e.g. ``"TMP"``, ``"UGRD"``)
        - ``"level"``: level string (e.g. ``"2 m above ground"``, ``"500 mb"``)
        - ``"level_type"``: level type (e.g. ``"isobaric"``, ``"surface"``)
        - ``"level_value"``: numeric level value (e.g. ``500.0``)
        - ``"forecast_time"``: forecast time in hours (e.g. ``6``)

    name : str, optional
        Stored in ``ds.attrs["name"]``.

    Returns
    -------
    xarray.Dataset

    Examples
    --------
    Open all messages::

        ds = open_grib2("hrrr.grib2")

    Open only 2-m temperature::

        ds = open_grib2("hrrr.grib2", filter={"variable": "TMP", "level": "2 m"})

    Open all isobaric-level fields::

        ds = open_grib2("hrrr.grib2", filter={"level_type": "isobaric"})
    """
    import rustmet

    grib = rustmet.open(path)
    file_name = os.path.basename(path) if name is None else name
    return messages_to_dataset(grib.messages, filter=filter, name=file_name)


def open_like_cfgrib(
    path: str,
    *,
    backend_kwargs: Optional[Dict[str, Any]] = None,
    filter_by_keys: Optional[Dict[str, Any]] = None,
    errors: str = "warn",
    indexpath: str = "",
) -> "List[xarray.Dataset]":
    """Open a GRIB2 file with an API that mimics ``cfgrib.open_datasets``.

    This is intended as a drop-in replacement for common cfgrib usage.
    It returns a **list** of xarray Datasets (one per distinct level type),
    matching cfgrib's convention of splitting heterogeneous GRIB files.

    Parameters
    ----------
    path : str
        Path to the GRIB2 file.
    backend_kwargs : dict, optional
        Ignored (accepted for cfgrib API compatibility).
    filter_by_keys : dict, optional
        Filtering dictionary.  Recognized keys (matching cfgrib conventions):

        - ``"shortName"`` or ``"variable"`` -- parameter short name
        - ``"typeOfLevel"`` or ``"level_type"`` -- level type string
        - ``"level"`` or ``"level_value"`` -- numeric level
        - ``"stepRange"`` or ``"forecast_time"`` -- forecast time

        Values can be strings (substring match) or numbers (exact match).
        Lists of values are OR-matched.
    errors : str, optional
        ``"warn"`` (default), ``"raise"``, or ``"ignore"``. Controls handling
        of messages that cannot be decoded.
    indexpath : str, optional
        Ignored (accepted for cfgrib API compatibility).

    Returns
    -------
    list of xarray.Dataset
        One dataset per distinct ``level_type`` found in the file.

    Examples
    --------
    ::

        datasets = open_like_cfgrib("gfs.grib2",
                                     filter_by_keys={"shortName": "t", "typeOfLevel": "isobaric"})
        ds = datasets[0]
    """
    xr = _require_xarray()
    np = _require_numpy()
    import rustmet

    grib = rustmet.open(path)

    # Normalize cfgrib-style filter keys to rustmet attribute names
    filt = _normalize_cfgrib_filter(filter_by_keys)

    # Group messages by level_type (cfgrib returns one dataset per level type)
    groups: Dict[str, list] = {}
    for msg in grib.messages:
        if not _match_cfgrib_filter(msg, filt):
            continue
        lt = msg.level_type
        groups.setdefault(lt, []).append(msg)

    datasets = []
    for level_type, msgs in groups.items():
        try:
            ds = messages_to_dataset(msgs, name=os.path.basename(path))
            ds.attrs["GRIB_typeOfLevel"] = level_type
            datasets.append(ds)
        except Exception as exc:
            if errors == "raise":
                raise
            elif errors == "warn":
                import warnings
                warnings.warn(
                    f"Failed to convert level_type={level_type!r}: {exc}",
                    stacklevel=2,
                )
            # else: ignore

    return datasets


def _normalize_cfgrib_filter(filt: Optional[dict]) -> Optional[dict]:
    """Map cfgrib filter_by_keys names to rustmet GribMessage attribute names."""
    if filt is None:
        return None
    mapping = {
        "shortName": "variable",
        "typeOfLevel": "level_type",
        "level": "level_value",
        "stepRange": "forecast_time",
        # Already-native names pass through
        "variable": "variable",
        "level_type": "level_type",
        "level_value": "level_value",
        "forecast_time": "forecast_time",
    }
    out = {}
    for k, v in filt.items():
        native_key = mapping.get(k, k)
        out[native_key] = v
    return out


def _match_cfgrib_filter(msg, filt: Optional[dict]) -> bool:
    """Match a message against a normalized cfgrib filter.

    Supports list-of-values (OR semantics) in addition to scalar values.
    """
    if filt is None:
        return True
    for key, want in filt.items():
        actual = getattr(msg, key, None)
        if actual is None:
            return False
        if isinstance(want, (list, tuple)):
            # OR semantics: message passes if it matches any value in list
            matched = False
            for w in want:
                if isinstance(w, str):
                    if w.lower() in str(actual).lower():
                        matched = True
                        break
                else:
                    if actual == w:
                        matched = True
                        break
            if not matched:
                return False
        elif isinstance(want, str):
            if want.lower() not in str(actual).lower():
                return False
        else:
            if actual != want:
                return False
    return True
