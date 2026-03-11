"""
Batch processing of multiple GRIB2 files.

This example demonstrates:
  - Scanning a directory for GRIB2 files
  - Extracting specific fields from each file
  - Parallel processing using concurrent.futures
  - Filtering messages with search_messages
  - Aggregating results (e.g., time series of max CAPE, mean temperature)

Usage:
    python examples/batch_processing.py /path/to/grib2/directory/
    python examples/batch_processing.py /path/to/grib2/directory/ --workers 8

    # Or download a time series and process it:
    python examples/batch_processing.py --download
"""

import sys
import os
import time
import glob
import numpy as np
from concurrent.futures import ProcessPoolExecutor, as_completed
import rustmet


def find_grib_files(directory):
    """Find all GRIB2 files in a directory."""
    patterns = ["*.grib2", "*.grb2", "*.grb", "*.grib"]
    files = []
    for pat in patterns:
        files.extend(glob.glob(os.path.join(directory, pat)))
        files.extend(glob.glob(os.path.join(directory, "**", pat), recursive=True))

    # Deduplicate and sort
    files = sorted(set(files))
    return files


def process_single_file(path, fields_to_extract):
    """
    Process a single GRIB2 file: extract requested fields, compute stats.

    This function is designed to be called from a process pool.
    Each worker parses its file independently.

    Args:
        path: Path to the GRIB2 file
        fields_to_extract: List of search queries, e.g. ["temperature 2m", "cape"]

    Returns:
        Dict with file path, reference time, and stats for each field.
    """
    result = {"path": path, "fields": {}}

    try:
        grib = rustmet.open(path)
        result["num_messages"] = grib.num_messages

        # Get reference time from first message
        if grib.messages:
            result["reference_time"] = grib.messages[0].reference_time
            result["forecast_time"] = grib.messages[0].forecast_time

        for query in fields_to_extract:
            matches = grib.search(query)
            if not matches:
                result["fields"][query] = {"found": False}
                continue

            msg = matches[0]
            values = msg.values()

            # Use rustmet's field_stats for fast statistics
            stats = rustmet.field_stats(values)

            result["fields"][query] = {
                "found": True,
                "variable": msg.variable,
                "level": msg.level,
                "units": msg.units,
                "nx": msg.nx,
                "ny": msg.ny,
                "min": stats["min"],
                "max": stats["max"],
                "mean": stats["mean"],
                "std_dev": stats["std_dev"],
            }

    except Exception as e:
        result["error"] = str(e)

    return result


def process_directory(directory, fields, max_workers=4):
    """
    Process all GRIB2 files in a directory in parallel.

    Uses ProcessPoolExecutor to parse files across multiple cores.
    Each file is parsed independently (no shared state).
    """
    files = find_grib_files(directory)
    if not files:
        print(f"No GRIB2 files found in {directory}")
        return []

    print(f"Found {len(files)} GRIB2 files in {directory}")
    print(f"Fields to extract: {fields}")
    print(f"Workers: {max_workers}")
    print()

    results = []
    t0 = time.perf_counter()

    with ProcessPoolExecutor(max_workers=max_workers) as pool:
        futures = {
            pool.submit(process_single_file, f, fields): f
            for f in files
        }

        for future in as_completed(futures):
            filepath = futures[future]
            try:
                result = future.result()
                results.append(result)

                # Print progress
                basename = os.path.basename(filepath)
                if "error" in result:
                    print(f"  ERROR {basename}: {result['error']}")
                else:
                    n_found = sum(
                        1 for v in result["fields"].values() if v.get("found")
                    )
                    print(f"  OK    {basename}: {result.get('num_messages', 0)} msgs, "
                          f"{n_found}/{len(fields)} fields found")
            except Exception as e:
                print(f"  FAIL  {os.path.basename(filepath)}: {e}")

    elapsed = time.perf_counter() - t0
    print(f"\nProcessed {len(results)} files in {elapsed:.2f}s "
          f"({len(results) / elapsed:.1f} files/sec)")

    return results


def download_time_series():
    """
    Download a HRRR time series for demonstration.

    Downloads f000 through f005 of the latest HRRR run, returning
    a list of GribFile objects (one per forecast hour).
    """
    print("Downloading HRRR time series (f000-f005) ...")
    client = rustmet.Client()

    t0 = time.perf_counter()
    # fetch() with a list of fhours downloads them in parallel via Rayon
    files = client.fetch(
        "hrrr", "latest",
        fhour=[0, 1, 2, 3, 4, 5],
        product="sfc",
        vars=[
            "TMP:2 m above ground",
            "CAPE:surface",
            "UGRD:10 m above ground",
            "VGRD:10 m above ground",
        ],
    )
    elapsed = time.perf_counter() - t0
    print(f"  Downloaded {len(files)} forecast hours in {elapsed:.1f}s\n")
    return files


