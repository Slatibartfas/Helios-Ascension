use helios_ascension::plugins::solar_system_data::{AsteroidClass, BodyType, SolarSystemData};

#[test]
fn test_texture_field_deserializes() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    // Check that Sol has a texture
    let sol = data.get_body("Sol").expect("Sol should exist");
    assert!(
        sol.texture.is_some(),
        "Sol should have a dedicated texture"
    );
    assert_eq!(
        sol.texture.as_ref().unwrap(),
        "textures/celestial/stars/sun_8k.jpg"
    );

    // Check that Earth has a texture
    let earth = data.get_body("Earth").expect("Earth should exist");
    assert!(
        earth.texture.is_some(),
        "Earth should have a dedicated texture"
    );
    assert_eq!(
        earth.texture.as_ref().unwrap(),
        "textures/celestial/planets/earth_8k.jpg"
    );

    // Check that Moon has a texture
    let moon = data.get_body("Moon").expect("Moon should exist");
    assert!(
        moon.texture.is_some(),
        "Moon should have a dedicated texture"
    );
    assert_eq!(
        moon.texture.as_ref().unwrap(),
        "textures/celestial/moons/moon_8k.jpg"
    );

    // Check Venus uses surface texture
    let venus = data.get_body("Venus").expect("Venus should exist");
    assert!(
        venus.texture.is_some(),
        "Venus should have a dedicated texture"
    );
    assert_eq!(
        venus.texture.as_ref().unwrap(),
        "textures/celestial/planets/venus_surface_8k.jpg",
        "Venus should use surface texture"
    );
}

#[test]
fn test_asteroid_classification_deserializes() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    // Check that Vesta has a dedicated texture and asteroid class
    let vesta = data.get_body("Vesta").expect("Vesta should exist");
    assert_eq!(vesta.body_type, BodyType::Asteroid);
    assert!(
        vesta.texture.is_some(),
        "Vesta should have a dedicated texture"
    );
    assert_eq!(
        vesta.texture.as_ref().unwrap(),
        "textures/celestial/asteroids/vesta_2k.jpg"
    );

    // Ceres should have asteroid classification
    let ceres = data.get_body("Ceres").expect("Ceres should exist");
    assert!(
        ceres.asteroid_class.is_some(),
        "Ceres should have asteroid classification"
    );
    assert_eq!(
        ceres.asteroid_class.as_ref().unwrap(),
        &AsteroidClass::CType,
        "Ceres should be C-type"
    );
}

#[test]
fn test_generic_texture_selection_logic() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    // Count bodies with dedicated textures
    let mut dedicated_count = 0;
    let mut generic_asteroid_count = 0;
    let mut generic_comet_count = 0;
    let mut generic_moon_count = 0;

    for body in &data.bodies {
        if body.texture.is_some() {
            dedicated_count += 1;
        } else {
            // These would get generic textures
            match body.body_type {
                BodyType::Asteroid => generic_asteroid_count += 1,
                BodyType::Comet => generic_comet_count += 1,
                BodyType::Moon => generic_moon_count += 1,
                _ => {}
            }
        }
    }

    // Verify we have the expected counts
    assert!(
        dedicated_count >= 25,
        "Should have at least 25 bodies with dedicated textures, got {}",
        dedicated_count
    );

    assert!(
        generic_asteroid_count >= 100,
        "Should have at least 100 asteroids using generic textures, got {}",
        generic_asteroid_count
    );

    assert!(
        generic_comet_count >= 15,
        "Should have at least 15 comets using generic textures, got {}",
        generic_comet_count
    );

    assert!(
        generic_moon_count >= 100,
        "Should have at least 100 moons using generic textures, got {}",
        generic_moon_count
    );

    println!("Texture coverage:");
    println!("  Dedicated: {}", dedicated_count);
    println!("  Generic asteroids: {}", generic_asteroid_count);
    println!("  Generic comets: {}", generic_comet_count);
    println!("  Generic moons: {}", generic_moon_count);
    println!(
        "  Total: {}",
        dedicated_count + generic_asteroid_count + generic_comet_count + generic_moon_count
    );
}

#[test]
fn test_asteroid_class_distribution() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    let asteroids = data.get_bodies_by_type(BodyType::Asteroid);

    let mut c_type_count = 0;
    let mut s_type_count = 0;
    let mut m_type_count = 0;
    let mut unknown_count = 0;

    for asteroid in asteroids {
        match &asteroid.asteroid_class {
            Some(AsteroidClass::CType) => c_type_count += 1,
            Some(AsteroidClass::SType) => s_type_count += 1,
            Some(AsteroidClass::MType) => m_type_count += 1,
            Some(AsteroidClass::Unknown) | None => unknown_count += 1,
        }
    }

    // C-type should be the most common
    assert!(
        c_type_count > s_type_count,
        "C-type asteroids should be most common"
    );

    println!("Asteroid classification distribution:");
    println!("  C-type: {}", c_type_count);
    println!("  S-type: {}", s_type_count);
    println!("  M-type: {}", m_type_count);
    println!("  Unknown: {}", unknown_count);
}

#[test]
fn test_no_bodies_missing_required_fields() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    for body in &data.bodies {
        // All bodies should have a name
        assert!(!body.name.is_empty(), "Body should have a name");

        // All bodies should have mass > 0
        assert!(body.mass > 0.0, "Body {} should have mass > 0", body.name);

        // All bodies should have radius > 0
        assert!(
            body.radius > 0.0,
            "Body {} should have radius > 0",
            body.name
        );

        // All non-star bodies should have a parent
        if body.body_type != BodyType::Star {
            assert!(
                body.parent.is_some(),
                "Non-star body {} should have a parent",
                body.name
            );
        }

        // All non-star bodies should have orbital parameters
        if body.body_type != BodyType::Star {
            assert!(
                body.orbit.is_some(),
                "Non-star body {} should have orbital parameters",
                body.name
            );
        }
    }
}

#[test]
fn test_major_moons_have_textures() {
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
        .expect("Failed to load solar system data");

    // Major moons that should have dedicated textures
    let major_moons = [
        "Moon",       // Earth's moon
        "Io",         // Jupiter
        "Europa",     // Jupiter
        "Ganymede",   // Jupiter
        "Callisto",   // Jupiter
        "Titan",      // Saturn
        "Enceladus",  // Saturn
        "Phobos",     // Mars
        "Deimos",     // Mars
        "Triton",     // Neptune
        "Miranda",    // Uranus
    ];

    for moon_name in &major_moons {
        let moon = data
            .get_body(moon_name)
            .unwrap_or_else(|| panic!("{} should exist", moon_name));
        assert_eq!(
            moon.body_type,
            BodyType::Moon,
            "{} should be a moon",
            moon_name
        );
        assert!(
            moon.texture.is_some(),
            "{} should have a dedicated texture",
            moon_name
        );
    }
}
