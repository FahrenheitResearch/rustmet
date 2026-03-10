"""
GRIB2 Processing Benchmark: rustmet vs Python ecosystem
========================================================

Runs identical workloads through rustmet (Rust) and Python tools (Herbie, cfgrib,
xarray), then prints a comparison table.

Usage:
    python benchmark/run_benchmark.py [--run YYYY-MM-DD/HHz] [-n ITERATIONS]
"""

import json
import os
import subprocess
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path


def get_default_run():
    """Yesterday 00z — always available on AWS."""
    dt = datetime.now(timezone.utc) - timedelta(hours=24)
    return f"{dt.strftime('%Y-%m-%d')}/00z"


def run_rustmet_bench(run_time, iterations, rustmet_exe):
    """Run the Rust benchmark binary and parse JSON output."""
    cmd = [rustmet_exe, "--run", run_time, "--iterations", str(iterations)]
    print(f"Running: {' '.join(cmd)}")
    print()

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=300,
    )

    # Print stderr (progress output) to console
    if result.stderr:
        for line in result.stderr.strip().split("\n"):
            print(f"  {line}")
        print()

    if result.returncode != 0:
        print(f"ERROR: rustmet-bench failed with code {result.returncode}")
        print(result.stderr)
        return None

    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        print(f"ERROR: Failed to parse rustmet-bench JSON output:")
        print(result.stdout[:500])
        return None


def run_python_bench(run_time, iterations, download_iters):
    """Run the Python benchmark and parse JSON output."""
    script = str(Path(__file__).parent / "bench_python.py")
    cmd = [
        sys.executable, script,
        "--run", run_time,
        "-n", str(iterations),
        "--download-iters", str(download_iters),
    ]
    print(f"Running: {' '.join(cmd)}")
    print()

    env = os.environ.copy()
    env["PYTHONIOENCODING"] = "utf-8"

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=600,
        env=env,
    )

    if result.stderr:
        for line in result.stderr.strip().split("\n"):
            print(f"  {line}")
        print()

    if result.returncode != 0:
        print(f"ERROR: Python benchmark failed with code {result.returncode}")
        if result.stderr:
            print(result.stderr[-1000:])
        return None

    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        print(f"ERROR: Failed to parse Python benchmark JSON output:")
        print(result.stdout[:500])
        return None


def format_ms(ms):
    if ms >= 1000:
        return f"{ms/1000:.2f}s"
    return f"{ms:.0f}ms"


def print_results(rust_result, python_results):
    """Print a comparison table."""
    print()
    print("=" * 80)
    print("  GRIB2 PROCESSING BENCHMARK RESULTS")
    print("=" * 80)
    print()

    if rust_result:
        run = rust_result.get("run", "?")
        nvars = rust_result.get("variables", "?")
        nbytes = rust_result.get("download_bytes", 0)
        print(f"  Model run:   HRRR {run} F000")
        print(f"  Variables:   {nvars} patterns ({nbytes/1048576:.1f} MB downloaded)")
        print()

    # Build rows
    rows = []

    if rust_result:
        dl = rust_result.get("download_median_ms", 0)
        dc = rust_result.get("decode_median_ms", 0)
        total = dl + dc if dl > 0 else dc
        rows.append({
            "tool": "rustmet (Rust)",
            "download_ms": dl,
            "decode_ms": dc,
            "total_ms": total,
            "messages": rust_result.get("num_messages", 0),
            "values": rust_result.get("total_values", 0),
        })

    if python_results:
        for pr in python_results:
            dl = pr.get("download_median_ms", 0)
            dc = pr.get("decode_median_ms", 0)
            total = dl + dc if dl > 0 else dc
            msgs = pr.get("num_messages", pr.get("num_vars_decoded", 0))
            vals = pr.get("total_values", 0)
            rows.append({
                "tool": pr.get("tool", "python"),
                "download_ms": dl,
                "decode_ms": dc,
                "total_ms": total,
                "messages": msgs,
                "values": vals,
            })

    if not rows:
        print("  No results to display.")
        return

    # Separate into full-pipeline (have download) and decode-only
    full_rows = [r for r in rows if r["download_ms"] > 0]
    decode_rows = [r for r in rows if r["download_ms"] == 0]

    # ── Full pipeline comparison (download + decode) ──
    if full_rows:
        full_rows.sort(key=lambda r: r["total_ms"])
        slowest = full_rows[-1]["total_ms"] if full_rows else 1

        print("  FULL PIPELINE (download + decode)")
        print()
        print(f"  {'Tool':<25} {'Download':>10} {'Decode':>10} {'Total':>10} {'Speedup':>10}")
        print(f"  {'-'*25} {'-'*10} {'-'*10} {'-'*10} {'-'*10}")

        for row in full_rows:
            dl_str = format_ms(row["download_ms"])
            dc_str = format_ms(row["decode_ms"])
            total_str = format_ms(row["total_ms"])
            speedup = slowest / row["total_ms"] if row["total_ms"] > 0 else 0
            speedup_str = f"{speedup:.1f}x" if speedup > 1.05 else "1.0x (base)"
            print(f"  {row['tool']:<25} {dl_str:>10} {dc_str:>10} {total_str:>10} {speedup_str:>10}")

        print()

    # ── Decode-only comparison ──
    all_with_decode = sorted(rows, key=lambda r: r["decode_ms"])
    if len(all_with_decode) > 1:
        slowest_decode = all_with_decode[-1]["decode_ms"]

        print("  DECODE ONLY (parse + unpack from memory/disk)")
        print()
        print(f"  {'Tool':<25} {'Decode':>10} {'Speedup':>10}")
        print(f"  {'-'*25} {'-'*10} {'-'*10}")

        for row in all_with_decode:
            dc_str = format_ms(row["decode_ms"])
            speedup = slowest_decode / row["decode_ms"] if row["decode_ms"] > 0 else 0
            speedup_str = f"{speedup:.1f}x" if speedup > 1.05 else "1.0x (base)"
            print(f"  {row['tool']:<25} {dc_str:>10} {speedup_str:>10}")

        print()

    print()

    # Detail breakdown
    print(f"  {'Tool':<25} {'Messages':>10} {'Total Values':>14}")
    print(f"  {'-'*25} {'-'*10} {'-'*14}")
    for row in rows:
        print(f"  {row['tool']:<25} {row['messages']:>10} {row['values']:>14,}")

    print()

    # Speedup summary
    if len(rows) >= 2 and rows[0]["tool"].startswith("rustmet"):
        rust_total = rows[0]["total_ms"]
        for row in rows[1:]:
            if row["total_ms"] > 0:
                speedup = row["total_ms"] / rust_total
                print(f"  rustmet is {speedup:.1f}x faster than {row['tool']}")

                # Breakdown
                if rows[0]["download_ms"] > 0 and row["download_ms"] > 0:
                    dl_speedup = row["download_ms"] / rows[0]["download_ms"]
                    print(f"    Download: {dl_speedup:.1f}x faster")
                if rows[0]["decode_ms"] > 0 and row["decode_ms"] > 0:
                    dc_speedup = row["decode_ms"] / rows[0]["decode_ms"]
                    print(f"    Decode:   {dc_speedup:.1f}x faster")

    print()
    print("=" * 80)


