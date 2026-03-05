//! System Populator Plugin
//!
//! This plugin handles procedural generation of star systems by:
//! 1. Loading confirmed exoplanet data from nearby stars
//! 2. Filling in missing planets/bodies using procedural generation
//! 3. Spawning asteroid belts and cometary clouds
//! 4. Applying resource generation with metallicity bonuses

use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::astronomy::components::{CurrentStarSystem, OrbitCenter, SystemId};
use crate::astronomy::exoplanets::RealPlanet;
use crate::astronomy::infer_ocean_properties;
use crate::astronomy::nearby_stars::load_nearby_stars_data;
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
    create_ring_mesh, Asteroid, AxialTilt, CelestialBody, ClickExcluded, Comet, DwarfPlanet,
    LogicalParent, Moon, Planet, Ring, RotationSpeed, Star,
};
use crate::plugins::solar_system_data::{
    calculate_visual_radius, system_visual_scale, AsteroidClass, BodyType,
};
use crate::plugins::starmap::{classify_exoplanet_with_mass, PlanetCategory, SystemMetadata};

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
        let system_id =
            if let Some(id) = NearbyStarsData::get_system_id_by_name(&system_data.system_name) {
                id
            } else {
                // System is not on the starmap — assign a unique high ID
                let id = next_fallback_id;
                next_fallback_id += 1;
                id
            };

        debug!(
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
            debug!(
                "  Using 3D coordinates for '{}': {:?}",
                system_data.system_name, star_position
            );
        } else {
            warn!(
                "  No 3D coordinates found for '{}', using fallback X-axis placement",
                system_data.system_name
            );
        }

        // Spawn the primary star (first star in the list)
        if let Some(primary_star) = system_data.stars.first() {
            // Use calculated position
            // let star_position is already defined above

            // Use real metallicity if available, otherwise generate random
            let metallicity = primary_star.metallicity.unwrap_or_else(|| {
                let random_value = rng.random_range(-0.5..0.5);
                debug!(
                    "  No metallicity data for '{}', using random: {:.2}",
                    primary_star.name, random_value
                );
                random_value
            });

            if primary_star.metallicity.is_some() {
                debug!(
                    "  Using real metallicity data for '{}': [Fe/H]={:.2}",
                    primary_star.name, metallicity
                );
            }

            // Compute visual size scale for this system.
            // Compact systems (brown dwarfs, late-M dwarfs) get smaller body
            // meshes so they don't overwhelm their tiny orbits.
            let vis_scale = system_visual_scale(primary_star.luminosity_sol);
            if vis_scale < 1.0 {
                debug!(
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

            // Star visual radius capping is deferred until after procedural
            // planets are generated, so it considers ALL inner planets.

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
            let mut all_planet_entities: Vec<(Entity, f64, f32, f32, f32, String)> = Vec::new(); // (entity, sma_au, mass_earth, visual_radius, radius_km, name)
            for planet_data in &primary_star.planets {
                let planet_entity = spawn_confirmed_planet(
                    &mut commands,
                    planet_data,
                    star_entity,
                    system_id,
                    primary_star.luminosity_sol,
                    vis_scale,
                    &mut rng,
                );
                existing_orbits.push(planet_data.semi_major_axis_au as f64);
                let radius_earth = planet_data.radius_earth.unwrap_or(1.0);
                let radius_km = radius_earth * 6371.0;
                let vis_r = capped_visual_radius(
                    BodyType::Planet,
                    radius_km,
                    planet_data.semi_major_axis_au as f64,
                    vis_scale,
                );
                all_planet_entities.push((
                    planet_entity,
                    planet_data.semi_major_axis_au as f64,
                    planet_data.mass_earth,
                    vis_r,
                    radius_km,
                    planet_data.name.clone(),
                ));
            }

            // Generate procedural architecture to fill gaps
            let architecture = map_star_to_system_architecture(
                &system_data.system_name,
                primary_star.mass_sol as f64,
                primary_star.luminosity_sol as f64,
                primary_star.planets.len(),
                &existing_orbits,
                &mut rng,
            );

            debug!(
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
                let vis_r = capped_visual_radius(
                    planet.body_type(),
                    planet.radius_km(),
                    planet.semi_major_axis_au,
                    vis_scale,
                );
                all_planet_entities.push((
                    planet_entity,
                    planet.semi_major_axis_au,
                    planet.mass_earth as f32,
                    vis_r,
                    planet.radius_km() as f32,
                    planet.name.clone(),
                ));
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
                let vis_r = capped_visual_radius(
                    planet.body_type(),
                    planet.radius_km(),
                    planet.semi_major_axis_au,
                    vis_scale,
                );
                all_planet_entities.push((
                    planet_entity,
                    planet.semi_major_axis_au,
                    planet.mass_earth as f32,
                    vis_r,
                    planet.radius_km() as f32,
                    planet.name.clone(),
                ));
            }

            // Generate moons and possibly rings for planets massive enough to retain them
            for (planet_entity, sma_au, mass_earth, vis_r, radius_km, planet_name) in
                &all_planet_entities
            {
                let (planet_entity, sma_au, mass_earth, vis_r, radius_km) =
                    (*planet_entity, *sma_au, *mass_earth, *vis_r, *radius_km);
                spawn_procedural_moons(
                    &mut commands,
                    planet_entity,
                    planet_name,
                    sma_au,
                    mass_earth as f32,
                    radius_km,
                    vis_r,
                    system_id,
                    primary_star.luminosity_sol,
                    vis_scale,
                    &mut rng,
                );

                // Possibly add a ring system around large gas/ice giants.
                // Only bodies outside ~half the frost line can retain stable rings (no tidal disruption).
                let ring_chance = if mass_earth > 30.0 && sma_au > architecture.frost_line_au * 0.5
                {
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

            // Spawn dwarf planets in the trans-Neptunian region
            if !architecture.dwarf_planets.is_empty() {
                spawn_dwarf_planets(
                    &mut commands,
                    &architecture.dwarf_planets,
                    star_entity,
                    system_id,
                    primary_star.luminosity_sol,
                    vis_scale,
                );
            }

            // Cap stellar visual radius so the star mesh doesn't swallow
            // the innermost planets. Uses all planets (confirmed + procedural).
            if let Some(inner_sma_au) = all_planet_entities
                .iter()
                .map(|&(_, sma, _, _, _, _)| sma)
                .reduce(f64::min)
            {
                // Planet should be visually outside the star surface.
                // Allow 12% of orbit distance as max star visual radius.
                let max_star_vis = (inner_sma_au as f32) * (SCALING_FACTOR as f32) * 0.12;
                let current = calculate_visual_radius(
                    BodyType::Star,
                    (primary_star.radius_sol * 695700.0) as f32,
                );
                if current > max_star_vis && max_star_vis > 2.0 {
                    if let Ok(mut body) = commands.get_entity(star_entity) {
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

            // Compute and store bounding radius for this system
            let mut max_radius_au: f64 = 10.0;
            for (_, sma_au, _, _, _, _) in &all_planet_entities {
                max_radius_au = max_radius_au.max(sma_au * 1.5);
            }
            if let Some(belt) = &architecture.asteroid_belt {
                max_radius_au = max_radius_au.max(belt.outer_au * 1.2);
            }
            if let Some(cloud) = &architecture.cometary_cloud {
                max_radius_au = max_radius_au.max(cloud.outer_au * 1.1);
            }
            for dp in &architecture.dwarf_planets {
                max_radius_au = max_radius_au.max(dp.semi_major_axis_au * 1.3);
            }
            system_metadata.set_bounding_radius(system_id, max_radius_au);
        }
    }

    info!(
        "Completed procedural population of {} star systems",
        stars_data
            .systems
            .iter()
            .filter(|s| s.system_name != "Sol")
            .count()
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
    let temp_k = 255.0
        * ((luminosity_sol as f64) / (distance_au * distance_au))
            .sqrt()
            .sqrt();
    let avg_temp_c = (temp_k - 273.15) as f32;

    // Airless bodies have extreme day/night differentials
    // Max temp ~1.55x equilibrium, Min temp ~0.40x equilibrium
    let max_k = temp_k * 1.55;
    let min_k = temp_k * 0.40;

    let min_temp_c = (min_k - 273.15) as f32;
    let max_temp_c = (max_k - 273.15) as f32;

    (avg_temp_c, min_temp_c, max_temp_c)
}

/// Adjust min/max temperature based on rotation period for airless bodies.
/// Fast rotators distribute heat more evenly; tidally locked have extreme differentials.
fn adjust_temperature_for_rotation(
    rotation_period_days: f32,
    _base_min: f32,
    _base_max: f32,
    avg_temp: f32,
) -> (f32, f32) {
    // Differential factor: how much the temperature deviates from average
    // on the day/night sides. Based on rotation period:
    //   - Very fast (<0.5 d): small differential (factor ~0.15)
    //   - Earth-like (~1 d): moderate (factor ~0.25)
    //   - Slow (>10 d): large (factor ~0.45)
    //   - Tidally locked (>50 d): extreme (factor ~0.65)
    let factor = if rotation_period_days < 0.3 {
        0.12
    } else if rotation_period_days < 2.0 {
        0.15 + (rotation_period_days - 0.3) * 0.06 // ~0.15 to ~0.25
    } else if rotation_period_days < 20.0 {
        0.25 + (rotation_period_days - 2.0) * 0.011 // ~0.25 to ~0.45
    } else if rotation_period_days < 100.0 {
        0.45 + (rotation_period_days - 20.0) * 0.0025 // ~0.45 to ~0.65
    } else {
        0.65 // tidally locked / very slow
    };

    // Convert average temperature to Kelvin, apply factor, convert back
    let avg_k = avg_temp + 273.15;
    let min_k = avg_k * (1.0 - factor);
    let max_k = avg_k * (1.0 + factor);
    ((min_k - 273.15), (max_k - 273.15))
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

    debug!(
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
    let (equilibrium_temp_c, min_temp, max_temp) =
        calculate_temperature_from_star(planet_data.semi_major_axis_au as f64, star_luminosity_sol);
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
        (*surface_temp, true)
    } else {
        (equilibrium_temp_c, false)
    };

    debug!(
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
    let cat_seed: u32 = planet_data
        .name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let category = classify_exoplanet_with_mass(BodyType::Planet, None, avg_temp, cat_seed, false, false, Some(mass_kg));

    // Generate a reasonable rotation period for confirmed exoplanets (no observational data)
    let is_gas_giant = mass_earth > 10.0;
    let sma = planet_data.semi_major_axis_au as f64;
    let rotation_period_days = if is_gas_giant {
        rng.random_range(0.3..0.9_f32)
    } else if sma < 0.15 {
        planet_data.period_days // tidally locked
    } else if sma < 0.3 {
        rng.random_range(10.0..60.0_f32)
    } else {
        let log_p = rng.random_range((-0.5_f32)..(1.0));
        10.0_f32.powf(log_p)
    };
    let rotation_speed = if rotation_period_days != 0.0 {
        (2.0 * std::f32::consts::PI) / (rotation_period_days.abs() * 86400.0)
    } else {
        0.0
    };
    let axial_tilt_deg = rng.random_range(0.0_f32..1.0).powf(1.5) * 45.0;

    // Adjust temperature range based on rotation for airless bodies
    let (adj_min, adj_max) = if has_atmosphere {
        (min_temp, max_temp)
    } else {
        adjust_temperature_for_rotation(rotation_period_days, min_temp, max_temp, avg_temp)
    };

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
            average_celsius: avg_temp,
            min_celsius: adj_min,
            max_celsius: adj_max,
        },
        PlanetCategory(category.to_string()),
        orbit,
        RotationSpeed(rotation_speed),
        OrbitPath::new(Color::srgba(0.4, 0.75, 1.0, 0.7)), // Cyan/blue — matches Sol palette
        SpaceCoordinates::default(),                       // Will be updated by propagate_orbits
        OrbitCenter(parent_star), // Link to parent star for orbital hierarchy
        OrbitsBody::new(parent_star),
        LogicalParent(parent_star),
        SystemId(system_id),
        Transform::default(), // Required so ring ChildOf relationships have a valid parent transform
        // Visibility required so that child entities (rings, atmosphere shells) have a
        // valid InheritedVisibility propagation chain — prevents Bevy B0004 warnings.
        // Hidden by default since these bodies are in distant systems and have no mesh yet.
        Visibility::Hidden,
    ));
    entity_commands.insert(AxialTilt {
        obliquity: axial_tilt_deg.to_radians(),
        north_pole_ra: rng.random_range(0.0..std::f32::consts::TAU),
    });

    // Extract ocean-relevant info before consuming atmosphere_result
    let pressure_mbar = atmosphere_result
        .as_ref()
        .map(|(a, _)| a.surface_pressure_mbar)
        .unwrap_or(0.0);

    // Add atmosphere if generated
    if let Some((atmosphere, _)) = atmosphere_result {
        entity_commands.insert(atmosphere);
    }

    // Infer ocean from temperature and atmosphere
    if let Some(ocean) = infer_ocean_properties(avg_temp, pressure_mbar, true, false, radius_km) {
        entity_commands.insert(ocean);
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
    let (equilibrium_temp_c, min_temp, max_temp) =
        calculate_temperature_from_star(planet.semi_major_axis_au, star_luminosity_sol);
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
        (*surface_temp, true)
    } else {
        (equilibrium_temp_c, false)
    };

    debug!(
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
    let cat_seed: u32 = planet
        .name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let category = classify_exoplanet_with_mass(planet.body_type(), None, avg_temp, cat_seed, false, false, Some(mass_kg));

    // Calculate rotation speed from period (same formula as solar_system.rs)
    let rotation_speed = if planet.rotation_period_days != 0.0 {
        (2.0 * std::f32::consts::PI) / (planet.rotation_period_days.abs() * 86400.0)
    } else {
        0.0
    };

    // Adjust temperature range based on rotation:
    // Fast rotators have smaller day/night differentials; tidally locked have extreme ones.
    let (adj_min, adj_max) = if has_atmosphere {
        // Atmospheres redistribute heat regardless of rotation
        (min_temp, max_temp)
    } else {
        adjust_temperature_for_rotation(planet.rotation_period_days, min_temp, max_temp, avg_temp)
    };

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
            average_celsius: avg_temp,
            min_celsius: adj_min,
            max_celsius: adj_max,
        },
        PlanetCategory(category.to_string()),
        orbit,
        RotationSpeed(rotation_speed),
        AxialTilt {
            obliquity: planet.axial_tilt_deg.to_radians(),
            north_pole_ra: rng.random_range(0.0..std::f32::consts::TAU),
        },
        OrbitPath::new(Color::srgba(0.4, 0.75, 1.0, 0.6)), // Cyan/blue — procedural planets
        SpaceCoordinates::default(),                       // Will be updated by propagate_orbits
        OrbitCenter(parent_star), // Link to parent star for orbital hierarchy
        OrbitsBody::new(parent_star),
        LogicalParent(parent_star),
        SystemId(system_id),
        Transform::default(), // Required so ring ChildOf relationships have a valid parent transform
        // Visibility required so that child entities (rings, atmosphere shells) have a
        // valid InheritedVisibility propagation chain — prevents Bevy B0004 warnings.
        // Hidden by default since these bodies are in distant systems and have no mesh yet.
        Visibility::Hidden,
    ));

    // Extract ocean-relevant info before consuming atmosphere_result
    let pressure_mbar = atmosphere_result
        .as_ref()
        .map(|(a, _)| a.surface_pressure_mbar)
        .unwrap_or(0.0);
    let has_methane = atmosphere_result
        .as_ref()
        .and_then(|(a, _)| a.gases.iter().find(|g| g.name == "CH4"))
        .map(|g| g.percentage > 0.5)
        .unwrap_or(false);

    // Add atmosphere if generated
    if let Some((atmosphere, _)) = atmosphere_result {
        entity_commands.insert(atmosphere);
    }

    // Infer ocean from temperature and atmosphere
    if let Some(ocean) =
        infer_ocean_properties(avg_temp, pressure_mbar, true, has_methane, radius_km)
    {
        entity_commands.insert(ocean);
    }

    let entity = entity_commands.id();

    // Resource generation will be handled by the existing system
    // The metallicity_multiplier will be applied in the resource generation

    entity
}

/// Spawn procedural dwarf planets in the trans-Neptunian region.
///
/// Dwarf planets are spawned with `BodyType::DwarfPlanet` and the `DwarfPlanet`
/// marker component. Their orbits are hidden by default (matching Sol behaviour).
fn spawn_dwarf_planets(
    commands: &mut Commands,
    dwarf_planets: &[ProceduralPlanet],
    parent_star: Entity,
    system_id: usize,
    star_luminosity_sol: f32,
    vis_scale: f32,
) {
    for planet in dwarf_planets {
        let orbit = planet.to_kepler_orbit();
        let mass_kg = planet.mass_kg();
        let radius_km = planet.radius_km();

        let (avg_temp, min_temp, max_temp) =
            calculate_temperature_from_star(planet.semi_major_axis_au, star_luminosity_sol);

        let visual_radius = calculate_visual_radius(BodyType::DwarfPlanet, radius_km) * vis_scale;

        debug!(
            "Spawning dwarf planet '{}': a={:.1}AU, e={:.2}, i={:.1}°, M={:.4}M⊕, R={:.0}km",
            planet.name,
            planet.semi_major_axis_au,
            planet.eccentricity,
            planet.inclination.to_degrees(),
            planet.mass_earth,
            radius_km,
        );

        // Calculate rotation speed from period
        let rotation_speed = if planet.rotation_period_days != 0.0 {
            (2.0 * std::f32::consts::PI) / (planet.rotation_period_days.abs() * 86400.0)
        } else {
            0.0
        };

        commands.spawn((
            DwarfPlanet,
            CelestialBody {
                name: planet.name.clone(),
                mass: mass_kg,
                radius: radius_km,
                body_type: BodyType::DwarfPlanet,
                visual_radius,
                asteroid_class: None,
            },
            SurfaceTemperature {
                average_celsius: avg_temp,
                min_celsius: min_temp,
                max_celsius: max_temp,
            },
            orbit,
            RotationSpeed(rotation_speed),
            AxialTilt {
                obliquity: planet.axial_tilt_deg.to_radians(),
                north_pole_ra: 0.0,
            },
            OrbitPath::new(Color::srgba(0.5, 0.5, 0.7, 0.5)), // Dim blue — matches Sol dwarf planet palette
            SpaceCoordinates::default(),
            OrbitCenter(parent_star),
            OrbitsBody::new(parent_star),
            LogicalParent(parent_star),
            SystemId(system_id),
            Visibility::Hidden,
        ));
    }
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

    debug!(
        "Spawning asteroid belt: {:.2}-{:.2} AU, {} asteroids",
        belt.inner_au, belt.outer_au, belt.count
    );

    for i in 0..belt.count {
        // Random orbital parameters within the belt
        let semi_major_axis = rng.random_range(belt.inner_au..belt.outer_au);

        // Eccentricity: most main-belt asteroids 0.0-0.3, median ~0.15
        // Power-law bias toward lower values
        let eccentricity = rng.random_range(0.0_f64..1.0).powf(1.5) * 0.35;

        // Inclination: real belt has 0-30° with most <15°
        // Rayleigh-like distribution centred on belt average
        let base_incl = belt.inclination;
        let incl_spread = rng.random_range(0.0_f64..1.0).powf(0.7) * 0.52; // up to ~30°
        let incl_sign = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let inclination = base_incl + incl_sign * incl_spread;

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

        // Determine asteroid class with realistic distribution:
        // C-type (carbonaceous): ~75% of all asteroids (dominant in outer belt)
        // S-type (silicate): ~17% of all asteroids (dominant in inner belt)
        // M-type (metallic): ~5% (scattered)
        // V-type (basaltic): ~3% (associated with Vesta family)
        let belt_midpoint = (belt.inner_au + belt.outer_au) * 0.5;
        let asteroid_class = if semi_major_axis > belt_midpoint {
            // Outer belt: C-type dominant
            let roll = rng.random_range(0.0..1.0_f64);
            if roll < 0.80 {
                AsteroidClass::CType
            } else if roll < 0.92 {
                AsteroidClass::SType
            } else if roll < 0.97 {
                AsteroidClass::MType
            } else {
                AsteroidClass::VType
            }
        } else {
            // Inner belt: S-type more common
            let roll = rng.random_range(0.0..1.0_f64);
            if roll < 0.45 {
                AsteroidClass::SType
            } else if roll < 0.80 {
                AsteroidClass::CType
            } else if roll < 0.93 {
                AsteroidClass::MType
            } else {
                AsteroidClass::VType
            }
        };

        // Power-law size distribution: N(>D) ∝ D^-2.5
        // Most asteroids are tiny, a few are large (Ceres 473km, Vesta 263km)
        // Inverse CDF: r = r_min × (1 - U)^(-1/(q-1)) where q = 3.5
        let u: f64 = rng.random_range(0.001..1.0); // avoid zero
        let r_min = 0.1_f64; // minimum radius in km
        let r_max = 250.0_f64; // maximum radius (Ceres-scale)
        let q = 3.5; // power-law exponent
        let radius_raw = r_min * (1.0 - u).powf(-1.0 / (q - 1.0));
        let radius = radius_raw.min(r_max);

        // Density varies by class (kg/m³)
        // C-type: 1300-2100 (porous, carbonaceous, like Mathilde ~1300)
        // S-type: 2400-3200 (rocky, stony, like Eros ~2670)
        // M-type: 3500-5500 (metallic, like Psyche ~3400-4100)
        // V-type: 2800-3500 (basaltic, like Vesta ~3456)
        let density = match asteroid_class {
            AsteroidClass::CType => rng.random_range(1300.0..2100.0_f64),
            AsteroidClass::SType => rng.random_range(2400.0..3200.0_f64),
            AsteroidClass::MType => rng.random_range(3500.0..5500.0_f64),
            AsteroidClass::VType => rng.random_range(2800.0..3500.0_f64),
            _ => rng.random_range(2000.0..3000.0_f64),
        };
        let mass = (4.0 / 3.0) * std::f64::consts::PI * (radius * 1000.0).powi(3) * density;

        // Calculate asteroid temperature based on its distance from the star
        let (avg_temp, min_temp, max_temp) =
            calculate_temperature_from_star(semi_major_axis, star_luminosity_sol);

        commands.spawn((
            Asteroid,
            CelestialBody {
                name: format!("{} Belt Asteroid {}", star_name, i + 1),
                mass,
                radius: radius as f32,
                body_type: BodyType::Asteroid,
                visual_radius: calculate_visual_radius(BodyType::Asteroid, radius as f32)
                    * vis_scale,
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

    debug!(
        "Spawning cometary cloud: {:.2}-{:.2} AU, {} comets",
        cloud.inner_au, cloud.outer_au, cloud.count
    );

    for i in 0..cloud.count {
        // Random orbital parameters within the cloud (spherical distribution)
        let semi_major_axis = rng.random_range(cloud.inner_au..cloud.outer_au);

        // Eccentricity: short-period comets 0.2-0.7, long-period comets 0.9-0.999
        // Mix of populations: ~70% short/intermediate period, ~30% near-parabolic
        let eccentricity = if rng.random_range(0.0..1.0_f64) < 0.3 {
            // Long-period / near-parabolic comets (Oort cloud origin)
            rng.random_range(0.90..0.999_f64)
        } else {
            // Short/intermediate period comets (scattered disk/Kuiper belt origin)
            rng.random_range(0.2..0.75_f64)
        };

        // Inclination: isotropic for long-period, concentrated for short-period
        // Short-period (Jupiter family): mostly <30°
        // Long-period: isotropic (0-180°)
        let inclination = if eccentricity > 0.85 {
            // Near-isotropic: uniform in cos(i)
            let cos_i = rng.random_range(-1.0..1.0_f64);
            cos_i.acos()
        } else {
            // Jupiter-family-like: concentrated near ecliptic
            rng.random_range(0.0_f64..1.0).powf(0.6) * 0.52 // up to ~30°, biased low
        };

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

        // Power-law size distribution for comets (steeper than asteroids)
        // Most comets are 0.5-5 km, a few reach 30+ km (Hale-Bopp ~30km, Chiron ~100km)
        let u: f64 = rng.random_range(0.001..1.0);
        let r_min = 0.3_f64;
        let r_max = 40.0_f64;
        let q = 4.0; // steeper than asteroids
        let radius_raw = r_min * (1.0 - u).powf(-1.0 / (q - 1.0));
        let radius = radius_raw.min(r_max);

        // Density: cometary nuclei are porous ice/rock mixtures
        // 67P/Churyumov–Gerasimenko: 533 kg/m³, Halley: ~600 kg/m³
        // Range: 200-800 kg/m³
        let density = rng.random_range(200.0..800.0_f64);
        let mass = (4.0 / 3.0) * std::f64::consts::PI * (radius * 1000.0).powi(3) * density;

        // Calculate comet temperature based on its distance from the star
        let (avg_temp, min_temp, max_temp) =
            calculate_temperature_from_star(semi_major_axis, star_luminosity_sol);

        commands.spawn((
            Comet,
            CelestialBody {
                name: format!("{} Cloud Comet {}", star_name, i + 1),
                mass,
                radius: radius as f32,
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
/// Generates a 1-D radial RGBA texture with multi-scale ring structure:
/// broad regions (analogous to Saturn's A/B/C rings), dozens of fine ringlets,
/// and multiple gaps of varying widths.  Colours are muted and realistic —
/// icy greys, warm beiges, and cool slate tones — matching Cassini/Voyager
/// imagery.  The texture is mapped across the U coordinate of the ring mesh
/// (0 = inner edge, 1 = outer edge).
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
    // Inner edge: 1.2–1.6× planet radius — guarantees a visible gap from the
    // planet surface (Saturn's main rings start at ~1.23 R, Uranus at ~1.60 R).
    // Previously inner_radius was derived from outer_radius × fraction, which
    // could place the inner edge inside the planet (0.45 × 1.6 = 0.72 R).
    let inner_scale: f32 = rng.random_range(1.2_f32..1.6);
    let inner_radius = planet_visual_radius * inner_scale;
    // Outer edge: 1.5–2.5× the inner radius → total span ~1.8–4.0× planet radius.
    let outer_scale: f32 = rng.random_range(1.5_f32..2.5);
    let outer_radius = inner_radius * outer_scale;

    // ── Ring flavor (base colour palette) ─────────────────────────────────
    // All palettes are muted and desaturated — real ring particles are
    // predominantly water ice (grey-white) with silicate/tholin impurities
    // adding subtle warm or cool tints.
    let flavor: u32 = rng.random_range(0..100);
    // (inner_rgb, outer_rgb) — colour lerps from inner to outer
    let (inner_rgb, outer_rgb, flavor_label): ([f32; 3], [f32; 3], &str) = if flavor < 30 {
        // Saturn-like: warm cream to muted tan (Cassini imagery)
        ([0.88, 0.84, 0.76], [0.76, 0.70, 0.58], "Saturn-warm")
    } else if flavor < 55 {
        // Uranus-like: pale blue-grey to cool slate
        ([0.72, 0.76, 0.80], [0.50, 0.55, 0.60], "Uranus-slate")
    } else if flavor < 75 {
        // Icy white: bright ice to cool grey (fresh ice-dominated rings)
        ([0.90, 0.91, 0.92], [0.72, 0.74, 0.78], "Icy-white")
    } else if flavor < 90 {
        // Dusty warm: very subtle warm grey with a hint of tan (tholin-stained)
        ([0.80, 0.76, 0.70], [0.62, 0.58, 0.52], "Dusty-warm")
    } else {
        // Dark tenuous: barely-visible sooty particles (Jupiter-like)
        ([0.40, 0.38, 0.35], [0.25, 0.23, 0.20], "Dark-faint")
    };

    // Per-ring colour jitter (very subtle — keeps neighbouring rings unique)
    let jitter_r = rng.random_range(-0.03_f32..0.03);
    let jitter_g = rng.random_range(-0.03_f32..0.03);
    let jitter_b = rng.random_range(-0.03_f32..0.03);

    // ── Generate procedural ring texture ─────────────────────────────────
    // 1024 pixels wide (U direction = radial), 1 pixel tall.
    // Higher resolution captures the fine ringlet structure.
    const TEX_W: u32 = 1024;
    const TEX_H: u32 = 1;

    // ── Multi-scale structure ─────────────────────────────────────────────
    // Level 1: Major regions (like Saturn's C/B/A rings) — 2-4 broad zones
    // that define the coarse opacity envelope.
    let num_major_regions: usize = rng.random_range(2..=4);
    struct MajorRegion {
        center: f32,
        half_width: f32,
        base_opacity: f32,
    }
    let major_regions: Vec<MajorRegion> = {
        // Place regions roughly evenly across the radial range with some randomness
        let spacing = 1.0 / (num_major_regions as f32 + 1.0);
        (0..num_major_regions)
            .map(|i| {
                let nominal = spacing * (i as f32 + 1.0);
                MajorRegion {
                    center: nominal + rng.random_range(-0.06_f32..0.06),
                    half_width: rng.random_range(0.10_f32..0.22),
                    base_opacity: rng.random_range(0.30_f32..0.90),
                }
            })
            .collect()
    };

    // Level 2: Fine ringlets — 25-60 narrow bands within the major regions
    let num_ringlets: usize = rng.random_range(25..=60);
    struct Ringlet {
        center: f32,
        sigma: f32,
        peak: f32,
    }
    let ringlets: Vec<Ringlet> = (0..num_ringlets)
        .map(|_| Ringlet {
            center: rng.random_range(0.02_f32..0.98),
            sigma: rng.random_range(0.004_f32..0.030),
            peak: rng.random_range(0.15_f32..0.70),
        })
        .collect();

    // Level 3: Gaps — mix of major divisions and narrow gaps
    let num_major_gaps: usize = rng.random_range(1..=3);
    let num_narrow_gaps: usize = rng.random_range(3..=10);

    struct GapInfo {
        center: f32,
        half_w: f32,
        sharpness: f32,
    }
    let mut gaps: Vec<GapInfo> = Vec::with_capacity(num_major_gaps + num_narrow_gaps);
    // Major gaps — wider divisions with sharp edges (Cassini Division analog)
    for _ in 0..num_major_gaps {
        gaps.push(GapInfo {
            center: rng.random_range(0.15_f32..0.85),
            half_w: rng.random_range(0.02_f32..0.06),
            sharpness: rng.random_range(0.7_f32..1.0),
        });
    }
    // Narrow gaps — hairline splits (Encke Gap analog)
    for _ in 0..num_narrow_gaps {
        gaps.push(GapInfo {
            center: rng.random_range(0.05_f32..0.95),
            half_w: rng.random_range(0.002_f32..0.012),
            sharpness: rng.random_range(0.5_f32..0.9),
        });
    }

    // Deterministic fine noise for density variations (no RNG per pixel)
    #[inline]
    fn fine_noise(x: u32, seed: u32) -> f32 {
        let h = x
            .wrapping_mul(2654435761)
            .wrapping_add(seed.wrapping_mul(2246822519));
        let h = ((h >> 13) ^ h).wrapping_mul(1597334677);
        (h & 0xFFFF) as f32 / 32768.0 - 1.0
    }
    let noise_seed: u32 = rng.random_range(0..u32::MAX);

    let mut pixels = Vec::with_capacity((TEX_W as usize) * 4);
    for x in 0..TEX_W {
        let u = x as f32 / (TEX_W - 1) as f32; // 0 = inner, 1 = outer

        // ── Colour ───────────────────────────────────────────────────────
        let mut cr = (inner_rgb[0] + (outer_rgb[0] - inner_rgb[0]) * u) + jitter_r;
        let mut cg = (inner_rgb[1] + (outer_rgb[1] - inner_rgb[1]) * u) + jitter_g;
        let mut cb = (inner_rgb[2] + (outer_rgb[2] - inner_rgb[2]) * u) + jitter_b;

        // Subtle per-pixel colour variation (tholin/silicate patches)
        let colour_noise = fine_noise(x, noise_seed.wrapping_add(7)) * 0.04;
        cr = (cr + colour_noise).clamp(0.0, 1.0);
        cg = (cg + colour_noise * 0.7).clamp(0.0, 1.0);
        cb = (cb + colour_noise * 0.3).clamp(0.0, 1.0);

        // ── Alpha: composite from major regions + fine ringlets ──────────
        // Start with the major region envelope
        let mut region_alpha: f32 = 0.0;
        for region in &major_regions {
            let d = (u - region.center) / region.half_width;
            // Soft-edged trapezoid: flat top (|d| < 0.6), smooth falloff beyond
            let contribution = if d.abs() < 0.6 {
                region.base_opacity
            } else {
                let falloff = ((1.0 - d.abs()) / 0.4).clamp(0.0, 1.0);
                region.base_opacity * falloff * falloff
            };
            region_alpha = region_alpha.max(contribution);
        }

        // Add fine ringlet structure on top
        let mut ringlet_alpha: f32 = 0.0;
        for ringlet in &ringlets {
            let d = (u - ringlet.center) / ringlet.sigma;
            ringlet_alpha += ringlet.peak * (-0.5 * d * d).exp();
        }

        // Combine: ringlets modulate within the region envelope
        let mut alpha =
            region_alpha * 0.4 + ringlet_alpha * 0.55 + (region_alpha * ringlet_alpha) * 0.3;

        // ── Cut gaps ─────────────────────────────────────────────────────
        for gap in &gaps {
            let dist = (u - gap.center).abs();
            if dist < gap.half_w {
                let edge_zone = gap.half_w * (1.0 - gap.sharpness);
                let edge_dist = gap.half_w - dist;
                let gap_factor = if edge_zone > 0.0001 && edge_dist > edge_zone {
                    0.0
                } else if edge_zone > 0.0001 {
                    (edge_dist / edge_zone).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                alpha *= gap_factor;
            }
        }

        // ── Edge fade (soft inner/outer boundaries) ──────────────────────
        let inner_fade = (u * 12.0).min(1.0);
        let outer_fade = ((1.0 - u) * 12.0).min(1.0);
        alpha *= inner_fade * outer_fade;

        // ── Fine-grain density noise ─────────────────────────────────────
        let density_noise = fine_noise(x, noise_seed) * 0.06;
        alpha = (alpha + density_noise).clamp(0.0, 1.0);

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
    let mat_handle = materials.add(StandardMaterial {
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

    debug!(
        "  Spawned rings for '{}' (outer={:.1}, inner={:.1}, regions={}, ringlets={}, gaps={}, flavor={})",
        planet_name, outer_radius, inner_radius, num_major_regions, num_ringlets,
        gaps.len(), flavor_label,
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
    planet_radius_km: f32,
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
    let mut moon_count: u32 = if planet_mass_earth > 50.0 {
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

    // ========================================================================
    // HILL SPHERE CAP: reduce moon count if the Hill sphere is too small to
    // fit the requested number of moons without them overlapping or orbiting
    // inside the planet.  The innermost regular moon must orbit beyond the
    // planet's Roche limit (~2.5× planet radius).
    // ========================================================================
    let planet_radius_m = planet_radius_km as f64 * 1000.0;
    let roche_limit_au = (2.5 * planet_radius_m) / 1.496e11; // ~2.5 R_planet
    let hill_radius_au =
        planet_sma_au * ((planet_mass_earth as f64) * 5.972e24 / (3.0 * 1.989e30)).powf(1.0 / 3.0);
    let regular_outer_au = (hill_radius_au * 0.05).max(0.0005);

    // If the regular moon zone can't even fit outside the Roche limit,
    // reduce moon count drastically or skip moons entirely.
    if regular_outer_au < roche_limit_au * 2.0 {
        // Barely any room — at most 1 irregular moon
        moon_count = moon_count.min(1);
    } else {
        // Estimate how many moons can fit with geometric spacing above the
        // Roche limit.  Each moon needs ~1.3× radial separation from the next.
        let usable_range = (regular_outer_au / roche_limit_au).max(1.0);
        let max_fitting = (usable_range.ln() / 1.3_f64.ln()).floor() as u32 + 1;
        moon_count = moon_count.min(max_fitting.max(1));
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

    // Approximate Hill sphere radius in AU: r_H ≈ a × (M_planet / 3·M_star)^(1/3)
    // Use 1 M☉ as a reasonable default for the parent star.
    let hill_radius_au =
        planet_sma_au * ((planet_mass_earth as f64) * 5.972e24 / (3.0 * 1.989e30)).powf(1.0 / 3.0);

    // Regular moons orbit within ~0.05 Hill radii (like Galilean system),
    // irregular moons extend to ~0.4 Hill radii (like Jupiter's outer groups).
    // Ensure innermost moon is always beyond the Roche limit (~2.5 planet radii).
    let regular_inner_au = roche_limit_au.max(regular_outer_au * 0.15);
    let irregular_outer_au = (hill_radius_au * 0.40).max(regular_outer_au * 3.0);

    // Pre-compute all moon orbital distances, then sort & deduplicate to
    // guarantee no crossing orbits.
    let mut moon_distances: Vec<(f64, bool)> = Vec::with_capacity(moon_count as usize);
    for i in 0..moon_count {
        // Classify moon population: inner ~60% are regular, rest irregular.
        let regular_fraction = 0.6;
        let is_regular = (i as f64) < (moon_count as f64 * regular_fraction);

        let orbital_distance_au = if is_regular {
            // Logarithmic spacing: inner_au × ratio^i, like Galilean resonances
            let ratio = if moon_count > 1 {
                (regular_outer_au / regular_inner_au).powf(1.0 / (moon_count as f64 - 1.0).max(1.0))
            } else {
                1.0
            };
            let base = regular_inner_au * ratio.powf(i as f64);
            // ±15% jitter (reduced from 25% to prevent orbit crossings)
            (base * rng.random_range(0.85..1.15_f64)).max(roche_limit_au)
        } else {
            // Irregular moons: log-uniform scatter in the outer Hill sphere
            let log_inner = regular_outer_au.ln();
            let log_outer = irregular_outer_au.ln();
            (log_inner + rng.random_range(0.3..1.0_f64) * (log_outer - log_inner)).exp()
        };
        moon_distances.push((orbital_distance_au, is_regular));
    }

    // Sort by distance and enforce minimum separation (each moon must be at
    // least 10% farther than the previous one to prevent visual overlap).
    moon_distances.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for i in 1..moon_distances.len() {
        let min_distance = moon_distances[i - 1].0 * 1.10;
        if moon_distances[i].0 < min_distance {
            moon_distances[i].0 = min_distance;
        }
    }

    for (i, &(orbital_distance_au, is_regular)) in moon_distances.iter().enumerate() {

        // --- Mass (log-uniform for realistic spread across orders of magnitude) ---
        // Real examples: Ganymede 0.025% of Jupiter, Deimos 0.000000025% of Mars,
        //                Earth's Moon 1.2% of Earth, Phobos 0.00000018% of Mars
        let log_mass_fraction = if planet_mass_earth > 10.0 {
            // Gas/ice giants: 10^-6 to 10^-3 of planet mass
            // Covers everything from tiny captured rocks to Ganymede-class moons
            rng.random_range(-6.0..-3.0_f64)
        } else {
            // Rocky planets: 10^-4.5 to 10^-1.5 (wider range)
            // From Phobos-like specks to Luna-class companions
            rng.random_range(-4.5..-1.5_f64)
        };
        let mass_fraction = 10.0_f64.powf(log_mass_fraction);

        // Irregular moons tend to be smaller (captured bodies)
        let mass_fraction = if is_regular {
            mass_fraction
        } else {
            mass_fraction * rng.random_range(0.01..0.3_f64)
        };

        let moon_mass_earth = (planet_mass_earth as f64) * mass_fraction;
        let moon_mass_kg = moon_mass_earth * 5.972e24;

        // --- Density varies with composition ---
        // Inner moons: rocky/metallic (Io: 3528, Europa: 3013, Moon: 3346)
        // Outer moons: increasingly icy (Ganymede: 1936, Callisto: 1834, Enceladus: 1609)
        // Irregulars: low-density captured bodies (~1300-2000)
        let density_kg_m3 = if is_regular {
            let t = i as f64 / (moon_distances.len() as f64).max(1.0);
            // Blend from rocky-inner (~3400) to icy-outer (~1800)
            let base_density = 3400.0 - t * 1600.0;
            base_density * rng.random_range(0.85..1.15_f64)
        } else {
            // Irregular: predominantly icy/porous (1100-2000)
            rng.random_range(1100.0..2000.0_f64)
        };

        let volume_m3 = moon_mass_kg / density_kg_m3;
        let radius_m = (volume_m3 * 3.0 / (4.0 * std::f64::consts::PI)).powf(1.0 / 3.0);
        let radius_km = (radius_m / 1000.0) as f32;

        // Orbital period from parent planet's mass (Kepler's third law)
        let parent_mass_kg = (planet_mass_earth as f64) * 5.972e24;
        let g = 6.674e-11;
        let sma_m = orbital_distance_au * 1.496e11;
        let period_s = std::f64::consts::TAU * (sma_m.powi(3) / (g * parent_mass_kg)).sqrt();
        let mean_motion = std::f64::consts::TAU / period_s;

        // --- Inclination ---
        // Regular moons: near-coplanar (<5°), like Io 0.05°, Europa 0.47°, Titan 0.35°
        // Irregular moons: highly inclined, some retrograde
        //   - Jupiter's Himalia group ~28°, Carme group ~165° (retrograde)
        let inclination_rad = if is_regular {
            rng.random_range(-0.09..0.09_f64) // ±~5°
        } else if rng.random_range(0.0..1.0_f64) < 0.3 {
            // ~30% of irregulars are retrograde (130-170°)
            rng.random_range(2.27..2.97_f64)
        } else {
            // Prograde irregular: 15-55°
            rng.random_range(0.26..0.96_f64)
        };

        // --- Eccentricity ---
        // Regular: very circular (Io 0.004, Europa 0.009, Titan 0.029)
        // Irregular: moderate to high (Himalia 0.16, Pasiphae 0.41, Nereid 0.75)
        let eccentricity = if is_regular {
            rng.random_range(0.0..0.03_f64)
        } else {
            rng.random_range(0.1..0.55_f64)
        };

        let orbit = KeplerOrbit::new(
            eccentricity,
            orbital_distance_au,
            inclination_rad,
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
            mean_motion,
        );

        // Compute orbit amplification so moons render outside the parent mesh
        let orbit_bevy = orbital_distance_au * SCALING_FACTOR;
        let total_moons = moon_distances.len();
        let amp = if total_moons == 1 {
            let mid_display = (inner_display + outer_display) * 0.5;
            (mid_display / orbit_bevy).max(1.0) as f32
        } else {
            let t = i as f64 / (total_moons - 1) as f64;
            let display_distance = inner_display + t * (outer_display - inner_display);
            (display_distance / orbit_bevy).max(1.0) as f32
        };

        let moon_name = format!("{} {}", planet_name, to_roman(i as u32 + 1));

        // Calculate moon temperature using parent planet's distance from star
        // (moons orbit the planet, but their temperature depends on their distance from the star)
        let (avg_temp, min_temp, max_temp) =
            calculate_temperature_from_star(planet_sma_au, star_luminosity_sol);

        // Cap moon visual radius relative to the parent planet's visual size.
        // Gas/ice giant moons are capped tighter (15%) because real moons like
        // Ganymede are only ~4% of Jupiter's radius.  Rocky planet moons allow up
        // to 25% (Earth's Moon is ~27% physically, but non-linear scaling inflates
        // the ratio).  Without this cap the shared MIN_VISUAL_RADIUS floor can
        // make small moons appear as large as their parent.
        let max_moon_ratio = if planet_mass_earth > 10.0 { 0.15 } else { 0.25 };
        let moon_visual_radius = (calculate_visual_radius(BodyType::Moon, radius_km) * vis_scale)
            .min(parent_visual_radius * max_moon_ratio);

        // Ensure the moon radius isn't absurdly large relative to the parent
        // A moon should not be larger than a small fraction of the parent's actual physical radius (e.g. Ganymede is small compared to Jupiter)
        // Earth's moon is an outlier but it's generated differently
        let physical_max_moon_ratio = if planet_mass_earth > 10.0 { 0.10 } else { 0.35 };
        let radius_km = radius_km.min(planet_radius_km * physical_max_moon_ratio);

        // Recompute mass based on the clamped radius, so density remains somewhat consistent
        let volume_m3_clamped =
            4.0 / 3.0 * std::f64::consts::PI * (radius_km as f64 * 1000.0).powi(3);
        let moon_mass_kg = volume_m3_clamped * density_kg_m3;

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
            OrbitPath::new(Color::srgba(0.7, 0.7, 0.7, 0.5)),
            SpaceCoordinates::default(),
            OrbitCenter(planet_entity),
            OrbitsBody::new(planet_entity),
            LogicalParent(planet_entity),
            LocalOrbitAmplification(amp),
            SystemId(system_id),
        ));
    }

    if !moon_distances.is_empty() {
        let spawned = moon_distances.len();
        debug!(
            "  Spawned {} moons for {} at {:.2} AU (orbit amp: {:.1}x-{:.1}x)",
            spawned,
            planet_name,
            planet_sma_au,
            (inner_display / (0.001 * SCALING_FACTOR)).max(1.0),
            (outer_display / ((0.001 + (spawned as f64 - 1.0) * 0.002) * SCALING_FACTOR))
                .max(1.0),
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
