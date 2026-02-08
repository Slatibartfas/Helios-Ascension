use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use rand::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::solar_system_data::{
    calculate_visual_radius, AsteroidClass, BodyType, SolarSystemData, MIN_VISUAL_RADIUS,
};
use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::economy::components::{Population, PowerGenerator, PowerSourceType};
use crate::astronomy::{
    orbit_position_from_mean_anomaly, KeplerOrbit, LocalOrbitAmplification, OrbitPath,
    SpaceCoordinates, SCALING_FACTOR, SurfaceTemperature,
};
use crate::plugins::camera::{CameraAnchor, GameCamera};
use crate::ui::SimulationTime;

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<StarGlowMaterial>::default())
            .add_systems(Startup, setup_solar_system)
            .add_systems(PostStartup, initial_camera_focus)
            .add_systems(
                Update,
                (
                    rotate_bodies,
                    update_billboards,
                    update_body_visibility,
                    update_star_glare_lod,
                ),
            )
            // System to convert loaded normal/specular textures to linear formats
            .add_systems(Update, apply_linear_to_images_system);
    }
}

/// Material for the star glow/corona effect
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarGlowMaterial {
    #[uniform(0)]
    pub color_core: Vec4,
    #[uniform(1)]
    pub color_halo: Vec4,
}

impl Material for StarGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/star_glow.wgsl".into()
    }

    // Set transparency mode to additive blending
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    // Force rendering on top of the star mesh to prevent clipping/z-fighting
    fn depth_bias(&self) -> f32 {
        100.0
    }
}

/// Component to make an entity always face the camera (e.g. sun glare)
#[derive(Component)]
pub struct Billboard;

/// Component to tag the star glare entity for LOD updates
#[derive(Component)]
pub struct StarGlare {
    pub base_core_color: Vec4,
    pub base_halo_color: Vec4,
}

