use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use std::collections::HashMap;
use rand::prelude::*;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use super::solar_system_data::{AsteroidClass, BodyType, SolarSystemData};
use crate::astronomy::{
    orbit_position_from_mean_anomaly, KeplerOrbit, OrbitPath, SpaceCoordinates, SCALING_FACTOR,
};
use crate::plugins::camera::{CameraAnchor, GameCamera};

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_solar_system)
            .add_systems(PostStartup, initial_camera_focus)
            .add_systems(Update, (rotate_bodies, update_billboards))
            // System to convert loaded normal/specular textures to linear formats
            .add_systems(Update, apply_linear_to_images_system);
    }
}

/// Component to make an entity always face the camera (e.g. sun glare)
#[derive(Component)]
pub struct Billboard;

fn update_billboards(
    mut query: Query<&mut Transform, With<Billboard>>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
) {
    if let Ok(camera_global_transform) = camera_query.get_single() {
        let camera_pos = camera_global_transform.translation();
        for mut transform in query.iter_mut() {
            // Point the Z-axis at the camera
            transform.look_at(camera_pos, Vec3::Y);
        }
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
    pub visual_radius: f32,
    /// Asteroid spectral class (if applicable)
    pub asteroid_class: Option<AsteroidClass>,
}

/// Logical parent for UI hierarchy, separate from spatial transform parenting
#[derive(Component)]
pub struct LogicalParent(pub Entity);

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
pub const RADIUS_SCALE: f32 = 0.01; // Increased for better visibility
// Minimum size to ensure small moons are visible and clickable
const MIN_VISUAL_RADIUS: f32 = 5.0; // Increased from 3.0 for easier clicking
// Sun needs a separate, smaller scale to not engulf the inner system when planets are oversized
const STAR_RADIUS_SCALE: f32 = 0.00015; // Slightly increased from 0.0001, kept safe for Mercury orbit

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
                // M-Type: Metallic - use S-type for now, procedural variation adds metallic property
                AsteroidClass::MType => Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string()),
                // V-Type: Basaltic - use S-type for now, procedural variation adds reddish tint
                AsteroidClass::VType => Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string()),
                // D-Type: Dark primitive - use C-type (both very dark), procedural variation enhances darkness
                AsteroidClass::DType => Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string()),
                // P-Type: Primitive - use C-type (both dark), procedural variation creates distinction
                AsteroidClass::PType => Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string()),
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
/// Enhanced to visually distinguish all 6 asteroid spectral classes
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
    
    // Vary color based on body type and asteroid spectral class
    let color_variation = match body_data.body_type {
        BodyType::Asteroid => {
            // Apply spectral class-specific coloring and brightness
            match body_data.asteroid_class.unwrap_or(AsteroidClass::CType) {
                AsteroidClass::CType => {
                    // Carbonaceous: Very dark gray
                    let brightness_var = 0.6 + random1 * 0.3; // 0.6 to 0.9 (dark)
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().green * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().blue * brightness_var).clamp(0.0, 1.0),
                    )
                }
                AsteroidClass::SType => {
                    // Silicaceous: Medium gray, stony
                    let brightness_var = 0.9 + random1 * 0.4; // 0.9 to 1.3 (medium-bright)
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().green * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().blue * brightness_var).clamp(0.0, 1.0),
                    )
                }
                AsteroidClass::MType => {
                    // Metallic: Bright silvery-gray
                    let brightness_var = 1.2 + random1 * 0.4; // 1.2 to 1.6 (bright, metallic)
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var).clamp(0.0, 1.5),
                        (base_color.to_srgba().green * brightness_var).clamp(0.0, 1.5),
                        (base_color.to_srgba().blue * brightness_var).clamp(0.0, 1.5),
                    )
                }
                AsteroidClass::VType => {
                    // Vestoid: Reddish-gray basaltic
                    let brightness_var = 1.0 + random1 * 0.3; // 1.0 to 1.3
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var * 1.15).clamp(0.0, 1.0), // Enhanced red
                        (base_color.to_srgba().green * brightness_var * 0.95).clamp(0.0, 1.0),
                        (base_color.to_srgba().blue * brightness_var * 0.90).clamp(0.0, 1.0),
                    )
                }
                AsteroidClass::DType => {
                    // Dark primitive: Extremely dark, brownish
                    let brightness_var = 0.4 + random1 * 0.2; // 0.4 to 0.6 (very dark)
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var * 1.1).clamp(0.0, 1.0), // Slightly warmer
                        (base_color.to_srgba().green * brightness_var * 0.9).clamp(0.0, 1.0),
                        (base_color.to_srgba().blue * brightness_var * 0.8).clamp(0.0, 1.0),
                    )
                }
                AsteroidClass::PType => {
                    // Primitive: Very dark gray-brown
                    let brightness_var = 0.5 + random1 * 0.25; // 0.5 to 0.75 (very dark but not extreme)
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().green * brightness_var * 0.95).clamp(0.0, 1.0),
                        (base_color.to_srgba().blue * brightness_var * 0.90).clamp(0.0, 1.0),
                    )
                }
                AsteroidClass::Unknown => {
                    // Default to C-type appearance
                    let brightness_var = 0.7 + random1 * 0.3;
                    Color::srgb(
                        (base_color.to_srgba().red * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().green * brightness_var).clamp(0.0, 1.0),
                        (base_color.to_srgba().blue * brightness_var).clamp(0.0, 1.0),
                    )
                }
            }
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
    
    // Vary roughness for surface variation based on spectral class
    let roughness_var = if has_texture {
        if body_data.body_type == BodyType::Ring {
            0.8 // Rings are dusty/icy
        } else if body_data.body_type == BodyType::Asteroid {
            match body_data.asteroid_class.unwrap_or(AsteroidClass::CType) {
                AsteroidClass::MType => 0.2 + random2 * 0.2, // 0.2 to 0.4 (smooth, metallic)
                AsteroidClass::DType | AsteroidClass::PType => 0.8 + random2 * 0.15, // 0.8 to 0.95 (very rough, primitive)
                _ => 0.7 + random2 * 0.2, // 0.7 to 0.9 for others
            }
        } else {
            0.7 + random2 * 0.2 // 0.7 to 0.9 for other textured bodies
        }
    } else {
        0.6 + random2 * 0.3 // 0.6 to 0.9 for non-textured bodies
    };
    
    // Vary metallic property strongly by spectral class
    let metallic_var = match body_data.body_type {
        BodyType::Asteroid => {
            match body_data.asteroid_class.unwrap_or(AsteroidClass::CType) {
                AsteroidClass::MType => 0.6 + random3 * 0.3, // 0.6 to 0.9 (highly metallic)
                AsteroidClass::VType => 0.15 + random3 * 0.1, // 0.15 to 0.25 (slightly metallic, basaltic)
                AsteroidClass::DType | AsteroidClass::PType => 0.0 + random3 * 0.05, // 0.0 to 0.05 (minimal metal)
                _ => 0.05 + random3 * 0.1, // 0.05 to 0.15 for C/S types
            }
        }
        _ => 0.1 + random3 * 0.1, // 0.1 to 0.2 for others
    };
    
    (color_variation, roughness_var, metallic_var)
}