def process_downloaded_series(grib_files):
    """
    Process a list of GribFile objects (from parallel download).
    Demonstrates working with in-memory GRIB data.
    """
    fields = ["temperature 2m", "cape", "wind 10m"]

    print(f"{'fhour':>5}  {'T2m min':>8}  {'T2m max':>8}  {'CAPE max':>9}  {'Wind max':>9}")
    print("-" * 50)

    for i, grib in enumerate(grib_files):
        row = {"fhour": i}

        # 2m Temperature
        t_results = grib.search("temperature 2m")
        if t_results:
            t_vals = t_results[0].values()
            t_f = rustmet.convert_units(t_vals, "K", "F")
            stats = rustmet.field_stats(t_f)
            row["t_min"] = stats["min"]
            row["t_max"] = stats["max"]
        else:
            row["t_min"] = row["t_max"] = float("nan")

        # CAPE
        cape_results = grib.search("cape")
        if cape_results:
            cape_vals = cape_results[0].values()
            cape_stats = rustmet.field_stats(cape_vals)
            row["cape_max"] = cape_stats["max"]
        else:
            row["cape_max"] = float("nan")

        # 10m Wind
        u_results = grib.search("u wind 10m")
        v_results = grib.search("v wind 10m")
        if u_results and v_results:
            u = u_results[0].values()
            v = v_results[0].values()
            wspd, _ = rustmet.wind_speed_dir(u, v)
            wspd_kt = rustmet.convert_units(wspd, "m/s", "kt")
            wind_stats = rustmet.field_stats(wspd_kt)
            row["wind_max"] = wind_stats["max"]
        else:
            row["wind_max"] = float("nan")

        print(f"{row['fhour']:>5d}  {row['t_min']:>8.1f}  {row['t_max']:>8.1f}  "
              f"{row['cape_max']:>9.0f}  {row['wind_max']:>9.1f}")


def print_summary(results, fields):
    """Print a summary table of batch processing results."""
    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)

    for query in fields:
        print(f"\n  Field: '{query}'")
        values_min = []
        values_max = []
        values_mean = []

        for r in results:
            field_data = r.get("fields", {}).get(query, {})
            if field_data.get("found"):
                values_min.append(field_data["min"])
                values_max.append(field_data["max"])
                values_mean.append(field_data["mean"])

        if values_min:
            print(f"    Found in {len(values_min)}/{len(results)} files")
            print(f"    Overall min: {min(values_min):.2f}")
            print(f"    Overall max: {max(values_max):.2f}")
            print(f"    Average of means: {np.mean(values_mean):.2f}")
        else:
            print(f"    Not found in any file")


def main():
    # Parse arguments
    max_workers = 4
    if "--workers" in sys.argv:
        idx = sys.argv.index("--workers")
        max_workers = int(sys.argv[idx + 1])

    # Fields to search for in each file
    fields = ["temperature 2m", "cape", "wind 10m"]

    if "--download" in sys.argv:
        # Download and process a time series (no local files needed)
        grib_files = download_time_series()
        process_downloaded_series(grib_files)
        return

    if len(sys.argv) < 2 or sys.argv[1].startswith("--"):
        print("Usage:")
        print("  python batch_processing.py /path/to/grib2/dir/ [--workers N]")
        print("  python batch_processing.py --download")
        print()
        print("Demonstrating with download mode ...")
        print()
        grib_files = download_time_series()
        process_downloaded_series(grib_files)
        return

    directory = sys.argv[1]
    if not os.path.isdir(directory):
        print(f"Error: {directory} is not a directory")
        sys.exit(1)

    results = process_directory(directory, fields, max_workers)

    if results:
        print_summary(results, fields)

        # Sort by reference time if available
        results_sorted = sorted(
            results,
            key=lambda r: r.get("reference_time", ""),
        )

        # Show time series if we have temporal data
        print("\n\nTime series of 2m temperature max:")
        for r in results_sorted:
            t_data = r.get("fields", {}).get("temperature 2m", {})
            if t_data.get("found"):
                ref = r.get("reference_time", "?")
                fhr = r.get("forecast_time", "?")
                print(f"  {ref} f{fhr:03d}: max={t_data['max']:.1f} {t_data['units']}")


if __name__ == "__main__":
    main()
