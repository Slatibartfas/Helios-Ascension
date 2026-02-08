# Procedural Generation Usage Examples

This file demonstrates how to use the procedural generation system in Helios: Ascension.

## Basic Usage

The `SystemPopulatorPlugin` runs automatically at startup and populates nearby star systems. No manual intervention is needed for basic functionality.

## Example 1: Generating a System from Scratch

```rust
use helios_ascension::astronomy::{
    map_star_to_system_architecture,
    calculate_frost_line,
};
use rand::thread_rng;

fn generate_custom_system() {
    let mut rng = thread_rng();
    
    // Generate a system for a Sun-like star with no existing planets
    let architecture = map_star_to_system_architecture(
        "Custom Star",     // Star name
        1.0,               // Solar luminosity
        0,                 // No existing planets
        &[],               // No existing orbits to avoid
        &mut rng,
    );
    
    println!("Generated system with:");
    println!("  - {} rocky planets", architecture.rocky_planets.len());
    println!("  - {} gas giants", architecture.gas_giants.len());
    println!("  - Frost line at {:.2} AU", architecture.frost_line_au);
    
    if let Some(belt) = &architecture.asteroid_belt {
        println!("  - Asteroid belt: {:.2}-{:.2} AU ({} asteroids)",
                 belt.inner_au, belt.outer_au, belt.count);
    }
    
    if let Some(cloud) = &architecture.cometary_cloud {
        println!("  - Cometary cloud: {:.2}-{:.2} AU ({} comets)",
                 cloud.inner_au, cloud.outer_au, cloud.count);
    }
}
```

## Example 2: Generating a System with Existing Planets

```rust
use helios_ascension::astronomy::map_star_to_system_architecture;
use rand::thread_rng;

fn generate_partial_system() {
    let mut rng = thread_rng();
    
    // Proxima Centauri has 2 confirmed planets at specific orbits
    let existing_orbits = vec![0.0289, 0.0485]; // Proxima d and b
    
    let architecture = map_star_to_system_architecture(
        "Proxima Centauri",
        0.0017,            // Very low luminosity
        2,                 // 2 existing planets
        &existing_orbits,  // Their orbits
        &mut rng,
    );
    
    // The system will add 2-3 more planets to reach target of 5
    // while avoiding collisions with existing planets
}
```

## Example 3: Spawning Procedural Planets in Bevy

```rust
use bevy::prelude::*;
use helios_ascension::astronomy::{
    map_star_to_system_architecture,
    KeplerOrbit,
    SpaceCoordinates,
};
use helios_ascension::plugins::solar_system::{CelestialBody, Planet};
use helios_ascension::plugins::system_populator::spawn_procedural_planet;

fn spawn_system(
    mut commands: Commands,
    star_entity: Entity,
) {
    let mut rng = rand::thread_rng();
    
    // Generate system architecture
    let architecture = map_star_to_system_architecture(
        "Alpha Centauri A",
        1.519,  // Luminosity
        0,      // No existing planets
        &[],
        &mut rng,
    );
    
    // Spawn each rocky planet
    for planet in &architecture.rocky_planets {
        spawn_procedural_planet(
            &mut commands,
            planet,
            star_entity,
            1,    // System ID
            1.0,  // Metallicity multiplier (solar)
        );
    }
    
    // Spawn each gas giant
    for planet in &architecture.gas_giants {
        spawn_procedural_planet(
            &mut commands,
            planet,
            star_entity,
            1,
            1.0,
        );
    }
}
```

## Example 4: Using Metallicity Bonuses