#[derive(Resource, Default)]
struct LinearImageQueue {
    handles: Vec<Handle<Image>>,
}

pub fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut materials_night: ResMut<Assets<crate::plugins::visual_effects::NightMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Queue to collect normal/specular handles that must be treated as linear textures
    let mut linear_handle_queue: Vec<Handle<Image>> = Vec::new();

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
        let (base_color_texture, _normal_map_texture, clouds_texture, night_texture, has_dedicated_texture) = 
            if let Some(ref multi) = body_data.multi_layer_textures {
                // Multi-layer textures - use base texture and normal map for now
                // TODO: Implement full multi-layer rendering with night/clouds/specular
                //       See assets/textures/MULTI_LAYER_TEXTURES.md for implementation roadmap
                let base_tex = Some(asset_server.load::<Image>(multi.base.clone()));
                let normal_tex = multi.normal.as_ref().map(|path| asset_server.load::<Image>(path.clone()));
                let clouds_tex = multi.clouds.as_ref().map(|path| asset_server.load::<Image>(path.clone()));
                let night_tex = multi.night.as_ref().map(|path| asset_server.load::<Image>(path.clone()));

                // Also load specular if present so we can ensure it's treated as linear (even if not used by StandardMaterial yet)
                let specular_tex = multi.specular.as_ref().map(|path| asset_server.load::<Image>(path.clone()));
                // Collect normal/specular handles for later conversion to linear color space
                if let Some(ref h) = normal_tex { linear_handle_queue.push(h.clone()); }
                if let Some(ref h) = specular_tex { linear_handle_queue.push(h.clone()); }
                // Night needs to be linear? Probably sRGB for emissive, but if it behaves as data, maybe linear. 
                // Usually diffuse/emissive maps are sRGB.
                
                (base_tex, normal_tex, clouds_tex, night_tex, true)
            } else if let Some(ref texture) = body_data.texture {
                // Single dedicated texture
                (Some(asset_server.load(texture.clone())), None, None, None, true)
            } else {
                // Generic texture based on body type
                let generic_path = get_generic_texture_path(body_data);
                (generic_path.map(|path| asset_server.load(path)), None, None, None, false)
            };
        
        let has_texture = base_color_texture.is_some();
        
        // Apply procedural variation to material properties
        let base_color = Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2);
        let (material_color, roughness, metallic) = if has_dedicated_texture {
            // For textured bodies, use slightly tinted color to enhance texture
            (Color::srgb(1.0, 1.0, 1.0), 0.7, 0.0)
        } else {
            // Generic/procedural texture - apply variation
            apply_procedural_variation(body_data, base_color, has_texture)
        };

        // Create material with improved visual properties
        let material = if is_star {
            materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture,
                // Extreme Emissive to trigger the high bloom threshold (50.0)
                emissive: LinearRgba::from(Color::srgb(
                   150.0,
                   130.0,
                   100.0,
                )),
                unlit: true, // Stars self-illuminate, show texture directly
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
            // For textured bodies, use the actual body color for emissive
            // For non-textured bodies, use material_color
            let emissive_base = if has_dedicated_texture {
                base_color // Use the body's actual color for emissive
            } else {
                material_color
            };
            materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture,
                // Note: normal_map_texture is loaded but not applied yet
                // TODO: Enable once multi-layer rendering is fully implemented
                // normal_map_texture,
                // Small emissive for ambient visibility on dark side, but low enough not to wash out texture
                emissive: LinearRgba::from(emissive_base) * 0.01,
                perceptual_roughness: roughness,
                metallic,
                reflectance: 0.5, // Higher reflectance for better lighting response
                ..default()
            })
        };

        // Initial transform will be updated after precise orbital data is inserted
        let initial_pos = Vec3::ZERO;

        // Build entity with appropriate components
        let mesh = if body_data.body_type == BodyType::Ring {
            // Create a custom donut/annulus mesh for the rings
            // Inner radius is approx 74,500km, Outer is 140,000km from center
            // Ratio is ~0.53
            let inner_radius = visual_radius * 0.53;
            let outer_radius = visual_radius;
            
            // Create ring mesh with high segment count for smoothness
            // We'll define a helper function create_ring_mesh
            meshes.add(create_ring_mesh(outer_radius, inner_radius, 128))
        } else if body_data.body_type == BodyType::Asteroid {
             let seed = calculate_hash(&body_data.name);
             meshes.add(create_asteroid_mesh(visual_radius, body_data.radius, seed))
        } else {
            meshes.add(Sphere::new(visual_radius).mesh().uv(64, 32))
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
                visual_radius,
                asteroid_class: body_data.asteroid_class,
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
                .map(|g| AtmosphericGas::new(&g.name, g.percentage))
                .collect();
            
            let atmosphere = AtmosphereComposition::new(
                atmo_data.surface_pressure_mbar,
                atmo_data.surface_temperature_celsius,
                gases,
            );
            
            entity_commands.insert(atmosphere);
        }

        let entity = entity_commands.id();

        // Add cloud layer if texture exists (e.g. Earth, Venus)
        if let Some(clouds_tex) = clouds_texture {
             commands.entity(entity).with_children(|parent| {
                parent.spawn(PbrBundle {
                    mesh: meshes.add(Sphere::new(visual_radius * 1.015).mesh().uv(64, 32)), // 1.5% larger than surface
                    material: materials.add(StandardMaterial {
                        base_color_texture: Some(clouds_tex),
                        base_color: Color::WHITE,
                        // Use additive blending since cloud textures are often black/white
                        // This makes black transparent and white opaque/bright
                        alpha_mode: AlphaMode::Add, 
                        unlit: false, // Clouds should be lit by the sun
                        perceptual_roughness: 0.8, // Clouds are rough (diffuse)
                        reflectance: 0.6,
                        ..default()
                    }),
                    transform: Transform::default(), // Relative to parent (0,0,0)
                    ..default()
                });
             });
        }
        
        // Add night lights layer if texture exists (e.g. Earth)
        if let Some(night_tex) = night_texture {
            // Import the NightMaterial from visual_effects
            use crate::plugins::visual_effects::NightMaterial;
            
            commands.entity(entity).with_children(|parent| {
               parent.spawn(MaterialMeshBundle {
                   mesh: meshes.add(Sphere::new(visual_radius * 1.002).mesh().uv(64, 32)), // Just slightly above surface
                   material: materials_night.add(NightMaterial {
                       night_texture: night_tex,
                       // Sun is at 0,0,0. 
                       // Note: If we had moving sun or dynamic lights, we'd need to update this uniform every frame.
                       // For now, Sun is static at 0,0,0.
                       sun_position: Vec4::new(0.0, 0.0, 0.0, 0.0), 
                   }),
                   transform: Transform::default(),
                   ..default()
               });
            });
       }

        entity_map.insert(body_data.name.clone(), entity);
    }

    // Second pass: Set up parenting and logical hierarchy
    for body_data in &data.bodies {
        if let Some(entity) = entity_map.get(&body_data.name) {
            if let Some(parent_name) = &body_data.parent {
                if let Some(parent_entity) = entity_map.get(parent_name) {
                    // Always set LogicalParent for UI hierarchy
                    commands.entity(*entity).insert(LogicalParent(*parent_entity));
                    
                    // Only set spatial parent for moons and rings
                    // Planets use absolute coordinates to match their orbit paths
                    if body_data.body_type == BodyType::Moon || body_data.body_type == BodyType::Ring {
                        commands.entity(*entity).set_parent(*parent_entity);
                    }
                } else {
                    warn!("Parent {} not found for body {}", parent_name, body_data.name);
                }
            }
        }
    }

    // Third pass: Add lights and corona visuals to stars
    for body_data in &data.bodies {
        if body_data.body_type == BodyType::Star {
            if let Some(entity) = entity_map.get(&body_data.name) {
                // Recalculate radius for visual effects
                let visual_radius = (body_data.radius * STAR_RADIUS_SCALE).max(MIN_VISUAL_RADIUS);
                
                // Spawn light as a child of the star entity so it follows the star
                commands.entity(*entity).with_children(|parent| {
                    parent.spawn(PointLightBundle {
                        point_light: PointLight {
                            // Intensity needs to be extremely high because of the 1 AU = 1500.0 scale
                            // Physical sun is ~3.75e28 lumens.
                            // Scaled down to be reasonable for 10,000 lux at 1 AU (1500 units):
                            // I = E * 4 * pi * r^2 = 10000 * 4 * pi * 1500^2 ≈ 2.8e11
                            intensity: 2.8e11, 
                            range: 2.0e9, // Effectively infinite within solar system bounds
                            shadows_enabled: false, // Disable to prevent star mesh from blocking its own light
                            ..default()
                        },
                        transform: Transform::default(),
                        ..default()
                    });

                    // Add Soft Glow visual (Billboard)
                    // Simplified to a soft radial gradient that blooms
                    parent.spawn((
                        PbrBundle {
                            mesh: create_glow_mesh(meshes.as_mut(), visual_radius * 4.0),
                            material: materials.add(StandardMaterial {
                                base_color: Color::WHITE,
                                emissive: LinearRgba::from(Color::srgb(50.0, 30.0, 10.0)), // High emissive for bloom
                                alpha_mode: AlphaMode::Add,
                                unlit: true,
                                ..default()
                            }),
                            // Push it slightly behind the sun so it backgrounds it (-0.1 Z local space?)
                            // Actually billboard overrides rotation, so translation is world relative usually.
                            // Just put it at center.
                            transform: Transform::from_xyz(0.0, 0.0, 0.0),
                            ..default()
                        },
                        Billboard,
                    ));
                });
            }
        }
    }

    // Store handles that need linear color space conversion
    commands.insert_resource(LinearImageQueue {
        handles: linear_handle_queue,
    });

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

            let initial_coords = orbit_position_from_mean_anomaly(
                &kepler_orbit,
                kepler_orbit.mean_anomaly_epoch,
            );
            let initial_translation = Vec3::new(
                (initial_coords.x * SCALING_FACTOR) as f32,
                (initial_coords.y * SCALING_FACTOR) as f32,
                (initial_coords.z * SCALING_FACTOR) as f32,
            );

            commands.entity(*entity).insert((
                kepler_orbit,
                SpaceCoordinates::new(initial_coords),
                Transform::from_translation(initial_translation),
            ));

            // Determine orbit color and visibility based on body type
            // Terra Invicta-inspired colors with higher alpha for bright trail heads
            let (orbit_color, should_show) = match body_data.body_type {
                BodyType::Planet => {
                    // Planets: bright cyan/blue, high alpha — trail head glows
                    (Color::srgba(0.4, 0.75, 1.0, 0.85), true)
                }
                BodyType::DwarfPlanet => {
                    // Dwarf Planets: dimmer blue, hidden by default
                     (Color::srgba(0.3, 0.5, 0.8, 0.6), false)
                }
                BodyType::Moon => {
                    // Moons: softer green-cyan
                    (Color::srgba(0.3, 0.8, 0.7, 0.7), true)
                }
                BodyType::Asteroid | BodyType::Comet => {
                    // Asteroids/Comets: amber/yellow when selected
                    (Color::srgba(1.0, 0.7, 0.2, 0.65), false)
                }
                BodyType::Ring => (Color::srgba(0.0, 0.0, 0.0, 0.0), false),
                _ => (Color::srgba(0.5, 0.5, 0.5, 0.4), false),
            };

            commands.entity(*entity).insert(OrbitPath {
                color: orbit_color,
                visible: should_show,
                segments: 128, // High segment count for smooth fading trails
            });
        }
    }

    info!("Solar system setup complete!");
}

