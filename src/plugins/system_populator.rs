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
use std::collections::{HashMap, HashSet};

use crate::astronomy::components::{CurrentStarSystem, OrbitCenter, SystemId};
use crate::astronomy::exoplanets::RealPlanet;
use crate::astronomy::infer_ocean_properties;
use crate::astronomy::nearby_stars::load_nearby_stars_data;
use crate::astronomy::nearby_stars::{BinaryOrbitData, NearbyStarsData, PlanetData, StarData};
use crate::astronomy::{
    calculate_frost_line, generate_procedural_atmosphere, map_star_to_system_architecture,
    AsteroidBelt, CometaryCloud, KeplerOrbit, LocalOrbitAmplification, OrbitPath,
    ProceduralPlanet, SpaceCoordinates, StellarProperties, SurfaceTemperature, SCALING_FACTOR,
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

#[derive(Clone, Copy)]
struct OrbitParentLink {
    spatial_parent: Entity,
    logical_parent: Entity,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OrbitBodyRef {
    Star(usize),
    OrbitLabel(String),
}

#[derive(Clone)]
struct ResolvedOrbitBody {
    mass_sol: f64,
    luminosity_sol: f32,
    metallicity: f32,
    representative_star_idx: usize,
}

struct SpawnedPlanetSummary {
    entity: Entity,
    semi_major_axis_au: f64,
    mass_earth: f32,
    visual_radius: f32,
    radius_km: f32,
    name: String,
}

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

        if system_data.stars.is_empty() {
            continue;
        }

        let system_barycenter = spawn_orbit_anchor(&mut commands, system_id, star_position);
        let mut star_entities = vec![Entity::PLACEHOLDER; system_data.stars.len()];
        let mut star_metallicities = vec![0.0_f32; system_data.stars.len()];
        let mut star_vis_scales = vec![1.0_f32; system_data.stars.len()];

        for (idx, star_data) in system_data.stars.iter().enumerate() {
            star_metallicities[idx] = star_data.metallicity.unwrap_or_else(|| {
                let random_value = rng.random_range(-0.5..0.5);
                debug!(
                    "  No metallicity data for '{}', using random: {:.2}",
                    star_data.name, random_value
                );
                random_value
            });
            star_vis_scales[idx] = system_visual_scale(star_data.luminosity_sol);
        }

        let orbit_defs_by_label: HashMap<String, &BinaryOrbitData> = system_data
            .binary_orbits
            .iter()
            .map(|orbit| (orbit.label.clone(), orbit))
            .collect();
        let orbit_anchors_by_label: HashMap<String, Entity> = system_data
            .binary_orbits
            .iter()
            .map(|orbit| {
                (
                    orbit.label.clone(),
                    spawn_orbit_anchor(&mut commands, system_id, star_position),
                )
            })
            .collect();

        let mut direct_star_parent: HashMap<usize, String> = HashMap::new();
        let mut direct_orbit_parent: HashMap<String, String> = HashMap::new();
        for orbit in &system_data.binary_orbits {
            if let Some(primary_ref) = orbit_primary_ref(orbit) {
                match primary_ref {
                    OrbitBodyRef::Star(idx) => {
                        direct_star_parent.insert(idx, orbit.label.clone());
                    }
                    OrbitBodyRef::OrbitLabel(label) => {
                        direct_orbit_parent.insert(label, orbit.label.clone());
                    }
                }
            }
            if let Some(secondary_ref) = orbit_secondary_ref(orbit) {
                match secondary_ref {
                    OrbitBodyRef::Star(idx) => {
                        direct_star_parent.insert(idx, orbit.label.clone());
                    }
                    OrbitBodyRef::OrbitLabel(label) => {
                        direct_orbit_parent.insert(label, orbit.label.clone());
                    }
                }
            }
        }

        let mut resolved_orbits: HashMap<String, ResolvedOrbitBody> = HashMap::new();
        loop {
            let mut progressed = false;
            for orbit in &system_data.binary_orbits {
                if resolved_orbits.contains_key(&orbit.label) {
                    continue;
                }
                let Some(primary_ref) = orbit_primary_ref(orbit) else {
                    continue;
                };
                let Some(secondary_ref) = orbit_secondary_ref(orbit) else {
                    continue;
                };
                let Some(primary_body) = resolve_orbit_body(
                    &primary_ref,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                ) else {
                    continue;
                };
                let Some(secondary_body) = resolve_orbit_body(
                    &secondary_ref,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                ) else {
                    continue;
                };

                resolved_orbits.insert(
                    orbit.label.clone(),
                    combine_orbit_bodies(&primary_body, &secondary_body),
                );
                progressed = true;
            }

            if resolved_orbits.len() == system_data.binary_orbits.len() || !progressed {
                break;
            }
        }

        let root_orbit_labels: Vec<String> = system_data
            .binary_orbits
            .iter()
            .map(|orbit| orbit.label.clone())
            .filter(|label| !direct_orbit_parent.contains_key(label))
            .collect();

        let unassigned_star_indices: Vec<usize> = (0..system_data.stars.len())
            .filter(|idx| !direct_star_parent.contains_key(idx))
            .collect();
        let mut fallback_star_orbits: HashMap<usize, (Entity, KeplerOrbit)> = HashMap::new();
        if root_orbit_labels.len() == 1 && unassigned_star_indices.len() == 1 {
            if let Some(root_body) = resolved_orbits.get(&root_orbit_labels[0]).cloned() {
                let star_idx = unassigned_star_indices[0];
                let star_data = &system_data.stars[star_idx];
                let root_orbit = orbit_defs_by_label
                    .get(&root_orbit_labels[0])
                    .copied()
                    .expect("root orbit must exist");
                let inner_apastron_au =
                    root_orbit.semi_major_axis_au * (1.0 + root_orbit.eccentricity);
                let (_root_anchor_orbit, star_orbit) = estimate_outer_companion_orbits(
                    &system_data.system_name,
                    inner_apastron_au,
                    root_body.mass_sol,
                    star_data.mass_sol as f64,
                );
                fallback_star_orbits.insert(star_idx, (system_barycenter, star_orbit));
            }
        }

        for (idx, star_data) in system_data.stars.iter().enumerate() {
            let mut orbit = None;
            let mut orbit_center = None;
            let mut position = star_position;

            if let Some(parent_label) = direct_star_parent.get(&idx) {
                let orbit_def = orbit_defs_by_label
                    .get(parent_label)
                    .copied()
                    .expect("orbit label should resolve");
                let primary_ref = orbit_primary_ref(orbit_def).expect("valid primary orbit ref");
                let secondary_ref =
                    orbit_secondary_ref(orbit_def).expect("valid secondary orbit ref");
                let primary_body = resolve_orbit_body(
                    &primary_ref,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                )
                .expect("primary orbit body should resolve");
                let secondary_body = resolve_orbit_body(
                    &secondary_ref,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                )
                .expect("secondary orbit body should resolve");
                let (primary_orbit, secondary_orbit) = build_binary_component_orbits(
                    orbit_def,
                    primary_body.mass_sol,
                    secondary_body.mass_sol,
                );

                orbit = Some(match primary_ref {
                    OrbitBodyRef::Star(primary_idx) if primary_idx == idx => primary_orbit,
                    _ => secondary_orbit,
                });
                orbit_center = orbit_anchors_by_label.get(parent_label).copied();
                position = DVec3::ZERO;
            } else if let Some((parent_entity, fallback_orbit)) = fallback_star_orbits.get(&idx) {
                orbit = Some(*fallback_orbit);
                orbit_center = Some(*parent_entity);
                position = DVec3::ZERO;
            } else if idx > 0 && system_data.stars.len() > 1 {
                orbit = Some(build_fallback_star_orbit(
                    &system_data.system_name,
                    idx,
                    star_data.mass_sol as f64,
                    system_data.stars[0].mass_sol as f64,
                ));
                orbit_center = Some(system_barycenter);
                position = DVec3::ZERO;
            }

            star_entities[idx] = spawn_star_entity_with_metallicity(
                &mut commands,
                star_data,
                system_id,
                position,
                star_metallicities[idx],
                orbit,
                orbit_center,
            );
        }

        for orbit in &system_data.binary_orbits {
            let anchor_entity = *orbit_anchors_by_label
                .get(&orbit.label)
                .expect("anchor should exist");
            if let Some(parent_label) = direct_orbit_parent.get(&orbit.label) {
                let parent_orbit = orbit_defs_by_label
                    .get(parent_label)
                    .copied()
                    .expect("parent orbit label should resolve");
                let parent_primary = orbit_primary_ref(parent_orbit).expect("valid primary orbit ref");
                let parent_secondary =
                    orbit_secondary_ref(parent_orbit).expect("valid secondary orbit ref");
                let primary_body = resolve_orbit_body(
                    &parent_primary,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                )
                .expect("parent primary body should resolve");
                let secondary_body = resolve_orbit_body(
                    &parent_secondary,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                )
                .expect("parent secondary body should resolve");
                let (primary_orbit, secondary_orbit) = build_binary_component_orbits(
                    parent_orbit,
                    primary_body.mass_sol,
                    secondary_body.mass_sol,
                );
                let child_is_primary = matches!(
                    &parent_primary,
                    OrbitBodyRef::OrbitLabel(label) if label == &orbit.label
                );

                commands.entity(anchor_entity).insert((
                    SpaceCoordinates::default(),
                    orbit_center_component(
                        orbit_anchors_by_label.get(parent_label).copied(),
                    ),
                    if child_is_primary {
                        primary_orbit
                    } else {
                        secondary_orbit
                    },
                ));
            } else {
                commands
                    .entity(anchor_entity)
                    .insert(SpaceCoordinates::new(star_position));
            }
        }

        let mut confirmed_planets_by_star = vec![Vec::new(); system_data.stars.len()];
        for (owner_idx, star_data) in system_data.stars.iter().enumerate() {
            for planet in &star_data.planets {
                let target_idx = planet.orbits_star.unwrap_or(owner_idx);
                if target_idx < confirmed_planets_by_star.len() {
                    confirmed_planets_by_star[target_idx].push(planet.clone());
                } else {
                    warn!(
                        "Planet '{}' in '{}' references invalid host star index {}; keeping it on '{}'",
                        planet.name, system_data.system_name, target_idx, star_data.name
                    );
                    confirmed_planets_by_star[owner_idx].push(planet.clone());
                }
            }
        }

        let mut system_max_radius_au = 10.0_f64;

        for orbit in &system_data.binary_orbits {
            let Some(primary_ref) = orbit_primary_ref(orbit) else {
                continue;
            };
            let Some(secondary_ref) = orbit_secondary_ref(orbit) else {
                continue;
            };
            let Some(primary_body) = resolve_orbit_body(
                &primary_ref,
                &system_data.stars,
                &star_metallicities,
                &resolved_orbits,
            ) else {
                continue;
            };
            let Some(secondary_body) = resolve_orbit_body(
                &secondary_ref,
                &system_data.stars,
                &star_metallicities,
                &resolved_orbits,
            ) else {
                continue;
            };

            let min_circumbinary_au =
                circumbinary_stability_limit(orbit, primary_body.mass_sol, secondary_body.mass_sol);
            if min_circumbinary_au > 200.0 {
                continue;
            }

            let host_body = resolved_orbits
                .get(&orbit.label)
                .expect("resolved orbit body should exist");
            let representative_star = star_entities[host_body.representative_star_idx];
            let representative_data = &system_data.stars[host_body.representative_star_idx];
            let vis_scale = system_visual_scale(host_body.luminosity_sol.max(0.0001));
            system_max_radius_au = system_max_radius_au.max(populate_host_bodies(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut images,
                &orbit.label,
                &system_data.system_name,
                OrbitParentLink {
                    spatial_parent: *orbit_anchors_by_label
                        .get(&orbit.label)
                        .expect("orbit anchor should exist"),
                    logical_parent: representative_star,
                },
                system_id,
                host_body.mass_sol,
                host_body.luminosity_sol,
                spectral_type_to_class(&representative_data.spectral_type),
                host_body.metallicity,
                vis_scale,
                &[],
                true,
                true,
                Some(min_circumbinary_au),
                None,
                None,
                &mut rng,
                game_seed.value,
            ));
        }

        let paired_star_indices: HashSet<usize> = direct_star_parent.keys().copied().collect();

        for (idx, star_data) in system_data.stars.iter().enumerate() {
            let confirmed_planets = &confirmed_planets_by_star[idx];
            let is_in_explicit_pair = paired_star_indices.contains(&idx);
            let should_populate_host = !is_in_explicit_pair || !confirmed_planets.is_empty();
            if !should_populate_host {
                continue;
            }

            let maximum_stable_orbit_au = direct_star_parent.get(&idx).and_then(|parent_label| {
                let orbit = orbit_defs_by_label.get(parent_label).copied()?;
                let primary_ref = orbit_primary_ref(orbit)?;
                let secondary_ref = orbit_secondary_ref(orbit)?;
                let primary_body = resolve_orbit_body(
                    &primary_ref,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                )?;
                let secondary_body = resolve_orbit_body(
                    &secondary_ref,
                    &system_data.stars,
                    &star_metallicities,
                    &resolved_orbits,
                )?;
                let (host_mass, companion_mass) = if matches!(primary_ref, OrbitBodyRef::Star(primary_idx) if primary_idx == idx) {
                    (primary_body.mass_sol, secondary_body.mass_sol)
                } else {
                    (secondary_body.mass_sol, primary_body.mass_sol)
                };
                Some(circumstellar_stability_limit(orbit, host_mass, companion_mass))
            });

            system_max_radius_au = system_max_radius_au.max(populate_host_bodies(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut images,
                &star_data.name,
                &system_data.system_name,
                OrbitParentLink {
                    spatial_parent: star_entities[idx],
                    logical_parent: star_entities[idx],
                },
                system_id,
                star_data.mass_sol as f64,
                star_data.luminosity_sol,
                spectral_type_to_class(&star_data.spectral_type),
                star_metallicities[idx],
                star_vis_scales[idx],
                confirmed_planets,
                !is_in_explicit_pair,
                !is_in_explicit_pair,
                None,
                maximum_stable_orbit_au,
                Some((star_entities[idx], star_data)),
                &mut rng,
                game_seed.value,
            ));
        }

        system_metadata.set_bounding_radius(system_id, system_max_radius_au);
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

fn orbit_center_component(parent: Option<Entity>) -> OrbitCenter {
    OrbitCenter(parent.expect("orbit parent should exist for moving anchor"))
}

fn spawn_orbit_anchor(commands: &mut Commands, system_id: usize, position: DVec3) -> Entity {
    commands
        .spawn((SpaceCoordinates::new(position), SystemId(system_id)))
        .id()
}

fn stable_unit_interval(seed: &str, salt: u64) -> f64 {
    let hash = seed.bytes().fold(salt, |acc, byte| {
        acc.wrapping_mul(1_099_511_628_211)
            .wrapping_add(byte as u64 + 97)
    });
    (hash % 10_000) as f64 / 10_000.0
}

fn orbit_body_ref(
    star_idx: Option<usize>,
    orbit_label: Option<&str>,
    role: &str,
    orbit_name: &str,
) -> Option<OrbitBodyRef> {
    match (star_idx, orbit_label) {
        (Some(idx), None) => Some(OrbitBodyRef::Star(idx)),
        (None, Some(label)) => Some(OrbitBodyRef::OrbitLabel(label.to_string())),
        (Some(_), Some(_)) => {
            warn!(
                "Orbit '{}' has both {}_idx and {}_orbit_label set; using the orbit label",
                orbit_name, role, role
            );
            orbit_label.map(|label| OrbitBodyRef::OrbitLabel(label.to_string()))
        }
        (None, None) => {
            warn!(
                "Orbit '{}' is missing {} body information; skipping this orbit definition",
                orbit_name, role
            );
            None
        }
    }
}

fn orbit_primary_ref(orbit: &BinaryOrbitData) -> Option<OrbitBodyRef> {
    orbit_body_ref(
        orbit.primary_idx,
        orbit.primary_orbit_label.as_deref(),
        "primary",
        &orbit.label,
    )
}

fn orbit_secondary_ref(orbit: &BinaryOrbitData) -> Option<OrbitBodyRef> {
    orbit_body_ref(
        orbit.secondary_idx,
        orbit.secondary_orbit_label.as_deref(),
        "secondary",
        &orbit.label,
    )
}

fn resolved_star_body(star_data: &StarData, metallicity: f32, star_index: usize) -> ResolvedOrbitBody {
    ResolvedOrbitBody {
        mass_sol: star_data.mass_sol as f64,
        luminosity_sol: star_data.luminosity_sol,
        metallicity,
        representative_star_idx: star_index,
    }
}

fn resolve_orbit_body(
    member: &OrbitBodyRef,
    stars: &[StarData],
    star_metallicities: &[f32],
    resolved_orbits: &HashMap<String, ResolvedOrbitBody>,
) -> Option<ResolvedOrbitBody> {
    match member {
        OrbitBodyRef::Star(idx) => stars
            .get(*idx)
            .zip(star_metallicities.get(*idx))
            .map(|(star_data, metallicity)| resolved_star_body(star_data, *metallicity, *idx)),
        OrbitBodyRef::OrbitLabel(label) => resolved_orbits.get(label).cloned(),
    }
}

fn combine_orbit_bodies(
    primary: &ResolvedOrbitBody,
    secondary: &ResolvedOrbitBody,
) -> ResolvedOrbitBody {
    let total_mass = (primary.mass_sol + secondary.mass_sol).max(1e-6);
    ResolvedOrbitBody {
        mass_sol: total_mass,
        luminosity_sol: primary.luminosity_sol + secondary.luminosity_sol,
        metallicity: ((primary.metallicity * primary.mass_sol as f32)
            + (secondary.metallicity * secondary.mass_sol as f32))
            / total_mass as f32,
        representative_star_idx: primary.representative_star_idx,
    }
}

fn build_component_orbits(
    semi_major_axis_au: f64,
    eccentricity: f64,
    inclination_deg: f64,
    arg_periastron_deg: f64,
    longitude_ascending_node_deg: f64,
    period_years: f64,
    primary_mass_sol: f64,
    secondary_mass_sol: f64,
) -> (KeplerOrbit, KeplerOrbit) {
    let total_mass = (primary_mass_sol + secondary_mass_sol).max(1e-6);
    let period_seconds = (period_years.max(1e-4)) * 365.25 * 86400.0;
    let mean_motion = std::f64::consts::TAU / period_seconds;
    let primary_axis = semi_major_axis_au * (secondary_mass_sol / total_mass);
    let secondary_axis = semi_major_axis_au * (primary_mass_sol / total_mass);
    let inclination = inclination_deg.to_radians();
    let arg_periastron = arg_periastron_deg.to_radians();
    let longitude_ascending_node = longitude_ascending_node_deg.to_radians();

    (
        KeplerOrbit::new(
            eccentricity,
            primary_axis,
            inclination,
            longitude_ascending_node,
            arg_periastron,
            0.0,
            mean_motion,
        ),
        KeplerOrbit::new(
            eccentricity,
            secondary_axis,
            inclination,
            longitude_ascending_node,
            arg_periastron + std::f64::consts::PI,
            0.0,
            mean_motion,
        ),
    )
}

fn build_binary_component_orbits(
    orbit: &BinaryOrbitData,
    primary_mass_sol: f64,
    secondary_mass_sol: f64,
) -> (KeplerOrbit, KeplerOrbit) {
    build_component_orbits(
        orbit.semi_major_axis_au,
        orbit.eccentricity,
        orbit.inclination_deg,
        orbit.arg_periastron_deg,
        orbit.longitude_ascending_node_deg,
        orbit.period_years,
        primary_mass_sol,
        secondary_mass_sol,
    )
}

fn circumstellar_stability_limit(
    orbit: &BinaryOrbitData,
    host_mass_sol: f64,
    companion_mass_sol: f64,
) -> f64 {
    let total_mass = (host_mass_sol + companion_mass_sol).max(1e-6);
    let mu = (companion_mass_sol / total_mass).clamp(0.0, 1.0);
    let e = orbit.eccentricity.clamp(0.0, 0.9);
    let ratio = 0.464 - 0.380 * mu - 0.631 * e + 0.586 * mu * e + 0.150 * e * e
        - 0.198 * mu * e * e;
    orbit.semi_major_axis_au * ratio.max(0.02)
}

fn estimate_outer_companion_orbits(
    system_name: &str,
    inner_apastron_au: f64,
    inner_mass_sol: f64,
    tertiary_mass_sol: f64,
) -> (KeplerOrbit, KeplerOrbit) {
    let semi_major_axis_au = (inner_apastron_au * 250.0).clamp(600.0, 15_000.0);
    let eccentricity = 0.3 + stable_unit_interval(system_name, 17) * 0.25;
    let inclination_deg = 20.0 + stable_unit_interval(system_name, 29) * 60.0;
    let arg_periastron_deg = stable_unit_interval(system_name, 43) * 360.0;
    let longitude_ascending_node_deg = stable_unit_interval(system_name, 61) * 360.0;
    let total_mass = (inner_mass_sol + tertiary_mass_sol).max(1e-6);
    let period_years = (semi_major_axis_au.powi(3) / total_mass).sqrt();

    build_component_orbits(
        semi_major_axis_au,
        eccentricity,
        inclination_deg,
        arg_periastron_deg,
        longitude_ascending_node_deg,
        period_years,
        inner_mass_sol,
        tertiary_mass_sol,
    )
}

fn build_fallback_star_orbit(
    system_name: &str,
    star_index: usize,
    star_mass_sol: f64,
    primary_mass_sol: f64,
) -> KeplerOrbit {
    let semi_major_axis_au = 200.0 + (star_index as f64 * 120.0);
    let eccentricity = 0.05 + stable_unit_interval(system_name, star_index as u64 + 101) * 0.2;
    let inclination_deg = stable_unit_interval(system_name, star_index as u64 + 211) * 35.0;
    let arg_periastron_deg = stable_unit_interval(system_name, star_index as u64 + 307) * 360.0;
    let longitude_ascending_node_deg =
        stable_unit_interval(system_name, star_index as u64 + 401) * 360.0;
    let total_mass = (star_mass_sol + primary_mass_sol).max(1e-6);
    let period_years = (semi_major_axis_au.powi(3) / total_mass).sqrt();

    build_component_orbits(
        semi_major_axis_au,
        eccentricity,
        inclination_deg,
        arg_periastron_deg,
        longitude_ascending_node_deg,
        period_years,
        primary_mass_sol,
        star_mass_sol,
    )
    .1
}

fn circumbinary_stability_limit(
    orbit: &BinaryOrbitData,
    primary_mass_sol: f64,
    secondary_mass_sol: f64,
) -> f64 {
    let total_mass = (primary_mass_sol + secondary_mass_sol).max(1e-6);
    let mu = (secondary_mass_sol / total_mass).clamp(0.0, 1.0);
    let e = orbit.eccentricity.clamp(0.0, 0.9);
    let ratio = 1.60 + 5.10 * e - 2.22 * e * e + 4.12 * mu - 4.27 * e * mu
        - 5.09 * mu * mu
        + 4.61 * e * e * mu * mu;
    orbit.semi_major_axis_au * ratio.max(2.0)
}

#[allow(clippy::too_many_arguments)]
fn populate_host_bodies(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    host_name: &str,
    system_name: &str,
    parent: OrbitParentLink,
    system_id: usize,
    host_mass_sol: f64,
    host_luminosity_sol: f32,
    spectral_class: SpectralClass,
    metallicity: f32,
    vis_scale: f32,
    confirmed_planets: &[PlanetData],
    allow_procedural_generation: bool,
    allow_minor_bodies: bool,
    minimum_procedural_orbit_au: Option<f64>,
    maximum_stable_orbit_au: Option<f64>,
    cap_star: Option<(Entity, &StarData)>,
    rng: &mut StdRng,
    game_seed: u64,
) -> f64 {
    let frost_line = calculate_frost_line(host_luminosity_sol as f64);
    let star_system = StarSystem::with_metallicity(frost_line, spectral_class, metallicity);
    let metallicity_mult = star_system.metallicity_multiplier();
    let mut existing_orbits = Vec::new();
    let mut all_planet_entities = Vec::new();

    for planet_data in confirmed_planets {
        if let Some(max_stable_au) = maximum_stable_orbit_au {
            if (planet_data.semi_major_axis_au as f64) > max_stable_au {
                warn!(
                    "Skipping unstable confirmed planet '{}' in '{}' at {:.3} AU; circumstellar stability limit is {:.3} AU",
                    planet_data.name,
                    system_name,
                    planet_data.semi_major_axis_au,
                    max_stable_au
                );
                continue;
            }
        }
        let planet_entity = spawn_confirmed_planet(
            commands,
            planet_data,
            parent,
            system_id,
            host_luminosity_sol,
            vis_scale,
            rng,
        );
        existing_orbits.push(planet_data.semi_major_axis_au as f64);
        let radius_earth = planet_data.radius_earth.unwrap_or(1.0);
        let radius_km = radius_earth * 6371.0;
        let visual_radius = capped_visual_radius(
            BodyType::Planet,
            radius_km,
            planet_data.semi_major_axis_au as f64,
            vis_scale,
        );
        all_planet_entities.push(SpawnedPlanetSummary {
            entity: planet_entity,
            semi_major_axis_au: planet_data.semi_major_axis_au as f64,
            mass_earth: planet_data.mass_earth,
            visual_radius,
            radius_km,
            name: planet_data.name.clone(),
        });
    }

    let min_orbit_au = minimum_procedural_orbit_au.unwrap_or(0.0);
    let max_orbit_au = maximum_stable_orbit_au.unwrap_or(f64::INFINITY);
    let architecture = allow_procedural_generation.then(|| {
        map_star_to_system_architecture(
            host_name,
            host_mass_sol,
            host_luminosity_sol as f64,
            existing_orbits.len(),
            &existing_orbits,
            rng,
        )
    });

    if let Some(arch) = &architecture {
        debug!(
            "  Generated {} rocky planets, {} gas giants for '{}' in '{}'",
            arch.rocky_planets.len(),
            arch.gas_giants.len(),
            host_name,
            system_name
        );
    }

    if let Some(arch) = &architecture {
        for planet in arch
            .rocky_planets
            .iter()
            .chain(arch.gas_giants.iter())
            .filter(|planet| {
                planet.semi_major_axis_au >= min_orbit_au && planet.semi_major_axis_au <= max_orbit_au
            })
        {
            let planet_entity = spawn_procedural_planet(
                commands,
                planet,
                parent,
                system_id,
                metallicity_mult,
                host_luminosity_sol,
                vis_scale,
                rng,
            );
            all_planet_entities.push(SpawnedPlanetSummary {
                entity: planet_entity,
                semi_major_axis_au: planet.semi_major_axis_au,
                mass_earth: planet.mass_earth,
                visual_radius: capped_visual_radius(
                    planet.body_type(),
                    planet.radius_km(),
                    planet.semi_major_axis_au,
                    vis_scale,
                ),
                radius_km: planet.radius_km() as f32,
                name: planet.name.clone(),
            });
        }
    }

    let frost_line_for_rings = architecture
        .as_ref()
        .map(|arch| arch.frost_line_au)
        .unwrap_or(frost_line);

    for planet in &all_planet_entities {
        spawn_procedural_moons(
            commands,
            planet.entity,
            &planet.name,
            planet.semi_major_axis_au,
            planet.mass_earth,
            planet.radius_km,
            planet.visual_radius,
            system_id,
            host_luminosity_sol,
            vis_scale,
            rng,
        );

        let ring_chance = if planet.mass_earth > 30.0
            && planet.semi_major_axis_au > frost_line_for_rings * 0.5
        {
            0.42
        } else if planet.mass_earth > 10.0 && planet.semi_major_axis_au > frost_line_for_rings * 0.5 {
            0.20
        } else {
            0.0
        };

        if ring_chance > 0.0 && rng.random_bool(ring_chance) {
            spawn_procedural_ring(
                commands,
                meshes,
                materials,
                images,
                planet.entity,
                &planet.name,
                planet.visual_radius,
                planet.mass_earth,
                system_id,
                rng,
            );
        }
    }

    let mut max_radius_au = all_planet_entities
        .iter()
        .map(|planet| planet.semi_major_axis_au * 1.5)
        .fold(10.0_f64, f64::max);

    if allow_minor_bodies {
        if let Some(arch) = &architecture {
            if let Some(belt) = arch.asteroid_belt.as_ref() {
                if let Some(adjusted_belt) =
                    clamp_asteroid_belt(belt, min_orbit_au, maximum_stable_orbit_au)
                {
                    max_radius_au = max_radius_au.max(adjusted_belt.outer_au * 1.2);
                    spawn_asteroid_belt(
                        commands,
                        &adjusted_belt,
                        parent,
                        system_id,
                        system_name,
                        host_luminosity_sol,
                        vis_scale,
                        game_seed,
                    );
                }
            }

            if let Some(cloud) = arch.cometary_cloud.as_ref() {
                if let Some(adjusted_cloud) =
                    clamp_cometary_cloud(cloud, min_orbit_au, maximum_stable_orbit_au)
                {
                    max_radius_au = max_radius_au.max(adjusted_cloud.outer_au * 1.1);
                    spawn_cometary_cloud(
                        commands,
                        &adjusted_cloud,
                        parent,
                        system_id,
                        system_name,
                        host_luminosity_sol,
                        vis_scale,
                        game_seed,
                    );
                }
            }

            let dwarf_planets: Vec<ProceduralPlanet> = arch
                .dwarf_planets
                .iter()
                .filter(|planet| {
                    planet.semi_major_axis_au >= min_orbit_au && planet.semi_major_axis_au <= max_orbit_au
                })
                .cloned()
                .collect();
            if !dwarf_planets.is_empty() {
                max_radius_au = dwarf_planets.iter().fold(max_radius_au, |radius, planet| {
                    radius.max(planet.semi_major_axis_au * 1.3)
                });
                spawn_dwarf_planets(
                    commands,
                    &dwarf_planets,
                    parent,
                    system_id,
                    host_luminosity_sol,
                    vis_scale,
                );
            }
        }
    }

    if let Some((star_entity, star_data)) = cap_star {
        if let Some(inner_sma_au) = all_planet_entities
            .iter()
            .map(|planet| planet.semi_major_axis_au)
            .reduce(f64::min)
        {
            let max_star_vis = (inner_sma_au as f32) * (SCALING_FACTOR as f32) * 0.12;
            let current = calculate_visual_radius(
                BodyType::Star,
                (star_data.radius_sol * 695700.0) as f32,
            );
            if current > max_star_vis && max_star_vis > 2.0 {
                if let Ok(mut body) = commands.get_entity(star_entity) {
                    body.insert(CelestialBody {
                        name: star_data.name.clone(),
                        mass: (star_data.mass_sol * 1.989e30) as f64,
                        radius: star_data.radius_sol * 695700.0,
                        body_type: BodyType::Star,
                        visual_radius: max_star_vis,
                        asteroid_class: None,
                    });
                }
            }
        }
    }

    max_radius_au
}

fn clamp_asteroid_belt(
    belt: &AsteroidBelt,
    minimum_inner_au: f64,
    maximum_outer_au: Option<f64>,
) -> Option<AsteroidBelt> {
    let inner_au = belt.inner_au.max(minimum_inner_au);
    let outer_au = belt.outer_au.min(maximum_outer_au.unwrap_or(f64::INFINITY));
    (inner_au < outer_au).then(|| AsteroidBelt {
        inner_au,
        outer_au,
        count: belt.count,
        inclination: belt.inclination,
    })
}

fn clamp_cometary_cloud(
    cloud: &CometaryCloud,
    minimum_inner_au: f64,
    maximum_outer_au: Option<f64>,
) -> Option<CometaryCloud> {
    let inner_au = cloud.inner_au.max(minimum_inner_au);
    let outer_au = cloud.outer_au.min(maximum_outer_au.unwrap_or(f64::INFINITY));
    (inner_au < outer_au).then(|| CometaryCloud {
        inner_au,
        outer_au,
        count: cloud.count,
        inclination: cloud.inclination,
    })
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
fn spawn_star_entity_with_metallicity(
    commands: &mut Commands,
    star_data: &StarData,
    system_id: usize,
    position: DVec3,
    metallicity: f32,
    orbit: Option<KeplerOrbit>,
    orbit_center: Option<Entity>,
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

    let mut entity_commands = commands.spawn((
        Star,
        CelestialBody {
            name: star_data.name.clone(),
            mass: (star_data.mass_sol * 1.989e30) as f64,
            radius: star_data.radius_sol * 695700.0,
            body_type: BodyType::Star,
            visual_radius: calculate_visual_radius(
                BodyType::Star,
                (star_data.radius_sol * 695700.0) as f32,
            ),
            asteroid_class: None,
        },
        StellarProperties::new(star_data.luminosity_sol, star_data.temp_k),
        if orbit.is_some() {
            SpaceCoordinates::default()
        } else {
            SpaceCoordinates::new(position)
        },
        SystemId(system_id),
        star_system,
    ));

    if let Some(star_orbit) = orbit {
        entity_commands.insert((
            star_orbit,
            OrbitPath::with_fade(Color::srgba(1.0, 0.72, 0.4, 0.45), 3.5),
        ));
        if let Some(parent) = orbit_center {
            entity_commands.insert(OrbitCenter(parent));
        }
    }

    entity_commands.id()
}

/// Compute the visual radius of a planet, capped at 10% of orbital distance
/// to prevent overlap with neighbors, with a minimum of 2.0.
fn capped_visual_radius(body_type: BodyType, radius_km: f32, sma_au: f64, vis_scale: f32) -> f32 {
    let base = calculate_visual_radius(body_type, radius_km) * vis_scale;
    let orbit_bevy = (sma_au as f32) * (SCALING_FACTOR as f32);
    base.min(orbit_bevy * 0.10).max(2.0)
}

/// Spawn a confirmed planet from real exoplanet data
fn spawn_confirmed_planet(
    commands: &mut Commands,
    planet_data: &PlanetData,
    parent: OrbitParentLink,
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
        // Tidal braking zone: real planets here rotate in ~2-8 days, not 10-60.
        // The previous 10-60 day range produced unrealistically extreme temperature
        // differentials (±65% of average) via adjust_temperature_for_rotation.
        rng.random_range(2.0..8.0_f32)
    } else {
        // Normal: log-uniform from ~0.3 to ~5 days
        let log_p = rng.random_range((-0.5_f32)..(0.7));
        10.0_f32.powf(log_p)
    };
    let rotation_speed = if rotation_period_days != 0.0 {
        (2.0 * std::f32::consts::PI) / (rotation_period_days.abs() * 86400.0)
    } else {
        0.0
    };
    // Tidal forces damp obliquity for close-in planets (Mercury 0.034° at 0.39 AU).
    let max_tilt = if sma < 0.1 {
        2.0_f32
    } else if sma < 0.3 {
        2.0 + (sma as f32 - 0.1) * (28.0 / 0.2)
    } else {
        45.0
    };
    let axial_tilt_deg = rng.random_range(0.0_f32..1.0).powf(1.5) * max_tilt;

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
        OrbitPath::new(Color::srgba(0.4, 0.75, 1.0, 0.85)), // Lighter blue — planets
        SpaceCoordinates::default(),                       // Will be updated by propagate_orbits
        OrbitCenter(parent.spatial_parent),
        OrbitsBody::new(parent.spatial_parent),
        LogicalParent(parent.logical_parent),
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
fn spawn_procedural_planet(
    commands: &mut Commands,
    planet: &ProceduralPlanet,
    parent: OrbitParentLink,
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
        OrbitPath::new(Color::srgba(0.4, 0.75, 1.0, 0.85)), // Lighter blue — planets
        SpaceCoordinates::default(),                       // Will be updated by propagate_orbits
        OrbitCenter(parent.spatial_parent),
        OrbitsBody::new(parent.spatial_parent),
        LogicalParent(parent.logical_parent),
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
    parent: OrbitParentLink,
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
            OrbitPath::new(Color::srgba(0.25, 0.45, 0.75, 0.7)), // Darker blue — dwarf planets
            SpaceCoordinates::default(),
            OrbitCenter(parent.spatial_parent),
            OrbitsBody::new(parent.spatial_parent),
            LogicalParent(parent.logical_parent),
            SystemId(system_id),
            Visibility::Hidden,
        ));
    }
}