```rust
use helios_ascension::economy::components::StarSystem;
use helios_ascension::economy::components::SpectralClass;

fn demonstrate_metallicity() {
    // Create stars with different metallicities
    
    // Metal-poor old star
    let metal_poor = StarSystem::with_metallicity(
        4.85,              // Frost line
        SpectralClass::G,
        -0.5,              // [Fe/H] = -0.5 (metal-poor)
    );
    println!("Metal-poor star multiplier: {:.2}x", 
             metal_poor.metallicity_multiplier());
    // Output: 0.70x (less rare metals/fissiles)
    
    // Solar metallicity star
    let solar = StarSystem::with_metallicity(
        4.85,
        SpectralClass::G,
        0.0,               // [Fe/H] = 0.0 (solar)
    );
    println!("Solar metallicity multiplier: {:.2}x",
             solar.metallicity_multiplier());
    // Output: 1.00x (standard abundance)
    
    // Metal-rich young star
    let metal_rich = StarSystem::with_metallicity(
        4.85,
        SpectralClass::G,
        +0.5,              // [Fe/H] = +0.5 (metal-rich)
    );
    println!("Metal-rich star multiplier: {:.2}x",
             metal_rich.metallicity_multiplier());
    // Output: 1.30x (more rare metals/fissiles)
}
```

## Example 5: Calculating Frost Lines for Different Stars

```rust
use helios_ascension::astronomy::calculate_frost_line;

fn frost_line_examples() {
    // M-dwarf (red dwarf) - Proxima Centauri
    let proxima = calculate_frost_line(0.0017);
    println!("Proxima (M5.5Ve): {:.2} AU", proxima);
    // Output: 0.20 AU (very close to star)
    
    // K-dwarf (orange dwarf) - Epsilon Eridani
    let epsilon_eri = calculate_frost_line(0.34);
    println!("Epsilon Eridani (K2V): {:.2} AU", epsilon_eri);
    // Output: 2.83 AU
    
    // G-dwarf (yellow dwarf) - Sun
    let sun = calculate_frost_line(1.0);
    println!("Sun (G2V): {:.2} AU", sun);
    // Output: 4.85 AU (actual asteroid belt at ~2.7 AU)
    
    // F-dwarf - Procyon A
    let procyon = calculate_frost_line(7.5);
    println!("Procyon A (F5IV-V): {:.2} AU", procyon);
    // Output: 13.28 AU
    
    // A-dwarf - Sirius A
    let sirius = calculate_frost_line(25.4);
    println!("Sirius A (A1V): {:.2} AU", sirius);
    // Output: 24.43 AU (huge inner system!)
}
```

## Example 6: Converting to KeplerOrbit

```rust
use helios_ascension::astronomy::{ProceduralPlanet, KeplerOrbit};

fn convert_to_kepler(planet: &ProceduralPlanet) -> KeplerOrbit {
    // ProceduralPlanet has a convenience method
    let kepler = planet.to_kepler_orbit();
    
    println!("Orbit parameters:");
    println!("  Semi-major axis: {:.3} AU", kepler.semi_major_axis);
    println!("  Eccentricity: {:.3}", kepler.eccentricity);
    println!("  Inclination: {:.3}°", kepler.inclination.to_degrees());
    println!("  Mean motion: {:.3e} rad/s", kepler.mean_motion);
    
    let period_years = std::f64::consts::TAU / kepler.mean_motion / 365.25 / 86400.0;
    println!("  Orbital period: {:.2} years", period_years);
    
    kepler
}
```

## Example 7: Querying Generated Resources

```rust
use bevy::prelude::*;
use helios_ascension::economy::components::PlanetResources;
use helios_ascension::economy::types::ResourceType;

fn query_planet_resources(
    planet_query: Query<(&Name, &PlanetResources)>,
) {
    for (name, resources) in planet_query.iter() {
        println!("Planet: {}", name);
        
        // Check for rare metals (affected by metallicity)
        if let Some(gold) = resources.get_deposit(&ResourceType::Gold) {
            println!("  Gold: {:.2} Mt (proven)", gold.reserve.proven_crustal);
            println!("        {:.2} Mt (deep)", gold.reserve.deep_deposits);
            println!("        {:.2e} Mt (bulk)", gold.reserve.planetary_bulk);
        }
        
        // Check for volatiles (frost line dependent)
        if let Some(water) = resources.get_deposit(&ResourceType::Water) {
            println!("  Water: {:.2e} Mt total", water.total_megatons());
        }
        
        // List all viable deposits
        let viable_count = resources.viable_deposits().count();
        println!("  Total viable deposits: {}", viable_count);
    }
}
```

## Example 8: Deterministic Generation

