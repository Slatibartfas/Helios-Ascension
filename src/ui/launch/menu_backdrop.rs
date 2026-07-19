//! Menu backdrop plugin — rotating Earth close-up + animated camera parallax.
//!
//! Spans the launch flow (GRA-XYZ): while the player is on the splash
//! screen, the main menu, or any launch subview (New Game / Load Game /
//! Settings / Save Game), this plugin spawns a menu-only Earth at the
//! origin, drapes a directional sun light + ambient fill, and positions
//! the [`GameCamera`] to frame a slow close-up. The backdrop stays alive
//! across subview transitions so the rotating planet remains visible
//! behind the subview form widgets.
//!
//! On the transition out of the menu family to [`LaunchState::InGame`],
//! the plugin despawns every backdrop entity, restores the saved orbit
//! state + camera transform, and yields the camera back to the in-game
//! `update_camera_transform` pipeline. The in-game Earth (spawned later
//! by `setup_solar_system`) is independent of this menu Earth — they
//! never coexist in the world because they belong to non-overlapping
//! `LaunchState` windows.
//!
//! The 1.4 M-unit procedural starfield backdrop from
//! `src/render/backdrop.rs` continues to follow the camera, so the
//! universe skybox is automatically visible behind the Earth. No
//! skybox code changes are required.
//!
//! All four gameplay-camera systems in `src/plugins/camera.rs` are
//! gated on `LaunchState::is_in_game()`, so this plugin owns the camera
//! for the entire menu session without contention.

use bevy::prelude::*;
use bevy::render::view::Visibility;
use bevy::pbr::DirectionalLight;

use crate::plugins::camera::{GameCamera, OrbitCamera};
use crate::ui::launch::LaunchState;

// ─── Constants ──────────────────────────────────────────────────────────
//
// Sized so Earth fills ~28 % of a 1080p viewport at the chosen camera
// distance (28 % screen coverage follows from `radius / (radius +
// MENU_EARTH_DISTANCE)` for a sphere of radius 250 at 1_400 units:
// atan(250 / 1_400) ≈ 10.1°, double for FOV = ~20°, so the planet reads
// as a clear disc with comfortable limb breathing room).

/// Visual radius of the menu-only Earth in Bevy world units.
const MENU_EARTH_VISUAL_RADIUS: f32 = 250.0;

/// Clouds sit 1.5 % above the surface (matches `src/plugins/solar_system.rs:1251-1270`).
const MENU_EARTH_CLOUD_RADIUS_FACTOR: f32 = 1.015;

/// Camera→Earth distance. Sits comfortably outside the planet (3.7× its
/// radius) and inside the `OrbitCamera.max_radius = 2_000_000.0`.
const MENU_EARTH_DISTANCE: f32 = 1_400.0;

/// Yaw rate for the surface (deg/real-second). 4 °/s ≈ one full rotation
/// every 90 seconds — slow enough to feel cinematic.
const MENU_EARTH_YAW_DEG_PER_S: f32 = 4.0;

/// Yaw rate for the cloud layer (deg/real-second). Drift slightly
/// *counter* to the surface so clouds visibly separate from continents.
const MENU_EARTH_CLOUD_YAW_DEG_PER_S: f32 = -1.5;

/// Sun offset relative to Earth. The directional light shines from this
/// offset toward the origin so the day/night terminator lands on the
/// visible disc.
const MENU_EARTH_SUN_OFFSET: Vec3 = Vec3::new(2_000.0, 800.0, 1_500.0);

/// Illuminance of the directional sun. 30 000 lux matches a daylight
/// surface in Bevy's PBR pipeline.
const MENU_SUN_ILLUMINANCE: f32 = 30_000.0;

/// Ambient brightness so the night side reads instead of going pitch
/// black. Tuned to complement the directional fill.
const MENU_AMBIENT_BRIGHTNESS: f32 = 0.6;

/// Sphere mesh tessellation. (96, 48) is dense enough that the limb
/// silhouette stays smooth at the close-up framing without over-tessellating.
const MENU_EARTH_SPHERE_UV: (u32, u32) = (96, 48);

// ─── Types ──────────────────────────────────────────────────────────────

/// Marker component on every entity this plugin owns (Earth surface
/// sphere, clouds sphere, directional sun light). Tags enable a single
/// `Query<Entity, With<MenuBackdropMarker>>` to despawn everything on
/// the menu→InGame transition.
#[derive(Component, Debug, Clone, Copy)]
pub struct MenuBackdropMarker;

