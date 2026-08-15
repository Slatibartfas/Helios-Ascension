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
    OceanType, OrbitPath, SpaceCoordinates, StellarProperties, SurfaceTemperature, SystemId,
    SCALING_FACTOR,
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
            // `LinearImageQueue` is also populated (with the handles
            // for the just-loaded normal/specular textures) by
            // `setup_solar_system` at runtime — but `setup_solar_system`
            // now runs deferred in `Update` (via `BootInitPlugin`),
            // while `apply_linear_to_images_system` (registered below)
            // runs in `Update` *unconditionally*. Without this
            // upfront init, the linear-conversion system would panic
            // on the first frame because the queue resource hasn't
            // been inserted yet. The default-initialised queue is
            // empty (no handles), which is the correct pre-setup
            // state — `setup_solar_system` later overwrites it with
            // the populated handles via `commands.insert_resource`.
            .init_resource::<LinearImageQueue>()
            // Note: `setup_solar_system`, `initial_camera_focus`, and
            // `initialize_colony_stockpiles` were previously registered
            // at `Startup` / `PostStartup` here. They are now owned by
            // `crate::boot_init::BootInitPlugin` so the splash can
            // hide the work. See `src/boot_init.rs`.
            // The `SolarSystemSpawned` idempotency marker is NOT
            // pre-initialised — `setup_solar_system` adds it itself
            // after a successful spawn so a save-restore that re-runs
            // `boot_init` (i.e. flips `BootState` back to `Loading`)
            // short-circuits instead of duplicating every body.
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

#[derive(Component, Reflect)]
#[reflect(Component)]
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
    /// Per-body override for the star-approach parking radius (AU).
    /// `None` means "use the global default" (0.3 AU for main-sequence stars; the
    /// planner will down-scale for sub-solar bodies internally if needed).
    /// Only meaningful for `BodyType::Star`; ignored otherwise.
    /// Rationale (GRA-149 C-2): the picker label "Sol Approach (0.3 AU)" used to
    /// lie about the arrival radius — the planner arrived at the planet's SOI
    /// boundary, not 0.3 AU.  This field pins the actual parking radius so the
    /// label can match the math.
    pub star_approach_au: Option<f64>,
    /// Rotation period in seconds, computed once at spawn from
    /// `CelestialBodyData::rotation_period` (already `.abs()`'d, so retrograde
    /// rotators report a positive value).  `None` for bodies with no
    /// measurable rotation (asteroids, comets, rings).
    /// Used by `radius_for_shell` to compute the synchronous-orbit shell
    /// (`r_sync = (GM·T_rot²/4π²)^(1/3)`) without widening the standard 5-tuple
    /// body query that every transfer-planner helper uses.
    pub rotation_period_s: Option<f64>,
    /// Outer edge of the star's habitable zone (AU), precomputed at spawn as
    /// `sqrt(L_star / L_sol) × 1.0 AU` when `StellarProperties` is present.
    /// `None` for non-stellar bodies, or for stars whose `StellarProperties`
    /// is unavailable at spawn time (rare; the loader uses `sol()` defaults so
    /// the field is normally populated for stars).  Used by the
    /// `HabitableOuter` orbit shell on the transfer-planner picker.
    pub habitable_outer_au: Option<f64>,
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
#[derive(Component, Copy, Clone, Reflect)]
#[reflect(Component)]
pub struct LogicalParent(pub Entity);

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Star;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Planet;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct DwarfPlanet;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Moon;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Asteroid;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Comet;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GasGiant;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Ring;

/// Marker component for entities that cannot be clicked/ray-picked in the 3-D view.
/// They remain selectable via the ledger panel.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct ClickExcluded;

/// Axial tilt (obliquity) and north-pole direction of a celestial body.
/// `obliquity` is the angle between the spin axis and the ecliptic normal (radians).
/// `north_pole_ra` is the right-ascension direction the north pole tilts toward (radians).
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct AxialTilt {
    pub obliquity: f32,
    pub north_pole_ra: f32,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
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
            // All non-Vesta asteroids share one neutral rock texture
            // (`generic_c_type_2k.jpg`). The C-type map carries the right
            // craters-and-regolith silhouette at a neutral grey base; the
            // per-class color profile in `asteroid_class_profile` does the
            // hue differentiation on top.
            //
            // History: an earlier draft routed S/M/V types through
            // `generic_s_type_2k.jpg`, but that map is dominated by warm
            // pink-brown splotches that read as a Mars-like surface and
            // crushed the dark side into a pitch-black silhouette under
            // any back-light. Funnelling every class through the same
            // neutral map keeps the rock readable as rock and lets the
            // class multiplier push the hue into the right zone without
            // fighting the underlying texture.
            //
            // Vesta is the only asteroid with a dedicated texture
            // (`vesta_4k.png`); that path bypasses this function via the
            // dedicated-texture branch above.
            Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
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

pub(crate) fn asteroid_class_profile(class: AsteroidClass) -> (Vec3, f32, f32) {
    // Approximate geometric albedo/tint ranges informed by spacecraft and
    // telescopic observations: Bennu/Ryugu are very dark C-types, Eros is a
    // neutral-to-warm S-type, Vesta is basaltic with high albedo contrast, and
    // Psyche is metal/silicate rather than a mirror-like pure metal surface.
    //
    // Every class shares the neutral grey `generic_c_type_2k.jpg` rock
    // texture, so the RGB values here represent the hue multiplier applied
    // on top of that neutral base. C-type lands near the texture's natural
    // charcoal grey (no shift). S-type pushes warm-grey. M-type pushes cool
    // steel grey. V-type pushes basaltic brown. D/P types darken further.
    //
    // The values are clamped in `asteroid_material_variation` to
    // [0.06, 0.72] per channel after a small per-body offset is applied;
    // the S-type hue here is therefore the centre of an effective range
    // rather than an absolute target.
    match class {
        // Roughness values are deliberately near the top of the [0, 1]
        // range: rock is matte, never satin-smooth. The shared
        // `generic_rock_roughness_2k.png` carries values in [0.95, 1.00]
        // and Bevy multiplies it with this scalar, so the effective
        // roughness still lands in [0.88, 0.95] even after the
        // micro-variation. Combined with the rock normal map, the
        // surface reads as a clearly matte rock instead of plastic.
        AsteroidClass::CType => (Vec3::new(0.55, 0.55, 0.54), 0.95, 0.02),
        AsteroidClass::SType => (Vec3::new(0.78, 0.75, 0.69), 0.92, 0.05),
        AsteroidClass::MType => (Vec3::new(0.62, 0.63, 0.64), 0.78, 0.28),
        AsteroidClass::VType => (Vec3::new(0.58, 0.54, 0.48), 0.94, 0.04),
        AsteroidClass::DType => (Vec3::new(0.45, 0.42, 0.40), 0.97, 0.01),
        AsteroidClass::PType => (Vec3::new(0.48, 0.46, 0.44), 0.96, 0.01),
        AsteroidClass::Unknown => (Vec3::new(0.58, 0.56, 0.54), 0.94, 0.03),
    }
}

/// Returns a restrained, class-based asteroid albedo and PBR response.
///
/// Real asteroid imagery is dominated by charcoal greys, neutral stone,
/// muted browns, and occasional basaltic dark patches; saturated red is not a
/// useful default. The name seed only supplies small body-to-body variation so
/// results remain deterministic across save/load and runs.
fn asteroid_material_variation(name: &str, class: AsteroidClass) -> (Color, f32, f32) {
    let seed = calculate_hash(&name);
    let jitter = ((seed % 1000) as f32 / 1000.0 - 0.5) * 0.12;
    let (rgb, roughness, metallic) = asteroid_class_profile(class);
    let rgb = (rgb + Vec3::splat(jitter)).clamp(Vec3::splat(0.06), Vec3::splat(0.72));
    (Color::srgb(rgb.x, rgb.y, rgb.z), roughness, metallic)
}

/// Per-body albedo jitter, deterministic from the body's name seed. Adds
/// small warm/cool shifts so two S-type asteroids don't read identically
/// even though they share the same class tint. Result is applied as a
/// multiplicative tint on top of the class profile, so the rough albedo
/// range is preserved (Bennu stays dark, Eros stays warm-grey, Vesta stays
/// basaltic).
pub(crate) fn asteroid_albedo_jitter(name: &str) -> Color {
    let seed = calculate_hash(&name);
    let r = ((seed % 1000) as f32) / 1000.0;
    let g = (((seed / 1000) % 1000) as f32) / 1000.0;
    let b = (((seed / 1_000_000) % 1000) as f32) / 1000.0;
    // Multiplicative shift in [0.88, 1.12] per channel.
    let jr = 0.88 + r * 0.24;
    let jg = 0.88 + g * 0.24;
    let jb = 0.88 + b * 0.24;
    Color::srgb(jr, jg, jb)
}

// ── Normal-map pool (per-body relief variety) ──────────────────────────────

/// Pool of 4 distinct tangent-space rock normal-map variants for
/// asteroids. The selector picks one per body via `hash(name) % N` so
/// each asteroid gets a different bump character across the catalog.
/// All four are more cratery than the original
/// `generic_rock_normal_2k.png` (the fallback), and each one
/// emphasises a different surface regime:
///
/// * **a** — heavily cratered, Bennu/Ryugu character
/// * **b** — sparse large craters with central peaks, Mathilde/Eros
/// * **c** — fractured rock faces, Itokawa character
/// * **d** — rolling regolith, Ceres/Vesta character
///
/// Adding a new variant is a one-line change here plus a new PNG on
/// disk; the selector falls back to the legacy map if the pool is
/// emptied by a mod, so the project still loads cleanly.
const ROCK_NORMAL_VARIANTS: &[&str] = &[
    "textures/celestial/asteroids/generic_rock_normal_a_2k.png",
    "textures/celestial/asteroids/generic_rock_normal_b_2k.png",
    "textures/celestial/asteroids/generic_rock_normal_c_2k.png",
    "textures/celestial/asteroids/generic_rock_normal_d_2k.png",
];

/// Fallback when the pool is empty (defensive — the pool is a
/// `const` slice, so this only fires if a future maintainer empties
/// it). The fallback is also what comets continue to use: comets
/// keep the original sparse map because icy nucleus relief reads
/// better against the lower-frequency legacy map.
const ROCK_NORMAL_FALLBACK: &str = "textures/celestial/asteroids/generic_rock_normal_2k.png";

/// Denser roughness map for asteroids. Same band as the legacy
/// `generic_rock_roughness_2k.png` but with an extra procedural
/// fine-grain layer added on top of the EXR-derived roughness, so
/// the surface reads as more gritty at close zoom. Comets stay on
/// the legacy map — the dense rock micro-variation doesn't read
/// correctly on an icy body.
const ROCK_ROUGHNESS_DENSE: &str =
    "textures/celestial/asteroids/generic_rock_roughness_dense_2k.png";

/// Pick a rock normal-map path for an asteroid body. Deterministic
/// from the body's name so save/load and new-game spawns land on
/// the same relief, matching the per-body colour-jitter contract.
///
/// Returns the fallback if [`ROCK_NORMAL_VARIANTS`] is empty.
pub(crate) fn pick_asteroid_rock_normal_path(name: &str) -> &'static str {
    if ROCK_NORMAL_VARIANTS.is_empty() {
        return ROCK_NORMAL_FALLBACK;
    }
    let idx = (calculate_hash(&name) as usize) % ROCK_NORMAL_VARIANTS.len();
    ROCK_NORMAL_VARIANTS[idx]
}

