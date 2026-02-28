//! System Populator Plugin
//!
//! This plugin handles procedural generation of star systems by:
//! 1. Loading confirmed exoplanet data from nearby stars
//! 2. Filling in missing planets/bodies using procedural generation
//! 3. Spawning asteroid belts and cometary clouds
//! 4. Applying resource generation with metallicity bonuses

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::RenderAssetUsages;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::astronomy::components::{CurrentStarSystem, OrbitCenter, SystemId};
use crate::astronomy::nearby_stars::load_nearby_stars_data;
use crate::astronomy::exoplanets::RealPlanet;
use crate::astronomy::nearby_stars::{NearbyStarsData, PlanetData, StarData};
use crate::astronomy::{
    calculate_frost_line, generate_procedural_atmosphere, map_star_to_system_architecture,
    KeplerOrbit, LocalOrbitAmplification, OrbitPath, ProceduralPlanet, SpaceCoordinates,
    StellarProperties, SurfaceTemperature, SCALING_FACTOR,
};
use crate::economy::components::{OrbitsBody, SpectralClass, StarSystem};
use crate::economy::generation::generate_solar_system_resources;
use crate::game_state::GameSeed;
use crate::plugins::solar_system::{
    Asteroid, CelestialBody, ClickExcluded, Comet, LogicalParent, Moon, Planet, Ring, Star,
    create_ring_mesh,
};
use crate::plugins::solar_system_data::{calculate_visual_radius, system_visual_scale, AsteroidClass, BodyType};
use crate::plugins::starmap::{PlanetCategory, SystemMetadata, classify_exoplanet};

pub struct SystemPopulatorPlugin;

impl Plugin for SystemPopulatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            populate_nearby_systems
                .after(load_nearby_stars_data)
                .before(generate_solar_system_resources),
        );
    }
}

