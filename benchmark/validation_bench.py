#!/usr/bin/env python3
"""
Official validation and benchmark harness for rustmet.

Runs correctness tests (GRIB roundtrip, thermo parity vs MetPy) and
performance benchmarks (GRIB, thermo, grid math) with clean summary output.

Exit code 0 if all correctness checks pass, 1 otherwise.

Run with:  python benchmark/validation_bench.py
"""

import sys
import platform
import timeit
import tempfile
import os
import traceback

import numpy as np

# ---------------------------------------------------------------------------
# Library availability
# ---------------------------------------------------------------------------

HAS_RUSTMET = False
HAS_CFGRIB = False
HAS_ECCODES = False
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
    import eccodes
    HAS_ECCODES = True
except ImportError:
    pass

try:
    import metpy.calc as mpcalc
    from metpy.units import units as metpy_units
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

class ValidationResult:
    """Tracks pass/fail for a single correctness check."""
    def __init__(self, name, passed, detail=""):
        self.name = name
        self.passed = passed
        self.detail = detail

    def __repr__(self):
        status = "PASS" if self.passed else "FAIL"
        return f"[{status}] {self.name}: {self.detail}"


class BenchmarkResult:
    """Tracks a single timing measurement."""
    def __init__(self, category, operation, library, time_s, speedup=None):
        self.category = category
        self.operation = operation
        self.library = library
        self.time_s = time_s
        self.speedup = speedup


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
    print("rustmet Validation & Benchmark Harness")
    print("=" * 78)
    print(f"Platform:  {platform.system()} {platform.release()} ({platform.machine()})")
    print(f"Python:    {platform.python_version()}")
    print(f"NumPy:     {np.__version__}")
    if HAS_RUSTMET:
        v = getattr(rustmet, "__version__", "unknown")
        print(f"rustmet:   {v}")
    else:
        print("rustmet:   NOT INSTALLED")
    if HAS_ECCODES:
        print(f"eccodes:   available")
    if HAS_CFGRIB:
        print(f"cfgrib:    {cfgrib.__version__}")
    if HAS_METPY:
        import metpy
        print(f"MetPy:     {metpy.__version__}")
    if HAS_SCIPY:
        import scipy
        print(f"SciPy:     {scipy.__version__}")
    print()


# =========================================================================
# 1. GRIB Correctness Tests
# =========================================================================

