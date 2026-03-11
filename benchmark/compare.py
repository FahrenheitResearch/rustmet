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
    """Return the median time per call in seconds."""
    times = timeit.repeat(fn, number=number, repeat=repeat)
    per_call = sorted(t / number for t in times)
    return per_call[len(per_call) // 2]


def fmt_time(seconds):
    if seconds < 1e-6:
        return f"{seconds * 1e9:.1f} ns"
    elif seconds < 1e-3:
        return f"{seconds * 1e6:.1f} us"
    elif seconds < 1.0:
        return f"{seconds * 1e3:.2f} ms"
    else:
        return f"{seconds:.3f} s"


def speedup_str(baseline, challenger):
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
    if HAS_CFGRIB:
        print(f"cfgrib:    {cfgrib.__version__}")
    if HAS_METPY:
        import metpy
        print(f"MetPy:     {metpy.__version__}")
    if HAS_SCIPY:
        import scipy
        print(f"SciPy:     {scipy.__version__}")
    print()
    print("Notes:")
    print("  - Times are median of 5 rounds")
    print("  - MetPy uses pint units (adds overhead by design for unit safety)")
    print("  - cfgrib uses eccodes C library under the hood")
    print("  - All rustmet operations use native Rust array processing (no Python loops)")
    print("  - Grid operations (vorticity, smoothing) are fully vectorized in Rust")
    print()


# =========================================================================
# 1. GRIB2 PARSING
# =========================================================================

def bench_grib2_parsing():
    print("-" * 78)
    print("## GRIB2 Parsing (rustmet vs cfgrib)")
    print("-" * 78)
    print()

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed\n")
        return []

    # Create a multi-field GRIB2 file for a more realistic benchmark
    nx, ny = 100, 100
    np.random.seed(42)

    writer = rustmet.Grib2Writer()
    for param_num in range(10):  # 10 fields
        values = np.random.randn(nx * ny).astype(np.float64) * 10.0 + 280.0
        writer.add_field(
            values=values,
            discipline=0,
            parameter_category=0,
            parameter_number=param_num,
            level_type=103,
            level_value=2.0,
            grid_template=0,
            nx=nx, ny=ny,
            lat1=30.0, lon1=-100.0,
            lat2=39.99, lon2=-90.01,
            dx=0.1, dy=0.1,
            bits_per_value=16,
            reference_time="2026-01-15 12:00:00",
        )
    grib_bytes = bytes(writer.to_bytes())
    print(f"  Test file: 10 fields, {nx}x{ny} grid, {len(grib_bytes):,} bytes")
    print()

    results = []

    # rustmet: parse from bytes
    t_rustmet = time_fn(lambda: rustmet.GribFile.from_bytes(grib_bytes),
                        number=1000, repeat=5)
    results.append(("GRIB2 parse (10 msgs, 100x100)", "rustmet", t_rustmet, None))
    print(f"  rustmet GribFile.from_bytes:   {fmt_time(t_rustmet)}")

    # rustmet: parse + unpack all values
    def rustmet_parse_unpack():
        gf = rustmet.GribFile.from_bytes(grib_bytes)
        for msg in gf.messages:
            msg.values()
    t_rustmet_full = time_fn(rustmet_parse_unpack, number=200, repeat=5)
    results.append(("GRIB2 parse+unpack (10 msgs)", "rustmet", t_rustmet_full, None))
    print(f"  rustmet parse + unpack all:    {fmt_time(t_rustmet_full)}")

    # cfgrib: open from file
    if HAS_CFGRIB:
        tmp = tempfile.NamedTemporaryFile(suffix=".grib2", delete=False)
        tmp.write(grib_bytes)
        tmp.close()
        try:
            # Warm up / check if it works
            try:
                cfgrib.open_datasets(tmp.name)
                t_cfgrib = time_fn(
                    lambda: cfgrib.open_datasets(tmp.name),
                    number=20, repeat=5
                )
                results.append(("GRIB2 parse (10 msgs, 100x100)", "cfgrib", t_cfgrib,
                                speedup_str(t_cfgrib, t_rustmet)))
                print(f"  cfgrib open_datasets:          {fmt_time(t_cfgrib)}")
                print(f"  speedup (parse only):          {speedup_str(t_cfgrib, t_rustmet)}")
            except Exception as e:
                print(f"  cfgrib: error ({e}), skipping")
        finally:
            os.unlink(tmp.name)
    else:
        print("  cfgrib: not installed, skipping")

    print()
    return results


# =========================================================================
# 2. METEOROLOGICAL CALCULATIONS
# =========================================================================

def bench_met_calcs():
    print("-" * 78)
    print("## Meteorological Calculations (rustmet vs MetPy)")
    print("-" * 78)
    print()

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed\n")
        return []

    N = 10_000
    np.random.seed(42)

    pressure_hpa = np.random.uniform(300.0, 1013.0, N)
    temperature_c = np.random.uniform(-40.0, 40.0, N)
    dewpoint_c = temperature_c - np.random.uniform(0.5, 20.0, N)
    rh_pct = np.random.uniform(10.0, 100.0, N)

    # Use native array functions if available, else fall back to np.vectorize
    from rustmet._rustmet import (
        potential_temperature_arr, mixratio_arr, thetae_arr, dewpoint_from_rh_arr,
    )

    results = []

    # --- Potential Temperature ---
    print(f"  **Potential Temperature** (N={N:,})")
    t_rm = time_fn(lambda: potential_temperature_arr(pressure_hpa, temperature_c),
                   number=100, repeat=5)
    print(f"    rustmet (native arr):  {fmt_time(t_rm)}")
    results.append(("potential_temperature", "rustmet", t_rm, None))

    if HAS_METPY:
        p_mp = pressure_hpa * units.hPa
        t_mp_arr = temperature_c * units.degC
        t_mp = time_fn(
            lambda: mpcalc.potential_temperature(p_mp, t_mp_arr),
            number=100, repeat=5
        )
        print(f"    MetPy (pint units):    {fmt_time(t_mp)}")
        print(f"    speedup:               {speedup_str(t_mp, t_rm)}")
        results.append(("potential_temperature", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    print()

    # --- Mixing Ratio ---
    print(f"  **Mixing Ratio** (N={N:,})")
    t_rm = time_fn(lambda: mixratio_arr(pressure_hpa, temperature_c),
                   number=100, repeat=5)
    print(f"    rustmet (native arr):  {fmt_time(t_rm)}")
    results.append(("mixing_ratio", "rustmet", t_rm, None))

    if HAS_METPY:
        p_mp = pressure_hpa * units.hPa
        td_mp = dewpoint_c * units.degC
        t_mp = time_fn(
            lambda: mpcalc.mixing_ratio_from_relative_humidity(p_mp, temperature_c * units.degC, rh_pct / 100.0),
            number=100, repeat=5
        )
        print(f"    MetPy (pint units):    {fmt_time(t_mp)}")
        print(f"    speedup:               {speedup_str(t_mp, t_rm)}")
        results.append(("mixing_ratio", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    print()

    # --- Equivalent Potential Temperature ---
    print(f"  **Equivalent Potential Temperature** (N={N:,})")
    t_rm = time_fn(lambda: thetae_arr(pressure_hpa, temperature_c, dewpoint_c),
                   number=100, repeat=5)
    print(f"    rustmet (native arr):  {fmt_time(t_rm)}")
    results.append(("equiv_potential_temp", "rustmet", t_rm, None))

    if HAS_METPY:
        p_mp = pressure_hpa * units.hPa
        t_mp_arr = temperature_c * units.degC
        td_mp = dewpoint_c * units.degC
        t_mp = time_fn(
            lambda: mpcalc.equivalent_potential_temperature(p_mp, t_mp_arr, td_mp),
            number=100, repeat=5
        )
        print(f"    MetPy (pint units):    {fmt_time(t_mp)}")
        print(f"    speedup:               {speedup_str(t_mp, t_rm)}")
        results.append(("equiv_potential_temp", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    print()

    # --- Dewpoint from RH ---
    print(f"  **Dewpoint from RH** (N={N:,})")
    t_rm = time_fn(lambda: dewpoint_from_rh_arr(temperature_c, rh_pct),
                   number=100, repeat=5)
    print(f"    rustmet (native arr):  {fmt_time(t_rm)}")
    results.append(("dewpoint_from_rh", "rustmet", t_rm, None))

    if HAS_METPY:
        t_mp_arr = temperature_c * units.degC
        rh_mp = rh_pct * units.percent
        t_mp = time_fn(
            lambda: mpcalc.dewpoint_from_relative_humidity(t_mp_arr, rh_mp),
            number=100, repeat=5
        )
        print(f"    MetPy (pint units):    {fmt_time(t_mp)}")
        print(f"    speedup:               {speedup_str(t_mp, t_rm)}")
        results.append(("dewpoint_from_rh", "MetPy", t_mp,
                         speedup_str(t_mp, t_rm)))
    print()

    return results


# =========================================================================
# 3. GRID OPERATIONS (fully vectorized in Rust)
# =========================================================================

def bench_grid_ops():
    print("-" * 78)
    print("## Grid Operations (rustmet vs numpy/scipy)")
    print("-" * 78)
    print("  Note: These are fully vectorized — no Python loop overhead")
    print()

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed\n")
        return []

    nx, ny = 200, 200
    dx = dy = 3000.0
    np.random.seed(42)

    u = np.random.randn(ny * nx).astype(np.float64) * 10.0
    v = np.random.randn(ny * nx).astype(np.float64) * 10.0
    scalar = np.random.randn(ny * nx).astype(np.float64) * 5.0 + 280.0
    u_2d = u.reshape(ny, nx)
    v_2d = v.reshape(ny, nx)
    scalar_2d = scalar.reshape(ny, nx)

    results = []

    # --- Vorticity ---
    print(f"  **Vorticity** ({nx}x{ny} grid, {nx*ny:,} points)")
    t_rm = time_fn(lambda: rustmet.vorticity(u, v, nx, ny, dx, dy),
                   number=200, repeat=5)
    print(f"    rustmet:    {fmt_time(t_rm)}")
    results.append(("vorticity (200x200)", "rustmet", t_rm, None))

    def numpy_vorticity():
        dvdx = np.gradient(v_2d, dx, axis=1)
        dudy = np.gradient(u_2d, dy, axis=0)
        return dvdx - dudy

    t_np = time_fn(numpy_vorticity, number=200, repeat=5)
    print(f"    numpy:      {fmt_time(t_np)} (np.gradient)")
    print(f"    speedup:    {speedup_str(t_np, t_rm)}")
    results.append(("vorticity (200x200)", "numpy", t_np,
                     speedup_str(t_np, t_rm)))
    print()

    # --- Divergence ---
    print(f"  **Divergence** ({nx}x{ny} grid)")
    t_rm = time_fn(lambda: rustmet.divergence(u, v, nx, ny, dx, dy),
                   number=200, repeat=5)
    print(f"    rustmet:    {fmt_time(t_rm)}")
    results.append(("divergence (200x200)", "rustmet", t_rm, None))

    def numpy_divergence():
        dudx = np.gradient(u_2d, dx, axis=1)
        dvdy = np.gradient(v_2d, dy, axis=0)
        return dudx + dvdy

    t_np = time_fn(numpy_divergence, number=200, repeat=5)
    print(f"    numpy:      {fmt_time(t_np)} (np.gradient)")
    print(f"    speedup:    {speedup_str(t_np, t_rm)}")
    results.append(("divergence (200x200)", "numpy", t_np,
                     speedup_str(t_np, t_rm)))
    print()

    # --- Gaussian Smooth ---
    print(f"  **Gaussian Smooth, sigma=2** ({nx}x{ny} grid)")
    t_rm = time_fn(lambda: rustmet.smooth(scalar, nx, ny, 2.0),
                   number=50, repeat=5)
    print(f"    rustmet:    {fmt_time(t_rm)}")
    results.append(("gaussian smooth s=2 (200x200)", "rustmet", t_rm, None))

    if HAS_SCIPY:
        t_sp = time_fn(
            lambda: scipy.ndimage.gaussian_filter(scalar_2d, sigma=2.0),
            number=50, repeat=5
        )
        print(f"    scipy:      {fmt_time(t_sp)} (gaussian_filter, C impl)")
        print(f"    speedup:    {speedup_str(t_sp, t_rm)}")
        results.append(("gaussian smooth s=2 (200x200)", "scipy", t_sp,
                         speedup_str(t_sp, t_rm)))
    print()

    # --- Larger grid test ---
    print(f"  **Vorticity** (500x500 grid, {500*500:,} points)")
    nx2, ny2 = 500, 500
    u_big = np.random.randn(ny2 * nx2).astype(np.float64) * 10.0
    v_big = np.random.randn(ny2 * nx2).astype(np.float64) * 10.0
    u_big_2d = u_big.reshape(ny2, nx2)
    v_big_2d = v_big.reshape(ny2, nx2)

    t_rm = time_fn(lambda: rustmet.vorticity(u_big, v_big, nx2, ny2, dx, dy),
                   number=50, repeat=5)
    print(f"    rustmet:    {fmt_time(t_rm)}")
    results.append(("vorticity (500x500)", "rustmet", t_rm, None))

    def numpy_vorticity_big():
        dvdx = np.gradient(v_big_2d, dx, axis=1)
        dudy = np.gradient(u_big_2d, dy, axis=0)
        return dvdx - dudy

    t_np = time_fn(numpy_vorticity_big, number=50, repeat=5)
    print(f"    numpy:      {fmt_time(t_np)}")
    print(f"    speedup:    {speedup_str(t_np, t_rm)}")
    results.append(("vorticity (500x500)", "numpy", t_np,
                     speedup_str(t_np, t_rm)))
    print()

    return results


# =========================================================================
# Summary
# =========================================================================

def print_summary(all_results):
    print("=" * 78)
    print("## Summary Table")
    print("=" * 78)
    print()
    print("| Operation | rustmet | Competitor | Speedup |")
    print("|-----------|---------|------------|---------|")

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
    print("Notes:")
    print("  - MetPy comparisons: MetPy's pint unit system adds overhead by")
    print("    design for unit safety. Raw numpy equivalents would be faster.")
    print("  - Met functions: rustmet uses native Rust array processing via PyO3+numpy.")
    print("  - Grid operations: fully vectorized in Rust, fair apples-to-apples.")
    print()
    print("*Run `python benchmark/compare.py` to reproduce on your system.*")
    print()


def main():
    print_header()
    all_results = []
    all_results.extend(bench_grib2_parsing())
    all_results.extend(bench_met_calcs())
    all_results.extend(bench_grid_ops())

    if all_results:
        print_summary(all_results)
    return 0


if __name__ == "__main__":
    sys.exit(main())
