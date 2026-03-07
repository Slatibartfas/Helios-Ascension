//! Starmap view module
//!
//! When the camera zooms out past `STARMAP_TRANSITION_THRESHOLD`, the game
//! transitions from the detailed solar-system view to a sector/galaxy-level
//! starmap. In the starmap:
//!
//!  - Individual celestial bodies and orbit paths are hidden.
//!  - Each star system is represented by a single glowing icon/billboard.
//!  - Single-clicking a system icon selects/highlights it.
//!  - Double-clicking a system icon anchors the camera and zooms into the system.
//!
//! Currently only the Sol system exists; more systems will be added later.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use bevy_egui::EguiPrimaryContextPass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::camera::{CameraAnchor, EguiPanelBounds, GameCamera, OrbitCamera, ViewMode};
use super::solar_system::{Billboard, CelestialBody, Ring, StarGlowMaterial, StarSurfaceMaterial};
use super::solar_system_data::{AsteroidClass, BodyType};
use crate::astronomy::components::{
    CurrentStarSystem, FloatingOrigin, SpaceCoordinates, SurfaceTemperature, SystemId,
};
use crate::astronomy::SCALING_FACTOR;
use crate::game_state::{ActiveMenu, GameMenu};

// Constants replaced by solar_system_data import

/// Default bounding radius for systems without calculated data (in AU).
/// Used for Sol system and as fallback. Sol extends to ~355 AU (Comet NEOWISE).
const DEFAULT_BOUNDING_RADIUS_AU: f64 = 400.0;

/// Default bounding radius for procedurally generated systems (in AU).
/// Most exoplanet systems have planets within ~10 AU; use conservative estimate.
const FALLBACK_BOUNDING_RADIUS_AU: f64 = 50.0;

/// Resource storing metadata about each star system, primarily their bounding radius.
/// This is used to calculate dynamic zoom thresholds.
#[derive(Resource, Default)]
pub struct SystemMetadata {
    /// Map from SystemId to bounding radius in AU
    pub bounding_radii: HashMap<usize, f64>,
}

impl SystemMetadata {
    pub fn set_bounding_radius(&mut self, system_id: usize, radius_au: f64) {
        self.bounding_radii.insert(system_id, radius_au);
    }

    pub fn get_bounding_radius(&self, system_id: usize) -> f64 {
        self.bounding_radii
            .get(&system_id)
            .copied()
            .unwrap_or(FALLBACK_BOUNDING_RADIUS_AU)
    }
}

// ── Planet Category ────────────────────────────────────────────────────────

/// ECS component storing the texture-category string for a celestial body.
///
/// The value is set at spawn time via [`classify_exoplanet`] and can be used by
/// any system that wants to know what visual archetype a body belongs to
/// (e.g. `"jungle"`, `"ice_giant"`, `"lava"`).
#[derive(Component, Debug, Clone)]
pub struct PlanetCategory(pub String);

// ── Planet Texture Manifest ──────────────────────────────────────────────────

/// Maps planet-type category names to lists of texture asset paths.
///
/// Loaded at startup from `assets/data/planet_textures.ron`.  Modders can add
/// or reorder texture paths in any existing category list without changing Rust
/// code.  Introducing brand-new categories requires also updating the
/// `classify_exoplanet` function so those categories are selected.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct PlanetTextureManifest {
    /// Map from category name (e.g. `"jungle"`, `"lava"`) to an ordered list
    /// of texture paths relative to the `assets/` directory.
    pub categories: HashMap<String, Vec<String>>,
}

impl PlanetTextureManifest {
    /// Load the manifest from a RON file on disk.
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let manifest: PlanetTextureManifest = ron::from_str(&contents)?;
        Ok(manifest)
    }

    /// Pick a texture from a category deterministically by `seed`.
    /// Returns `None` if the category is missing or empty.
    pub fn pick(&self, category: &str, seed: u32) -> Option<&str> {
        let list = self.categories.get(category)?;
        if list.is_empty() {
            return None;
        }
        Some(&list[(seed as usize) % list.len()])
    }

    /// Built-in fallback used when `planet_textures.ron` cannot be read.
    fn default_fallback() -> Self {
        let entries: &[(&str, &[&str])] = &[
            ("barren", &["textures/celestial/planets/mercury_8k.jpg"]),
            (
                "desert",
                &[
                    "textures/celestial/planets/mars_8k.jpg",
                    "textures/celestial/planets/venus_surface_8k.jpg",
                ],
            ),
            ("temperate", &["textures/celestial/planets/earth_8k.jpg"]),
            ("jungle", &["textures/celestial/planets/earth_8k.jpg"]),
            ("ocean", &["textures/celestial/planets/earth_8k.jpg"]),
            ("alpine", &["textures/celestial/planets/earth_8k.jpg"]),
            ("savannah", &["textures/celestial/planets/earth_8k.jpg"]),
            ("swamp", &["textures/celestial/planets/earth_8k.jpg"]),
            (
                "tundra",
                &[
                    "textures/celestial/planets/pluto_8k.png",
                    "textures/celestial/planets/mars_8k.jpg",
                ],
            ),
            (
                "ice",
                &[
                    "textures/celestial/planets/pluto_8k.png",
                    "textures/celestial/planets/eris_2k.jpg",
                ],
            ),
            (
                "lava",
                &[
                    "textures/celestial/planets/mercury_8k.jpg",
                    "textures/celestial/planets/venus_surface_8k.jpg",
                ],
            ),
            (
                "scorched",
                &[
                    "textures/celestial/planets/venus_surface_8k.jpg",
                    "textures/celestial/planets/mercury_8k.jpg",
                ],
            ),
            (
                "gas_giant",
                &[
                    "textures/celestial/planets/jupiter_8k.jpg",
                    "textures/celestial/planets/saturn_8k.jpg",
                ],
            ),
            (
                "ice_giant",
                &[
                    "textures/celestial/planets/neptune_2k.jpg",
                    "textures/celestial/planets/uranus_2k.jpg",
                ],
            ),
            (
                "dwarf",
                &[
                    "textures/celestial/planets/pluto_8k.png",
                    "textures/celestial/planets/eris_2k.jpg",
                    "textures/celestial/asteroids/generic_s_type_2k.jpg",
                    "textures/celestial/asteroids/generic_c_type_2k.jpg",
                ],
            ),
            (
                "moon",
                &[
                    "textures/celestial/moons/moon_8k.jpg",
                    "textures/celestial/moons/europa_4k.png",
                    "textures/celestial/moons/ganymede_4k.jpg",
                    "textures/celestial/moons/callisto_4k.jpg",
                    // titan_4k.jpg excluded — it is a Cassini RADAR/SAR map
                    // (monochromatic/dark), not a colour image.
                    "textures/celestial/moons/triton_4k.jpg",
                ],
            ),
            (
                "asteroid_s",
                &["textures/celestial/asteroids/generic_s_type_2k.jpg"],
            ),
            (
                "asteroid_c",
                &["textures/celestial/asteroids/generic_c_type_2k.jpg"],
            ),
            (
                "comet",
                &["textures/celestial/comets/generic_nucleus_2k.jpg"],
            ),
        ];
        let categories = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect();
        Self { categories }
    }
}

/// Startup system that loads `assets/data/planet_textures.ron` and inserts it
/// as a `PlanetTextureManifest` resource.  Falls back to built-in defaults if
/// the file cannot be read.
fn load_planet_texture_manifest(mut commands: Commands) {
    match PlanetTextureManifest::load_from_file("assets/data/planet_textures.ron") {
        Ok(manifest) => {
            info!(
                "Loaded planet texture manifest ({} categories)",
                manifest.categories.len()
            );
            commands.insert_resource(manifest);
        }
        Err(e) => {
            warn!(
                "Could not load planet_textures.ron ({}). Using built-in defaults.",
                e
            );
            commands.insert_resource(PlanetTextureManifest::default_fallback());
        }
    }
}

/// Plugin that manages the starmap view layer.
pub struct StarmapPlugin;