// System to convert any queued normal/specular images to linear format once they are loaded
fn apply_linear_to_images_system(
    mut images: ResMut<Assets<Image>>,
    mut queue: ResMut<LinearImageQueue>,
) {
    use bevy::render::render_resource::TextureFormat;

    // Retain only those handles that are not yet processed
    queue.handles.retain(|handle| {
        if let Some(image) = images.get_mut(handle) {
            // If image uses an sRGB format, switch it to the linear equivalent
            match image.texture_descriptor.format {
                TextureFormat::Rgba8UnormSrgb => {
                    image.texture_descriptor.format = TextureFormat::Rgba8Unorm;
                }
                TextureFormat::Bgra8UnormSrgb => {
                    image.texture_descriptor.format = TextureFormat::Bgra8Unorm;
                }
                // Add more mappings if other srgb formats are encountered
                _ => {}
            }

            // Processed — remove from queue
            false
        } else {
            // Not yet loaded — keep for future frames
            true
        }
    });
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

// Helper to create a flat ring (annulus) mesh
fn create_ring_mesh(outer_radius: f32, inner_radius: f32, segments: u32) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    // Create vertices
    for i in 0..=segments {
        let angle_fraction = i as f32 / segments as f32; // 0 to 1
        let angle = angle_fraction * std::f32::consts::TAU;
        let (sin, cos) = angle.sin_cos();

        // Inner vertex
        positions.push([inner_radius * cos, 0.0, inner_radius * sin]);
        normals.push([0.0, 1.0, 0.0]); // Up-facing normal
        
        // Outer vertex
        positions.push([outer_radius * cos, 0.0, outer_radius * sin]);
        normals.push([0.0, 1.0, 0.0]); // Up-facing normal

        // UV Mapping:
        // U coordinate maps to radius (0 = inner, 1 = outer)
        // V coordinate maps to angle (0 = 0deg, 1 = 360deg)
        uvs.push([0.0, angle_fraction]);
        uvs.push([1.0, angle_fraction]);
    }

    // Create indices (two triangles per segment)
    for i in 0..segments {
        let base = i * 2;
        // Vertices at this segment: base (inner), base+1 (outer)
        // Vertices at next segment: base+2 (inner), base+3 (outer)
        
        // Triangle 1: Inner-Current, Outer-Current, Inner-Next
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 1);

        // Triangle 2: Inner-Next, Outer-Next, Outer-Current
        indices.push(base + 2);
        indices.push(base + 3);
        indices.push(base + 1);
    }
    
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    
    mesh
}