/// Generate procedural variation for material based on body properties
/// Enhanced to visually distinguish all 6 asteroid spectral classes
fn apply_procedural_variation(
    body_data: &super::solar_system_data::CelestialBodyData,
    base_color: Color,
    has_texture: bool,
) -> (Color, f32, f32) {
    if body_data.body_type == BodyType::Asteroid {
        let class = body_data.asteroid_class.unwrap_or(AsteroidClass::CType);
        let (class_color, class_roughness, class_metallic) =
            asteroid_material_variation(&body_data.name, class);
        let base = base_color.to_srgba();
        let class_rgb = class_color.to_srgba();
        // Deterministic per-body jitter so two S-type asteroids don't
        // share an identical tint. The jitter is multiplicative in
        // [0.88, 1.12] per channel, well within the class profile's
        // safe range.
        let jitter = asteroid_albedo_jitter(&body_data.name).to_srgba();
        let color = if has_texture {
            // The texture is the dominant signal. The class color tints
            // the texture into the right class-specific hue; we don't
            // blend in `base_color` (the body's RON tint) because
            // adding it was crushing the linear-space product below the
            // visible range. The texture carries its own albedo, so
            // all we want from the class color is a class-specific
            // multiplier on top.
            Color::srgb(
                (class_rgb.red * jitter.red).clamp(0.0, 1.0),
                (class_rgb.green * jitter.green).clamp(0.0, 1.0),
                (class_rgb.blue * jitter.blue).clamp(0.0, 1.0),
            )
        } else {
            // No texture -- use the class profile as the entire albedo
            // and blend a small fraction of the body's tint to keep
            // minor per-body variation.
            Color::srgb(
                (class_rgb.red * jitter.red * 0.85 + base.red * 0.15).clamp(0.0, 1.0),
                (class_rgb.green * jitter.green * 0.85 + base.green * 0.15).clamp(0.0, 1.0),
                (class_rgb.blue * jitter.blue * 0.85 + base.blue * 0.15).clamp(0.0, 1.0),
            )
        };
        return (color, class_roughness, class_metallic);
    }

    // Use body name as seed for consistent randomness
    let mut seed = 0u32;
    for byte in body_data.name.bytes() {
        seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
    }

    // Generate pseudo-random values from seed
    let random1 = ((seed % 1000) as f32) / 1000.0;
    let random2 = (((seed / 1000) % 1000) as f32) / 1000.0;
    let random3 = (((seed / 1000000) % 1000) as f32) / 1000.0;

    // Vary color based on body type. Asteroids use the measured class profile
    // above; this branch handles comets, moons, dwarf planets and rings.
    let color_variation = match body_data.body_type {
        BodyType::Comet => {
            let brightness = 0.25 + random2 * 0.35;
            Color::srgb(brightness * 1.05, brightness, brightness * 0.92)
        }
        BodyType::Moon => {
            let gray_variation = 0.9 + random1 * 0.2;
            let base = base_color.to_srgba();
            Color::srgb(
                base.red * gray_variation,
                base.green * gray_variation,
                base.blue * gray_variation,
            )
        }
        BodyType::DwarfPlanet => {
            let brightness = 0.42 + random2 * 0.35;
            let tint = (random3 - 0.5) * 0.12;
            Color::srgb(
                (brightness + tint).clamp(0.0, 1.0),
                brightness.clamp(0.0, 1.0),
                (brightness - tint).clamp(0.0, 1.0),
            )
        }
        BodyType::Ring => base_color,
        _ => base_color,
    };

    let roughness_var = if has_texture {
        0.75 + random2 * 0.2
    } else {
        0.65 + random2 * 0.25
    };
    let metallic_var = match body_data.body_type {
        BodyType::Comet => 0.02 + random3 * 0.04,
        BodyType::DwarfPlanet => 0.05 + random3 * 0.1,
        _ => 0.1 + random3 * 0.1,
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

/// Idempotency marker so `setup_solar_system` runs exactly once per
/// save. Without this, a save-restore that resets `BootState` back
/// to `Loading` (or any future "re-run boot-init" path) would
/// duplicate every celestial body in the active system. Strip this
/// resource in the same reset path as
/// [`crate::fleets::DayOneFleetSpawned`].
///
/// Lives next to the spawn function so the
/// `marker ↔ spawn function` pairing is obvious. Not registered
/// into `AppTypeRegistry` — there is no save-time value, only a
/// "have we spawned yet?" gate.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct SolarSystemSpawned;

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
    solar_system_marker: Option<Res<SolarSystemSpawned>>,
    // Tier 3: if `boot_init::start_pre_parse` already drained
    // the RON parse onto the async pool, the parsed value lives
    // here and we skip the synchronous file read + RON decode.
    // The chain's step 0 lands here on a typical New Game click
    // (5+ s after splash dismiss), so the 150 ms parse is hidden
    // behind the player's menu time.
    boot_pre_parse: Res<crate::boot_init::BootPreParseState>,
) {
    if solar_system_marker.is_some() {
        // Idempotency: do not re-spawn. The marker must be removed
        // by a future "new game" / "fresh save load" path before
        // this branch is taken. Matches the
        // `DayOneFleetSpawned` / `AsteroidRegistryLoaded` pattern.
        return;
    }
    // Queue to collect normal/specular handles that must be treated as linear textures
    let mut linear_handle_queue: Vec<Handle<Image>> = Vec::new();

    // Load solar system data — Tier 3 fast path first, sync parse
    // as fallback. The fast path clones the pre-parsed value
    // (SolarSystemData is `Clone`), the fallback path is the
    // original synchronous file read + RON decode.
    let mut data = if let Some(cached) = boot_pre_parse.solar_data.as_ref() {
        info!(
            "setup_solar_system: using Tier 3 pre-parsed data ({} bodies cached)",
            cached.bodies.len()
        );
        cached.clone()
    } else {
        match SolarSystemData::load_from_file("assets/data/solar_system.ron") {
            Ok(data) => {
                info!(
                    "setup_solar_system: pre-parse not ready, fell back to sync parse ({} bodies)",
                    data.bodies.len()
                );
                data
            }
            Err(e) => {
                error!("Failed to load solar system data: {}", e);
                // Mark spawned even on load failure to prevent
                // boot-init from retrying every tick and spamming the
                // log. Matches the `AsteroidRegistryLoaded` failure
                // pattern.
                commands.init_resource::<SolarSystemSpawned>();
                return;
            }
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
            // Single dedicated texture. Asteroids pick one variant out of
            // [`ROCK_NORMAL_VARIANTS`] via `hash(name) % N`; comets and
            // other bodies fall through to the legacy shared map. The
            // dedicated-texture path (s-type, vesta, etc.) keeps the same
            // per-class material treatment as the generic path — only the
            // bump character varies per body. The normal map handle is
            // queued for linear conversion (it's data, not albedo) and
            // applied to the StandardMaterial below.
            let normal_path = match body_data.body_type {
                BodyType::Asteroid => Some(pick_asteroid_rock_normal_path(&body_data.name)),
                BodyType::Comet => Some(ROCK_NORMAL_FALLBACK),
                _ => None,
            };
            let normal_tex = normal_path.map(|path| asset_server.load::<Image>(path));
            if let Some(ref handle) = normal_tex {
                linear_handle_queue.push(handle.clone());
            }
            (
                Some(asset_server.load(texture.clone())),
                normal_tex,
                None,
                None,
                None,
                true,
            )
        } else {
            // Generic asteroid maps are deliberately shared by spectral class,
            // so select a deterministic normal map too. StandardMaterial uses
            // tangent-space normals; the generated relief map adds craters and
            // regolith breakup without changing the silhouette. Asteroids
            // pick one of the 4 variants via the name hash; comets fall
            // through to the legacy shared map (see [`pick_asteroid_rock_normal_path`]).
            let generic_path = get_generic_texture_path(body_data);
            let normal_path = match body_data.body_type {
                BodyType::Asteroid => Some(pick_asteroid_rock_normal_path(&body_data.name)),
                BodyType::Comet => Some(ROCK_NORMAL_FALLBACK),
                _ => None,
            };
            let normal_tex = normal_path.map(|path| asset_server.load::<Image>(path));
            if let Some(ref handle) = normal_tex {
                linear_handle_queue.push(handle.clone());
            }
            (
                generic_path.map(|path| asset_server.load(path)),
                normal_tex,
                None,
                None,
                None,
                false,
            )
        };

        let has_texture = base_color_texture.is_some();

        // Apply procedural variation to material properties. For asteroids
        // and comets we always want the class-derived roughness / metallic /
        // tint, regardless of whether the body has a dedicated texture,
        // otherwise the legacy "white tint + 0.7 roughness" branch kept
        // every S-type asteroid looking like a smooth red rock.
        let base_color = Color::srgb(body_data.color.0, body_data.color.1, body_data.color.2);
        let is_asteroid_or_comet =
            matches!(body_data.body_type, BodyType::Asteroid | BodyType::Comet);
        let (material_color, roughness, metallic) = if is_asteroid_or_comet {
            apply_procedural_variation(body_data, base_color, has_texture)
        } else if has_dedicated_texture {
            // For textured non-asteroid bodies, use a slight white tint to
            // enhance the texture without re-tinting it.
            (Color::srgb(1.0, 1.0, 1.0), 0.7, 0.0)
        } else {
            apply_procedural_variation(body_data, base_color, has_texture)
        };

        // Optional metallic_roughness map for asteroids.  The reference
        // rock ships a roughness EXR; the bake script also produces a PNG
        // sibling so the runtime does not depend on the OpenEXR loader.
        // Asteroids use the dense variant (more micro-grit at close
        // zoom); comets stay on the legacy map because icy nucleus
        // surfaces read better with the less-detailed roughness.
        // Load it once and tag the handle for linear conversion.
        let metallic_roughness_texture = if is_asteroid_or_comet {
            let roughness_path = match body_data.body_type {
                BodyType::Asteroid => ROCK_ROUGHNESS_DENSE,
                _ => "textures/celestial/asteroids/generic_rock_roughness_2k.png",
            };
            let handle = asset_server.load::<Image>(roughness_path);
            linear_handle_queue.push(handle.clone());
            Some(handle)
        } else {
            None
        };

        // Note: Bevy 0.18 `StandardMaterial` does not expose a public
        // `normal_map_strength` field — that knob lives on a deferred path
        // in the PBR shader and was only added in 0.19. We don't try to
        // set it; the rock reference map (`generic_rock_normal_2k.png` /
        // the per-body pool at `ROCK_NORMAL_VARIANTS`) is dense enough
        // that the bump reads at in-game zoom distances with the default
        // strength. The new pool increases the gradient gain at bake
        // time (see `scripts/generate_rock_normal_variants.py`) to
        // compensate for the missing public knob.

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
                // Note: Bevy 0.18 does not expose a public
                // `normal_map_strength` field. We rely on the dense
                // rock reference map for visible relief.
                metallic_roughness_texture,
                // Minimal emissive floor so planets in dim/distant star systems
                // aren't pitch black on the night side.  Intentionally very low
                // so day/night contrast is still strong.
                //
                // Asteroids use a much higher floor (0.045) than the
                // generic 0.006 because:
                //   * Bevy's PBR doesn't model surface-to-surface
                //     interreflection, so a back-lit asteroid has nothing
                //     lighting its dark side. The old 0.015 floor was too
                //     dim -- Hathor/Itokawa-sized bodies rendered as nearly
                //     black silhouettes against the deep-space background.
                //   * 0.045 in linear units reads as ~0.24 in sRGB: a clearly
                //     visible charcoal that lets the rock texture read on
                //     every hemisphere while leaving day/night contrast
                //     strong (lit side still dominated by direct sun).
                //   * The bump is per-asteroid-only, so planets/moons keep
                //     their much darker 0.006 floor and don't lose their
                //     dramatic day/night terminator.
                emissive: LinearRgba::WHITE
                    * if body_data.body_type == BodyType::Asteroid {
                        0.045
                    } else {
                        0.006
                    },
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
                    star_approach_au: body_data.star_approach_au,
                    // GRA-NNN: spawn-time caches for the orbit-shell resolver.
                    // Stars always receive `StellarProperties::sol()` below (L = 1),
                    // so `sqrt(L) * 1.0 AU = 1.0 AU` for every star loaded via RON.
                    rotation_period_s: body_data.rotation_period_seconds(),
                    habitable_outer_au: Some(1.0),
                },
                // GRA-358 PR-J: SystemId(0) tags every Sol-system
                // body spawned by `setup_solar_system` so the
                // persistence apply path's `build_body_index`
                // (which filters on `&SystemId`) can find them
                // when overlaying saved divergences on Restore.
                SystemId(0usize),
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
                    star_approach_au: body_data.star_approach_au,
                    // GRA-NNN: spawn-time caches for the orbit-shell resolver.
                    rotation_period_s: body_data.rotation_period_seconds(),
                    habitable_outer_au: None,
                },
                SystemId(0usize),
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
            //
            // v0.5.1 canary-1: food per-build downscaled (Farm 1,000→360,
            // Greenhouse 500→200, Aquaculture 750→200 — these are the
            // hard-coded values in `Colony::food_production_per_year`, the
            // simulation does NOT read the RON `FoodProduction` modifier)
            // and per-capita demand corrected (0.0001 → 0.0000011, the
            // 1,000× off — Mt-vs-kg unit confusion). v0.5.1 hit 9,000 Mt/yr
            // supply ≈ 9,020 Mt/yr demand.
            //
            // v3.5: this calibration now reads the RON `FoodProduction`
            // modifier (see `Colony::food_production_per_year`).
            //
            // v3.7: dropped supplemental Greenhouses (10→1) and
            // Aquaculture (10→1). Earth now starts at 25×360 + 1×200
            // + 1×200 = 9,400 Mt/yr = 1.042× world demand (vs v3.5's
            // 1.44× surplus). 1.042× gives ~5 years of headroom at the
            // v3.7 base growth rate (0.9%/yr, FAO 2024), so the
            // player feels food pressure mid-game rather than in 50
            // years. Player must build more food infrastructure as
            // population grows.
            // 1,100 kg/p/yr = 1.1 × 10⁻⁶ Mt/p/yr unit conversion that v0.5
            // canary-1 had wrong by 1,000×).
            //
            // v3.7: starting counts calibrated for 1.042× world food demand.
            // 25 Farms × 360 = 9,000 Mt/yr (parity). 1 Greenhouse +
            // 1 Aquaculture = 400 Mt/yr (4.2% surplus). Total 9,400 Mt/yr.
            // 25 lands in the middle of the 10–50 manageable-count band.
            // (v3.5 had 10 Greenhouses + 10 Aquaculture = 4,000 Mt/yr
            //  supplemental buffer = 1.44× surplus; dropped to 1.042× in
            // v3.7 so food pressure arrives mid-game, not after 50 years.)
            //
            // Other building counts (Mine, Refinery, etc.) are unchanged
            // in canary 1; they will be revised in canary 2 / roll-forward
            // when their per-build values land.
            //
            // Reference: docs/design/BALANCE_PATCHES_v0.5.md §8.8 canary 1.
            let base_buildings = [
                // Housing: scaled for population capacity
                (BuildingType::Housing, 400),
                // Food (v3.7 calibrated for 1.042× world demand):
                // 25 Farms × 360 Mt/yr = 9,000 Mt/yr ≈ 8.2B × 1,100 kg/p/yr.
                (BuildingType::Farm, 25),
                // Greenhouses: 1 of 10 (v3.7 trimmed from 10 to 1) — small
                // specialty-crop buffer (200 Mt/yr = 2.2% of demand).
                (BuildingType::Greenhouse, 1),
                // Aquaculture: 1 of 10 (v3.7 trimmed from 10 to 1) — seafood
                // specialty (200 Mt/yr = 2.2% of demand).
                (BuildingType::AquacultureFacility, 1),
                // v0.5.2: per-resource dedicated mines — 25 of each
                // (manageable-count band, calibrated so 25 × base_yield ×
                // 0.6 Earth accessibility ≈ world demand). See
                // BALANCE_PATCHES_v0.5.md §5.2–§5.20 for per-resource
                // yields.
                (BuildingType::Factory, 1_200),
                // Construction (9)
                (BuildingType::IronMine, 25),
                (BuildingType::AluminumMine, 25),
                (BuildingType::TitaniumMine, 25),
                (BuildingType::SilicatesMine, 25),
                (BuildingType::NickelMine, 25),
                (BuildingType::TungstenMine, 25),
                (BuildingType::CarbonMine, 25),
                (BuildingType::ChromiumMine, 25),
                (BuildingType::MagnesiumMine, 25),
                // Precious (3 — v0.5.1)
                (BuildingType::GoldMine, 25),
                (BuildingType::SilverMine, 25),
                (BuildingType::PlatinumMine, 25),
                // Strategic (6)
                (BuildingType::CopperMine, 25),
                (BuildingType::RareEarthsMine, 25),
                (BuildingType::LithiumMine, 25),
                (BuildingType::SulfurMine, 25),
                (BuildingType::PhosphorusMine, 25),
                (BuildingType::CobaltMine, 25),
                (BuildingType::FluorineMine, 25),
                // Fissile (2)
                (BuildingType::UraniumMine, 25),
                (BuildingType::ThoriumMine, 25),
                // Hydrocarbons (1)
                (BuildingType::MethaneExtractor, 25),
                // Heavy water (1)
                (BuildingType::DeuteriumExtractor, 25),
                // Generic industry
                (BuildingType::ChemicalPlant, 700),
                (BuildingType::AtmosphericProcessor, 300),
                // Power: v3.4 IEA 2026 calibration. Total 3.40 TW supply, 3.31 TW demand,
                // ratio 0.974 (97.4% utilization, 2.7% reserve). Per-build values
                // (buildings.ron) sized so 320 solar / 195 coal / 135 gas / 82 hydro /
                // 400 wind / 20 fission reproduce IEA 2026 generation mix within 1-2pp.
                // Effective mix: Coal 31.9%, Gas 23.4%, Hydro 14.9%, Nuclear 9.6%,
                // Wind 10.6%, Solar 9.6% (IEA 2026 targets: 30/22/14/9/10/9).
                (BuildingType::SolarPower, 320), // 320 × 1.02 = 326 GW
                (BuildingType::CoalPowerPlant, 195), // 195 × 5.56 = 1,084 GW
                (BuildingType::NaturalGasPlant, 135), // 135 × 5.89 = 795 GW
                (BuildingType::HydroelectricDam, 82), // 82 × 6.18 = 507 GW
                (BuildingType::WindFarm, 400),   // 400 × 0.90 = 360 GW
                (BuildingType::FissionReactor, 20), // 20 × 16.28 = 326 GW
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
                    // GRA-358 PR-J follow-up: rings previously received
                    // `ChildOf(planet)` here as well as `LogicalParent(planet)`.
                    // The former populated the planet's `Children` collection,
                    // which Bevy 0.18's recursive `propagate_parent_transforms`
                    // walked indefinitely on the post-save/load world (the
                    // `despawn_helios_simulation_entities` step leaves stale
                    // `Children` entries on the live App's prior-session
                    // parent entities; STATUS_STACK_OVERFLOW on Windows,
                    // SIGSEGV on Linux). Rings now use `LogicalParent` only;
                    // `update_render_transform` resolves the parent via
                    // `LogicalParent` (not `ChildOf`), so the ring's visual
                    // position is unaffected.
                    commands
                        .entity(*entity)
                        .insert(LogicalParent(*parent_entity));
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

            // Insert amplification component for moons.
            //
            // **Rings** (Saturn Rings, Uranus Rings) also need a
            // `LocalOrbitAmplification` so `update_render_transform`
            // resolves their world position via the host planet's
            // `SpaceCoordinates` rather than reporting at the world
            // origin. The regen chain only runs the
            // `if let Some(orbit)` block for non-rings (rings have
            // `orbit: None` in `solar_system.ron`), so the ring's
            // `SpaceCoordinates` is never initialised and stays at
            // `DVec3::ZERO`. Without `LocalOrbitAmplification`,
            // `update_render_transform` falls into the "non-moon
            // body" branch that uses `coords.position × SCALING_FACTOR`
            // = `(0,0,0)` — the ring renders at Sol's location.
            // The amplification branch (taken when this component is
            // `Some`) instead resolves `parent_world = host planet's
            // SpaceCoordinates × SCALING_FACTOR` and writes
            // `transform.translation = parent_world + 0 × SCALING_FACTOR × 1.0`
            // = parent_world. The amp value `1.0` keeps the ring's
            // own offset (zero) at zero scale; the ring tracks the
            // planet through the orbit but does not add its own
            // offset.
            //
            // GRA-358 PR-K — this matches the contract the
            // `populate_restored_bodies_3d` decorator applies to
            // restore-path rings; the fix is now harmonised across
            // both spawn paths.
            //
            // (Ring bodies are handled in the dedicated post-pass
            // loop at the end of this function — they have
            // `orbit: None` so this `if let Some(orbit)` block
            // skips them entirely.)
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
                    // Asteroids: dim brown — matches the rocky/siliceous
                    // aesthetic so the orbit line reads as an asteroid trail
                    // and not a planetary ring.  Steep fade keeps individual
                    // trails short so dense belts don't pile up into thick
                    // opaque loops.
                    (Color::srgba(0.42, 0.32, 0.20, 0.35), false)
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

    // GRA-358 PR-K: ring bodies have `orbit: None` in
    // `solar_system.ron`, so the second pass above skipped them.
    // They still need a `SpaceCoordinates` placeholder so the
    // `update_render_transform` query selects them (the query
    // requires `&SpaceCoordinates`) and a `LocalOrbitAmplification`
    // so the rendering path picks the "amplification" branch
    // that resolves their world position via the host planet's
    // `SpaceCoordinates` via `LogicalParent`. Without this, the
    // ring falls into the "non-moon body" branch that returns
    // `coords.position × SCALING_FACTOR = (0,0,0)` — the ring
    // renders at Sol's location.
    //
    // The amplification value `1.0` keeps the ring's own offset
    // (zero) at zero scale; the ring tracks the planet through
    // the orbit but does not add its own offset.
    for body_data in &data.bodies {
        if body_data.body_type != BodyType::Ring {
            continue;
        }
        let Some(entity) = entity_map.get(&body_data.name) else {
            continue;
        };
        let mut entity_cmds = commands.entity(*entity);
        entity_cmds.insert(SpaceCoordinates::new(bevy::math::DVec3::ZERO));
        entity_cmds.insert(LocalOrbitAmplification(1.0));
        // Reset Transform to default so the first frame doesn't
        // ship the Vec3::ZERO translation from the initial
        // spawn frame (visible as a single-frame "ring at Sol"
        // before `update_render_transform` runs on the same
        // frame).
        entity_cmds.insert(Transform::default());
    }

    info!("Solar system setup complete!");
    // Mark the system as spawned only on the success path so a
    // mid-spawn failure can be retried on the next boot-init cycle.
    // Mirrors `AsteroidRegistryLoaded` / `DayOneFleetSpawned`.
    commands.init_resource::<SolarSystemSpawned>();
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
///
/// Originally registered at `PostStartup`; now registered at
/// `Update` via `crate::boot_init::BootInitPlugin`. `pub` so the
/// boot-init plugin can call it.
pub fn initial_camera_focus(
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

fn calculate_hash<T: Hash + ?Sized>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

/// 3D value noise: hash the integer cell and trilinearly interpolate.
///
/// Used by `create_asteroid_mesh` to displace sphere vertices with a
/// pattern that has no axis-aligned periodicity. The earlier sine-wave
/// superposition produced visibly-banded "stack of rings" artefacts
/// (see 79 Eurynome screenshots) because the lowest-frequency layer
/// dominated at the macro scale. Value noise gives a genuinely chaotic
/// surface that looks like a rubble-pile asteroid.
///
/// Returns a float in approximately [-1, 1].
fn value_noise_3d(p: Vec3, seed: u64) -> f32 {
    // Integer cell coordinates.
    let xi = p.x.floor() as i32;
    let yi = p.y.floor() as i32;
    let zi = p.z.floor() as i32;
    // Fractional position inside the cell, in [0, 1].
    let xf = p.x - xi as f32;
    let yf = p.y - yi as f32;
    let zf = p.z - zi as f32;
    // Smoothstep so the surface is C¹-continuous across cell boundaries
    // (linear interpolation gives visible creases).
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let w = zf * zf * (3.0 - 2.0 * zf);

    // Hash the eight cell corners. The seed is mixed in via xor so two
    // asteroids with different seeds produce different noise fields.
    let hash = |x: i32, y: i32, z: i32| -> f32 {
        // Three large primes from the standard integer-hash toolkit;
        // xor with the seed keeps the field stable per body.
        let mut h: u32 = (x as u32).wrapping_mul(73856093)
            ^ (y as u32).wrapping_mul(19349663)
            ^ (z as u32).wrapping_mul(83492791)
            ^ seed as u32;
        // Mix bits and map to [-1, 1].
        h ^= h << 13;
        h ^= h >> 17;
        h ^= h << 5;
        ((h % 10000) as f32 / 5000.0) - 1.0
    };

    let c000 = hash(xi, yi, zi);
    let c100 = hash(xi + 1, yi, zi);
    let c010 = hash(xi, yi + 1, zi);
    let c110 = hash(xi + 1, yi + 1, zi);
    let c001 = hash(xi, yi, zi + 1);
    let c101 = hash(xi + 1, yi, zi + 1);
    let c011 = hash(xi, yi + 1, zi + 1);
    let c111 = hash(xi + 1, yi + 1, zi + 1);

    // Trilinear interpolation.
    let x00 = c000 + u * (c100 - c000);
    let x10 = c010 + u * (c110 - c010);
    let x01 = c001 + u * (c101 - c001);
    let x11 = c011 + u * (c111 - c011);
    let y0 = x00 + v * (x10 - x00);
    let y1 = x01 + v * (x11 - x01);
    y0 + w * (y1 - y0)
}

/// Fractal Brownian Motion built from value noise. Sums 6 octaves at
/// increasing frequency and decreasing amplitude so the macro shape has
/// Ridged FBM with two octaves. Quilez's ridged-noise formula
/// `1 - 2 * |n|` produces sharp ridge lines where the underlying
/// value noise crosses zero. Used at low frequency in the asteroid
/// pipeline to give the macro silhouette ridge-like features
/// (rocky backbones) rather than blobby noise bumps. The previous
/// `eroded_fbm_3d` derivative-damping approach was producing
/// "fuzzy" silhouettes (Vesta shot) because damping shifts the
/// high-frequency energy into the silhouette edge instead of the
/// texture. Plain ridged FBM with G=0.5 gives clean lines.
///
/// Two octaves at lacunarity 2.0, gain 0.5. Output is in [-1, 1].
fn ridged_fbm_2(p: Vec3, seed: u64) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut max_amp = 0.0;
    for _ in 0..2 {
        let n = value_noise_3d(p * freq, seed);
        // 1 - 2*|n| maps value noise in [-1, 1] to a ridge signal
        // in [-1, 1] with peaks at the zero-crossings of the
        // underlying noise.
        sum += amp * (1.0 - 2.0 * n.abs());
        max_amp += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / max_amp
}

/// Two-octave standard FBM. The dominant macro-shape signal in the
/// asteroid pipeline. Output is in [-1, 1] approximately. Most
/// weights go to the first octave so the result reads as one
/// rolling-lump shape rather than a noise field.
fn fbm_at_2(p: Vec3, seed: u64) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut max_amp = 0.0;
    for _ in 0..2 {
        sum += amp * value_noise_3d(p * freq, seed);
        max_amp += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / max_amp
}

/// Mathematical definition of a single impact crater: a point on
/// the unit sphere, a rim radius (geodesic, in radians), and a
/// depth (negative, applied inside the rim). The displacement
/// profile at a vertex is given by `sample_crater_field`.
#[derive(Clone, Copy)]
struct Crater {
    /// Unit-vector centre direction (already rotated into body
    /// frame by the body seed; angles are absolute on the unit
    /// sphere).
    centre: Vec3,
    /// Rim radius in radians. 0.25 ≈ 14° across — small enough to
    /// look like a discrete crater, large enough to be visible at
    /// any reasonable zoom. Real asteroid craters range from 0.05
    /// to 0.6 radians.
    rim_radius: f32,
    /// Depth, in units of `visual_radius`. The deepest part of the
    /// bowl is at `1 - depth` (i.e. sunk inward). Real asteroid
    /// craters are depth/rim ≈ 0.10–0.20.
    depth: f32,
}

/// Build the per-body crater catalogue. The number of craters is
/// 3–8 depending on the body's physical size; craters on small
/// bodies are smaller and more numerous, craters on large bodies
/// are wider and deeper. Each crater is deterministically seeded
/// from `seed + physical_radius_km` so the same asteroid always
/// gets the same craters across runs.
///
/// Returns `(craters, macro_seed, ridged_seed, micro_seed)` — the
/// three seeds are derived from the body seed so we don't have to
/// reuse `seed` for distinct noise fields.
fn build_asteroid_features(
    seed: u64,
    physical_radius_km: f32,
    _irregularity_factor: f32,
) -> (Vec<Crater>, u64, u64, u64) {
    let mut rng = StdRng::seed_from_u64(seed.wrapping_mul(0x9e3779b97f4a7c15));

    // Number of craters scales mildly with size — small bodies
    // have many small craters, large bodies have a few big ones.
    // 3 craters minimum, scaling up to 8 with size.
    let n_craters = if physical_radius_km > 500.0 {
        3 + (rng.random::<u32>() % 3) as usize // 3..=5
    } else if physical_radius_km > 100.0 {
        4 + (rng.random::<u32>() % 3) as usize // 4..=6
    } else {
        5 + (rng.random::<u32>() % 4) as usize // 5..=8
    };

    let mut craters = Vec::with_capacity(n_craters);
    for _ in 0..n_craters {
        // Random unit vector via spherical coords with a uniform
        // cos(theta) distribution (the standard inverse-CDF
        // approach using `rng.random_range`). Equal-area on the
        // sphere so craters are evenly distributed without
        // polar-clustering artefacts.
        let u = rng.random::<f32>();
        let cos_theta = 2.0 * u - 1.0;
        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let phi = rng.random::<f32>() * std::f32::consts::TAU;
        let centre = Vec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);

        // Rim radius: 0.10 to 0.45 radians. Larger bodies get
        // wider craters proportionally.
        let rim_radius = 0.10 + rng.random::<f32>() * 0.35;

        // Depth: 0.05 to 0.25 of the radius. Deeper craters on
        // smaller bodies (rubble-pile shape is more mouldable).
        let depth = 0.05 + rng.random::<f32>() * 0.20;

        craters.push(Crater {
            centre,
            rim_radius,
            depth,
        });
    }

    let macro_seed = seed.wrapping_add(0x1a2b3c);
    let ridged_seed = seed.wrapping_add(0x4d5e6f);
    let micro_seed = seed.wrapping_add(0x7a8b9c);

    (craters, macro_seed, ridged_seed, micro_seed)
}

/// Compute the displacement contribution from all craters at the
/// unit direction `dir`. Each crater contributes a smooth bowl
/// profile centred on its `centre` direction, with the deepest
/// part inside the rim and a smooth ramp back to zero at the rim.
///
/// The bowl profile is `1 - smoothstep(rim_inner, rim_outer, d)`
/// where `d` is the geodesic distance to the crater centre. The
/// inner-rim band (`rim_inner = 0.4 * rim_radius`) is the flat
/// bottom of the bowl; the outer-rim band is the raised rim that
/// gradually returns to zero displacement. We sum the negative
/// contributions across all craters — overlapping craters deepen
/// the basin where their bowls intersect.
fn sample_crater_field(dir: Vec3, craters: &[Crater]) -> f32 {
    let mut displacement = 0.0_f32;
    for crater in craters {
        // Geodesic distance in radians (= arccos of dot, clamped).
        let dot = dir.dot(crater.centre).clamp(-1.0, 1.0);
        let dist = dot.acos();
        // Smoothstep over the rim band of the crater.
        let rim_outer = crater.rim_radius;
        let rim_inner = crater.rim_radius * 0.4;
        if dist >= rim_outer {
            // Outside the rim — no contribution.
            continue;
        }
        // Bowl profile: -1 at the centre, 0 at the rim. The
        // smoothstep gives a smooth (cosine-like) ramp.
        let t = (dist - rim_inner) / (rim_outer - rim_inner);
        let t = t.clamp(0.0, 1.0);
        let bowl = 1.0 - t * t * (3.0 - 2.0 * t);
        displacement -= crater.depth * bowl;
    }
    displacement
}

// --- Shape-class palette ------------------------------------------------
//
// Real asteroids span a recognisable shape vocabulary. The categories
// below are deliberately smooth (no axis-aligned spikes) so the noise
// displacement can layer on top without producing visible facets.
//
// Each class is a function `shape_radius_factor(dir, axes)` that
// returns a scalar in roughly [0.7, 1.3] for any unit direction. The
// main mesh vertex is then placed at `dir * visual_radius * factor`.
//
// The shape axes encode the per-body randomized axis orientation:
//   - `pole_axis`: which body axis is the long / short one
//   - `lobe_main` / `lobe_minor`: the two contact-binary attractors
//   - `equator_ratio`: how much the equator bulges vs the poles
//   - `neck_strength`: how pronounced the waist constriction is
//
// `shape_seed` is derived from the body seed so the same asteroid
// always gets the same shape.

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShapeClass {
    /// Hydrostatic round. Factor ~1.0 everywhere.
    Sphere,
    /// Strongly elongated along `pole_axis`. Factor ~1.3 at poles,
    /// ~0.7 at equator.
    Prolate,
    /// Flattened along `pole_axis`. Factor ~1.2 at equator, ~0.7
    /// at poles.
    Oblate,
    /// Spinning top: equatorial ridge + polar flattening. Factor
    /// ~1.25 at the equator, ~0.75 at the poles.
    Diamond,
    /// Two lobes + a narrower waist. Factor swings from ~1.3 at
    /// each attractor to ~0.75 at the midpoint between them.
    ContactBinary,
    /// Hourglass with two large lobes and a thin neck. Factor
    /// ~1.3 at each attractor, ~0.65 at the neck.
    Dogbone,
}

/// Per-body randomized shape parameters. Determined by `shape_seed`.
struct ShapeAxes {
    /// Which body axis is the "pole" of the shape (the long axis
    /// for Prolate, the short axis for Oblate, the rotation axis
    /// for Diamond, etc.).
    pole_axis: Vec3,
    /// Primary contact-binary attractor (the larger of the two for
    /// Dogbone). Used by ContactBinary and Dogbone.
    lobe_main: Vec3,
    /// Secondary contact-binary attractor. Always antipodal to
    /// `lobe_main` so the two lobes are on opposite sides.
    lobe_minor: Vec3,
    /// Strength of the equatorial bulge (Diamond) or waist
    /// constriction (Dogbone). In [0, 1].
    shape_intensity: f32,
}

impl ShapeAxes {
    fn from_seed(shape_seed: u64) -> Self {
        // Random pole axis. We avoid the degenerate case where
        // the pole axis is too close to vertical by sampling
        // uniformly on the sphere (the standard cosθ
        // distribution).
        let cos_theta = 2.0 * rng_deterministic_f32(shape_seed, 10) - 1.0;
        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let phi = rng_deterministic_f32(shape_seed, 11) * std::f32::consts::TAU;
        let pole_axis = Vec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);

        // Two antipodal attractor points. Choose a random axis
        // perpendicular to the pole axis, then the two attractors
        // are at ±that axis.
        let perp = pole_axis.any_orthonormal_pair();
        let perp_axis = perp.0;
        let attractor_lon = rng_deterministic_f32(shape_seed, 12) * std::f32::consts::TAU;
        let rot = Quat::from_axis_angle(pole_axis, attractor_lon);
        let lobe_main = rot * perp_axis;
        let lobe_minor = -lobe_main;

        // Shape intensity. Most bodies are on the gentler end
        // so the shape doesn't dominate the noise field.
        let shape_intensity = 0.4 + rng_deterministic_f32(shape_seed, 13) * 0.4;

        Self {
            pole_axis,
            lobe_main,
            lobe_minor,
            shape_intensity,
        }
    }
}

/// Pick a shape class deterministically from `shape_seed`,
/// constrained by physical size (large bodies are round).
fn pick_shape_class(shape_seed: u64, physical_radius_km: f32) -> ShapeClass {
    // Large bodies (>500 km) are hydrostatic round.
    if physical_radius_km > 500.0 {
        return ShapeClass::Sphere;
    }

    // Even moderately large bodies skip the most extreme shapes.
    let r = (shape_seed >> 8) as u32 % 100;

    if physical_radius_km > 200.0 {
        // 50% Sphere, 20% Prolate, 15% Oblate, 15% Diamond
        match r {
            0..=49 => ShapeClass::Sphere,
            50..=69 => ShapeClass::Prolate,
            70..=84 => ShapeClass::Oblate,
            _ => ShapeClass::Diamond,
        }
    } else if physical_radius_km > 50.0 {
        // Mid-size rubble piles: full palette, but rare dogbone.
        match r {
            0..=24 => ShapeClass::Sphere,
            25..=44 => ShapeClass::Prolate,
            45..=59 => ShapeClass::Oblate,
            60..=79 => ShapeClass::Diamond,
            80..=94 => ShapeClass::ContactBinary,
            _ => ShapeClass::Dogbone,
        }
    } else {
        // Small bodies (<50 km): dogbone is most common here —
        // these are the rubble piles that have undergone the
        // most reshaping by YORP spin-up.
        match r {
            0..=14 => ShapeClass::Sphere,
            15..=29 => ShapeClass::Prolate,
            30..=44 => ShapeClass::Oblate,
            45..=64 => ShapeClass::Diamond,
            65..=84 => ShapeClass::ContactBinary,
            _ => ShapeClass::Dogbone,
        }
    }
}

/// Compute the radial factor for a shape class at a given
/// body-frame direction. The factor is centred on 1.0, with the
/// shape class perturbing it within [0.7, 1.3] for Sphere,
/// Prolate, Oblate, Diamond; [0.75, 1.3] for ContactBinary; and
/// [0.65, 1.3] for Dogbone.
fn shape_radius_factor(class: ShapeClass, dir: Vec3, axes: &ShapeAxes) -> f32 {
    let cos_lat = dir.dot(axes.pole_axis).clamp(-1.0, 1.0);
    let sin_lat = (1.0 - cos_lat * cos_lat).max(0.0).sqrt();

    match class {
        ShapeClass::Sphere => 1.0,
        ShapeClass::Prolate => {
            // Long along the pole axis. Equator is squashed,
            // poles are stretched.
            //
            // sin²(lat) at the equator gives 0 at the poles
            // and 1 at the equator. We want the opposite: 1.3
            // at the poles, 0.7 at the equator. So we use
            // cos²(lat) = 1 - sin²(lat).
            let pole_factor = 1.0 + 0.3 * axes.shape_intensity;
            let equator_factor = 1.0 - 0.3 * axes.shape_intensity;
            equator_factor + (pole_factor - equator_factor) * cos_lat * cos_lat
        }
        ShapeClass::Oblate => {
            // Flattened along the pole axis. Equator bulges,
            // poles are squashed.
            // sin²(lat) = 1 at the equator, 0 at the poles.
            let equator_factor = 1.0 + 0.2 * axes.shape_intensity;
            let pole_factor = 1.0 - 0.3 * axes.shape_intensity;
            pole_factor + (equator_factor - pole_factor) * sin_lat * sin_lat
        }
        ShapeClass::Diamond => {
            // Spinning top: equatorial ridge with a sharp
            // falloff toward the poles. The factor is highest
            // at the equator and drops smoothly to its minimum
            // at the poles.
            //
            // We use |cos(2 * lat)| = |1 - 2 * sin²(lat)| which
            // is 1 at the equator and the poles, 0 at the
            // ±45° latitudes. Combined with the smooth ramp, it
            // gives a "spinning top" silhouette.
            let ridge_amp = 0.20 * axes.shape_intensity;
            let polar_drop = 0.25 * axes.shape_intensity;
            // Base ridge: stronger at the equator than at the
            // poles, with a smooth taper.
            let ridge = ridge_amp * (1.0 - cos_lat * cos_lat).powf(1.5);
            // Polar flattening: pulls the poles inward.
            let pole_drag = -polar_drop * cos_lat * cos_lat;
            1.0 + ridge + pole_drag
        }
        ShapeClass::ContactBinary => {
            // Two lobes joined by a neck. Each lobe is centred
            // on one attractor. The factor is the maximum of
            // two Gaussian-like bumps centred on the attractors,
            // minus a smooth pull-in at the waist.
            let dot_main = dir.dot(axes.lobe_main).clamp(0.0, 1.0);
            let dot_minor = dir.dot(axes.lobe_minor).clamp(0.0, 1.0);
            // Smooth bumps: peak at the attractor (dot = 1),
            // falling off as cos² to the side.
            let bump_main = dot_main * dot_main;
            let bump_minor = dot_minor * dot_minor;
            // Max of the two lobes (smooth max).
            let lobes = (bump_main + bump_minor) * 0.5;
            // The midpoint between the lobes is at dir = 0
            // (attractors are antipodal). Suppress the equator
            // between them so the waist reads.
            let waist = (cos_lat * cos_lat).min(1.0);
            let waist_pull = 0.25 * axes.shape_intensity * waist;
            1.0 + 0.30 * axes.shape_intensity * lobes - waist_pull
        }
        ShapeClass::Dogbone => {
            // Hourglass with two large lobes and a thin neck.
            // Same two-attractor geometry as ContactBinary but
            // the lobes are stronger and the waist is narrower.
            let dot_main = dir.dot(axes.lobe_main).clamp(0.0, 1.0);
            let dot_minor = dir.dot(axes.lobe_minor).clamp(0.0, 1.0);
            // The neck is between the two lobes, at dir = 0
            // (attractors are antipodal). We narrow the neck
            // by steepening the radial suppression near the
            // waist.
            let dist_from_attractor = (1.0 - dot_main.max(dot_minor)).max(0.0);
            // Strong bump at each attractor.
            let lobes = 0.30 * axes.shape_intensity * dot_main.max(dot_minor).powf(1.5);
            // Neck pull: the further from any attractor, the
            // more the radius shrinks. The neck is the deepest
            // point.
            let neck = 0.35 * axes.shape_intensity * dist_from_attractor.powf(1.2);
            1.0 + lobes - neck
        }
    }
}

/// Compute the direction to feed the noise field for a given
/// shape class. For Sphere/Prolate/Oblate/Diamond we use the
/// body-frame direction directly — the noise field is sampled on
/// the unit sphere, so the craters and lumps are evenly
/// distributed. For ContactBinary and Dogbone we sample relative
/// to the closest attractor so the noise field follows the
/// two-lobe geometry (each lobe has its own craters).
fn shape_noise_lookup(class: ShapeClass, dir: Vec3, axes: &ShapeAxes) -> Vec3 {
    match class {
        ShapeClass::Sphere | ShapeClass::Prolate | ShapeClass::Oblate | ShapeClass::Diamond => {
            dir.normalize_or_zero()
        }
        ShapeClass::ContactBinary | ShapeClass::Dogbone => {
            // Pick the closest attractor and feed the noise
            // field the direction relative to that attractor.
            let dot_main = dir.dot(axes.lobe_main);
            let dot_minor = dir.dot(axes.lobe_minor);
            let attractor = if dot_main > dot_minor {
                axes.lobe_main
            } else {
                axes.lobe_minor
            };
            // The relative direction is `dir - attractor *
            // dot(dir, attractor)`. We then renormalise so the
            // noise falloff is uniform across the lobe.
            let parallel = attractor * dot_main.max(dot_minor);
            let perp = (dir - parallel).normalize_or_zero();
            // Blend with the attractor direction so the noise
            // field is centred on the lobe but still has some
            // global orientation reference.
            (perp * 0.7 + attractor * 0.3).normalize_or_zero()
        }
    }
}

/// Deterministic pseudo-random in [0, 1) derived from a seed
/// and an axis index. Replaces `rand::random()` so the shape
/// class is deterministic across runs without keeping the
/// `StdRng` around.
fn rng_deterministic_f32(seed: u64, axis: u32) -> f32 {
    let mut h: u64 = seed
        .wrapping_add((axis as u64).wrapping_mul(0x9e3779b97f4a7c15))
        .wrapping_add(0x123456789abcdef0);
    h ^= h << 13;
    h ^= h >> 7;
    h ^= h << 17;
    ((h % 1_000_000) as f32) / 1_000_000.0
}

fn create_asteroid_mesh(visual_radius: f32, physical_radius_km: f32, seed: u64) -> Mesh {
    // Generate icosphere base. Bevy's `Sphere::ico(5)` returns a
    // uniformly-triangulated icosphere (subdivision 5 ≈ 642 vertices,
    // 1280 triangles). The previous 96×48 UV sphere had pole
    // singularities: vertical-strip quads collapse to triangles at the
    // poles, and the noise displacement amplified those into spike
    // fans (Hathor read as a chimney-stack silhouette). An icosphere
    // has roughly the same triangle everywhere on the surface, so
    // the displacement produces a uniform rocky-sphere read.
    let mut mesh = Sphere::new(visual_radius).mesh().ico(5).unwrap();

    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        // Note: `rng` is not constructed here -- the per-body
        // randomness is consumed entirely by `build_asteroid_features`
        // (which builds the crater catalogue and the noise seeds).
        // The crater catalogue is what breaks axis-aligned regularity
        // in the asteroid silhouette.

        // ---- Per-body shape factors --------------------------------
        //
        // Real asteroids are mildly irregular spheroids. Bodies past
        // hydrostatic equilibrium (>500 km, e.g. Ceres, Vesta) are
        // round; small rubble-pile bodies (<200 km) are noticeably
        // lumpy but stay within ~18% of the mean radius. The previous
        // 0.40 cap was too high — Eros / Bennu are 0.13–0.18 lumpy,
        // not 0.40. The 0.18 cap matches the realistic range.
        let irregularity_factor = if physical_radius_km > 500.0 {
            0.04 // Mostly round
        } else if physical_radius_km > 200.0 {
            // Linear interpolation from 0.04 at 500km to 0.18 at 200km
            0.04 + (1.0 - (physical_radius_km - 200.0) / 300.0) * 0.14
        } else {
            0.18 // Mildly irregular (was 0.40)
        };

        // ---- Per-body shape-class palette --------------------------
        //
        // Real asteroids span a recognisable shape vocabulary:
        //
        //   - Sphere (Vesta, Ceres): round, hydrostatic.
        //   - Prolate (Eros, Toutatis): strongly elongated along
        //     one axis, ~2.5:1:1 aspect ratio.
        //   - Oblate (Vesta, Ceres-class): flattened along one axis,
        //     ~1:1:0.7 ratio.
        //   - Diamond / spinning top (Bennu, Ryugu): equatorial
        //     ridge + polar flattening — the iconic "spinning top".
        //   - Contact binary (Itokawa): two lobes joined by a
        //     narrower waist, "peanut" silhouette.
        //   - Dogbone (Kleopatra): hourglass with two large lobes
        //     and a thin neck.
        //
        // The previous "Cylinder 1.6× / Wedge 0.85×" attempts were
        // unrealistic pole-shapes. A shape-class palette produces
        // silhouettes that players can recognise as Bennu-shaped
        // or Itokawa-shaped.
        //
        // Large bodies (>500 km) collapse to Sphere because the
        // hydrostatic-equilibrium bodies are rounded. The most
        // extreme shapes (Dogbone) are reserved for mid-size
        // bodies that have the surface area to express them.
        let shape_seed = seed.wrapping_mul(0x9e3779b97f4a7c15);
        let shape_class = pick_shape_class(shape_seed, physical_radius_km);
        let shape_axes = ShapeAxes::from_seed(shape_seed);
        let shape_rotation = Quat::from_euler(
            EulerRot::XYZ,
            rng_deterministic_f32(shape_seed, 0),
            rng_deterministic_f32(shape_seed, 1),
            rng_deterministic_f32(shape_seed, 2),
        );

        // ---- Crater catalogue --------------------------------------
        //
        // Real asteroids are dominated by impact craters. We pick 3–8
        // craters per body, each defined by a centre direction, a
        // rim radius (in radians), and a depth. The displacement
        // contribution at a vertex `dir` is a smooth bowl profile
        // gated by the geodesic distance to the centre. The bowl
        // profile is `1 - smoothstep(rim_inner, rim_outer, dist)` so
        // the deepest part is cos-shaped and the rim is sharp. Real
        // asteroid surfaces are nothing-but-craters at typical
        // gameplay zoom distances, so we lean toward "lots of
        // craters per body".
        let (craters, macro_seed, ridged_seed, micro_seed) =
            build_asteroid_features(seed, physical_radius_km, irregularity_factor);

        let noise_scale = 2.5; // 5–6 macro lumps around the equator

        let new_positions: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| {
                let v = Vec3::from(*p);
                let dir = v.normalize_or_zero();

                // -- Shape-class silhouette --------------------------
                //
                // Rotate the vertex direction into the shape's
                // body frame so the class's intrinsic scaling
                // (e.g. equatorial ridge for Diamond, two-lobe
                // attractors for ContactBinary) is applied along
                // the right axis. The shape class returns a
                // scalar multiplier in roughly [0.7, 1.3] that
                // tells us how far from centre this vertex
                // should sit relative to the mean radius.
                let body_dir = shape_rotation.inverse() * dir;
                let shape_factor = shape_radius_factor(shape_class, body_dir, &shape_axes);

                // The shape's intrinsic scaling defines the
                // body-frame direction we feed to the noise
                // field. For most shapes the natural choice is
                // the body-frame direction itself (so the lumps
                // and craters appear on the body's shape rather
                // than on a separate sphere). For ContactBinary
                // and Dogbone, the lookup is centred on the
                // closest attractor so the noise follows the
                // lobes.
                let lookup_dir = shape_noise_lookup(shape_class, body_dir, &shape_axes);

                // --- Macro lumps: very-low-frequency FBM ------------
                //
                // Two octaves at lacunarity 2.0, gain 0.5. This is
                // the dominant silhouette signal — the "big rock"
                // lumps. No derivative damping: the previous
                // derivative damping was producing "fuzz" (Vesta
                // shot) because damping shifts the high-frequency
                // energy into the silhouette instead of the texture.
                // Plain FBM with G=0.5 gives smooth rolling lumps
                // that real asteroid meshes show.
                let lumps = fbm_at_2(lookup_dir * noise_scale, macro_seed);

                // --- Macro ridges: ridged-noise FBM ------------------
                //
                // Quilez's ridged FBM: `1 - 2 * |n|`. Produces sharp
                // ridge lines where the underlying value noise
                // crosses zero. Used at lower frequency than the
                // lumps so the ridges feel like the body's
                // structural features rather than fine surface
                // pitting. Real asteroid photography shows that the
                // major bumps are ridge-like, not noise-bump-like.
                let ridges = ridged_fbm_2(lookup_dir * (noise_scale * 0.5), ridged_seed);

                // --- Craters: analytical bowl profile ---------------
                //
                // Each crater is a depth-bias profile smoothed at the
                // rim. We use the geodesic distance to the crater
                // centre (not the Euclidean chord) so the bowl is
                // correctly circular on the sphere regardless of
                // shape. The deepest basin dominates when craters
                // overlap.
                let crater_displacement = sample_crater_field(lookup_dir, &craters);

                // --- Micro detail: low-amplitude high-frequency noise
                //
                // A single octave at high frequency adds surface
                // texture that the rock albedo/normal map can then
                // bring out. Kept very low so it doesn't dominate
                // the silhouette — the macro shape is the lumps +
                // ridges + craters, the texture is the rock material.
                let micro = value_noise_3d(lookup_dir * 12.0, micro_seed);

                // --- Combine -----------------------------------------
                //
                // The shape class contributes a moderate
                // large-scale silhouette deviation (in [0.7, 1.3]
                // for most classes). The noise field adds
                // fine-grained weathering on top (lumps + ridges +
                // micro), bounded by `irregularity_factor`. Craters
                // are a separate negative contribution.
                //
                // The final displacement is the shape factor
                // (centred on 1.0) plus the noise deviation times
                // `irregularity_factor`. Total silhouette
                // deviation is at most `shape_factor - 1.0 +                                     // irregularity * 0.93`, which for the
                // typically-picked shape factors keeps the
                // silhouette within realistic limits.
                let noise = lumps * 0.7 + ridges * 0.3 + micro * 0.15;
                let crater = crater_displacement * 0.5;

                let displacement =
                    shape_factor + noise * irregularity_factor + crater * irregularity_factor;

                // Apply the displacement to the unit direction.
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
            // Other colonies start with a bootstrap supply sized for
            // the v3.2 starter-tier buildings (HabitatTent 3 Fe + 5
            // Si, HabitatModule 10 Fe + 15 Si + 1 Cu + 3 Al) plus a
            // handful of metropolitan buildings (LifeSupport, Farm,
            // WaterProcessor). 50 Mt Fe lets the player build ~5
            // HabitatTents + 2 HabitatModules + 1 IronMine; 100 Mt
            // Si covers the HabitatModule cost + a WaterProcessor.
            //
            // v3.2 (2026-08-07): bumped from the v0.5.0
            // 10 Fe / 50 Si / 2 Al / 0.5 Cu / 1 Poly / 0 P / 5 Water
            // (couldn't even afford a single Farm — needed P 3 and
            // bootstrap had 0). New values cover the starter-tier
            // building set so the player can found a working outpost
            // before the first freighter arrives.
            //
            // (All values in Mt.)
            LocalStockpile::with_stockpiles([
                (ResourceType::Iron, 50.0),
                (ResourceType::Silicates, 100.0),
                (ResourceType::Aluminum, 10.0),
                (ResourceType::Copper, 5.0),
                (ResourceType::Polymers, 5.0),
                (ResourceType::Phosphorus, 5.0),
                (ResourceType::Food, 10_000.0),
                (ResourceType::Water, 20.0),
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