impl Plugin for StarmapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentStarSystem>()
            .init_resource::<FloatingOrigin>()
            .init_resource::<SystemMetadata>()
            .add_systems(Startup, (setup_starmap, load_planet_texture_manifest))
            .add_systems(
                Update,
                (
                    tag_sol_bodies,
                    handle_system_transition,
                    handle_starmap_transition,
                    spawn_system_bodies.after(handle_system_transition),
                    toggle_system_view_entities
                        .after(handle_system_transition)
                        .after(spawn_system_bodies),
                    update_starmap_visibility.after(handle_system_transition),
                    update_starmap_icon_scale,
                    update_starmap_coordinates,
                ),
            )
            // Starmap hover/selection systems use EguiContexts — must run in EguiPrimaryContextPass
            .add_systems(
                EguiPrimaryContextPass,
                (handle_starmap_hover, handle_starmap_selection),
            );
    }
}

// ── Components ──────────────────────────────────────────────────────────────

/// Marker for starmap-level star system icons.
#[derive(Component)]
pub struct StarSystemIcon {
    /// Unique ID of the system (index in the stars array)
    pub id: usize,
    /// Display name shown in the starmap
    pub name: String,
    /// Position in Universe space (AU) from Sol
    pub position: DVec3,
    /// Bounding radius of the system in AU (distance to outermost body)
    /// Used to determine appropriate zoom transition threshold
    pub bounding_radius_au: f64,
}

/// Tag for the Sol system's starmap icon (spawned once at startup).
#[derive(Component)]
pub struct SolSystemIcon;

/// Marker for a star system that is currently hovered by the mouse
#[derive(Component)]
pub struct HoveredStarSystem;

/// Marker for the currently selected/anchored star system in starmap view.
#[derive(Component)]
pub struct SelectedStarSystem;

// ── Startup ─────────────────────────────────────────────────────────────────

// 1 Light Year in Astronomical Units
const LY_TO_AU: f64 = 63241.077;

// (Moved to src/astronomy/nearby_stars.rs)
// 50 Closest Star Systems to Sol (excluding Sol)
// Coordinates in Light Years (Equatorial J2000 Cartesian)
// NEARBY_STARS definition moved to src/astronomy/nearby_stars.rs

/// Returns `(core_col, halo_col, billboard_size)` for a starmap glow billboard,
/// scaled by spectral class so hot giants show a blinding wide corona and
/// dim brown dwarfs show a small, faint ember.
///
/// `billboard_size` is the Rectangle side length in local space (icon sphere = 1.0).
/// Core brightness and halo extent both scale with temperature.
fn star_icon_glow_params(spectral_class: char, r: f32, g: f32, b: f32) -> (Vec4, Vec4, f32) {
    // (core_brightness, halo_brightness, billboard_size)
    // Hot stars: blinding white/blue, large corona.
    // Cool dwarfs: dim ember, tight corona barely hiding the disk.
    let (cb, hb, gs) = match spectral_class {
        'O' => (18.0, 10.0, 14.0), // Blue giants: blinding, huge corona
        'B' => (12.0, 7.0, 12.0),  // Blue-white
        'A' => (7.0, 4.5, 10.0),   // White
        'F' => (4.5, 3.0, 9.0),    // Yellow-white
        'G' => (3.5, 2.5, 8.0),    // Sol-like
        'K' => (3.0, 2.2, 7.5),    // Orange
        'M' => (2.2, 1.8, 7.0),    // Red
        'L' => (1.5, 1.0, 6.0),    // Brown dwarf
        _ => (1.0, 0.7, 5.0),      // T, Y, unknown — cold brown dwarfs
    };
    // Core: blend 60 % spectral + 40 % white so hot blue stars trend to
    // white-blue and cool red stars stay warm-orange rather than pure white.
    let core_col = Vec4::new(
        cb * (r * 0.6 + 0.4),
        cb * (g * 0.6 + 0.4),
        cb * (b * 0.6 + 0.4),
        1.0,
    );
    // Halo keeps the full spectral tint.
    let halo_col = Vec4::new(r * hb, g * hb, b * hb, 1.0);
    (core_col, halo_col, gs)
}

/// Spawn the starmap icon for the Sol system.
/// It starts hidden and becomes visible when `ViewMode::Starmap` is active.
fn setup_starmap(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut materials_glow: ResMut<Assets<StarGlowMaterial>>,
    mut system_metadata: ResMut<SystemMetadata>,
) {
    // Initialize Sol's bounding radius
    system_metadata.set_bounding_radius(0, DEFAULT_BOUNDING_RADIUS_AU);

    // A bright glowing sphere representing the star system
    let icon_mesh = meshes.add(Sphere::new(1.0).mesh().uv(16, 8));

    // --- Sol System (ID: 0) ---
    let sol_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.95, 0.7),
        emissive: LinearRgba::new(1.8, 1.7, 1.1, 1.0), // Subdued — corona shader provides most of the brightness
        unlit: true,
        ..default()
    });

    // The icon is placed at the origin (same as the Sun) and scaled
    // dynamically based on camera distance.
    // Sol: G2V, yellow-white
    let (sol_core, sol_halo, sol_gs) = star_icon_glow_params('G', 1.0, 0.95, 0.7);

    commands
        .spawn((
            Mesh3d(icon_mesh.clone()),
            MeshMaterial3d(sol_material),
            Transform::from_translation(Vec3::ZERO),
            Visibility::Hidden, // starts hidden; shown in Starmap mode
            StarSystemIcon {
                id: 0,
                name: "Sol System".to_string(),
                position: DVec3::ZERO,
                bounding_radius_au: DEFAULT_BOUNDING_RADIUS_AU,
            },
            SolSystemIcon,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(Rectangle::new(sol_gs, sol_gs))),
                MeshMaterial3d(materials_glow.add(StarGlowMaterial {
                    color_core: sol_core,
                    color_halo: sol_halo,
                    time_phase: 0.0,
                })),
                Transform::from_translation(Vec3::Z * 0.1),
                Billboard,
            ));
        });

    // --- Nearby Stars (ID: 1..50) ---
    use crate::astronomy::nearby_stars::NEARBY_STARS_POSITIONS;
    for (i, star) in NEARBY_STARS_POSITIONS.iter().enumerate() {
        let id = i + 1; // 0 is Sol

        // Determine color from spectral type
        let (r, g, b) = match star.spectral_type.chars().next().unwrap_or('G') {
            'O' => (0.6, 0.8, 1.0),             // Blue
            'B' => (0.7, 0.85, 1.0),            // Bluish White
            'A' => (0.9, 0.9, 1.0),             // White
            'F' => (1.0, 1.0, 0.9),             // Yellow-White
            'G' => (1.0, 0.95, 0.7),            // Yellow
            'K' => (1.0, 0.8, 0.6),             // Light Orange
            'M' => (1.0, 0.6, 0.4),             // Orange-Red
            'L' | 'T' | 'Y' => (0.8, 0.2, 0.2), // Brown/Dark Red
            _ => (1.0, 1.0, 1.0),               // Default White
        };

        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            // Scale emissive brightness by spectral class so hot stars glow
            // visibly even at maximum starmap zoom-out.
            emissive: LinearRgba::new(r * 2.0, g * 2.0, b * 2.0, 1.0), // Subdued — corona shader provides most of the brightness
            unlit: true,
            ..default()
        });

        // Corona size, core brightness, and halo tint all vary with spectral type.
        let spec_char = star.spectral_type.chars().next().unwrap_or('G');
        let (core_col, halo_col, glow_size) = star_icon_glow_params(spec_char, r, g, b);

        // Convert LY to AU
        let pos_au = DVec3::new(star.pos_ly[0], star.pos_ly[1], star.pos_ly[2]) * LY_TO_AU;

        // Initial transform assumes Origin is Sol (0,0,0)
        // Starmap Scale: 1 Unit = 1 AU.
        let spawn_pos = Vec3::new(pos_au.x as f32, pos_au.y as f32, pos_au.z as f32);

        // Estimate bounding radius for systems without detailed data
        // Most exoplanet systems discovered so far have planets within ~10 AU
        // Binary stars can extend much farther (hundreds to thousands of AU)
        // Use a conservative estimate for unknown systems
        let bounding_radius_au = FALLBACK_BOUNDING_RADIUS_AU;

        commands
            .spawn((
                Mesh3d(icon_mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(spawn_pos),
                Visibility::Hidden,
                StarSystemIcon {
                    id,
                    name: star.name.to_string(),
                    position: pos_au,
                    bounding_radius_au,
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Rectangle::new(glow_size, glow_size))),
                    MeshMaterial3d(materials_glow.add(StarGlowMaterial {
                        color_core: core_col,
                        color_halo: halo_col,
                        time_phase: 0.0,
                    })),
                    Transform::from_translation(Vec3::Z * 0.1),
                    Billboard,
                ));
            });
    }
}