/// Per-entity role for entities tagged with [`MenuBackdropMarker`].
/// Lets the rotation system animate only Earth + clouds (and skip the
/// directional light) without positional indexing.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBackdropKind {
    /// The Earth surface sphere — rotates at `MENU_EARTH_YAW_DEG_PER_S`.
    Earth,
    /// The clouds sphere — rotates at `MENU_EARTH_CLOUD_YAW_DEG_PER_S`.
    Clouds,
    /// The directional sun light — never rotated.
    SunLight,
}

/// Whether the menu backdrop is currently alive. Set true on the first
/// frame the launch flow enters a menu state; cleared on exit to InGame.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct MenuBackdropActive(pub bool);

/// Snapshot of the [`OrbitCamera`] state captured when the menu session
/// begins. Restored on the menu→InGame transition so the player resumes
/// gameplay from exactly the zoom level they left.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct MenuBackdropCameraState {
    pub saved: Option<MenuBackdropSavedCamera>,
}

/// Concrete camera snapshot. Stored separately so the saved value is
/// `Copy` (Option of a Copy struct) and the Resource has `Default::default()`.
#[derive(Debug, Clone, Copy)]
pub struct MenuBackdropSavedCamera {
    pub radius: f32,
    pub pan_offset: Vec3,
    pub pitch: f32,
    pub yaw: f32,
}

/// System set — gives the menu backdrop its own scheduling slot inside
/// `Update` so other plugins can `.before()` / `.after()` it cleanly.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuBackdropSystemSet {
    /// Spawn / despawn entities on `LaunchState` edges.
    Transition,
    /// Per-frame animation (rotation, camera parallax).
    Animate,
}

// ─── Plugin ─────────────────────────────────────────────────────────────

/// Spawns the menu-only Earth + sun light, drives the slow rotation and
/// the gentle parallax orbit, and tears everything down on the
/// menu→InGame transition.
pub struct MenuBackdropPlugin;

impl Plugin for MenuBackdropPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuBackdropActive>()
            .init_resource::<MenuBackdropCameraState>()
            .configure_sets(
                Update,
                (
                    MenuBackdropSystemSet::Transition,
                    MenuBackdropSystemSet::Animate,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    menu_backdrop_transition_system.in_set(MenuBackdropSystemSet::Transition),
                    position_menu_camera.in_set(MenuBackdropSystemSet::Animate),
                    rotate_menu_earth.in_set(MenuBackdropSystemSet::Animate),
                ),
            );
    }
}

/// Public registrar so `LaunchPlugin::build` can opt-in via the same
/// `register_*` convention used by the launch subviews.
pub fn register_menu_backdrop_plugin(app: &mut App) {
    app.add_plugins(MenuBackdropPlugin);
}

// ─── Transition System ──────────────────────────────────────────────────

/// Detects `LaunchState` edges and spawns/despawns the backdrop
/// accordingly. Runs in [`MenuBackdropSystemSet::Transition`] inside
/// `Update`. The `Changed<LaunchState>` filter ensures it fires
/// exactly once per state edge, not every frame.
fn menu_backdrop_transition_system(
    launch_state: Res<LaunchState>,
    mut active: ResMut<MenuBackdropActive>,
    mut saved_state: ResMut<MenuBackdropCameraState>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    marker_query: Query<Entity, (With<MenuBackdropMarker>, Without<ChildOf>)>,
    mut camera_query: Query<(&mut OrbitCamera, &mut Transform), With<GameCamera>>,
) {
    if !launch_state.is_changed() {
        return;
    }

    if is_menu_launch_state(*launch_state) && !active.0 {
        // ── Entering the menu family: spawn backdrop + save camera state.
        if let Ok((mut orbit, _transform)) = camera_query.single_mut() {
            saved_state.saved = Some(MenuBackdropSavedCamera {
                radius: orbit.radius,
                pan_offset: orbit.pan_offset,
                pitch: orbit.pitch,
                yaw: orbit.yaw,
            });
        }
        spawn_menu_earth(&mut commands, &mut meshes, &mut materials, &asset_server);
        active.0 = true;
    } else if !is_menu_launch_state(*launch_state) && active.0 {
        // ── Leaving the menu family: despawn backdrop + restore camera.
        // Despawn only backdrop roots. In Bevy 0.18, despawning the Earth
        // cascades through its clouds child; explicitly queuing the child as
        // well would target it again after the parent removes it.
        for entity in marker_query.iter() {
            commands.entity(entity).despawn();
        }
        if let (Some(saved), Ok((mut orbit, mut transform))) =
            (saved_state.saved, camera_query.single_mut())
        {
            orbit.radius = saved.radius;
            orbit.pan_offset = saved.pan_offset;
            orbit.pitch = saved.pitch;
            orbit.yaw = saved.yaw;
            orbit.target_center = Vec3::ZERO;
            // Recompute Transform from restored OrbitCamera state so the
            // gameplay view picks up exactly where the player left it.
            let rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
                * Quat::from_axis_angle(Vec3::X, orbit.pitch);
            let offset = rot * Vec3::Z * orbit.radius;
            transform.translation = orbit.target_center + orbit.pan_offset + offset;
            transform.look_at(orbit.target_center + orbit.pan_offset, Vec3::Y);
        }
        saved_state.saved = None;
        active.0 = false;
    }
}