def test_grib_correctness():
    """GRIB2 correctness: roundtrip and cross-library comparison."""
    print("-" * 78)
    print("## GRIB2 Correctness Tests")
    print("-" * 78)
    print()

    results = []

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return results

    # -- Test 1: Synthetic roundtrip (write with rustmet, read back, verify) --
    print("  Test: Synthetic GRIB2 roundtrip")
    try:
        nx, ny = 50, 50
        np.random.seed(12345)
        original_values = np.random.randn(nx * ny).astype(np.float64) * 15.0 + 280.0

        writer = rustmet.Grib2Writer()
        writer.add_field(
            values=original_values,
            discipline=0,
            parameter_category=0,
            parameter_number=0,
            level_type=103,
            level_value=2.0,
            grid_template=0,
            nx=nx, ny=ny,
            lat1=45.0, lon1=-100.0,
            lat2=40.0, lon2=-95.0,
            dx=0.1, dy=0.1,
            bits_per_value=24,
            reference_time="2025-06-15 12:00:00",
        )
        grib_bytes = bytes(writer.to_bytes())

        # Read back
        gf = rustmet.GribFile.from_bytes(grib_bytes)
        assert len(gf.messages) == 1, f"Expected 1 message, got {len(gf.messages)}"
        readback = np.array(gf.messages[0].values())

        # With 24-bit packing, tolerance depends on value range and bit depth
        max_diff = np.max(np.abs(original_values - readback))
        # For 24-bit simple packing over a ~90-unit range, precision is ~5e-6
        passed = np.allclose(original_values, readback, atol=0.01)
        detail = f"max diff = {max_diff:.2e}, 24-bit packing"
        results.append(ValidationResult("GRIB2 roundtrip (rustmet write/read)", passed, detail))
        print(f"    {'PASS' if passed else 'FAIL'}: {detail}")
    except Exception as e:
        results.append(ValidationResult("GRIB2 roundtrip (rustmet write/read)", False, str(e)))
        print(f"    FAIL: {e}")

    # -- Test 2: Multi-field roundtrip --
    print("  Test: Multi-field GRIB2 roundtrip (10 fields)")
    try:
        nx, ny = 100, 100
        np.random.seed(42)
        fields_original = []

        writer = rustmet.Grib2Writer()
        for i in range(10):
            vals = np.random.randn(nx * ny).astype(np.float64) * 10.0 + 280.0
            fields_original.append(vals.copy())
            writer.add_field(
                values=vals,
                discipline=0,
                parameter_category=0,
                parameter_number=i,
                level_type=103,
                level_value=2.0,
                grid_template=0,
                nx=nx, ny=ny,
                lat1=45.0, lon1=-100.0,
                lat2=35.0, lon2=-90.0,
                dx=0.1, dy=0.1,
                bits_per_value=16,
                reference_time="2025-06-15 12:00:00",
            )
        grib_bytes = bytes(writer.to_bytes())

        gf = rustmet.GribFile.from_bytes(grib_bytes)
        assert len(gf.messages) == 10, f"Expected 10 messages, got {len(gf.messages)}"

        all_close = True
        worst_diff = 0.0
        for i, msg in enumerate(gf.messages):
            readback = np.array(msg.values())
            diff = np.max(np.abs(fields_original[i] - readback))
            worst_diff = max(worst_diff, diff)
            # 16-bit packing over ~60-unit range: precision ~1e-3
            if not np.allclose(fields_original[i], readback, atol=0.01):
                all_close = False

        detail = f"10 fields, worst max diff = {worst_diff:.2e}, 16-bit packing"
        results.append(ValidationResult("GRIB2 multi-field roundtrip", all_close, detail))
        print(f"    {'PASS' if all_close else 'FAIL'}: {detail}")
    except Exception as e:
        results.append(ValidationResult("GRIB2 multi-field roundtrip", False, str(e)))
        print(f"    FAIL: {e}")

    # -- Test 3: Cross-library comparison with ecCodes --
    if HAS_ECCODES:
        print("  Test: rustmet vs ecCodes decoded values")
        try:
            nx, ny = 50, 50
            np.random.seed(99)
            original = np.random.randn(nx * ny).astype(np.float64) * 20.0 + 300.0

            writer = rustmet.Grib2Writer()
            writer.add_field(
                values=original,
                discipline=0,
                parameter_category=0,
                parameter_number=0,
                level_type=103,
                level_value=2.0,
                grid_template=0,
                nx=nx, ny=ny,
                lat1=45.0, lon1=-100.0,
                lat2=40.0, lon2=-95.0,
                dx=0.1, dy=0.1,
                bits_per_value=16,
                reference_time="2025-06-15 12:00:00",
            )
            grib_bytes = bytes(writer.to_bytes())

            # Decode with rustmet
            gf = rustmet.GribFile.from_bytes(grib_bytes)
            rustmet_vals = np.array(gf.messages[0].values())

            # Decode with ecCodes
            tmp = tempfile.NamedTemporaryFile(suffix=".grib2", delete=False)
            tmp.write(grib_bytes)
            tmp.close()
            try:
                with open(tmp.name, "rb") as f:
                    msgid = eccodes.codes_grib_new_from_file(f)
                    eccodes_vals = eccodes.codes_get_values(msgid)
                    eccodes.codes_release(msgid)
            finally:
                os.unlink(tmp.name)

            max_diff = np.max(np.abs(rustmet_vals - eccodes_vals))
            passed = np.allclose(rustmet_vals, eccodes_vals, atol=1e-10)
            detail = f"max diff = {max_diff:.2e}"
            results.append(ValidationResult("GRIB2 rustmet vs ecCodes values", passed, detail))
            print(f"    {'PASS' if passed else 'FAIL'}: {detail}")
        except Exception as e:
            results.append(ValidationResult("GRIB2 rustmet vs ecCodes values", False, str(e)))
            print(f"    FAIL: {e}")
    else:
        print("  [SKIP] ecCodes not installed -- skipping cross-library GRIB comparison")

    print()
    return results


