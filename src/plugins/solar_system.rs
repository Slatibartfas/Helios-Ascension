use bevy::prelude::*;
use std::collections::HashMap;

use super::solar_system_data::{BodyType, SolarSystemData};
use crate::astronomy::{KeplerOrbit, OrbitPath, SpaceCoordinates};

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_solar_system)
            .add_systems(Update, (update_orbits, rotate_bodies));
    }
}

#[derive(Component)]
pub struct CelestialBody {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub radius: f32,
    #[allow(dead_code)]
    pub mass: f64,
}

#[derive(Component)]
pub struct Star;

#[derive(Component)]
pub struct Planet;

#[derive(Component)]
pub struct DwarfPlanet;

#[derive(Component)]
pub struct Moon;

#[derive(Component)]
pub struct Asteroid;

#[derive(Component)]
pub struct Comet;

#[derive(Component)]
pub struct OrbitalPath {
    pub parent: Option<Entity>,
    pub semi_major_axis: f32,
    #[allow(dead_code)]
    pub eccentricity: f32,
    #[allow(dead_code)]
    pub inclination: f32,
    #[allow(dead_code)]
    pub orbital_period: f32,
    pub current_angle: f32,
    pub angular_velocity: f32,
}

#[derive(Component)]
pub struct RotationSpeed(pub f32);

// Visualization scale factors
const AU_TO_UNITS: f32 = 50.0; // 1 AU = 50 game units for visibility
const RADIUS_SCALE: f32 = 0.0001; // Scale down radii for visibility
const MIN_VISUAL_RADIUS: f32 = 0.3; // Minimum visible radius

// Time conversion constants
const SECONDS_PER_DAY: f64 = 86400.0; // Number of seconds in one Earth day

fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Load solar system data
    let data = match SolarSystemData::load_from_file("assets/data/solar_system.ron") {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to load solar system data: {}", e);
            return;
        }
    };

    info!("Loaded {} celestial bodies", data.bodies.len());

    // Map to track entities by name for parent-child relationships
    let mut entity_map: HashMap<String, Entity> = HashMap::new();

    // First pass: Create all bodies
    for body_data in &data.bodies {
        // Calculate visual radius (with minimum for visibility)
        let visual_radius = (body_data.radius * RADIUS_SCALE).max(MIN_VISUAL_RADIUS);

        // Calculate rotation speed (convert from days to radians per second)
        let rotation_speed = if body_data.rotation_period != 0.0 {
            (2.0 * std::f32::consts::PI) / (body_data.rotation_period.abs() * SECONDS_PER_DAY as f32)
                * if body_data.rotation_period < 0.0 {
                    -1.0
                } else {
                    1.0
                }
        } else {
            0.0
        };

        // Determine if this is the star (to add light)
        let is_star = body_data.body_type == BodyType::Star;

        // Create material
        let material = if is_star {
            materials.add(StandardMaterial {
                base_color: Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2),
                emissive: LinearRgba::from(Color::srgb(
                    body_data.emissive.0,
                    body_data.emissive.1,
                    body_data.emissive.2,
                )),
                ..default()
            })
        } else {
            materials.add(StandardMaterial {
                base_color: Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2),
                ..default()
            })
        };

        // Determine initial position
        let initial_pos = if let Some(ref orbit) = body_data.orbit {
            let angle_rad = orbit.initial_angle.to_radians();
            let distance = orbit.semi_major_axis * AU_TO_UNITS;
            Vec3::new(distance * angle_rad.cos(), 0.0, distance * angle_rad.sin())
        } else {
            Vec3::ZERO
        };

        // Build entity with appropriate components
        let mut entity_commands = commands.spawn((
            PbrBundle {
                mesh: meshes.add(Sphere::new(visual_radius)),
                material,
                transform: Transform::from_translation(initial_pos),
                ..default()
            },
            CelestialBody {
                name: body_data.name.clone(),
                radius: body_data.radius,
                mass: body_data.mass,
            },
            RotationSpeed(rotation_speed),
        ));

        // Add type-specific component
        match body_data.body_type {
            BodyType::Star => {
                entity_commands.insert(Star);
            }
            BodyType::Planet => {
                entity_commands.insert(Planet);
            }
            BodyType::DwarfPlanet => {
                entity_commands.insert(DwarfPlanet);
            }
            BodyType::Moon => {
                entity_commands.insert(Moon);
            }
            BodyType::Asteroid => {
                entity_commands.insert(Asteroid);
            }
            BodyType::Comet => {
                entity_commands.insert(Comet);
            }
        }

        let entity = entity_commands.id();
        entity_map.insert(body_data.name.clone(), entity);

        // Add light for stars
        if is_star {
            commands.spawn(PointLightBundle {
                point_light: PointLight {
                    intensity: 5000000.0,
                    range: 10000.0,
                    shadows_enabled: false, // Disable for performance with many objects
                    ..default()
                },
                transform: Transform::from_translation(initial_pos),
                ..default()
            });
        }
    }

    // Second pass: Add orbital components with parent references
    for body_data in &data.bodies {
        if let Some(ref orbit) = body_data.orbit {
            let entity = entity_map.get(&body_data.name).unwrap();

            // Get parent entity
            let parent_entity = body_data
                .parent
                .as_ref()
                .and_then(|parent_name| entity_map.get(parent_name))
                .copied();

            // Calculate angular velocity (radians per second)
            // orbital_period is in Earth days
            let angular_velocity = if orbit.orbital_period > 0.0 {
                (2.0 * std::f32::consts::PI) / (orbit.orbital_period * SECONDS_PER_DAY as f32)
            } else {
                0.0
            };

            // Add legacy OrbitalPath component (for backwards compatibility)
            commands.entity(*entity).insert(OrbitalPath {
                parent: parent_entity,
                semi_major_axis: orbit.semi_major_axis * AU_TO_UNITS,
                eccentricity: orbit.eccentricity,
                inclination: orbit.inclination,
                orbital_period: orbit.orbital_period,
                current_angle: orbit.initial_angle.to_radians(),
                angular_velocity,
            });

            // Add new high-precision astronomy components
            // Convert orbital period in days to mean motion in radians/second
            let mean_motion = if orbit.orbital_period > 0.0 {
                (2.0 * std::f64::consts::PI) / (orbit.orbital_period as f64 * SECONDS_PER_DAY)
            } else {
                0.0
            };

            // Create KeplerOrbit component with high-precision values
            let kepler_orbit = KeplerOrbit::new(
                orbit.eccentricity as f64,
                orbit.semi_major_axis as f64, // Already in AU
                orbit.inclination.to_radians() as f64,
                orbit.longitude_ascending_node.to_radians() as f64,
                orbit.argument_of_periapsis.to_radians() as f64,
                orbit.initial_angle.to_radians() as f64, // mean_anomaly_epoch
                mean_motion,
            );

            commands.entity(*entity).insert((
                kepler_orbit,
                SpaceCoordinates::default(),
            ));

            // Determine orbit color and visibility based on body type
            let (orbit_color, should_show) = match body_data.body_type {
                BodyType::Planet | BodyType::DwarfPlanet => {
                    // Planets: always show, use bluish color
                    (Color::srgba(0.3, 0.5, 0.8, 0.4), true)
                }
                BodyType::Moon => {
                    // Moons: show with lower opacity, grayish color
                    // Will be controlled by zoom level in a future update
                    (Color::srgba(0.5, 0.5, 0.5, 0.2), true)
                }
                BodyType::Asteroid | BodyType::Comet => {
                    // Asteroids/Comets: hidden by default, yellowish
                    // Will be shown when selected
                    (Color::srgba(0.8, 0.6, 0.2, 0.3), false)
                }
                _ => (Color::srgba(0.5, 0.5, 0.5, 0.3), false),
            };

            commands.entity(*entity).insert(OrbitPath {
                color: orbit_color,
                visible: should_show,
                segments: 64,
            });
        }
    }

    info!("Solar system setup complete!");
}

