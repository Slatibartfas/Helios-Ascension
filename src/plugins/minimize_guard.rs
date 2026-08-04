//! DX12-safe path for window minimize (Bevy 0.18 surface-loss fix).
//!
//! ## The crash
//!
//! On Windows + DX12, minimizing the game window causes Bevy 0.18
//! to receive a `WindowResized` of `(0, 0)` from winit. The render
//! world, running in `Render`, then calls `surface.configure(0, 0)`
//! on the D3D12 swap chain. DX12 responds with `0x887A0001`
//! ("window is in use"), Bevy treats the wgpu error as fatal, and
//! the game panics in `bevy_render::view::window::create_surfaces`
//! followed by `prepare_windows`. This is a known regression on
//! top of [bevyengine/bevy#15077](https://github.com/bevyengine/bevy/issues/15077)
//! and tracks against [bevyengine/bevy#22225](https://github.com/bevyengine/bevy/issues/22225)
//! (closed via PR #22254 but not yet in the 0.18.0 release on
//! crates.io).
//!
//! ## The fix
//!
//! Intercept the 0×0 resize **before** the render world extracts.
//! The natural hook is `Last`:
//!
//! 1. `bevy_winit`'s `changed_windows` (registered in `Last` by
//!    `WinitPlugin::build`) writes a `WindowResized` message and
//!    mutates `Window::resolution` to the bad 0×0 value.
//! 2. Our `stabilize_minimized_window` system runs in the same
//!    `Last` schedule, after `changed_windows` thanks to
//!    registration order (bevy_winit is added first by
//!    `DefaultPlugins`, our plugin is added later from
//!    `LaunchPlugin::build`). It reads `WindowResized`, and when
//!    the new size is 0×0 it restores the last known non-zero
//!    resolution from a one-off `MinimizeGuard` resource.
//! 3. The render world then extracts `Window::resolution` for the
//!    next frame's `surface.configure` call — and the value is now
//!    a sane non-zero size, so DX12 is happy.
//!
//! The fix doesn't try to track a "minimized" flag on `Window` —
//! Bevy 0.18's `Window` struct has no observable `minimized` field
//! (only the request-side `Window::set_minimized()` method, which
//! writes `internal.minimize_request`). The cache + size rewrite
//! is the actual fix; the OS still minimizes the window because
//! we don't touch `Window::visible` (the winit side of the
//! minimize is invisible to ECS code on Bevy 0.18).
//!
//! The system is idempotent and cheap (a single `MessageReader`
//! drain + an optional `Window::resolution` write), so leaving it
//! running every frame has no measurable cost.
//!
//! ## Fallback for the first minimize
//!
//! If the very first event we see is a 0×0 resize (no non-zero
//! baseline has been cached yet), we fall back to the
//! `Window::resize_constraints.min_width/min_height` defined in
//! `main.rs` (1280×720). After that one frame the cache is
//! populated and subsequent minimize events restore the real
//! pre-minimize size.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, Window, WindowResized, WindowResolution};

use crate::plugins::window_constants::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};

/// Plugin that registers `stabilize_minimized_window` in `Last` so
/// it runs after `bevy_winit::changed_windows` (which writes the
/// `WindowResized` message and updates `Window::resolution`).
///
/// Registered from `LaunchPlugin::build` (see
/// `src/ui/launch/mod.rs`).
pub struct MinimizeGuardPlugin;

impl Plugin for MinimizeGuardPlugin {
    fn build(&self, app: &mut App) {
        register_minimize_guard(app);
    }
}

/// Register the minimize-guard system + the cache resource.
///
/// Exposed separately so tests can splice the system into a
/// scratch `App` without dragging in the full plugin.
pub fn register_minimize_guard(app: &mut App) {
    app.init_resource::<MinimizeGuard>()
        .add_systems(Last, stabilize_minimized_window);
}

/// Cached "last known non-zero resolution" for the primary window,
/// used by [`stabilize_minimized_window`] to restore the size when
/// the OS reports a 0×0 resize (minimize).
///
/// `None` on the very first run; the system populates it on the
/// first non-zero resize and reuses it for every subsequent
/// minimize. The resource is per-app, not per-window: the
/// primary window is the only target for the guard.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct MinimizeGuard(pub Option<(u32, u32)>);

