//! Benchmark for fleet management computations.
//!
//! Tests: Fleet spawn, transfer planning, fuel calculations,
//! and fleet state updates.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use helios_ascension::fleets::{
    orbital_mechanics::{
        calculate_transfer_options, compute_transfer_window, hohmann_transfer, AU_IN_METERS, GM_SUN,
    },
    PropulsionType, ShipClass,
};

fn bench_transfer_options_calculation(c: &mut Criterion) {
    c.bench_function("calculate_transfer_options_earth_mars", |b| {
        b.iter(|| {
            calculate_transfer_options(
                black_box(1.0),
                black_box(1.52),
                black_box(GM_SUN),
                black_box(0.0), // co-planar
            )
        });
    });

    c.bench_function("calculate_transfer_options_jupiter_saturn", |b| {
        b.iter(|| {
            calculate_transfer_options(
                black_box(5.2),
                black_box(9.58),
                black_box(GM_SUN),
                black_box(0.0),
            )
        });
    });

    c.bench_function("calculate_transfer_options_inclined", |b| {
        b.iter(|| {
            calculate_transfer_options(
                black_box(1.0),
                black_box(1.52),
                black_box(GM_SUN),
                black_box(0.1), // 0.1 rad inclination
            )
        });
    });
}

fn bench_propulsion_types(c: &mut Criterion) {
    c.bench_function("propulsion_thrust_chemical", |b| {
        b.iter(|| PropulsionType::Chemical.thrust_kn(black_box(2000.0)));
    });

    c.bench_function("propulsion_thrust_ion", |b| {
        b.iter(|| PropulsionType::IonDrive.thrust_kn(black_box(2000.0)));
    });

    c.bench_function("propulsion_thrust_all_types", |b| {
        let mass = 2000.0;
        b.iter(|| {
            for prop_type in [
                PropulsionType::Chemical,
                PropulsionType::NuclearThermal,
                PropulsionType::IonDrive,
                PropulsionType::NuclearPulse,
                PropulsionType::FusionTorch,
            ] {
                prop_type.thrust_kn(mass);
            }
        });
    });

    c.bench_function("ship_class_fuel_capacity", |b| {
        b.iter(|| {
            for ship_class in [
                ShipClass::Courier,
                ShipClass::Frigate,
                ShipClass::Destroyer,
                ShipClass::Cruiser,
                ShipClass::ResearchVessel,
                ShipClass::Freighter,
                ShipClass::Station,
            ] {
                let fuel = match ship_class {
                    ShipClass::Courier => 20.0,
                    ShipClass::Frigate => 100.0,
                    ShipClass::Destroyer => 200.0,
                    ShipClass::Cruiser => 500.0,
                    ShipClass::ResearchVessel => 150.0,
                    ShipClass::Freighter => 800.0,
                    ShipClass::Station => 50.0,
                };
                black_box(fuel);
            }
        });
    });
}

fn bench_fleet_transfer_planning(c: &mut Criterion) {
    use helios_ascension::fleets::orbital_mechanics::calculate_transfer_options_phased;

    // Earth-Mars transfer with current window
    let window = compute_transfer_window(1.0, 1.52, GM_SUN, 0.0, 2.094);

    c.bench_function("plan_transfer_with_window", |b| {
        b.iter(|| {
            calculate_transfer_options_phased(
                black_box(1.0),
                black_box(1.52),
                black_box(GM_SUN),
                black_box(0.0), // depart now
                black_box(&window),
                black_box(0.0),
            )
        });
    });

    c.bench_function("plan_transfer_departure_offset", |b| {
        b.iter(|| {
            // Plan for departure in 30 days
            let offset_days = 30.0 * 86400.0;
            calculate_transfer_options_phased(
                black_box(1.0),
                black_box(1.52),
                black_box(GM_SUN),
                black_box(offset_days),
                black_box(&window),
                black_box(0.0),
            )
        });
    });

    c.bench_function("plan_multiple_route_options", |b| {
        b.iter(|| {
            let routes = [
                (1.0, 1.52),  // Earth to Mars
                (1.52, 5.2),  // Mars to Jupiter
                (5.2, 9.58),  // Jupiter to Saturn
                (9.58, 19.2), // Saturn to Uranus
            ];
            for (r1, r2) in routes {
                let win = compute_transfer_window(r1, r2, GM_SUN, 0.0, 1.5);
                calculate_transfer_options_phased(r1, r2, GM_SUN, 0.0, &win, 0.0);
            }
        });
    });
}

fn bench_fuel_computations(c: &mut Criterion) {
    use helios_ascension::fleets::orbital_mechanics::{
        estimate_fuel_cost_tonnes, rocket_equation_fuel_fraction,
    };

    c.bench_function("estimate_fuel_small_ship", |b| {
        b.iter(|| {
            estimate_fuel_cost_tonnes(
                black_box(100.0),  // dry mass
                black_box(2000.0), // delta-v m/s
                black_box(450.0),  // Isp chemical
            )
        });
    });

    c.bench_function("estimate_fuel_heavy_freighter", |b| {
        b.iter(|| {
            estimate_fuel_cost_tonnes(
                black_box(10000.0),
                black_box(8000.0),
                black_box(900.0), // Nuclear thermal
            )
        });
    });

    c.bench_function("fuel_fraction_chemical", |b| {
        b.iter(|| rocket_equation_fuel_fraction(black_box(2000.0), black_box(450.0)));
    });

    c.bench_function("fuel_fraction_ion", |b| {
        b.iter(|| rocket_equation_fuel_fraction(black_box(2000.0), black_box(5000.0)));
    });

    c.bench_function("fuel_fraction_fusion", |b| {
        b.iter(|| rocket_equation_fuel_fraction(black_box(5000.0), black_box(50000.0)));
    });

    c.bench_function("fuel_calculation_fleet_mass", |b| {
        b.iter(|| {
            // Calculate fuel for a fleet with multiple ships
            let ships = [100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
            let dv = 3000.0;
            let isp = 450.0;
            for mass in ships {
                estimate_fuel_cost_tonnes(mass, dv, isp);
            }
        });
    });
}

fn bench_simulation_stress(c: &mut Criterion) {
    use helios_ascension::fleets::orbital_mechanics::calculate_transfer_options_phased;

    c.bench_function("stress_full_route_sequence", |b| {
        b.iter(|| {
            // Simulate planning a sequence of transfers through the solar system
            let bodies = [
                (1.0, 1.52, "Earth-Mars"),
                (1.52, 5.2, "Mars-Jupiter"),
                (5.2, 9.58, "Jupiter-Saturn"),
                (9.58, 19.2, "Saturn-Uranus"),
                (19.2, 30.0, "Uranus-Neptune"),
            ];
            for (r1, r2, _name) in bodies {
                let window = compute_transfer_window(r1, r2, GM_SUN, 0.0, 1.5);
                let options = calculate_transfer_options_phased(r1, r2, GM_SUN, 0.0, &window, 0.0);
                black_box(options);
            }
        });
    });

    c.bench_function("stress_multi_window_calculation", |b| {
        b.iter(|| {
            // Calculate transfer windows for many departure times
            for day_offset in 0..30 {
                let offset = day_offset as f64 * 86400.0;
                let window = compute_transfer_window(1.0, 1.52, GM_SUN, 0.0, 2.094 + offset * 1e-7);
                black_box(window.time_to_window_s);
            }
        });
    });
}

criterion_group!(
    fleet_computations,
    bench_transfer_options_calculation,
    bench_propulsion_types,
    bench_fleet_transfer_planning,
    bench_fuel_computations,
    bench_simulation_stress,
);
criterion_main!(fleet_computations);
