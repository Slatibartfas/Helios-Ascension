use bevy::prelude::*;
use std::collections::HashMap;

use super::solar_system_data::{BodyType, SolarSystemData};
use crate::astronomy::{KeplerOrbit, OrbitPath, SpaceCoordinates};

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_solar_system)
            .add_systems(Update, rotate_bodies);
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
pub struct RotationSpeed(pub f32);

// Visualization scale factors - Note: Legacy AU_TO_UNITS removed, now using astronomy module's SCALING_FACTOR
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

        // Create material with improved visual properties
        let material = if is_star {
            materials.add(StandardMaterial {
                base_color: Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2),
                emissive: LinearRgba::from(Color::srgb(
                    body_data.emissive.0,
                    body_data.emissive.1,
                    body_data.emissive.2,
                )),
                perceptual_roughness: 1.0, // Stars are rough/diffuse
                metallic: 0.0,
                ..default()
            })
        } else {
            materials.add(StandardMaterial {
                base_color: Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2),
                perceptual_roughness: 0.8, // Slightly rough for planets
                metallic: 0.1, // Slight metallic for better lighting
                reflectance: 0.3, // Some reflectance for rim lighting
                ..default()
            })
        };

        // Determine initial position
        // Note: Initial position is approximate - astronomy module handles precise orbital mechanics
        let initial_pos = if let Some(ref orbit) = body_data.orbit {
            let angle_rad = orbit.initial_angle.to_radians();
            // Use 50.0 to match SCALING_FACTOR (1 AU = 50 Bevy units)
            let distance = orbit.semi_major_axis * 50.0;
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

    // Second pass: Add high-precision astronomy components with parent references
    for body_data in &data.bodies {
        if let Some(ref orbit) = body_data.orbit {
            let entity = entity_map.get(&body_data.name).unwrap();

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
            // Using Terra Invicta-inspired colors: clear, functional, high contrast
            let (orbit_color, should_show) = match body_data.body_type {
                BodyType::Planet | BodyType::DwarfPlanet => {
                    // Planets: bright cyan/blue for high visibility (Terra Invicta style)
                    (Color::srgba(0.4, 0.7, 1.0, 0.6), true)
                }
                BodyType::Moon => {
                    // Moons: softer green-cyan, lower opacity
                    (Color::srgba(0.3, 0.8, 0.7, 0.35), true)
                }
                BodyType::Asteroid | BodyType::Comet => {
                    // Asteroids/Comets: amber/yellow when selected
                    (Color::srgba(1.0, 0.7, 0.2, 0.5), false)
                }
                _ => (Color::srgba(0.5, 0.5, 0.5, 0.3), false),
            };

            commands.entity(*entity).insert(OrbitPath {
                color: orbit_color,
                visible: should_show,
                segments: 96, // Smoother orbit lines (increased from 64)
            });
        }
    }

    info!("Solar system setup complete!");
}

fn rotate_bodies(time: Res<Time>, mut query: Query<(&mut Transform, &RotationSpeed)>) {
    for (mut transform, rotation_speed) in query.iter_mut() {
        transform.rotate_y(rotation_speed.0 * time.delta_seconds() * 1000.0);
    }
}