# =========================================================================
# 2. GRIB Performance
# =========================================================================

def bench_grib_performance():
    """GRIB2 performance: rustmet vs ecCodes/cfgrib."""
    print("-" * 78)
    print("## GRIB2 Performance")
    print("-" * 78)
    print()

    bench_results = []

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return bench_results

    # Build a test file
    nx, ny = 100, 100
    np.random.seed(42)

    writer = rustmet.Grib2Writer()
    for param_num in range(10):
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
            lat1=39.99, lon1=-100.0,
            lat2=30.0, lon2=-90.01,
            dx=0.1, dy=0.1,
            bits_per_value=16,
            reference_time="2026-01-15 12:00:00",
        )
    grib_bytes = bytes(writer.to_bytes())

    tmp = tempfile.NamedTemporaryFile(suffix=".grib2", delete=False)
    tmp.write(grib_bytes)
    tmp.close()
    print(f"  Test file: 10 fields, {nx}x{ny} grid, {len(grib_bytes):,} bytes")
    print()

    try:
        # -- rustmet file open --
        t_rm_open = time_fn(lambda: rustmet.GribFile.open(tmp.name), number=200, repeat=5)
        bench_results.append(BenchmarkResult("GRIB2", "file open (10 msgs)", "rustmet", t_rm_open))
        print(f"  rustmet GribFile.open:         {fmt_time(t_rm_open)}")

        # -- rustmet from_bytes --
        t_rm_bytes = time_fn(lambda: rustmet.GribFile.from_bytes(grib_bytes), number=1000, repeat=5)
        bench_results.append(BenchmarkResult("GRIB2", "from_bytes (10 msgs)", "rustmet", t_rm_bytes))
        print(f"  rustmet GribFile.from_bytes:   {fmt_time(t_rm_bytes)}")

        # -- rustmet parse + unpack all --
        def rustmet_parse_unpack():
            gf = rustmet.GribFile.from_bytes(grib_bytes)
            for msg in gf.messages:
                msg.values()

        t_rm_full = time_fn(rustmet_parse_unpack, number=200, repeat=5)
        bench_results.append(BenchmarkResult("GRIB2", "parse+unpack (10 msgs)", "rustmet", t_rm_full))
        print(f"  rustmet parse + unpack all:    {fmt_time(t_rm_full)}")

        # -- ecCodes scan --
        if HAS_ECCODES:
            def eccodes_scan():
                with open(tmp.name, "rb") as f:
                    while True:
                        msgid = eccodes.codes_grib_new_from_file(f)
                        if msgid is None:
                            break
                        eccodes.codes_release(msgid)

            try:
                eccodes_scan()  # warmup
                t_ec_scan = time_fn(eccodes_scan, number=50, repeat=5)
                spd = speedup_str(t_ec_scan, t_rm_open)
                bench_results.append(BenchmarkResult("GRIB2", "file open (10 msgs)", "ecCodes", t_ec_scan, spd))
                print(f"  ecCodes scan:                  {fmt_time(t_ec_scan)}")
                print(f"    speedup (open):              {spd}")
            except Exception as e:
                print(f"  ecCodes scan: error ({e})")

            # -- ecCodes unpack --
            def eccodes_unpack():
                with open(tmp.name, "rb") as f:
                    while True:
                        msgid = eccodes.codes_grib_new_from_file(f)
                        if msgid is None:
                            break
                        eccodes.codes_get_values(msgid)
                        eccodes.codes_release(msgid)

            try:
                t_ec_unpack = time_fn(eccodes_unpack, number=50, repeat=5)
                spd = speedup_str(t_ec_unpack, t_rm_full)
                bench_results.append(BenchmarkResult("GRIB2", "parse+unpack (10 msgs)", "ecCodes", t_ec_unpack, spd))
                print(f"  ecCodes scan+unpack:           {fmt_time(t_ec_unpack)}")
                print(f"    speedup (unpack):            {spd}")
            except Exception as e:
                print(f"  ecCodes unpack: error ({e})")
        else:
            print("  ecCodes: not installed, skipping")

        # -- cfgrib open_datasets --
        if HAS_CFGRIB:
            try:
                cfgrib.open_datasets(tmp.name)  # warmup
                t_cf = time_fn(lambda: cfgrib.open_datasets(tmp.name), number=20, repeat=5)
                spd = speedup_str(t_cf, t_rm_open)
                bench_results.append(BenchmarkResult("GRIB2", "file open (10 msgs)", "cfgrib", t_cf, spd))
                print(f"  cfgrib open_datasets:          {fmt_time(t_cf)}")
                print(f"    speedup (vs cfgrib):         {spd}")
            except Exception as e:
                print(f"  cfgrib: error ({e})")
        else:
            print("  cfgrib: not installed, skipping")

    finally:
        os.unlink(tmp.name)

    print()
    return bench_results


