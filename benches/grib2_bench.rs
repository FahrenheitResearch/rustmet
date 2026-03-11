//! Criterion benchmarks for rustmet-core.
//!
//! Covers meteorological calculations, dynamics operations, smoothing,
//! GRIB2 writer/parser round-trip, and search.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use rustmet_core::metfuncs;
use rustmet_core::dynamics;
use rustmet_core::grib2::{
    self, Grib2Writer, MessageBuilder, PackingMethod, GridDefinition, ProductDefinition,
    search_messages,
};

// ─────────────────────────────────────────────
// Deterministic synthetic data generators
// ─────────────────────────────────────────────

/// Generate a Vec<f64> of `n` values using a simple deterministic formula.
fn synthetic_1d(n: usize, base: f64, scale: f64) -> Vec<f64> {
    (0..n)
        .map(|i| base + scale * ((i as f64 * 0.37).sin() + 0.5 * ((i as f64 * 1.13).cos())))
        .collect()
}

/// Generate a 2D grid (row-major) with smooth spatial variation.
fn synthetic_grid(nx: usize, ny: usize, base: f64, scale: f64) -> Vec<f64> {
    let mut v = Vec::with_capacity(nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let x = i as f64 / nx as f64;
            let y = j as f64 / ny as f64;
            v.push(base + scale * ((x * 6.28).sin() * (y * 6.28).cos() + 0.3 * (x * 12.56).sin()));
        }
    }
    v
}

// ─────────────────────────────────────────────
// Meteorological calculations (1000-element arrays)
// ─────────────────────────────────────────────

fn bench_metfuncs(c: &mut Criterion) {
    let n = 1000;

    // Realistic met values: pressure ~850-1013 hPa, temp ~-10 to 35 C, dewpoint ~-20 to 25 C
    let pressures = synthetic_1d(n, 950.0, 50.0);
    let temperatures = synthetic_1d(n, 15.0, 20.0);
    let dewpoints = synthetic_1d(n, 5.0, 15.0);

    let mut group = c.benchmark_group("metfuncs");

    group.bench_function("thetae_1000", |b| {
        b.iter(|| {
            for i in 0..n {
                black_box(metfuncs::thetae(pressures[i], temperatures[i], dewpoints[i]));
            }
        });
    });

    group.bench_function("mixratio_1000", |b| {
        b.iter(|| {
            for i in 0..n {
                black_box(metfuncs::mixratio(pressures[i], temperatures[i]));
            }
        });
    });

    group.bench_function("potential_temperature_1000", |b| {
        b.iter(|| {
            for i in 0..n {
                black_box(metfuncs::potential_temperature(pressures[i], temperatures[i]));
            }
        });
    });

    group.bench_function("wet_bulb_temperature_1000", |b| {
        b.iter(|| {
            for i in 0..n {
                black_box(metfuncs::wet_bulb_temperature(
                    pressures[i],
                    temperatures[i],
                    dewpoints[i],
                ));
            }
        });
    });

    group.finish();
}

// ─────────────────────────────────────────────
// Dynamics operations on 100x100 grids
// ─────────────────────────────────────────────

fn bench_dynamics(c: &mut Criterion) {
    let nx = 100;
    let ny = 100;
    let dx = 3000.0; // 3 km grid spacing
    let dy = 3000.0;

    let u = synthetic_grid(nx, ny, 10.0, 15.0); // u-wind (m/s)
    let v = synthetic_grid(nx, ny, 5.0, 12.0);  // v-wind (m/s)
    let temperature = synthetic_grid(nx, ny, 280.0, 20.0); // temperature (K)

    let mut group = c.benchmark_group("dynamics");

    group.bench_function("vorticity_100x100", |b| {
        b.iter(|| {
            black_box(dynamics::vorticity(&u, &v, nx, ny, dx, dy));
        });
    });

    group.bench_function("divergence_100x100", |b| {
        b.iter(|| {
            black_box(dynamics::divergence(&u, &v, nx, ny, dx, dy));
        });
    });

    group.bench_function("advection_100x100", |b| {
        b.iter(|| {
            black_box(dynamics::advection(&temperature, &u, &v, nx, ny, dx, dy));
        });
    });

    group.bench_function("laplacian_100x100", |b| {
        b.iter(|| {
            black_box(dynamics::laplacian(&temperature, nx, ny, dx, dy));
        });
    });

    group.bench_function("total_deformation_100x100", |b| {
        b.iter(|| {
            black_box(dynamics::total_deformation(&u, &v, nx, ny, dx, dy));
        });
    });

    group.finish();
}