/// Spawn asteroids in a belt
fn spawn_asteroid_belt(
    commands: &mut Commands,
    belt: &crate::astronomy::AsteroidBelt,
    parent: OrbitParentLink,
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
            OrbitPath::with_fade(Color::srgba(0.3, 0.55, 0.22, 0.45), 5.0), // Dark green, steep fade — asteroids
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent.spatial_parent),
            OrbitsBody::new(parent.spatial_parent),
            LogicalParent(parent.logical_parent),
            SystemId(system_id),
        ));
    }
}

/// Spawn comets in a cloud
fn spawn_cometary_cloud(
    commands: &mut Commands,
    cloud: &crate::astronomy::CometaryCloud,
    parent: OrbitParentLink,
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
            OrbitPath::new(Color::srgba(1.0, 0.8, 0.3, 0.65)), // Yellow — comets
            SpaceCoordinates::default(), // Will be updated by propagate_orbits
            OrbitCenter(parent.spatial_parent),
            OrbitsBody::new(parent.spatial_parent),
            LogicalParent(parent.logical_parent),
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
            OrbitPath::new(Color::srgba(0.65, 0.65, 0.65, 0.5)), // Grey — moons
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
    use crate::astronomy::systems::orbit_position_from_mean_anomaly;

    #[test]
    fn test_spectral_type_conversion() {
        assert_eq!(spectral_type_to_class("G2V"), SpectralClass::G);
        assert_eq!(spectral_type_to_class("M5.5Ve"), SpectralClass::M);
        assert_eq!(spectral_type_to_class("K1V"), SpectralClass::K);
        assert_eq!(spectral_type_to_class("A5"), SpectralClass::A);
    }

    #[test]
    fn test_binary_component_orbits_balance_around_barycenter() {
        let orbit = BinaryOrbitData {
            label: "Test AB".to_string(),
            primary_idx: Some(0),
            primary_orbit_label: None,
            secondary_idx: Some(1),
            secondary_orbit_label: None,
            semi_major_axis_au: 12.0,
            period_years: 20.0,
            eccentricity: 0.3,
            inclination_deg: 0.0,
            longitude_ascending_node_deg: 0.0,
            arg_periastron_deg: 0.0,
        };

        let primary_mass = 1.2;
        let secondary_mass = 0.8;
        let (primary_orbit, secondary_orbit) =
            build_binary_component_orbits(&orbit, primary_mass, secondary_mass);

        assert!((primary_orbit.semi_major_axis - 4.8).abs() < 1e-6);
        assert!((secondary_orbit.semi_major_axis - 7.2).abs() < 1e-6);

        let primary_pos =
            orbit_position_from_mean_anomaly(&primary_orbit, primary_orbit.mean_anomaly_epoch);
        let secondary_pos = orbit_position_from_mean_anomaly(
            &secondary_orbit,
            secondary_orbit.mean_anomaly_epoch,
        );
        let barycenter = primary_pos * primary_mass + secondary_pos * secondary_mass;

        assert!(barycenter.length() < 1e-6);
    }

    #[test]
    fn test_circumbinary_stability_limit_exceeds_binary_extent() {
        let orbit = BinaryOrbitData {
            label: "Alpha Test".to_string(),
            primary_idx: Some(0),
            primary_orbit_label: None,
            secondary_idx: Some(1),
            secondary_orbit_label: None,
            semi_major_axis_au: 23.299,
            period_years: 79.762,
            eccentricity: 0.51947,
            inclination_deg: 79.243,
            longitude_ascending_node_deg: 0.0,
            arg_periastron_deg: 231.519,
        };

        let critical_radius = circumbinary_stability_limit(&orbit, 1.1, 0.907);
        let apastron = orbit.semi_major_axis_au * (1.0 + orbit.eccentricity);

        assert!(critical_radius > apastron);
        assert!(critical_radius > 50.0);
    }

    #[test]
    fn test_circumstellar_stability_limit_stays_inside_binary() {
        let orbit = BinaryOrbitData {
            label: "Tight Pair".to_string(),
            primary_idx: Some(0),
            primary_orbit_label: None,
            secondary_idx: Some(1),
            secondary_orbit_label: None,
            semi_major_axis_au: 20.0,
            period_years: 63.0,
            eccentricity: 0.35,
            inclination_deg: 20.0,
            longitude_ascending_node_deg: 15.0,
            arg_periastron_deg: 35.0,
        };

        let stable_radius = circumstellar_stability_limit(&orbit, 1.0, 0.8);

        assert!(stable_radius > 0.0);
        assert!(stable_radius < orbit.semi_major_axis_au);
    }
}
