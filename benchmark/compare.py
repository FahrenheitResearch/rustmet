#!/usr/bin/env python3
"""
Competitive benchmark: rustmet vs cfgrib, MetPy, and scipy.

Compares equivalent operations across libraries with fair, reproducible timing.
Run with:  python benchmark/compare.py

Libraries are imported with try/except so missing ones are skipped gracefully.
"""

import sys
import platform
import timeit
import tempfile
import os

import numpy as np

# ---------------------------------------------------------------------------
# Library availability
# ---------------------------------------------------------------------------

HAS_RUSTMET = False
HAS_CFGRIB = False
HAS_METPY = False
HAS_SCIPY = False

try:
    import rustmet
    HAS_RUSTMET = True
except ImportError:
    pass

try:
    import cfgrib
    HAS_CFGRIB = True
except ImportError:
    pass

try:
    import metpy.calc as mpcalc
    from metpy.units import units
    HAS_METPY = True
except ImportError:
    pass

try:
    import scipy.ndimage
    HAS_SCIPY = True
except ImportError:
    pass


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def time_fn(fn, number=100, repeat=5):
    """Return the best median time in seconds across `repeat` rounds."""
    times = timeit.repeat(fn, number=number, repeat=repeat)
    # Each entry is total time for `number` calls; convert to per-call
    per_call = [t / number for t in times]
    per_call.sort()
    return per_call[len(per_call) // 2]  # median


def fmt_time(seconds):
    """Format a time value for display."""
    if seconds < 1e-6:
        return f"{seconds * 1e9:.1f} ns"
    elif seconds < 1e-3:
        return f"{seconds * 1e6:.1f} us"
    elif seconds < 1.0:
        return f"{seconds * 1e3:.2f} ms"
    else:
        return f"{seconds:.3f} s"


def speedup_str(baseline, challenger):
    """Return 'Nx faster' or 'Nx slower' string."""
    if challenger == 0 or baseline == 0:
        return "N/A"
    ratio = baseline / challenger
    if ratio >= 1.0:
        return f"{ratio:.1f}x faster"
    else:
        return f"{1.0 / ratio:.1f}x slower"


def print_header():
    print("=" * 78)
    print("rustmet Competitive Benchmark")
    print("=" * 78)
    print(f"Platform:  {platform.system()} {platform.release()} ({platform.machine()})")
    print(f"Python:    {platform.python_version()}")
    print(f"NumPy:     {np.__version__}")
    if HAS_RUSTMET:
        print(f"rustmet:   {rustmet.__version__}")
    else:
        print("rustmet:   NOT INSTALLED (skipping rustmet benchmarks)")
    if HAS_CFGRIB:
        print(f"cfgrib:    {cfgrib.__version__}")
    else:
        print("cfgrib:    not installed (skipping cfgrib comparisons)")
    if HAS_METPY:
        import metpy
        print(f"MetPy:     {metpy.__version__}")
    else:
        print("MetPy:     not installed (skipping MetPy comparisons)")
    if HAS_SCIPY:
        print(f"SciPy:     {scipy.__version__}")
    else:
        print("SciPy:     not installed (skipping SciPy comparisons)")
    print()
    print("Notes:")
    print("  - Times are median of 5 rounds, each with 100+ iterations")
    print("  - MetPy uses pint units which adds per-call overhead vs raw numpy")
    print("  - cfgrib uses eccodes C library under the hood")
    print("  - 'speedup' = baseline_time / rustmet_time")
    print()


# =========================================================================
# 1. GRIB2 PARSING BENCHMARK
# =========================================================================

def bench_grib2_parsing():
    print("-" * 78)
    print("## GRIB2 Parsing (rustmet vs cfgrib)")
    print("-" * 78)
    print()

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return []

    # Create a test GRIB2 file using rustmet's writer
    nx, ny = 50, 50
    values = np.random.randn(nx * ny).astype(np.float64) * 10.0 + 280.0

    writer = rustmet.Grib2Writer()
    writer.add_field(
        values=values,
        discipline=0,
        parameter_category=0,
        parameter_number=0,
        level_type=103,
        level_value=2.0,
        grid_template=0,
        nx=nx, ny=ny,
        lat1=30.0, lon1=-100.0,
        lat2=39.0, lon2=-91.0,
        dx=0.2, dy=0.2,
        bits_per_value=16,
        reference_time="2026-01-15 12:00:00",
    )
    grib_bytes = bytes(writer.to_bytes())

    # Write to temp file for cfgrib (which needs a file path)
    tmp = tempfile.NamedTemporaryFile(suffix=".grib2", delete=False)
    tmp.write(grib_bytes)
    tmp.close()
    tmp_path = tmp.name

    results = []

    try:
        # --- rustmet: parse from bytes ---
        t_rustmet = time_fn(lambda: rustmet.GribFile.from_bytes(grib_bytes),
                            number=500, repeat=5)
        results.append(("GRIB2 parse (50x50)", "rustmet", t_rustmet, None))
        print(f"  rustmet GribFile.from_bytes:  {fmt_time(t_rustmet)}")

        # --- cfgrib: open_datasets from file ---
        if HAS_CFGRIB:
            try:
                t_cfgrib = time_fn(
                    lambda: cfgrib.open_datasets(tmp_path),
                    number=50, repeat=5
                )
                results.append(("GRIB2 parse (50x50)", "cfgrib", t_cfgrib,
                                speedup_str(t_cfgrib, t_rustmet)))
                print(f"  cfgrib open_datasets:         {fmt_time(t_cfgrib)}")
                print(f"  speedup:                      {speedup_str(t_cfgrib, t_rustmet)}")
            except Exception as e:
                print(f"  cfgrib: error ({e}), skipping")
        else:
            print("  cfgrib: not installed, skipping")
    finally:
        os.unlink(tmp_path)

    print()
    return results


# =========================================================================
# 2. METEOROLOGICAL CALCULATIONS BENCHMARK
# =========================================================================

def bench_met_calcs():
    print("-" * 78)
    print("## Meteorological Calculations (rustmet vs MetPy)")
    print("-" * 78)
    print()

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return []

    N = 10_000
    np.random.seed(42)

    # Realistic atmospheric values
    pressure_hpa = np.random.uniform(300.0, 1013.0, N)      # hPa
    temperature_c = np.random.uniform(-40.0, 40.0, N)       # Celsius
    dewpoint_c = temperature_c - np.random.uniform(0.5, 20.0, N)  # Td <= T
    rh_pct = np.random.uniform(10.0, 100.0, N)              # percent

    results = []

    # --- Potential Temperature ---
    print("  **Potential Temperature** (N=10,000)")
    t_rm = time_fn(
        lambda: np.array([rustmet.potential_temperature(p, t)
                          for p, t in zip(pressure_hpa, temperature_c)]),
        number=20, repeat=5
    )
    print(f"    rustmet:  {fmt_time(t_rm)}")
    results.append(("potential_temperature (10k)", "rustmet", t_rm, None))

    if HAS_METPY:
        p_metpy = pressure_hpa * units.hPa
        t_metpy = temperature_c * units.degC
        t_mp = time_fn(
            lambda: mpcalc.potential_temperature(p_metpy, t_metpy),
            number=20, repeat=5
        )
        print(f"    MetPy:    {fmt_time(t_mp)} (includes pint unit overhead)")
        print(f"    speedup:  {speedup_str(t_mp, t_rm)}")
        results.append(("potential_temperature (10k)", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    else:
        print("    MetPy: not installed, skipping")
    print()

    # --- Mixing Ratio ---
    print("  **Mixing Ratio** (N=10,000)")
    t_rm = time_fn(
        lambda: np.array([rustmet.mixratio(p, t)
                          for p, t in zip(pressure_hpa, temperature_c)]),
        number=20, repeat=5
    )
    print(f"    rustmet:  {fmt_time(t_rm)}")
    results.append(("mixratio (10k)", "rustmet", t_rm, None))

    if HAS_METPY:
        p_metpy = pressure_hpa * units.hPa
        td_metpy = dewpoint_c * units.degC
        t_mp = time_fn(
            lambda: mpcalc.mixing_ratio_from_dewpoint(p_metpy, td_metpy),
            number=20, repeat=5
        )
        print(f"    MetPy:    {fmt_time(t_mp)} (mixing_ratio_from_dewpoint, pint units)")
        print(f"    speedup:  {speedup_str(t_mp, t_rm)}")
        results.append(("mixratio (10k)", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    else:
        print("    MetPy: not installed, skipping")
    print()

    # --- Equivalent Potential Temperature ---
    print("  **Equivalent Potential Temperature** (N=10,000)")
    t_rm = time_fn(
        lambda: np.array([rustmet.equivalent_potential_temperature(p, t, td)
                          for p, t, td in zip(pressure_hpa, temperature_c,
                                              dewpoint_c)]),
        number=20, repeat=5
    )
    print(f"    rustmet:  {fmt_time(t_rm)}")
    results.append(("equiv_pot_temp (10k)", "rustmet", t_rm, None))

    if HAS_METPY:
        p_metpy = pressure_hpa * units.hPa
        t_metpy = temperature_c * units.degC
        td_metpy = dewpoint_c * units.degC
        t_mp = time_fn(
            lambda: mpcalc.equivalent_potential_temperature(
                p_metpy, t_metpy, td_metpy),
            number=20, repeat=5
        )
        print(f"    MetPy:    {fmt_time(t_mp)} (includes pint unit overhead)")
        print(f"    speedup:  {speedup_str(t_mp, t_rm)}")
        results.append(("equiv_pot_temp (10k)", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    else:
        print("    MetPy: not installed, skipping")
    print()

    # --- Dewpoint from RH ---
    print("  **Dewpoint from RH** (N=10,000)")
    t_rm = time_fn(
        lambda: np.array([rustmet.dewpoint_from_rh(t, rh)
                          for t, rh in zip(temperature_c, rh_pct)]),
        number=20, repeat=5
    )
    print(f"    rustmet:  {fmt_time(t_rm)}")
    results.append(("dewpoint_from_rh (10k)", "rustmet", t_rm, None))

    if HAS_METPY:
        t_metpy = temperature_c * units.degC
        rh_metpy = rh_pct * units.percent
        t_mp = time_fn(
            lambda: mpcalc.dewpoint_from_relative_humidity(t_metpy, rh_metpy),
            number=20, repeat=5
        )
        print(f"    MetPy:    {fmt_time(t_mp)} (includes pint unit overhead)")
        print(f"    speedup:  {speedup_str(t_mp, t_rm)}")
        results.append(("dewpoint_from_rh (10k)", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    else:
        print("    MetPy: not installed, skipping")
    print()

    return results


# =========================================================================
# 3. GRID OPERATIONS BENCHMARK
# =========================================================================

def bench_grid_ops():
    print("-" * 78)
    print("## Grid Operations (rustmet vs numpy/scipy)")
    print("-" * 78)
    print()

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return []

    nx, ny = 200, 200
    dx = dy = 3000.0  # 3 km grid spacing in meters
    np.random.seed(42)

    u = np.random.randn(ny, nx).astype(np.float64) * 10.0  # m/s
    v = np.random.randn(ny, nx).astype(np.float64) * 10.0
    scalar = np.random.randn(ny, nx).astype(np.float64) * 5.0 + 280.0

    u_flat = u.ravel()
    v_flat = v.ravel()
    scalar_flat = scalar.ravel()

    results = []

    # --- Vorticity ---
    print("  **Vorticity** (200x200 grid)")
    t_rm = time_fn(
        lambda: rustmet.vorticity(u_flat, v_flat, nx, ny, dx, dy),
        number=100, repeat=5
    )
    print(f"    rustmet:    {fmt_time(t_rm)}")
    results.append(("vorticity (200x200)", "rustmet", t_rm, None))

    # numpy manual gradient equivalent
    def numpy_vorticity():
        dvdx = np.gradient(v, dx, axis=1)
        dudy = np.gradient(u, dy, axis=0)
        return dvdx - dudy

    t_np = time_fn(numpy_vorticity, number=100, repeat=5)
    print(f"    numpy:      {fmt_time(t_np)} (np.gradient, 2nd order)")
    print(f"    speedup:    {speedup_str(t_np, t_rm)}")
    results.append(("vorticity (200x200)", "numpy", t_np,
                     speedup_str(t_np, t_rm)))
    print()

    # --- Gaussian Smooth ---
    print("  **Gaussian Smooth, sigma=2** (200x200 grid)")
    t_rm = time_fn(
        lambda: rustmet.smooth(scalar_flat, nx, ny, 2.0),
        number=100, repeat=5
    )
    print(f"    rustmet:    {fmt_time(t_rm)}")
    results.append(("smooth gaussian s=2 (200x200)", "rustmet", t_rm, None))

    if HAS_SCIPY:
        t_sp = time_fn(
            lambda: scipy.ndimage.gaussian_filter(scalar, sigma=2.0),
            number=100, repeat=5
        )
        print(f"    scipy:      {fmt_time(t_sp)} (gaussian_filter)")
        print(f"    speedup:    {speedup_str(t_sp, t_rm)}")
        results.append(("smooth gaussian s=2 (200x200)", "scipy", t_sp,
                         speedup_str(t_sp, t_rm)))
    else:
        print("    scipy: not installed, skipping")
    print()

    return results


# =========================================================================
# Summary table
# =========================================================================

def print_summary(all_results):
    print("=" * 78)
    print("## Summary Table")
    print("=" * 78)
    print()
    print("| Operation | rustmet | Competitor | Speedup |")
    print("|-----------|---------|------------|---------|")

    # Group results by operation
    ops = {}
    for (op, lib, t, spd) in all_results:
        if op not in ops:
            ops[op] = {}
        ops[op][lib] = (t, spd)

    for op, libs in ops.items():
        rm_time = libs.get("rustmet", (None, None))[0]
        for lib, (t, spd) in libs.items():
            if lib == "rustmet":
                continue
            rm_str = fmt_time(rm_time) if rm_time else "N/A"
            comp_str = f"{lib}: {fmt_time(t)}"
            spd_str = spd if spd else "N/A"
            print(f"| {op} | {rm_str} | {comp_str} | {spd_str} |")

    print()
    print("*Run `python benchmark/compare.py` to reproduce these numbers on your system.*")
    print()


# =========================================================================
# Main
# =========================================================================

def main():
    print_header()

    all_results = []
    all_results.extend(bench_grib2_parsing())
    all_results.extend(bench_met_calcs())
    all_results.extend(bench_grid_ops())

    if all_results:
        print_summary(all_results)
    else:
        print("No benchmarks could run. Install rustmet and at least one")
        print("comparison library (cfgrib, metpy, or scipy).")

    return 0


if __name__ == "__main__":
    sys.exit(main())
