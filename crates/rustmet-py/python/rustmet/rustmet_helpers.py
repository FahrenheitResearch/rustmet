"""
rustmet_helpers -- convenience utilities for rustmet
=====================================================

High-level helpers that wrap rustmet's core GRIB2 functionality for
common interactive use cases: quick plotting and cross-library validation.

Example::

    from rustmet.rustmet_helpers import quick_plot, compare_to_eccodes

    quick_plot("hrrr.grib2", "TMP", level="2 m")
    report = compare_to_eccodes("hrrr.grib2")
"""

from __future__ import annotations

from typing import Optional


def quick_plot(
    path: str,
    parameter: str,
    level: Optional[str] = None,
    *,
    colormap: str = "viridis",
    vmin: Optional[float] = None,
    vmax: Optional[float] = None,
    title: Optional[str] = None,
    figsize: tuple = (12, 8),
    save: Optional[str] = None,
):
    """One-liner to open a GRIB2 file and plot a field with matplotlib.

    Opens the file with rustmet, finds the requested parameter (and optional
    level), unpacks the 2-D data with lat/lon coordinates, and produces a
    filled-contour plot using matplotlib and cartopy (if available) or a
    plain imshow fallback.

    Parameters
    ----------
    path : str
        Path to a GRIB2 file.
    parameter : str
        Variable short name to search for (e.g. ``"TMP"``, ``"UGRD"``,
        ``"REFC"``).  Uses case-insensitive substring matching.
    level : str, optional
        Level substring to match (e.g. ``"2 m"``, ``"500 mb"``).
        If omitted, the first message matching *parameter* is used.
    colormap : str, optional
        Matplotlib colormap name (default ``"viridis"``).
    vmin, vmax : float, optional
        Explicit color scale limits.  If omitted, derived from data range.
    title : str, optional
        Plot title.  If omitted, auto-generated from message metadata.
    figsize : tuple, optional
        Figure size in inches (default ``(12, 8)``).
    save : str, optional
        If provided, saves the figure to this path (PNG, PDF, etc.).

    Returns
    -------
    tuple of (matplotlib.figure.Figure, matplotlib.axes.Axes)
        The created figure and axes so you can further customize the plot.

    Raises
    ------
    FileNotFoundError
        If *path* does not exist.
    ValueError
        If no message matches *parameter* (and *level*).
    ImportError
        If matplotlib is not installed.

    Examples
    --------
    ::

        fig, ax = quick_plot("hrrr.grib2", "TMP", level="2 m")
        fig, ax = quick_plot("gfs.grib2", "HGT", level="500 mb",
                              colormap="coolwarm", title="500 hPa Heights")
    """
    import rustmet
    import numpy as np

    try:
        import matplotlib.pyplot as plt
    except ImportError:
        raise ImportError(
            "matplotlib is required for quick_plot(). "
            "Install with: pip install matplotlib"
        )

    # Open and find the message
    grib = rustmet.open(path)
    msg = grib.find(parameter, level)
    if msg is None:
        available = [f"{m.variable}:{m.level}" for m in grib.messages]
        raise ValueError(
            f"No message matching variable={parameter!r}, level={level!r}. "
            f"Available: {available[:20]}{'...' if len(available) > 20 else ''}"
        )

    # Unpack data and coordinates
    data = np.asarray(msg.values_2d(), dtype=np.float64)
    lats = np.asarray(msg.lats(), dtype=np.float64)
    lons = np.asarray(msg.lons(), dtype=np.float64)
    ny, nx = int(msg.ny), int(msg.nx)

    # Replace GRIB missing-value sentinels with NaN
    data = np.where(np.abs(data) > 1e15, np.nan, data)

    # Build title
    if title is None:
        title = f"{msg.variable} -- {msg.level} [{msg.units}]"
        if msg.reference_time:
            title += f"\nRef: {msg.reference_time}  FH: {msg.forecast_time}"

    # Determine if coordinates are 2-D (projected grid) or 1-D (regular)
    is_2d = len(lats) == ny * nx

    # Try cartopy for geo-referenced plot
    use_cartopy = False
    try:
        import cartopy.crs as ccrs
        import cartopy.feature as cfeature
        use_cartopy = True
    except ImportError:
        pass

    if use_cartopy and is_2d:
        lats_2d = lats.reshape(ny, nx)
        lons_2d = lons.reshape(ny, nx)

        fig, ax = plt.subplots(
            figsize=figsize,
            subplot_kw={"projection": ccrs.LambertConformal(
                central_longitude=float(np.nanmean(lons_2d)),
                central_latitude=float(np.nanmean(lats_2d)),
            )},
        )
        im = ax.pcolormesh(
            lons_2d, lats_2d, data,
            cmap=colormap, vmin=vmin, vmax=vmax,
            transform=ccrs.PlateCarree(),
            shading="auto",
        )
        ax.add_feature(cfeature.COASTLINE, linewidth=0.5)
        ax.add_feature(cfeature.BORDERS, linewidth=0.3)
        ax.add_feature(cfeature.STATES, linewidth=0.2)
        plt.colorbar(im, ax=ax, shrink=0.7, label=msg.units)
    elif is_2d:
        # 2-D coordinates but no cartopy -- plain pcolormesh
        lats_2d = lats.reshape(ny, nx)
        lons_2d = lons.reshape(ny, nx)
        fig, ax = plt.subplots(figsize=figsize)
        im = ax.pcolormesh(
            lons_2d, lats_2d, data,
            cmap=colormap, vmin=vmin, vmax=vmax,
            shading="auto",
        )
        ax.set_xlabel("Longitude")
        ax.set_ylabel("Latitude")
        plt.colorbar(im, ax=ax, shrink=0.7, label=msg.units)
    else:
        # Regular grid -- imshow
        fig, ax = plt.subplots(figsize=figsize)
        extent = [
            float(lons[0]), float(lons[min(nx - 1, len(lons) - 1)]),
            float(lats[0]), float(lats[min(ny - 1, len(lats) - 1)]),
        ]
        im = ax.imshow(
            data, cmap=colormap, vmin=vmin, vmax=vmax,
            extent=extent, origin="upper", aspect="auto",
        )
        ax.set_xlabel("Longitude")
        ax.set_ylabel("Latitude")
        plt.colorbar(im, ax=ax, shrink=0.7, label=msg.units)

    ax.set_title(title)
    plt.tight_layout()

    if save is not None:
        fig.savefig(save, dpi=150, bbox_inches="tight")

    return fig, ax


