//! `Window::mode` ← `PersistentSettings` bridge.
//!
//! The Settings subview's graphics tab (in
//! [`crate::ui::launch::subview_settings`]) writes
//! `PersistentSettings::window_mode` whenever the player picks a
//! mode. This plugin's `apply_window_mode_to_primary` system reads
//! that resource and pushes the value to `Window::mode` on the
//! primary window so the actual window mode changes immediately.
//!
//! ## Update vs change-detection
//!
//! The system runs every frame (not gated on `is_changed()`) because
//! the assignment is cheap and the resilient behavior is what we
//! want: a window that accidentally lost its mode (e.g. via a
//! minimize/restore flicker) snaps back to the player-intended mode
//! on the next frame without requiring the player to re-open the
//! settings subview. The empty-handle bail-out (`Query::single_mut`
//! returns `Err` only when the primary window is missing or there
//! is more than one) keeps the cost negligible.
//!
//! ## Schedule
//!
//! `Update` is the natural slot: `PersistentSettings` is read once
//! per frame, after the launch subview has had a chance to mutate
//! it during the same frame (the egui subview lives in
//! `EguiPrimaryContextPass`, which runs before `Update` in Bevy 0.18).
//!
//! The system is generic over `bevy::window::WindowMode`; the
//! translation from `PersistentWindowMode` lives in
//! [`crate::ui::launch::userdata`] so the two-step conversion can be
//! tested independently of the Bevy world.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, Window};

use crate::ui::launch::userdata::PersistentSettings;

/// Plugin that registers `apply_window_mode_to_primary` in `Update`.
///
/// `LaunchPlugin` calls [`register_window_mode_bridge`] at build
/// time. The plugin lives in `src/plugins/` because it's a thin
/// ECS plumbing layer — no egui, no asset IO, no UI state.
pub struct WindowModeBridgePlugin;

impl Plugin for WindowModeBridgePlugin {
    fn build(&self, app: &mut App) {
        register_window_mode_bridge(app);
    }
}

/// Register the bridge system. Exposed separately so callers who
/// don't want the plugin wrapper (e.g. tests) can splice the
/// system into an existing app without going through the plugin
/// lifecycle.
pub fn register_window_mode_bridge(app: &mut App) {
    app.add_systems(Update, apply_window_mode_to_primary);
}

/// Read `PersistentSettings::window_mode` and apply it to the
/// primary window's `Window::mode`.
///
/// `Window::mode` is a `Clone`able enum with `PartialEq` (Bevy
/// 0.18), so the equality check is a cheap structural compare and
/// the assignment is a plain field write — no winit calls happen
/// here. `bevy_winit::changed_windows` (in `Last`) is the actual
/// bridge to the winit backend; see `bevy_winit-0.18.0/src/system.rs`.
fn apply_window_mode_to_primary(
    settings: Res<PersistentSettings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let desired: bevy::window::WindowMode = settings.window_mode.into();
    if window.mode != desired {
        window.mode = desired;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::userdata::PersistentWindowMode;

    /// Build a minimal `World` with a `PersistentSettings` resource
    /// and a single primary window attached. The window's `mode`
    /// defaults to `Windowed` so the bridge has a known baseline.
    fn world_with_primary_window() -> World {
        let mut world = World::new();
        world.insert_resource(PersistentSettings::default());
        world.spawn((Window::default(), PrimaryWindow));
        world
    }

    #[test]
    fn applies_windowed_mode_to_primary_window() {
        let mut world = world_with_primary_window();
        let mut settings = world.resource_mut::<PersistentSettings>();
        settings.window_mode = PersistentWindowMode::Windowed;

        // Run the system once via `Schedule` to avoid pulling in
        // the full `App` (and therefore `DefaultPlugins`).
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_window_mode_to_primary);
        schedule.run(&mut world);

        let window = world
            .query::<&Window>()
            .iter(&world)
            .next()
            .expect("primary window must exist");
        assert_eq!(window.mode, bevy::window::WindowMode::Windowed);
    }

    #[test]
    fn applies_fullscreen_mode_to_primary_window() {
        let mut world = world_with_primary_window();
        let mut settings = world.resource_mut::<PersistentSettings>();
        settings.window_mode = PersistentWindowMode::Fullscreen;

        let mut schedule = Schedule::default();
        schedule.add_systems(apply_window_mode_to_primary);
        schedule.run(&mut world);

        let window = world
            .query::<&Window>()
            .iter(&world)
            .next()
            .expect("primary window must exist");
        // The `From` impl maps `Fullscreen` to
        // `WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)`.
        // We only check the variant tag here so the monitor-selection
        // bound types don't leak into the test.
        assert!(matches!(
            window.mode,
            bevy::window::WindowMode::Fullscreen(_, _)
        ));
    }

    #[test]
    fn applies_borderless_fullscreen_mode_to_primary_window() {
        let mut world = world_with_primary_window();
        let mut settings = world.resource_mut::<PersistentSettings>();
        settings.window_mode = PersistentWindowMode::BorderlessFullscreen;

        let mut schedule = Schedule::default();
        schedule.add_systems(apply_window_mode_to_primary);
        schedule.run(&mut world);

        let window = world
            .query::<&Window>()
            .iter(&world)
            .next()
            .expect("primary window must exist");
        assert!(matches!(
            window.mode,
            bevy::window::WindowMode::BorderlessFullscreen(_)
        ));
    }

    /// When the persisted mode already matches the window's mode,
    /// the system must run without panicking. The equality check
    /// inside the system early-outs so the actual `Window::mode`
    /// field is untouched.
    #[test]
    fn no_op_when_mode_already_matches() {
        let mut world = world_with_primary_window();
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_window_mode_to_primary);
        schedule.run(&mut world);

        let window = world
            .query::<&Window>()
            .iter(&world)
            .next()
            .expect("primary window must exist");
        assert_eq!(window.mode, bevy::window::WindowMode::Windowed);
    }

    /// The system is robust to a missing `PersistentSettings` resource
    /// (the system-param `Res<PersistentSettings>` would refuse to
    /// build the world in that case, but the `App::add_systems` path
    /// panics before the system runs). We don't directly test that
    /// panic — instead we verify the primary-handle-missing path is
    /// a no-op.
    #[test]
    fn no_op_when_primary_window_missing() {
        let mut world = World::new();
        world.insert_resource(PersistentSettings::default());
        // Intentionally do NOT spawn a primary window.
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_window_mode_to_primary);
        schedule.run(&mut world);
        // Reaching here without panicking is the assertion.
    }
}
