"""
Python GRIB2 benchmark — times download + decode using common Python tools.

Benchmarks:
  1. Herbie + cfgrib (xarray) — the standard Python workflow
  2. cfgrib direct (eccodes backend) — lower-level decode
  3. requests + cfgrib — manual download + decode (closest to rustmet's approach)

Outputs JSON timing results to stdout.
"""

import json
import os
import sys
import time
import tempfile
import hashlib
from pathlib import Path
from datetime import datetime, timedelta, timezone

# Suppress herbie's verbose output
os.environ["HERBIE_VERBOSE"] = "false"

# ── Benchmark variables (same as Rust) ──────────────────────────
BENCH_VARS_HERBIE = [
    "TMP:2 m above ground",
    "DPT:2 m above ground",
    "UGRD:10 m above ground",
    "VGRD:10 m above ground",
    "CAPE:surface",
    "REFC:entire atmosphere",
    "MSLMA:mean sea level",
    "HGT:500 mb",
]

# For cfgrib/xarray, we need shortName or filter_by_keys
CFGRIB_FILTERS = [
    {"shortName": "2t", "typeOfLevel": "heightAboveGround", "level": 2},
    {"shortName": "2d", "typeOfLevel": "heightAboveGround", "level": 2},
    {"shortName": "10u", "typeOfLevel": "heightAboveGround", "level": 10},
    {"shortName": "10v", "typeOfLevel": "heightAboveGround", "level": 10},
    {"shortName": "cape", "typeOfLevel": "surface"},
    {"shortName": "refc", "typeOfLevel": "atmosphere"},
    {"shortName": "unknown", "typeOfLevel": "meanSea"},   # MSLMA
    {"shortName": "gh", "typeOfLevel": "isobaricInhPa", "level": 500},
]


def get_run_time(run_str=None):
    """Parse run time string or default to yesterday 00z."""
    if run_str:
        parts = run_str.split("/")
        date_str = parts[0].replace("-", "")
        hour = int(parts[1].rstrip("zZ")) if len(parts) > 1 else 0
        dt = datetime.strptime(date_str, "%Y%m%d").replace(
            hour=hour, tzinfo=timezone.utc
        )
    else:
        dt = datetime.now(timezone.utc) - timedelta(hours=24)
        dt = dt.replace(hour=0, minute=0, second=0, microsecond=0)
    return dt


