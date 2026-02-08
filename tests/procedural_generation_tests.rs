//! Integration tests for procedural star system generation
//!
//! Tests the complete workflow: frost line calculation, system architecture generation,
//! planet spawning, asteroid belts, cometary clouds, and metallicity bonuses.

use bevy::prelude::*;
use helios_ascension::astronomy::{
    calculate_frost_line, map_star_to_system_architecture, KeplerOrbit, PlanetType,
    ProceduralPlanet,
};
use helios_ascension::economy::components::{PlanetResources, SpectralClass, StarSystem};
use helios_ascension::economy::types::ResourceType;
use rand::rngs::StdRng;
use rand::SeedableRng;

#[test]
fn test_frost_line_calculations_for_different_stars() {
    // Test frost line for various stellar types

    // Sun (G2V): L = 1.0 L☉
    let sun_frost = calculate_frost_line(1.0);
    assert!(
        (sun_frost - 4.85).abs() < 0.01,
        "Sun frost line should be ~4.85 AU, got {:.2}",
        sun_frost
    );

    // Alpha Centauri A (G2V): L = 1.519 L☉
    let alpha_cen_a_frost = calculate_frost_line(1.519);
    assert!(
        alpha_cen_a_frost > 5.9 && alpha_cen_a_frost < 6.1,
        "Alpha Cen A frost line should be ~5.98 AU, got {:.2}",
        alpha_cen_a_frost
    );

    // Proxima Centauri (M5.5Ve): L = 0.0017 L☉
    let proxima_frost = calculate_frost_line(0.0017);
    assert!(
        proxima_frost < 0.21,
        "Proxima frost line should be ~0.20 AU, got {:.2}",
        proxima_frost
    );

    // Sirius A (A1V): L = 25.4 L☉
    let sirius_frost = calculate_frost_line(25.4);
    assert!(
        sirius_frost > 24.0 && sirius_frost < 25.0,
        "Sirius frost line should be ~24.4 AU, got {:.2}",
        sirius_frost
    );
}

#[test]
fn test_system_generation_for_empty_sun_like_system() {
    let mut rng = StdRng::seed_from_u64(12345);

    let architecture = map_star_to_system_architecture(
        "Test Star",
        1.0, // Solar luminosity
        0,   // No existing planets
        &[], // No existing orbits
        &mut rng,
    );

    // Should generate enough planets to reach target of 5
    let total_planets = architecture.rocky_planets.len() + architecture.gas_giants.len();
    assert!(
        total_planets >= 4 && total_planets <= 7,
        "Expected 4-7 planets for empty system, got {}",
        total_planets
    );

    // Frost line should be solar-like
    assert!(
        (architecture.frost_line_au - 4.85).abs() < 0.5,
        "Frost line should be ~4.85 AU, got {:.2}",
        architecture.frost_line_au
    );

    // Should likely have an asteroid belt
    assert!(
        architecture.asteroid_belt.is_some(),
        "System should likely have an asteroid belt"
    );
}

#[test]
fn test_system_generation_respects_existing_planets() {
    let mut rng = StdRng::seed_from_u64(67890);

    // System with 3 existing planets at specific orbits
    let existing_orbits = vec![0.72, 1.0, 1.52]; // Venus, Earth, Mars-like

    let architecture = map_star_to_system_architecture(
        "Test Star",
        1.0,
        3, // 3 existing planets
        &existing_orbits,
        &mut rng,
    );

    // Should generate 2-3 more planets to reach target of 5-6
    let total_planets = architecture.rocky_planets.len() + architecture.gas_giants.len();
    assert!(
        total_planets >= 1 && total_planets <= 4,
        "Expected 1-4 new planets to fill gaps, got {}",
        total_planets
    );

    // New planets should not overlap with existing ones
    for planet in architecture.rocky_planets.iter() {
        for &existing in &existing_orbits {
            let separation = (planet.semi_major_axis_au - existing).abs();
            assert!(
                separation > 0.1,
                "Rocky planet at {:.2} AU too close to existing planet at {:.2} AU (sep: {:.3})",
                planet.semi_major_axis_au,
                existing,
                separation
            );
        }
    }

    for planet in architecture.gas_giants.iter() {
        for &existing in &existing_orbits {
            let separation = (planet.semi_major_axis_au - existing).abs();
            assert!(
                separation > 0.5,
                "Gas giant at {:.2} AU too close to existing planet at {:.2} AU (sep: {:.3})",
                planet.semi_major_axis_au,
                existing,
                separation
            );
        }
    }
}