/// True for any `LaunchState` variant where the menu backdrop should be
/// alive (and its camera framing should be applied).
fn is_menu_launch_state(state: LaunchState) -> bool {
    matches!(
        state,
        LaunchState::MainMenu
            | LaunchState::NewGame
            | LaunchState::LoadGame
            | LaunchState::Settings
            | LaunchState::SaveGame
    )
}

// ─── Spawning ───────────────────────────────────────────────────────────

/// Build the menu-only Earth + clouds + lights. Mirrors the in-game
/// recipe at `src/plugins/solar_system.rs:822-834` (surface) and
/// `:1251-1270` (clouds). We deliberately skip the night-lights
/// `NightMaterial` (would shimmer at this close range) and the
/// atmosphere scattering shell (would render a misaligned glow ring
/// when the camera is inside Earth's shadow cone).
fn spawn_menu_earth(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
) {
    // ── Surface sphere ──────────────────────────────────────────────
    let daymap = asset_server.load("textures/celestial/planets/earth_daymap_8k.jpg");
    let normal = asset_server.load("textures/celestial/planets/earth_normal_8k.png");

    let surface_mesh = meshes.add(
        Sphere::new(MENU_EARTH_VISUAL_RADIUS)
            .mesh()
            .uv(MENU_EARTH_SPHERE_UV.0, MENU_EARTH_SPHERE_UV.1),
    );
    let surface_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(daymap),
        normal_map_texture: Some(normal),
        // Tiny emissive floor so the night side reads instead of going
        // pitch black — matches the in-game recipe at solar_system.rs:829.
        emissive: LinearRgba::WHITE * 0.006,
        perceptual_roughness: 0.7,
        metallic: 0.0,
        reflectance: 0.3,
        ..default()
    });

    commands
        .spawn((
            Mesh3d(surface_mesh),
            MeshMaterial3d(surface_material),
            Transform::from_translation(Vec3::ZERO),
            Visibility::default(),
            MenuBackdropMarker,
            MenuBackdropKind::Earth,
        ))
        .with_children(|parent| {
            // ── Clouds shell (1.5 % larger than surface) ────────────
            let clouds_tex =
                asset_server.load("textures/celestial/planets/earth_clouds_8k.jpg");
            let clouds_mesh = meshes.add(
                Sphere::new(MENU_EARTH_VISUAL_RADIUS * MENU_EARTH_CLOUD_RADIUS_FACTOR)
                    .mesh()
                    .uv(MENU_EARTH_SPHERE_UV.0, MENU_EARTH_SPHERE_UV.1),
            );
            let clouds_material = materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(clouds_tex),
                alpha_mode: AlphaMode::Add,
                depth_bias: -1.0,
                unlit: false,
                perceptual_roughness: 0.8,
                metallic: 0.0,
                reflectance: 0.6,
                ..default()
            });
            parent.spawn((
                Mesh3d(clouds_mesh),
                MeshMaterial3d(clouds_material),
                Transform::default(),
                MenuBackdropMarker,
                MenuBackdropKind::Clouds,
            ));
        });

    // ── Directional sun light (no shadows — close-up is static enough
    //    that shadow maps would be wasted GPU work).
    let sun_dir = -MENU_EARTH_SUN_OFFSET.normalize();
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.88),
            illuminance: MENU_SUN_ILLUMINANCE,
            shadows_enabled: false,
            ..default()
        },
        Transform::default().looking_at(sun_dir, Vec3::Y),
        MenuBackdropMarker,
        MenuBackdropKind::SunLight,
    ));

    // ── Ambient fill (cool blue tint, low brightness) ──────────────
    commands.insert_resource(bevy::pbr::AmbientLight {
        color: Color::srgb(0.06, 0.10, 0.18),
        brightness: MENU_AMBIENT_BRIGHTNESS,
        ..default()
    });
}

// ─── Per-frame Systems ─────────────────────────────────────────────────