def compare_to_eccodes(
    path: str,
    *,
    rtol: float = 1e-6,
    atol: float = 1e-10,
    max_messages: Optional[int] = None,
    verbose: bool = True,
) -> dict:
    """Compare rustmet's GRIB2 decoding against ecCodes field-by-field.

    Opens the same file with both rustmet and ecCodes (via the ``eccodes``
    or ``cfgrib`` Python package), unpacks every message, and reports
    differences in the decoded data values and metadata.

    This is useful as a validation / diagnostic tool when bringing up
    rustmet on a new dataset.

    Parameters
    ----------
    path : str
        Path to a GRIB2 file.
    rtol : float, optional
        Relative tolerance for ``numpy.allclose`` (default 1e-6).
    atol : float, optional
        Absolute tolerance for ``numpy.allclose`` (default 1e-10).
    max_messages : int, optional
        Limit comparison to the first *N* messages.  Useful for large files.
    verbose : bool, optional
        If True (default), print a summary line for each message.

    Returns
    -------
    dict
        Summary with keys:

        - ``"total"``: number of messages compared
        - ``"matched"``: number with identical (within tolerance) data
        - ``"mismatched"``: number with differing data
        - ``"skipped"``: number skipped (decode error on either side)
        - ``"details"``: list of per-message dicts with fields
          ``msg_index``, ``variable``, ``level``, ``status``,
          ``max_abs_diff``, ``max_rel_diff``, ``rustmet_shape``,
          ``eccodes_shape``.

    Raises
    ------
    ImportError
        If neither ``eccodes`` nor ``cfgrib`` is installed.
    FileNotFoundError
        If *path* does not exist.

    Examples
    --------
    ::

        report = compare_to_eccodes("hrrr.grib2")
        print(f"{report['matched']}/{report['total']} fields match")
    """
    import rustmet
    import numpy as np

    # Try to import eccodes (the ECMWF low-level bindings)
    eccodes = _import_eccodes()

    # Open with rustmet
    grib_rm = rustmet.open(path)
    rm_messages = grib_rm.messages
    if max_messages is not None:
        rm_messages = rm_messages[:max_messages]

    # Open with eccodes
    ec_messages = _read_eccodes_messages(path, eccodes, max_messages)

    total = min(len(rm_messages), len(ec_messages))
    matched = 0
    mismatched = 0
    skipped = 0
    details = []

    for i in range(total):
        rm_msg = rm_messages[i]
        ec_data = ec_messages[i]

        info = {
            "msg_index": i,
            "variable": rm_msg.variable,
            "level": rm_msg.level,
            "status": "unknown",
            "max_abs_diff": None,
            "max_rel_diff": None,
            "rustmet_shape": None,
            "eccodes_shape": None,
        }

        # Unpack rustmet data
        try:
            rm_vals = np.asarray(rm_msg.values(), dtype=np.float64)
            info["rustmet_shape"] = rm_vals.shape
        except Exception as exc:
            info["status"] = f"rustmet_error: {exc}"
            skipped += 1
            details.append(info)
            if verbose:
                print(f"  [{i:3d}] {rm_msg.variable:12s} {rm_msg.level:25s} -- SKIP (rustmet error)")
            continue

        # ecCodes data
        ec_vals = ec_data.get("values")
        if ec_vals is None:
            info["status"] = "eccodes_error: no values"
            skipped += 1
            details.append(info)
            if verbose:
                print(f"  [{i:3d}] {rm_msg.variable:12s} {rm_msg.level:25s} -- SKIP (eccodes error)")
            continue

        ec_vals = np.asarray(ec_vals, dtype=np.float64)
        info["eccodes_shape"] = ec_vals.shape

        # Shape check
        if rm_vals.shape != ec_vals.shape:
            info["status"] = f"shape_mismatch: rustmet={rm_vals.shape} eccodes={ec_vals.shape}"
            mismatched += 1
            details.append(info)
            if verbose:
                print(f"  [{i:3d}] {rm_msg.variable:12s} {rm_msg.level:25s} -- MISMATCH (shape)")
            continue

        # Replace missing-value sentinels
        rm_clean = np.where(np.abs(rm_vals) > 1e15, np.nan, rm_vals)
        ec_clean = np.where(np.abs(ec_vals) > 1e15, np.nan, ec_vals)

        # Mask where both are NaN
        both_nan = np.isnan(rm_clean) & np.isnan(ec_clean)
        rm_only_nan = np.isnan(rm_clean) & ~np.isnan(ec_clean)
        ec_only_nan = ~np.isnan(rm_clean) & np.isnan(ec_clean)
        nan_mismatch_count = int(np.sum(rm_only_nan) + np.sum(ec_only_nan))

        # Compare valid values
        valid = ~np.isnan(rm_clean) & ~np.isnan(ec_clean)
        if np.any(valid):
            diff = np.abs(rm_clean[valid] - ec_clean[valid])
            max_abs = float(np.max(diff))
            denom = np.maximum(np.abs(ec_clean[valid]), 1e-30)
            max_rel = float(np.max(diff / denom))
            info["max_abs_diff"] = max_abs
            info["max_rel_diff"] = max_rel

            close = np.allclose(rm_clean[valid], ec_clean[valid], rtol=rtol, atol=atol)
        else:
            close = True  # all NaN on both sides
            max_abs = 0.0
            max_rel = 0.0
            info["max_abs_diff"] = 0.0
            info["max_rel_diff"] = 0.0

        if close and nan_mismatch_count == 0:
            info["status"] = "match"
            matched += 1
            if verbose:
                print(f"  [{i:3d}] {rm_msg.variable:12s} {rm_msg.level:25s} -- OK  (max_diff={max_abs:.2e})")
        else:
            reason_parts = []
            if not close:
                reason_parts.append(f"max_abs={max_abs:.2e} max_rel={max_rel:.2e}")
            if nan_mismatch_count > 0:
                reason_parts.append(f"nan_mismatch={nan_mismatch_count}")
            info["status"] = "mismatch: " + ", ".join(reason_parts)
            mismatched += 1
            if verbose:
                print(f"  [{i:3d}] {rm_msg.variable:12s} {rm_msg.level:25s} -- MISMATCH ({', '.join(reason_parts)})")

        details.append(info)

    # Messages present in one but not the other
    if len(rm_messages) != len(ec_messages):
        if verbose:
            print(f"\n  Note: rustmet has {len(rm_messages)} messages, "
                  f"eccodes has {len(ec_messages)} messages")

    summary = {
        "total": total,
        "matched": matched,
        "mismatched": mismatched,
        "skipped": skipped,
        "details": details,
        "rustmet_count": len(rm_messages),
        "eccodes_count": len(ec_messages),
    }

    if verbose:
        print(f"\n  Summary: {matched}/{total} match, "
              f"{mismatched} mismatch, {skipped} skipped")

    return summary


