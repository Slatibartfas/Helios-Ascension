use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::astronomy::components::CurrentStarSystem;
use crate::astronomy::SCALING_FACTOR;
use crate::game_state::{ActiveMenu, GameMenu};
use crate::plugins::starmap::SystemMetadata;

/// Base zoom threshold multiplier. The actual threshold is calculated as
/// `bounding_radius_au * SCALING_FACTOR * THRESHOLD_MULTIPLIER`.
/// This provides comfortable zoom distances without requiring excessive scrolling.
/// Value of 0.6 gives Sol (~400 AU) a threshold of ~360k units, close to the
/// original fixed threshold of 350k that worked well.
pub const STARMAP_THRESHOLD_MULTIPLIER: f32 = 0.6;

/// Minimum zoom threshold in game units to ensure reasonable behavior for very small systems.
pub const MIN_STARMAP_THRESHOLD: f32 = 50_000.0;

/// The active view mode, driven by camera zoom level.
///
/// - `System` — normal solar-system view with orbits, planets, moons.
/// - `Starmap` — zoomed-out galaxy/sector view showing star systems as icons.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    System,
    Starmap,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewMode>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    orbit_camera_controls
                        // Run AFTER egui has processed input to respect UI interaction
                        .after(bevy_egui::EguiSet::ProcessInput),
                    update_camera_transform,
                    update_view_mode,
                ),
            );
    }
}

#[derive(Component)]
pub struct GameCamera;

#[derive(Component)]
pub struct CameraAnchor(pub Option<Entity>);

#[derive(Component)]
pub struct OrbitCamera {
    pub radius: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub min_radius: f32,
    pub max_radius: f32,
    pub zoom_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub target_center: Vec3,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            radius: 2000.0,
            pitch: 0.5,
            yaw: 0.0,
            min_radius: 5.0,
            max_radius: 2_000_000.0, // Increased to exceed max threshold (Sol: 400*1500*2.5 = 1.5M)
            zoom_sensitivity: 100.0,
            rotate_sensitivity: 0.003,
            target_center: Vec3::ZERO,
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 1000.0, 2000.0).looking_at(Vec3::ZERO, Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                far: 3_000_000.0, // Increased to comfortably render at max camera distance
                ..default()
            }),
            ..default()
        },
        GameCamera,
        CameraAnchor(None),
        OrbitCamera::default(),
    ));
}

fn orbit_camera_controls(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion_events: EventReader<MouseMotion>,
    mut scroll_events: EventReader<MouseWheel>,
    mut query: Query<&mut OrbitCamera>,
) {
    let mut camera = query.single_mut();

    // Block camera control when in full-screen UI modes (Research)
    if active_menu.current == GameMenu::Research {
        motion_events.clear();
        scroll_events.clear();
        return;
    }

    // Check if Egui wants the input (e.g. mouse over a window)
    // Use try_ctx_mut() to avoid panicking if the window is closed/uninitialized
    if let Some(ctx) = contexts.try_ctx_mut() {
        // Robust check for Egui interaction:
        // 1. is_pointer_over_area() - covers windows/panels
        // 2. wants_pointer_input() - covers active interaction (scrolling, clicking)
        // 3. is_using_pointer() - covers dragging
        if ctx.is_pointer_over_area() || ctx.wants_pointer_input() || ctx.is_using_pointer() {
            motion_events.clear();
            scroll_events.clear();
            return;
        }
    }

    // Mouse rotation when right button is held
    if mouse.pressed(MouseButton::Right) {
        for event in motion_events.read() {
            camera.yaw -= event.delta.x * camera.rotate_sensitivity;
            camera.pitch -= event.delta.y * camera.rotate_sensitivity;

            // Clamp pitch to avoid gimbal lock or going under
            camera.pitch = camera.pitch.clamp(-1.5, 1.5);
        }
    } else {
        motion_events.clear();
    }

    // Zoom
    for event in scroll_events.read() {
        let zoom_amount = event.y * camera.zoom_sensitivity * (camera.radius / 1000.0).max(0.1);
        camera.radius -= zoom_amount;
        camera.radius = camera.radius.clamp(camera.min_radius, camera.max_radius);
    }
}

fn update_camera_transform(
    mut camera_query: Query<(&mut Transform, &mut OrbitCamera, &CameraAnchor)>,
    target_query: Query<&GlobalTransform, Without<GameCamera>>,
) {
    let (mut transform, mut orbit, anchor) = camera_query.single_mut();

    // Update target center if anchored
    if let Some(entity) = anchor.0 {
        if let Ok(target_transform) = target_query.get(entity) {
            orbit.target_center = target_transform.translation();
        }
    }

    // Calculate camera position
    let rot =
        Quat::from_axis_angle(Vec3::Y, orbit.yaw) * Quat::from_axis_angle(Vec3::X, orbit.pitch);
    let offset = rot * Vec3::Z * orbit.radius;
    let position = orbit.target_center + offset;

    transform.translation = position;
    transform.look_at(orbit.target_center, Vec3::Y);
}

/// Updates `ViewMode` based on camera zoom radius, with hysteresis to avoid
/// flickering at the boundary. The threshold is dynamically calculated based on
/// the current system's bounding radius to ensure appropriate zoom levels for
/// systems of different sizes.
fn update_view_mode(
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    current_system: Res<CurrentStarSystem>,
    system_metadata: Res<SystemMetadata>,
    mut view_mode: ResMut<ViewMode>,
) {
    let Ok(orbit) = camera_query.get_single() else {
        return;
    };

    // Get the current system's bounding radius from metadata
    let bounding_radius_au = system_metadata.get_bounding_radius(current_system.0);

    // Convert bounding radius to game units and apply multiplier
    // SCALING_FACTOR = 1500.0 (1 AU = 1500 game units)
    let base_threshold =
        (bounding_radius_au * SCALING_FACTOR as f64 * STARMAP_THRESHOLD_MULTIPLIER as f64) as f32;
    let enter_starmap = base_threshold.max(MIN_STARMAP_THRESHOLD);

    // Hysteresis: require crossing past the threshold by 15% in either direction
    let exit_starmap = enter_starmap * 0.85;

    let new_mode = match *view_mode {
        ViewMode::System if orbit.radius > enter_starmap => ViewMode::Starmap,
        ViewMode::Starmap if orbit.radius < exit_starmap => ViewMode::System,
        other => other,
    };

    if new_mode != *view_mode {
        info!(
            "View mode changed: {:?} → {:?} (radius: {:.0}, threshold: {:.0}, system size: {:.1} AU)",
            *view_mode, new_mode, orbit.radius, enter_starmap, bounding_radius_au
        );
        *view_mode = new_mode;
    }
}