# =========================================================================
# 3. Thermo Parity Tests (vs MetPy)
# =========================================================================

def test_thermo_parity():
    """Thermo correctness: compare rustmet scalar+array functions vs MetPy."""
    print("-" * 78)
    print("## Thermo Parity Tests (rustmet vs MetPy)")
    print("-" * 78)
    print()

    results = []

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return results

    if not HAS_METPY:
        print("  [SKIP] MetPy not installed -- cannot validate thermo parity")
        print()
        return results

    # Generate realistic weather data
    N = 10_000
    np.random.seed(42)

    temperature_c = np.random.uniform(-40.0, 45.0, N)
    dewpoint_c = temperature_c - np.random.uniform(0.5, 30.0, N)
    dewpoint_c = np.clip(dewpoint_c, -80.0, None)
    pressure_hpa = np.random.uniform(100.0, 1050.0, N)
    rh_pct = np.random.uniform(5.0, 100.0, N)

    def report_diff(name, rm_vals, mp_vals, tol_mean, tol_p99):
        """Compare arrays and return a ValidationResult."""
        diff = np.abs(rm_vals - mp_vals)
        finite = np.isfinite(diff)
        if np.sum(finite) == 0:
            return ValidationResult(name, False, "no finite values to compare")
        diff = diff[finite]
        mean_d = np.mean(diff)
        max_d = np.max(diff)
        p99_d = np.percentile(diff, 99)
        passed = mean_d < tol_mean and p99_d < tol_p99
        detail = f"mean={mean_d:.4e}, p99={p99_d:.4e}, max={max_d:.4e} (tol: mean<{tol_mean}, p99<{tol_p99})"
        return ValidationResult(name, passed, detail)

    # -- Saturation Vapor Pressure --
    print("  Test: saturation_vapor_pressure")
    try:
        rm_es = np.array(rustmet.vappres_arr(temperature_c))
        mp_es = np.array(mpcalc.saturation_vapor_pressure(temperature_c * metpy_units.degC).to("hPa").magnitude)
        r = report_diff("saturation_vapor_pressure", rm_es, mp_es, tol_mean=0.01, tol_p99=0.05)
        results.append(r)
        print(f"    {r}")
    except Exception as e:
        results.append(ValidationResult("saturation_vapor_pressure", False, str(e)))
        print(f"    FAIL: {e}")

    # -- Saturation Mixing Ratio --
    print("  Test: saturation_mixing_ratio")
    try:
        rm_ws = np.array(rustmet.mixratio_arr(pressure_hpa, temperature_c))
        # MetPy returns dimensionless (kg/kg), rustmet returns g/kg
        mp_ws_raw = mpcalc.saturation_mixing_ratio(pressure_hpa * metpy_units.hPa, temperature_c * metpy_units.degC)
        mp_ws = np.array(mp_ws_raw.to("g/kg").magnitude)
        r = report_diff("saturation_mixing_ratio", rm_ws, mp_ws, tol_mean=0.01, tol_p99=0.1)
        results.append(r)
        print(f"    {r}")
    except Exception as e:
        results.append(ValidationResult("saturation_mixing_ratio", False, str(e)))
        print(f"    FAIL: {e}")

    # -- Potential Temperature --
    print("  Test: potential_temperature")
    try:
        rm_theta = np.array(rustmet.potential_temperature_arr(pressure_hpa, temperature_c))
        mp_theta = np.array(
            mpcalc.potential_temperature(
                pressure_hpa * metpy_units.hPa,
                temperature_c * metpy_units.degC
            ).to("K").magnitude
        )
        r = report_diff("potential_temperature", rm_theta, mp_theta, tol_mean=0.01, tol_p99=0.05)
        results.append(r)
        print(f"    {r}")
    except Exception as e:
        results.append(ValidationResult("potential_temperature", False, str(e)))
        print(f"    FAIL: {e}")

    # -- Equivalent Potential Temperature --
    print("  Test: equivalent_potential_temperature")
    try:
        rm_thetae = np.array(rustmet.thetae_arr(pressure_hpa, temperature_c, dewpoint_c))
        mp_thetae = np.array(
            mpcalc.equivalent_potential_temperature(
                pressure_hpa * metpy_units.hPa,
                temperature_c * metpy_units.degC,
                dewpoint_c * metpy_units.degC
            ).to("K").magnitude
        )
        # Thetae has larger inherent differences due to formula variants
        r = report_diff("equivalent_potential_temperature", rm_thetae, mp_thetae,
                         tol_mean=0.5, tol_p99=2.0)
        results.append(r)
        print(f"    {r}")
    except Exception as e:
        results.append(ValidationResult("equivalent_potential_temperature", False, str(e)))
        print(f"    FAIL: {e}")

    # -- Dewpoint from RH --
    print("  Test: dewpoint_from_rh")
    try:
        rm_td = np.array(rustmet.dewpoint_from_rh_arr(temperature_c, rh_pct))
        mp_td = np.array(
            mpcalc.dewpoint_from_relative_humidity(
                temperature_c * metpy_units.degC,
                rh_pct * metpy_units.percent
            ).to("degC").magnitude
        )
        r = report_diff("dewpoint_from_rh", rm_td, mp_td, tol_mean=0.05, tol_p99=0.5)
        results.append(r)
        print(f"    {r}")
    except Exception as e:
        results.append(ValidationResult("dewpoint_from_rh", False, str(e)))
        print(f"    FAIL: {e}")

    print()
    return results