fn update_billboards(
    mut query: Query<(&mut Transform, &GlobalTransform, &Parent), With<Billboard>>,
    parent_query: Query<&GlobalTransform, Without<Billboard>>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
) {
    if let Ok(camera_global_transform) = camera_query.get_single() {
        let camera_pos = camera_global_transform.translation();
        for (mut transform, _global, parent) in query.iter_mut() {
            // Compute the billboard's world position from its parent
            let parent_global = parent_query
                .get(parent.get())
                .map(|g| g.compute_transform())
                .unwrap_or_default();

            let world_pos = parent_global.transform_point(transform.translation);

            // Compute world-space rotation that faces the camera
            let forward = (camera_pos - world_pos).normalize_or_zero();
            if forward.length_squared() < 0.001 {
                continue;
            }
            let world_rotation = Transform::IDENTITY.looking_at(-forward, Vec3::Y).rotation;

            // Convert to local space by removing the parent's rotation
            transform.rotation = parent_global.rotation.inverse() * world_rotation;
        }
    }
}
/// Updates visibility of celestial bodies based on the current star system
fn update_body_visibility(
    current_system: Res<CurrentStarSystem>,
    mut param_set: ParamSet<(
        // Case 1: System Changed - update everyone
        Query<(&mut Visibility, &SystemId), With<CelestialBody>>,
        // Case 2: System Stable - update only new/changed bodies
        Query<(&mut Visibility, &SystemId), (With<CelestialBody>, Changed<SystemId>)>,
    )>,
) {
    if current_system.is_changed() {
        for (mut vis, system_id) in param_set.p0().iter_mut() {
            *vis = if system_id.0 == current_system.0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    } else {
        for (mut vis, system_id) in param_set.p1().iter_mut() {
            *vis = if system_id.0 == current_system.0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Dynamically adjusts star glare intensity/opacity based on camera distance (LOD).
/// When zoomed out, the glare is full brightness, hiding the surface.
/// When zoomed in close, the glare fades to transparent, revealing the star surface.
fn update_star_glare_lod(
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut glare_query: Query<(&GlobalTransform, &Handle<StarGlowMaterial>, &StarGlare)>,
    mut materials: ResMut<Assets<StarGlowMaterial>>,
) {
    if let Ok(cam_transform) = camera_query.get_single() {
        let cam_pos = cam_transform.translation();

        for (glare_transform, mat_handle, glare_data) in glare_query.iter_mut() {
            let glare_pos = glare_transform.translation();
            let distance = (cam_pos - glare_pos).length();

            // Defined LOD ranges based on typical Solar System visual scale
            // Visual radius of Sun is approx 104.0
            // Close up: ~200-400 units
            // Far out: > 2000 units

            let min_dist = 200.0; // Fully transparent (revealing surface)
            let max_dist = 1500.0; // Fully opaque/bright (hidden surface)

            // Normalize distance 0.0 .. 1.0
            let t = ((distance - min_dist) / (max_dist - min_dist)).clamp(0.0, 1.0);

            // Apply easing curve for smoother transition (e.g. smoothstep or quadratic)
            // t * t gives a slower fade-in, keeping surface visible longer
            // sqrt(t) gives faster fade-in
            let t_eased = t * t * (3.0 - 2.0 * t); // Smoothstep

            if let Some(material) = materials.get_mut(mat_handle) {
                // Modulate brightness.
                // We keep the HDR intensity but fade alpha/mix to 0 when close.
                // Multiplying the whole vector scales both brightness and alpha (if alpha is in .w)

                material.color_core = glare_data.base_core_color * t_eased;
                material.color_halo = glare_data.base_halo_color * t_eased;

                // Ensure alpha doesn't drop below 0 (vector mul handles this)
                // When t -> 0, colors -> 0 (black/transparent with Additive blending)
            }
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

impl CelestialBody {
    /// Calculate surface gravity in Earth g (9.80665 m/s²)
    /// formula: g = GM/r²
    pub fn surface_gravity(&self) -> f32 {
        if self.radius <= 0.0 {
            return 0.0;
        }
        
        const G: f64 = 6.674e-11; // Gravitational constant
        const G_EARTH: f64 = 9.80665; // Earth gravity
        
        let radius_m = self.radius as f64 * 1000.0;
        let surface_gravity_m_s2 = G * self.mass / (radius_m * radius_m);
        
        (surface_gravity_m_s2 / G_EARTH) as f32
    }
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
pub struct GasGiant;

#[derive(Component)]
pub struct Ring;

/// Axial tilt (obliquity) and north-pole direction of a celestial body.
/// `obliquity` is the angle between the spin axis and the ecliptic normal (radians).
/// `north_pole_ra` is the right-ascension direction the north pole tilts toward (radians).
#[derive(Component)]
pub struct AxialTilt {
    pub obliquity: f32,
    pub north_pole_ra: f32,
}

#[derive(Component)]
pub struct RotationSpeed(pub f32);

// Constants moved to solar_system_data.rs

// Time conversion constants
const SECONDS_PER_DAY: f64 = 86400.0; // Number of seconds in one Earth day

/// Determine which generic texture to use for a body without a dedicated texture
fn get_generic_texture_path(
    body_data: &super::solar_system_data::CelestialBodyData,
) -> Option<String> {
    match body_data.body_type {
        BodyType::Asteroid => {
            // Choose based on asteroid class
            let class = body_data.asteroid_class.unwrap_or(AsteroidClass::CType);
            match class {
                AsteroidClass::CType => {
                    Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
                }
                AsteroidClass::SType => {
                    Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string())
                }
                // M-Type: Metallic - use S-type for now, procedural variation adds metallic property
                AsteroidClass::MType => {
                    Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string())
                }
                // V-Type: Basaltic - use S-type for now, procedural variation adds reddish tint
                AsteroidClass::VType => {
                    Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string())
                }
                // D-Type: Dark primitive - use C-type (both very dark), procedural variation enhances darkness
                AsteroidClass::DType => {
                    Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
                }
                // P-Type: Primitive - use C-type (both dark), procedural variation creates distinction
                AsteroidClass::PType => {
                    Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
                }
                AsteroidClass::Unknown => {
                    Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
                }
            }
        }
        BodyType::Comet => Some("textures/celestial/comets/generic_nucleus_2k.jpg".to_string()),
        BodyType::Moon => {
            // Use a generic icy or rocky texture based on density
            // For now, use the C-type asteroid texture as a generic rocky surface
            Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
        }
        BodyType::DwarfPlanet => {
            // Dwarf planets without dedicated textures use a generic rocky surface
            // Procedural color/brightness variation makes each one look distinct
            // Use C-type for darker/icy KBOs, S-type for rockier ones
            let mut seed = 0u32;
            for byte in body_data.name.bytes() {
                seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
            }
            if seed % 3 == 0 {
                Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string())
            } else {
                Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
            }
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
            // Comets: Wide variety from pristine icy to dark carbonaceous
            // Use multiple random values for more distinct appearances
            let comet_type = (random1 * 5.0) as u32;
            match comet_type {
                0 => {
                    // Pristine icy comet - bluish-white
                    let brightness = 0.75 + random2 * 0.25;
                    Color::srgb(brightness * 0.85, brightness * 0.90, brightness * 1.0)
                }
                1 => {
                    // Dusty/old comet - warm brown/tan
                    let brightness = 0.4 + random2 * 0.3;
                    Color::srgb(brightness * 1.1, brightness * 0.85, brightness * 0.65)
                }
                2 => {
                    // Dark carbonaceous nucleus
                    let brightness = 0.25 + random2 * 0.2;
                    Color::srgb(brightness * 1.0, brightness * 0.95, brightness * 0.85)
                }
                3 => {
                    // Reddish organic-rich surface
                    let brightness = 0.45 + random2 * 0.25;
                    Color::srgb(brightness * 1.2, brightness * 0.75, brightness * 0.6)
                }
                _ => {
                    // Mixed ice and dust - gray with slight variation
                    let brightness = 0.5 + random2 * 0.3;
                    let tint = random3 * 0.15;
                    Color::srgb(brightness + tint, brightness, brightness - tint * 0.5)
                }
            }
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
        BodyType::DwarfPlanet => {
            // Dwarf planets: diverse surface compositions
            // KBOs range from bright icy to dark reddish
            let dp_type = (random1 * 6.0) as u32;
            match dp_type {
                0 => {
                    // Bright icy surface (like Eris/Makemake)
                    let brightness = 0.85 + random2 * 0.15;
                    Color::srgb(brightness * 0.95, brightness * 0.95, brightness * 1.0)
                }
                1 => {
                    // Reddish tholins (like Sedna/Quaoar)
                    let brightness = 0.55 + random2 * 0.25;
                    Color::srgb(
                        (brightness * 1.25).min(1.0),
                        brightness * 0.78,
                        brightness * 0.6,
                    )
                }
                2 => {
                    // Gray rocky (like Orcus)
                    let brightness = 0.6 + random2 * 0.2;
                    Color::srgb(brightness, brightness * 0.97, brightness * 0.95)
                }
                3 => {
                    // Dark with slight blue tint (water ice patches)
                    let brightness = 0.45 + random2 * 0.2;
                    Color::srgb(brightness * 0.9, brightness * 0.92, brightness * 1.05)
                }
                4 => {
                    // Warm brownish (like Haumea family)
                    let brightness = 0.65 + random2 * 0.2;
                    Color::srgb(brightness * 1.05, brightness * 0.92, brightness * 0.8)
                }
                _ => {
                    // Neutral slightly varied
                    let brightness = 0.55 + random2 * 0.25;
                    let tint = (random3 - 0.5) * 0.1;
                    Color::srgb(
                        (brightness + tint).clamp(0.0, 1.0),
                        brightness.clamp(0.0, 1.0),
                        (brightness - tint * 0.5).clamp(0.0, 1.0),
                    )
                }
            }
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
        } else if body_data.body_type == BodyType::Comet {
            0.75 + random2 * 0.2 // 0.75 to 0.95 (rough, irregular surface)
        } else if body_data.body_type == BodyType::DwarfPlanet {
            0.6 + random2 * 0.25 // 0.6 to 0.85 (varied surfaces)
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
        BodyType::Comet => 0.02 + random3 * 0.06, // 0.02 to 0.08 (low metallic, icy/dusty)
        BodyType::DwarfPlanet => 0.05 + random3 * 0.15, // 0.05 to 0.2 (varied)
        _ => 0.1 + random3 * 0.1,                 // 0.1 to 0.2 for others
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
    mut materials_glow: ResMut<Assets<StarGlowMaterial>>,
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
        // Calculate visual radius (with minimum for visibility)
        let visual_radius = calculate_visual_radius(body_data.body_type, body_data.radius);

        // Calculate rotation speed (convert from days to radians per second)
        let rotation_speed = if body_data.rotation_period != 0.0 {
            (2.0 * std::f32::consts::PI)
                / (body_data.rotation_period.abs() * SECONDS_PER_DAY as f32)
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
        let (
            base_color_texture,
            _normal_map_texture,
            clouds_texture,
            night_texture,
            has_dedicated_texture,
        ) = if let Some(ref multi) = body_data.multi_layer_textures {
            // Multi-layer textures - use base texture and normal map for now
            // TODO: Implement full multi-layer rendering with night/clouds/specular
            //       See assets/textures/MULTI_LAYER_TEXTURES.md for implementation roadmap
            let base_tex = Some(asset_server.load::<Image>(multi.base.clone()));
            let normal_tex = multi
                .normal
                .as_ref()
                .map(|path| asset_server.load::<Image>(path.clone()));
            let clouds_tex = multi
                .clouds
                .as_ref()
                .map(|path| asset_server.load::<Image>(path.clone()));
            let night_tex = multi
                .night
                .as_ref()
                .map(|path| asset_server.load::<Image>(path.clone()));

            // Also load specular if present so we can ensure it's treated as linear (even if not used by StandardMaterial yet)
            let specular_tex = multi
                .specular
                .as_ref()
                .map(|path| asset_server.load::<Image>(path.clone()));
            // Collect normal/specular handles for later conversion to linear color space
            if let Some(ref h) = normal_tex {
                linear_handle_queue.push(h.clone());
            }
            if let Some(ref h) = specular_tex {
                linear_handle_queue.push(h.clone());
            }
            // Night needs to be linear? Probably sRGB for emissive, but if it behaves as data, maybe linear.
            // Usually diffuse/emissive maps are sRGB.

            (base_tex, normal_tex, clouds_tex, night_tex, true)
        } else if let Some(ref texture) = body_data.texture {
            // Single dedicated texture
            (
                Some(asset_server.load(texture.clone())),
                None,
                None,
                None,
                true,
            )
        } else {
            // Generic texture based on body type
            let generic_path = get_generic_texture_path(body_data);
            (
                generic_path.map(|path| asset_server.load(path)),
                None,
                None,
                None,
                false,
            )
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
                // Emissive above bloom threshold (50.0) – white-yellow like the real Sun
                emissive: LinearRgba::from(Color::srgb(80.0, 76.0, 68.0)),
                unlit: true,               // Stars self-illuminate, show texture directly
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
                base_color_texture: base_color_texture.clone(),
                // Note: normal_map_texture is loaded but not applied yet
                // TODO: Enable once multi-layer rendering is fully implemented
                // normal_map_texture,
                // Subtle emissive so the dark side isn't pitch-black.
                // Use the base-color texture as emissive_texture so the glow
                // preserves surface detail instead of washing it out with a flat color.
                emissive: LinearRgba::WHITE * 0.02,
                emissive_texture: base_color_texture,
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
        } else if body_data.body_type == BodyType::Asteroid
            || body_data.body_type == BodyType::Comet
        {
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

        // Add axial tilt if present (convert degrees to radians)
        if body_data.axial_tilt != 0.0 || body_data.north_pole_ra != 0.0 {
            entity_commands.insert(AxialTilt {
                obliquity: body_data.axial_tilt.to_radians(),
                north_pole_ra: body_data.north_pole_ra.to_radians(),
            });
        }

        // Add type-specific component
        match body_data.body_type {
            BodyType::Star => {
                entity_commands.insert(Star);
            }
            BodyType::Planet => {
                entity_commands.insert(Planet);
            }
            BodyType::GasGiant => {
                // Gas giants are planets but have a distinct marker component
                entity_commands.insert(Planet);
                entity_commands.insert(GasGiant);
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

        let mut surface_temperature_celsius = -200.0; // Default cold vacuum

        // Add atmosphere component if the body has atmosphere data
        if let Some(ref atmo_data) = body_data.atmosphere {
            use crate::astronomy::{AtmosphereComposition, AtmosphericGas};

            surface_temperature_celsius = atmo_data.surface_temperature_celsius;

            // Convert gas data from deserialized format to runtime format
            let gases: Vec<AtmosphericGas> = atmo_data
                .gases
                .iter()
                .map(|g| AtmosphericGas::new(&g.name, g.percentage))
                .collect();

            let atmosphere = AtmosphereComposition::new_with_body_data(
                atmo_data.surface_pressure_mbar,
                atmo_data.surface_temperature_celsius,
                gases,
                body_data.mass,
                body_data.radius,
                atmo_data.is_reference_pressure,
            );

            entity_commands.insert(atmosphere);
        } else if let Some(ref orbit_data) = body_data.orbit {
             // If no atmosphere, approximate temperature based on distance from Sun
             // Sol Effective Temp ~ 5778 K
             // Simplified black body approximation: T = 278 K / sqrt(r_au)
             // (Assuming typical albedo ~0.3 for rocky bodies)
             if orbit_data.semi_major_axis > 0.0 {
                 let temp_k = 278.0 / orbit_data.semi_major_axis.sqrt();
                 surface_temperature_celsius = temp_k - 273.15;
             }
        }

        entity_commands.insert(SurfaceTemperature {
            average_celsius: surface_temperature_celsius,
            min_celsius: surface_temperature_celsius - 50.0, // Simple range
            max_celsius: surface_temperature_celsius + 50.0,
        });

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
                        unlit: false,              // Clouds should be lit by the sun
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

        // Initialize population
        // Earth starts with ~8.2 Billion people. Others empty.
        let population_count = if body_data.name == "Earth" {
            8_200_000_000.0
        } else {
            0.0
        };
        commands.entity(entity).insert(Population { count: population_count });

        // Initialize power generation
        // Earth starts with ~20 TW (Type 0.73 civilization)
        if body_data.name == "Earth" {
            commands.entity(entity).insert(PowerGenerator {
                output: 20_000_000_000_000.0, // 20 TW
                source_type: PowerSourceType::Planet,
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
                    commands
                        .entity(*entity)
                        .insert(LogicalParent(*parent_entity));

                    // Only set spatial parent for rings (they rotate with their planet)
                    // Moons and planets use world-space coordinates so that the
                    // parent planet's spin rotation does NOT drag moon positions
                    if body_data.body_type == BodyType::Ring {
                        commands.entity(*entity).set_parent(*parent_entity);
                    }
                } else {
                    warn!(
                        "Parent {} not found for body {}",
                        parent_name, body_data.name
                    );
                }
            }
        }
    }

    // Third pass: Add lights and corona visuals to stars
    for body_data in &data.bodies {
        if body_data.body_type == BodyType::Star {
            if let Some(entity) = entity_map.get(&body_data.name) {
                // Recalculate radius for visual effects
                let visual_radius = calculate_visual_radius(body_data.body_type, body_data.radius);

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
                    // 使用 custom shader for a high-quality procedural corona/glare
                    // Size is large because the shader fades out significantly towards the edges
                    let core_col = Vec4::new(5.0, 5.0, 5.0, 1.0); // Blinding white core
                    let halo_col = Vec4::new(4.0, 2.5, 0.5, 1.0); // Golden/Orange halo

                    parent.spawn((
                        MaterialMeshBundle {
                            mesh: meshes
                                .add(Rectangle::new(visual_radius * 12.0, visual_radius * 12.0)),
                            material: materials_glow.add(StarGlowMaterial {
                                color_core: core_col,
                                color_halo: halo_col,
                            }),
                            transform: Transform::from_translation(Vec3::Z * 0.1), // Slight offset to avoid z-fighting with star
                            ..default()
                        },
                        Billboard,
                        StarGlare {
                            base_core_color: core_col,
                            base_halo_color: halo_col,
                        },
                    ));
                });
            }
        }
    }

    // Store handles that need linear color space conversion
    commands.insert_resource(LinearImageQueue {
        handles: linear_handle_queue,
    });

    // ── Compute per-moon adaptive orbit amplification ───────────────
    // Moons' orbital distances in Bevy units are tiny compared to the
    // parent's upscaled visual radius, so they end up *inside* the mesh.
    //
    // Universe Sandbox-style approach: map all moon orbits into a bounded
    // visual range using logarithmic spacing:
    //   inner bound = parent_visual_radius * INNER_MOON_MULTIPLIER
    //   outer bound = parent_visual_radius * OUTER_MOON_MULTIPLIER
    // This keeps orbits compact, preserves orbital ordering via log
    // distribution, and works well regardless of how many moons a planet has.

    /// Innermost moon orbits at this multiple of parent visual radius
    const INNER_MOON_MULTIPLIER: f64 = 2.0;
    /// Outermost moon orbits at this multiple of parent visual radius
    const OUTER_MOON_MULTIPLIER: f64 = 10.0;

    // Per-moon amplification factor: moon_name → amplification
    let mut moon_amplification: HashMap<String, f32> = HashMap::new();
    {
        // Group moons by parent, collecting (name, semi_major_axis)
        let mut moons_by_parent: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        for body_data in &data.bodies {
            if body_data.body_type == BodyType::Moon {
                if let (Some(parent_name), Some(orbit)) = (&body_data.parent, &body_data.orbit) {
                    moons_by_parent
                        .entry(parent_name.clone())
                        .or_default()
                        .push((body_data.name.clone(), orbit.semi_major_axis as f64));
                }
            }
        }

        for (parent_name, moons) in &moons_by_parent {
            // Find parent visual radius
            let parent_visual_radius = data
                .bodies
                .iter()
                .find(|b| &b.name == parent_name)
                .map(|b| calculate_visual_radius(b.body_type, b.radius))
                .unwrap_or(MIN_VISUAL_RADIUS) as f64;

            let inner_display = parent_visual_radius * INNER_MOON_MULTIPLIER;
            let outer_display = parent_visual_radius * OUTER_MOON_MULTIPLIER;

            // Find min/max real orbit distances
            let min_orbit = moons.iter().map(|(_, a)| *a).fold(f64::MAX, f64::min);
            let max_orbit = moons.iter().map(|(_, a)| *a).fold(f64::MIN, f64::max);

            for (moon_name, orbit_au) in moons {
                let orbit_bevy = orbit_au * SCALING_FACTOR;

                if moons.len() == 1 || (max_orbit / min_orbit) < 1.01 {
                    // Single moon or all at same distance: place at midpoint
                    let mid_display = (inner_display + outer_display) * 0.5;
                    let amp = (mid_display / orbit_bevy).max(1.0) as f32;
                    moon_amplification.insert(moon_name.clone(), amp);
                } else {
                    // Log-space interpolation for even visual distribution
                    let log_min = min_orbit.ln();
                    let log_max = max_orbit.ln();
                    let t = (orbit_au.ln() - log_min) / (log_max - log_min);

                    let display_distance = inner_display + t * (outer_display - inner_display);
                    let amp = (display_distance / orbit_bevy).max(1.0) as f32;
                    moon_amplification.insert(moon_name.clone(), amp);
                }
            }
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

            let initial_coords =
                orbit_position_from_mean_anomaly(&kepler_orbit, kepler_orbit.mean_anomaly_epoch);

            // Apply local orbit amplification for moons (per-moon adaptive factor)
            let amp = if body_data.body_type == BodyType::Moon {
                moon_amplification
                    .get(&body_data.name)
                    .copied()
                    .unwrap_or(1.0)
            } else {
                1.0
            };

            let initial_translation = Vec3::new(
                (initial_coords.x * SCALING_FACTOR * amp as f64) as f32,
                (initial_coords.y * SCALING_FACTOR * amp as f64) as f32,
                (initial_coords.z * SCALING_FACTOR * amp as f64) as f32,
            );

            let mut entity_cmds = commands.entity(*entity);
            entity_cmds.insert((
                kepler_orbit,
                SpaceCoordinates::new(initial_coords),
                Transform::from_translation(initial_translation),
            ));

            // Insert amplification component for moons
            if body_data.body_type == BodyType::Moon && amp > 1.0 {
                entity_cmds.insert(LocalOrbitAmplification(amp));
            }

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

/// Analytically computes body rotation from total elapsed simulation time.
/// Instead of accumulating incremental `rotate_y()` calls (which drift and
/// break at high time-scales), we compute the absolute rotation directly: angle = speed × t.
///
/// When an `AxialTilt` is present the spin axis is oriented in 3-D:
///   1. Spin by `angle` around local Y (body’s day/night cycle)
///   2. Tilt by `obliquity` around X (lean the pole)
///   3. Rotate by `north_pole_ra` around Y (orient the lean direction)
fn rotate_bodies(
    sim_time: Res<SimulationTime>,
    mut query: Query<(&mut Transform, &RotationSpeed, Option<&AxialTilt>)>,
) {
    let t = sim_time.elapsed_seconds() as f32;
    for (mut transform, rotation_speed, axial_tilt) in query.iter_mut() {
        // Preserve existing translation and scale, only replace rotation
        let angle = rotation_speed.0 * t;
        let spin = Quat::from_rotation_y(angle);

        transform.rotation = if let Some(tilt) = axial_tilt {
            // Orient the tilt direction (north pole RA), then tilt, then spin
            let ra = Quat::from_rotation_y(tilt.north_pole_ra);
            let obl = Quat::from_rotation_x(tilt.obliquity);
            ra * obl * spin
        } else {
            spin
        };
    }
}

// Sets the initial camera focus to the Sun
fn initial_camera_focus(
    query_bodies: Query<(Entity, &CelestialBody), With<Star>>,
    mut query_camera: Query<&mut CameraAnchor, With<GameCamera>>,
) {
    // Find Sol
    let sol_entity = query_bodies
        .iter()
        .find(|(_, body)| body.name == "Sol")
        .map(|(e, _)| e);

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

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    mesh
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn create_asteroid_mesh(visual_radius: f32, physical_radius_km: f32, seed: u64) -> Mesh {
    // Generate base sphere
    // Use higher resolution for smoother look as requested
    // 64 sectors, 32 stacks
    let mut mesh = Mesh::from(Sphere::new(visual_radius).mesh().uv(64, 32));

    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        let mut rng = StdRng::seed_from_u64(seed);

        // Define random axes for sine wave superposition
        let mut axes = Vec::new();
        let mut phases = Vec::new();
        let num_layers = 6;

        for _ in 0..num_layers {
            axes.push(
                Vec3::new(
                    rng.gen::<f32>() * 2.0 - 1.0,
                    rng.gen::<f32>() * 2.0 - 1.0,
                    rng.gen::<f32>() * 2.0 - 1.0,
                )
                .normalize_or_zero(),
            );
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

        let new_positions: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| {
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
            })
            .collect();

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_positions);

        // Recompute normals for smooth shading
        mesh.compute_normals();
    }

    mesh
}
