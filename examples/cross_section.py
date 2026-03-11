"""
Vertical cross-section from pressure-level GRIB2 data.

This example demonstrates:
  - Opening pressure-level GRIB2 data
  - Extracting temperature, dewpoint, and wind at multiple pressure levels
  - Computing derived quantities (theta-e, wind speed)
  - Interpolating a cross-section between two geographic points
  - Plotting the cross-section with matplotlib

Usage:
    python examples/cross_section.py /path/to/gfs_prs.grib2

    # Or download GFS pressure-level data:
    python examples/cross_section.py --download
"""

import sys
import time
import numpy as np
import rustmet


# Cross-section endpoints (lat, lon)
# Example: Oklahoma City to Memphis (through a typical severe weather corridor)
POINT_A = (35.5, -97.5)  # OKC
POINT_B = (35.1, -90.0)  # Memphis


def load_data():
    """Load pressure-level GRIB2 data from file or download."""
    if len(sys.argv) > 1 and sys.argv[1] != "--download":
        print(f"Opening {sys.argv[1]} ...")
        return rustmet.open(sys.argv[1])
    else:
        print("Downloading GFS pressure-level data (latest, f000) ...")
        client = rustmet.Client()
        # Request temperature and RH at standard pressure levels, plus wind
        grib = client.fetch(
            "gfs", "latest", fhour=0, product="prs",
            vars=[
                "TMP:925 mb", "TMP:850 mb", "TMP:700 mb", "TMP:500 mb",
                "TMP:400 mb", "TMP:300 mb", "TMP:250 mb", "TMP:200 mb",
                "RH:925 mb", "RH:850 mb", "RH:700 mb", "RH:500 mb",
                "RH:400 mb", "RH:300 mb", "RH:250 mb", "RH:200 mb",
                "UGRD:925 mb", "UGRD:850 mb", "UGRD:700 mb", "UGRD:500 mb",
                "UGRD:400 mb", "UGRD:300 mb", "UGRD:250 mb", "UGRD:200 mb",
                "VGRD:925 mb", "VGRD:850 mb", "VGRD:700 mb", "VGRD:500 mb",
                "VGRD:400 mb", "VGRD:300 mb", "VGRD:250 mb", "VGRD:200 mb",
            ],
        )
        print(f"  Downloaded {grib.num_messages} messages")
        return grib


def extract_cross_section(lats_2d, lons_2d, field_2d, lat_a, lon_a, lat_b, lon_b, npts=100):
    """
    Bilinear interpolation along a straight line between two points.

    Returns the interpolated values along the transect.
    """
    ny, nx = field_2d.shape
    lat_line = np.linspace(lat_a, lat_b, npts)
    lon_line = np.linspace(lon_a, lon_b, npts)

    # Convert lat/lon to fractional grid indices
    # Assumes regular or near-regular grid; uses first row/col as reference
    lat0, lat1 = lats_2d[0, 0], lats_2d[-1, 0]
    lon0, lon1 = lons_2d[0, 0], lons_2d[0, -1]

    # Handle grids that go north-to-south or south-to-north
    fj = (lat_line - lat0) / (lat1 - lat0) * (ny - 1)
    fi = (lon_line - lon0) / (lon1 - lon0) * (nx - 1)

    # Bilinear interpolation
    values = np.full(npts, np.nan)
    for k in range(npts):
        j = fj[k]
        i = fi[k]
        j0 = int(np.clip(np.floor(j), 0, ny - 2))
        i0 = int(np.clip(np.floor(i), 0, nx - 2))
        dj = j - j0
        di = i - i0
        if 0 <= j0 < ny - 1 and 0 <= i0 < nx - 1:
            values[k] = (
                field_2d[j0, i0] * (1 - dj) * (1 - di)
                + field_2d[j0, i0 + 1] * (1 - dj) * di
                + field_2d[j0 + 1, i0] * dj * (1 - di)
                + field_2d[j0 + 1, i0 + 1] * dj * di
            )

    return lat_line, lon_line, values


