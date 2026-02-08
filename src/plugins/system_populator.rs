//! System Populator Plugin
//!
//! This plugin handles procedural generation of star systems by:
//! 1. Loading confirmed exoplanet data from nearby stars
//! 2. Filling in missing planets/bodies using procedural generation
//! 3. Spawning asteroid belts and cometary clouds
//! 4. Applying resource generation with metallicity bonuses

use bevy::math::DVec3;
use bevy::prelude::*;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::astronomy::components::{CurrentStarSystem, OrbitCenter, SystemId};
use crate::astronomy::exoplanets::RealPlanet;
use crate::astronomy::nearby_stars::{NearbyStarsData, PlanetData, StarData};
use crate::astronomy::{
    calculate_frost_line, map_star_to_system_architecture, KeplerOrbit, OrbitPath,
    ProceduralPlanet, SpaceCoordinates, SystemArchitecture,
};
use crate::economy::components::{OrbitsBody, PlanetResources, SpectralClass, StarSystem};
use crate::economy::generation::generate_solar_system_resources;
use crate::game_state::GameSeed;
use crate::plugins::solar_system::{
    Asteroid, CelestialBody, Comet, DwarfPlanet, Moon, Planet, Star,
};
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

pub struct SystemPopulatorPlugin;

impl Plugin for SystemPopulatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            populate_nearby_systems.before(generate_solar_system_resources),
        );
    }
}

/// Main system that populates nearby star systems with procedural bodies
/// This runs after the initial solar system is set up and uses the GameSeed
/// for deterministic generation
fn populate_nearby_systems(
    mut commands: Commands,
    stars_data: Res<NearbyStarsData>,
    game_seed: Res<GameSeed>,
    current_system: Res<CurrentStarSystem>,
) {
    // Use game seed for deterministic generation
    let mut rng = StdRng::seed_from_u64(game_seed.value);

    info!(
        "Starting procedural population of nearby star systems with seed {}",
        game_seed.value
    );

    // Start at system ID 1 (Sol is 0)
    let mut system_id = 1;

    for system_data in &stars_data.systems {
        // Skip if this is the Sol system (already populated)
        if system_data.system_name == "Sol" {
            continue;
        }

        info!(
            "Populating system '{}' at {:.2} ly with {} stars",
            system_data.system_name,
            system_data.distance_ly,
            system_data.stars.len()
        );

        // For simplicity, generate systems in a line along the X-axis in "galactic coordinates"
        // Each light year = 63,241 AU, place systems at their actual distances
        let distance_au = (system_data.distance_ly as f64) * 63241.0;

        // Spawn the primary star (first star in the list)
        if let Some(primary_star) = system_data.stars.first() {
            let star_position = DVec3::new(distance_au, 0.0, 0.0);

            // Use real metallicity if available, otherwise generate random
            let metallicity = primary_star.metallicity.unwrap_or_else(|| {
                let random_value = rng.gen_range(-0.5..0.5);
                info!(
                    "  No metallicity data for '{}', using random: {:.2}",
                    primary_star.name, random_value
                );
                random_value
            });

            if primary_star.metallicity.is_some() {
                info!(
                    "  Using real metallicity data for '{}': [Fe/H]={:.2}",
                    primary_star.name, metallicity
                );
            }

            let star_entity = spawn_star_entity_with_metallicity(
                &mut commands,
                primary_star,
                system_id,
                star_position,
                metallicity,
            );

            // Get the star's frost line and metallicity multiplier
            let frost_line = calculate_frost_line(primary_star.luminosity_sol as f64);
            let star_system = StarSystem::with_metallicity(
                frost_line,
                spectral_type_to_class(&primary_star.spectral_type),
                metallicity,
            );
            let metallicity_mult = star_system.metallicity_multiplier();

            // Spawn confirmed planets first
            let mut existing_orbits = Vec::new();
            for planet_data in &primary_star.planets {
                spawn_confirmed_planet(&mut commands, planet_data, star_entity, system_id);
                existing_orbits.push(planet_data.semi_major_axis_au as f64);
            }

            // Generate procedural architecture to fill gaps
            let architecture = map_star_to_system_architecture(
                &system_data.system_name,
                primary_star.luminosity_sol as f64,
                primary_star.planets.len(),
                &existing_orbits,
                &mut rng,
            );

            info!(
                "  Generated {} rocky planets, {} gas giants for '{}'",
                architecture.rocky_planets.len(),
                architecture.gas_giants.len(),
                system_data.system_name
            );

            // Spawn procedural planets
            for planet in &architecture.rocky_planets {
                spawn_procedural_planet(
                    &mut commands,
                    planet,
                    star_entity,
                    system_id,
                    metallicity_mult,
                );
            }

            for planet in &architecture.gas_giants {
                spawn_procedural_planet(
                    &mut commands,
                    planet,
                    star_entity,
                    system_id,
                    metallicity_mult,
                );
            }

            // Spawn asteroid belt if present
            if let Some(belt) = &architecture.asteroid_belt {
                spawn_asteroid_belt(
                    &mut commands,
                    belt,
                    star_entity,
                    system_id,
                    &system_data.system_name,
                    game_seed.value,
                );
            }

            // Spawn cometary cloud if present
            if let Some(cloud) = &architecture.cometary_cloud {
                spawn_cometary_cloud(
                    &mut commands,
                    cloud,
                    star_entity,
                    system_id,
                    &system_data.system_name,
                    game_seed.value,
                );
            }
        }

        system_id += 1;
    }

    info!(
        "Completed procedural population of {} star systems",
        system_id - 1
    );
}

