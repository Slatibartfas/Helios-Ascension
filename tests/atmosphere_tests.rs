use helios_ascension::astronomy::{AtmosphereComposition, AtmosphericGas};

#[test]
fn test_earth_atmosphere_is_breathable() {
    let earth_atmosphere = AtmosphereComposition::new(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
            AtmosphericGas::new("Ar", 0.93),
            AtmosphericGas::new("CO2", 0.04),
        ],
    );

    assert!(
        earth_atmosphere.breathable,
        "Earth atmosphere should be breathable"
    );
    assert_eq!(earth_atmosphere.surface_pressure_mbar, 1013.0);
    assert_eq!(earth_atmosphere.surface_temperature_celsius, 15.0);
    assert!(earth_atmosphere.has_gas("O2"));
    assert!(earth_atmosphere.has_gas("N2"));
    assert_eq!(earth_atmosphere.get_gas_percentage("O2"), Some(21.0));
}

#[test]
fn test_mars_atmosphere_not_breathable() {
    let mars_atmosphere = AtmosphereComposition::new(
        6.0,
        -63.0,
        vec![
            AtmosphericGas::new("CO2", 95.0),
            AtmosphericGas::new("N2", 2.7),
            AtmosphericGas::new("Ar", 1.6),
            AtmosphericGas::new("O2", 0.13),
        ],
    );

    assert!(
        !mars_atmosphere.breathable,
        "Mars atmosphere should not be breathable"
    );
    assert_eq!(mars_atmosphere.surface_pressure_mbar, 6.0);
    assert!(mars_atmosphere.has_gas("CO2"));
}

#[test]
fn test_venus_atmosphere_not_breathable() {
    let venus_atmosphere = AtmosphereComposition::new(
        92000.0,
        465.0,
        vec![
            AtmosphericGas::new("CO2", 96.5),
            AtmosphericGas::new("N2", 3.5),
        ],
    );

    assert!(
        !venus_atmosphere.breathable,
        "Venus atmosphere should not be breathable"
    );
    assert_eq!(venus_atmosphere.surface_pressure_mbar, 92000.0);
    assert!(venus_atmosphere.has_gas("CO2"));
    assert!(!venus_atmosphere.has_gas("O2"));
}

#[test]
fn test_jupiter_atmosphere() {
    let jupiter_atmosphere = AtmosphereComposition::new(
        1000.0,
        -145.0,
        vec![
            AtmosphericGas::new("H2", 90.0),
            AtmosphericGas::new("He", 10.0),
        ],
    );

    assert!(
        !jupiter_atmosphere.breathable,
        "Jupiter atmosphere should not be breathable"
    );
    assert!(jupiter_atmosphere.has_gas("H2"));
    assert!(jupiter_atmosphere.has_gas("He"));
}

#[test]
fn test_titan_atmosphere() {
    let titan_atmosphere = AtmosphereComposition::new(
        1500.0,
        -179.0,
        vec![
            AtmosphericGas::new("N2", 98.4),
            AtmosphericGas::new("CH4", 1.4),
        ],
    );

    assert!(
        !titan_atmosphere.breathable,
        "Titan atmosphere should not be breathable (no O2)"
    );
    assert!(titan_atmosphere.has_gas("N2"));
    assert!(titan_atmosphere.has_gas("CH4"));
    // Titan has high pressure but no oxygen
    assert!(!titan_atmosphere.has_gas("O2"));
}

#[test]
fn test_colony_cost_calculation_earth() {
    let earth_atmosphere = AtmosphereComposition::new(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
            AtmosphericGas::new("Ar", 0.93),
        ],
    );

    let cost = earth_atmosphere.calculate_colony_cost(1.0, 15.0, 15.0);
    assert!(cost < 0.01, "Earth should have colony cost of 0");
}

#[test]
fn test_colony_cost_calculation_mars() {
    let mars_atmosphere = AtmosphereComposition::new(
        6.0,
        -63.0,
        vec![
            AtmosphericGas::new("CO2", 95.0),
            AtmosphericGas::new("N2", 2.7),
        ],
    );

    let cost = mars_atmosphere.calculate_colony_cost(0.379, -63.0, -63.0);
    // Mars: atmosphere cost ~1.2 + temp cost ~1.0 + pressure ~1.5 + gravity ~0.8 ≈ 4.5
    assert!(
        cost > 2.0 && cost < 8.0,
        "Mars should have moderate colony cost (got {})",
        cost
    );
}