def main():
    grib = load_data()

    # Define pressure levels to extract (sorted top-down for plotting)
    plevels = [925, 850, 700, 500, 400, 300, 250, 200]

    # --- Extract fields at each pressure level ---
    # We build 2D arrays: [n_levels, n_transect_points]
    npts = 150  # number of points along the cross-section

    # Get grid shape from any message
    sample_msg = grib.messages[0]
    nx, ny = sample_msg.nx, sample_msg.ny
    lats = sample_msg.lats().reshape(ny, nx)
    lons = sample_msg.lons().reshape(ny, nx)

    print(f"\nGrid: {nx} x {ny}")
    print(f"Cross-section: ({POINT_A[0]:.1f}N, {POINT_A[1]:.1f}W) "
          f"to ({POINT_B[0]:.1f}N, {POINT_B[1]:.1f}W)")
    print(f"Pressure levels: {plevels}")

    # Storage for cross-section data
    cs_theta_e = np.full((len(plevels), npts), np.nan)
    cs_wspd = np.full((len(plevels), npts), np.nan)
    cs_temp = np.full((len(plevels), npts), np.nan)

    for k, plev in enumerate(plevels):
        # Find messages for this level
        t_msg = grib.find("TMP", f"{plev}")
        rh_msg = grib.find("RH", f"{plev}")
        u_msg = grib.find("UGRD", f"{plev}")
        v_msg = grib.find("VGRD", f"{plev}")

        if not all([t_msg, rh_msg, u_msg, v_msg]):
            print(f"  Skipping {plev} hPa (missing fields)")
            continue

        # Unpack and convert
        t_k = t_msg.values_2d()
        t_c_2d = t_k - 273.15

        rh_2d = rh_msg.values_2d()

        # Compute dewpoint from temperature and RH using rustmet
        # Flatten for the _arr function, then reshape
        td_c_flat = rustmet.dewpoint_from_rh_arr(
            t_c_2d.ravel().astype(np.float64),
            rh_2d.ravel().astype(np.float64),
        )
        td_c_2d = td_c_flat.reshape(ny, nx)

        # Compute equivalent potential temperature
        p_arr = np.full(ny * nx, float(plev), dtype=np.float64)
        theta_e_flat = rustmet.thetae_arr(
            p_arr,
            t_c_2d.ravel().astype(np.float64),
            td_c_flat,
        )
        theta_e_2d = theta_e_flat.reshape(ny, nx)

        # Compute wind speed
        u_2d = u_msg.values_2d()
        v_2d = v_msg.values_2d()
        wspd_flat, _ = rustmet.wind_speed_dir(
            u_2d.ravel().astype(np.float64),
            v_2d.ravel().astype(np.float64),
        )
        wspd_2d = wspd_flat.reshape(ny, nx)

        # Convert wind to knots
        wspd_kt_flat = rustmet.convert_units(wspd_flat, "m/s", "kt")
        wspd_kt_2d = wspd_kt_flat.reshape(ny, nx)

        # Extract cross-section at this level
        _, _, cs_theta_e[k, :] = extract_cross_section(
            lats, lons, theta_e_2d,
            POINT_A[0], POINT_A[1], POINT_B[0], POINT_B[1], npts)

        _, _, cs_wspd[k, :] = extract_cross_section(
            lats, lons, wspd_kt_2d,
            POINT_A[0], POINT_A[1], POINT_B[0], POINT_B[1], npts)

        _, _, cs_temp[k, :] = extract_cross_section(
            lats, lons, t_c_2d,
            POINT_A[0], POINT_A[1], POINT_B[0], POINT_B[1], npts)

        print(f"  {plev:4d} hPa: T [{t_c_2d.min():.0f}, {t_c_2d.max():.0f}] C"
              f"  theta_e [{theta_e_2d.min():.0f}, {theta_e_2d.max():.0f}] K")

    # --- Distance axis ---
    # Great-circle distance between endpoints (approximate)
    lat_line = np.linspace(POINT_A[0], POINT_B[0], npts)
    lon_line = np.linspace(POINT_A[1], POINT_B[1], npts)
    dlat = np.radians(np.diff(lat_line))
    dlon = np.radians(np.diff(lon_line))
    lat_rad = np.radians(lat_line[:-1])
    a = np.sin(dlat / 2) ** 2 + np.cos(lat_rad) * np.cos(lat_rad + dlat) * np.sin(dlon / 2) ** 2
    dist_km = np.concatenate([[0], np.cumsum(6371.0 * 2 * np.arctan2(np.sqrt(a), np.sqrt(1 - a)))])

    # --- Plot ---
    try:
        import matplotlib.pyplot as plt
    except ImportError:
        print("\nmatplotlib not available, skipping plot.")
        return

    fig, axes = plt.subplots(1, 2, figsize=(16, 7), sharey=True)
    p_arr_plot = np.array(plevels)

    # Panel 1: Equivalent potential temperature
    ax = axes[0]
    X, Y = np.meshgrid(dist_km, p_arr_plot)
    cf = ax.contourf(X, Y, cs_theta_e, levels=20, cmap="RdYlBu_r")
    cs = ax.contour(X, Y, cs_theta_e, levels=10, colors="k", linewidths=0.5)
    ax.clabel(cs, fontsize=7, fmt="%.0f")
    ax.set_yscale("log")
    ax.set_ylim(1000, 200)
    ax.set_yticks(plevels)
    ax.set_yticklabels([str(p) for p in plevels])
    ax.set_ylabel("Pressure (hPa)")
    ax.set_xlabel("Distance (km)")
    ax.set_title("Equivalent Potential Temperature (K)")
    fig.colorbar(cf, ax=ax, label="theta-e (K)", shrink=0.8)

    # Panel 2: Wind speed
    ax = axes[1]
    cf = ax.contourf(X, Y, cs_wspd, levels=np.arange(0, 120, 10), cmap="YlOrRd")
    cs = ax.contour(X, Y, cs_wspd, levels=np.arange(0, 120, 20), colors="k", linewidths=0.5)
    ax.clabel(cs, fontsize=7, fmt="%.0f")
    ax.set_yscale("log")
    ax.set_ylim(1000, 200)
    ax.set_yticks(plevels)
    ax.set_yticklabels([str(p) for p in plevels])
    ax.set_xlabel("Distance (km)")
    ax.set_title("Wind Speed (kt)")
    fig.colorbar(cf, ax=ax, label="Wind Speed (kt)", shrink=0.8)

    fig.suptitle(
        f"Cross-Section: ({POINT_A[0]:.1f}N, {abs(POINT_A[1]):.1f}W) "
        f"to ({POINT_B[0]:.1f}N, {abs(POINT_B[1]):.1f}W)",
        fontsize=12,
    )
    plt.tight_layout()
    plt.savefig("cross_section.png", dpi=150, bbox_inches="tight")
    print("\nSaved cross_section.png")
    plt.show()


if __name__ == "__main__":
    main()
