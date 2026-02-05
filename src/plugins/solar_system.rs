use bevy::prelude::*;
use std::collections::HashMap;

use super::solar_system_data::{AsteroidClass, BodyType, SolarSystemData};
use crate::astronomy::{KeplerOrbit, OrbitPath, SpaceCoordinates};
use crate::plugins::camera::{CameraAnchor, GameCamera};

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_solar_system)
            .add_systems(PostStartup, initial_camera_focus)
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
    pub body_type: BodyType,
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
pub struct Ring;

#[derive(Component)]
pub struct RotationSpeed(pub f32);

// Visualization scale factors
// Increased scale for planets to be easily visible and clickable
pub const RADIUS_SCALE: f32 = 0.002; // Increased from 0.001 for better visibility
// Minimum size to ensure small moons are visible and clickable
const MIN_VISUAL_RADIUS: f32 = 5.0; // Increased from 3.0 for easier clicking
// Sun needs a separate, smaller scale to not engulf the inner system when planets are oversized
const STAR_RADIUS_SCALE: f32 = 0.0001; 

// Time conversion constants
const SECONDS_PER_DAY: f64 = 86400.0; // Number of seconds in one Earth day

/// Determine which generic texture to use for a body without a dedicated texture
fn get_generic_texture_path(body_data: &super::solar_system_data::CelestialBodyData) -> Option<String> {
    match body_data.body_type {
        BodyType::Asteroid => {
            // Choose based on asteroid class
            let class = body_data.asteroid_class.unwrap_or(AsteroidClass::CType);
            match class {
                AsteroidClass::CType => Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string()),
                AsteroidClass::SType => Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string()),
                AsteroidClass::MType => Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string()), // Use S-type for now
                AsteroidClass::Unknown => Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string()),
            }
        }
        BodyType::Comet => {
            Some("textures/celestial/comets/generic_nucleus_2k.jpg".to_string())
        }
        BodyType::Moon => {
            // Use a generic icy or rocky texture based on density
            // For now, use the C-type asteroid texture as a generic rocky surface
            Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
        }
        _ => None, // Planets and stars should have dedicated textures
    }
}

/// Generate procedural variation for material based on body properties
fn apply_procedural_variation(
    body_data: &super::solar_system_data::CelestialBodyData,
    base_color: Color,
    has_texture: bool,
) -> (Color, f32, f32) {
    // Use body name as seed for consistent randomness
    let mut seed = 0u32;
    for byte in body_data.name.bytes() {
        seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
    }
    
    // Generate pseudo-random values from seed
    let random1 = ((seed % 1000) as f32) / 1000.0;
    let random2 = (((seed / 1000) % 1000) as f32) / 1000.0;
    let random3 = (((seed / 1000000) % 1000) as f32) / 1000.0;
    
    // Vary color slightly based on body properties
    let color_variation = match body_data.body_type {
        BodyType::Asteroid => {
            // Asteroids: Vary brightness and hue slightly
            let brightness_var = 0.8 + random1 * 0.4; // 0.8 to 1.2
            Color::srgb(
                (base_color.to_srgba().red * brightness_var).clamp(0.0, 1.0),
                (base_color.to_srgba().green * brightness_var).clamp(0.0, 1.0),
                (base_color.to_srgba().blue * brightness_var).clamp(0.0, 1.0),
            )
        }
        BodyType::Comet => {
            // Comets: Vary between icy white and dusty brown
            let ice_factor = random1;
            Color::srgb(
                0.6 + ice_factor * 0.3,
                0.6 + ice_factor * 0.2,
                0.5 + ice_factor * 0.4,
            )
        }
        BodyType::Moon => {
            // Moons: Slight color variation
            let gray_variation = 0.9 + random1 * 0.2;
            Color::srgb(
                (base_color.to_srgba().red * gray_variation).clamp(0.0, 1.0),
                (base_color.to_srgba().green * gray_variation).clamp(0.0, 1.0),
                (base_color.to_srgba().blue * gray_variation).clamp(0.0, 1.0),
            )
        }
        BodyType::Ring => base_color, // Rings rely on texture/transparency
        _ => base_color,
    };
    
    // Vary roughness for surface variation
    let roughness_var = if has_texture {
        if body_data.body_type == BodyType::Ring {
            0.8 // Rings are dusty/icy
        } else {
            0.7 + random2 * 0.2 // 0.7 to 0.9 for textured bodies
        }
    } else {
        0.6 + random2 * 0.3 // 0.6 to 0.9 for non-textured bodies
    };
    
    // Vary metallic slightly for asteroids
    let metallic_var = match body_data.body_type {
        BodyType::Asteroid if body_data.asteroid_class == Some(AsteroidClass::MType) => {
            0.5 + random3 * 0.3 // 0.5 to 0.8 for metallic asteroids
        }
        BodyType::Asteroid => {
            0.05 + random3 * 0.1 // 0.05 to 0.15 for rocky asteroids
        }
        _ => 0.1 + random3 * 0.1, // 0.1 to 0.2 for others
    };
    
    (color_variation, roughness_var, metallic_var)
}