// Helper to create a soft radial gradient mesh (disk)
fn create_glow_mesh(meshes: &mut Assets<Mesh>, radius: f32) -> Handle<Mesh> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut colors = Vec::new();

    // Center vertex
    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 0.0, 1.0]);
    uvs.push([0.5, 0.5]);
    colors.push([1.0, 0.9, 0.7, 1.0]); // Warm center, full alpha

    let segments = 32;
    for i in 0..=segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let (sin, cos) = angle.sin_cos();

        positions.push([cos * radius, sin * radius, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.5 + cos * 0.5, 0.5 + sin * 0.5]);
        colors.push([0.8, 0.4, 0.1, 0.0]); // Orange edge, zero alpha (transparent)
    }

    // Indices (Fan)
    for i in 1..=segments {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    
    meshes.add(mesh)
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn create_asteroid_mesh(visual_radius: f32, physical_radius_km: f32, seed: u64) -> Mesh {
    // Generate base sphere
    // Use lower resolution for asteroids to make them look more jagged naturally,
    // but high enough to support the noise deformation.
    // 32 sectors, 16 stacks
    let mut mesh = Mesh::from(Sphere::new(visual_radius).mesh().uv(32, 16));

    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        let mut rng = StdRng::seed_from_u64(seed);
        
        // Define random axes for sine wave superposition
        let mut axes = Vec::new();
        let mut phases = Vec::new();
        let num_layers = 6;
        
        for _ in 0..num_layers {
            axes.push(Vec3::new(
                rng.gen::<f32>() * 2.0 - 1.0, 
                rng.gen::<f32>() * 2.0 - 1.0, 
                rng.gen::<f32>() * 2.0 - 1.0
            ).normalize_or_zero());
            phases.push(rng.gen::<f32>() * std::f32::consts::TAU);
        }

        // Determine roughness based on physical size
        // Bodies > 500km tend to be spherical (hydrostatic equilibrium)
        // Bodies < 200km are very irregular
        let irregularity_factor = if physical_radius_km > 500.0 {
            0.05 // Mostly round
        } else if physical_radius_km > 200.0 {
            // Linear interpolation from 0.05 at 500km to 0.4 at 200km
            0.05 + (1.0 - (physical_radius_km - 200.0) / 300.0) * 0.35
        } else {
            0.4 // Very irregular
        };

        let new_positions: Vec<[f32; 3]> = positions.iter().map(|p| {
            let v = Vec3::from(*p);
            let dir = v.normalize_or_zero();
            
            let mut noise = 0.0;
            for i in 0..num_layers {
                let frequency = 2.0 + (i as f32); // increasing frequency
                let val = (dir.dot(axes[i]) * frequency + phases[i]).sin();
                noise += val * (1.0 / (i as f32 + 1.0)); // decreasing amplitude
            }
            
            // Normalize noise to roughly -1 to 1 range
            noise /= 2.5; 
            
            let displacement = 1.0 + noise * irregularity_factor;
            
            (dir * visual_radius * displacement).into()
        }).collect();

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_positions);
        
        // Essential to recompute normals so lighting looks correct on the deformed mesh
        // We want a flat-shaded, faceted look for asteroids, so we must duplicate vertices
        // to make the mesh non-indexed before computing flat normals.
        mesh.duplicate_vertices();
        mesh.compute_flat_normals(); 
    }
    
    mesh
}