```rust
use rand::SeedableRng;
use rand::rngs::StdRng;
use helios_ascension::astronomy::map_star_to_system_architecture;

fn deterministic_generation() {
    // Use a fixed seed for reproducible results
    let seed = 42;
    let mut rng1 = StdRng::seed_from_u64(seed);
    
    let arch1 = map_star_to_system_architecture(
        "Test Star", 1.0, 0, &[], &mut rng1
    );
    
    // Same seed = same results
    let mut rng2 = StdRng::seed_from_u64(seed);
    let arch2 = map_star_to_system_architecture(
        "Test Star", 1.0, 0, &[], &mut rng2
    );
    
    assert_eq!(arch1.rocky_planets.len(), arch2.rocky_planets.len());
    assert_eq!(arch1.gas_giants.len(), arch2.gas_giants.len());
    
    // Useful for testing, debugging, and procedural galaxy generation
}
```

## Example 9: Creating a Multi-Star System

```rust
use bevy::prelude::*;
use helios_ascension::economy::components::{StarSystem, SpectralClass};
use helios_ascension::astronomy::calculate_frost_line;

fn setup_binary_system(mut commands: Commands) {
    // Alpha Centauri A (primary)
    let frost_a = calculate_frost_line(1.519);
    let star_a_system = StarSystem::with_metallicity(
        frost_a,
        SpectralClass::G,
        0.2,  // Slightly metal-rich
    );
    
    let star_a = commands.spawn((
        StarSystem(star_a_system),
        Name::new("Alpha Centauri A"),
        // ... other components
    )).id();
    
    // Alpha Centauri B (secondary)
    let frost_b = calculate_frost_line(0.5);
    let star_b_system = StarSystem::with_metallicity(
        frost_b,
        SpectralClass::K,
        0.2,  // Same metallicity as primary
    );
    
    let star_b = commands.spawn((
        StarSystem(star_b_system),
        Name::new("Alpha Centauri B"),
        // ... other components
    )).id();
    
    // Each star would get its own planetary system
    // generated independently
}
```

## Example 10: Custom Planet Type Detection

```rust
use helios_ascension::astronomy::{ProceduralPlanet, PlanetType};

fn classify_planet(planet: &ProceduralPlanet) {
    match planet.planet_type {
        PlanetType::Rocky => {
            if planet.mass_earth < 0.5 {
                println!("Sub-Earth (Mars-like)");
            } else if planet.mass_earth < 2.0 {
                println!("Terrestrial (Earth/Venus-like)");
            } else {
                println!("Super-Earth");
            }
        }
        PlanetType::IceGiant => {
            println!("Ice Giant (Neptune/Uranus-like)");
        }
        PlanetType::GasGiant => {
            if planet.mass_earth < 100.0 {
                println!("Mini-Gas Giant (Saturn-like)");
            } else {
                println!("Gas Giant (Jupiter-like)");
            }
        }
    }
    
    // Calculate surface gravity
    let radius_m = (planet.radius_km() as f64) * 1000.0;
    let mass_kg = planet.mass_kg();
    let gravity = 6.674e-11 * mass_kg / (radius_m * radius_m);
    let gravity_g = gravity / 9.81; // In Earth gravities
    println!("Surface gravity: {:.2}g", gravity_g);
}
```

## Tips and Best Practices

1. **Always use a seeded RNG** for procedural generation to ensure reproducibility
2. **Check existing orbits** before generating new planets to avoid collisions
3. **Respect the frost line** when placing resources and planet types
4. **Apply metallicity bonuses** after base resource generation
5. **Use high-precision (f64) orbits** for astronomical accuracy
6. **Test with different stellar types** to ensure variety
7. **Log generation details** for debugging and verification
8. **Cache generated systems** to avoid regenerating on every load
9. **Consider stellar age** when fine-tuning debris field populations
10. **Validate orbital stability** for close-in planets in binary systems

## See Also

- `docs/PROCEDURAL_GENERATION.md` - Complete system documentation
- `tests/procedural_generation_tests.rs` - Comprehensive test examples
- `src/astronomy/procedural.rs` - Implementation details
- `src/plugins/system_populator.rs` - Integration code