/// Main system that populates nearby star systems with procedural bodies
/// This runs after the initial solar system is set up and uses the GameSeed
/// for deterministic generation
fn populate_nearby_systems(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    stars_data: Res<NearbyStarsData>,
    game_seed: Res<GameSeed>,
    _current_system: Res<CurrentStarSystem>,
    mut system_metadata: ResMut<SystemMetadata>,
) {
    // Use game seed for deterministic generation
    let mut rng = StdRng::seed_from_u64(game_seed.value);

    info!(
        "Starting procedural population of nearby star systems with seed {}",
        game_seed.value
    );

    // Fallback counter for systems that are NOT in NEARBY_STARS_POSITIONS.
    // These systems will not appear on the starmap, but we still give them
    // unique IDs so their entities are harmless rather than conflicting.
    use crate::astronomy::nearby_stars::NEARBY_STARS_POSITIONS;
    let mut next_fallback_id = NEARBY_STARS_POSITIONS.len() + 1;

    for system_data in &stars_data.systems {
        // Skip if this is the Sol system (already populated)
        if system_data.system_name == "Sol" {
            continue;
        }

        // Look up the starmap-compatible system ID for this system.
        // This MUST match the ID the starmap icon was spawned with
        // (index in NEARBY_STARS_POSITIONS + 1) so that the floating
        // origin is set to the correct position on system transition.
        let system_id = if let Some(id) = NearbyStarsData::get_system_id_by_name(&system_data.system_name) {
            id
        } else {
            // System is not on the starmap — assign a unique high ID
            let id = next_fallback_id;
            next_fallback_id += 1;
            id
        };

        info!(
            "Populating system '{}' at {:.2} ly with {} stars",
            system_data.system_name,
            system_data.distance_ly,
            system_data.stars.len()
        );

        // Use 3D coordinates from the static data if available
        // Each light year = 63,241.077 AU
        let distance_au = (system_data.distance_ly as f64) * 63241.077;
        
        let mut star_position = DVec3::new(distance_au, 0.0, 0.0);

        if let Some(pos_ly) = NearbyStarsData::get_position_by_name(&system_data.system_name) {
             star_position = DVec3::new(pos_ly[0], pos_ly[1], pos_ly[2]) * 63241.077;
             info!("  Using 3D coordinates for '{}': {:?}", system_data.system_name, star_position);
        } else {
             warn!("  No 3D coordinates found for '{}', using fallback X-axis placement", system_data.system_name);
        }

        // Spawn the primary star (first star in the list)
        if let Some(primary_star) = system_data.stars.first() {
            // Use calculated position
            // let star_position is already defined above


            // Use real metallicity if available, otherwise generate random
            let metallicity = primary_star.metallicity.unwrap_or_else(|| {
                let random_value = rng.random_range(-0.5..0.5);
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

            // Compute visual size scale for this system.
            // Compact systems (brown dwarfs, late-M dwarfs) get smaller body
            // meshes so they don't overwhelm their tiny orbits.
            let vis_scale = system_visual_scale(primary_star.luminosity_sol);
            if vis_scale < 1.0 {
                info!(
                    "  Visual scale for '{}': {:.2}x (L={:.2e})",
                    system_data.system_name, vis_scale, primary_star.luminosity_sol
                );
            }

            let star_entity = spawn_star_entity_with_metallicity(
                &mut commands,
                primary_star,
                system_id,
                star_position,
                metallicity,
            );

            // Cap stellar visual radius so the star mesh doesn't swallow
            // the innermost planets in compact systems.
            if let Some(inner_sma) = primary_star.planets.iter()
                .map(|p| p.semi_major_axis_au)
                .reduce(f32::min)
            {
                let max_star_vis = (inner_sma as f32) * (SCALING_FACTOR as f32) * 0.12;
                if let Ok(mut body) = commands.get_entity(star_entity) {
                    // We can't query components during command building, so
                    // read back the visual_radius we just set and clamp it.
                    let current = calculate_visual_radius(
                        BodyType::Star,
                        (primary_star.radius_sol * 695700.0) as f32,
                    );
                    if current > max_star_vis && max_star_vis > 2.0 {
                        body.insert(CelestialBody {
                            name: primary_star.name.clone(),
                            mass: (primary_star.mass_sol * 1.989e30) as f64,
                            radius: primary_star.radius_sol * 695700.0,
                            body_type: BodyType::Star,
                            visual_radius: max_star_vis,
                            asteroid_class: None,
                        });
                    }
                }
            }

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
            let mut all_planet_entities: Vec<(Entity, f64, f32, f32, String)> = Vec::new(); // (entity, sma_au, mass_earth, visual_radius, name)
            for planet_data in &primary_star.planets {
                let planet_entity = spawn_confirmed_planet(&mut commands, planet_data, star_entity, system_id, primary_star.luminosity_sol, vis_scale, &mut rng);
                existing_orbits.push(planet_data.semi_major_axis_au as f64);
                let radius_earth = planet_data.radius_earth.unwrap_or(1.0);
                let radius_km = radius_earth * 6371.0;
                let vis_r = capped_visual_radius(BodyType::Planet, radius_km, planet_data.semi_major_axis_au as f64, vis_scale);
                all_planet_entities.push((planet_entity, planet_data.semi_major_axis_au as f64, planet_data.mass_earth, vis_r, planet_data.name.clone()));
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
                let planet_entity = spawn_procedural_planet(
                    &mut commands,
                    planet,
                    star_entity,
                    system_id,
                    metallicity_mult,
                    primary_star.luminosity_sol,
                    vis_scale,
                    &mut rng,
                );
                let vis_r = capped_visual_radius(planet.body_type(), planet.radius_km(), planet.semi_major_axis_au, vis_scale);
                all_planet_entities.push((planet_entity, planet.semi_major_axis_au, planet.mass_earth as f32, vis_r, planet.name.clone()));
            }

            for planet in &architecture.gas_giants {
                let planet_entity = spawn_procedural_planet(
                    &mut commands,
                    planet,
                    star_entity,
                    system_id,
                    metallicity_mult,
                    primary_star.luminosity_sol,
                    vis_scale,
                    &mut rng,
                );
                let vis_r = capped_visual_radius(planet.body_type(), planet.radius_km(), planet.semi_major_axis_au, vis_scale);
                all_planet_entities.push((planet_entity, planet.semi_major_axis_au, planet.mass_earth as f32, vis_r, planet.name.clone()));
            }

            // Generate moons and possibly rings for planets massive enough to retain them
            for (planet_entity, sma_au, mass_earth, vis_r, planet_name) in &all_planet_entities {
                let (planet_entity, sma_au, mass_earth, vis_r) = (*planet_entity, *sma_au, *mass_earth, *vis_r);
                spawn_procedural_moons(
                    &mut commands,
                    planet_entity,
                    planet_name,
                    sma_au,
                    mass_earth as f32,
                    vis_r,
                    system_id,
                    primary_star.luminosity_sol,
                    vis_scale,
                    &mut rng,
                );

                // Possibly add a ring system around large gas/ice giants.
                // Only bodies outside ~half the frost line can retain stable rings (no tidal disruption).
                let ring_chance = if mass_earth > 30.0 && sma_au > architecture.frost_line_au * 0.5 {
                    0.42 // Large gas giants: ~42% chance
                } else if mass_earth > 10.0 && sma_au > architecture.frost_line_au * 0.5 {
                    0.20 // Ice giants / sub-giants: ~20% chance
                } else {
                    0.0
                };
                if ring_chance > 0.0 && rng.random_bool(ring_chance) {
                    spawn_procedural_ring(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        planet_entity,
                        planet_name,
                        vis_r,
                        mass_earth,
                        system_id,
                        &mut rng,
                    );
                }
            }

            // Spawn asteroid belt if present
            if let Some(belt) = &architecture.asteroid_belt {
                spawn_asteroid_belt(
                    &mut commands,
                    belt,
                    star_entity,
                    system_id,
                    &system_data.system_name,
                    primary_star.luminosity_sol,
                    vis_scale,
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
                    primary_star.luminosity_sol,
                    vis_scale,
                    game_seed.value,
                );
            }

            // Compute and store bounding radius for this system
            let mut max_radius_au: f64 = 10.0;
            for (_, sma_au, _, _, _) in &all_planet_entities {
                max_radius_au = max_radius_au.max(sma_au * 1.5);
            }
            if let Some(belt) = &architecture.asteroid_belt {
                max_radius_au = max_radius_au.max(belt.outer_au * 1.2);
            }
            if let Some(cloud) = &architecture.cometary_cloud {
                max_radius_au = max_radius_au.max(cloud.outer_au * 1.1);
            }
            system_metadata.set_bounding_radius(system_id, max_radius_au);
        }
    }

    info!(
        "Completed procedural population of {} star systems",
        stars_data.systems.iter().filter(|s| s.system_name != "Sol").count()
    );
}

/// Calculate surface temperature for a planet based on its distance from a star
/// and the star's luminosity.
///
/// Uses the equilibrium temperature formula: T = (L / (16πσd²))^0.25 * T_star
/// Simplified as: T = T_eq * sqrt(sqrt(L_sol))
/// where T_eq is the equilibrium temperature at 1 AU from a Sol-like star.
///
/// # Arguments
/// * `distance_au` - Distance from the star in AU
/// * `luminosity_sol` - Star's luminosity relative to Sol (1.0 = Sol)
///
/// # Returns
/// A tuple of (average, min, max) temperatures in Celsius
fn calculate_temperature_from_star(distance_au: f64, luminosity_sol: f32) -> (f32, f32, f32) {
    if distance_au <= 0.0 {
        return (-200.0, -200.0, -200.0);
    }

    // Equilibrium temperature formula: T_eq = 278.5 K * sqrt(L/d²)^0.25
    // Where 278.5 K is Earth's equilibrium temperature (without greenhouse effect)
    // Actually using 255 K for better representation of airless bodies
    let temp_k = 255.0 * ((luminosity_sol as f64) / (distance_au * distance_au)).sqrt().sqrt();
    let avg_temp_c = (temp_k - 273.15) as f32;

    // Airless bodies have extreme day/night differentials
    // Max temp ~1.55x equilibrium, Min temp ~0.40x equilibrium
    let max_k = temp_k * 1.55;
    let min_k = temp_k * 0.40;

    let min_temp_c = (min_k - 273.15) as f32;
    let max_temp_c = (max_k - 273.15) as f32;

    (avg_temp_c, min_temp_c, max_temp_c)
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
                visual_radius: calculate_visual_radius(
                    BodyType::Star,
                    (star_data.radius_sol * 695700.0) as f32,
                ),
                asteroid_class: None,
            },
            StellarProperties::new(star_data.luminosity_sol, star_data.temp_k),
            SpaceCoordinates::new(position),
            SystemId(system_id),
            star_system,
        ))
        .id();

    entity
}

