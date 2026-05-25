//! Benchmark for astronomy/orbital propagation computations.
//!
//! Tests: Keplerian orbit propagation, position calculations,
//! coordinate transforms, and ephemeris computations.

use bevy::math::DVec3;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use helios_ascension::astronomy::{orbit_position_from_mean_anomaly, KeplerOrbit};

/// Create a test orbit (Earth-like)
fn earth_orbit() -> KeplerOrbit {
    KeplerOrbit {
        semi_major_axis: 1.0,
        eccentricity: 0.01671,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        mean_anomaly_epoch: 0.0,
        mean_motion: 1.991e-7, // ~1 year orbit in rad/s
    }
}

/// Create a Mars-like orbit
fn mars_orbit() -> KeplerOrbit {
    KeplerOrbit {
        semi_major_axis: 1.524,
        eccentricity: 0.0934,
        inclination: 0.0323,
        longitude_ascending_node: 0.849,
        argument_of_periapsis: 4.602,
        mean_anomaly_epoch: 5.1,
        mean_motion: 1.058e-7,
    }
}

/// Create a Jupiter-like orbit
fn jupiter_orbit() -> KeplerOrbit {
    KeplerOrbit {
        semi_major_axis: 5.2,
        eccentricity: 0.0489,
        inclination: 0.0228,
        longitude_ascending_node: 1.755,
        argument_of_periapsis: 3.085,
        mean_anomaly_epoch: 0.3,
        mean_motion: 1.678e-8,
    }
}

/// Compute mean anomaly for an orbit at a given elapsed time.
fn compute_mean_anomaly(orbit: &KeplerOrbit, elapsed_seconds: f64) -> f64 {
    orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_seconds
}

/// Propagate orbit to a given time and get the resulting position.
fn propagate_orbit_position(orbit: &KeplerOrbit, elapsed_seconds: f64) -> DVec3 {
    let mean_anomaly = compute_mean_anomaly(orbit, elapsed_seconds);
    orbit_position_from_mean_anomaly(orbit, mean_anomaly)
}

fn bench_orbit_position_from_mean_anomaly(c: &mut Criterion) {
    let earth = earth_orbit();
    let mars = mars_orbit();
    let jupiter = jupiter_orbit();

    c.bench_function("orbit_position_earth", |b| {
        b.iter(|| {
            orbit_position_from_mean_anomaly(
                black_box(&earth),
                black_box(1.5), // mean anomaly in radians
            )
        });
    });

    c.bench_function("orbit_position_mars", |b| {
        b.iter(|| orbit_position_from_mean_anomaly(black_box(&mars), black_box(2.5)));
    });

    c.bench_function("orbit_position_jupiter", |b| {
        b.iter(|| orbit_position_from_mean_anomaly(black_box(&jupiter), black_box(1.0)));
    });

    c.bench_function("orbit_position_many_bodies", |b| {
        b.iter(|| {
            let orbits = [earth.clone(), mars.clone(), jupiter.clone()];
            for (i, orbit) in orbits.iter().enumerate() {
                let mean_anomaly = (i as f64) * 0.5;
                orbit_position_from_mean_anomaly(orbit, mean_anomaly);
            }
        });
    });
}

fn bench_propagate_orbit_position(c: &mut Criterion) {
    let earth = earth_orbit();
    let mars = mars_orbit();

    c.bench_function("propagate_orbit_earth_one_year", |b| {
        b.iter(|| {
            propagate_orbit_position(
                black_box(&earth),
                black_box(365.25 * 24.0 * 3600.0), // 1 year in seconds
            )
        });
    });

    c.bench_function("propagate_orbit_mars_one_year", |b| {
        b.iter(|| propagate_orbit_position(black_box(&mars), black_box(365.25 * 24.0 * 3600.0)));
    });

    c.bench_function("propagate_orbit_earth_short_times", |b| {
        b.iter(|| {
            let times = [3600.0, 86400.0, 604800.0, 2592000.0]; // 1h, 1d, 1w, 1mo
            for &t in &times {
                propagate_orbit_position(black_box(&earth), black_box(t));
            }
        });
    });
}

fn bench_simulation_time_propagation(c: &mut Criterion) {
    c.bench_function("propagate_planet_positions_one_year", |b| {
        // Simulate propagating positions for inner planets
        let orbits = vec![
            // Mercury
            KeplerOrbit {
                semi_major_axis: 0.387,
                eccentricity: 0.2056,
                inclination: 0.122,
                longitude_ascending_node: 0.508,
                argument_of_periapsis: 0.219,
                mean_anomaly_epoch: 0.0,
                mean_motion: 1.24e-6,
            },
            // Venus
            KeplerOrbit {
                semi_major_axis: 0.723,
                eccentricity: 0.0067,
                inclination: 0.059,
                longitude_ascending_node: 1.332,
                argument_of_periapsis: 1.175,
                mean_anomaly_epoch: 0.0,
                mean_motion: 4.90e-7,
            },
            // Earth
            earth_orbit(),
            // Mars
            mars_orbit(),
        ];

        b.iter(|| {
            let sim_time = 86400.0 * 365.25; // 1 year
            let mut positions = Vec::with_capacity(orbits.len());
            for orbit in &orbits {
                positions.push(propagate_orbit_position(
                    black_box(orbit),
                    black_box(sim_time),
                ));
            }
            black_box(positions)
        });
    });
}

fn bench_high_eccentricity_orbits(c: &mut Criterion) {
    // High eccentricity orbits (comets, etc.)
    let elliptical = KeplerOrbit {
        semi_major_axis: 2.0,
        eccentricity: 0.9, // Very elliptical
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        mean_anomaly_epoch: 0.0,
        mean_motion: 1e-7,
    };

    c.bench_function("orbit_position_high_eccentricity", |b| {
        b.iter(|| orbit_position_from_mean_anomaly(black_box(&elliptical), black_box(1.0)));
    });

    c.bench_function("propagate_high_eccentricity_full_orbit", |b| {
        // Propagate for large time offset
        b.iter(|| propagate_orbit_position(black_box(&elliptical), black_box(1e9)));
    });
}

fn bench_distance_calculations(c: &mut Criterion) {
    let earth = earth_orbit();
    let mars = mars_orbit();

    c.bench_function("distance_between_two_bodies", |b| {
        let pos_earth = orbit_position_from_mean_anomaly(&earth, 1.0);
        let pos_mars = orbit_position_from_mean_anomaly(&mars, 2.5);

        b.iter(|| {
            let diff = pos_earth - pos_mars;
            black_box(diff.length())
        });
    });

    c.bench_function("distance_batch_calculation", |b| {
        // Calculate distances between multiple body positions
        let positions: Vec<DVec3> = (0..8)
            .map(|i| orbit_position_from_mean_anomaly(&earth, i as f64 * 0.5))
            .collect();

        b.iter(|| {
            let mut distances = Vec::new();
            for i in 0..positions.len() {
                for j in (i + 1)..positions.len() {
                    let diff = positions[i] - positions[j];
                    distances.push(diff.length());
                }
            }
            black_box(distances)
        });
    });
}

criterion_group!(
    astronomy,
    bench_orbit_position_from_mean_anomaly,
    bench_propagate_orbit_position,
    bench_simulation_time_propagation,
    bench_high_eccentricity_orbits,
    bench_distance_calculations,
);
criterion_main!(astronomy);