# =========================================================================
# 4. Thermo Performance
# =========================================================================

def bench_thermo_performance():
    """Thermo performance: rustmet array functions vs MetPy."""
    print("-" * 78)
    print("## Thermo Performance (10k arrays)")
    print("-" * 78)
    print()

    bench_results = []

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return bench_results

    N = 10_000
    np.random.seed(42)

    pressure_hpa = np.random.uniform(300.0, 1013.0, N)
    temperature_c = np.random.uniform(-40.0, 40.0, N)
    dewpoint_c = temperature_c - np.random.uniform(0.5, 20.0, N)
    rh_pct = np.random.uniform(10.0, 100.0, N)

    functions = [
        (
            "saturation_vapor_pressure",
            lambda: rustmet.vappres_arr(temperature_c),
            lambda: mpcalc.saturation_vapor_pressure(temperature_c * metpy_units.degC) if HAS_METPY else None,
        ),
        (
            "potential_temperature",
            lambda: rustmet.potential_temperature_arr(pressure_hpa, temperature_c),
            lambda: mpcalc.potential_temperature(pressure_hpa * metpy_units.hPa, temperature_c * metpy_units.degC) if HAS_METPY else None,
        ),
        (
            "saturation_mixing_ratio",
            lambda: rustmet.mixratio_arr(pressure_hpa, temperature_c),
            lambda: mpcalc.saturation_mixing_ratio(pressure_hpa * metpy_units.hPa, temperature_c * metpy_units.degC) if HAS_METPY else None,
        ),
        (
            "equiv_potential_temp",
            lambda: rustmet.thetae_arr(pressure_hpa, temperature_c, dewpoint_c),
            lambda: mpcalc.equivalent_potential_temperature(pressure_hpa * metpy_units.hPa, temperature_c * metpy_units.degC, dewpoint_c * metpy_units.degC) if HAS_METPY else None,
        ),
        (
            "dewpoint_from_rh",
            lambda: rustmet.dewpoint_from_rh_arr(temperature_c, rh_pct),
            lambda: mpcalc.dewpoint_from_relative_humidity(temperature_c * metpy_units.degC, rh_pct * metpy_units.percent) if HAS_METPY else None,
        ),
    ]

    for name, rm_fn, mp_fn in functions:
        print(f"  {name} (N={N:,})")
        t_rm = time_fn(rm_fn, number=100, repeat=5)
        bench_results.append(BenchmarkResult("Thermo", name, "rustmet", t_rm))
        print(f"    rustmet:  {fmt_time(t_rm)}")

        if HAS_METPY:
            try:
                t_mp = time_fn(mp_fn, number=100, repeat=5)
                spd = speedup_str(t_mp, t_rm)
                bench_results.append(BenchmarkResult("Thermo", name, "MetPy", t_mp, spd))
                print(f"    MetPy:    {fmt_time(t_mp)}")
                print(f"    speedup:  {spd}")
            except Exception as e:
                print(f"    MetPy:    error ({e})")
        else:
            print(f"    MetPy:    not installed, skipping")
        print()

    return bench_results