#[test]
fn test_rocky_planets_inside_frost_line() {
    let mut rng = StdRng::seed_from_u64(11111);

    let architecture = map_star_to_system_architecture("Test Star", 1.0, 0, &[], &mut rng);

    let frost_line = architecture.frost_line_au;

    // All rocky planets should be inside the frost line
    for planet in &architecture.rocky_planets {
        assert!(
            planet.semi_major_axis_au < frost_line,
            "Rocky planet at {:.2} AU should be inside frost line ({:.2} AU)",
            planet.semi_major_axis_au,
            frost_line
        );

        assert_eq!(
            planet.planet_type,
            PlanetType::Rocky,
            "Planet should be rocky type"
        );

        // Rocky planets should have reasonable masses (0.1 - 10 M⊕)
        assert!(
            planet.mass_earth > 0.1 && planet.mass_earth < 10.0,
            "Rocky planet mass {:.1} M⊕ out of reasonable range",
            planet.mass_earth
        );

        // Low eccentricity for inner system
        assert!(
            planet.eccentricity < 0.3,
            "Rocky planet eccentricity {:.2} too high",
            planet.eccentricity
        );
    }
}

#[test]
fn test_gas_giants_outside_frost_line() {
    let mut rng = StdRng::seed_from_u64(22222);

    let architecture = map_star_to_system_architecture("Test Star", 1.0, 0, &[], &mut rng);

    let frost_line = architecture.frost_line_au;

    // All gas/ice giants should be outside the frost line
    for planet in &architecture.gas_giants {
        assert!(
            planet.semi_major_axis_au > frost_line,
            "Gas giant at {:.2} AU should be outside frost line ({:.2} AU)",
            planet.semi_major_axis_au,
            frost_line
        );

        assert!(
            planet.planet_type == PlanetType::GasGiant
                || planet.planet_type == PlanetType::IceGiant,
            "Planet should be gas or ice giant type"
        );

        // Giants should have significant mass (> 10 M⊕)
        assert!(
            planet.mass_earth > 10.0,
            "Gas giant mass {:.1} M⊕ too low",
            planet.mass_earth
        );
    }
}

#[test]
fn test_asteroid_belt_generation() {
    let mut rng = StdRng::seed_from_u64(33333);

    let architecture = map_star_to_system_architecture("Test Star", 1.0, 0, &[], &mut rng);

    if let Some(belt) = &architecture.asteroid_belt {
        // Belt should be in reasonable location
        assert!(
            belt.inner_au < belt.outer_au,
            "Belt inner edge {:.2} should be less than outer edge {:.2}",
            belt.inner_au,
            belt.outer_au
        );

        // Belt should have reasonable width (0.5 - 3 AU typically)
        let width = belt.outer_au - belt.inner_au;
        assert!(
            width > 0.3 && width < 5.0,
            "Belt width {:.2} AU seems unreasonable",
            width
        );

        // Should spawn a reasonable number of asteroids
        assert!(
            belt.count >= 50 && belt.count <= 200,
            "Belt asteroid count {} out of expected range",
            belt.count
        );
    }
}

#[test]
fn test_cometary_cloud_generation() {
    let mut rng = StdRng::seed_from_u64(44444);

    let architecture = map_star_to_system_architecture("Test Star", 1.0, 0, &[], &mut rng);

    if let Some(cloud) = &architecture.cometary_cloud {
        // Cloud should be in outer system
        assert!(
            cloud.inner_au > 15.0,
            "Cometary cloud inner edge {:.2} AU too close to star",
            cloud.inner_au
        );

        assert!(
            cloud.outer_au > cloud.inner_au,
            "Cloud inner edge {:.2} should be less than outer edge {:.2}",
            cloud.inner_au,
            cloud.outer_au
        );

        // Should spawn a reasonable number of comets
        assert!(
            cloud.count >= 20 && cloud.count <= 80,
            "Cloud comet count {} out of expected range",
            cloud.count
        );
    }
}

#[test]
fn test_procedural_planet_kepler_orbit_conversion() {
    let mut rng = StdRng::seed_from_u64(55555);

    let architecture = map_star_to_system_architecture("Test Star", 1.0, 0, &[], &mut rng);

    // Test conversion to KeplerOrbit for all generated planets
    for planet in architecture.rocky_planets.iter() {
        let kepler = planet.to_kepler_orbit();

        assert_eq!(
            kepler.semi_major_axis, planet.semi_major_axis_au,
            "Semi-major axis should match"
        );
        assert_eq!(
            kepler.eccentricity, planet.eccentricity,
            "Eccentricity should match"
        );
        assert!(kepler.mean_motion > 0.0, "Mean motion should be positive");

        // Verify Kepler's third law: T² ∝ a³
        let period_from_orbit = std::f64::consts::TAU / kepler.mean_motion;
        let period_from_planet = planet.period_days * 86400.0;
        assert!(
            (period_from_orbit - period_from_planet).abs() < 1.0,
            "Orbital period mismatch: {:.1} vs {:.1} seconds",
            period_from_orbit,
            period_from_planet
        );
    }
}