/// Compute the visual radius of a planet, capped at 10% of orbital distance
/// to prevent overlap with neighbors, with a minimum of 2.0.
fn capped_visual_radius(body_type: BodyType, radius_km: f32, sma_au: f64, vis_scale: f32) -> f32 {
    let base = calculate_visual_radius(body_type, radius_km) * vis_scale;
    let orbit_bevy = (sma_au as f32) * (SCALING_FACTOR as f32);
    base.min(orbit_bevy * 0.10).max(2.0)
}

/// Spawn a confirmed planet from real exoplanet data
pub fn spawn_confirmed_planet(
    commands: &mut Commands,
    planet_data: &PlanetData,
    parent_star: Entity,
    system_id: usize,
    star_luminosity_sol: f32,
    vis_scale: f32,
    rng: &mut impl rand::Rng,
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

    // Calculate equilibrium temperature based on stellar luminosity
    let (equilibrium_temp_c, min_temp, max_temp) = calculate_temperature_from_star(
        planet_data.semi_major_axis_au as f64,
        star_luminosity_sol,
    );
    let equilibrium_temp_k = (equilibrium_temp_c as f64) + 273.15;

    // Try to generate procedural atmosphere
    let atmosphere_result = generate_procedural_atmosphere(
        planet_data.mass_earth,
        planet_data.radius_earth.unwrap_or(1.0),
        planet_data.semi_major_axis_au as f64,
        star_luminosity_sol,
        equilibrium_temp_k,
        rng,
    );

    let (avg_temp, has_atmosphere) = if let Some((_atmosphere, surface_temp)) = &atmosphere_result {
        (surface_temp, true)
    } else {
        (&equilibrium_temp_c, false)
    };

    info!(
        "Spawning confirmed planet '{}': a={:.2}AU, M={:.1}M⊕, type={}, T={:.1}°C{}",
        planet_data.name,
        planet_data.semi_major_axis_au,
        planet_data.mass_earth,
        planet_data.planet_type,
        avg_temp,
        if has_atmosphere { " (atmosphere)" } else { "" }
    );

    // Cap visual radius to 10% of orbital distance (in Bevy units) so
    // close-in planets don't visually overlap the star or each other.
    let base_visual_radius = calculate_visual_radius(BodyType::Planet, radius_km) * vis_scale;
    let orbit_distance_bevy = (planet_data.semi_major_axis_au as f32) * (SCALING_FACTOR as f32);
    let max_orbit_fraction = orbit_distance_bevy * 0.10;
    let visual_radius = base_visual_radius.min(max_orbit_fraction).max(2.0);

    // Classify the planet based on temperature for texture/UI display
    let cat_seed: u32 = planet_data.name.bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let category = classify_exoplanet(BodyType::Planet, None, *avg_temp, cat_seed);

    let mut entity_commands = commands.spawn((
        Planet,
        RealPlanet, // Mark as confirmed planet
        CelestialBody {
            name: planet_data.name.clone(),
            mass: mass_kg,
            radius: radius_km,
            body_type: BodyType::Planet,
            visual_radius,
            asteroid_class: None,
        },
        SurfaceTemperature {
            average_celsius: *avg_temp,
            min_celsius: min_temp,
            max_celsius: max_temp,
        },
        PlanetCategory(category.to_string()),
        orbit,
        OrbitPath::new(Color::srgba(0.4, 0.75, 1.0, 0.7)), // Cyan/blue — matches Sol palette
        SpaceCoordinates::default(),                      // Will be updated by propagate_orbits
        OrbitCenter(parent_star), // Link to parent star for orbital hierarchy
        OrbitsBody::new(parent_star),
        LogicalParent(parent_star),
        SystemId(system_id),
    ));

    // Add atmosphere if generated
    if let Some((atmosphere, _)) = atmosphere_result {
        entity_commands.insert(atmosphere);
    }

    entity_commands.id()
}