/// Spawn a star entity with its system properties and custom metallicity
pub fn spawn_star_entity_with_metallicity(
    commands: &mut Commands,
    star_data: &StarData,
    system_id: usize,
    position: DVec3,
    metallicity: f32,
) -> Entity {
    let spectral_class = spectral_type_to_class(&star_data.spectral_type);

    // Calculate frost line from luminosity
    let frost_line_au = calculate_frost_line(star_data.luminosity_sol as f64);

    let star_system = StarSystem::with_metallicity(frost_line_au, spectral_class, metallicity);

    info!(
        "Spawning star '{}' ({}): L={:.3}L☉, frost_line={:.2}AU, [Fe/H]={:.2}",
        star_data.name,
        star_data.spectral_type,
        star_data.luminosity_sol,
        frost_line_au,
        metallicity
    );

    let entity = commands
        .spawn((
            Star,
            CelestialBody {
                name: star_data.name.clone(),
                mass: (star_data.mass_sol * 1.989e30) as f64, // Convert to kg
                radius: star_data.radius_sol * 695700.0,      // Convert to km
                body_type: BodyType::Star,
                visual_radius: star_data.radius_sol * 695700.0,
                asteroid_class: None,
            },
            SpaceCoordinates::new(position),
            SystemId(system_id),
            star_system,
        ))
        .id();

    entity
}

/// Spawn a confirmed planet from real exoplanet data
pub fn spawn_confirmed_planet(
    commands: &mut Commands,
    planet_data: &PlanetData,
    parent_star: Entity,
    system_id: usize,
) -> Entity {
    // Calculate orbital parameters
    let period_seconds = (planet_data.period_days as f64) * 86400.0;
    let mean_motion = std::f64::consts::TAU / period_seconds;

    let orbit = KeplerOrbit::new(
        planet_data.eccentricity as f64,
        planet_data.semi_major_axis_au as f64,
        0.0, // Inclination not provided, assume coplanar
        0.0, // Random longitude of ascending node
        0.0, // Random argument of periapsis
        0.0, // Random mean anomaly
        mean_motion,
    );

    // Estimate radius and mass
    let radius_earth = planet_data.radius_earth.unwrap_or(1.0);
    let mass_earth = planet_data.mass_earth;

    // Convert to SI units
    const EARTH_MASS_KG: f64 = 5.972e24;
    const EARTH_RADIUS_KM: f32 = 6371.0;
    let mass_kg = (mass_earth as f64) * EARTH_MASS_KG;
    let radius_km = radius_earth * EARTH_RADIUS_KM;

    info!(
        "Spawning confirmed planet '{}': a={:.2}AU, M={:.1}M⊕, type={}",
        planet_data.name,
        planet_data.semi_major_axis_au,
        planet_data.mass_earth,
        planet_data.planet_type
    );

    let entity = commands
        .spawn((
            Planet,
            RealPlanet, // Mark as confirmed planet
            CelestialBody {
                name: planet_data.name.clone(),
                mass: mass_kg,
                radius: radius_km,
                body_type: BodyType::Planet,
                visual_radius: radius_km,
                asteroid_class: None,
            },
            orbit,
            OrbitPath::new(Color::srgba(0.3, 0.8, 0.3, 0.5)), // Green for confirmed planets
            SpaceCoordinates::default(),                      // Will be updated by propagate_orbits
            OrbitCenter(parent_star), // Link to parent star for orbital hierarchy
            OrbitsBody::new(parent_star),
            SystemId(system_id),
        ))
        .id();

    entity
}

