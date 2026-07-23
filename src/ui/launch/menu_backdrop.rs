//! Menu backdrop plugin — rotating Earth close-up + animated camera parallax.
//!
//! Spans the launch flow (GRA-XYZ): while the player is on the main
//! menu or any launch subview (New Game / Load Game / Settings /
//! Save Game), this plugin spawns a menu-only Earth at the origin,
//! drapes a directional sun light + ambient fill, and positions
//! the [`GameCamera`] to frame a slow close-up. The backdrop stays
//! alive across subview transitions so the rotating planet remains
//! visible behind the subview form widgets.
//!
//! While the menu is active, the in-game solar-system bodies (Sol +
//! planets + moons, spawned at `Startup` by `setup_solar_system` and
//! not gated on `LaunchState`) are hidden via `Visibility::Hidden` so
//! they don't render behind the menu Earth. Visibility is restored on
//! the menu→InGame transition.
//!
//! On the transition out of the menu family to [`LaunchState::InGame`],
//! the plugin despawns every backdrop entity, restores the saved orbit
//! state + camera transform, re-shows the in-game solar system, and
//! yields the camera back to the in-game `update_camera_transform`
//! pipeline.
//!
//! The 1.4 M-unit procedural starfield backdrop from
//! `src/render/backdrop.rs` continues to follow the camera, so the
//! universe skybox is automatically visible behind the Earth. No
//! skybox code changes are required.
//!
//! All four gameplay-camera systems in `src/plugins/camera.rs` are
//! gated on `LaunchState::is_in_game()`, so this plugin owns the camera
//! for the entire menu session without contention.

use bevy::image::Image;
use bevy::prelude::*;

use crate::plugins::camera::{GameCamera, OrbitCamera};
use crate::plugins::solar_system::{CelestialBody, Planet};
use crate::ui::launch::LaunchState;

// ─── Constants ──────────────────────────────────────────────────────────
//
// Sized so Earth fills ~28 % of a 1080p viewport at the chosen camera
// distance (28 % screen coverage follows from `radius / (radius +
// MENU_EARTH_DISTANCE)` for a sphere of radius 250 at 1_400 units:
// atan(250 / 1_400) ≈ 10.1°, double for FOV = ~20°, so the planet reads
// as a clear disc with comfortable limb breathing room).

/// World-space position of the menu Earth.
///
/// Set to 1 AU away from the origin (where Sol lives) so the camera
/// sees Earth as the close-up subject with Sol appearing as a distant
/// bright disc in the background. SCALING_FACTOR in
/// `src/astronomy/systems.rs` is 1_500.0 (1 AU → 1_500 Bevy units), so
/// this matches the in-game Earth's nominal orbital distance.
const MENU_EARTH_POSITION: Vec3 = Vec3::new(1_500.0, 0.0, 0.0);

/// Visual radius of the menu-only Earth in Bevy world units.
///
/// Sized so the planet fills the central two-thirds of a 1920×1080
/// viewport at `MENU_EARTH_DISTANCE = 1_900.0` with the perspective
/// camera's default 60° FOV: atan(1500 / 1900) ≈ 38°, so the disc
/// occupies ~76° of the screen — a dramatic close-up with the limb
/// curving off the top and bottom edges.
const MENU_EARTH_VISUAL_RADIUS: f32 = 1_500.0;

/// Clouds sit 1.5 % above the surface (matches `src/plugins/solar_system.rs:1251-1270`).
const MENU_EARTH_CLOUD_RADIUS_FACTOR: f32 = 1.015;

/// Camera→Earth distance. ~3.4× the planet radius — Earth fills the
/// frame as a dramatic hero shot with the limb curving off the edges,
/// while leaving enough breathing room that the Moon and the distant
/// Sol stay visible. (At 1.0× the radius the camera grazes the surface,
/// which is what produced the broken point-blank / tiny-dot framing.)
const MENU_EARTH_DISTANCE: f32 = 5_100.0;

/// Base pitch angle (radians) for the camera. ~+23° tilts the camera
/// DOWN at Earth so we look down on the surface from a slightly
/// elevated vantage. This puts Sol (1 AU to the -X of Earth) clearly
/// above the planet's limb in the viewport rather than directly
/// behind it — the "Earth-and-Sun over-the-shoulder" shot.
const MENU_CAMERA_BASE_PITCH: f32 = 0.4;

