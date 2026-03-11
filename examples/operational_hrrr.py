"""
Operational HRRR surface analysis with rustmet.

This example demonstrates:
  - Opening a local HRRR GRIB2 file (or downloading one)
  - Searching for 2m temperature, 10m wind, surface pressure
  - Unpacking fields and converting units
  - Plotting with matplotlib
  - Timing comparison vs cfgrib

Usage:
    python examples/operational_hrrr.py /path/to/hrrr.t00z.wrfsfcf00.grib2

    # Or let rustmet download it:
    python examples/operational_hrrr.py --download
"""

import sys
import time
import numpy as np
import rustmet


def load_from_file(path):
    """Open a local HRRR surface GRIB2 file."""
    print(f"Opening {path} ...")
    t0 = time.perf_counter()
    grib = rustmet.open(path)
    elapsed = time.perf_counter() - t0
    print(f"  Parsed {grib.num_messages} messages in {elapsed:.3f}s")
    return grib


def load_from_download():
    """Download HRRR surface data for latest available run."""
    print("Downloading HRRR surface data (latest run, f000) ...")
    client = rustmet.Client()
    t0 = time.perf_counter()

    # Download only the fields we need using byte-range requests.
    # This pulls ~5 MB instead of the full ~100 MB file.
    grib = client.fetch(
        "hrrr", "latest", fhour=0, product="sfc",
        vars=[
            "TMP:2 m above ground",
            "UGRD:10 m above ground",
            "VGRD:10 m above ground",
            "PRES:surface",
        ],
    )
    elapsed = time.perf_counter() - t0
    print(f"  Downloaded {grib.num_messages} messages in {elapsed:.1f}s")
    print(f"  Cache dir: {client.cache_dir()}")
    return grib


def main():
    # --- Load data ---
    if len(sys.argv) > 1 and sys.argv[1] != "--download":
        grib = load_from_file(sys.argv[1])
    else:
        grib = load_from_download()

    # Print a quick inventory
    print("\nInventory:")
    for line in grib.inventory():
        print(f"  {line}")

    # --- Search for fields using fuzzy queries ---
    # search() returns a ranked list of matching messages.
    t2m_msg = grib.search("temperature 2m")[0]
    u10_msg = grib.search("u wind 10m")[0]
    v10_msg = grib.search("v wind 10m")[0]

    # For surface pressure, try search first; fall back to find()
    psfc_results = grib.search("surface pressure")
    if psfc_results:
        psfc_msg = psfc_results[0]
    else:
        psfc_msg = grib.find("PRES", "surface")

    print(f"\n2m Temperature: {t2m_msg}")
    print(f"10m U-Wind:     {u10_msg}")
    print(f"10m V-Wind:     {v10_msg}")
    if psfc_msg:
        print(f"Sfc Pressure:   {psfc_msg}")

    # --- Unpack and convert units ---
    # Temperature: GRIB2 stores in Kelvin, convert to Fahrenheit
    t2m_k = t2m_msg.values()
    t2m_f = rustmet.convert_units(t2m_k, "K", "F")

    # Wind: compute speed and direction from U/V components
    u10 = u10_msg.values()
    v10 = v10_msg.values()
    wspd, wdir = rustmet.wind_speed_dir(u10, v10)

    # Convert wind speed from m/s to knots
    wspd_kt = rustmet.convert_units(wspd, "m/s", "kt")

    # Get grid coordinates
    lats = t2m_msg.lats()
    lons = t2m_msg.lons()
    nx, ny = t2m_msg.nx, t2m_msg.ny

    # Reshape everything to 2D for plotting
    t2m_2d = t2m_f.reshape(ny, nx)
    wspd_2d = wspd_kt.reshape(ny, nx)
    lats_2d = lats.reshape(ny, nx)
    lons_2d = lons.reshape(ny, nx)

    # --- Field statistics ---
    stats = rustmet.field_stats(t2m_f)
    print(f"\n2m Temperature (F):")
    print(f"  min={stats['min']:.1f}  max={stats['max']:.1f}  mean={stats['mean']:.1f}")

    stats_wind = rustmet.field_stats(wspd_kt)
    print(f"10m Wind Speed (kt):")
    print(f"  min={stats_wind['min']:.1f}  max={stats_wind['max']:.1f}  mean={stats_wind['mean']:.1f}")

    # --- Plot with matplotlib ---
    try:
        import matplotlib.pyplot as plt
        import matplotlib.colors as mcolors
    except ImportError:
        print("\nmatplotlib not available, skipping plot.")
        return

    fig, axes = plt.subplots(1, 2, figsize=(16, 6))

    # Panel 1: 2m Temperature
    ax = axes[0]
    im = ax.pcolormesh(lons_2d, lats_2d, t2m_2d, cmap="RdYlBu_r",
                       vmin=20, vmax=100, shading="auto")
    ax.set_title(f"HRRR 2m Temperature (F)\n{t2m_msg.reference_time} f{t2m_msg.forecast_time:03d}")
    ax.set_xlabel("Longitude")
    ax.set_ylabel("Latitude")
    fig.colorbar(im, ax=ax, label="Temperature (F)", shrink=0.8)

    # Panel 2: 10m Wind Speed
    ax = axes[1]
    im = ax.pcolormesh(lons_2d, lats_2d, wspd_2d, cmap="YlOrRd",
                       vmin=0, vmax=40, shading="auto")
    ax.set_title(f"HRRR 10m Wind Speed (kt)\n{u10_msg.reference_time} f{u10_msg.forecast_time:03d}")
    ax.set_xlabel("Longitude")
    ax.set_ylabel("Latitude")
    fig.colorbar(im, ax=ax, label="Wind Speed (kt)", shrink=0.8)

    plt.tight_layout()
    plt.savefig("hrrr_surface.png", dpi=150, bbox_inches="tight")
    print("\nSaved hrrr_surface.png")
    plt.show()


def compare_with_cfgrib(path):
    """
    Optional timing comparison: parse the same file with cfgrib.
    Run with: python examples/operational_hrrr.py /path/to/file.grib2 --compare
    """
    print("\n--- Timing comparison ---")

    # rustmet
    t0 = time.perf_counter()
    grib = rustmet.open(path)
    msg = grib.search("temperature 2m")[0]
    data = msg.values_2d()
    t_rustmet = time.perf_counter() - t0
    print(f"rustmet:  parse + search + unpack in {t_rustmet:.3f}s")

    # cfgrib (if available)
    try:
        import cfgrib
        t0 = time.perf_counter()
        datasets = cfgrib.open_datasets(path)
        t_cfgrib = time.perf_counter() - t0
        print(f"cfgrib:   open_datasets in {t_cfgrib:.3f}s")
        print(f"Speedup:  {t_cfgrib / t_rustmet:.1f}x")
    except ImportError:
        print("cfgrib not installed, skipping comparison.")


if __name__ == "__main__":
    main()

    # Run comparison if --compare flag is present
    if "--compare" in sys.argv and len(sys.argv) > 1:
        path = [a for a in sys.argv[1:] if not a.startswith("--")][0]
        compare_with_cfgrib(path)
