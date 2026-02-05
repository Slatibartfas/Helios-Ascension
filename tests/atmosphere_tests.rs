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