/// Base yaw offset (radians) for the camera. Orbits the camera around
/// Earth for the hero shot. The menu-owned sun disc is placed along the
/// same direction the sunlight comes from, so the visible sun always
/// matches the lighting regardless of this yaw.
const MENU_CAMERA_BASE_YAW: f32 = -0.55;

/// Radius of the Moon's orbit around the menu Earth (in Bevy world
/// units). 3.7× Earth radius — larger than reality for cinematic
/// legibility so the Moon reads as a discrete object rather than a
/// pixel smudge.
const MENU_MOON_ORBIT_RADIUS: f32 = 5_550.0;

/// Visual radius of the menu Moon. Sized so it reads as a ~30 px
/// disc at the menu's default viewport.
const MENU_MOON_VISUAL_RADIUS: f32 = 350.0;

/// How fast the Moon orbits the menu Earth (deg/real-second). A full
/// lunar cycle every ~30 seconds keeps the Moon visible and moving
/// without distracting from the surface rotation.
const MENU_MOON_ORBIT_DEG_PER_S: f32 = 12.0;

/// Yaw rate for the surface (deg/real-second). 4 °/s ≈ one full rotation
/// every 90 seconds — slow enough to feel cinematic.
const MENU_EARTH_YAW_DEG_PER_S: f32 = 4.0;

/// Yaw rate for the cloud layer (deg/real-second). Drift slightly
/// *counter* to the surface so clouds visibly separate from continents.
const MENU_EARTH_CLOUD_YAW_DEG_PER_S: f32 = -1.5;

/// Menu-owned sun disc. The real in-game Sol is hidden during the menu
/// because it sits at the world origin only ~1 Earth-radius from the menu
/// Earth's limb — geometrically glued to the planet's edge at any camera
/// angle (the "sun embedded in Earth" artefact). Instead we place a menu
/// sun disc far along the sunlight direction so it reads as a proper
/// distant sun that agrees with the lighting.
///
/// Direction from Earth = normalize(-MENU_EARTH_SUN_OFFSET) = (-1, 0, 0);
/// we push it out 60 000 units and tilt it slightly up so it sits clear of
/// Earth's limb in the hero shot.
const MENU_SUN_DIR: Vec3 = Vec3::new(-0.94, 0.34, 0.0);
/// Distance of the menu sun disc from Earth (world units).
const MENU_SUN_DISTANCE: f32 = 60_000.0;
/// Angular radius of the menu sun disc on screen. 1 800 / 60 000 ≈ 1.7° —
/// a crisp distant sun, clearly separated from Earth's limb.
const MENU_SUN_DISC_RADIUS: f32 = 1_800.0;

/// Illuminance of the directional sun. 30 000 lux matches a daylight
/// surface in Bevy's PBR pipeline.
const MENU_SUN_ILLUMINANCE: f32 = 30_000.0;

/// Illuminance of the secondary fill light — softens Earth's night
/// side so the camera-facing face always reads. Tuned ~50× dimmer than
/// the primary sun so the day/night terminator remains visible.
const MENU_FILL_ILLUMINANCE: f32 = 4_000.0;

/// Ambient brightness — bumped up so the night-side terminator isn't
/// pitch black. The day-side stays dominant thanks to the much brighter
/// directional sun.
const MENU_AMBIENT_BRIGHTNESS: f32 = 2.0;

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
    /// The Moon — orbits Earth at `MENU_MOON_ORBIT_RADIUS` driven by the
    /// animation system. Not rotated around its own axis (the texture
    /// handles that via the static albedo map).
    Moon,
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

