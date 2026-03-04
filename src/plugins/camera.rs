use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::view::Hdr;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass, EguiStartupSet};

use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::astronomy::{update_render_transform, SCALING_FACTOR};
use crate::game_state::ActiveMenu;
use crate::plugins::solar_system::CelestialBody;
use crate::plugins::solar_system_data::BodyType;
use crate::plugins::starmap::SystemMetadata;

/// Base zoom threshold multiplier. The actual threshold is calculated as
/// `bounding_radius_au * SCALING_FACTOR * THRESHOLD_MULTIPLIER`.
/// This provides comfortable zoom distances without requiring excessive scrolling.
/// Value of 1.2 ensures the camera stays in system view long enough to see
/// outer orbits before transitioning to the starmap.
pub const STARMAP_THRESHOLD_MULTIPLIER: f32 = 1.2;

/// Minimum zoom threshold in game units to ensure reasonable behavior for very small systems.
pub const MIN_STARMAP_THRESHOLD: f32 = 75_000.0;

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

/// Stores the egui `available_rect` captured at the end of the previous frame
/// (in `PostUpdate`, after all UI panels have been drawn).
///
/// Used by `orbit_camera_controls` to detect when the pointer is over an anchored
/// panel (SidePanel, TopBottomPanel) — these panels don't show up in
/// `ctx.is_pointer_over_area()`, which only detects floating windows.
#[derive(Resource, Default)]
pub struct EguiPanelBounds {
    pub available_rect: Option<egui::Rect>,
}

/// Saved camera radius from before entering a full-screen menu.
/// Restored when returning to Survey/Starmap view.
#[derive(Resource, Default)]
pub struct SavedSurveyRadius(pub Option<f32>);

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewMode>()
            .init_resource::<EguiPanelBounds>()
            .init_resource::<SavedSurveyRadius>()
            // Spawn camera in PreStartup before EguiStartupSet::InitContexts so
            // bevy_egui attaches its context to this camera entity. Without this,
            // EguiContexts::ctx_mut() returns Err during Startup and custom fonts
            // (including emoji fonts) are silently never applied.
            .add_systems(
                PreStartup,
                spawn_camera.before(EguiStartupSet::InitContexts),
            )
            .add_systems(EguiPrimaryContextPass, orbit_camera_controls)
            .add_systems(
                Update,
                (
                    update_camera_transform.after(update_render_transform),
                    update_view_mode,
                    update_min_zoom,
                    save_restore_zoom_on_menu_change,
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
    pub pan_sensitivity: f32,
    pub target_center: Vec3,
    /// Offset from the anchor position for panning (WASD movement)
    pub pan_offset: Vec3,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            radius: 20_000.0,
            pitch: 0.5,
            yaw: 0.0,
            min_radius: 5.0,
            max_radius: 2_000_000.0, // Increased to exceed max threshold (Sol: 400*1500*2.5 = 1.5M)
            zoom_sensitivity: 100.0,
            rotate_sensitivity: 0.003,
            pan_sensitivity: 500.0,
            target_center: Vec3::ZERO,
            pan_offset: Vec3::ZERO,
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 10_000.0, 20_000.0).looking_at(Vec3::ZERO, Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            far: 3_000_000.0, // Increased to comfortably render at max camera distance
            ..default()
        }),
        Hdr,
        GameCamera,
        CameraAnchor(None),
        OrbitCamera::default(),
    ));
}

/// Captures `available_rect` from egui after all UI panels have been rendered.
/// Must be registered AFTER all egui panel systems so the rect is panel-aware.
/// Registered by UIPlugin (not CameraPlugin) to ensure correct ordering within
/// the Update schedule where egui context is valid.
pub fn capture_egui_panel_bounds(mut contexts: EguiContexts, mut bounds: ResMut<EguiPanelBounds>) {
    if let Ok(ctx) = contexts.ctx_mut() {
        bounds.available_rect = Some(ctx.available_rect());
    }
}

fn orbit_camera_controls(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut motion_events: MessageReader<MouseMotion>,
    mut scroll_events: MessageReader<MouseWheel>,
    mut query: Query<&mut OrbitCamera>,
    panel_bounds: Res<EguiPanelBounds>,
) {
    let mut camera = query.single_mut().unwrap();

    // Block camera control when in full-screen UI modes (i.e. menus that block world interaction)
    if active_menu.current.blocks_world_interaction() {
        motion_events.clear();
        scroll_events.clear();
        return;
    }

    // Check if Egui wants the input (e.g. mouse over a floating window or an anchored panel)
    if let Ok(ctx) = contexts.ctx_mut() {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());

        // is_pointer_over_area() catches floating windows/popups.
        // panel_bounds.available_rect (captured last frame in PostUpdate, after all panels
        // rendered) catches anchored panels (SidePanel, TopBottomPanel) that don't show up
        // in is_pointer_over_area().
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.map_or(false, |pos| !available.contains(pos))
        } else {
            false
        };

        if ctx.is_pointer_over_area() || ctx.is_using_pointer() || over_panel {
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

    // WASD panning - W/S = up/down, A/D = left/right
    let dt = time.delta_secs();
    let mut pan_direction = Vec3::ZERO;

    // W = up, S = down
    if keyboard.pressed(KeyCode::KeyW) {
        pan_direction += Vec3::Y;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        pan_direction -= Vec3::Y;
    }
    // A = left, D = right
    if keyboard.pressed(KeyCode::KeyA) {
        pan_direction -= Vec3::X;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        pan_direction += Vec3::X;
    }

    if pan_direction != Vec3::ZERO {
        // Rotate pan direction to match camera yaw orientation (for A/D)
        let rot = Quat::from_axis_angle(Vec3::Y, camera.yaw);
        let world_pan = rot * pan_direction.normalize();
        let pan_amount = world_pan * camera.pan_sensitivity * dt;
        camera.pan_offset += pan_amount;
    }

    // Home key to recenter the view (clear pan offset)
    if keyboard.just_pressed(KeyCode::Home) {
        camera.pan_offset = Vec3::ZERO;
    }
}