/// Spawn a procedurally generated planet
pub fn spawn_procedural_planet(
    commands: &mut Commands,
    planet: &ProceduralPlanet,
    parent_star: Entity,
    system_id: usize,
    _metallicity_multiplier: f32,
    star_luminosity_sol: f32,
    vis_scale: f32,
    rng: &mut impl rand::Rng,
) -> Entity {
    let orbit = planet.to_kepler_orbit();
    let mass_kg = planet.mass_kg();
    let radius_km = planet.radius_km();

    // Calculate equilibrium temperature based on stellar luminosity
    let (equilibrium_temp_c, min_temp, max_temp) = calculate_temperature_from_star(
        planet.semi_major_axis_au,
        star_luminosity_sol,
    );
    let equilibrium_temp_k = (equilibrium_temp_c as f64) + 273.15;

    // Try to generate procedural atmosphere for terrestrial planets
    let atmosphere_result = generate_procedural_atmosphere(
        planet.mass_earth,
        planet.radius_earth,
        planet.semi_major_axis_au,
        star_luminosity_sol,
        equilibrium_temp_k,
        rng,
    );

    let (avg_temp, has_atmosphere) = if let Some((_, surface_temp)) = &atmosphere_result {
        (surface_temp, true)
    } else {
        (&equilibrium_temp_c, false)
    };

    info!(
        "Spawning procedural planet '{}': a={:.2}AU, M={:.1}M⊕, R={:.1}R⊕, type={:?}, T={:.1}°C{}",
        planet.name,
        planet.semi_major_axis_au,
        planet.mass_earth,
        planet.radius_earth,
        planet.planet_type,
        avg_temp,
        if has_atmosphere { " (atmosphere)" } else { "" }
    );

    // Cap visual radius to 10% of orbital distance (in Bevy units) so
    // close-in planets don't visually overlap the star or each other.
    let base_visual_radius = calculate_visual_radius(planet.body_type(), radius_km) * vis_scale;
    let orbit_distance_bevy = (planet.semi_major_axis_au as f32) * (SCALING_FACTOR as f32);
    let max_orbit_fraction = orbit_distance_bevy * 0.10;
    let visual_radius = base_visual_radius.min(max_orbit_fraction).max(2.0);

    // Classify the planet based on temperature for texture/UI display
    let cat_seed: u32 = planet.name.bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let category = classify_exoplanet(planet.body_type(), None, *avg_temp, cat_seed);

    let mut entity_commands = commands.spawn((
        Planet,
        CelestialBody {
            name: planet.name.clone(),
            mass: mass_kg,
            radius: radius_km,
            body_type: planet.body_type(),
            visual_radius,
            asteroid_class: None,
        },
        SurfaceTemperature {
            average_celsius: *avg_temp,
            min_celsius: min_temp,
            max_celsius: max_temp,
        },
        PlanetCategory(category.to_string()),
        orbit,
        OrbitPath::new(Color::srgba(0.4, 0.75, 1.0, 0.6)), // Cyan/blue — procedural planets
        SpaceCoordinates::default(), // Will be updated by propagate_orbits
        OrbitCenter(parent_star),    // Link to parent star for orbital hierarchy
        OrbitsBody::new(parent_star),
        LogicalParent(parent_star),
        SystemId(system_id),
    ));

    // Add atmosphere if generated
    if let Some((atmosphere, _)) = atmosphere_result {
        entity_commands.insert(atmosphere);
    }

    let entity = entity_commands.id();

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
    star_luminosity_sol: f32,
    vis_scale: f32,
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
        let semi_major_axis = rng.random_range(belt.inner_au..belt.outer_au);
        let eccentricity = rng.random_range(0.0..0.2);
        let inclination = belt.inclination + rng.random_range(-0.05..0.05);

        // Calculate orbital period using Kepler's third law
        let period_years = semi_major_axis.powf(1.5);
        let period_seconds = period_years * 365.25 * 86400.0;
        let mean_motion = std::f64::consts::TAU / period_seconds;

        let orbit = KeplerOrbit::new(
            eccentricity,
            semi_major_axis,
            inclination,
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            mean_motion,
        );

        // Determine asteroid class (MType, SType, VType for inner belt)
        let asteroid_class = if rng.random_bool(0.3) {
            AsteroidClass::MType // Metal-rich
        } else if rng.random_bool(0.6) {
            AsteroidClass::SType // Silicate-rich
        } else {
            AsteroidClass::VType // Basaltic
        };

        // Random size (radius 0.1 - 15 km); most belt asteroids are small
        let radius = rng.random_range(0.1..15.0);
        // Rough mass estimate (density ~2500 kg/m³)
        let mass = (4.0 / 3.0) * std::f64::consts::PI * (radius as f64 * 1000.0).powi(3) * 2500.0;

        // Calculate asteroid temperature based on its distance from the star
        let (avg_temp, min_temp, max_temp) = calculate_temperature_from_star(semi_major_axis, star_luminosity_sol);

        commands.spawn((
            Asteroid,
            CelestialBody {
                name: format!("{} Belt Asteroid {}", star_name, i + 1),
                mass,
                radius,
                body_type: BodyType::Asteroid,
                visual_radius: calculate_visual_radius(BodyType::Asteroid, radius as f32) * vis_scale,
                asteroid_class: Some(asteroid_class),
            },
            SurfaceTemperature {
                average_celsius: avg_temp,
                min_celsius: min_temp,
                max_celsius: max_temp,
            },
            orbit,
            OrbitPath::new(Color::srgba(0.6, 0.6, 0.5, 0.2)),
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent_star),    // Link to parent star for orbital hierarchy
            OrbitsBody::new(parent_star),
            LogicalParent(parent_star),
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
    star_luminosity_sol: f32,
    vis_scale: f32,
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
        let semi_major_axis = rng.random_range(cloud.inner_au..cloud.outer_au);
        let eccentricity = rng.random_range(0.3..0.9); // Highly eccentric
        let inclination = rng.random_range(0.0..std::f64::consts::PI); // Any inclination

        // Calculate orbital period using Kepler's third law
        let period_years = semi_major_axis.powf(1.5);
        let period_seconds = period_years * 365.25 * 86400.0;
        let mean_motion = std::f64::consts::TAU / period_seconds;

        let orbit = KeplerOrbit::new(
            eccentricity,
            semi_major_axis,
            inclination,
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            mean_motion,
        );

        // Comets are small (0.5-10 km radius)
        let radius = rng.random_range(0.5..10.0);
        // Low density ice/rock (density ~500 kg/m³)
        let mass = (4.0 / 3.0) * std::f64::consts::PI * (radius as f64 * 1000.0).powi(3) * 500.0;

        // Calculate comet temperature based on its distance from the star
        let (avg_temp, min_temp, max_temp) = calculate_temperature_from_star(semi_major_axis, star_luminosity_sol);

        commands.spawn((
            Comet,
            CelestialBody {
                name: format!("{} Cloud Comet {}", star_name, i + 1),
                mass,
                radius,
                body_type: BodyType::Comet,
                visual_radius: calculate_visual_radius(BodyType::Comet, radius as f32) * vis_scale,
                asteroid_class: Some(AsteroidClass::PType), // P-type (volatile-rich)
            },
            SurfaceTemperature {
                average_celsius: avg_temp,
                min_celsius: min_temp,
                max_celsius: max_temp,
            },
            orbit,
            OrbitPath::new(Color::srgba(0.4, 0.6, 0.8, 0.3)),
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent_star),    // Link to parent star for orbital hierarchy
            OrbitsBody::new(parent_star),
            LogicalParent(parent_star),
            SystemId(system_id),
        ));
    }
}