// ── Systems ─────────────────────────────────────────────────────────────────

/// Tag all celestial bodies spawned by solar_system.rs as belonging to System 0 (Sol).
/// We only tag the CelestialBody entity itself. Child entities (lights, clouds, etc)
/// may be added/removed asynchronously and inserting into them here can panic
/// if they are despawned before buffered commands are applied. Child entities'
/// ownership is inferred from their Parent during visibility logic.
fn tag_sol_bodies(
    mut commands: Commands,
    query: Query<Entity, (With<CelestialBody>, Without<SystemId>)>,
) {
    for entity in query.iter() {
        commands.entity(entity).insert(SystemId(0));
    }
}

/// Classify an exoplanet body into a texture category string.
///
/// The category is used to look up a texture from `PlanetTextureManifest` and
/// to derive a tint colour in `category_tint`.
///
/// `has_surface_ocean` and `ocean_is_water` allow the classifier to prefer the
/// "ocean" archetype when a body actually possesses a liquid-water ocean,
/// rather than relying solely on the seed-based random distribution.
pub fn classify_exoplanet(
    body_type: BodyType,
    asteroid_class: Option<AsteroidClass>,
    avg_temp_c: f32,
    seed: u32,
    has_surface_ocean: bool,
    ocean_is_water: bool,
) -> &'static str {
    classify_exoplanet_with_mass(
        body_type,
        asteroid_class,
        avg_temp_c,
        seed,
        has_surface_ocean,
        ocean_is_water,
        None,
    )
}

/// Extended classifier that also considers body mass (kg).
///
/// For `GasGiant` body types, mass distinguishes true gas giants (Jupiter/Saturn,
/// dominated by H/He, mass > 2×10²⁶ kg) from ice giants (Uranus/Neptune,
/// significant "ices" — water, ammonia, methane).
pub fn classify_exoplanet_with_mass(
    body_type: BodyType,
    asteroid_class: Option<AsteroidClass>,
    avg_temp_c: f32,
    seed: u32,
    has_surface_ocean: bool,
    ocean_is_water: bool,
    mass_kg: Option<f64>,
) -> &'static str {
    match body_type {
        BodyType::GasGiant => {
            // Distinguish gas giants (H/He dominated, like Jupiter/Saturn)
            // from ice giants (significant ices, like Uranus/Neptune) by mass.
            // Threshold: ~2×10²⁶ kg (~33 Earth masses) separates the two classes.
            // Falls back to temperature heuristic only when mass is unavailable.
            if let Some(mass) = mass_kg {
                if mass > 2.0e26 {
                    "gas_giant"
                } else {
                    "ice_giant"
                }
            } else if avg_temp_c < -80.0 {
                "ice_giant"
            } else {
                "gas_giant"
            }
        }
        BodyType::DwarfPlanet => "dwarf",
        BodyType::Moon => "moon",
        BodyType::Asteroid => match asteroid_class.unwrap_or(AsteroidClass::CType) {
            AsteroidClass::SType | AsteroidClass::VType | AsteroidClass::MType => "asteroid_s",
            _ => "asteroid_c",
        },
        BodyType::Comet => "comet",
        BodyType::Planet => {
            if avg_temp_c > 500.0 {
                "lava"
            } else if avg_temp_c > 200.0 {
                // Extreme heat (200–500 °C) — Venus-like greenhouse infernos
                "scorched"
            } else if avg_temp_c > 60.0 {
                // Very hot worlds above habitable band
                "desert"
            } else if avg_temp_c >= -20.0 {
                // Habitable-zone planets: split by temperature into four archetypes
                if avg_temp_c < -5.0 {
                    "alpine"
                } else if avg_temp_c > 45.0 {
                    "savannah"
                } else if has_surface_ocean && ocean_is_water {
                    // Bodies with confirmed liquid-water oceans always classify as "ocean"
                    "ocean"
                } else {
                    match seed % 4 {
                        0 => "jungle",
                        1 => "ocean",
                        2 => "temperate",
                        _ => "swamp",
                    }
                }
            } else if avg_temp_c >= -100.0 {
                "tundra"
            } else {
                "ice"
            }
        }
        _ => "barren",
    }
}