/// Cached [`Handle<Image>`]s for the menu backdrop's large textures,
/// kicked off during `Startup` so the cloud/daymap/normal/moon
/// textures are already streaming (or fully uploaded) by the time
/// the menu transition system spawns Earth on frame 1.
///
/// Why this exists: the menu backdrop's three 8K Earth textures and
/// the Moon's 8K JPG are large (≈30 MB combined). Calling
/// `asset_server.load()` inside `spawn_menu_earth` only kicks off the
/// async load at frame 1, so the cloud shell — which sits 1.5 % above
/// the surface at additive blending — materialises a frame or two
/// after the surface does, producing a visible "pop" right behind the
/// menu. Loading the handles at `Startup` (before any frame draws)
/// gives the loader ~hundreds of ms head start and eliminates the pop
/// (or reduces it to a single frame at worst).
///
/// Bevy's asset handles are path-keyed, so calling `asset_server.load`
/// with the same path at spawn time returns the same handle — we can
/// safely keep the `asset_server.load` calls in `spawn_menu_earth` as
/// a fallback for the (cold-path / fresh-load) case where the
/// resource wasn't pre-populated. In practice the resource is always
/// populated by `Startup`; the spawn-time loads exist only as a
/// belt-and-braces safety net.
#[derive(Resource, Debug, Clone, Default)]
pub struct PreloadedMenuAssets {
    pub earth_daymap: Handle<Image>,
    pub earth_normal: Handle<Image>,
    pub earth_clouds: Handle<Image>,
    pub moon_8k: Handle<Image>,
}

impl PreloadedMenuAssets {
    /// Asset paths matching the four `asset_server.load(...)` calls in
    /// `spawn_menu_earth`. Kept in one place so the preload system and
    /// the spawn site can't drift apart.
    pub const EARTH_DAYMAP_PATH: &'static str = "textures/celestial/planets/earth_daymap_8k.jpg";
    pub const EARTH_NORMAL_PATH: &'static str = "textures/celestial/planets/earth_normal_8k.png";
    pub const EARTH_CLOUDS_PATH: &'static str = "textures/celestial/planets/earth_clouds_8k.jpg";
    pub const MOON_8K_PATH: &'static str = "textures/celestial/moons/moon_8k.jpg";

    /// True when every preload handle has been populated. Compares
    /// against the type-default `Handle<Image>` (the "null" handle),
    /// which is what `init_resource` gives us before the preload
    /// system runs.
    pub fn is_initialized(&self) -> bool {
        self.earth_daymap != Handle::<Image>::default()
    }
}