/// Spawn procedural moons for a planet based on its mass
///
/// Gas giants (>10 M⊕) get 2-6 moons, rocky planets (>0.5 M⊕) get 0-2.
/// Smaller bodies get no moons.
///
/// Each moon receives a [`LocalOrbitAmplification`] component so that its
/// orbit renders outside the parent planet's visual mesh, matching the
/// Universe Sandbox-style approach used for Sol-system moons.
/// Spawn a procedural ring system around a gas/ice giant.
///
/// Generates a 1-D radial RGBA texture with distinct ring bands, gaps, and a
/// colour gradient — mimicking the approach used by the Sol system’s real
/// Saturn / Uranus ring textures.  The texture is mapped across the U
/// coordinate of the ring mesh (0 = inner edge, 1 = outer edge).
fn spawn_procedural_ring(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    planet_entity: Entity,
    planet_name: &str,
    planet_visual_radius: f32,
    mass_earth: f32,
    system_id: usize,
    rng: &mut impl Rng,
) {
    // ── Geometry ──────────────────────────────────────────────────────────
    let outer_scale: f32 = rng.random_range(1.5_f32..2.5);
    let inner_frac:  f32 = rng.random_range(0.40_f32..0.70);
    let outer_radius = planet_visual_radius * outer_scale;
    let inner_radius = outer_radius * inner_frac;

    // ── Ring flavor (base colour palette) ─────────────────────────────────
    let flavor: u32 = rng.random_range(0..100);
    // (inner_rgb, outer_rgb) – the colour lerps from inner to outer
    let (inner_rgb, outer_rgb, flavor_label): ([f32; 3], [f32; 3], &str) = if flavor < 35 {
        // Saturn-like: warm cream-to-tan gradient
        ([0.92, 0.86, 0.70], [0.72, 0.58, 0.38], "Saturn-tan")
    } else if flavor < 58 {
        // Uranus-like: pale blue-grey → slate
        ([0.72, 0.82, 0.88], [0.45, 0.55, 0.62], "Uranus-blue")
    } else if flavor < 76 {
        // Icy white (Neptune-like): almost white → cool grey
        ([0.92, 0.94, 0.96], [0.68, 0.72, 0.78], "Icy-white")
    } else if flavor < 92 {
        // Dusty reddish-brown: warm rust gradient
        ([0.78, 0.52, 0.36], [0.55, 0.32, 0.20], "Dusty-red")
    } else {
        // Near-transparent sooty dark (faint Jupiter-like)
        ([0.35, 0.30, 0.25], [0.18, 0.15, 0.12], "Dark-faint")
    };

    // Per-ring jitter so no two look identical
    let jitter_r = rng.random_range(-0.05_f32..0.05);
    let jitter_g = rng.random_range(-0.05_f32..0.05);
    let jitter_b = rng.random_range(-0.05_f32..0.05);

    // ── Generate procedural ring texture ─────────────────────────────────
    // 512 pixels wide (U direction = radial), 1 pixel tall.
    // This is replicated around the ring via UV mapping.
    const TEX_W: u32 = 512;
    const TEX_H: u32 = 1;

    // Decide how many ring bands and gaps this system has
    let num_bands: usize = rng.random_range(3..=8);
    let num_gaps:  usize = rng.random_range(1..=num_bands.saturating_sub(1).max(1));

    // Pre-generate gap locations (as fractions 0..1 across the radial extent)
    // and widths.  Gaps are narrow transparent slots between bands.
    struct GapInfo {
        center: f32, // 0..1
        half_w: f32, // half-width in 0..1
    }
    let gaps: Vec<GapInfo> = (0..num_gaps)
        .map(|_| GapInfo {
            center: rng.random_range(0.08_f32..0.92),
            half_w: rng.random_range(0.01_f32..0.06),
        })
        .collect();

    // Pre-generate band density peaks.  Each band is a smooth bump that
    // contributes opacity; overlapping bumps create the rich layered look.
    struct BandInfo {
        center: f32,
        sigma:  f32, // gaussian width
        peak:   f32, // peak alpha
    }
    let bands: Vec<BandInfo> = (0..num_bands)
        .map(|_| BandInfo {
            center: rng.random_range(0.05_f32..0.95),
            sigma:  rng.random_range(0.04_f32..0.20),
            peak:   rng.random_range(0.35_f32..0.85),
        })
        .collect();

    let mut pixels = Vec::with_capacity((TEX_W as usize) * 4);
    for x in 0..TEX_W {
        let u = x as f32 / (TEX_W - 1) as f32; // 0 = inner, 1 = outer

        // Colour: lerp inner → outer + jitter
        let cr = ((inner_rgb[0] + (outer_rgb[0] - inner_rgb[0]) * u) + jitter_r).clamp(0.0, 1.0);
        let cg = ((inner_rgb[1] + (outer_rgb[1] - inner_rgb[1]) * u) + jitter_g).clamp(0.0, 1.0);
        let cb = ((inner_rgb[2] + (outer_rgb[2] - inner_rgb[2]) * u) + jitter_b).clamp(0.0, 1.0);

        // Alpha: sum of band contributions
        let mut alpha: f32 = 0.0;
        for band in &bands {
            let d = (u - band.center) / band.sigma;
            alpha += band.peak * (-0.5 * d * d).exp();
        }

        // Cut gaps: anywhere inside a gap, drive alpha to zero
        for gap in &gaps {
            if (u - gap.center).abs() < gap.half_w {
                // Smooth edge: fade linearly over the outer 30% of the gap
                let edge_dist = gap.half_w - (u - gap.center).abs();
                let edge_zone = gap.half_w * 0.3;
                let gap_factor = if edge_dist > edge_zone {
                    0.0 // deep inside gap
                } else {
                    edge_dist / edge_zone // smooth ramp at edge
                };
                alpha *= gap_factor;
            }
        }

        // Soft fade at inner and outer edges so the ring doesn't end abruptly
        let edge_fade = (u * 8.0).min(1.0) * ((1.0 - u) * 8.0).min(1.0);
        alpha *= edge_fade;

        // Add a touch of fine-grain noise for texture
        let noise = rng.random_range(-0.04_f32..0.04);
        alpha = (alpha + noise).clamp(0.0, 1.0);

        pixels.push((cr * 255.0) as u8);
        pixels.push((cg * 255.0) as u8);
        pixels.push((cb * 255.0) as u8);
        pixels.push((alpha * 255.0) as u8);
    }

    let ring_image = Image::new(
        Extent3d {
            width: TEX_W,
            height: TEX_H,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let texture_handle = images.add(ring_image);

    // ── Slight random tilt ────────────────────────────────────────────────
    let tilt: f32 = rng.random_range(-0.14_f32..0.14);
    let transform = Transform::from_rotation(Quat::from_rotation_x(tilt));

    // ── Ring mass estimate ────────────────────────────────────────────────
    let ring_mass_kg: f64 = (mass_earth as f64).sqrt() * 2.0e18;
    let ring_radius_km = outer_radius * 5_000.0;

    // ── Build mesh + material ─────────────────────────────────────────────
    let mesh_handle = meshes.add(create_ring_mesh(outer_radius, inner_radius, 128));
    let mat_handle  = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(texture_handle),
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        unlit: true,
        perceptual_roughness: 0.9,
        metallic: 0.0,
        ..default()
    });

    commands.spawn((
        Ring,
        ClickExcluded,
        CelestialBody {
            name: format!("{} Rings", planet_name),
            mass: ring_mass_kg,
            radius: ring_radius_km,
            body_type: BodyType::Ring,
            visual_radius: outer_radius,
            asteroid_class: None,
        },
        Mesh3d(mesh_handle),
        MeshMaterial3d(mat_handle),
        transform,
        SystemId(system_id),
        ChildOf(planet_entity),
        LogicalParent(planet_entity),
    ));

    info!(
        "  Spawned rings for '{}' (outer={:.1}, inner={:.1}, bands={}, gaps={}, flavor={})",
        planet_name, outer_radius, inner_radius, num_bands, num_gaps, flavor_label,
    );
}

