use helios_ascension::astronomy::{AtmosphereComposition, AtmosphericGas};

#[test]
fn test_atmosphere_ui_data_available() {
    // Test that atmosphere data can be properly queried for UI display

    // Create a test atmosphere similar to Earth
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

    // Verify atmosphere properties are accessible for UI
    assert_eq!(earth_atmosphere.surface_pressure_mbar, 1013.0);
    assert_eq!(earth_atmosphere.surface_temperature_celsius, 15.0);
    assert!(earth_atmosphere.breathable);
    assert!(earth_atmosphere.calculate_colony_cost(1.0).abs() < 0.01);

    // Verify gas composition can be iterated
    assert_eq!(earth_atmosphere.gases.len(), 4);

    // Verify pressure conversion for UI display
    let pressure_bar = earth_atmosphere.surface_pressure_mbar / 1000.0;
    assert!((pressure_bar - 1.013).abs() < 0.01);
}

#[test]
fn test_atmosphere_ui_formatting() {
    // Test that different atmospheres format correctly for UI

    // Venus - high pressure
    let venus = AtmosphereComposition::new(
        92000.0,
        465.0,
        vec![
            AtmosphericGas::new("CO2", 96.5),
            AtmosphericGas::new("N2", 3.5),
        ],
    );

    let pressure_bar = venus.surface_pressure_mbar / 1000.0;
    assert!(pressure_bar >= 1.0); // Should display as "bar"
    assert!(venus.calculate_colony_cost(0.904) > 20.0); // Venus should have high cost (2.0 Base + 42.5 Temp + 44 Pressure)

    // Mars - low pressure
    let mars = AtmosphereComposition::new(
        6.0,
        -63.0,
        vec![
            AtmosphericGas::new("CO2", 95.0),
            AtmosphericGas::new("N2", 2.7),
        ],
    );

    let pressure_bar = mars.surface_pressure_mbar / 1000.0;
    assert!(pressure_bar < 1.0); // Should display as "mbar"
    assert!(mars.calculate_colony_cost(0.379) > 2.0); // Mars cost: 2.0 Base + Temp cost
}

#[test]
fn test_colony_cost_colors() {
    // Test that colony costs map to correct color categories

    let test_atmospheres = vec![
        // Good (0-3)
        AtmosphereComposition::new(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
            ],
        ),
        // Moderate (4-6)
        AtmosphereComposition::new(500.0, -30.0, vec![AtmosphericGas::new("N2", 95.0)]),
        // Bad (7-8)
        AtmosphereComposition::new(92000.0, 465.0, vec![AtmosphericGas::new("CO2", 96.5)]),
    ];

    let costs: Vec<f32> = test_atmospheres
        .iter()
        .map(|a| a.calculate_colony_cost(1.0))
        .collect();

    // Verify we have a range of costs
    assert!(costs[0] <= 0.01); // Earth-like
    assert!(costs[1] > 2.0); // Moderate
    assert!(costs[2] > 20.0); // Extreme
}

#[test]
fn test_gas_composition_display() {
    // Test that gas composition is properly formatted for display

    let atmosphere = AtmosphereComposition::new(
        1000.0,
        0.0,
        vec![
            AtmosphericGas::new("H2", 90.0),
            AtmosphericGas::new("He", 10.0),
        ],
    );

    // Verify gases can be accessed for display
    assert_eq!(atmosphere.gases.len(), 2);

    // Verify gas percentages sum to 100
    let total: f32 = atmosphere.gases.iter().map(|g| g.percentage).sum();
    assert!((total - 100.0).abs() < 0.1);

    // Verify individual gas properties are accessible
    for gas in &atmosphere.gases {
        assert!(!gas.name.is_empty());
        assert!(gas.percentage >= 0.0 && gas.percentage <= 100.0);
    }
}
