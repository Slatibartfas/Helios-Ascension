use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

/// Zoom threshold (in game units) at which the camera transitions from
/// system view to starmap view. ~233 AU worth of game units (well past Kuiper Belt).
pub const STARMAP_TRANSITION_THRESHOLD: f32 = 350_000.0;

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
            .add_systems(Update, (
                orbit_camera_controls,
                update_camera_transform,
                update_view_mode,
            ));
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
            max_radius: 500_000.0,
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
                far: 1_500_000.0,
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
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion_events: EventReader<MouseMotion>,
    mut scroll_events: EventReader<MouseWheel>,
    mut query: Query<&mut OrbitCamera>,
) {
    let mut camera = query.single_mut();

    // Check if Egui wants the input (e.g. mouse over a window)
    let ctx = contexts.ctx_mut();
    
    // Robust check for Egui interaction:
    // 1. is_pointer_over_area() - covers windows/panels
    // 2. wants_pointer_input() - covers active interaction (scrolling, clicking)
    // 3. is_using_pointer() - covers dragging
    if ctx.is_pointer_over_area() || ctx.wants_pointer_input() || ctx.is_using_pointer() {
        motion_events.clear();
        scroll_events.clear();
        return;
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
    let rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw) * Quat::from_axis_angle(Vec3::X, orbit.pitch);
    let offset = rot * Vec3::Z * orbit.radius;
    let position = orbit.target_center + offset;

    transform.translation = position;
    transform.look_at(orbit.target_center, Vec3::Y);
}

/// Updates `ViewMode` based on camera zoom radius, with hysteresis to avoid
/// flickering at the boundary.
fn update_view_mode(
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    mut view_mode: ResMut<ViewMode>,
) {
    let Ok(orbit) = camera_query.get_single() else {
        return;
    };

    // Hysteresis: require crossing past the threshold by 10% in either direction
    let enter_starmap = STARMAP_TRANSITION_THRESHOLD;
    let exit_starmap = STARMAP_TRANSITION_THRESHOLD * 0.85;

    let new_mode = match *view_mode {
        ViewMode::System if orbit.radius > enter_starmap => ViewMode::Starmap,
        ViewMode::Starmap if orbit.radius < exit_starmap => ViewMode::System,
        other => other,
    };

    if new_mode != *view_mode {
        info!("View mode changed: {:?} → {:?} (radius: {:.0})", *view_mode, new_mode, orbit.radius);
        *view_mode = new_mode;
    }
}