#[test]
fn test_colony_cost_calculation_venus() {
    let venus_atmosphere = AtmosphereComposition::new(
        92000.0,
        465.0,
        vec![
            AtmosphericGas::new("CO2", 96.5),
            AtmosphericGas::new("N2", 3.5),
        ],
    );

    let cost = venus_atmosphere.calculate_colony_cost(0.904, 465.0, 465.0);
    // Venus: atmosphere ~1.2 + temp ~2.9 + pressure ~1.3 ≈ 5.4  (bounded 0–10 scale)
    assert!(
        cost > 4.0 && cost <= 10.0,
        "Venus should have high colony cost (got {})",
        cost
    );
}

#[test]
fn test_atmospheric_gas_creation() {
    let gas = AtmosphericGas::new("O2", 21.0);
    assert_eq!(gas.name, "O2");
    assert_eq!(gas.percentage, 21.0);
}

#[test]
fn test_get_gas_percentage_nonexistent() {
    let atmosphere = AtmosphereComposition::new(
        1000.0,
        0.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
        ],
    );

    assert_eq!(atmosphere.get_gas_percentage("He"), None);
    assert!(!atmosphere.has_gas("He"));
}

#[test]
fn test_escape_velocity_calculation() {
    // Test Earth's escape velocity (should be ~11.2 km/s)
    let earth_mass = 5.97237e24; // kg
    let earth_radius = 6371.0; // km
    let earth_escape_velocity =
        AtmosphereComposition::calculate_escape_velocity(earth_mass, earth_radius);

    // Should be close to 11.2 km/s (within 0.1 km/s)
    assert!(
        (earth_escape_velocity - 11.2).abs() < 0.1,
        "Earth escape velocity should be ~11.2 km/s, got {}",
        earth_escape_velocity
    );

    // Test Moon's escape velocity (should be ~2.4 km/s)
    let moon_mass = 7.342e22; // kg
    let moon_radius = 1737.4; // km
    let moon_escape_velocity =
        AtmosphereComposition::calculate_escape_velocity(moon_mass, moon_radius);

    // Should be close to 2.4 km/s
    assert!(
        (moon_escape_velocity - 2.4).abs() < 0.1,
        "Moon escape velocity should be ~2.4 km/s, got {}",
        moon_escape_velocity
    );

    // Test Jupiter's escape velocity (should be ~60 km/s)
    let jupiter_mass = 1.8982e27; // kg
    let jupiter_radius = 69911.0; // km
    let jupiter_escape_velocity =
        AtmosphereComposition::calculate_escape_velocity(jupiter_mass, jupiter_radius);

    // Should be close to 60 km/s (within 1 km/s)
    assert!(
        (jupiter_escape_velocity - 60.0).abs() < 1.0,
        "Jupiter escape velocity should be ~60 km/s, got {}",
        jupiter_escape_velocity
    );
}

#[test]
fn test_atmosphere_retention() {
    // Earth should retain atmosphere (escape velocity ~11.2 km/s)
    let earth_mass = 5.97237e24; // kg
    let earth_radius = 6371.0; // km
    assert!(
        AtmosphereComposition::can_retain_atmosphere(earth_mass, earth_radius),
        "Earth should be able to retain an atmosphere"
    );

    // Mars should retain atmosphere (escape velocity ~5.0 km/s)
    let mars_mass = 6.4171e23; // kg
    let mars_radius = 3389.5; // km
    assert!(
        AtmosphereComposition::can_retain_atmosphere(mars_mass, mars_radius),
        "Mars should be able to retain an atmosphere"
    );

    // Moon is at the retention threshold (escape velocity ~2.4 km/s)
    let moon_mass = 7.342e22; // kg
    let moon_radius = 1737.4; // km
    assert!(
        AtmosphereComposition::can_retain_atmosphere(moon_mass, moon_radius),
        "Moon is at boundary: can retain heavy gases but threshold is ≥ 2.0 km/s"
    );

    // Titan should retain atmosphere (escape velocity ~2.6 km/s, denser than Moon)
    let titan_mass = 1.3452e23; // kg
    let titan_radius = 2574.73; // km
    assert!(
        AtmosphereComposition::can_retain_atmosphere(titan_mass, titan_radius),
        "Titan should be able to retain an atmosphere"
    );

    // Very small asteroid should NOT retain atmosphere
    let small_asteroid_mass = 1.0e15; // kg (very small)
    let small_asteroid_radius = 1.0; // km
    assert!(
        !AtmosphereComposition::can_retain_atmosphere(small_asteroid_mass, small_asteroid_radius),
        "Small asteroid should not be able to retain an atmosphere"
    );
}

