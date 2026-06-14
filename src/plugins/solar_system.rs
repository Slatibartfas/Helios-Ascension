use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use image::{imageops::FilterType, ImageBuffer, RgbaImage};
use rand::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::solar_system_data::{
    calculate_visual_radius, AsteroidClass, BodyType, SolarSystemData, MIN_VISUAL_RADIUS,
};
use crate::astronomy::AtmosphereComposition;
use crate::astronomy::{
    orbit_position_from_mean_anomaly, KeplerOrbit, LocalOrbitAmplification, OceanProperties,
    OceanType, OrbitPath, SpaceCoordinates, StellarProperties, SurfaceTemperature, SCALING_FACTOR,
};
use crate::colony::{BuildingType, Colony};
use crate::economy::budget::GlobalBudget;
use crate::economy::components::{LocalStockpile, Population, SurveyLevel};
use crate::plugins::camera::{CameraAnchor, GameCamera};
use crate::ui::SimulationTime;

use super::star_materials::{
    update_billboards, update_body_visibility, update_corona_3d_time, update_glow_time,
    update_star_corona_3d_lod, update_star_diffraction_lod, update_star_glare_lod,
};
pub use super::star_materials::{
    Billboard, StarCorona3dMaterial, StarCoronaShell, StarDiffraction, StarDiffractionMaterial,
    StarGlare, StarGlowMaterial, StarHalo3dMaterial, StarHaloShell, StarSurfaceMaterial,
};
use super::starmap::PlanetCategory;

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<StarGlowMaterial>::default())
            .add_plugins(MaterialPlugin::<StarSurfaceMaterial>::default())
            .add_plugins(MaterialPlugin::<StarDiffractionMaterial>::default())
            .add_plugins(MaterialPlugin::<StarCorona3dMaterial>::default())
            .add_plugins(MaterialPlugin::<StarHalo3dMaterial>::default())
            .init_resource::<RingAlphaCombineQueue>()
            .add_systems(Startup, setup_solar_system)
            .add_systems(
                PostStartup,
                (initial_camera_focus, initialize_colony_stockpiles),
            )
            .add_systems(
                Update,
                (
                    rotate_bodies,
                    update_billboards,
                    update_body_visibility,
                    update_star_glare_lod,
                    update_star_diffraction_lod,
                    update_star_corona_3d_lod,
                    update_glow_time,
                    update_corona_3d_time,
                ),
            )
            // System to convert loaded normal/specular textures to linear formats
            .add_systems(Update, apply_linear_to_images_system)
            .add_systems(Update, combine_ring_alpha_textures)
            .add_systems(
                Update,
                (spawn_atmosphere_shell_reactive, update_atmosphere_shell),
            );
    }
}

/// Reactively spawns a scattering shell when a body gains an `AtmosphereComposition`
/// after startup (e.g. through a future terraforming system).
fn spawn_atmosphere_shell_reactive(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials_atmosphere: ResMut<Assets<crate::plugins::atmosphere::AtmosphereMaterial>>,
    atmosphere_settings: Res<crate::plugins::atmosphere::AtmosphereSettings>,
    query: Query<
        (
            Entity,
            &AtmosphereComposition,
            &CelestialBody,
            &GlobalTransform,
        ),
        (
            Added<AtmosphereComposition>,
            Without<crate::plugins::atmosphere::HasAtmosphereShell>,
        ),
    >,
) {
    use crate::plugins::atmosphere::{AtmosphereMaterial, AtmosphereShell, HasAtmosphereShell};
    if !atmosphere_settings.enabled {
        return;
    }
    for (entity, atmo, body, gtransform) in &query {
        if body.body_type == BodyType::Star {
            continue;
        }
        let planet_pos: Vec3 = gtransform.translation();
        let atmo_mat = AtmosphereMaterial::from_composition(
            body.visual_radius,
            atmo,
            planet_pos,
            Vec3::ZERO,
            atmosphere_settings.quality,
        );
        commands
            .entity(entity)
            .insert(HasAtmosphereShell)
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Sphere::new(body.visual_radius * 1.05).mesh().uv(64, 32))),
                    MeshMaterial3d(materials_atmosphere.add(atmo_mat)),
                    Transform::default(),
                    AtmosphereShell {
                        body_entity: entity,
                    },
                ));
            });
    }
}