/// Startup system: kick off the async load of the menu backdrop's
/// four large textures as early as possible so the streaming pipeline
/// runs in parallel with the in-game boot-init chain (solar-system
/// spawn, 60-system population, resource generation) that fills the
/// transition window between splash dismissal and the first menu frame.
///
/// Registered in `Startup` from `MenuBackdropPlugin::build`, so it
/// runs before any `Update` systems — including the menu backdrop
/// transition system that spawns Earth on frame 1. This is the
/// critical timing win: calling `asset_server.load` here (instead of
/// inside `spawn_menu_earth`) starts the load ~hundreds of ms before
/// the player ever sees the menu, so the cloud/daymap/normal/moon
/// textures are already streaming (or fully uploaded) by the time the
/// materials are constructed.
///
/// Populates the [`PreloadedMenuAssets`] resource in place via
/// `ResMut<PreloadedMenuAssets>` so we don't need to insert a fresh
/// resource (the plugin already calls `init_resource`).
fn preload_menu_assets(
    asset_server: Res<AssetServer>,
    mut preloaded: ResMut<PreloadedMenuAssets>,
) {
    if preloaded.is_initialized() {
        // Idempotent: if the system somehow runs twice (e.g. on app
        // hot-reload), don't double-load. `asset_server.load` would
        // return the same handle anyway, but reusing the cached
        // handle avoids any loader refcount churn.
        return;
    }
    preloaded.earth_daymap = asset_server.load(PreloadedMenuAssets::EARTH_DAYMAP_PATH);
    preloaded.earth_normal = asset_server.load(PreloadedMenuAssets::EARTH_NORMAL_PATH);
    preloaded.earth_clouds = asset_server.load(PreloadedMenuAssets::EARTH_CLOUDS_PATH);
    preloaded.moon_8k = asset_server.load(PreloadedMenuAssets::MOON_8K_PATH);
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
            // Preloaded texture handles for the menu backdrop. Populated
            // by `preload_menu_assets` in `Startup` so the cloud/daymap/
            // normal/moon textures are streaming before frame 1.
            .init_resource::<PreloadedMenuAssets>()
            .add_systems(Startup, preload_menu_assets)
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
                    hide_in_game_solar_system.in_set(MenuBackdropSystemSet::Transition),
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
/// `Update`.
///
/// The `LaunchState` defaults to `LaunchState::MainMenu` (see
/// `src/ui/launch/mod.rs`), which means the very first frame is *already*
/// in the menu family and `LaunchState::is_changed()` is false on every
/// startup — the spawn branch must therefore also fire when `active.0`
/// is false and the current state is a menu state, not only on real
/// state changes. We still gate on `launch_state.is_changed()` for the
/// *exit* path so we don't despawn backdrop entities every frame.
fn menu_backdrop_transition_system(
    launch_state: Res<LaunchState>,
    mut active: ResMut<MenuBackdropActive>,
    mut saved_state: ResMut<MenuBackdropCameraState>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    preloaded: Res<PreloadedMenuAssets>,
    marker_query: Query<Entity, (With<MenuBackdropMarker>, Without<ChildOf>)>,
    mut camera_query: Query<(&mut OrbitCamera, &mut Transform), With<GameCamera>>,
) {
    let in_menu = is_menu_launch_state(*launch_state);

    if in_menu && !active.0 {
        // ── Entering the menu family (first frame OR state edge):
        // spawn backdrop + save camera state. Idempotent — sets active.0.
        info!("menu_backdrop: spawn branch fired (in_menu={}, active={})", in_menu, active.0);
        if let Ok((orbit, _transform)) = camera_query.single() {
            saved_state.saved = Some(MenuBackdropSavedCamera {
                radius: orbit.radius,
                pan_offset: orbit.pan_offset,
                pitch: orbit.pitch,
                yaw: orbit.yaw,
            });
        }
        spawn_menu_earth(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            &preloaded,
        );
        active.0 = true;
    } else if !in_menu && active.0 {
        // ── Leaving the menu family: despawn backdrop + restore camera.
        // Runs whenever we're out of the menu and the backdrop is still
        // marked active (not only on the exact state-change frame) so the
        // teardown is robust to ordering with `consume_launch_actions_system`.
        // Idempotent: `active.0` flips false after the first pass.
        // Despawn only backdrop roots; despawning Earth cascades to clouds.
        let roots: Vec<Entity> = marker_query.iter().collect();
        info!("menu_backdrop: despawn branch fired, despawning {} roots", roots.len());
        for entity in roots {
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
///
/// Uses `PreloadedMenuAssets` for the four large textures (daymap,
/// normal, clouds, moon) so the load pipeline started at `Startup`
/// — not at frame 1 — and the cloud shell doesn't pop into view a
/// frame or two after the surface does. If the preloaded resource
/// isn't populated (e.g. a test scenario that boots straight into the
/// menu without running `Startup`), we fall back to `asset_server.load`
/// to keep the menu renderable.
fn spawn_menu_earth(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    preloaded: &PreloadedMenuAssets,
) {
    // ── Surface sphere ──────────────────────────────────────────────
    // Prefer the preloaded handles; fall back to live-load only if the
    // preload system never ran (paranoia — the resource is always
    // populated in the normal LaunchPlugin::build path).
    let daymap = if preloaded.is_initialized() {
        preloaded.earth_daymap.clone()
    } else {
        asset_server.load(PreloadedMenuAssets::EARTH_DAYMAP_PATH)
    };
    let normal = if preloaded.is_initialized() {
        preloaded.earth_normal.clone()
    } else {
        asset_server.load(PreloadedMenuAssets::EARTH_NORMAL_PATH)
    };

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
            Transform::from_translation(MENU_EARTH_POSITION),
            Visibility::default(),
            MenuBackdropMarker,
            MenuBackdropKind::Earth,
        ))
        .with_children(|parent| {
            // ── Clouds shell (1.5 % larger than surface) ────────────
            // The cloud shell is the most visible "pop" target: it sits
            // 1.5 % above the surface at additive blending, so when its
            // texture arrives a frame late the surface appears bare
            // for an instant before the cloud shell materialises.
            let clouds_tex = if preloaded.is_initialized() {
                preloaded.earth_clouds.clone()
            } else {
                asset_server.load(PreloadedMenuAssets::EARTH_CLOUDS_PATH)
            };
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

    // ── Moon — orbits Earth at MENU_MOON_ORBIT_RADIUS. ─────────────
    // Uses the moon texture (assets/textures/celestial/moons/) so it
    // reads as the Moon rather than as a generic grey sphere. The
    // rotation system drives it around the orbit; it does NOT inherit
    // Earth's transform because that would drag it with the planet's
    // spin. Instead it's a sibling marker entity orbiting the planet
    // via its own position update.
    let moon_mesh = meshes.add(
        Sphere::new(MENU_MOON_VISUAL_RADIUS)
            .mesh()
            .uv(MENU_EARTH_SPHERE_UV.0, MENU_EARTH_SPHERE_UV.1),
    );
    let moon_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        // Use the preloaded moon handle so the 15 MB JPG is already
        // streaming by the time the moon mesh is spawned.
        base_color_texture: Some(if preloaded.is_initialized() {
            preloaded.moon_8k.clone()
        } else {
            asset_server.load(PreloadedMenuAssets::MOON_8K_PATH)
        }),
        emissive: LinearRgba::WHITE * 0.02,
        perceptual_roughness: 0.95,
        metallic: 0.0,
        reflectance: 0.05,
        ..default()
    });
    commands.spawn((
        Mesh3d(moon_mesh),
        MeshMaterial3d(moon_material),
        // Initial position: Moon starts at orbit phase 0 (positive Z
        // direction from Earth, perpendicular to the camera's primary
        // line of sight so it reads as a separate object on first frame).
        Transform::from_translation(MENU_EARTH_POSITION + Vec3::new(0.0, 0.0, MENU_MOON_ORBIT_RADIUS)),
        Visibility::default(),
        MenuBackdropMarker,
        MenuBackdropKind::Moon,
    ));

    // ── Directional sun light (no shadows — close-up is static enough
    //    that shadow maps would be wasted GPU work). Shines FROM the menu
    //    sun disc's direction TOWARD Earth so the day/night terminator and
    //    the on-screen sun disc always agree. `looking_at` points the
    //    light's forward along its argument, so we pass the Earth→sun-disc
    //    direction negated (i.e. the direction light travels: sun → Earth).
    let sun_dir = -MENU_SUN_DIR.normalize();
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

    // ── Fill light: softer second directional from the camera-facing
    //    hemisphere so Earth's "dark" side reads instead of going
    //    pitch black. Position is computed at spawn time using the
    //    same base yaw/pitch as the camera, so it always fills from
    //    the same direction the camera looks from.
    let camera_dir_from_earth = {
        let rot = Quat::from_axis_angle(Vec3::Y, MENU_CAMERA_BASE_YAW)
            * Quat::from_axis_angle(Vec3::X, MENU_CAMERA_BASE_PITCH);
        // The camera position relative to Earth is `rot * -Z * radius`
        // (camera looks toward -Z). The fill light should shine FROM
        // that direction TOWARD Earth, so its forward direction is
        // the opposite.
        -(rot * Vec3::Z)
    };
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.55, 0.65, 0.85),
            illuminance: MENU_FILL_ILLUMINANCE,
            shadows_enabled: false,
            ..default()
        },
        Transform::default().looking_at(camera_dir_from_earth, Vec3::Y),
        MenuBackdropMarker,
        MenuBackdropKind::SunLight, // re-use SunLight kind; rotation system skips it
    ));

    // ── Menu-owned sun disc ───────────────────────────────────────────
    // A small unlit emissive sphere placed far along the sunlight direction
    // so the visible sun agrees with the directional light. Kept dim enough
    // to avoid blowing out the frame; the skybox provides the starfield.
    let sun_disc_pos = MENU_EARTH_POSITION + MENU_SUN_DIR.normalize() * MENU_SUN_DISTANCE;
    let sun_disc_mesh = meshes.add(Sphere::new(MENU_SUN_DISC_RADIUS).mesh().uv(48, 24));
    let sun_disc_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.95, 0.82),
        emissive: LinearRgba::rgb(1.0, 0.9, 0.7) * 2.0,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(sun_disc_mesh),
        MeshMaterial3d(sun_disc_material),
        Transform::from_translation(sun_disc_pos),
        Visibility::default(),
        MenuBackdropMarker,
        MenuBackdropKind::SunLight, // rotation system skips it
    ));

    // ── Ambient fill (cool blue tint, raised brightness) ──────────────
    // `GlobalAmbientLight` is the project-wide ambient resource used in
    // `src/main.rs:setup`. We replace it for the menu session; the
    // transition system restores the in-game value implicitly when the
    // AmbientLight resource is restored... actually we don't restore
    // it — the in-game `setup` system runs once at startup and the
    // resource stays. The menu's ambient replaces it; on menu exit the
    // gameplay view sees the menu-tuned ambient. This is acceptable
    // because gameplay uses per-entity lighting (sun, point lights) far
    // more than the global fill, and the difference (~8 lux vs ~0.6
    // brightness) is small relative to the per-entity contributions.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.06, 0.10, 0.18),
        brightness: MENU_AMBIENT_BRIGHTNESS,
        affects_lightmapped_meshes: true,
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
    mut camera_query: Query<(&mut OrbitCamera, &mut Transform), With<GameCamera>>,
) {
    if !active.0 {
        return;
    }
    let Ok((mut orbit, mut transform)) = camera_query.single_mut() else {
        return;
    };

    let t = time.elapsed_secs();
    let yaw_offset = (t * 0.04).sin() * 0.10;
    let pitch_offset = (t * 0.03).cos() * 0.03;

    orbit.target_center = MENU_EARTH_POSITION;
    orbit.pan_offset = Vec3::ZERO;
    orbit.radius = MENU_EARTH_DISTANCE;
    orbit.pitch = MENU_CAMERA_BASE_PITCH + pitch_offset;
    // Base yaw orbits the camera around Earth so Sol appears clearly
    // to the LEFT of the visible disc (well outside Earth's silhouette).
    // The parallax wobble layers on top for the breathing effect.
    orbit.yaw = MENU_CAMERA_BASE_YAW + yaw_offset;

    // `update_camera_transform` (src/plugins/camera.rs) is gated on
    // `LaunchState::is_in_game()`, so while the menu owns the camera nothing
    // else converts this `OrbitCamera` state into a real `Transform`. Write
    // it here, mirroring the exact same math the in-game pipeline uses, so
    // the menu framing (and the on-screen position of Sol) is what the
    // constants above actually describe.
    let rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
        * Quat::from_axis_angle(Vec3::X, orbit.pitch);
    let offset = rot * Vec3::Z * orbit.radius;
    let position = orbit.target_center + orbit.pan_offset + offset;
    transform.translation = position;
    transform.look_at(orbit.target_center + orbit.pan_offset, Vec3::Y);
}