# =========================================================================
# 5. Grid Math Performance
# =========================================================================

def bench_grid_performance():
    """Grid math performance: rustmet vs numpy/scipy."""
    print("-" * 78)
    print("## Grid Math Performance")
    print("-" * 78)
    print()

    bench_results = []

    if not HAS_RUSTMET:
        print("  [SKIP] rustmet not installed")
        print()
        return bench_results

    nx, ny = 200, 200
    dx = dy = 3000.0
    np.random.seed(42)

    u = np.random.randn(ny * nx).astype(np.float64) * 10.0
    v = np.random.randn(ny * nx).astype(np.float64) * 10.0
    scalar = np.random.randn(ny * nx).astype(np.float64) * 5.0 + 280.0
    u_2d = u.reshape(ny, nx)
    v_2d = v.reshape(ny, nx)
    scalar_2d = scalar.reshape(ny, nx)

    # -- Vorticity --
    print(f"  Vorticity ({nx}x{ny} grid, {nx*ny:,} points)")
    t_rm = time_fn(lambda: rustmet.vorticity(u, v, nx, ny, dx, dy), number=200, repeat=5)
    bench_results.append(BenchmarkResult("Grid", f"vorticity ({nx}x{ny})", "rustmet", t_rm))
    print(f"    rustmet:  {fmt_time(t_rm)}")

    def numpy_vorticity():
        dvdx = np.gradient(v_2d, dx, axis=1)
        dudy = np.gradient(u_2d, dy, axis=0)
        return dvdx - dudy

    t_np = time_fn(numpy_vorticity, number=200, repeat=5)
    spd = speedup_str(t_np, t_rm)
    bench_results.append(BenchmarkResult("Grid", f"vorticity ({nx}x{ny})", "numpy", t_np, spd))
    print(f"    numpy:    {fmt_time(t_np)}")
    print(f"    speedup:  {spd}")
    print()

    # -- Divergence --
    print(f"  Divergence ({nx}x{ny} grid)")
    t_rm = time_fn(lambda: rustmet.divergence(u, v, nx, ny, dx, dy), number=200, repeat=5)
    bench_results.append(BenchmarkResult("Grid", f"divergence ({nx}x{ny})", "rustmet", t_rm))
    print(f"    rustmet:  {fmt_time(t_rm)}")

    def numpy_divergence():
        dudx = np.gradient(u_2d, dx, axis=1)
        dvdy = np.gradient(v_2d, dy, axis=0)
        return dudx + dvdy

    t_np = time_fn(numpy_divergence, number=200, repeat=5)
    spd = speedup_str(t_np, t_rm)
    bench_results.append(BenchmarkResult("Grid", f"divergence ({nx}x{ny})", "numpy", t_np, spd))
    print(f"    numpy:    {fmt_time(t_np)}")
    print(f"    speedup:  {spd}")
    print()

    # -- Gaussian Smooth --
    print(f"  Gaussian Smooth, sigma=2 ({nx}x{ny} grid)")
    t_rm = time_fn(lambda: rustmet.smooth(scalar, nx, ny, 2.0), number=50, repeat=5)
    bench_results.append(BenchmarkResult("Grid", f"gaussian smooth ({nx}x{ny})", "rustmet", t_rm))
    print(f"    rustmet:  {fmt_time(t_rm)}")

    if HAS_SCIPY:
        t_sp = time_fn(lambda: scipy.ndimage.gaussian_filter(scalar_2d, sigma=2.0), number=50, repeat=5)
        spd = speedup_str(t_sp, t_rm)
        bench_results.append(BenchmarkResult("Grid", f"gaussian smooth ({nx}x{ny})", "scipy", t_sp, spd))
        print(f"    scipy:    {fmt_time(t_sp)}")
        print(f"    speedup:  {spd}")
    else:
        print(f"    scipy:    not installed, skipping")
    print()

    return bench_results


