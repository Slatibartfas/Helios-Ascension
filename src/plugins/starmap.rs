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
use bevy_egui::EguiPrimaryContextPass;
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use std::collections::HashMap;

use super::camera::{CameraAnchor, EguiPanelBounds, GameCamera, OrbitCamera, ViewMode};
use super::solar_system::{
    Billboard, CelestialBody, StarDiffraction, StarDiffractionMaterial, StarGlare,
    StarGlowMaterial, StarSurfaceMaterial,
};
use super::solar_system_data::BodyType;
use crate::astronomy::components::{
    CurrentStarSystem, FloatingOrigin, SpaceCoordinates, SystemId,
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

/// Plugin that manages the starmap view layer.
pub struct StarmapPlugin;

impl Plugin for StarmapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentStarSystem>()
            .init_resource::<FloatingOrigin>()
            .init_resource::<SystemMetadata>()
            .add_systems(Startup, setup_starmap)
            .add_systems(
                Update,
                (
                    tag_sol_bodies,
                    handle_system_transition,
                    spawn_system_bodies.after(handle_system_transition),
                    toggle_system_view_entities
                        .after(handle_system_transition)
                        .after(spawn_system_bodies),
                    update_starmap_visibility.after(handle_system_transition),
                    update_starmap_icon_scale,
                    update_starmap_coordinates,
                    twinkle_starmap_icons,
                ),
            )
            // Starmap hover/selection systems use EguiContexts — must run in EguiPrimaryContextPass
            .add_systems(
                EguiPrimaryContextPass,
                (
                    handle_starmap_hover,
                    handle_starmap_selection,
                ),
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

/// Per-star twinkling state stored on the glow billboard child entity.
/// Drives multi-frequency brightness oscillation for a natural scintillation effect.
#[derive(Component)]
struct StarTwinkle {
    /// Randomised per-star phase offset (radians) so stars don't pulse in unison.
    phase: f32,
    /// Base oscillation speed in radians/second.
    speed: f32,
    /// Unmodulated core colour (what the glow shader receives when flicker = 1.0).
    base_core: Vec4,
    /// Unmodulated halo colour.
    base_halo: Vec4,
}

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
        'B' => (12.0,  7.0, 12.0), // Blue-white
        'A' => ( 7.0,  4.5, 10.0), // White
        'F' => ( 4.5,  3.0,  9.0), // Yellow-white
        'G' => ( 3.5,  2.5,  8.0), // Sol-like
        'K' => ( 3.0,  2.2,  7.5), // Orange
        'M' => ( 2.2,  1.8,  7.0), // Red
        'L' => ( 1.5,  1.0,  6.0), // Brown dwarf
        _   => ( 1.0,  0.7,  5.0), // T, Y, unknown — cold brown dwarfs
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
        emissive: LinearRgba::new(5.0, 4.8, 3.0, 1.0), // Bright enough to blend into glow seamlessly
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
                StarTwinkle {
                    phase: 0.0,
                    speed: 1.1,
                    base_core: sol_core,
                    base_halo: sol_halo,
                },
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
            emissive: LinearRgba::new(r * 6.0, g * 6.0, b * 6.0, 1.0),
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

        // Spread twinkle phases across stars using the index for variety
        let twinkle_phase = (i as f32 * 2.3999_f32) % std::f32::consts::TAU;
        // Vary speed slightly per star so they don't all pulse in unison
        let twinkle_speed = 0.8 + (i as f32 * 0.137_f32) % 0.7;

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
                    StarTwinkle {
                        phase: twinkle_phase,
                        speed: twinkle_speed,
                        base_core: core_col,
                        base_halo: halo_col,
                    },
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

/// Adds visual components (meshes, materials, lights) to existing data-only entities
/// when visiting a non-Sol system for the first time.
fn spawn_system_bodies(
    mut commands: Commands,
    current_system: Res<CurrentStarSystem>,
    floating_origin: Res<FloatingOrigin>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut materials_glow: ResMut<Assets<StarGlowMaterial>>,
    mut materials_surface: ResMut<Assets<StarSurfaceMaterial>>,
    mut materials_diffraction: ResMut<Assets<StarDiffractionMaterial>>,
    // Query for bodies that need visual components added
    bodies_without_visuals: Query<
        (
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            &SystemId,
            Option<&crate::astronomy::StellarProperties>,
        ),
        (Without<Mesh3d>, Without<MeshMaterial3d<StandardMaterial>>),
    >,
    // Query to check if system already has visual entities
    bodies_with_visuals: Query<&SystemId, (With<CelestialBody>, With<Mesh3d>)>,
) {
    if !current_system.is_changed() {
        return;
    }

    let sys_id = current_system.0;
    if sys_id == 0 {
        return;
    } // Sol is handled by solar_system.rs

    // Check if this system already has visual entities
    if bodies_with_visuals.iter().any(|id| id.0 == sys_id) {
        // Visuals already added, nothing to do
        return;
    }

    info!("Adding visual components to system {}", sys_id);

    let origin_offset = floating_origin.position;

    // Find all data-only entities for this system and add visual components
    for (entity, body, space_coords, _system_id, stellar_props) in bodies_without_visuals.iter() {
        if _system_id.0 != sys_id {
            continue;
        }

        // Use the pre-computed visual_radius from CelestialBody (already scaled
        // for system compactness by system_populator) instead of recalculating.
        let visual_radius = body.visual_radius;

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
        let scaled_position =
            (space_coords.position - origin_offset) * SCALING_FACTOR;
        let p_vec = Vec3::new(
            scaled_position.x as f32,
            scaled_position.y as f32,
            scaled_position.z as f32,
        );
        let initial_transform = Transform::from_translation(p_vec);

        if matches!(body.body_type, BodyType::Star) {
            // Stars use StarSurfaceMaterial (limb darkening) + corona + diffraction billboards,
            // matching how the Sol star is rendered in setup_solar_system.
            let linear = color.to_linear();
            let (cr, cg, cb) = (linear.red, linear.green, linear.blue);

            // Center colour: hot HDR white derived from spectral colour (triggers bloom)
            let center_col = Vec4::new(cr * 90.0, cg * 90.0, cb * 90.0, 1.0);
            // Limb colour: cooler shift — red is retained, green/blue sharply attenuated
            let limb_col   = Vec4::new(cr * 55.0, cg * 28.0, cb * 8.0, 1.0);

            commands.entity(entity).insert((
                Mesh3d(mesh),
                MeshMaterial3d(materials_surface.add(StarSurfaceMaterial {
                    color_center: center_col,
                    color_limb:   limb_col,
                    star_texture:  None, // No texture for procedural stars
                })),
                initial_transform,
            ));

            // Add light and glow as children
            let intensity = 2.8e11;

            // Corona at 5× visual_radius keeps the glow tight enough to avoid
            // swallowing close-in planets (star sphere capped at ~15% of inner orbit).
            let corona_size = visual_radius * 5.0;
            let core_col    = Vec4::new(5.0, 5.0, 5.0, 1.0);
            let halo_col    = Vec4::new(cr, cg, cb, 1.0) * 4.0;
            // Diffraction: warm white derived from spectral color
            let diff_col    = Vec4::new(cr * 4.5, cg * 4.2, cb * 3.5, 1.0);

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

                // Diffraction spike billboard (behind corona in depth order)
                parent.spawn((
                    Mesh3d(meshes.add(Rectangle::new(
                        visual_radius * 10.0,
                        visual_radius * 10.0,
                    ))),
                    MeshMaterial3d(materials_diffraction.add(StarDiffractionMaterial {
                        color: Vec4::ZERO, // LOD system drives it in
                    })),
                    Transform::from_translation(Vec3::Z * 0.05),
                    Billboard,
                    StarDiffraction { base_color: diff_col },
                    SystemId(sys_id),
                ));

                // Corona / halo billboard
                parent.spawn((
                    Mesh3d(meshes.add(Rectangle::new(corona_size, corona_size))),
                    MeshMaterial3d(materials_glow.add(StarGlowMaterial {
                        color_core: core_col,
                        color_halo: halo_col,
                        time_phase: 0.0,
                    })),
                    Transform::from_translation(Vec3::Z * 0.1),
                    StarGlare {
                        base_core_color: core_col,
                        base_halo_color: halo_col,
                    },
                    Billboard,
                    SystemId(sys_id),
                ));
            });
        } else {
            let material = materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.8,
                reflectance: 0.1,
                ..default()
            });

            commands.entity(entity).insert((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                initial_transform,
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
                    parent_sys_query.get(parent.parent()).map(|s| s.0).unwrap_or(0)
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
    bodies_with_visuals: Query<&SystemId, (With<CelestialBody>, With<Mesh3d>)>,
) {
    if !view_mode.is_changed() && !current_system.is_changed() && !active_menu.is_changed() {
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

/// Animate each star's glow billboard with a multi-frequency brightness flicker,
/// simulating atmospheric scintillation (twinkling). Each star has a unique
/// phase and speed so they don't pulse in unison.
fn twinkle_starmap_icons(
    view_mode: Res<ViewMode>,
    time: Res<Time>,
    mut twinkle_query: Query<(&StarTwinkle, &MeshMaterial3d<StarGlowMaterial>)>,
    mut glow_materials: ResMut<Assets<StarGlowMaterial>>,
) {
    if *view_mode != ViewMode::Starmap {
        return;
    }

    let t = time.elapsed_secs();

    for (twinkle, mat_handle) in twinkle_query.iter_mut() {
        let Some(mat) = glow_materials.get_mut(&mat_handle.0) else {
            continue;
        };

        // Combine three incommensurate frequencies for organic-looking twinkling.
        // Primary oscillation: gentle swell
        // Secondary: faster shimmer
        // Tertiary: rapid micro-flicker
        let f1 = (t * twinkle.speed + twinkle.phase).sin();
        let f2 = (t * twinkle.speed * 2.37 + twinkle.phase * 1.61).sin();
        let f3 = (t * twinkle.speed * 5.13 + twinkle.phase * 0.91).sin();
        // Weighted blend: flicker stays in ~[0.65, 1.20] range for visible twinkling
        let flicker = 0.90 + 0.18 * f1 + 0.08 * f2 + 0.04 * f3;
        // Halo modulates less than core
        let halo_flicker = 0.94 + 0.08 * f1 + 0.03 * f2;

        mat.color_core = twinkle.base_core * flicker;
        mat.color_halo = twinkle.base_halo * halo_flicker;
    }
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

                // Clear the anchor so the camera is free to move in the new system
                // But wait! If we clear the anchor, the camera target_center stays where it was (at the icon).
                // Since the Floating Origin shifted, the Icon moved to (0,0,0).
                // So target_center should be (0,0,0).
                // And OrbitCamera will naturally look at (0,0,0).
                anchor.0 = None;
                
                // Reset OrbitCamera target center to (0,0,0) explicitly
                // This ensures we are looking at the star (which is at local 0,0,0)
                // disregarding any previous starmap-space offset
                if let Ok(mut orbit_camera) = camera_query.single_mut() {
                    orbit_camera.target_center = Vec3::ZERO;
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