#[test]
fn test_metallicity_multiplier() {
    // Test the metallicity multiplier calculation

    // Solar metallicity ([Fe/H] = 0.0) should give 1.0x
    let solar = StarSystem::with_metallicity(4.85, SpectralClass::G, 0.0);
    assert!(
        (solar.metallicity_multiplier() - 1.0).abs() < 0.01,
        "Solar metallicity should give 1.0x multiplier, got {:.3}",
        solar.metallicity_multiplier()
    );

    // Metal-rich star ([Fe/H] = +0.3) should give higher multiplier
    let metal_rich = StarSystem::with_metallicity(4.85, SpectralClass::G, 0.3);
    assert!(
        metal_rich.metallicity_multiplier() > 1.15,
        "Metal-rich star should give >1.15x multiplier, got {:.3}",
        metal_rich.metallicity_multiplier()
    );

    // Metal-poor star ([Fe/H] = -0.3) should give lower multiplier
    let metal_poor = StarSystem::with_metallicity(4.85, SpectralClass::G, -0.3);
    assert!(
        metal_poor.metallicity_multiplier() < 0.85,
        "Metal-poor star should give <0.85x multiplier, got {:.3}",
        metal_poor.metallicity_multiplier()
    );

    // Test clamping: very high metallicity
    let very_high = StarSystem::with_metallicity(4.85, SpectralClass::G, 2.0);
    assert!(
        very_high.metallicity_multiplier() <= 1.5,
        "Multiplier should be clamped at 1.5x, got {:.3}",
        very_high.metallicity_multiplier()
    );

    // Test clamping: very low metallicity
    let very_low = StarSystem::with_metallicity(4.85, SpectralClass::G, -2.0);
    assert!(
        very_low.metallicity_multiplier() >= 0.5,
        "Multiplier should be clamped at 0.5x, got {:.3}",
        very_low.metallicity_multiplier()
    );
}

#[test]
fn test_dim_star_system_generation() {
    // Test generation for a dim M-dwarf star (like Proxima Centauri)
    let mut rng = StdRng::seed_from_u64(66666);

    let architecture = map_star_to_system_architecture(
        "Proxima",
        0.0017, // Very low luminosity
        0,
        &[],
        &mut rng,
    );

    // Frost line should be very close to star
    assert!(
        architecture.frost_line_au < 0.25,
        "M-dwarf frost line should be < 0.25 AU, got {:.2}",
        architecture.frost_line_au
    );

    // Rocky planets should be very close in
    for planet in &architecture.rocky_planets {
        assert!(
            planet.semi_major_axis_au < 0.25,
            "Rocky planet should be < 0.25 AU for M-dwarf, got {:.2}",
            planet.semi_major_axis_au
        );
    }
}

#[test]
fn test_bright_star_system_generation() {
    // Test generation for a bright A-type star (like Sirius A)
    let mut rng = StdRng::seed_from_u64(77777);

    let architecture = map_star_to_system_architecture(
        "Sirius",
        25.4, // High luminosity
        0,
        &[],
        &mut rng,
    );

    // Frost line should be far from star
    assert!(
        architecture.frost_line_au > 20.0,
        "A-type star frost line should be > 20 AU, got {:.2}",
        architecture.frost_line_au
    );

    // If there are rocky planets, they should span a large inner system
    if !architecture.rocky_planets.is_empty() {
        let max_rocky_orbit = architecture
            .rocky_planets
            .iter()
            .map(|p| p.semi_major_axis_au)
            .fold(0.0, f64::max);

        assert!(
            max_rocky_orbit > 10.0,
            "Bright star should have rocky planets beyond 10 AU, got {:.2}",
            max_rocky_orbit
        );
    }
}

#[test]
fn test_deterministic_generation_with_seed() {
    // Test that generation is deterministic with the same seed

    let mut rng1 = StdRng::seed_from_u64(99999);
    let arch1 = map_star_to_system_architecture("Star", 1.0, 0, &[], &mut rng1);

    let mut rng2 = StdRng::seed_from_u64(99999);
    let arch2 = map_star_to_system_architecture("Star", 1.0, 0, &[], &mut rng2);

    // Should generate same number of planets
    assert_eq!(
        arch1.rocky_planets.len(),
        arch2.rocky_planets.len(),
        "Should generate same number of rocky planets with same seed"
    );
    assert_eq!(
        arch1.gas_giants.len(),
        arch2.gas_giants.len(),
        "Should generate same number of gas giants with same seed"
    );

    // First rocky planet should have same properties
    if !arch1.rocky_planets.is_empty() {
        let p1 = &arch1.rocky_planets[0];
        let p2 = &arch2.rocky_planets[0];

        assert!(
            (p1.semi_major_axis_au - p2.semi_major_axis_au).abs() < 0.001,
            "Rocky planet orbits should match with same seed"
        );
        assert!(
            (p1.mass_earth - p2.mass_earth).abs() < 0.001,
            "Rocky planet masses should match with same seed"
        );
    }
}