/// Spawn a procedurally generated planet
pub fn spawn_procedural_planet(
    commands: &mut Commands,
    planet: &ProceduralPlanet,
    parent_star: Entity,
    system_id: usize,
    metallicity_multiplier: f32,
) -> Entity {
    let orbit = planet.to_kepler_orbit();
    let mass_kg = planet.mass_kg();
    let radius_km = planet.radius_km();

    info!(
        "Spawning procedural planet '{}': a={:.2}AU, M={:.1}M⊕, R={:.1}R⊕, type={:?}",
        planet.name,
        planet.semi_major_axis_au,
        planet.mass_earth,
        planet.radius_earth,
        planet.planet_type
    );

    let entity = commands
        .spawn((
            Planet,
            CelestialBody {
                name: planet.name.clone(),
                mass: mass_kg,
                radius: radius_km,
                body_type: planet.body_type(),
                visual_radius: radius_km,
                asteroid_class: None,
            },
            orbit,
            OrbitPath::new(Color::srgba(0.5, 0.7, 1.0, 0.4)),
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent_star),    // Link to parent star for orbital hierarchy
            OrbitsBody::new(parent_star),
            SystemId(system_id),
        ))
        .id();

    // Resource generation will be handled by the existing system
    // The metallicity_multiplier will be applied in the resource generation

    entity
}

/// Spawn asteroids in a belt
pub fn spawn_asteroid_belt(
    commands: &mut Commands,
    belt: &crate::astronomy::AsteroidBelt,
    parent_star: Entity,
    system_id: usize,
    star_name: &str,
    game_seed: u64,
) {
    // Deterministic RNG seeded from system_id and belt properties to ensure reproducible generation
    let seed = game_seed
        .wrapping_mul(system_id as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (belt.count as u64)
        ^ belt.inner_au.to_bits()
        ^ belt.outer_au.to_bits();
    let mut rng = StdRng::seed_from_u64(seed);

    info!(
        "Spawning asteroid belt: {:.2}-{:.2} AU, {} asteroids",
        belt.inner_au, belt.outer_au, belt.count
    );

    for i in 0..belt.count {
        // Random orbital parameters within the belt
        let semi_major_axis = rng.gen_range(belt.inner_au..belt.outer_au);
        let eccentricity = rng.gen_range(0.0..0.2);
        let inclination = belt.inclination + rng.gen_range(-0.05..0.05);

        // Calculate orbital period using Kepler's third law
        let period_years = semi_major_axis.powf(1.5);
        let period_seconds = period_years * 365.25 * 86400.0;
        let mean_motion = std::f64::consts::TAU / period_seconds;

        let orbit = KeplerOrbit::new(
            eccentricity,
            semi_major_axis,
            inclination,
            rng.gen_range(0.0..std::f64::consts::TAU),
            rng.gen_range(0.0..std::f64::consts::TAU),
            rng.gen_range(0.0..std::f64::consts::TAU),
            mean_motion,
        );

        // Determine asteroid class (M, S, V types for inner belt)
        let asteroid_class = if rng.gen_bool(0.3) {
            AsteroidClass::MType // Metal-rich
        } else if rng.gen_bool(0.6) {
            AsteroidClass::SType // Silicate-rich
        } else {
            AsteroidClass::VType // Basaltic
        };

        // Random size (radius 0.1 - 50 km)
        let radius = rng.gen_range(0.1..50.0);
        // Rough mass estimate (density ~2500 kg/m³)
        let mass = (4.0 / 3.0) * std::f64::consts::PI * (radius as f64 * 1000.0).powi(3) * 2500.0;

        commands.spawn((
            Asteroid,
            CelestialBody {
                name: format!("{} Belt Asteroid {}", star_name, i + 1),
                mass,
                radius,
                body_type: BodyType::Asteroid,
                visual_radius: radius,
                asteroid_class: Some(asteroid_class),
            },
            orbit,
            OrbitPath::new(Color::srgba(0.6, 0.6, 0.5, 0.2)),
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent_star),    // Link to parent star for orbital hierarchy
            OrbitsBody::new(parent_star),
            SystemId(system_id),
        ));
    }
}