pub fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
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
        // Determine if this is the star (to add light)
        let is_star = body_data.body_type == BodyType::Star;

        // Calculate visual radius (with minimum for visibility)
        // Use different scale for stars to avoid them being too huge compared to orbits
        let scale_factor = if is_star { STAR_RADIUS_SCALE } else { RADIUS_SCALE };
        let visual_radius = (body_data.radius * scale_factor).max(MIN_VISUAL_RADIUS);

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

        // Check for multi-layer textures first, then single texture, then generic
        let (base_color_texture, normal_map_texture, has_dedicated_texture) = 
            if let Some(ref multi) = body_data.multi_layer_textures {
                // Multi-layer textures - use base texture and normal map for now
                // TODO: Implement full multi-layer rendering with night/clouds/specular
                //       See assets/textures/MULTI_LAYER_TEXTURES.md for implementation roadmap
                let base_tex = Some(asset_server.load::<Image>(multi.base.clone()));
                let normal_tex = multi.normal.as_ref().map(|path| asset_server.load::<Image>(path.clone()));
                (base_tex, normal_tex, true)
            } else if let Some(ref texture) = body_data.texture {
                // Single dedicated texture
                (Some(asset_server.load(texture.clone())), None, true)
            } else {
                // Generic texture based on body type
                let generic_path = get_generic_texture_path(body_data);
                (generic_path.map(|path| asset_server.load(path)), None, false)
            };
        
        let has_texture = base_color_texture.is_some();
        
        // Apply procedural variation to material properties
        // For dedicated textures, use WHITE to avoid tinting the texture
        // For generic/procedural textures, apply color variation for diversity
        let base_color = Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2);
        let (material_color, roughness, metallic) = if has_dedicated_texture {
            // Dedicated texture - use WHITE to show texture without tinting
            (Color::WHITE, 0.7, 0.0)
        } else {
            // Generic/procedural texture - apply variation
            apply_procedural_variation(body_data, base_color, has_texture)
        };

        // Create material with improved visual properties
        let material = if is_star {
            materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture,
                // Reduced emissive intensity to prevent blowout/whiteness
                emissive: LinearRgba::from(Color::srgb(
                   2.0, // Reduced from high values
                   1.8,
                   1.4,
                )),
                perceptual_roughness: 1.0, // Stars are rough/diffuse
                metallic: 0.0,
                ..default()
            })
        } else if body_data.body_type == BodyType::Ring {
            materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture: base_color_texture.clone(),
                perceptual_roughness: roughness,
                metallic: 0.0,
                reflectance: 0.2,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None, // Double-sided
                unlit: true, // Rings often look better unlit or carefully lit, but for now transparent unlit or lit? 
                            // Real rings are lit by sun. But avoiding shadows casting weirdly. 
                            // Let's stick to lit but standard.
                ..default()
            })
        } else {
            materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture,
                normal_map_texture,
                perceptual_roughness: roughness,
                metallic,
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
        let mesh = if body_data.body_type == BodyType::Ring {
            // Plane for rings (2x radius because plane size is usually defined by "size" which maps to width/height, 
            // but Plane3d uses specific construction. 
            // Plane3d::default() is infinite? No, Bevy primitives changed. 
            // Let's use Plane3d and Rectangle mesh builder or similar.
            // Bevy 0.14: Mesh::from(Plane3d { normal: Dir3::Y, half_size: Vec2::splat(visual_radius) }) theoretically.
            // Or Mesh::from(Rectangle::new(visual_radius * 2.0, visual_radius * 2.0)) rotated?
            // Plane usually implies XZ.
            meshes.add(Plane3d::default().mesh().size(visual_radius * 2.0, visual_radius * 2.0))
        } else {
            meshes.add(Sphere::new(visual_radius))
        };

        let mut entity_commands = commands.spawn((
            PbrBundle {
                mesh,
                material,
                transform: Transform::from_translation(initial_pos),
                ..default()
            },
            CelestialBody {
                name: body_data.name.clone(),
                radius: body_data.radius,
                mass: body_data.mass,
                body_type: body_data.body_type,
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
            BodyType::Ring => {
                entity_commands.insert(Ring);
            }
        }

        // Add atmosphere component if the body has atmosphere data
        if let Some(ref atmo_data) = body_data.atmosphere {
            use crate::astronomy::{AtmosphereComposition, AtmosphericGas};
            
            // Convert gas data from deserialized format to runtime format
            let gases: Vec<AtmosphericGas> = atmo_data
                .gases
                .iter()
                .map(|g| {
                    // Leak the string to get a 'static str - this is acceptable for gas names
                    // which are known at compile time and never change
                    let name: &'static str = Box::leak(g.name.clone().into_boxed_str());
                    AtmosphericGas::new(name, g.percentage)
                })
                .collect();
            
            let atmosphere = AtmosphereComposition::new(
                atmo_data.surface_pressure_mbar,
                atmo_data.surface_temperature_celsius,
                gases,
            );
            
            entity_commands.insert(atmosphere);
        }

        let entity = entity_commands.id();
        entity_map.insert(body_data.name.clone(), entity);
        
        // Handle parenting for all bodies with a parent (enables relative positioning)
        if let Some(parent_name) = &body_data.parent {
            if let Some(parent_entity) = entity_map.get(parent_name) {
                commands.entity(entity).set_parent(*parent_entity);
            } else {
                warn!("Parent {} not found for body {}", parent_name, body_data.name);
            }
        }

        // Add light for stars
        if is_star {
            commands.spawn(PointLightBundle {
                point_light: PointLight {
                    intensity: 10000000.0, // Increased from 5000000.0 for better illumination
                    range: 20000.0, // Increased from 10000.0 to reach distant bodies
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
                BodyType::Planet => {
                    // Planets: bright cyan/blue for high visibility (Terra Invicta style)
                    (Color::srgba(0.4, 0.7, 1.0, 0.6), true)
                }
                BodyType::DwarfPlanet => {
                    // Dwarf Planets: dimmer blue, hidden by default to reduce clutter
                     (Color::srgba(0.3, 0.5, 0.8, 0.4), false)
                }
                BodyType::Moon => {
                    // Moons: softer green-cyan, lower opacity
                    (Color::srgba(0.3, 0.8, 0.7, 0.35), true)
                }
                BodyType::Asteroid | BodyType::Comet => {
                    // Asteroids/Comets: amber/yellow when selected
                    (Color::srgba(1.0, 0.7, 0.2, 0.5), false)
                }
                BodyType::Ring => (Color::srgba(0.0, 0.0, 0.0, 0.0), false), // No orbit line for rings
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

// Sets the initial camera focus to the Sun
fn initial_camera_focus(
    query_bodies: Query<(Entity, &CelestialBody), With<Star>>,
    mut query_camera: Query<&mut CameraAnchor, With<GameCamera>>,
) {
    // Find Sol
    let sol_entity = query_bodies.iter().find(|(_, body)| body.name == "Sol").map(|(e, _)| e);
    
    if let Some(sol) = sol_entity {
        if let Ok(mut anchor) = query_camera.get_single_mut() {
            if anchor.0.is_none() {
                info!("Setting initial camera focus to Sol");
                anchor.0 = Some(sol);
            }
        }
    }
}