def _import_eccodes():
    """Import the eccodes module, trying multiple package names."""
    try:
        import eccodes
        return eccodes
    except ImportError:
        pass
    # cfgrib ships its own eccodes wrapper in some configurations
    try:
        import cfgrib.messages as eccodes
        return eccodes
    except ImportError:
        pass
    raise ImportError(
        "eccodes or cfgrib is required for compare_to_eccodes(). "
        "Install with: pip install eccodes  or  conda install -c conda-forge eccodes"
    )


def _read_eccodes_messages(path, eccodes, max_messages=None):
    """Read GRIB messages from a file using the eccodes C-library bindings.

    Returns a list of dicts, each containing at minimum a ``values`` key
    with the unpacked numpy array.
    """
    import numpy as np

    messages = []
    with open(path, "rb") as f:
        while True:
            if max_messages is not None and len(messages) >= max_messages:
                break
            try:
                msgid = eccodes.codes_grib_new_from_file(f)
            except Exception:
                break
            if msgid is None:
                break
            try:
                vals = eccodes.codes_get_values(msgid)
                shortName = eccodes.codes_get(msgid, "shortName", ktype=str)
                level = eccodes.codes_get(msgid, "level", ktype=int)
                typeOfLevel = eccodes.codes_get(msgid, "typeOfLevel", ktype=str)
                messages.append({
                    "values": np.asarray(vals, dtype=np.float64),
                    "shortName": shortName,
                    "level": level,
                    "typeOfLevel": typeOfLevel,
                })
            except Exception as exc:
                messages.append({
                    "values": None,
                    "error": str(exc),
                })
            finally:
                try:
                    eccodes.codes_release(msgid)
                except Exception:
                    pass
    return messages