fn update_camera_transform(
    // ParamSet is required because both queries access `Transform`:
    // - p0 mutably (camera) and p1 immutably (target body).
    // Read the target body's Transform (not GlobalTransform) so we see the
    // current frame's position written by update_render_transform.
    // GlobalTransform is only flushed in PostUpdate, so it lags one frame.
    mut param_set: ParamSet<(
        Query<(&mut Transform, &mut OrbitCamera, &CameraAnchor)>,
        Query<&Transform, Without<GameCamera>>,
    )>,
) {
    // Step 1: extract the anchor entity while holding the camera borrow.
    let anchor_entity: Option<Entity> =
        param_set.p0().single().map(|(_, _, a)| a.0).unwrap_or(None);

    // Step 2: look up the target's current world position via p1 (no conflicts).
    let target_pos: Option<Vec3> =
        anchor_entity.and_then(|e| param_set.p1().get(e).ok().map(|t| t.translation));

    // Step 3: update the camera using the positions gathered above.
    if let Ok((mut transform, mut orbit, _)) = param_set.p0().single_mut() {
        if let Some(pos) = target_pos {
            orbit.target_center = pos;
        }

        let rot =
            Quat::from_axis_angle(Vec3::Y, orbit.yaw) * Quat::from_axis_angle(Vec3::X, orbit.pitch);
        let offset = rot * Vec3::Z * orbit.radius;
        let position = orbit.target_center + orbit.pan_offset + offset;

        transform.translation = position;
        transform.look_at(orbit.target_center + orbit.pan_offset, Vec3::Y);
    }
}

/// Dynamically adjusts the camera's `min_radius` based on what's currently anchored.
/// Prevents the camera from zooming so close to a star that the glare billboard fades
/// to zero and the star becomes a black sphere.
///
/// - **Stars**: clamp to `max(visual_radius × 2.5, 250)` — safely above the 200-unit
///   glare-fade threshold in `update_star_glare_lod`.
/// - **Other bodies**: clamp to `max(visual_radius × 2.0, 5.0)` for comfortable close-ups.
/// Dynamically adjusts the camera's `min_radius` so the camera can never zoom
/// close enough to a star that the glare LOD fades to zero (leaving a black sphere).
///
/// Two-tier logic:
/// 1. If the anchor entity IS a `CelestialBody` (non-star), allow a tighter zoom.
/// 2. Otherwise (anchor is a StarSystemIcon, or no anchor) scan the current
///    system's star bodies and floor at `visual_radius × 2.5, min 250`.
fn update_min_zoom(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    mut camera_query: Query<(&mut OrbitCamera, &CameraAnchor)>,
    body_query: Query<(&CelestialBody, Option<&SystemId>)>,
) {
    let Ok((mut orbit, anchor)) = camera_query.single_mut() else {
        return;
    };

    // Only meaningful while in system view.
    if *view_mode != ViewMode::System {
        // Restore a generous default in starmap so icons can be approached.
        if orbit.min_radius > 5.0 {
            orbit.min_radius = 5.0;
        }
        return;
    }

    // --- Case 1: anchored directly to a CelestialBody -----------------------
    if let Some(anchor_entity) = anchor.0 {
        if let Ok((body, _)) = body_query.get(anchor_entity) {
            let new_min = if body.body_type == BodyType::Star {
                (body.visual_radius * 2.5).max(250.0)
            } else {
                (body.visual_radius * 2.0).max(5.0)
            };
            apply_min(&mut orbit, new_min);
            return;
        }
    }

    // --- Case 2: anchored to a StarSystemIcon or no anchor ------------------
    // Find the largest star in the current system and enforce its floor.
    let sys_id = current_system.0;
    let star_floor = body_query
        .iter()
        .filter(|(body, sid)| {
            body.body_type == BodyType::Star && sid.map_or(sys_id == 0, |s| s.0 == sys_id)
        })
        .map(|(body, _)| (body.visual_radius * 2.5).max(250.0))
        .fold(0.0_f32, f32::max);

    let new_min = if star_floor > 0.0 { star_floor } else { 5.0 };
    apply_min(&mut orbit, new_min);
}

/// Saves the camera zoom radius when entering a full-screen menu (blocks_world_interaction),
/// and restores it when returning to Survey or Starmap.
fn save_restore_zoom_on_menu_change(
    active_menu: Res<ActiveMenu>,
    mut saved: ResMut<SavedSurveyRadius>,
    mut camera_query: Query<&mut OrbitCamera, With<GameCamera>>,
) {
    if !active_menu.is_changed() {
        return;
    }
    let Ok(mut orbit) = camera_query.single_mut() else {
        return;
    };

    if active_menu.current.blocks_world_interaction() {
        // Entering a full-screen menu — save the current radius.
        saved.0 = Some(orbit.radius);
    } else if let Some(saved_radius) = saved.0.take() {
        // Returning to survey/starmap — restore the saved radius.
        orbit.radius = saved_radius.clamp(orbit.min_radius, orbit.max_radius);
    }
}

#[inline]
fn apply_min(orbit: &mut OrbitCamera, new_min: f32) {
    if (orbit.min_radius - new_min).abs() > 0.1 {
        if orbit.radius < new_min {
            orbit.radius = new_min;
        }
        orbit.min_radius = new_min;
    }
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
    let Ok(orbit) = camera_query.single() else {
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