/// Position the [`GameCamera`] to frame the menu Earth. Writes only
/// `OrbitCamera` state — the gated-off `update_camera_transform` in
/// `src/plugins/camera.rs` will compute the actual `Transform` next tick.
/// This keeps the camera state compatible with the in-game pipeline for
/// a clean handoff when we restore saved values on exit.
///
/// Uses two incommensurable sine waves for a gentle parallax orbit so the
/// shot never feels static and the camera never returns to exactly the
/// same angle. Periods are ~157 s (yaw) and ~209 s (pitch) — slow enough
/// to be cinematic, fast enough that the player notices the breathing.
fn position_menu_camera(
    active: Res<MenuBackdropActive>,
    time: Res<Time>,
    mut camera_query: Query<&mut OrbitCamera, With<GameCamera>>,
) {
    if !active.0 {
        return;
    }
    let Ok(mut orbit) = camera_query.single_mut() else {
        return;
    };

    let t = time.elapsed_secs();
    let yaw_offset = (t * 0.04).sin() * 0.12;
    let pitch_offset = (t * 0.03).cos() * 0.04;

    orbit.target_center = Vec3::ZERO;
    orbit.pan_offset = Vec3::ZERO;
    orbit.radius = MENU_EARTH_DISTANCE;
    orbit.pitch = 0.18 + pitch_offset;
    orbit.yaw = yaw_offset;
    // Allow the menu camera to zoom inside the default `min_radius = 5.0`
    // (our `radius = 1_400.0` is well above that floor, but `update_min_zoom`
    // clamps the floor to a star's visual radius whenever an anchor is set;
    // we keep `CameraAnchor(None)` so the floor stays at 5.0).
}

/// Rotate the Earth surface + clouds at their respective rates. Uses real
/// `Time` so the menu animation continues even when the simulation is
/// paused (`SimulationTime` doesn't advance during the launch flow).
fn rotate_menu_earth(
    active: Res<MenuBackdropActive>,
    time: Res<Time>,
    mut query: Query<(&MenuBackdropKind, &mut Transform), With<MenuBackdropMarker>>,
) {
    if !active.0 {
        return;
    }
    let dt = time.delta_secs();
    for (kind, mut transform) in query.iter_mut() {
        let rate_deg_per_s = match kind {
            MenuBackdropKind::Earth => MENU_EARTH_YAW_DEG_PER_S,
            MenuBackdropKind::Clouds => MENU_EARTH_CLOUD_YAW_DEG_PER_S,
            // The directional light's `Transform` only matters for its
            // `look_at` direction — it has no parent so we skip rotation.
            MenuBackdropKind::SunLight => continue,
        };
        transform.rotate_y(rate_deg_per_s.to_radians() * dt);
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_backdrop_camera_state_defaults_empty() {
        let state = MenuBackdropCameraState::default();
        assert!(state.saved.is_none());
    }

    #[test]
    fn menu_backdrop_active_defaults_to_false() {
        let active = MenuBackdropActive::default();
        assert!(!active.0);
    }

    #[test]
    fn menu_state_predicate_matches_menu_family() {
        // Splash is NOT a menu state — the splash owns its own panel.
        assert!(!is_menu_launch_state(LaunchState::Splash));
        assert!(is_menu_launch_state(LaunchState::MainMenu));
        assert!(is_menu_launch_state(LaunchState::NewGame));
        assert!(is_menu_launch_state(LaunchState::LoadGame));
        assert!(is_menu_launch_state(LaunchState::Settings));
        assert!(is_menu_launch_state(LaunchState::SaveGame));
        // InGame explicitly clears the backdrop.
        assert!(!is_menu_launch_state(LaunchState::InGame));
    }

    #[test]
    fn menu_backdrop_marker_kind_equality_is_useful() {
        // The rotation system relies on `match` exhaustiveness; this
        // asserts the variants are distinct and `Copy`.
        assert_ne!(MenuBackdropKind::Earth, MenuBackdropKind::Clouds);
        assert_ne!(MenuBackdropKind::Clouds, MenuBackdropKind::SunLight);
        assert_ne!(MenuBackdropKind::Earth, MenuBackdropKind::SunLight);
        let a = MenuBackdropKind::Earth;
        let _b = a; // Copy
        let _c = a; // Copy doesn't consume
    }

    #[test]
    fn parallax_offsets_stay_in_design_band() {
        // The parallax amplitudes are tuned to be visually noticeable but
        // not nauseating. If someone "fixes" them later, this test
        // catches a regression that would jerk the camera.
        let sample_times = [0.0_f32, 30.0, 60.0, 90.0, 120.0, 150.0];
        for &t in &sample_times {
            let yaw = (t * 0.04).sin() * 0.12;
            let pitch = (t * 0.03).cos() * 0.04;
            assert!(yaw.abs() <= 0.121, "yaw {} out of band at t={}", yaw, t);
            assert!(pitch.abs() <= 0.041, "pitch {} out of band at t={}", pitch, t);
        }
    }
}