#[test]
fn test_atmosphere_with_body_data() {
    // Test Earth with body data
    let earth_atmosphere = AtmosphereComposition::new_with_body_data(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
            AtmosphericGas::new("Ar", 0.93),
        ],
        5.97237e24, // Earth mass
        6371.0,     // Earth radius
        false,      // Surface pressure
    );

    assert!(
        earth_atmosphere.can_support_atmosphere,
        "Earth should support atmosphere"
    );
    assert!(
        earth_atmosphere.breathable,
        "Earth atmosphere should be breathable"
    );
    assert_eq!(earth_atmosphere.surface_pressure_mbar, 1013.0);

    // Test Mars with body data
    let mars_atmosphere = AtmosphereComposition::new_with_body_data(
        6.0,
        -63.0,
        vec![
            AtmosphericGas::new("CO2", 95.0),
            AtmosphericGas::new("N2", 2.7),
        ],
        6.4171e23, // Mars mass
        3389.5,    // Mars radius
        false,     // Surface pressure, not reference
    );

    assert!(
        mars_atmosphere.can_support_atmosphere,
        "Mars should support atmosphere"
    );
    assert!(
        !mars_atmosphere.breathable,
        "Mars atmosphere should not be breathable"
    );
    assert!(
        !mars_atmosphere.is_reference_pressure,
        "Mars has surface pressure"
    );
}

#[test]
fn test_gas_giant_reference_pressure() {
    // Test Jupiter with reference pressure flag
    let jupiter_atmosphere = AtmosphereComposition::new_with_body_data(
        1000.0, // 1 bar reference level
        -108.0,
        vec![
            AtmosphericGas::new("H2", 90.0),
            AtmosphericGas::new("He", 10.0),
        ],
        1.8982e27, // Jupiter mass
        69911.0,   // Jupiter radius
        true,      // Reference pressure, not surface
    );

    assert!(
        jupiter_atmosphere.can_support_atmosphere,
        "Jupiter should support atmosphere"
    );
    assert!(
        !jupiter_atmosphere.breathable,
        "Jupiter atmosphere should not be breathable"
    );
    assert!(
        jupiter_atmosphere.is_reference_pressure,
        "Jupiter uses reference pressure at 1 bar level"
    );
    assert_eq!(jupiter_atmosphere.surface_pressure_mbar, 1000.0);
}

#[test]
fn test_terrestrial_surface_pressure() {
    // Test Earth with surface pressure (not reference)
    let earth_atmosphere = AtmosphereComposition::new_with_body_data(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
            AtmosphericGas::new("Ar", 0.93),
        ],
        5.97237e24, // Earth mass
        6371.0,     // Earth radius
        false,      // Surface pressure, not reference
    );

    assert!(
        earth_atmosphere.can_support_atmosphere,
        "Earth should support atmosphere"
    );
    assert!(
        earth_atmosphere.breathable,
        "Earth atmosphere should be breathable"
    );
    assert!(
        !earth_atmosphere.is_reference_pressure,
        "Earth has actual surface pressure"
    );
}

#[test]
fn test_harvest_altitude_gas_giant() {
    // Test Jupiter with harvest altitude
    let jupiter = AtmosphereComposition::new_with_body_data(
        1000.0, // 1 bar reference
        -108.0,
        vec![
            AtmosphericGas::new("H2", 90.0),
            AtmosphericGas::new("He", 10.0),
        ],
        1.8982e27, // Jupiter mass
        69911.0,   // Jupiter radius
        true,      // Gas giant: reference pressure
    );

    // Should have default harvest altitude of 10 bar
    assert_eq!(jupiter.harvest_altitude_bar, 10.0);
    assert_eq!(jupiter.max_harvest_altitude_bar, 50.0);

    // Yield multiplier should be ~10x at 10 bar (vs 1 bar reference)
    let yield_mult = jupiter.harvest_yield_multiplier();
    assert!(
        (yield_mult - 10.0).abs() < 0.1,
        "Yield at 10 bar should be ~10x, got {}",
        yield_mult
    );

    // Should be able to increase harvest altitude
    assert!(jupiter.can_increase_harvest_altitude());
    assert_eq!(jupiter.remaining_harvest_capacity_bar(), 40.0); // 50 - 10 = 40
}