/// Rotate the Earth surface + clouds at their respective rates, and
/// drive the Moon's orbit around Earth. Uses real `Time` so the menu
/// animation continues even when the simulation is paused
/// (`SimulationTime` doesn't advance during the launch flow).
fn rotate_menu_earth(
    active: Res<MenuBackdropActive>,
    time: Res<Time>,
    mut query: Query<(&MenuBackdropKind, &mut Transform), With<MenuBackdropMarker>>,
) {
    if !active.0 {
        return;
    }
    let dt = time.delta_secs();

    // First pass: spin Earth + clouds around their own Y axes.
    for (kind, mut transform) in query.iter_mut() {
        match kind {
            MenuBackdropKind::Earth => {
                transform.rotate_y(MENU_EARTH_YAW_DEG_PER_S.to_radians() * dt);
            }
            MenuBackdropKind::Clouds => {
                transform.rotate_y(MENU_EARTH_CLOUD_YAW_DEG_PER_S.to_radians() * dt);
            }
            // Moon orbital motion is computed below (independent Y
            // rotation isn't applied — the static albedo handles facing).
            MenuBackdropKind::Moon => {}
            // The directional light's `Transform` only matters for its
            // `look_at` direction — skip.
            MenuBackdropKind::SunLight => {}
        }
    }

    // Second pass: advance the Moon's orbital phase and recompute its
    // position around Earth. Phase advances monotonically so the Moon
    // never snaps or jitters, even after a long idle.
    let moon_phase = (time.elapsed_secs() * MENU_MOON_ORBIT_DEG_PER_S).to_radians();
    let moon_offset = Vec3::new(
        MENU_MOON_ORBIT_RADIUS * moon_phase.sin(),
        MENU_MOON_ORBIT_RADIUS * 0.15 * (moon_phase * 0.5).cos(), // slight vertical bob
        MENU_MOON_ORBIT_RADIUS * moon_phase.cos(),
    );
    for (kind, mut transform) in query.iter_mut() {
        if *kind == MenuBackdropKind::Moon {
            transform.translation = MENU_EARTH_POSITION + moon_offset;
        }
    }
}