/// Return `(base_color_tint, perceptual_roughness, metallic)` for a body
/// category.  The tint multiplies the texture sample to give each planet type
/// a distinctive colour cast while still showing the underlying texture detail.
fn category_tint(category: &str, r1: f32, r2: f32, r3: f32) -> (Color, f32, f32) {
    match category {
        "lava" => {
            // Deep orange-red — volcanic, scorched
            let b = 0.75 + r1 * 0.15;
            (
                Color::srgb(b, (b * 0.32).min(1.0), (b * 0.06).min(1.0)),
                0.70 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "scorched" => {
            // Dark red-orange — Venus-like extreme greenhouse worlds
            let b = 0.82 + r1 * 0.12;
            (
                Color::srgb(b, (b * 0.45).min(1.0), (b * 0.15).min(1.0)),
                0.72 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "desert" => {
            // Warm orange-tan — arid and dusty
            let b = 0.88 + r1 * 0.10;
            (
                Color::srgb(b, (b * 0.80).min(1.0), (b * 0.58).min(1.0)),
                0.75 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "temperate" => {
            // Near-white — let the texture dominate
            let b = 0.90 + r1 * 0.08;
            (Color::srgb(b, b, b), 0.65 + r2 * 0.20, 0.0 + r3 * 0.05)
        }
        "jungle" => {
            // Deep green tint — dense vegetation
            let b = 0.80 + r1 * 0.12;
            (
                Color::srgb((b * 0.68).min(1.0), b, (b * 0.65).min(1.0)),
                0.70 + r2 * 0.18,
                0.0 + r3 * 0.05,
            )
        }
        "ocean" => {
            // Deep blue — ocean-dominated
            let b = 0.80 + r1 * 0.12;
            (
                Color::srgb((b * 0.50).min(1.0), (b * 0.72).min(1.0), b),
                0.60 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "tundra" => {
            // Blue-grey — cold, partially frozen
            let b = 0.78 + r1 * 0.14;
            (
                Color::srgb((b * 0.86).min(1.0), (b * 0.91).min(1.0), b),
                0.78 + r2 * 0.12,
                0.0 + r3 * 0.05,
            )
        }
        "ice" => {
            // Pale blue-white — deeply frozen
            let b = 0.85 + r1 * 0.12;
            (
                Color::srgb((b * 0.88).min(1.0), (b * 0.93).min(1.0), b),
                0.72 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "barren" => {
            // Neutral grey — cratered, airless
            let b = 0.78 + r1 * 0.16;
            (
                Color::srgb(b, (b * 0.93).min(1.0), (b * 0.88).min(1.0)),
                0.80 + r2 * 0.12,
                0.0 + r3 * 0.05,
            )
        }
        "gas_giant" => {
            // Warm amber — banded atmosphere
            let b = 0.88 + r1 * 0.08;
            (
                Color::srgb(b, (b * 0.87).min(1.0), (b * 0.65).min(1.0)),
                0.60 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "ice_giant" => {
            // Blue-cyan — methane-dominated
            let b = 0.78 + r1 * 0.12;
            (
                Color::srgb((b * 0.76).min(1.0), (b * 0.90).min(1.0), b),
                0.62 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "dwarf" => {
            let b = 0.65 + r1 * 0.22;
            (
                Color::srgb(b, (b * 0.95).min(1.0), (b * 0.90).min(1.0)),
                0.78 + r2 * 0.15,
                0.02 + r3 * 0.06,
            )
        }
        "moon" => {
            let b = 0.80 + r1 * 0.18;
            (
                Color::srgb(b, (b * 0.98).min(1.0), (b * 0.95).min(1.0)),
                0.75 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "asteroid_s" => {
            let b = 0.85 + r1 * 0.12;
            (
                Color::srgb(b, (b * 0.93).min(1.0), (b * 0.84).min(1.0)),
                0.68 + r2 * 0.18,
                0.05 + r3 * 0.10,
            )
        }
        "asteroid_c" => {
            let b = 0.52 + r1 * 0.22;
            (
                Color::srgb(b, (b * 0.96).min(1.0), (b * 0.90).min(1.0)),
                0.82 + r2 * 0.12,
                0.02 + r3 * 0.07,
            )
        }
        "comet" => {
            let b = 0.55 + r1 * 0.28;
            let tint_r = r2 * 0.12;
            (
                Color::srgb(
                    (b + tint_r).min(1.0),
                    (b + tint_r * 0.5).min(1.0),
                    (b - tint_r * 0.3).clamp(0.0, 1.0),
                ),
                0.78 + r2 * 0.15,
                0.01 + r3 * 0.05,
            )
        }
        "alpine" => {
            // Cool grey-blue with white snow caps
            let b = 0.82 + r1 * 0.12;
            (
                Color::srgb((b * 0.85).min(1.0), (b * 0.90).min(1.0), b),
                0.72 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "savannah" => {
            // Warm golden-brown — dry grasslands
            let b = 0.85 + r1 * 0.10;
            (
                Color::srgb(b, (b * 0.82).min(1.0), (b * 0.55).min(1.0)),
                0.73 + r2 * 0.15,
                0.0 + r3 * 0.05,
            )
        }
        "swamp" => {
            // Dark green-brown — murky wetlands
            let b = 0.70 + r1 * 0.15;
            (
                Color::srgb(
                    (b * 0.72).min(1.0),
                    (b * 0.85).min(1.0),
                    (b * 0.55).min(1.0),
                ),
                0.78 + r2 * 0.14,
                0.0 + r3 * 0.05,
            )
        }
        _ => (Color::WHITE, 0.80, 0.0),
    }
}

/// Adds visual components (meshes, materials, lights) to existing data-only entities
/// when visiting a non-Sol system for the first time.
fn spawn_system_bodies(
    mut commands: Commands,
    current_system: Res<CurrentStarSystem>,
    floating_origin: Res<FloatingOrigin>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut materials_surface: ResMut<Assets<StarSurfaceMaterial>>,
    mut materials_corona_3d: ResMut<Assets<super::solar_system::StarCorona3dMaterial>>,
    mut materials_halo_3d: ResMut<Assets<super::solar_system::StarHalo3dMaterial>>,
    asset_server: Res<AssetServer>,
    manifest: Res<PlanetTextureManifest>,
    // Query for bodies that need visual components added
    bodies_without_visuals: Query<
        (
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            &SystemId,
            Option<&crate::astronomy::StellarProperties>,
            Option<&SurfaceTemperature>,
        ),
        (Without<Mesh3d>, Without<MeshMaterial3d<StandardMaterial>>),
    >,
    // Query to check if system already has visual entities — exclude Ring entities
    // because rings are spawned with Mesh3d at Startup and would falsely trigger
    // the early-return guard before the star/planet meshes are ever inserted.
    bodies_with_visuals: Query<&SystemId, (With<CelestialBody>, With<Mesh3d>, Without<Ring>)>,
) {
    if !current_system.is_changed() {
        return;
    }

    let sys_id = current_system.0;
    if sys_id == 0 {
        return;
    } // Sol is handled by solar_system.rs

    // Check if this system already has non-ring visual entities
    if bodies_with_visuals.iter().any(|id| id.0 == sys_id) {
        // Visuals already added, nothing to do
        return;
    }

    info!("Adding visual components to system {}", sys_id);

    let origin_offset = floating_origin.position;

    // Find all data-only entities for this system and add visual components
    for (entity, body, space_coords, _system_id, stellar_props, surface_temp) in
        bodies_without_visuals.iter()
    {
        if _system_id.0 != sys_id {
            continue;
        }

        // Use the pre-computed visual_radius from CelestialBody (already scaled
        // for system compactness by system_populator) instead of recalculating.
        let visual_radius = body.visual_radius;

        let avg_temp_c: Option<f32> = surface_temp.map(|t| t.average_celsius);

        // Determine color based on body type
        let color = match body.body_type {
            BodyType::Star => {
                // Calculate color from temperature if available
                if let Some(props) = stellar_props {
                    super::solar_system_data::kelvin_to_color(props.temperature_kelvin)
                } else {
                    Color::srgb(1.0, 0.95, 0.8) // Default yellow star
                }
            }
            BodyType::Planet | BodyType::DwarfPlanet => Color::srgb(0.5, 0.5, 0.7),
            BodyType::GasGiant => Color::srgb(0.9, 0.8, 0.6),
            BodyType::Moon => Color::srgb(0.6, 0.6, 0.6),
            BodyType::Asteroid => Color::srgb(0.4, 0.4, 0.3),
            BodyType::Comet => Color::srgb(0.7, 0.8, 0.9),
            BodyType::Ring => {
                // Rings should have been created as separate entities, skip for now
                continue;
            }
        };

        // Create mesh - Higher resolution for stars to avoid boxy look
        let mesh = if matches!(body.body_type, BodyType::Star) {
            meshes.add(Sphere::new(visual_radius).mesh().uv(128, 64))
        } else {
            meshes.add(Sphere::new(visual_radius).mesh().uv(32, 16))
        };

        // Compute the correct initial transform position using floating origin
        let scaled_position = (space_coords.position - origin_offset) * SCALING_FACTOR;
        let p_vec = Vec3::new(
            scaled_position.x as f32,
            scaled_position.y as f32,
            scaled_position.z as f32,
        );
        let initial_transform = Transform::from_translation(p_vec);

        if matches!(body.body_type, BodyType::Star) {
            // Stars use StarSurfaceMaterial (limb darkening) + 3D volumetric corona/halo,
            // matching how the Sol star is rendered in setup_solar_system.
            let linear = color.to_linear();
            let (cr, cg, cb) = (linear.red, linear.green, linear.blue);

            // Luminosity-based scaling: brighter/hotter stars get a wider, more intense corona.
            // lum_factor uses a steeper pow and a low floor (0.2) so dim M-dwarfs and brown
            // dwarfs produce a proportionally smaller bloom halo than Sol, preventing the
            // unnatural orange glow around close-in planets in compact systems.
            // Sol (L=1.0): lum_factor=1.0  → ×9 center, ×5.5 limb (identical to previous)
            // M-dwarf (L=0.01): lum_factor≈0.32 → ×4.2 center  (noticeably dimmer)
            // Brown dwarf (L=0.001): lum_factor≈0.18 → ×3.25 center
            let luminosity = stellar_props.map(|p| p.luminosity_sol).unwrap_or(1.0);
            // Steeper pow(0.4) so sub-solar stars drop off faster:
            //   Sol (L=1.0):  lum_factor=1.0   → ×9 center (identical to Sol star)
            //   K-type (L=0.34): lum_factor≈0.61 → ×6.3 center
            //   M-dwarf (L=0.01): lum_factor≈0.16 → ×3.1 center (just above bloom threshold)
            let lum_factor = luminosity.powf(0.4).clamp(0.15, 1.0);

            // Surface colours: scale HDR intensity by lum_factor so dim stars don't produce
            // the same bloom spreading radius as Sol.  At lum_factor=1.0 the formula gives
            // exactly ×9 and ×5.5 — identical to the Sol star in setup_solar_system.
            let center_col = Vec4::new(
                cr * (2.0 + 7.0 * lum_factor),
                cg * (2.0 + 7.0 * lum_factor),
                cb * (2.0 + 7.0 * lum_factor),
                1.0,
            );
            // Limb colour: cooler shift — red is retained, green/blue sharply attenuated
            let limb_col = Vec4::new(
                cr * (1.5 + 4.0 * lum_factor),
                cg * (1.5 + 4.0 * lum_factor) * 0.51,
                cb * (1.5 + 4.0 * lum_factor) * 0.15,
                1.0,
            );

            commands.entity(entity).insert((
                Mesh3d(mesh),
                MeshMaterial3d(materials_surface.add(StarSurfaceMaterial {
                    color_center: center_col,
                    color_limb: limb_col,
                    star_texture: None,
                })),
                initial_transform,
            ));

            // Sub-linear (sqrt) intensity scaling compresses the dynamic range so
            // super-luminous stars (Sirius at 25 L☉) don't create reflected-
            // light bloom on very close-in planets, while dim stars still get
            // proportionally less light.
            // Sol (1.0): 1.0×, Sirius (25): ~5×, M-dwarf (0.01): ~0.1×
            let intensity = 2.8e11 * luminosity.max(1e-5).sqrt() * lum_factor;

            // core_col: spectrally-tinted inner corona to match the star sphere surface.
            let core_col = Vec4::new(
                (cr * 5.5 + 1.0) * lum_factor,
                (cg * 5.5 + 1.0) * lum_factor,
                (cb * 5.5 + 1.0) * lum_factor,
                1.0,
            );
            // Gentle warm shift — avoid extreme channel suppression that
            // causes visible colour banding on cool (M/K) stars.
            let halo_col = Vec4::new(
                cr * 4.5 * lum_factor,
                cg * 4.0 * lum_factor,
                cb * 3.0 * lum_factor,
                1.0,
            );

            // Shell radii — match Sol's proportions for realistic corona sizing
            let corona_shell_r = visual_radius * 1.75;
            let halo_shell_r = visual_radius * 4.0;

            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    PointLight {
                        intensity,
                        range: 2.0e9,
                        shadows_enabled: false,
                        color,
                        ..default()
                    },
                    Transform::default(),
                    SystemId(sys_id),
                ));

                // Inner volumetric corona shell
                parent.spawn((
                    Mesh3d(meshes.add(Sphere::new(corona_shell_r).mesh().uv(64, 32))),
                    MeshMaterial3d(materials_corona_3d.add(
                        super::solar_system::StarCorona3dMaterial {
                            color_core: Vec4::ZERO, // LOD system drives it
                            color_halo: Vec4::ZERO,
                            time_phase: 0.0,
                            corona_params: Vec4::new(visual_radius, corona_shell_r, 0.0, 0.0),
                        },
                    )),
                    Transform::default(),
                    super::solar_system::StarCoronaShell {
                        base_core_color: core_col,
                        base_halo_color: halo_col,
                        visual_radius,
                    },
                    SystemId(sys_id),
                ));

                // Outer diffuse halo shell
                parent.spawn((
                    Mesh3d(meshes.add(Sphere::new(halo_shell_r).mesh().uv(32, 16))),
                    MeshMaterial3d(materials_halo_3d.add(
                        super::solar_system::StarHalo3dMaterial {
                            color_halo: Vec4::ZERO, // LOD system drives it
                            time_phase: 0.0,
                            halo_params: Vec4::new(visual_radius, halo_shell_r, 0.0, 0.0),
                        },
                    )),
                    Transform::default(),
                    super::solar_system::StarHaloShell {
                        base_halo_color: halo_col,
                        visual_radius,
                    },
                    SystemId(sys_id),
                ));
            });
        } else {
            // Classify body into a texture category, then look up the manifest
            let avg_temp = avg_temp_c.unwrap_or(-100.0);
            let seed: u32 = body
                .name
                .bytes()
                .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));

            let category = classify_exoplanet(
                body.body_type,
                body.asteroid_class,
                avg_temp,
                seed,
                false,
                false,
            );

            let r1 = ((seed % 1000) as f32) / 1000.0;
            let r2 = (((seed / 1000) % 1000) as f32) / 1000.0;
            let r3 = (((seed / 1_000_000) % 1000) as f32) / 1000.0;
            let (tint, roughness, metallic) = category_tint(category, r1, r2, r3);

            let base_color_texture: Option<Handle<Image>> = manifest
                .pick(category, seed)
                .map(|p| p.to_string())
                .map(|p| asset_server.load(p));
            let has_texture = base_color_texture.is_some();

            // When a texture is loaded, use the tint as a subtle multiplier;
            // when there is no texture, use the fallback flat colour.
            let base_color = if has_texture { tint } else { color };

            let material = materials.add(StandardMaterial {
                base_color,
                // Minimal emissive floor so planets in dim/distant star systems
                // aren't pitch black on the night side.
                emissive: LinearRgba::WHITE * 0.006,
                base_color_texture,
                perceptual_roughness: roughness,
                metallic,
                reflectance: 0.4,
                ..default()
            });

            commands.entity(entity).insert((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                initial_transform,
                PlanetCategory(category.to_string()),
            ));
        }
    }

    info!("Finished adding visual components to system {}", sys_id);
}

/// Hide all celestial bodies and their orbit gizmos when in Starmap mode.
/// Also handles hiding bodies from other systems when in System mode.
fn toggle_system_view_entities(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    mut body_query: Query<(&mut Visibility, Option<&SystemId>), With<CelestialBody>>,
    mut light_query: Query<
        (&mut Visibility, Option<&SystemId>, Option<&ChildOf>),
        (
            With<PointLight>,
            Without<CelestialBody>,
            Without<StarSystemIcon>,
        ),
    >,
    parent_sys_query: Query<&SystemId>,
    newly_spawned_bodies: Query<Entity, Added<CelestialBody>>,
    newly_added_meshes: Query<Entity, (With<CelestialBody>, Added<Mesh3d>)>,
) {
    // Run if view mode changed, current system changed, new bodies were spawned,
    // or existing bodies just received visual components (meshes)
    if !view_mode.is_changed()
        && !current_system.is_changed()
        && newly_spawned_bodies.is_empty()
        && newly_added_meshes.is_empty()
    {
        return;
    }

    match *view_mode {
        ViewMode::System => {
            // Show bodies only for the current system
            for (mut vis, sys_id) in body_query.iter_mut() {
                let id = sys_id.map(|s| s.0).unwrap_or(0); // Default to Sol if untagged
                if id == current_system.0 {
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
            // Update lights and other child entities by checking their own SystemId,
            // falling back to their Parent's SystemId if available. This avoids
            // inserting components into children (which can panic if they are
            // despawned before command application).
            for (mut vis, sys_id, parent) in light_query.iter_mut() {
                let id = if let Some(s) = sys_id {
                    s.0
                } else if let Some(parent) = parent {
                    parent_sys_query
                        .get(parent.parent())
                        .map(|s| s.0)
                        .unwrap_or(0)
                } else {
                    0
                };

                if id == current_system.0 {
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
        }
        ViewMode::Starmap => {
            // Hide everything in Starmap mode (except Icons)
            for (mut vis, _) in body_query.iter_mut() {
                *vis = Visibility::Hidden;
            }
            for (mut vis, _, _) in light_query.iter_mut() {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Update starmap icon positions relative to the floating origin
fn update_starmap_coordinates(
    floating_origin: Res<FloatingOrigin>,
    mut query: Query<(&mut Transform, &StarSystemIcon)>,
) {
    if !floating_origin.is_changed() {
        // Optimization: usually only update if origin changes,
        // BUT finding if new icons spawned is hard.
        // For 50 items, running every frame is cheap.
    }

    let origin = floating_origin.position;

    // Starmap scale: We render icons at 1 Unit = 1 AU relative to origin.
    // This makes the starmap "miniature" compared to the System View (1500 Units = 1 AU).
    // This allows the camera to see the starmap within reasonable Z-range.

    for (mut transform, icon) in query.iter_mut() {
        // Calculate position in AU relative to origin
        let relative_au = icon.position - origin;

        // Map to Bevy units: 1 AU = 1.0 Unit (Starmap Scale)
        transform.translation = Vec3::new(
            relative_au.x as f32,
            relative_au.y as f32,
            relative_au.z as f32,
        );
    }
}

/// Show/hide starmap icons based on current `ViewMode`.
///
/// When inside a non-Sol system that already has high-resolution visual
/// entities (spawned by `spawn_system_bodies`), the low-poly starmap icon is
/// hidden so the proper star mesh is the only one rendered.
fn update_starmap_visibility(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    active_menu: Res<ActiveMenu>,
    mut icon_query: Query<(&mut Visibility, &StarSystemIcon)>,
    // Exclude Ring entities — same reason as spawn_system_bodies.
    bodies_with_visuals: Query<&SystemId, (With<CelestialBody>, With<Mesh3d>, Without<Ring>)>,
    // React on the frame after deferred mesh-insertion commands are flushed.
    newly_added_meshes: Query<Entity, (With<CelestialBody>, Added<Mesh3d>)>,
) {
    if !view_mode.is_changed()
        && !current_system.is_changed()
        && !active_menu.is_changed()
        && newly_added_meshes.is_empty()
    {
        return;
    }

    // Hide everything when in Research view
    if active_menu.current == GameMenu::Research {
        for (mut vis, _) in icon_query.iter_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    match *view_mode {
        ViewMode::System => {
            // Check whether the current system has proper visual entities
            let system_has_visuals = bodies_with_visuals
                .iter()
                .any(|id| id.0 == current_system.0);

            for (mut vis, icon) in icon_query.iter_mut() {
                if icon.id == current_system.0 && icon.id != 0 && !system_has_visuals {
                    // No real visuals yet — show icon as a placeholder
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
        }
        ViewMode::Starmap => {
            for (mut vis, _) in icon_query.iter_mut() {
                *vis = Visibility::Inherited;
            }
        }
    };
}

/// Scale the starmap icon so it remains a comfortable visual size regardless of
/// how far the camera is zoomed out.
fn update_starmap_icon_scale(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    mut icon_query: Query<(&mut Transform, &StarSystemIcon)>,
) {
    let Ok(orbit) = camera_query.single() else {
        return;
    };

    // Scale icons with sub-linear (square root) growth to maintain good proportions
    // at all zoom levels. Icons grow more slowly as you zoom out, preventing overlap.
    // At 100k units: ~707 radius, at 1M units: ~2236 radius, at 2M units: ~3162 radius
    let base_size = 800.0;
    let reference_zoom = 100_000.0;
    let icon_radius = base_size * (orbit.radius / reference_zoom).sqrt();
    let scale = Vec3::splat(icon_radius);

    match *view_mode {
        ViewMode::Starmap => {
            for (mut transform, _) in icon_query.iter_mut() {
                transform.scale = scale;
            }
        }
        ViewMode::System => {
            // Only update the active system icon so it looks good as a placeholder
            // But skip Sol, as it's hidden anyway
            if current_system.0 != 0 {
                for (mut transform, icon) in icon_query.iter_mut() {
                    if icon.id == current_system.0 {
                        transform.scale = scale;
                    }
                }
            }
        }
    }
}

/// Detect hover over starmap icons
fn handle_starmap_hover(
    view_mode: Res<ViewMode>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    icon_query: Query<(Entity, &GlobalTransform, &StarSystemIcon)>,
    mut commands: Commands,
    hovered_query: Query<Entity, With<HoveredStarSystem>>,
    mut egui_contexts: bevy_egui::EguiContexts,
    panel_bounds: Res<EguiPanelBounds>,
) {
    // Only active in starmap view
    if *view_mode != ViewMode::Starmap {
        // Clear hover markers if not in starmap
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<HoveredStarSystem>();
        }
        return;
    }

    // Don't process if egui is using the mouse / pointer is over a UI panel
    let ctx = match egui_contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.map_or(false, |p| !available.contains(p))
        } else {
            false
        };
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() || over_panel {
            // Clear hover when over UI
            for entity in hovered_query.iter() {
                commands.entity(entity).remove::<HoveredStarSystem>();
            }
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get cursor position
    let Some(cursor_position) = window.cursor_position() else {
        // No cursor, clear hover
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<HoveredStarSystem>();
        }
        return;
    };

    // Convert screen position to ray
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Find the closest star system icon to the ray
    let mut closest_icon: Option<Entity> = None;
    let mut closest_distance = f32::MAX;

    for (entity, transform, _icon) in icon_query.iter() {
        let icon_pos = transform.translation();

        // Calculate distance from ray to icon center
        let to_icon = icon_pos - ray.origin;
        let projection = to_icon.dot(*ray.direction);

        if projection < 0.0 {
            continue; // Icon is behind camera
        }

        let closest_point = ray.origin + *ray.direction * projection;
        let distance_to_ray = (icon_pos - closest_point).length();

        // Icon scale determines its hoverable radius
        let icon_scale = transform.compute_transform().scale.x;
        let hover_radius = icon_scale * 2.0; // Larger for hover than click

        if distance_to_ray < hover_radius {
            let distance_from_camera = (icon_pos - ray.origin).length();

            if distance_from_camera < closest_distance {
                closest_icon = Some(entity);
                closest_distance = distance_from_camera;
            }
        }
    }

    // Update hover state
    // Remove hover from all entities first
    for entity in hovered_query.iter() {
        commands.entity(entity).remove::<HoveredStarSystem>();
    }

    // Add hover to the closest icon if found
    if let Some(entity) = closest_icon {
        commands.entity(entity).insert(HoveredStarSystem);
    }
}

#[derive(Default)]
struct StarmapSelectionState {
    last_click_time: f64,
    last_clicked_entity: Option<Entity>,
}

/// Handle single-click selection and double-click zoom of star system icons.
/// Single-clicking selects/highlights the system.
/// Double-clicking anchors the camera and zooms into the system.
fn handle_starmap_selection(
    view_mode: Res<ViewMode>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    icon_query: Query<(Entity, &GlobalTransform, &StarSystemIcon)>,
    mut commands: Commands,
    selected_query: Query<Entity, With<SelectedStarSystem>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    mut orbit_camera_query: Query<&mut OrbitCamera, With<GameCamera>>,
    time: Res<Time>,
    mut selection_state: Local<StarmapSelectionState>,
    mut egui_contexts: bevy_egui::EguiContexts,
    panel_bounds: Res<EguiPanelBounds>,
) {
    // Only active in starmap view
    if *view_mode != ViewMode::Starmap {
        return;
    }

    // Set cursor to default arrow to prevent text selection cursor
    if let Ok(ctx) = egui_contexts.ctx_mut() {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.map_or(false, |p| !available.contains(p))
        } else {
            false
        };
        if !ctx.is_pointer_over_area() && !over_panel {
            ctx.output_mut(|o| o.cursor_icon = egui::CursorIcon::Default);
        }
    }

    // Only process on mouse click
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    // Don't process if egui is using the mouse / pointer is over a UI panel
    let Ok(ctx) = egui_contexts.ctx_mut() else {
        return;
    };
    {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.map_or(false, |p| !available.contains(p))
        } else {
            false
        };
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() || over_panel {
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get cursor position
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    // Convert screen position to ray
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Find the closest star system icon to the ray
    let mut closest_icon: Option<(Entity, f32, String)> = None;

    for (entity, transform, icon) in icon_query.iter() {
        let icon_pos = transform.translation();

        // Calculate distance from ray to icon center
        let to_icon = icon_pos - ray.origin;
        let projection = to_icon.dot(*ray.direction);

        if projection < 0.0 {
            continue; // Icon is behind camera
        }

        let closest_point = ray.origin + *ray.direction * projection;
        let distance_to_ray = (icon_pos - closest_point).length();

        // Icon scale determines its clickable radius
        let icon_scale = transform.compute_transform().scale.x;
        let click_radius = icon_scale * 1.5; // 50% larger for easier clicking

        if distance_to_ray < click_radius {
            let distance_from_camera = (icon_pos - ray.origin).length();

            if closest_icon.is_none() || distance_from_camera < closest_icon.as_ref().unwrap().1 {
                closest_icon = Some((entity, distance_from_camera, icon.name.clone()));
            }
        }
    }

    // If we found an icon, handle selection and double-click
    if let Some((entity, _, name)) = closest_icon {
        let current_time = time.elapsed_secs_f64();
        let is_double_click = selection_state.last_clicked_entity == Some(entity)
            && (current_time - selection_state.last_click_time) < 0.3; // 300ms window

        selection_state.last_clicked_entity = Some(entity);
        selection_state.last_click_time = current_time;

        if is_double_click {
            // Double-click: Zoom into the system
            info!("Double-clicked star system: {} - zooming in", name);

            // Anchor camera to this system icon's position
            if let Ok(mut anchor) = anchor_query.single_mut() {
                anchor.0 = Some(entity);
                info!("Camera anchored to {}", name);
            }

            // Set zoom to medium level (150k units for comfortable view)
            if let Ok(mut orbit_camera) = orbit_camera_query.single_mut() {
                orbit_camera.radius = 150_000.0;
            }
        } else {
            // Single-click: Just select/highlight the system
            info!("Selected star system: {}", name);

            // Clear previous selection
            for selected_entity in selected_query.iter() {
                commands
                    .entity(selected_entity)
                    .remove::<SelectedStarSystem>();
            }

            // Mark this system as selected
            commands.entity(entity).insert(SelectedStarSystem);
        }
    }
}

/// Handle transition from Starmap to System view.
/// This updates the floating origin and current system if we were anchored to a star.
/// Also clears any celestial body selections from the previous system.
fn handle_system_transition(
    view_mode: Res<ViewMode>,
    mut current_system: ResMut<CurrentStarSystem>,
    mut floating_origin: ResMut<FloatingOrigin>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    mut camera_query: Query<&mut OrbitCamera, With<GameCamera>>,
    icon_query: Query<&StarSystemIcon>,
    star_body_query: Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&SystemId>)>,
    selected_query: Query<Entity, With<SelectedStarSystem>>,
    body_selected_query: Query<Entity, With<crate::astronomy::components::Selected>>,
    mut commands: Commands,
) {
    if !view_mode.is_changed() || *view_mode != ViewMode::System {
        return;
    }

    // Identify which star we are anchored to
    if let Ok(mut anchor) = anchor_query.single_mut() {
        if let Some(anchored_entity) = anchor.0 {
            // Check if the anchored entity is a star system icon
            if let Ok(icon) = icon_query.get(anchored_entity) {
                // We are zooming into this system!

                // Update Current System
                current_system.0 = icon.id;

                // Update Floating Origin to center on this star
                floating_origin.position = icon.position;

                info!(
                    "Transitioned to system: {} (Origin: {:?})",
                    icon.name, floating_origin.position
                );

                let focus_star = star_body_query
                    .iter()
                    .filter(|(_, body, _, sys_id)| {
                        body.body_type == BodyType::Star
                            && sys_id.map(|s| s.0).unwrap_or(0) == icon.id
                    })
                    .max_by(|(_, body_a, _, _), (_, body_b, _, _)| {
                        body_a
                            .mass
                            .partial_cmp(&body_b.mass)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(entity, _body, coords, _)| (entity, coords.position));

                if let Ok(mut orbit_camera) = camera_query.single_mut() {
                    orbit_camera.pan_offset = Vec3::ZERO;
                    if let Some((focus_entity, focus_pos)) = focus_star {
                        let local_focus = (focus_pos - floating_origin.position) * SCALING_FACTOR;
                        orbit_camera.target_center = Vec3::new(
                            local_focus.x as f32,
                            local_focus.y as f32,
                            local_focus.z as f32,
                        );
                        anchor.0 = Some(focus_entity);
                    } else {
                        anchor.0 = None;
                        orbit_camera.target_center = Vec3::ZERO;
                    }
                }
            }
        }
    }

    // Clear all celestial body selections from the previous system
    // so bodies from the old system don't get forced visible by
    // update_body_lod_visibility
    for entity in body_selected_query.iter() {
        commands
            .entity(entity)
            .remove::<crate::astronomy::components::Selected>();
    }

    // Clear all starmap selections (visual rings etc)
    for entity in selected_query.iter() {
        commands.entity(entity).remove::<SelectedStarSystem>();
    }
}

/// Handle transition from System to Starmap view.
/// Clears the body anchor so the previous body's moons/LP markers don't stay visible.
fn handle_starmap_transition(
    view_mode: Res<ViewMode>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
) {
    if !view_mode.is_changed() || *view_mode != ViewMode::Starmap {
        return;
    }

    // Clear the body anchor when leaving system view
    // This ensures moons and Lagrange points from the old system are hidden
    if let Ok(mut anchor) = anchor_query.single_mut() {
        if anchor.0.is_some() {
            info!("Clearing body anchor on starmap transition");
            anchor.0 = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_seed(name: &str) -> u32 {
        name.bytes()
            .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32))
    }

    // ── classify_exoplanet ───────────────────────────────────────────────────

    #[test]
    fn test_classify_lava() {
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 600.0, 0, false, false),
            "lava"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 501.0, 0, false, false),
            "lava"
        );
        // exactly 500.0 is scorched (condition is > 500.0 for lava)
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 500.0, 0, false, false),
            "scorched"
        );
    }

    #[test]
    fn test_classify_scorched() {
        // Extreme-heat worlds (200–500 °C) — Venus-like greenhouse infernos
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 465.0, 0, false, false),
            "scorched"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 300.0, 0, false, false),
            "scorched"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 201.0, 0, false, false),
            "scorched"
        );
        // exactly 200.0 is desert (condition is > 200.0 for scorched)
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 200.0, 0, false, false),
            "desert"
        );
    }

    #[test]
    fn test_classify_desert() {
        // Hot worlds (60–200 °C)
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 200.0, 0, false, false),
            "desert"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 60.1, 0, false, false),
            "desert"
        );
        // exactly 60.0 sits at the hot habitable band and should be savannah
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 60.0, 0, false, false),
            "savannah"
        );
    }

    #[test]
    fn test_classify_habitable_zone() {
        // Temperatures inside habitable band (-20.0..=60.0).
        // Four-category distribution controlled by seed % 4.
        // Seed 0 → 0 % 4 == 0 → "jungle"
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 0, false, false),
            "jungle"
        );
        // Seed 1 → 1 % 4 == 1 → "ocean"
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 1, false, false),
            "ocean"
        );
        // Seed 2 → 2 % 4 == 2 → "temperate"
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 2, false, false),
            "temperate"
        );
        // Seed 3 → 3 % 4 == 3 → "swamp"
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 3, false, false),
            "swamp"
        );
    }

    #[test]
    fn test_classify_tundra() {
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -50.0, 0, false, false),
            "tundra"
        );
        // -20.0 is the habitable lower bound (inclusive), so just below it is tundra
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -20.1, 0, false, false),
            "tundra"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -99.9, 0, false, false),
            "tundra"
        );
        // exactly -100.0 is tundra (>= -100.0)
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -100.0, 0, false, false),
            "tundra"
        );
        // -10.0 is cold but still in habitable band and should classify as alpine
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -10.0, 0, false, false),
            "alpine"
        );
        // exactly -20.0 lies on the cold edge of the habitable band and
        // is classified as alpine under the new rules.
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -20.0, 0, false, false),
            "alpine"
        );
    }

    #[test]
    fn test_classify_alpine() {
        // Temperatures well inside the habitable window but coldend (< -5°C) are alpine.
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -20.0, 0, false, false),
            "alpine"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -10.0, 0, false, false),
            "alpine"
        );
    }

    #[test]
    fn test_boundary_minus_five() {
        // The boundary temp -5°C should no longer count as alpine; it maps to jungle.
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -5.0, 0, false, false),
            "jungle"
        );
    }

    #[test]
    fn test_classify_savannah() {
        // Very hot worlds above habitable band but below lava, and hot
        // habitable-zone worlds (>45°C) should be labelled "savannah".
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 46.0, 0, false, false),
            "savannah"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 60.0, 0, false, false),
            "savannah"
        );
    }

    #[test]
    fn test_classify_ice() {
        // -100.0 is tundra (>= -100.0); strictly below -100.0 is ice
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -100.1, 0, false, false),
            "ice"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, -250.0, 0, false, false),
            "ice"
        );
    }

    #[test]
    fn test_classify_gas_and_ice_giants_by_mass() {
        // Jupiter-mass (1.9e27 kg) — gas giant regardless of temperature
        assert_eq!(
            classify_exoplanet_with_mass(
                BodyType::GasGiant,
                None,
                -108.0,
                0,
                false,
                false,
                Some(1.9e27)
            ),
            "gas_giant"
        );
        // Saturn-mass (5.7e26 kg) — gas giant
        assert_eq!(
            classify_exoplanet_with_mass(
                BodyType::GasGiant,
                None,
                -133.0,
                0,
                false,
                false,
                Some(5.7e26)
            ),
            "gas_giant"
        );
        // Uranus-mass (8.7e25 kg) — ice giant
        assert_eq!(
            classify_exoplanet_with_mass(
                BodyType::GasGiant,
                None,
                -197.0,
                0,
                false,
                false,
                Some(8.7e25)
            ),
            "ice_giant"
        );
        // Neptune-mass (1.0e26 kg) — ice giant
        assert_eq!(
            classify_exoplanet_with_mass(
                BodyType::GasGiant,
                None,
                -201.0,
                0,
                false,
                false,
                Some(1.0e26)
            ),
            "ice_giant"
        );
        // Mass threshold: > 2e26 is gas giant
        assert_eq!(
            classify_exoplanet_with_mass(
                BodyType::GasGiant,
                None,
                -200.0,
                0,
                false,
                false,
                Some(2.01e26)
            ),
            "gas_giant"
        );
        assert_eq!(
            classify_exoplanet_with_mass(
                BodyType::GasGiant,
                None,
                -200.0,
                0,
                false,
                false,
                Some(2.0e26)
            ),
            "ice_giant"
        );
    }

    #[test]
    fn test_classify_gas_giants_no_mass_fallback() {
        // Without mass, falls back to temperature heuristic
        assert_eq!(
            classify_exoplanet(BodyType::GasGiant, None, -200.0, 0, false, false),
            "ice_giant"
        );
        // strictly below -80.0 is ice_giant; -80.0 itself is gas_giant
        assert_eq!(
            classify_exoplanet(BodyType::GasGiant, None, -80.1, 0, false, false),
            "ice_giant"
        );
        assert_eq!(
            classify_exoplanet(BodyType::GasGiant, None, -80.0, 0, false, false),
            "gas_giant"
        );
        assert_eq!(
            classify_exoplanet(BodyType::GasGiant, None, 100.0, 0, false, false),
            "gas_giant"
        );
    }

    #[test]
    fn test_classify_small_bodies() {
        assert_eq!(
            classify_exoplanet(BodyType::DwarfPlanet, None, -200.0, 0, false, false),
            "dwarf"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Moon, None, 0.0, 0, false, false),
            "moon"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Comet, None, -50.0, 0, false, false),
            "comet"
        );
    }

    #[test]
    fn test_classify_asteroids() {
        assert_eq!(
            classify_exoplanet(
                BodyType::Asteroid,
                Some(AsteroidClass::SType),
                0.0,
                0,
                false,
                false
            ),
            "asteroid_s"
        );
        assert_eq!(
            classify_exoplanet(
                BodyType::Asteroid,
                Some(AsteroidClass::MType),
                0.0,
                0,
                false,
                false
            ),
            "asteroid_s"
        );
        assert_eq!(
            classify_exoplanet(
                BodyType::Asteroid,
                Some(AsteroidClass::CType),
                0.0,
                0,
                false,
                false
            ),
            "asteroid_c"
        );
        assert_eq!(
            classify_exoplanet(
                BodyType::Asteroid,
                Some(AsteroidClass::DType),
                0.0,
                0,
                false,
                false
            ),
            "asteroid_c"
        );
    }

    #[test]
    fn test_classify_ocean_override() {
        // With has_surface_ocean=true and ocean_is_water=true, habitable-zone planets
        // should always classify as "ocean" regardless of seed.
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 0, true, true),
            "ocean"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 2, true, true),
            "ocean"
        );
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 3, true, true),
            "ocean"
        );
        // Non-water oceans should NOT force the "ocean" category
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 15.0, 0, true, false),
            "jungle"
        );
        // Outside habitable zone, ocean flag has no effect
        assert_eq!(
            classify_exoplanet(BodyType::Planet, None, 600.0, 0, true, true),
            "lava"
        );
    }

    // ── PlanetTextureManifest::pick ──────────────────────────────────────────

    #[test]
    fn test_manifest_pick_deterministic() {
        let manifest = PlanetTextureManifest::default_fallback();
        let a = manifest.pick("desert", 5).unwrap().to_string();
        let b = manifest.pick("desert", 5).unwrap().to_string();
        assert_eq!(a, b, "same seed should always return same texture");
    }

    #[test]
    fn test_manifest_pick_wraps_index() {
        let manifest = PlanetTextureManifest::default_fallback();
        // "gas_giant" has 2 textures; seeds 0 and 2 should map to the same one
        let t0 = manifest.pick("gas_giant", 0).unwrap().to_string();
        let t2 = manifest.pick("gas_giant", 2).unwrap().to_string();
        assert_eq!(t0, t2);
    }

    #[test]
    fn test_manifest_pick_missing_category_returns_none() {
        let manifest = PlanetTextureManifest::default_fallback();
        assert!(manifest.pick("nonexistent_category", 0).is_none());
    }

    #[test]
    fn test_manifest_fallback_has_all_expected_categories() {
        let manifest = PlanetTextureManifest::default_fallback();
        for cat in &[
            "barren",
            "desert",
            "temperate",
            "jungle",
            "ocean",
            "tundra",
            "ice",
            "lava",
            "gas_giant",
            "ice_giant",
            "dwarf",
            "moon",
            "asteroid_s",
            "asteroid_c",
            "comet",
        ] {
            assert!(
                manifest.categories.contains_key(*cat),
                "missing category: {}",
                cat
            );
        }
    }

    #[test]
    fn test_manifest_loads_from_ron_file() {
        let result = PlanetTextureManifest::load_from_file("assets/data/planet_textures.ron");
        assert!(
            result.is_ok(),
            "Failed to load planet_textures.ron: {:?}",
            result.err()
        );
        let manifest = result.unwrap();
        assert!(
            manifest.categories.len() >= 14,
            "expected at least 14 categories"
        );
        assert!(manifest.pick("desert", 0).is_some());
        assert!(manifest.pick("jungle", 0).is_some());
        assert!(manifest.pick("lava", 0).is_some());
    }

    // ── category_tint colour values stay within [0, 1] ──────────────────────

    #[test]
    fn test_category_tint_values_in_range() {
        let categories = [
            "lava",
            "desert",
            "temperate",
            "jungle",
            "ocean",
            "tundra",
            "ice",
            "barren",
            "gas_giant",
            "ice_giant",
            "dwarf",
            "moon",
            "asteroid_s",
            "asteroid_c",
            "comet",
        ];
        // Test with extreme r values to catch clamping issues
        for cat in &categories {
            for &r in &[0.0f32, 0.5, 0.999] {
                let (color, roughness, metallic) = category_tint(cat, r, r, r);
                let srgba = color.to_srgba();
                assert!(
                    srgba.red >= 0.0 && srgba.red <= 1.0,
                    "{cat} red out of range"
                );
                assert!(
                    srgba.green >= 0.0 && srgba.green <= 1.0,
                    "{cat} green out of range"
                );
                assert!(
                    srgba.blue >= 0.0 && srgba.blue <= 1.0,
                    "{cat} blue out of range"
                );
                assert!(
                    roughness >= 0.0 && roughness <= 1.0,
                    "{cat} roughness out of range"
                );
                assert!(
                    metallic >= 0.0 && metallic <= 1.0,
                    "{cat} metallic out of range"
                );
            }
        }
    }
}
