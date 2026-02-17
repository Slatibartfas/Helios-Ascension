//! Starmap view module
//!
//! When the camera zooms out past `STARMAP_TRANSITION_THRESHOLD`, the game
//! transitions from the detailed solar-system view to a sector/galaxy-level
//! starmap. In the starmap:
//!
//!  - Individual celestial bodies and orbit paths are hidden.
//!  - Each star system is represented by a single glowing icon/billboard.
//!  - Double-clicking a system icon anchors the camera and allows zoom-in.
//!
//! Currently only the Sol system exists; more systems will be added later.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

use super::camera::{CameraAnchor, GameCamera, OrbitCamera, ViewMode};
use super::solar_system::CelestialBody;
use super::solar_system_data::{BodyType, calculate_visual_radius};
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

// ── Startup ─────────────────────────────────────────────────────────────────

// 1 Light Year in Astronomical Units
const LY_TO_AU: f64 = 63241.077;

// (Moved to src/astronomy/nearby_stars.rs)
// 50 Closest Star Systems to Sol (excluding Sol)
// Coordinates in Light Years (Equatorial J2000 Cartesian)
// NEARBY_STARS definition moved to src/astronomy/nearby_stars.rs

/// Spawn the starmap icon for the Sol system.
/// It starts hidden and becomes visible when `ViewMode::Starmap` is active.
fn setup_starmap(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut system_metadata: ResMut<SystemMetadata>,
) {
    // Initialize Sol's bounding radius
    system_metadata.set_bounding_radius(0, DEFAULT_BOUNDING_RADIUS_AU);

    // A bright glowing sphere representing the star system
    let icon_mesh = meshes.add(Sphere::new(1.0).mesh().uv(16, 8));

    // --- Sol System (ID: 0) ---
    let sol_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.95, 0.7),
        emissive: Color::srgb(6.0, 5.5, 3.5).into(), // Very bright for home system
        unlit: true,
        ..default()
    });

    // The icon is placed at the origin (same as the Sun) and scaled
    // dynamically based on camera distance.
    commands.spawn((
        PbrBundle {
            mesh: icon_mesh.clone(),
            material: sol_material,
            transform: Transform::from_translation(Vec3::ZERO),
            visibility: Visibility::Hidden, // starts hidden; shown in Starmap mode
            ..default()
        },
        StarSystemIcon {
            id: 0,
            name: "Sol System".to_string(),
            position: DVec3::ZERO,
            bounding_radius_au: DEFAULT_BOUNDING_RADIUS_AU,
        },
        SolSystemIcon,
    ));

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
            emissive: Color::srgb(r * 8.0, g * 8.0, b * 8.0).into(),
            unlit: true,
            ..default()
        });

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

        commands.spawn((
            PbrBundle {
                mesh: icon_mesh.clone(),
                material,
                transform: Transform::from_translation(spawn_pos),
                visibility: Visibility::Hidden,
                ..default()
            },
            StarSystemIcon {
                id,
                name: star.name.to_string(),
                position: pos_au,
                bounding_radius_au,
            },
        ));
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
    // Query for bodies that need visual components added
    bodies_without_visuals: Query<
        (Entity, &CelestialBody, &SpaceCoordinates, &SystemId),
        (Without<Handle<Mesh>>, Without<Handle<StandardMaterial>>),
    >,
    // Query to check if system already has visual entities
    bodies_with_visuals: Query<&SystemId, (With<CelestialBody>, With<Handle<Mesh>>)>,
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
    for (entity, body, space_coords, _system_id) in bodies_without_visuals.iter() {
        if _system_id.0 != sys_id {
            continue;
        }

        // Determine visual properties based on body type
        let (color, visual_radius) = match body.body_type {
            BodyType::Star => {
                let color = Color::srgb(1.0, 0.95, 0.8); // Default yellow star
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);
                (color, visual_radius)
            }
            BodyType::Planet | BodyType::DwarfPlanet => {
                let color = Color::srgb(0.5, 0.5, 0.7); // Default blue-grey planet
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);
                (color, visual_radius)
            }
            BodyType::GasGiant => {
                let color = Color::srgb(0.9, 0.8, 0.6); // Tan/beige for gas giant
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);
                (color, visual_radius)
            }
            BodyType::Moon => {
                let color = Color::srgb(0.6, 0.6, 0.6); // Grey moon
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);
                (color, visual_radius)
            }
            BodyType::Asteroid => {
                let color = Color::srgb(0.4, 0.4, 0.3); // Brown-grey asteroid
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);
                (color, visual_radius)
            }
            BodyType::Comet => {
                let color = Color::srgb(0.7, 0.8, 0.9); // Icy blue-white
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);
                (color, visual_radius)
            }
            BodyType::Ring => {
                // Rings should have been created as separate entities, skip for now
                continue;
            }
        };

        // Create mesh
        let mesh = meshes.add(Sphere::new(visual_radius).mesh().uv(32, 16));

        // Create material
        let material = if matches!(body.body_type, BodyType::Star) {
            materials.add(StandardMaterial {
                base_color: color,
                emissive: LinearRgba::from(color).into(),
                unlit: true,
                ..default()
            })
        } else {
            materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.8,
                reflectance: 0.1,
                ..default()
            })
        };

        // Compute the correct initial transform position using floating origin
        let scaled_position =
            (space_coords.position - origin_offset) * SCALING_FACTOR;
        let initial_transform = Transform::from_translation(Vec3::new(
            scaled_position.x as f32,
            scaled_position.y as f32,
            scaled_position.z as f32,
        ));

        // Add visual components to existing entity
        commands.entity(entity).insert((
            mesh,
            material,
            initial_transform,
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ));

        // Add light for stars
        if matches!(body.body_type, BodyType::Star) {
            let intensity = 2.8e11; // Default star intensity
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    PointLightBundle {
                        point_light: PointLight {
                            intensity,
                            range: 2.0e9,
                            shadows_enabled: false,
                            color,
                            ..default()
                        },
                        ..default()
                    },
                    SystemId(sys_id),
                ));
            });
        }

        info!("Added visuals to {} ({:?})", body.name, body.body_type);
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
        (&mut Visibility, Option<&SystemId>, Option<&Parent>),
        (
            With<PointLight>,
            Without<CelestialBody>,
            Without<StarSystemIcon>,
        ),
    >,
    parent_sys_query: Query<&SystemId>,
    newly_spawned_bodies: Query<Entity, Added<CelestialBody>>,
    newly_added_meshes: Query<Entity, (With<CelestialBody>, Added<Handle<Mesh>>)>,
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
                    parent_sys_query.get(parent.get()).map(|s| s.0).unwrap_or(0)
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
fn update_starmap_visibility(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    active_menu: Res<ActiveMenu>,
    mut icon_query: Query<(&mut Visibility, &StarSystemIcon)>,
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
            for (mut vis, icon) in icon_query.iter_mut() {
                // For Sol (0), we have a real model, so hide the icon.
                // For others, show the icon as a placeholder star until we implement real loading.
                if icon.id == current_system.0 && icon.id != 0 {
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
    let Ok(orbit) = camera_query.get_single() else {
        return;
    };

    // Scale icons with linear growth plus substantial base size to ensure visibility
    // at all zoom levels. Base size prevents icons from becoming too small.
    let base_size = 500.0;
    let icon_radius = base_size + (orbit.radius * 0.012);
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
) {
    // Only active in starmap view
    if *view_mode != ViewMode::Starmap {
        // Clear hover markers if not in starmap
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<HoveredStarSystem>();
        }
        return;
    }

    // Don't process if egui is using the mouse
    let ctx = match egui_contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };
    if ctx.is_pointer_over_area() || ctx.wants_pointer_input() {
        // Clear hover when over UI
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<HoveredStarSystem>();
        }
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.get_single() else {
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
    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
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

/// Handle double-click selection of star system icons in starmap view.
/// Double-clicking anchors the camera to the system's position.
fn handle_starmap_selection(
    view_mode: Res<ViewMode>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    icon_query: Query<(Entity, &GlobalTransform, &StarSystemIcon)>,
    mut commands: Commands,
    selected_query: Query<Entity, With<SelectedStarSystem>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    time: Res<Time>,
    mut selection_state: Local<StarmapSelectionState>,
    mut egui_contexts: bevy_egui::EguiContexts,
) {
    // Only active in starmap view
    if *view_mode != ViewMode::Starmap {
        return;
    }

    // Only process on mouse click
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    // Don't process if egui is using the mouse
    let Some(ctx) = egui_contexts.try_ctx_mut() else {
        return;
    };
    if ctx.is_pointer_over_area() || ctx.wants_pointer_input() || ctx.is_using_pointer() {
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    // Get cursor position
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    // Convert screen position to ray
    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
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

    // If we found an icon, check for double-click
    if let Some((entity, _, name)) = closest_icon {
        let current_time = time.elapsed_seconds_f64();
        let is_double_click = selection_state.last_clicked_entity == Some(entity)
            && (current_time - selection_state.last_click_time) < 0.3; // 300ms window

        selection_state.last_clicked_entity = Some(entity);
        selection_state.last_click_time = current_time;

        if is_double_click {
            info!("Double-clicked star system: {}", name);

            // Clear previous selection
            for selected_entity in selected_query.iter() {
                commands
                    .entity(selected_entity)
                    .remove::<SelectedStarSystem>();
            }

            // Mark this system as selected/anchored
            commands.entity(entity).insert(SelectedStarSystem);

            // Anchor camera to this system icon's position
            // Note: We anchor to the entity itself so the camera follows it
            if let Ok(mut anchor) = anchor_query.get_single_mut() {
                anchor.0 = Some(entity);
                info!("Camera anchored to {}", name);
            }
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
    if let Ok(mut anchor) = anchor_query.get_single_mut() {
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
                if let Ok(mut orbit_camera) = camera_query.get_single_mut() {
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