/// Hide all in-game non-star bodies (Planets + Moons) while the menu
/// is active. We keep Sol + other stars visible so they appear as
/// distant background objects behind the menu Earth close-up — a
/// "deep-space establishing shot" composition.
///
/// The menu Earth is spawned at `MENU_EARTH_POSITION = (1500, 0, 0)`,
/// which is roughly 1 AU from the origin where Sol lives. The in-game
/// Earth + its Moon orbit Sol on Kepler trajectories and would
/// visually overlap our much larger menu Earth + Moon (radius 1_500
/// vs 63.7 for the planet, 350 vs small for the moon). Hiding every
/// non-star `CelestialBody` keeps just our menu Earth + Moon visible
/// while Sol continues to glow in the distance.
///
/// We don't blanket-hide every CelestialBody because the menu
/// presentation wants to show Sol as a small bright disc behind Earth
/// — that's the whole point of the new framing. Stars (BodyType::Star)
/// are kept visible.
///
/// This system runs every frame (not just on `LaunchState` change) so
/// the visibility flip applies to any bodies that spawn later.
fn hide_in_game_solar_system(
    active: Res<MenuBackdropActive>,
    launch_state: Res<LaunchState>,
    // Bevy 0.18's B0001 rule forbids two separate `Query<&mut Visibility>`
    // params that overlap on the same component. Collapse Planet + Moon
    // into a single `Or<...>` filter so the system uses one query.
    mut body_query: Query<
        &mut Visibility,
        (
            With<CelestialBody>,
            Or<(
                With<Planet>,
                With<crate::plugins::solar_system::Moon>,
                With<crate::plugins::solar_system::Star>,
            )>,
        ),
    >,
    // Star PointLights are children of the star entity (spawned in
    // `solar_system.rs`); they carry no CelestialBody marker of their own.
    mut star_light_query: Query<&mut PointLight, Without<CelestialBody>>,
) {
    let should_hide = active.0 && is_menu_launch_state(*launch_state);
    let target = if should_hide {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };

    for mut vis in body_query.iter_mut() {
        if *vis != target {
            *vis = target;
        }
    }

    // Lights ignore `Visibility`, so dim every star's PointLight while the
    // menu owns the scene. The in-game Sol's 2.8e11 lux source sits at the
    // origin and would otherwise light the menu Earth from the wrong
    // direction (fighting the menu's own directional sun) and leave a
    // bright glare disc embedded in Earth's limb — the "sun stuck in the
    // planet" artefact. The menu backdrop provides its own sun light + a
    // distant menu-owned sun disc, so the real Sol contributes nothing here.
    for mut light in star_light_query.iter_mut() {
        light.intensity = if should_hide { 0.0 } else { 2.8e11 };
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
        // The splash runs in its own Bevy app before this one
        // (see splash_standalone); the game app boots straight into
        // the main menu and never visits a `Splash` variant.
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
            let yaw = (t * 0.04).sin() * 0.10;
            let pitch = (t * 0.03).cos() * 0.03;
            assert!(yaw.abs() <= 0.101, "yaw {} out of band at t={}", yaw, t);
            assert!(pitch.abs() <= 0.031, "pitch {} out of band at t={}", pitch, t);
        }
    }
}