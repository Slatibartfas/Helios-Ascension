use helios_ascension::astronomy::AtmosphereComposition;
use helios_ascension::plugins::solar_system_data::SolarSystemData;

#[test]
fn test_atmosphere_data_loading() {
    // Load the solar system data
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    // Earth should have atmosphere data
    let earth = data.get_body("Earth").expect("Earth not found");
    assert!(earth.atmosphere.is_some(), "Earth should have atmosphere data");
    
    let earth_atmo = earth.atmosphere.as_ref().unwrap();
    assert_eq!(earth_atmo.surface_pressure_mbar, 1013.0);
    assert_eq!(earth_atmo.surface_temperature_celsius, 15.0);
    assert!(earth_atmo.gases.iter().any(|g| g.name == "N2"));
    assert!(earth_atmo.gases.iter().any(|g| g.name == "O2"));

    // Mars should have atmosphere data
    let mars = data.get_body("Mars").expect("Mars not found");
    assert!(mars.atmosphere.is_some(), "Mars should have atmosphere data");
    
    let mars_atmo = mars.atmosphere.as_ref().unwrap();
    assert_eq!(mars_atmo.surface_pressure_mbar, 6.0);
    assert!(mars_atmo.gases.iter().any(|g| g.name == "CO2"));

    // Venus should have atmosphere data
    let venus = data.get_body("Venus").expect("Venus not found");
    assert!(venus.atmosphere.is_some(), "Venus should have atmosphere data");
    
    let venus_atmo = venus.atmosphere.as_ref().unwrap();
    assert_eq!(venus_atmo.surface_pressure_mbar, 92000.0);
    
    // Jupiter should have atmosphere data
    let jupiter = data.get_body("Jupiter").expect("Jupiter not found");
    assert!(jupiter.atmosphere.is_some(), "Jupiter should have atmosphere data");
    
    // Titan should have atmosphere data
    let titan = data.get_body("Titan").expect("Titan not found");
    assert!(titan.atmosphere.is_some(), "Titan should have atmosphere data");
    
    // Mercury should NOT have atmosphere data (no significant atmosphere)
    let mercury = data.get_body("Mercury").expect("Mercury not found");
    assert!(mercury.atmosphere.is_none(), "Mercury should not have atmosphere data");
    
    // Moon should NOT have atmosphere data
    let moon = data.get_body("Moon").expect("Moon not found");
    assert!(moon.atmosphere.is_none(), "Moon should not have atmosphere data");
}

#[test]
fn test_atmosphere_breathability_check() {
    // Create atmospheres from loaded data and check breathability calculation
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    // Earth should be breathable
    let earth = data.get_body("Earth").unwrap();
    if let Some(earth_atmo_data) = &earth.atmosphere {
        use helios_ascension::astronomy::AtmosphericGas;
        
        let gases: Vec<AtmosphericGas> = earth_atmo_data
            .gases
            .iter()
            .map(|g| AtmosphericGas::new(&g.name, g.percentage))
            .collect();
        
        let atmosphere = AtmosphereComposition::new(
            earth_atmo_data.surface_pressure_mbar,
            earth_atmo_data.surface_temperature_celsius,
            gases,
        );
        
        assert!(atmosphere.breathable, "Earth should be breathable");
        assert_eq!(atmosphere.calculate_colony_cost(), 0, "Earth should have colony cost of 0");
    }

    // Mars should not be breathable
    let mars = data.get_body("Mars").unwrap();
    if let Some(mars_atmo_data) = &mars.atmosphere {
        use helios_ascension::astronomy::AtmosphericGas;
        
        let gases: Vec<AtmosphericGas> = mars_atmo_data
            .gases
            .iter()
            .map(|g| AtmosphericGas::new(&g.name, g.percentage))
            .collect();
        
        let atmosphere = AtmosphereComposition::new(
            mars_atmo_data.surface_pressure_mbar,
            mars_atmo_data.surface_temperature_celsius,
            gases,
        );
        
        assert!(!atmosphere.breathable, "Mars should not be breathable");
        assert!(atmosphere.calculate_colony_cost() >= 5, "Mars should have high colony cost");
    }
}