// ─────────────────────────────────────────────
// Smoothing on 200x200 grids
// ─────────────────────────────────────────────

fn bench_smoothing(c: &mut Criterion) {
    let nx = 200;
    let ny = 200;
    let field = synthetic_grid(nx, ny, 280.0, 30.0);

    let mut group = c.benchmark_group("smoothing");

    group.bench_function("smooth_gaussian_200x200_sigma2", |b| {
        b.iter(|| {
            black_box(grib2::smooth_gaussian(&field, nx, ny, 2.0));
        });
    });

    group.bench_function("smooth_gaussian_200x200_sigma5", |b| {
        b.iter(|| {
            black_box(grib2::smooth_gaussian(&field, nx, ny, 5.0));
        });
    });

    // 400x400 benchmarks (Codex test size — larger grid shows cache effects)
    let nx4 = 400;
    let ny4 = 400;
    let field4: Vec<f64> = (0..nx4 * ny4).map(|i| (i as f64 * 0.01).sin()).collect();

    group.bench_function("smooth_gaussian_400x400_sigma2", |b| {
        b.iter(|| {
            black_box(grib2::smooth_gaussian(&field4, nx4, ny4, 2.0));
        });
    });

    group.bench_function("smooth_gaussian_400x400_sigma5", |b| {
        b.iter(|| {
            black_box(grib2::smooth_gaussian(&field4, nx4, ny4, 5.0));
        });
    });

    group.bench_function("smooth_n_point_9_1pass_200x200", |b| {
        b.iter(|| {
            black_box(grib2::smooth_n_point(&field, nx, ny, 9, 1));
        });
    });

    group.bench_function("smooth_n_point_5_3pass_200x200", |b| {
        b.iter(|| {
            black_box(grib2::smooth_n_point(&field, nx, ny, 5, 3));
        });
    });

    group.finish();
}

// ─────────────────────────────────────────────
// GRIB2 writer + parser round-trip
// ─────────────────────────────────────────────

fn bench_grib2_roundtrip(c: &mut Criterion) {
    let nx = 50;
    let ny = 50;
    let values = synthetic_grid(nx, ny, 270.0, 40.0);

    let grid = GridDefinition {
        template: 0,
        nx: nx as u32,
        ny: ny as u32,
        lat1: 30.0,
        lon1: -100.0,
        lat2: 40.0,
        lon2: -90.0,
        dx: 0.2,
        dy: 0.2,
        scan_mode: 0,
        ..GridDefinition::default()
    };

    let product = ProductDefinition {
        template: 0,
        parameter_category: 0,
        parameter_number: 0,
        generating_process: 2,
        forecast_time: 0,
        time_range_unit: 1,
        level_type: 103,
        level_value: 2.0,
    };

    // Pre-build the GRIB2 bytes for the parse-only benchmark
    let writer = Grib2Writer::new().add_message(
        MessageBuilder::new(0, values.clone())
            .grid(grid.clone())
            .product(product.clone())
            .packing(PackingMethod::Simple { bits_per_value: 16 }),
    );
    let grib_bytes = writer.to_bytes().unwrap();

    let mut group = c.benchmark_group("grib2_roundtrip");

    group.bench_function("write_50x50", |b| {
        b.iter(|| {
            let w = Grib2Writer::new().add_message(
                MessageBuilder::new(0, values.clone())
                    .grid(grid.clone())
                    .product(product.clone())
                    .packing(PackingMethod::Simple { bits_per_value: 16 }),
            );
            black_box(w.to_bytes().unwrap());
        });
    });

    group.bench_function("parse_50x50", |b| {
        b.iter(|| {
            let grib = grib2::Grib2File::from_bytes(black_box(&grib_bytes)).unwrap();
            black_box(&grib);
        });
    });

    group.bench_function("parse_unpack_50x50", |b| {
        b.iter(|| {
            let grib = grib2::Grib2File::from_bytes(&grib_bytes).unwrap();
            for msg in &grib.messages {
                black_box(grib2::unpack_message(msg).unwrap());
            }
        });
    });

    // Multi-message round-trip (5 messages)
    let multi_writer = {
        let mut w = Grib2Writer::new();
        for i in 0..5 {
            let vals = synthetic_grid(nx, ny, 250.0 + i as f64 * 10.0, 30.0);
            w = w.add_message(
                MessageBuilder::new(0, vals)
                    .grid(grid.clone())
                    .product(ProductDefinition {
                        parameter_category: i as u8,
                        ..product.clone()
                    })
                    .packing(PackingMethod::Simple { bits_per_value: 16 }),
            );
        }
        w
    };
    let multi_bytes = multi_writer.to_bytes().unwrap();

    group.bench_function("roundtrip_5msg_50x50", |b| {
        b.iter(|| {
            let grib = grib2::Grib2File::from_bytes(black_box(&multi_bytes)).unwrap();
            for msg in &grib.messages {
                black_box(grib2::unpack_message(msg).unwrap());
            }
        });
    });

    group.finish();
}