#[test]
fn test_harvest_altitude_terrestrial() {
    // Test Earth - should have no harvest altitude (not a gas giant)
    let earth = AtmosphereComposition::new_with_body_data(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
        ],
        5.97237e24, // Earth mass
        6371.0,     // Earth radius
        false,      // Terrestrial: surface pressure
    );

    // Terrestrial planets have no harvest altitude
    assert_eq!(earth.harvest_altitude_bar, 0.0);
    assert_eq!(earth.max_harvest_altitude_bar, 0.0);

    // Harvest yield should be 0 for terrestrial
    assert_eq!(earth.harvest_yield_multiplier(), 0.0);
    assert!(!earth.can_increase_harvest_altitude());
    assert_eq!(earth.remaining_harvest_capacity_bar(), 0.0);
}

#[test]
fn test_harvest_yield_scaling() {
    // Test that harvest yield scales linearly with pressure
    let mut atmosphere = AtmosphereComposition::new_with_body_data(
        1000.0, // 1 bar reference
        -150.0,
        vec![AtmosphericGas::new("H2", 100.0)],
        1.0e27,  // Large mass
        50000.0, // Large radius
        true,    // Gas giant
    );

    // Manually set different harvest altitudes to test scaling
    atmosphere.harvest_altitude_bar = 1.0;
    assert!((atmosphere.harvest_yield_multiplier() - 1.0).abs() < 0.01);

    atmosphere.harvest_altitude_bar = 25.0;
    assert!((atmosphere.harvest_yield_multiplier() - 25.0).abs() < 0.01);

    atmosphere.harvest_altitude_bar = 100.0;
    assert!((atmosphere.harvest_yield_multiplier() - 100.0).abs() < 0.01);
}

// ── Atmospheric Scattering Tests ───────────────────────────────────────────

#[test]
fn test_mean_molecular_weight_earth() {
    let earth = AtmosphereComposition::new(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
            AtmosphericGas::new("Ar", 0.93),
            AtmosphericGas::new("CO2", 0.04),
        ],
    );
    let mmw = earth.mean_molecular_weight();
    // Earth's mean molecular weight is ~28.97 g/mol
    assert!(
        (mmw - 28.97).abs() < 1.0,
        "Earth mmw should be ~28.97, got {mmw}"
    );
}

#[test]
fn test_mean_molecular_weight_hydrogen() {
    let h2_atmo = AtmosphereComposition::new(
        1000.0,
        -145.0,
        vec![
            AtmosphericGas::new("H2", 90.0),
            AtmosphericGas::new("He", 10.0),
        ],
    );
    let mmw = h2_atmo.mean_molecular_weight();
    // 90% H2 (2.016) + 10% He (4.003) = ~2.21
    assert!(
        mmw < 3.0 && mmw > 1.5,
        "H2/He mmw should be ~2.2, got {mmw}"
    );
}

#[test]
fn test_derive_scattering_earth() {
    let mut earth = AtmosphereComposition::new_with_body_data(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
            AtmosphericGas::new("Ar", 0.93),
            AtmosphericGas::new("CO2", 0.04),
        ],
        5.97237e24,
        6371.0,
        false,
    );

    earth.derive_scattering_params(1.0, None, None, None, None, None, None, None);

    // Scale height should be close to 8.5 km for Earth
    assert!(
        (earth.scale_height_km - 8.5).abs() < 2.0,
        "Earth scale height should be ~8.5 km, got {}",
        earth.scale_height_km
    );

    // Rayleigh strength should be ~1.0 (Earth reference)
    assert!(
        earth.rayleigh_strength > 0.5 && earth.rayleigh_strength < 2.0,
        "Earth rayleigh_strength should be ~1.0, got {}",
        earth.rayleigh_strength
    );

    // Should have blue-ish Rayleigh tint (blue > red)
    assert!(
        earth.rayleigh_rgb[2] > earth.rayleigh_rgb[0],
        "Earth rayleigh should be blue-dominant"
    );

    // Intensity defaults to 1.0
    assert!(
        (earth.atmosphere_intensity - 1.0).abs() < 0.01,
        "Default intensity should be 1.0"
    );
}