def main():
    import argparse
    parser = argparse.ArgumentParser(description="GRIB2 benchmark: rustmet vs Python")
    parser.add_argument("--run", type=str, default=None,
                        help="Model run time (default: yesterday 00z)")
    parser.add_argument("-n", "--iterations", type=int, default=5,
                        help="Number of decode iterations (default: 5)")
    parser.add_argument("--download-iters", type=int, default=3,
                        help="Number of download iterations (default: 3)")
    parser.add_argument("--rust-only", action="store_true",
                        help="Only run Rust benchmark")
    parser.add_argument("--python-only", action="store_true",
                        help="Only run Python benchmark")
    args = parser.parse_args()

    run_time = args.run or get_default_run()
    print()
    print("GRIB2 Processing Benchmark")
    print(f"  Run: HRRR {run_time} F000")
    print(f"  Decode iterations: {args.iterations}")
    print(f"  Download iterations: {args.download_iters}")
    print()

    # Find rustmet-bench executable
    project_root = Path(__file__).parent.parent
    rustmet_exe = project_root / "target" / "release" / "rustmet-bench.exe"
    if not rustmet_exe.exists():
        rustmet_exe = project_root / "target" / "release" / "rustmet-bench"
    if not rustmet_exe.exists():
        # Try to build it
        print("Building rustmet-bench (release)...")
        build_result = subprocess.run(
            ["cargo", "build", "--release", "--bin", "rustmet-bench"],
            cwd=str(project_root),
            capture_output=True,
            text=True,
            timeout=300,
        )
        if build_result.returncode != 0:
            print(f"ERROR: Failed to build rustmet-bench:")
            print(build_result.stderr[-1000:])
            if not args.python_only:
                return
        # Re-check
        rustmet_exe = project_root / "target" / "release" / "rustmet-bench.exe"
        if not rustmet_exe.exists():
            rustmet_exe = project_root / "target" / "release" / "rustmet-bench"

    rust_result = None
    python_results = None

    # Run Rust benchmark
    if not args.python_only:
        print("─" * 60)
        print("RUST: rustmet")
        print("─" * 60)
        rust_result = run_rustmet_bench(run_time, args.iterations, str(rustmet_exe))

    # Run Python benchmarks
    if not args.rust_only:
        print("─" * 60)
        print("PYTHON: herbie + cfgrib + xarray")
        print("─" * 60)
        python_results = run_python_bench(run_time, args.iterations, args.download_iters)

    # Print comparison
    print_results(rust_result, python_results or [])

    # Save raw results
    output = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "run_time": run_time,
        "iterations": args.iterations,
        "rust": rust_result,
        "python": python_results,
    }
    results_path = project_root / "benchmark" / "results.json"
    with open(results_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"  Raw results saved to: {results_path}")
    print()


if __name__ == "__main__":
    main()