fn update_orbits(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut OrbitalPath, &CelestialBody)>,
    parent_query: Query<&Transform, Without<OrbitalPath>>,
) {
    // Use a larger time multiplier for faster orbits at high simulation speeds
    let time_multiplier = 1000.0; // Make orbits 1000x faster for visibility

    for (mut transform, mut orbit, _body) in query.iter_mut() {
        // Update the angle based on angular velocity
        orbit.current_angle += orbit.angular_velocity * time.delta_seconds() * time_multiplier;

        // Keep angle in range [0, 2π]
        if orbit.current_angle > 2.0 * std::f32::consts::PI {
            orbit.current_angle -= 2.0 * std::f32::consts::PI;
        }

        // Calculate position based on orbital parameters
        // For now, using simplified circular orbits (eccentricity not yet implemented)
        let x = orbit.semi_major_axis * orbit.current_angle.cos();
        let z = orbit.semi_major_axis * orbit.current_angle.sin();

        // Apply inclination
        let y = orbit.semi_major_axis
            * orbit.inclination.to_radians().sin()
            * orbit.current_angle.sin();

        // Get parent position if it exists
        let parent_pos = if let Some(parent_entity) = orbit.parent {
            if let Ok(parent_transform) = parent_query.get(parent_entity) {
                parent_transform.translation
            } else {
                Vec3::ZERO
            }
        } else {
            Vec3::ZERO
        };

        transform.translation = parent_pos + Vec3::new(x, y, z);
    }
}

fn rotate_bodies(time: Res<Time>, mut query: Query<(&mut Transform, &RotationSpeed)>) {
    for (mut transform, rotation_speed) in query.iter_mut() {
        transform.rotate_y(rotation_speed.0 * time.delta_seconds() * 1000.0);
    }
}