/// Convert a 1-based index to a Roman numeral string (supports I–XX).
fn to_roman(n: u32) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        6 => "VI",
        7 => "VII",
        8 => "VIII",
        9 => "IX",
        10 => "X",
        _ => "?",
    }
}

fn spawn_procedural_moons(
    commands: &mut Commands,
    planet_entity: Entity,
    planet_name: &str,
    planet_sma_au: f64,
    planet_mass_earth: f32,
    parent_visual_radius: f32,
    system_id: usize,
    star_luminosity_sol: f32,
    vis_scale: f32,
    rng: &mut StdRng,
) {
    /// Innermost moon orbits at this multiple of parent visual radius
    const INNER_MOON_MULTIPLIER: f64 = 2.0;
    /// Outermost moon orbits at this multiple of parent visual radius
    const OUTER_MOON_MULTIPLIER: f64 = 10.0;

    // Determine moon count based on planet mass
    let moon_count = if planet_mass_earth > 50.0 {
        // Gas giants get many moons
        rng.random_range(3..=6)
    } else if planet_mass_earth > 10.0 {
        // Sub-giants / ice giants
        rng.random_range(1..=4)
    } else if planet_mass_earth > 0.5 {
        // Rocky planets may get 0-2 moons
        rng.random_range(0..=2_u32)
    } else {
        // Too small to retain moons
        return;
    };

    if moon_count == 0 {
        return;
    }

    // Visual bounds for moon orbits (in Bevy units).
    // The outer display radius is additionally capped at 20% of the planet's own
    // orbital distance from its star.  Without this cap, `capped_visual_radius`
    // already limits planet visual size to ~10% of orbit, so the 10× multiplier
    // could push the outermost moon orbit out to ~100% of the planet's orbital
    // radius — causing it to intersect neighbouring planet orbits.
    // `.max(inner_display * 1.5)` ensures there is always a minimum spread even
    // for very close-in planets where the 20% orbital cap is tight.
    let hill_sphere_cap = planet_sma_au * SCALING_FACTOR * 0.20;
    let inner_display = parent_visual_radius as f64 * INNER_MOON_MULTIPLIER;
    let outer_display = (parent_visual_radius as f64 * OUTER_MOON_MULTIPLIER)
        .min(hill_sphere_cap)
        .max(inner_display * 1.5);

    for i in 0..moon_count {
        // Moon orbital distance scales with planet mass (Hill sphere proxy)
        // Typical range: 0.001 - 0.01 AU from planet
        let base_distance = 0.001 + (i as f64) * 0.002;
        let orbital_distance_au = base_distance * (1.0 + rng.random_range(-0.3..0.3_f64));

        // Moon mass: tiny fraction of planet mass, scaled by planet type
        // Real examples: Ganymede ~0.025% of Jupiter, Titan ~0.024% of Saturn,
        //                Earth's Moon ~1.2% of Earth
        let mass_fraction = if planet_mass_earth > 10.0 {
            // Gas/ice giants: moons are a much smaller fraction (0.001% - 0.05%)
            rng.random_range(0.00001..0.0005_f64)
        } else {
            // Rocky planets: moons can be a larger fraction (0.01% - 1.5%)
            rng.random_range(0.0001..0.015_f64)
        };
        let moon_mass_earth = (planet_mass_earth as f64) * mass_fraction;
        let moon_mass_kg = moon_mass_earth * 5.972e24;

        // Estimate radius from mass (assume rocky density ~3500 kg/m³)
        let volume_m3 = moon_mass_kg / 3500.0;
        let radius_m = (volume_m3 * 3.0 / (4.0 * std::f64::consts::PI)).powf(1.0 / 3.0);
        let radius_km = (radius_m / 1000.0) as f32;

        // Orbital period from parent planet's mass
        let parent_mass_kg = (planet_mass_earth as f64) * 5.972e24;
        let g = 6.674e-11;
        let sma_m = orbital_distance_au * 1.496e11;
        let period_s = std::f64::consts::TAU * (sma_m.powi(3) / (g * parent_mass_kg)).sqrt();
        let mean_motion = std::f64::consts::TAU / period_s;

        let orbit = KeplerOrbit::new(
            rng.random_range(0.0..0.05_f64),       // Low eccentricity
            orbital_distance_au,                 // SMA in AU
            rng.random_range(-0.05..0.05_f64),     // Near-coplanar
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            mean_motion,
        );

        // Compute orbit amplification so moons render outside the parent mesh
        let orbit_bevy = orbital_distance_au * SCALING_FACTOR;
        let amp = if moon_count == 1 {
            let mid_display = (inner_display + outer_display) * 0.5;
            (mid_display / orbit_bevy).max(1.0) as f32
        } else {
            let t = i as f64 / (moon_count - 1) as f64;
            let display_distance = inner_display + t * (outer_display - inner_display);
            (display_distance / orbit_bevy).max(1.0) as f32
        };

        let moon_name = format!("{} {}", planet_name, to_roman(i + 1));

        // Calculate moon temperature using parent planet's distance from star
        // (moons orbit the planet, but their temperature depends on their distance from the star)
        let (avg_temp, min_temp, max_temp) = calculate_temperature_from_star(planet_sma_au, star_luminosity_sol);

        // Cap moon visual radius relative to the parent planet's visual size.
        // Gas/ice giant moons are capped tighter (15%) because real moons like
        // Ganymede are only ~4% of Jupiter's radius.  Rocky planet moons allow up
        // to 25% (Earth's Moon is ~27% physically, but non-linear scaling inflates
        // the ratio).  Without this cap the shared MIN_VISUAL_RADIUS floor can
        // make small moons appear as large as their parent.
        let max_moon_ratio = if planet_mass_earth > 10.0 { 0.15 } else { 0.25 };
        let moon_visual_radius = (calculate_visual_radius(BodyType::Moon, radius_km) * vis_scale)
            .min(parent_visual_radius * max_moon_ratio);

        commands.spawn((
            Moon,
            CelestialBody {
                name: moon_name,
                mass: moon_mass_kg,
                radius: radius_km,
                body_type: BodyType::Moon,
                visual_radius: moon_visual_radius,
                asteroid_class: None,
            },
            SurfaceTemperature {
                average_celsius: avg_temp,
                min_celsius: min_temp,
                max_celsius: max_temp,
            },
            orbit,
            OrbitPath::new(Color::srgba(0.7, 0.7, 0.7, 0.3)),
            SpaceCoordinates::default(),
            OrbitCenter(planet_entity),
            OrbitsBody::new(planet_entity),
            LogicalParent(planet_entity),
            LocalOrbitAmplification(amp),
            SystemId(system_id),
        ));
    }

    if moon_count > 0 {
        info!(
            "  Spawned {} moons for {} at {:.2} AU (orbit amp: {:.1}x-{:.1}x)",
            moon_count, planet_name, planet_sma_au,
            (inner_display / (0.001 * SCALING_FACTOR)).max(1.0),
            (outer_display / ((0.001 + (moon_count as f64 - 1.0) * 0.002) * SCALING_FACTOR)).max(1.0),
        );
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