/// Clamp a `WindowResized` payload to the primary window's last
/// known non-zero size, or the configured minimum if no baseline
/// has been cached yet.
///
/// Reads `MessageReader<WindowResized>` so it sees the message
/// exactly once across many frames. Mutates only the primary
/// window (the splash window is unaffected — it has its own
/// `SplashWindow` marker and is hidden after dismissal).
fn stabilize_minimized_window(
    mut messages: MessageReader<WindowResized>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut guard: ResMut<MinimizeGuard>,
) {
    let Ok((primary_entity, mut window)) = windows.single_mut() else {
        return;
    };
    for msg in messages.read() {
        // Only the primary window matters here. The secondary
        // splash window (which has its own `WindowResized` events
        // on startup) is ignored.
        if msg.window != primary_entity {
            continue;
        }

        let is_zero = msg.width <= 0.0 || msg.height <= 0.0;
        if is_zero {
            // Restore the cached size, or fall back to the
            // configured minimum if no baseline has been observed
            // yet. The fallback is essentially never hit in
            // practice (the player sees the splash before they
            // ever minimize the main window), but keeping it
            // means the first 0×0 event is safe.
            let (lw, lh) = guard
                .0
                .unwrap_or((MIN_WINDOW_WIDTH as u32, MIN_WINDOW_HEIGHT as u32));
            window.resolution = WindowResolution::new(lw, lh);
        } else {
            let w = msg.width.max(MIN_WINDOW_WIDTH) as u32;
            let h = msg.height.max(MIN_WINDOW_HEIGHT) as u32;
            guard.0 = Some((w, h));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::WindowResized;

    /// Build a `World` with a `MinimizeGuard` resource and a single
    /// primary `Window` whose resolution is the supplied size.
    /// Pre-registers the `WindowResized` message so the system can
    /// drain it without panicking.
    fn world_with_window(width: u32, height: u32) -> World {
        let mut world = World::new();
        world.init_resource::<MinimizeGuard>();
        world.init_resource::<Messages<WindowResized>>();
        world.spawn((
            Window {
                resolution: WindowResolution::new(width, height),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        world
    }

    /// Run the system against a one-tick schedule. The schedule is
    /// a plain `Schedule::default()` so the test doesn't depend on
    /// the `Last` schedule label.
    fn run_once(world: &mut World) {
        let mut schedule = Schedule::default();
        schedule.add_systems(stabilize_minimized_window);
        schedule.run(world);
    }

    fn primary_entity(world: &mut World) -> Entity {
        world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(world)
            .unwrap()
    }

    fn send_resize(world: &mut World, target: Entity, width: f32, height: f32) {
        world
            .resource_mut::<Messages<WindowResized>>()
            .write(WindowResized {
                window: target,
                width,
                height,
            });
    }

    #[test]
    fn zero_resize_is_replaced_with_cached_size() {
        let mut world = world_with_window(1920, 1080);
        let primary = primary_entity(&mut world);

        // First, drive a non-zero resize so the cache is populated.
        send_resize(&mut world, primary, 1280.0, 720.0);
        run_once(&mut world);

        let guard = world.resource::<MinimizeGuard>();
        assert_eq!(guard.0, Some((1280, 720)));

        // Now feed a 0x0 resize (the minimize signal). The system
        // must restore the cached 1280x720 — that's the
        // crash-avoiding part.
        send_resize(&mut world, primary, 0.0, 0.0);
        run_once(&mut world);

        let mut query = world.query::<&Window>();
        let window = query.iter(&world).next().expect("window exists");
        assert_eq!(window.resolution.physical_width(), 1280);
        assert_eq!(window.resolution.physical_height(), 720);
    }

    #[test]
    fn zero_resize_before_any_baseline_falls_back_to_minimum() {
        // No non-zero resize has been observed yet, so `guard`
        // is `None`. The system must fall back to the configured
        // `MIN_WINDOW_WIDTH/HEIGHT` (1280×720) rather than panicking.
        let mut world = world_with_window(1920, 1080);
        let primary = primary_entity(&mut world);

        send_resize(&mut world, primary, 0.0, 0.0);
        run_once(&mut world);

        let mut query = world.query::<&Window>();
        let window = query.iter(&world).next().expect("window exists");
        assert_eq!(window.resolution.physical_width(), MIN_WINDOW_WIDTH as u32);
        assert_eq!(window.resolution.physical_height(), MIN_WINDOW_HEIGHT as u32);
    }

    #[test]
    fn non_zero_resize_updates_cache() {
        let mut world = world_with_window(1920, 1080);
        let primary = primary_entity(&mut world);

        // Seed with a 0x0 resize so the cache is `None`. The
        // system restores the window to the fallback minimum
        // (1280×720) since no baseline has been observed yet.
        send_resize(&mut world, primary, 0.0, 0.0);
        run_once(&mut world);

        // Now drive a non-zero resize (the un-minimize path). The
        // OS / `bevy_winit` will already have updated
        // `Window::resolution` to the new size before the message
        // arrives; the system just records the size in the cache.
        send_resize(&mut world, primary, 1920.0, 1080.0);
        run_once(&mut world);

        let guard = world.resource::<MinimizeGuard>();
        assert_eq!(
            guard.0,
            Some((1920, 1080)),
            "cache must record the last non-zero resize"
        );
    }

    #[test]
    fn ignores_resize_for_non_primary_window() {
        // The splash window is a separate `Window` entity. Its
        // `WindowResized` events must not be drained by the
        // guard, so the primary window's cache stays untouched.
        let mut world = world_with_window(1920, 1080);
        let primary = primary_entity(&mut world);
        let splash = world.spawn(Window::default()).id();

        send_resize(&mut world, splash, 0.0, 0.0);
        run_once(&mut world);

        let guard = world.resource::<MinimizeGuard>();
        assert!(guard.0.is_none(), "primary cache must not be touched");
        let mut query = world.query::<(Entity, &Window)>();
        let (_, window) = query
            .iter(&world)
            .find(|(e, _)| *e == primary)
            .expect("primary window must exist");
        assert_eq!(
            window.resolution.physical_width(),
            1920,
            "primary window must not be resized"
        );
    }

    /// The public re-exports must be reachable so the tests can
    /// exercise them. Without this, a missing `pub` would slip
    /// through.
    #[test]
    fn plugin_and_resource_are_public() {
        let _ = MinimizeGuardPlugin;
        let _ = MinimizeGuard(None);
    }
}