def median(values):
    s = sorted(values)
    n = len(s)
    if n % 2 == 0:
        return (s[n // 2 - 1] + s[n // 2]) / 2
    return s[n // 2]


# ── Benchmark 1: Herbie (download + xarray decode) ─────────────
def bench_herbie(dt, iterations=5, download_iters=3):
    """Benchmark Herbie's full workflow: download via idx + decode via cfgrib."""
    import warnings
    warnings.filterwarnings("ignore")

    from herbie import Herbie

    # Herbie compares against tz-naive internally, so strip tzinfo
    if dt.tzinfo is not None:
        dt = dt.replace(tzinfo=None)

    results = {
        "tool": "herbie+cfgrib",
        "download_times_ms": [],
        "decode_times_ms": [],
        "num_vars_decoded": 0,
        "total_values": 0,
    }

    # ── Download timing ──
    for i in range(download_iters):
        # Clear herbie cache for this file
        cache_dir = Path.home() / ".cache" / "herbie"
        if cache_dir.exists():
            for f in cache_dir.rglob("*.grib2*"):
                if dt.strftime("%Y%m%d") in str(f) and "wrfprs" in str(f):
                    try:
                        f.unlink()
                    except Exception:
                        pass

        t0 = time.perf_counter()

        H = Herbie(dt, model="hrrr", product="prs", fxx=0, verbose=False, save_dir=str(cache_dir))

        # Download each variable pattern (Herbie downloads one at a time)
        for var in BENCH_VARS_HERBIE:
            try:
                H.download(var, verbose=False)
            except Exception as e:
                print(f"  Warning: failed to download {var}: {e}", file=sys.stderr)

        elapsed_ms = (time.perf_counter() - t0) * 1000
        results["download_times_ms"].append(elapsed_ms)
        print(f"  Herbie download iter {i+1}: {elapsed_ms:.0f}ms", file=sys.stderr)

    # ── Decode timing ──
    for i in range(iterations):
        t0 = time.perf_counter()
        total_vals = 0
        num_decoded = 0

        H = Herbie(dt, model="hrrr", product="prs", fxx=0, verbose=False,
                   save_dir=str(cache_dir))

        for var in BENCH_VARS_HERBIE:
            try:
                ds = H.xarray(var, verbose=False)
                for data_var in ds.data_vars:
                    arr = ds[data_var].values
                    total_vals += arr.size
                    num_decoded += 1
            except Exception:
                pass

        elapsed_ms = (time.perf_counter() - t0) * 1000
        results["decode_times_ms"].append(elapsed_ms)
        results["num_vars_decoded"] = num_decoded
        results["total_values"] = total_vals
        print(f"  Herbie decode iter {i+1}: {elapsed_ms:.0f}ms ({num_decoded} vars, {total_vals} values)", file=sys.stderr)

    results["download_median_ms"] = median(results["download_times_ms"]) if results["download_times_ms"] else 0
    results["decode_median_ms"] = median(results["decode_times_ms"]) if results["decode_times_ms"] else 0
    return results


# ── Benchmark 2: requests + cfgrib direct ──────────────────────
def bench_requests_cfgrib(dt, iterations=5, download_iters=3):
    """Manual download via requests + idx parsing, decode via cfgrib.
    This is the closest apples-to-apples comparison to rustmet."""
    import requests
    import cfgrib

    date_str = dt.strftime("%Y%m%d")
    hour = dt.hour

    idx_url = f"https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{date_str}/conus/hrrr.t{hour:02d}z.wrfprsf00.grib2.idx"
    grib_url = f"https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{date_str}/conus/hrrr.t{hour:02d}z.wrfprsf00.grib2"

    results = {
        "tool": "requests+cfgrib",
        "download_times_ms": [],
        "decode_times_ms": [],
        "download_bytes": 0,
        "num_messages": 0,
        "total_values": 0,
    }

    tmp_path = os.path.join(tempfile.gettempdir(), f"rustmet_bench_{date_str}_{hour:02d}z.grib2")

    # ── Download timing (manual idx + byte-range, like rustmet) ──
    for i in range(download_iters):
        # Remove cached file
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)

        t0 = time.perf_counter()

        # Fetch and parse idx
        idx_resp = requests.get(idx_url)
        idx_resp.raise_for_status()
        idx_lines = idx_resp.text.strip().split("\n")

        # Parse idx entries
        entries = []
        for line in idx_lines:
            parts = line.split(":")
            if len(parts) >= 6:
                entries.append({
                    "msg": int(parts[0]),
                    "offset": int(parts[1]),
                    "date": parts[2],
                    "var": parts[3],
                    "level": parts[4],
                    "forecast": parts[5],
                })

        # Find matching entries
        selected_offsets = set()
        selected = []
        for var_pat in BENCH_VARS_HERBIE:
            var_parts = var_pat.split(":", 1)
            var_name = var_parts[0]
            var_level = var_parts[1] if len(var_parts) > 1 else ""

            for e in entries:
                if e["var"] == var_name and var_level in e["level"]:
                    if e["offset"] not in selected_offsets:
                        selected_offsets.add(e["offset"])
                        selected.append(e)

        # Compute byte ranges
        all_offsets = sorted(set(e["offset"] for e in entries))
        ranges = []
        for sel in selected:
            start = sel["offset"]
            idx = all_offsets.index(start)
            end = all_offsets[idx + 1] - 1 if idx + 1 < len(all_offsets) else ""
            ranges.append((start, end))

        # Download byte ranges
        all_data = bytearray()
        for start, end in ranges:
            range_header = f"bytes={start}-{end}" if end != "" else f"bytes={start}-"
            resp = requests.get(grib_url, headers={"Range": range_header})
            resp.raise_for_status()
            all_data.extend(resp.content)

        # Write to temp file (cfgrib needs a file)
        with open(tmp_path, "wb") as f:
            f.write(all_data)

        elapsed_ms = (time.perf_counter() - t0) * 1000
        results["download_times_ms"].append(elapsed_ms)
        results["download_bytes"] = len(all_data)
        print(f"  requests+cfgrib download iter {i+1}: {elapsed_ms:.0f}ms ({len(all_data)/1048576:.2f} MB)", file=sys.stderr)

    # ── Decode timing ──
    for i in range(iterations):
        t0 = time.perf_counter()

        datasets = cfgrib.open_datasets(tmp_path)
        total_vals = 0
        num_msgs = 0
        for ds in datasets:
            for var_name in ds.data_vars:
                arr = ds[var_name].values
                total_vals += arr.size
                num_msgs += 1

        elapsed_ms = (time.perf_counter() - t0) * 1000
        results["decode_times_ms"].append(elapsed_ms)
        results["num_messages"] = num_msgs
        results["total_values"] = total_vals
        print(f"  cfgrib decode iter {i+1}: {elapsed_ms:.0f}ms ({num_msgs} vars, {total_vals} values)", file=sys.stderr)

    results["download_median_ms"] = median(results["download_times_ms"]) if results["download_times_ms"] else 0
    results["decode_median_ms"] = median(results["decode_times_ms"]) if results["decode_times_ms"] else 0

    # Cleanup
    if os.path.exists(tmp_path):
        os.unlink(tmp_path)

    return results


# ── Benchmark 3: xarray + cfgrib (from file) ──────────────────
def bench_xarray_cfgrib(dt, iterations=5, download_iters=3):
    """xarray.open_dataset with cfgrib engine — common user workflow."""
    import requests

    date_str = dt.strftime("%Y%m%d")
    hour = dt.hour
    idx_url = f"https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{date_str}/conus/hrrr.t{hour:02d}z.wrfprsf00.grib2.idx"
    grib_url = f"https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.{date_str}/conus/hrrr.t{hour:02d}z.wrfprsf00.grib2"

    results = {
        "tool": "xarray+cfgrib",
        "download_times_ms": [],
        "decode_times_ms": [],
        "download_bytes": 0,
        "num_vars_decoded": 0,
        "total_values": 0,
    }

    tmp_path = os.path.join(tempfile.gettempdir(), f"rustmet_bench_xa_{date_str}_{hour:02d}z.grib2")

    # Download once (same manual idx approach)
    if not os.path.exists(tmp_path):
        idx_resp = requests.get(idx_url)
        idx_resp.raise_for_status()
        idx_lines = idx_resp.text.strip().split("\n")
        entries = []
        for line in idx_lines:
            parts = line.split(":")
            if len(parts) >= 6:
                entries.append({
                    "msg": int(parts[0]),
                    "offset": int(parts[1]),
                    "var": parts[3],
                    "level": parts[4],
                })

        selected_offsets = set()
        selected = []
        for var_pat in BENCH_VARS_HERBIE:
            var_parts = var_pat.split(":", 1)
            var_name = var_parts[0]
            var_level = var_parts[1] if len(var_parts) > 1 else ""
            for e in entries:
                if e["var"] == var_name and var_level in e["level"]:
                    if e["offset"] not in selected_offsets:
                        selected_offsets.add(e["offset"])
                        selected.append(e)

        all_offsets = sorted(set(e["offset"] for e in entries))
        all_data = bytearray()
        for sel in selected:
            start = sel["offset"]
            idx = all_offsets.index(start)
            end = all_offsets[idx + 1] - 1 if idx + 1 < len(all_offsets) else ""
            range_header = f"bytes={start}-{end}" if end != "" else f"bytes={start}-"
            resp = requests.get(grib_url, headers={"Range": range_header})
            resp.raise_for_status()
            all_data.extend(resp.content)
        with open(tmp_path, "wb") as f:
            f.write(all_data)
        results["download_bytes"] = len(all_data)

    # ── Decode timing with xarray ──
    for i in range(iterations):
        t0 = time.perf_counter()

        total_vals = 0
        num_decoded = 0

        # xarray + cfgrib: open multiple datasets from heterogeneous GRIB
        import cfgrib
        datasets = cfgrib.open_datasets(tmp_path)
        for ds in datasets:
            for var_name in ds.data_vars:
                arr = ds[var_name].values  # forces lazy load
                total_vals += arr.size
                num_decoded += 1

        elapsed_ms = (time.perf_counter() - t0) * 1000
        results["decode_times_ms"].append(elapsed_ms)
        results["num_vars_decoded"] = num_decoded
        results["total_values"] = total_vals
        print(f"  xarray+cfgrib decode iter {i+1}: {elapsed_ms:.0f}ms ({num_decoded} vars, {total_vals} values)", file=sys.stderr)

    results["decode_median_ms"] = median(results["decode_times_ms"]) if results["decode_times_ms"] else 0
    results["download_median_ms"] = 0  # not benchmarked separately for xarray

    # Cleanup
    if os.path.exists(tmp_path):
        os.unlink(tmp_path)

    return results


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Python GRIB2 benchmark")
    parser.add_argument("--run", type=str, default=None, help="Run time: YYYY-MM-DD/HHz")
    parser.add_argument("--iterations", "-n", type=int, default=5, help="Decode iterations")
    parser.add_argument("--download-iters", type=int, default=3, help="Download iterations")
    parser.add_argument("--tool", type=str, default="all",
                        choices=["all", "herbie", "requests", "xarray"],
                        help="Which tool to benchmark")
    args = parser.parse_args()

    dt = get_run_time(args.run)
    print(f"Python GRIB2 Benchmark", file=sys.stderr)
    print(f"  Run:        {dt.strftime('%Y%m%d')}/{dt.hour:02d}z", file=sys.stderr)
    print(f"  Variables:  {len(BENCH_VARS_HERBIE)} patterns", file=sys.stderr)
    print(f"  Iterations: {args.iterations} (decode), {args.download_iters} (download)", file=sys.stderr)
    print(file=sys.stderr)

    all_results = []

    if args.tool in ("all", "requests"):
        print("── requests + cfgrib ──", file=sys.stderr)
        r = bench_requests_cfgrib(dt, iterations=args.iterations, download_iters=args.download_iters)
        all_results.append(r)
        print(file=sys.stderr)

    if args.tool in ("all", "herbie"):
        print("── Herbie + cfgrib ──", file=sys.stderr)
        try:
            r = bench_herbie(dt, iterations=args.iterations, download_iters=args.download_iters)
            all_results.append(r)
        except Exception as e:
            print(f"  Herbie benchmark failed: {e}", file=sys.stderr)
        print(file=sys.stderr)

    if args.tool in ("all", "xarray"):
        print("── xarray + cfgrib ──", file=sys.stderr)
        try:
            r = bench_xarray_cfgrib(dt, iterations=args.iterations, download_iters=args.download_iters)
            all_results.append(r)
        except Exception as e:
            print(f"  xarray benchmark failed: {e}", file=sys.stderr)
        print(file=sys.stderr)

    # Output JSON
    print(json.dumps(all_results, indent=2))


if __name__ == "__main__":
    main()