#[test]
fn test_derive_scattering_mars() {
    let mut mars = AtmosphereComposition::new_with_body_data(
        6.0,
        -63.0,
        vec![
            AtmosphericGas::new("CO2", 95.0),
            AtmosphericGas::new("N2", 2.7),
            AtmosphericGas::new("Ar", 1.6),
            AtmosphericGas::new("O2", 0.13),
        ],
        6.4171e23,
        3389.5,
        false,
    );

    // Mars gravity ~0.38 g
    mars.derive_scattering_params(0.38, None, None, None, None, None, None, None);

    // Mars has very thin atmosphere, so rayleigh_strength should be << 1.0
    assert!(
        mars.rayleigh_strength < 0.2,
        "Mars rayleigh_strength should be very low, got {}",
        mars.rayleigh_strength
    );

    // CO2 dominant → warm red-orange tint
    assert!(
        mars.rayleigh_rgb[0] > mars.rayleigh_rgb[2],
        "Mars rayleigh should be red-dominant for CO2 atmosphere"
    );
}

#[test]
fn test_derive_scattering_overrides() {
    let mut earth = AtmosphereComposition::new_with_body_data(
        1013.0,
        15.0,
        vec![
            AtmosphericGas::new("N2", 78.0),
            AtmosphericGas::new("O2", 21.0),
        ],
        5.97237e24,
        6371.0,
        false,
    );

    // Override all parameters
    earth.derive_scattering_params(
        1.0,
        Some(10.0),            // scale height override
        Some((1.0, 0.0, 0.0)), // pure red rayleigh
        Some(5.0),             // strong rayleigh
        Some(0.1),             // mie override
        Some(0.9),             // mie_g override
        Some((0.5, 0.5, 0.5)), // grey haze
        Some(2.0),             // double intensity
    );

    assert!((earth.scale_height_km - 10.0).abs() < 0.01);
    assert!((earth.rayleigh_rgb[0] - 1.0).abs() < 0.01);
    assert!((earth.rayleigh_rgb[1]).abs() < 0.01);
    assert!((earth.rayleigh_strength - 5.0).abs() < 0.01);
    assert!((earth.mie_strength - 0.1).abs() < 0.01);
    assert!((earth.mie_g - 0.9).abs() < 0.01);
    assert!((earth.haze_color[0] - 0.5).abs() < 0.01);
    assert!((earth.atmosphere_intensity - 2.0).abs() < 0.01);
}

#[test]
fn test_derive_scattering_titan_haze() {
    let mut titan = AtmosphereComposition::new_with_body_data(
        1500.0,
        -179.0,
        vec![
            AtmosphericGas::new("N2", 98.4),
            AtmosphericGas::new("CH4", 1.4),
        ],
        1.3452e23,
        2574.73,
        false,
    );

    // Titan gravity ~0.14 g
    titan.derive_scattering_params(0.14, None, None, None, None, None, None, None);

    // Titan has CH4 → higher mie_strength (haze factor = 0.08)
    assert!(
        titan.mie_strength > 0.01,
        "Titan mie_strength should be elevated due to CH4, got {}",
        titan.mie_strength
    );

    // Haze colour should be orange/amber
    assert!(
        titan.haze_color[0] > titan.haze_color[2],
        "Titan haze should be warm-coloured"
    );
}

#[test]
fn test_ron_optional_scattering_fields_deserialize() {
    // Verify that the RON file still loads with the new optional fields
    use helios_ascension::plugins::solar_system_data::SolarSystemData;
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("solar_system.ron should load with new optional scattering fields");

    // Earth should have scattering overrides
    let earth = data.get_body("Earth").expect("Earth should exist");
    let atmo = earth
        .atmosphere
        .as_ref()
        .expect("Earth should have atmosphere");
    assert!(
        atmo.scale_height_km.is_some(),
        "Earth should have scale_height_km override"
    );
    assert!(
        atmo.rayleigh_rgb.is_some(),
        "Earth should have rayleigh_rgb override"
    );

    // Mars should have scattering overrides
    let mars = data.get_body("Mars").expect("Mars should exist");
    let mars_atmo = mars
        .atmosphere
        .as_ref()
        .expect("Mars should have atmosphere");
    assert!(
        mars_atmo.atmosphere_intensity.is_some(),
        "Mars should have atmosphere_intensity override"
    );

    // Moon should have no atmosphere
    let moon = data.get_body("Moon").expect("Moon should exist");
    assert!(moon.atmosphere.is_none(), "Moon should not have atmosphere");

    // Jupiter — atmosphere without overrides should still work
    let jupiter = data.get_body("Jupiter").expect("Jupiter should exist");
    let jup_atmo = jupiter
        .atmosphere
        .as_ref()
        .expect("Jupiter should have atmosphere");
    assert!(
        jup_atmo.scale_height_km.is_none(),
        "Jupiter should have no scale_height override (uses defaults)"
    );
}