/// Spawn comets in a cloud
pub fn spawn_cometary_cloud(
    commands: &mut Commands,
    cloud: &crate::astronomy::CometaryCloud,
    parent_star: Entity,
    system_id: usize,
    star_name: &str,
    game_seed: u64,
) {
    // Deterministic RNG seeded from system_id and cloud properties to ensure reproducible generation
    let seed = game_seed
        .wrapping_mul(system_id as u64)
        .wrapping_mul(0x517C_C1B7_2722_0A95)
        ^ (cloud.count as u64)
        ^ cloud.inner_au.to_bits()
        ^ cloud.outer_au.to_bits();
    let mut rng = StdRng::seed_from_u64(seed);

    info!(
        "Spawning cometary cloud: {:.2}-{:.2} AU, {} comets",
        cloud.inner_au, cloud.outer_au, cloud.count
    );

    for i in 0..cloud.count {
        // Random orbital parameters within the cloud (spherical distribution)
        let semi_major_axis = rng.gen_range(cloud.inner_au..cloud.outer_au);
        let eccentricity = rng.gen_range(0.3..0.9); // Highly eccentric
        let inclination = rng.gen_range(0.0..std::f64::consts::PI); // Any inclination

        // Calculate orbital period using Kepler's third law
        let period_years = semi_major_axis.powf(1.5);
        let period_seconds = period_years * 365.25 * 86400.0;
        let mean_motion = std::f64::consts::TAU / period_seconds;

        let orbit = KeplerOrbit::new(
            eccentricity,
            semi_major_axis,
            inclination,
            rng.gen_range(0.0..std::f64::consts::TAU),
            rng.gen_range(0.0..std::f64::consts::TAU),
            rng.gen_range(0.0..std::f64::consts::TAU),
            mean_motion,
        );

        // Comets are small (0.5-10 km radius)
        let radius = rng.gen_range(0.5..10.0);
        // Low density ice/rock (density ~500 kg/m³)
        let mass = (4.0 / 3.0) * std::f64::consts::PI * (radius as f64 * 1000.0).powi(3) * 500.0;

        commands.spawn((
            Comet,
            CelestialBody {
                name: format!("{} Cloud Comet {}", star_name, i + 1),
                mass,
                radius,
                body_type: BodyType::Comet,
                visual_radius: radius,
                asteroid_class: Some(AsteroidClass::PType), // P-type (volatile-rich)
            },
            orbit,
            OrbitPath::new(Color::srgba(0.4, 0.6, 0.8, 0.3)),
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent_star),    // Link to parent star for orbital hierarchy
            OrbitsBody::new(parent_star),
            SystemId(system_id),
        ));
    }
}

/// Convert spectral type string to SpectralClass enum
fn spectral_type_to_class(spectral_type: &str) -> SpectralClass {
    let first_char = spectral_type.chars().next().unwrap_or('G');
    match first_char {
        'O' => SpectralClass::O,
        'B' => SpectralClass::B,
        'A' => SpectralClass::A,
        'F' => SpectralClass::F,
        'G' => SpectralClass::G,
        'K' => SpectralClass::K,
        'M' => SpectralClass::M,
        _ => SpectralClass::G, // Default to G
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spectral_type_conversion() {
        assert_eq!(spectral_type_to_class("G2V"), SpectralClass::G);
        assert_eq!(spectral_type_to_class("M5.5Ve"), SpectralClass::M);
        assert_eq!(spectral_type_to_class("K1V"), SpectralClass::K);
        assert_eq!(spectral_type_to_class("A5"), SpectralClass::A);
    }
}