/// Updates the shell material on any body whose `AtmosphereComposition` changed
/// (e.g. composition shift as terraforming progresses).
fn update_atmosphere_shell(
    mut materials: ResMut<Assets<crate::plugins::atmosphere::AtmosphereMaterial>>,
    atmosphere_settings: Res<crate::plugins::atmosphere::AtmosphereSettings>,
    changed_bodies: Query<
        (
            Entity,
            &AtmosphereComposition,
            &CelestialBody,
            &GlobalTransform,
            Option<&Children>,
        ),
        (
            Changed<AtmosphereComposition>,
            With<crate::plugins::atmosphere::HasAtmosphereShell>,
        ),
    >,
    shells: Query<(
        &crate::plugins::atmosphere::AtmosphereShell,
        &MeshMaterial3d<crate::plugins::atmosphere::AtmosphereMaterial>,
    )>,
) {
    if !atmosphere_settings.enabled {
        return;
    }
    for (entity, atmo, body, gtransform, maybe_children) in &changed_bodies {
        if body.body_type == BodyType::Star {
            continue;
        }
        let Some(children): Option<&Children> = maybe_children else {
            continue;
        };
        for child in children.iter() {
            if let Ok((shell, mat_handle)) = shells.get(child) {
                if shell.body_entity == entity {
                    if let Some(mat) = materials.get_mut(&mat_handle.0) {
                        let planet_pos: Vec3 = gtransform.translation();
                        *mat = crate::plugins::atmosphere::AtmosphereMaterial::from_composition(
                            body.visual_radius,
                            atmo,
                            planet_pos,
                            Vec3::ZERO,
                            atmosphere_settings.quality,
                        );
                    }
                }
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

/// Marker component for entities that cannot be clicked/ray-picked in the 3-D view.
/// They remain selectable via the ledger panel.
#[derive(Component)]
pub struct ClickExcluded;

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
            if seed.is_multiple_of(3) {
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

struct RingAlphaEntry {
    material_handle: Handle<StandardMaterial>,
    color_handle: Handle<Image>,
    alpha_handle: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct RingAlphaCombineQueue {
    entries: Vec<RingAlphaEntry>,
}

pub fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut materials_night: ResMut<Assets<crate::plugins::visual_effects::NightMaterial>>,
    mut materials_surface: ResMut<Assets<StarSurfaceMaterial>>,
    mut materials_corona_3d: ResMut<Assets<StarCorona3dMaterial>>,
    mut materials_halo_3d: ResMut<Assets<StarHalo3dMaterial>>,
    mut materials_atmosphere: ResMut<Assets<crate::plugins::atmosphere::AtmosphereMaterial>>,
    atmosphere_settings: Res<crate::plugins::atmosphere::AtmosphereSettings>,
    asset_server: Res<AssetServer>,
    mut ring_alpha_queue: ResMut<RingAlphaCombineQueue>,
    sim_time: Res<crate::ui::SimulationTime>,
) {
    // Queue to collect normal/specular handles that must be treated as linear textures
    let mut linear_handle_queue: Vec<Handle<Image>> = Vec::new();

    // Load solar system data
    let mut data = match SolarSystemData::load_from_file("assets/data/solar_system.ron") {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to load solar system data: {}", e);
            return;
        }
    };

    // Remove bodies that were permanently destroyed before the game's start date.
    // This covers historically destroyed comets (e.g. ISON 2013, SL-9 1994) so they
    // simply never appear when the game starts in an era after their destruction.
    // Bodies with no `destroyed_at` (the vast majority) are always kept.
    let start_ts = sim_time.start_timestamp();
    let pre_load = data.bodies.len();
    data.bodies
        .retain(|body| body.destroyed_at.is_none_or(|t| start_ts < t));
    let removed = pre_load - data.bodies.len();
    if removed > 0 {
        info!(
            "Skipped {} bod{} already destroyed before game start (Unix {})",
            removed,
            if removed == 1 { "y" } else { "ies" },
            start_ts
        );
    }

    info!("Loaded {} celestial bodies", data.bodies.len());

    // Pre-calculate distance to sun for all bodies to ensure correct temperature calculation for moons
    let mut distance_to_sun: HashMap<&String, f32> = HashMap::new();

    // Pass 1: Add Sol and direct children (planets)
    for body in &data.bodies {
        if body.name == "Sol" {
            distance_to_sun.insert(&body.name, 0.0);
        } else if let Some(orbit) = &body.orbit {
            if let Some(parent) = &body.parent {
                if parent == "Sol" {
                    distance_to_sun.insert(&body.name, orbit.semi_major_axis);
                }
            }
        }
    }

    // Pass 2: Add moons (children of planets around Sol)
    for body in &data.bodies {
        if !distance_to_sun.contains_key(&body.name) {
            if let Some(parent) = &body.parent {
                if let Some(parent_dist) = distance_to_sun.get(parent) {
                    distance_to_sun.insert(&body.name, *parent_dist);
                }
            }
        }
    }

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
            normal_map_texture,
            clouds_texture,
            clouds_blend_mode,
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
            let clouds_blend = multi.clouds_blend_mode.clone();
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

            (
                base_tex,
                normal_tex,
                clouds_tex,
                clouds_blend,
                night_tex,
                true,
            )
        } else if let Some(ref texture) = body_data.texture {
            // Single dedicated texture
            (
                Some(asset_server.load(texture.clone())),
                None,
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

        // Star surface material — uses limb darkening shader instead of StandardMaterial.
        // For non-star bodies, build the StandardMaterial as before (wrapped in Option
        // so we can choose which bundle to spawn below).
        let star_surface_mat: Option<Handle<StarSurfaceMaterial>> = if is_star {
            // Derive HDR center/limb colours from the body's emissive data.
            // body_data.emissive encodes the star's spectral colour at (0…10+) scale.
            // ×9 gives a blinding-white HDR centre that drives bloom;
            // the limb shifts cooler by strongly attenuating green and blue.
            let (er, eg, eb) = body_data.emissive;
            let center_col = Vec4::new(er * 9.0, eg * 9.0, eb * 9.0, 1.0);
            let limb_col = Vec4::new(er * 5.5, eg * 2.8, eb * 0.8, 1.0);
            Some(materials_surface.add(StarSurfaceMaterial {
                color_center: center_col,
                color_limb: limb_col,
                star_texture: base_color_texture.clone(),
            }))
        } else {
            None
        };

        // Non-star standard material
        let material: Option<Handle<StandardMaterial>> = if is_star {
            None
        } else if body_data.body_type == BodyType::Ring {
            let ring_material_handle = materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture: base_color_texture.clone(),
                perceptual_roughness: roughness,
                metallic: 0.0,
                reflectance: 0.2,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None, // Double-sided
                unlit: true,
                ..default()
            });

            if let (Some(color_handle), Some(alpha_path)) =
                (&base_color_texture, &body_data.ring_alpha_texture)
            {
                let alpha_handle = asset_server.load::<Image>(alpha_path.clone());
                ring_alpha_queue.entries.push(RingAlphaEntry {
                    material_handle: ring_material_handle.clone(),
                    color_handle: color_handle.clone(),
                    alpha_handle,
                });
            }

            Some(ring_material_handle)
        } else {
            Some(materials.add(StandardMaterial {
                base_color: material_color,
                base_color_texture: base_color_texture.clone(),
                normal_map_texture,
                // Minimal emissive floor so planets in dim/distant star systems
                // aren't pitch black on the night side.  Intentionally very low
                // so day/night contrast is still strong.
                emissive: LinearRgba::WHITE * 0.006,
                perceptual_roughness: roughness,
                metallic,
                reflectance: 0.3,
                ..default()
            }))
        };

        // Initial transform will be updated after precise orbital data is inserted
        let initial_pos = Vec3::ZERO;

        // Build entity with appropriate components
        let mesh = if body_data.body_type == BodyType::Ring {
            // Rings must not visually intersect their parent planet.
            // Because calculate_visual_radius uses a non-linear (radius^0.65) scale,
            // the naive physical ratio (74,500 / 140,000 ≈ 0.53) can place the inner
            // edge inside the parent's rendered sphere. Instead we derive the inner
            // edge from the parent planet's actual visual radius, plus a ~15% gap
            // for a realistic Cassini-gap breathing room.
            let parent_visual_radius = body_data
                .parent
                .as_deref()
                .and_then(|parent_name| data.bodies.iter().find(|b| b.name == parent_name))
                .map(|parent| calculate_visual_radius(parent.body_type, parent.radius))
                .unwrap_or(visual_radius * 0.55); // fallback: 55% of outer

            // Inner edge = parent surface + 15% clearance gap.
            // Outer edge is the ring body's own visual radius (unchanged).
            let inner_radius = parent_visual_radius * 1.15;
            let outer_radius = visual_radius;

            // Create ring mesh with high segment count for smoothness
            meshes.add(create_ring_mesh(outer_radius, inner_radius, 128))
        } else if body_data.body_type == BodyType::Asteroid
            || body_data.body_type == BodyType::Comet
        {
            let seed = calculate_hash(&body_data.name);
            meshes.add(create_asteroid_mesh(visual_radius, body_data.radius, seed))
        } else if body_data.body_type == BodyType::Star {
            // Higher resolution for stars to appear smooth and round
            meshes.add(Sphere::new(visual_radius).mesh().uv(128, 64))
        } else {
            meshes.add(Sphere::new(visual_radius).mesh().uv(64, 32))
        };

        // Stars use the limb-darkening StarSurfaceMaterial; all other bodies use PbrBundle.
        // compute classification string based on data; helper defined below
        fn classify_for_spawn(
            body_data: &super::solar_system_data::CelestialBodyData,
        ) -> &'static str {
            let mut seed = 0u32;
            for byte in body_data.name.bytes() {
                seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
            }

            // Airless rocky planets (no atmosphere data) are "barren" — e.g. Mercury.
            if body_data.body_type == BodyType::Planet && body_data.atmosphere.is_none() {
                return if seed.is_multiple_of(2) {
                    "barren"
                } else {
                    "rock"
                };
            }

            // mimic the logic used in starmap classification so categories agree
            let avg_temp = body_data
                .atmosphere
                .as_ref()
                .map(|a| a.surface_temperature_celsius)
                .unwrap_or(-100.0);
            crate::plugins::starmap::classify_exoplanet_with_mass(
                body_data.body_type,
                body_data.asteroid_class,
                avg_temp,
                seed,
                body_data.ocean_fraction.unwrap_or(0.0) > 0.0
                    && body_data.ocean_type != Some(OceanType::Subsurface),
                body_data.ocean_type == Some(OceanType::Water),
                Some(body_data.mass),
            )
        }

        let mut entity_commands = if let Some(star_mat) = star_surface_mat {
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(star_mat),
                Transform::from_translation(initial_pos),
                CelestialBody {
                    name: body_data.name.clone(),
                    radius: body_data.radius,
                    mass: body_data.mass,
                    body_type: body_data.body_type,
                    visual_radius,
                    asteroid_class: body_data.asteroid_class,
                },
                RotationSpeed(rotation_speed),
                // Stars sit at the system origin; give them SpaceCoordinates so they
                // are visible to queries that need to look up the star by entity
                // (e.g. the fleet transfer-planner solar-approach logic).
                SpaceCoordinates::new(bevy::math::DVec3::ZERO),
                PlanetCategory(classify_for_spawn(body_data).to_string()),
            ))
        } else {
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material.expect("non-star body must have StandardMaterial")),
                Transform::from_translation(initial_pos),
                CelestialBody {
                    name: body_data.name.clone(),
                    radius: body_data.radius,
                    mass: body_data.mass,
                    body_type: body_data.body_type,
                    visual_radius,
                    asteroid_class: body_data.asteroid_class,
                },
                RotationSpeed(rotation_speed),
                PlanetCategory(classify_for_spawn(body_data).to_string()),
            ))
        };

        // Add axial tilt if present (convert degrees to radians)
        if body_data.axial_tilt != 0.0 || body_data.north_pole_ra != 0.0 {
            entity_commands.insert(AxialTilt {
                obliquity: body_data.axial_tilt.to_radians(),
                north_pole_ra: body_data.north_pole_ra.to_radians(),
            });
        }

        // Initialize Earth as a colony
        if body_data.name == "Earth" {
            // PR-F (GRA-117): Earth keeps a `SurveyLevel` for
            // backward compatibility (legacy code paths still
            // look at the enum), but the canonical state is the
            // `SurveyState` inserted below — and it deliberately
            // does NOT map to `CoreSample`. The homeworld is
            // well-surveyed in 2026, but it is not 100% explored
            // (no full mantle drilling, no full ocean floor
            // mapping, etc.), and the v0.5.0 dossier wants
            // "Recommended next step" prompts to drive gameplay.
            // The tier-4 / tier-3 starter matches the real-world
            // record; the player can advance it with missions.
            entity_commands.insert(SurveyLevel::SeismicSurvey);
            // SurveyState is seeded further down by the
            // per-body helper (`for_named_solar_system_body`),
            // which gives Earth a tier-4 baseline and 0 drill
            // missions — the T3 (Planetary Bulk) gate stays
            // locked until the player actually drills.

            // Earth is a Civilisation-tier homeworld (× 1.00 yield).  Founding
            // a colony (i.e. `Colony::new()`) defaults to the Outpost tier
            // (× 0.10) per GRA-22 §4.5; the homeworld is the only colony
            // that starts above the Outpost package.
            let mut colony = Colony::new_civilisation("Earth".to_string(), 8.2e9); // 8.2 Billion

            // Add initial infrastructure
            let base_buildings = [
                // Housing: scaled for population capacity
                (BuildingType::Housing, 400),
                // Food: 820 Farms × 1,000 Mt/yr = 820,000 Mt/yr → feeds 8.2B ✓
                (BuildingType::Farm, 820),
                // Greenhouses: trimmed to a modest buffer above baseline food demand.
                (BuildingType::Greenhouse, 60),
                // Aquaculture: retained as a smaller supplemental protein buffer.
                (BuildingType::AquacultureFacility, 20),
                // Industry (scaled for ~2.8 TW consumption with room to build more)
                // These are reduced from full Earth capacity to give player building room
                (BuildingType::Factory, 1_200),
                (BuildingType::Mine, 2_000),
                (BuildingType::Refinery, 500),
                (BuildingType::ChemicalPlant, 700),
                (BuildingType::HydrocarbonExtractor, 300),
                (BuildingType::AtmosphericProcessor, 300),
                (BuildingType::RecyclingCenter, 300),
                // Power: effective-output 2026 baseline tuned for the coal/renewables flip
                // visible in 2025-2026 generation data.
                // Effective mix: Coal ~32.0%, Gas ~22.2%, Hydro ~15.2%, Nuclear ~9.9%,
                // Wind ~9.9%, Solar ~11.0%.
                // Wind + Solar combined ≈ 20.8% of delivered output, with total effective
                // generation ≈ 3.65 TW and ~14.5% reserve over the 3.19 TW starting load.
                (BuildingType::SolarPower, 320), // 320 × 1.25 = 400 GW
                (BuildingType::CoalPowerPlant, 195), // 195 × 6.0 = 1,170 GW
                (BuildingType::NaturalGasPlant, 135), // 135 × 6.0 = 810 GW
                (BuildingType::HydroelectricDam, 82), // 82 × 6.75 = 553.5 GW
                (BuildingType::WindFarm, 400),   // 400 × 0.9 = 360 GW
                (BuildingType::FissionReactor, 20), // 20 × 18 = 360 GW
                // Water
                (BuildingType::WaterTreatmentPlant, 500),
                // Research & Tech (high power consumers)
                (BuildingType::ResearchLab, 500),
                (BuildingType::DataCenter, 100), // 100 × 500 MW = 50 GW (realistic for early game)
                (BuildingType::AiCluster, 10),   // 10 × 2000 MW = 20 GW (very advanced tech)
                // Space access
                (BuildingType::LaunchSite, 200),
                (BuildingType::SpacePort, 50),
                (BuildingType::Shipyard, 18), // Still dominant, but no longer enough to trivialize ship construction timelines
                // Economy
                (BuildingType::FinancialCenter, 100),
                (BuildingType::CommercialHub, 500),
                (BuildingType::TradePort, 50),
                // Medical/Population
                (BuildingType::MedicalCenter, 200),
                (BuildingType::PharmaceuticalPlant, 100),
                // Storage infrastructure: 4 depots = +10% cap, keeping Earth within
                // the one-year stockpile target while preserving a small building margin.
                (BuildingType::Warehouse, 4),
            ];

            for (b_type, count) in base_buildings {
                for _ in 0..count {
                    colony.add_building(b_type);
                }
            }

            entity_commands.insert(colony);
            info!("Established Earth colony with 8.2B population");
        }

        // PR-F (GRA-117): every solar-system body gets a baseline
        // `SurveyState` at game start so the dossier SURVEY ledger
        // is visible from the moment the player selects a planet.
        //
        // Bodies in this spawn system are exclusively loaded from
        // `assets/data/solar_system.ron` (the Sol catalogue).
        // Procedurally-generated bodies in other star systems are
        // spawned by `system_populator` and never go through this
        // path — they remain unsurveyed until the player dispatches
        // a survey mission, at which point the dispatch handler
        // inserts a fresh `SurveyState` (see
        // `dispatch_survey_mission` in `survey::systems`).
        //
        // The per-body tier map reflects the real 2026 record:
        // - Stars: no `SurveyState` (the dossier's star-properties
        //   section is the authoritative read-out).
        // - Earth: tier-4 on well-explored dims, tier-3 on
        //   subsurface/anomalies, drill_missions_completed = 0
        //   so the T3 (Planetary Bulk) gate is still locked.
        // - Moon, Mars, Mercury, Venus, Titan, Ceres, Vesta:
        //   tier-5 on the dimensions the actual missions covered.
        // - Pluto, Charon, Triton, Galilean moons, Titan-class
        //   moons: tier-3/2 on the dimensions a flyby mapped.
        // - Phobos, Deimos, outer-planet minor moons, asteroids,
        //   comets: tier-1 ("telescope spotted") floor.
        // - Anything else in the RON catalogue (KBOs, dwarf
        //   planets past Pluto): tier-1 floor.
        if let Some(state) = crate::survey::components::SurveyState::for_named_solar_system_body(
            &body_data.name,
            body_data.body_type,
            body_data.atmosphere.is_some(),
            sim_time.elapsed_seconds(),
        ) {
            entity_commands.insert(state);
        }

        // Add type-specific component
        match body_data.body_type {
            BodyType::Star => {
                entity_commands.insert(Star);
                // Add stellar properties for all stars (default to Sol values if not specified)
                entity_commands.insert(StellarProperties::sol());
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
                entity_commands.insert((Ring, ClickExcluded));
            }
        }

        let mut surface_temperature_celsius = -200.0; // Default cold vacuum
        let mut min_temp_c = -200.0;
        let mut max_temp_c = -200.0;

        // Add atmosphere component if the body has atmosphere data
        if let Some(ref atmo_data) = body_data.atmosphere {
            use crate::astronomy::{AtmosphereComposition, AtmosphericGas};

            surface_temperature_celsius = atmo_data.surface_temperature_celsius;

            // Atmosphere moderates temperature swings.
            // Thick atmospheres (pressure > 0.5 bar) have smaller diurnal variations.
            let swing = if atmo_data.surface_pressure_mbar > 500.0 {
                // Earth/Venus like (Venus varies very little, <1C, but Earth ~10-20C)
                15.0
            } else {
                // Thin atmosphere (Mars) - Large swings (-125C to +20C)
                80.0
            };
            min_temp_c = surface_temperature_celsius - swing;
            max_temp_c = surface_temperature_celsius + swing;

            // Convert gas data from deserialized format to runtime format
            let gases: Vec<AtmosphericGas> = atmo_data
                .gases
                .iter()
                .map(|g| AtmosphericGas::new(&g.name, g.percentage))
                .collect();

            let mut atmosphere = AtmosphereComposition::new_with_body_data(
                atmo_data.surface_pressure_mbar,
                atmo_data.surface_temperature_celsius,
                gases,
                body_data.mass,
                body_data.radius,
                atmo_data.is_reference_pressure,
            );

            // Compute surface gravity for scattering derivation
            let surface_gravity_g = {
                const G_CONST: f64 = 6.674e-11;
                const G_EARTH: f64 = 9.80665;
                let radius_m = body_data.radius as f64 * 1000.0;
                if radius_m > 0.0 {
                    (G_CONST * body_data.mass / (radius_m * radius_m) / G_EARTH) as f32
                } else {
                    1.0
                }
            };

            // Derive scattering parameters from physical properties + optional RON overrides
            atmosphere.derive_scattering_params(
                surface_gravity_g,
                atmo_data.scale_height_km,
                atmo_data.rayleigh_rgb,
                atmo_data.rayleigh_strength,
                atmo_data.mie_strength,
                atmo_data.mie_g,
                atmo_data.haze_color,
                atmo_data.atmosphere_intensity,
            );

            entity_commands.insert(atmosphere.clone());

            // Spawn atmospheric scattering shell (translucent child sphere)
            // Deferred to after entity_commands scope — collect data for second pass
        } else if let Some(ref orbit_data) = body_data.orbit {
            // If no atmosphere, approximate temperature based on distance from Sun.
            // For moons, we must use the parent planet's distance to the Sun, NOT the moon's distance to the planet.
            let effective_distance = *distance_to_sun
                .get(&body_data.name)
                .unwrap_or(&orbit_data.semi_major_axis);

            // Sol Effective Temp ~ 5778 K
            // Simplified black body approximation: T = 255 K / sqrt(r_au)
            // Using 255 K (Earth equilibrium temp) instead of 278 K (Earth surface temp with greenhouse)
            // to better represent airless bodies like the Moon (Mean -20C to -50C)
            if effective_distance > 0.0 {
                let temp_k = 255.0 / effective_distance.sqrt();
                surface_temperature_celsius = temp_k - 273.15;

                // Airless bodies have extreme day/night differentials
                // Moon: Avg ~250K (-23C), Max ~390K (117C), Min ~100K (-173C)
                let max_k = temp_k * 1.55;
                let min_k = temp_k * 0.40;

                min_temp_c = min_k - 273.15;
                max_temp_c = max_k - 273.15;
            }
        }

        // Override for Stars
        if body_data.body_type == BodyType::Star {
            surface_temperature_celsius = 5500.0;
            min_temp_c = 5500.0;
            max_temp_c = 5500.0;
        }

        entity_commands.insert(SurfaceTemperature {
            average_celsius: surface_temperature_celsius,
            min_celsius: min_temp_c,
            max_celsius: max_temp_c,
        });

        // Insert ocean properties from RON data if present
        if let Some(fraction) = body_data.ocean_fraction {
            let ocean_type = body_data.ocean_type.unwrap_or(OceanType::Water);
            let depth = body_data.ocean_depth_km.unwrap_or(3.0);
            let is_subsurface = ocean_type == OceanType::Subsurface;
            entity_commands.insert(OceanProperties {
                ocean_type,
                surface_fraction: fraction,
                mean_depth_km: depth,
                is_subsurface,
            });
        }

        let entity = entity_commands.id();

        // Add cloud layer if texture exists (e.g. Earth, Venus)
        if let Some(clouds_tex) = clouds_texture {
            let alpha_mode = match clouds_blend_mode.as_deref() {
                Some("blend") => AlphaMode::Blend,
                Some("opaque") => AlphaMode::Opaque,
                _ => AlphaMode::Add, // Default to Add for Earth-like clouds
            };

            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Sphere::new(visual_radius * 1.015).mesh().uv(64, 32))), // 1.5% larger than surface
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color_texture: Some(clouds_tex),
                        base_color: Color::WHITE,
                        alpha_mode,
                        unlit: false,              // Clouds should be lit by the sun
                        perceptual_roughness: 0.8, // Clouds are rough (diffuse)
                        reflectance: 0.6,
                        // Negative depth_bias makes this layer sort as "further from camera"
                        // so it renders BEFORE (underneath) the atmosphere shell, which has
                        // depth_bias = +1.0. Prevents dark-side flickering when both children
                        // share the same world-space centre and Bevy can't determine order.
                        depth_bias: -1.0,
                        ..default()
                    })),
                    Transform::default(), // Relative to parent (0,0,0)
                ));
            });
        }

        // Add night lights layer if texture exists (e.g. Earth)
        if let Some(night_tex) = night_texture {
            // Import the NightMaterial from visual_effects
            use crate::plugins::visual_effects::NightMaterial;

            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Sphere::new(visual_radius * 1.002).mesh().uv(64, 32))), // Just slightly above surface
                    MeshMaterial3d(materials_night.add(NightMaterial {
                        night_texture: night_tex,
                        // Sun is at 0,0,0.
                        // Note: If we had moving sun or dynamic lights, we'd need to update this uniform every frame.
                        // For now, Sun is static at 0,0,0.
                        sun_position: Vec4::new(0.0, 0.0, 0.0, 0.0),
                    })),
                    Transform::default(),
                ));
            });
        }

        // Add atmospheric scattering shell if atmosphere data exists and scattering is enabled
        if atmosphere_settings.enabled && body_data.body_type != BodyType::Star {
            if let Some(ref atmo_data) = body_data.atmosphere {
                use crate::astronomy::{AtmosphereComposition, AtmosphericGas};
                use crate::plugins::atmosphere::{AtmosphereMaterial, AtmosphereShell};

                // Rebuild atmosphere for scattering (already inserted as component above)
                let gases: Vec<AtmosphericGas> = atmo_data
                    .gases
                    .iter()
                    .map(|g| AtmosphericGas::new(&g.name, g.percentage))
                    .collect();

                let mut atmo_comp = AtmosphereComposition::new_with_body_data(
                    atmo_data.surface_pressure_mbar,
                    atmo_data.surface_temperature_celsius,
                    gases,
                    body_data.mass,
                    body_data.radius,
                    atmo_data.is_reference_pressure,
                );

                let surface_gravity_g = {
                    const G_CONST: f64 = 6.674e-11;
                    const G_EARTH: f64 = 9.80665;
                    let radius_m = body_data.radius as f64 * 1000.0;
                    if radius_m > 0.0 {
                        (G_CONST * body_data.mass / (radius_m * radius_m) / G_EARTH) as f32
                    } else {
                        1.0
                    }
                };

                atmo_comp.derive_scattering_params(
                    surface_gravity_g,
                    atmo_data.scale_height_km,
                    atmo_data.rayleigh_rgb,
                    atmo_data.rayleigh_strength,
                    atmo_data.mie_strength,
                    atmo_data.mie_g,
                    atmo_data.haze_color,
                    atmo_data.atmosphere_intensity,
                );

                let atmo_mat = AtmosphereMaterial::from_composition(
                    visual_radius,
                    &atmo_comp,
                    initial_pos,
                    Vec3::ZERO, // Sun at origin
                    atmosphere_settings.quality,
                );

                let atmo_shell_radius = visual_radius * 1.05;
                commands
                    .entity(entity)
                    .insert(crate::plugins::atmosphere::HasAtmosphereShell)
                    .with_children(|parent| {
                        parent.spawn((
                            Mesh3d(meshes.add(Sphere::new(atmo_shell_radius).mesh().uv(64, 32))),
                            MeshMaterial3d(materials_atmosphere.add(atmo_mat)),
                            Transform::default(),
                            AtmosphereShell {
                                body_entity: entity,
                            },
                        ));
                    });
            }
        }

        // Initialize population
        // Earth starts with ~8.2 Billion people. Others empty.
        let population_count = if body_data.name == "Earth" {
            8_200_000_000.0
        } else {
            0.0
        };
        commands.entity(entity).insert(Population {
            count: population_count,
        });

        // Power generation now handled via Colony buildings
        // No separate PowerGenerator needed - Earth uses building-based power

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
                        commands.entity(*entity).insert(ChildOf(*parent_entity));
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

    // Third pass: Add lights and 3D volumetric corona/halo to stars
    for body_data in &data.bodies {
        if body_data.body_type == BodyType::Star {
            if let Some(entity) = entity_map.get(&body_data.name) {
                // Recalculate radius for visual effects
                let visual_radius = calculate_visual_radius(body_data.body_type, body_data.radius);

                // Derive corona colours from body emissive data
                let (er, eg, eb) = body_data.emissive;
                let core_col = Vec4::new(er * 5.0, eg * 5.0, eb * 5.0, 1.0);
                // Gentle warm shift — avoid extreme channel suppression that
                // causes visible colour banding on cool (M/K) stars.
                let halo_col = Vec4::new(er * 4.5, eg * 3.5, eb * 1.8, 1.0);

                // Shell radii
                let corona_shell_r = visual_radius * 1.75;
                let halo_shell_r = visual_radius * 4.0;

                // Spawn light and 3D corona shells as children of the star
                commands.entity(*entity).with_children(|parent| {
                    parent.spawn((
                        PointLight {
                            intensity: 2.8e11,
                            range: 2.0e9,
                            color: LinearRgba::new(er, eg, eb, 1.0).into(),
                            shadows_enabled: false,
                            ..default()
                        },
                        Transform::default(),
                    ));

                    // ── Inner volumetric corona shell ──────────────────────────
                    // Ray-marched 3D FBM plasma at 1.75× star radius.
                    parent.spawn((
                        Mesh3d(meshes.add(Sphere::new(corona_shell_r).mesh().ico(5).unwrap())),
                        MeshMaterial3d(materials_corona_3d.add(StarCorona3dMaterial {
                            color_core: Vec4::ZERO, // starts hidden; LOD system drives it
                            color_halo: Vec4::ZERO,
                            time_phase: 0.0,
                            corona_params: Vec4::new(visual_radius, corona_shell_r, 0.0, 0.0),
                        })),
                        Transform::default(),
                        StarCoronaShell {
                            base_core_color: core_col,
                            base_halo_color: halo_col,
                            visual_radius,
                        },
                    ));

                    // ── Outer diffuse halo shell ──────────────────────────────
                    // Limb-brightening glow at 3× star radius.
                    parent.spawn((
                        Mesh3d(meshes.add(Sphere::new(halo_shell_r).mesh().uv(32, 16))),
                        MeshMaterial3d(materials_halo_3d.add(StarHalo3dMaterial {
                            color_halo: Vec4::ZERO, // starts hidden; LOD system drives it
                            time_phase: 0.0,
                            halo_params: Vec4::new(visual_radius, halo_shell_r, 0.0, 0.0),
                        })),
                        Transform::default(),
                        StarHaloShell {
                            base_halo_color: halo_col,
                            visual_radius,
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
            // Orbit trail colors with higher alpha for bright trail heads
            let (orbit_color, should_show) = match body_data.body_type {
                BodyType::Star => {
                    // Orbiting stars in binary/trinary systems should always show
                    // the same fading partial-orbit treatment as planets.
                    (Color::srgba(1.0, 0.82, 0.5, 0.82), true)
                }
                BodyType::Planet | BodyType::GasGiant => {
                    // Planets & gas/ice giants: lighter blue
                    (Color::srgba(0.4, 0.75, 1.0, 0.85), true)
                }
                BodyType::DwarfPlanet => {
                    // Dwarf Planets: darker blue, hidden by default
                    (Color::srgba(0.25, 0.45, 0.75, 0.7), false)
                }
                BodyType::Moon => {
                    // Moons: subtle grey
                    (Color::srgba(0.65, 0.65, 0.65, 0.5), true)
                }
                BodyType::Asteroid => {
                    // Asteroids: dark green, steep fade so individual trails are short
                    // — prevents a thick opaque ring when many are visible at once.
                    (Color::srgba(0.3, 0.55, 0.22, 0.45), false)
                }
                BodyType::Comet => {
                    // Comets: yellow/amber
                    (Color::srgba(1.0, 0.8, 0.3, 0.65), false)
                }
                BodyType::Ring => (Color::srgba(0.0, 0.0, 0.0, 0.0), false),
            };

            // Asteroids get a steep fade to avoid thick ring buildup at high speed
            let fade_exponent = if body_data.body_type == BodyType::Asteroid {
                5.0
            } else {
                1.8
            };

            commands.entity(*entity).insert(OrbitPath {
                color: orbit_color,
                visible: should_show,
                segments: 128, // High segment count for smooth fading trails
                fade_exponent,
            });
        }
    }

    info!("Solar system setup complete!");
}

fn combine_ring_alpha_textures(
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut queue: ResMut<RingAlphaCombineQueue>,
) {
    fn to_rgba8_pixels(image: &Image) -> Option<(u32, u32, Vec<u8>)> {
        let width = image.texture_descriptor.size.width;
        let height = image.texture_descriptor.size.height;
        let data = image.data.as_ref()?;

        let rgba = match image.texture_descriptor.format {
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => data.clone(),
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
                if data.len() != (width as usize) * (height as usize) * 4 {
                    return None;
                }
                let mut out = Vec::with_capacity(data.len());
                for chunk in data.chunks_exact(4) {
                    out.push(chunk[2]);
                    out.push(chunk[1]);
                    out.push(chunk[0]);
                    out.push(chunk[3]);
                }
                out
            }
            _ => return None,
        };

        Some((width, height, rgba))
    }

    let mut pending = Vec::with_capacity(queue.entries.len());

    for entry in queue.entries.drain(..) {
        let prepared = {
            let color_image = images.get(&entry.color_handle);
            let alpha_image = images.get(&entry.alpha_handle);

            if let (Some(color_image), Some(alpha_image)) = (color_image, alpha_image) {
                let color = to_rgba8_pixels(color_image);
                let alpha = to_rgba8_pixels(alpha_image);
                Some((color, alpha))
            } else {
                None
            }
        };

        let Some((Some((color_w, color_h, color_bytes)), Some((alpha_w, alpha_h, alpha_bytes)))) =
            prepared
        else {
            pending.push(entry);
            continue;
        };

        let Some(color_rgba): Option<RgbaImage> =
            ImageBuffer::from_raw(color_w, color_h, color_bytes)
        else {
            continue;
        };
        let Some(alpha_rgba): Option<RgbaImage> =
            ImageBuffer::from_raw(alpha_w, alpha_h, alpha_bytes)
        else {
            continue;
        };

        let alpha_resized = if alpha_w == color_w && alpha_h == color_h {
            alpha_rgba
        } else {
            image::imageops::resize(&alpha_rgba, color_w, color_h, FilterType::Triangle)
        };

        let mut combined = Vec::with_capacity((color_w as usize) * (color_h as usize) * 4);
        for (color_px, alpha_px) in color_rgba.pixels().zip(alpha_resized.pixels()) {
            let [r, g, b, _] = color_px.0;
            let [ar, ag, ab, _] = alpha_px.0;
            let alpha = ((ar as u16 * 77 + ag as u16 * 150 + ab as u16 * 29) / 256) as u8;
            combined.extend_from_slice(&[r, g, b, alpha]);
        }

        let combined_image = Image::new(
            Extent3d {
                width: color_w,
                height: color_h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            combined,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );

        let combined_handle = images.add(combined_image);
        if let Some(material) = materials.get_mut(&entry.material_handle) {
            material.base_color_texture = Some(combined_handle);
        }
    }

    queue.entries = pending;
}

// System to convert any queued normal/specular images to linear format once they are loaded
fn apply_linear_to_images_system(
    mut images: ResMut<Assets<Image>>,
    mut queue: ResMut<LinearImageQueue>,
) {
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
    time_scale: Res<crate::ui::TimeScale>,
    real_time: Res<Time<Real>>,
    // Stars are excluded: their granulation texture spinning at high game speed
    // creates unnatural strobing / sparkle artefacts. Star orientation has no
    // gameplay significance (unlike planetary day/night cycles).
    mut query: Query<(&mut Transform, &RotationSpeed, Option<&AxialTilt>), Without<Star>>,
) {
    /// Base visual rotation speed in rad/real-second.
    /// Matches the orbital cap (2π ≈ 1 revolution per real second).
    /// Above this, speed is logarithmically compressed.
    const VISUAL_SPEED_BASE: f32 = std::f32::consts::TAU;

    let sim_t = sim_time.elapsed_seconds() as f32;
    let real_t = real_time.elapsed_secs();
    let scale = time_scale.scale;

    for (mut transform, rotation_speed, axial_tilt) in query.iter_mut() {
        // Effective angular speed in rad/real-second
        let effective_speed = rotation_speed.0.abs() * scale;

        let angle = if effective_speed > VISUAL_SPEED_BASE {
            // Logarithmic cap: faster at higher speeds, never strobes
            let vis_speed = VISUAL_SPEED_BASE * (1.0 + (effective_speed / VISUAL_SPEED_BASE).ln());
            let capped = vis_speed * rotation_speed.0.signum();
            capped * real_t
        } else {
            // Normal: use analytical sim-time rotation
            rotation_speed.0 * sim_t
        };

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
        if let Ok(mut anchor) = query_camera.single_mut() {
            if anchor.0.is_none() {
                info!("Setting initial camera focus to Sol");
                anchor.0 = Some(sol);
            }
        }
    }
}

// Helper to create a flat ring (annulus) mesh
pub(crate) fn create_ring_mesh(outer_radius: f32, inner_radius: f32, segments: u32) -> Mesh {
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
    let mut mesh = Sphere::new(visual_radius).mesh().uv(64, 32);

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
                    rng.random::<f32>() * 2.0 - 1.0,
                    rng.random::<f32>() * 2.0 - 1.0,
                    rng.random::<f32>() * 2.0 - 1.0,
                )
                .normalize_or_zero(),
            );
            phases.push(rng.random::<f32>() * std::f32::consts::TAU);
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

/// PostStartup system that attaches a `LocalStockpile` and `MinimumStockpile`
/// to every colony entity.
///
/// Earth gets the realistic 2026 starting values from `GlobalBudget::new()`.
/// Other colonies start with a small bootstrap stockpile so construction is
/// immediately possible without requiring freighter deliveries.
///
/// All colonies also receive a default `MinimumStockpile` with conservative
/// thresholds for critical supplies (food, water, oxygen) so that freighters
/// keep them stocked without requiring manual configuration.
///
/// This runs in `PostStartup` so all colony entities from `setup_solar_system`
/// already exist.
pub fn initialize_colony_stockpiles(
    mut commands: Commands,
    colony_query: Query<(Entity, &Colony), Without<LocalStockpile>>,
) {
    use crate::economy::logistics::MinimumStockpile;
    use crate::economy::types::ResourceType;

    let defaults = GlobalBudget::new();

    for (entity, colony) in colony_query.iter() {
        let stockpile = if colony.name == "Earth" {
            // Earth starts with the full realistic 2026 stockpile
            LocalStockpile::with_stockpiles(defaults.stockpiles.iter().map(|(k, v)| (*k, *v)))
        } else {
            // Other colonies start with a small bootstrap supply to allow
            // initial construction without requiring freighter transport.
            // (All values in Mt — enough for a few basic buildings.)
            LocalStockpile::with_stockpiles([
                (ResourceType::Iron, 10.0),
                (ResourceType::Silicates, 50.0),
                (ResourceType::Aluminum, 2.0),
                (ResourceType::Copper, 0.5),
                (ResourceType::Polymers, 1.0),
                (ResourceType::Food, 10_000.0),
                (ResourceType::Water, 5.0),
            ])
        };

        // Default minimum stockpile thresholds — conservative values for
        // critical life-support resources so freighters keep the colony topped up.
        let mut minimum = MinimumStockpile::default();
        if colony.name != "Earth" {
            // Outposts need steady resupply of core consumables.
            // Defaults match the GRA-31 life-support scale: Water=100 (O₂ parity)
            // and Food=500 (~5× O₂ default, comfortably below starting stockpile
            // so the auto-freight loop does not fire on day 1).
            minimum.set(ResourceType::Food, 500.0);
            minimum.set(ResourceType::Water, 100.0);
        }

        commands.entity(entity).insert(stockpile);
        commands.entity(entity).insert(minimum);
    }
}
