//! Benchmark for core orbital mechanics computations.
//!
//! Tests: Hohmann transfers, transfer window calculations, gravity assists,
//! and Tsiolkovsky rocket equation computations.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use helios_ascension::fleets::orbital_mechanics::{
    compute_gravity_assist, compute_transfer_window, find_gravity_assist_options, hohmann_transfer,
    phase_dv_factor, AU_IN_METERS, GM_SUN,
};

fn bench_hohmann_transfer(c: &mut Criterion) {
    c.bench_function("hohmann_transfer_earth_mars", |b| {
        b.iter(|| {
            // Earth (1 AU) to Mars (1.52 AU)
            hohmann_transfer(black_box(1.0), black_box(1.52), black_box(GM_SUN))
        });
    });

    c.bench_function("hohmann_transfer_inner_solar", |b| {
        b.iter(|| {
            // Mercury to Venus
            hohmann_transfer(black_box(0.39), black_box(0.72), black_box(GM_SUN))
        });
    });

    c.bench_function("hohmann_transfer_outer_solar", |b| {
        b.iter(|| {
            // Jupiter to Saturn
            hohmann_transfer(black_box(5.2), black_box(9.58), black_box(GM_SUN))
        });
    });

    c.bench_function("hohmann_transfer_many_bodies", |b| {
        b.iter(|| {
            // Simulate transfer planning for many body pairs
            let bodies = [
                (0.39, 0.72),
                (0.72, 1.0),
                (1.0, 1.52),
                (1.52, 5.2),
                (5.2, 9.58),
                (9.58, 19.2),
                (19.2, 30.0),
            ];
            for (r1, r2) in bodies {
                hohmann_transfer(black_box(r1), black_box(r2), GM_SUN);
            }
        });
    });
}

fn bench_transfer_window(c: &mut Criterion) {
    let theta_earth = 0.0_f64; // Starting position
    let theta_mars = 2.094_f64; // ~120 degrees ahead

    c.bench_function("compute_transfer_window_earth_mars", |b| {
        b.iter(|| {
            compute_transfer_window(
                black_box(1.0),
                black_box(1.52),
                black_box(GM_SUN),
                black_box(theta_earth),
                black_box(theta_mars),
            )
        });
    });

    c.bench_function("compute_transfer_window_many_iterations", |b| {
        b.iter(|| {
            for i in 0..100 {
                let theta = (i as f64) * 0.1;
                compute_transfer_window(1.0, 1.52, GM_SUN, theta, theta + 2.094);
            }
        });
    });
}

fn bench_phase_dv_factor(c: &mut Criterion) {
    c.bench_function("phase_dv_factor_optimal", |b| {
        b.iter(|| phase_dv_factor(black_box(0.0)));
    });

    c.bench_function("phase_dv_factor_suboptimal", |b| {
        b.iter(|| phase_dv_factor(black_box(std::f64::consts::FRAC_PI_2)));
    });

    c.bench_function("phase_dv_factor_worst", |b| {
        b.iter(|| phase_dv_factor(black_box(std::f64::consts::PI)));
    });

    c.bench_function("phase_dv_factor_range", |b| {
        b.iter(|| {
            for i in 0..100 {
                let phi = (i as f64) * 0.0628; // 0 to ~2π
                phase_dv_factor(phi);
            }
        });
    });
}

fn bench_gravity_assist(c: &mut Criterion) {
    // Earth to Jupiter via Venus gravity assist (rare but possible)
    // Or just benchmark the Jupiter flyby as a high-value scenario
    c.bench_function("compute_gravity_assist_jupiter", |b| {
        b.iter(|| {
            compute_gravity_assist(
                black_box(1.0), // Earth orbit
                black_box(5.2), // Jupiter orbit
                black_box(5.2), // Jupiter as flyby
                black_box(GM_SUN),
                black_box(1.26687e17), // GM Jupiter
                black_box("Jupiter".to_string()),
                black_box(7.15e7 / AU_IN_METERS), // ~71500 km minimum periapsis
            )
        });
    });

    c.bench_function("find_gravity_assist_options_mars_jupiter", |b| {
        let bodies = vec![
            ("Mars".to_string(), 1.52, 4.0e14, 3397e3 / AU_IN_METERS),
            ("Ceres".to_string(), 2.77, 4.5e13, 473e3 / AU_IN_METERS),
            ("Jupiter".to_string(), 5.2, 1.26687e17, 71e6 / AU_IN_METERS),
        ];
        b.iter(|| {
            find_gravity_assist_options(
                black_box(1.52),
                black_box(5.2),
                black_box(GM_SUN),
                black_box(&bodies),
            )
        });
    });
}

fn bench_fuel_estimation(c: &mut Criterion) {
    use helios_ascension::fleets::orbital_mechanics::estimate_fuel_cost_tonnes;

    c.bench_function("estimate_fuel_cost_standard", |b| {
        b.iter(|| {
            estimate_fuel_cost_tonnes(
                black_box(1000.0), // dry mass tonnes
                black_box(5000.0), // delta-v m/s
                black_box(450.0),  // Isp seconds (chemical)
            )
        });
    });

    c.bench_function("estimate_fuel_cost_ion", |b| {
        b.iter(|| {
            estimate_fuel_cost_tonnes(
                black_box(1000.0),
                black_box(5000.0),
                black_box(5000.0), // Isp seconds (ion drive)
            )
        });
    });

    c.bench_function("estimate_fuel_cost_heavy_freighter", |b| {
        b.iter(|| {
            // Heavy freighter with big delta-v
            estimate_fuel_cost_tonnes(black_box(10000.0), black_box(10000.0), black_box(900.0))
        });
    });
}

criterion_group!(
    orbital_mechanics,
    bench_hohmann_transfer,
    bench_transfer_window,
    bench_phase_dv_factor,
    bench_gravity_assist,
    bench_fuel_estimation,
);
criterion_main!(orbital_mechanics);