# =========================================================================
# Summary
# =========================================================================

def print_summary(validation_results, bench_results):
    print("=" * 78)
    print("## Summary")
    print("=" * 78)
    print()

    # -- Correctness summary --
    if validation_results:
        passed = sum(1 for r in validation_results if r.passed)
        total = len(validation_results)
        print(f"Correctness: {passed}/{total} tests passed")
        print()
        for r in validation_results:
            status = "PASS" if r.passed else "FAIL"
            print(f"  [{status}] {r.name}")
            if not r.passed:
                print(f"         {r.detail}")
        print()
    else:
        print("Correctness: no tests were run")
        print()

    # -- Performance summary table --
    if bench_results:
        print("Performance:")
        print()
        print(f"  {'Operation':<40} {'rustmet':<14} {'Competitor':<22} {'Speedup':<16}")
        print(f"  {'-'*40} {'-'*14} {'-'*22} {'-'*16}")

        # Group by (category, operation)
        ops = {}
        for br in bench_results:
            key = (br.category, br.operation)
            if key not in ops:
                ops[key] = {}
            ops[key][br.library] = br

        for (cat, op), libs in ops.items():
            rm = libs.get("rustmet")
            if rm is None:
                continue
            rm_str = fmt_time(rm.time_s)
            for lib_name, br in libs.items():
                if lib_name == "rustmet":
                    continue
                comp_str = f"{lib_name}: {fmt_time(br.time_s)}"
                spd_str = br.speedup if br.speedup else "N/A"
                print(f"  {op:<40} {rm_str:<14} {comp_str:<22} {spd_str:<16}")
        print()


def main():
    print_header()

    all_validation = []
    all_bench = []

    # GRIB correctness
    all_validation.extend(test_grib_correctness())

    # GRIB performance
    all_bench.extend(bench_grib_performance())

    # Thermo parity
    all_validation.extend(test_thermo_parity())

    # Thermo performance
    all_bench.extend(bench_thermo_performance())

    # Grid math performance
    all_bench.extend(bench_grid_performance())

    # Summary
    print_summary(all_validation, all_bench)

    # Exit code
    if not all_validation:
        print("No validation tests were run. Exiting with code 0.")
        return 0

    all_passed = all(r.passed for r in all_validation)
    if all_passed:
        print(f"All {len(all_validation)} correctness checks passed.")
        return 0
    else:
        failed = [r for r in all_validation if not r.passed]
        print(f"{len(failed)} correctness check(s) FAILED.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
