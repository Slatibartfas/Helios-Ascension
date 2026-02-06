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

    assert!(earth_atmosphere.breathable, "Earth atmosphere should be breathable");
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

    assert!(!mars_atmosphere.breathable, "Mars atmosphere should not be breathable");
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

    assert!(!venus_atmosphere.breathable, "Venus atmosphere should not be breathable");
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

    assert!(!jupiter_atmosphere.breathable, "Jupiter atmosphere should not be breathable");
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

    assert!(!titan_atmosphere.breathable, "Titan atmosphere should not be breathable (no O2)");
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

    let cost = earth_atmosphere.calculate_colony_cost();
    assert_eq!(cost, 0, "Earth should have colony cost of 0");
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

    let cost = mars_atmosphere.calculate_colony_cost();
    // Mars should have high colony cost due to:
    // - Low pressure (< 0.01 bar): +3
    // - Extreme temperature (> 50°C from ideal): +2
    // - Not breathable: +2
    // Total: 7
    assert!(cost >= 5, "Mars should have high colony cost (got {})", cost);
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

    let cost = venus_atmosphere.calculate_colony_cost();
    // Venus should have maximum colony cost due to:
    // - Extreme pressure (> 10 bar): +3
    // - Extreme temperature (> 100°C from ideal): +3
    // - Not breathable: +2
    // Total: 8 (capped at 8)
    assert_eq!(cost, 8, "Venus should have maximum colony cost");
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
    let earth_escape_velocity = AtmosphereComposition::calculate_escape_velocity(earth_mass, earth_radius);
    
    // Should be close to 11.2 km/s (within 0.1 km/s)
    assert!((earth_escape_velocity - 11.2).abs() < 0.1, 
            "Earth escape velocity should be ~11.2 km/s, got {}", earth_escape_velocity);
    
    // Test Moon's escape velocity (should be ~2.4 km/s)
    let moon_mass = 7.342e22; // kg
    let moon_radius = 1737.4; // km
    let moon_escape_velocity = AtmosphereComposition::calculate_escape_velocity(moon_mass, moon_radius);
    
    // Should be close to 2.4 km/s
    assert!((moon_escape_velocity - 2.4).abs() < 0.1,
            "Moon escape velocity should be ~2.4 km/s, got {}", moon_escape_velocity);
    
    // Test Jupiter's escape velocity (should be ~60 km/s)
    let jupiter_mass = 1.8982e27; // kg
    let jupiter_radius = 69911.0; // km
    let jupiter_escape_velocity = AtmosphereComposition::calculate_escape_velocity(jupiter_mass, jupiter_radius);
    
    // Should be close to 60 km/s (within 1 km/s)
    assert!((jupiter_escape_velocity - 60.0).abs() < 1.0,
            "Jupiter escape velocity should be ~60 km/s, got {}", jupiter_escape_velocity);
}

#[test]
fn test_atmosphere_retention() {
    // Earth should retain atmosphere (escape velocity ~11.2 km/s)
    let earth_mass = 5.97237e24; // kg
    let earth_radius = 6371.0; // km
    assert!(AtmosphereComposition::can_retain_atmosphere(earth_mass, earth_radius),
            "Earth should be able to retain an atmosphere");
    
    // Mars should retain atmosphere (escape velocity ~5.0 km/s)
    let mars_mass = 6.4171e23; // kg
    let mars_radius = 3389.5; // km
    assert!(AtmosphereComposition::can_retain_atmosphere(mars_mass, mars_radius),
            "Mars should be able to retain an atmosphere");
    
    // Moon is at the retention threshold (escape velocity ~2.4 km/s)
    let moon_mass = 7.342e22; // kg
    let moon_radius = 1737.4; // km
    assert!(AtmosphereComposition::can_retain_atmosphere(moon_mass, moon_radius),
            "Moon is at boundary: can retain heavy gases but threshold is ≥ 2.0 km/s");
    
    // Titan should retain atmosphere (escape velocity ~2.6 km/s, denser than Moon)
    let titan_mass = 1.3452e23; // kg
    let titan_radius = 2574.73; // km
    assert!(AtmosphereComposition::can_retain_atmosphere(titan_mass, titan_radius),
            "Titan should be able to retain an atmosphere");
    
    // Very small asteroid should NOT retain atmosphere
    let small_asteroid_mass = 1.0e15; // kg (very small)
    let small_asteroid_radius = 1.0; // km
    assert!(!AtmosphereComposition::can_retain_atmosphere(small_asteroid_mass, small_asteroid_radius),
            "Small asteroid should not be able to retain an atmosphere");
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
    );
    
    assert!(earth_atmosphere.can_support_atmosphere, "Earth should support atmosphere");
    assert!(earth_atmosphere.breathable, "Earth atmosphere should be breathable");
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
    );
    
    assert!(mars_atmosphere.can_support_atmosphere, "Mars should support atmosphere");
    assert!(!mars_atmosphere.breathable, "Mars atmosphere should not be breathable");
}