// ─────────────────────────────────────────────
// Search operations
// ─────────────────────────────────────────────

fn bench_search(c: &mut Criterion) {
    let nx = 10;
    let ny = 10;
    let grid = GridDefinition {
        template: 0,
        nx: nx as u32,
        ny: ny as u32,
        lat1: 30.0,
        lon1: -100.0,
        lat2: 39.0,
        lon2: -91.0,
        dx: 1.0,
        dy: 1.0,
        scan_mode: 0,
        ..GridDefinition::default()
    };

    // Build a multi-message GRIB2 file with varied products
    let categories = [0u8, 0, 0, 1, 2, 2, 3, 6, 7, 19];
    let param_numbers = [0u8, 4, 6, 1, 2, 3, 5, 1, 6, 0];
    let level_types = [103u8, 103, 100, 100, 103, 103, 1, 200, 200, 1];
    let level_values = [2.0, 2.0, 50000.0, 85000.0, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0];

    let mut writer = Grib2Writer::new();
    for i in 0..10 {
        let vals = synthetic_grid(nx, ny, 270.0 + i as f64, 10.0);
        writer = writer.add_message(
            MessageBuilder::new(0, vals)
                .grid(grid.clone())
                .product(ProductDefinition {
                    template: 0,
                    parameter_category: categories[i],
                    parameter_number: param_numbers[i],
                    generating_process: 2,
                    forecast_time: 0,
                    time_range_unit: 1,
                    level_type: level_types[i],
                    level_value: level_values[i],
                })
                .packing(PackingMethod::Simple { bits_per_value: 16 }),
        );
    }
    let bytes = writer.to_bytes().unwrap();
    let grib = grib2::Grib2File::from_bytes(&bytes).unwrap();

    let mut group = c.benchmark_group("search");

    group.bench_function("search_temperature", |b| {
        b.iter(|| {
            black_box(search_messages(&grib.messages, "temperature"));
        });
    });

    group.bench_function("search_wind", |b| {
        b.iter(|| {
            black_box(search_messages(&grib.messages, "wind"));
        });
    });

    group.bench_function("search_specific_level", |b| {
        b.iter(|| {
            black_box(search_messages(&grib.messages, "500 mb"));
        });
    });

    group.bench_function("search_no_match", |b| {
        b.iter(|| {
            black_box(search_messages(&grib.messages, "nonexistent_variable_xyz"));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_metfuncs,
    bench_dynamics,
    bench_smoothing,
    bench_grib2_roundtrip,
    bench_search,
);
criterion_main!(benches);
