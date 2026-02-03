use helios_ascension::plugins::solar_system_data::{BodyType, SolarSystemData};

#[test]
fn test_solar_system_data_loads() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");
    
    // Should have 377+ bodies now!
    assert!(data.bodies.len() >= 370, "Expected at least 370 bodies, got {}", data.bodies.len());
    
    // Check for specific bodies
    assert!(data.get_body("Sol").is_some(), "Sol should exist");
    assert!(data.get_body("Earth").is_some(), "Earth should exist");
    assert!(data.get_body("Moon").is_some(), "Moon should exist");
    assert!(data.get_body("Jupiter").is_some(), "Jupiter should exist");
    assert!(data.get_body("Pluto").is_some(), "Pluto should exist");
    
    // Verify body types
    let planets = data.get_bodies_by_type(BodyType::Planet);
    assert_eq!(planets.len(), 8, "Should have 8 planets");
    
    let stars = data.get_bodies_by_type(BodyType::Star);
    assert_eq!(stars.len(), 1, "Should have 1 star");
    
    let moons = data.get_bodies_by_type(BodyType::Moon);
    assert!(moons.len() >= 140, "Should have at least 140 moons, got {}", moons.len());
    
    let asteroids = data.get_bodies_by_type(BodyType::Asteroid);
    assert!(asteroids.len() >= 100, "Should have at least 100 asteroids, got {}", asteroids.len());
    
    let dwarf_planets = data.get_bodies_by_type(BodyType::DwarfPlanet);
    assert!(dwarf_planets.len() >= 50, "Should have at least 50 dwarf planets/KBOs, got {}", dwarf_planets.len());
    
    let comets = data.get_bodies_by_type(BodyType::Comet);
    assert!(comets.len() >= 15, "Should have at least 15 comets, got {}", comets.len());
}

#[test]
fn test_solar_system_hierarchy() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");
    
    // Earth should be a child of Sol
    let earth = data.get_body("Earth").expect("Earth should exist");
    assert_eq!(earth.parent.as_ref().map(|s: &String| s.as_str()), Some("Sol"));
    
    // Moon should be a child of Earth
    let moon = data.get_body("Moon").expect("Moon should exist");
    assert_eq!(moon.parent.as_ref().map(|s: &String| s.as_str()), Some("Earth"));
    
    // Jupiter should have multiple moons
    let jupiter_moons = data.get_children("Jupiter");
    assert!(jupiter_moons.len() >= 50, "Jupiter should have at least 50 moons, got {}", jupiter_moons.len());
    
    // Check for specific Jovian moons
    assert!(data.get_body("Io").is_some());
    assert!(data.get_body("Europa").is_some());
    assert!(data.get_body("Ganymede").is_some());
    assert!(data.get_body("Callisto").is_some());
}

#[test]
fn test_orbital_parameters() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");
    
    // Earth's semi-major axis should be approximately 1 AU
    let earth = data.get_body("Earth").expect("Earth should exist");
    let earth_orbit = earth.orbit.as_ref().expect("Earth should have orbit");
    assert!((earth_orbit.semi_major_axis - 1.0).abs() < 0.01, "Earth should be ~1 AU from Sun");
    
    // Earth's orbital period should be approximately 365 days
    assert!((earth_orbit.orbital_period - 365.0).abs() < 1.0, "Earth year should be ~365 days");
    
    // Mars should be farther than Earth
    let mars = data.get_body("Mars").expect("Mars should exist");
    let mars_orbit = mars.orbit.as_ref().expect("Mars should have orbit");
    assert!(mars_orbit.semi_major_axis > earth_orbit.semi_major_axis, "Mars should be farther than Earth");
}

#[test]
fn test_physical_properties() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");
    
    // Sun should be massive
    let sol = data.get_body("Sol").expect("Sol should exist");
    assert!(sol.mass > 1e30, "Sun should be very massive");
    
    // Jupiter should be more massive than Earth
    let jupiter = data.get_body("Jupiter").expect("Jupiter should exist");
    let earth = data.get_body("Earth").expect("Earth should exist");
    assert!(jupiter.mass > earth.mass * 100.0, "Jupiter should be much more massive than Earth");
    
    // Earth should have reasonable radius
    assert!((earth.radius - 6371.0).abs() < 100.0, "Earth radius should be ~6371 km");
}
