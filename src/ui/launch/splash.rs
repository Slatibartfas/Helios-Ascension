//! Splash render system + window setup.
//!
//! Architecture (post GRA-3xx PR-A):
//!
//! The splash lives in its own OS-level **Window** entity that the
//! Bevy `WindowPlugin` spawns at Startup. The main game window also
//! exists from the start but has `visible: false`; on splash
//! dismissal the splash window hides and the main window shows.
//! Both windows share a single Bevy `app.run()` because winit 0.30
//! forbids creating a second `EventLoop` after the first exits
//! (`RecreationAttempt` panic at `bevy_winit-0.18/src/lib.rs:128`).
//!
//! Bevy_egui multi-window setup follows the project's `two_windows`
//! pattern:
//!
//! 1. `EguiGlobalSettings::auto_create_primary_context = false` so
//!    bevy_egui doesn't auto-attach an `EguiContext` to whichever
//!    camera it sees first.
//! 2. The **main camera** carries the `PrimaryEguiContext` marker,
//!    so `EguiContexts::ctx_mut()` resolves to it.
//! 3. The **splash camera** carries
//!    `EguiMultipassSchedule::new(SplashContextPass)`, routing
//!    splash-only systems to its own egui context. Splash render
//!    systems use `EguiContexts::ctx_for_entity_mut(splash_cam)` to
//!    target the splash context explicitly.
//! 4. Both cameras target their respective windows via
//!    `RenderTarget::Window(WindowRef::Entity(...))`.
//!
//! Dismissal: the splash render system hides the splash window +
//! shows the main window. The system self-gates on splash-window
//! visibility, so once dismissed it stops rendering. The hidden native
//! window stays alive until application exit: removing a secondary
//! window at runtime causes winit on Windows to deliver final focus and
//! destruction events after Bevy has removed its WindowId mapping.

use bevy::camera::RenderTarget;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::window::{
    MonitorSelection, PrimaryWindow, Window, WindowLevel, WindowPosition, WindowRef,
    WindowResolution,
};
use bevy_egui::egui::{self, ColorImage, TextureHandle, TextureOptions};
use bevy_egui::{EguiContexts, EguiGlobalSettings, EguiMultipassSchedule, EguiStartupSet};
use image::GenericImageView;

use super::manifest::LaunchUiManifest;

/// Marker component on the splash window entity. Used by the
/// render system to find the splash window (for visibility gating)
/// and by the camera spawn system to wire up `RenderTarget`.
#[derive(Component)]
pub struct SplashWindow;

/// The splash window entity created during plugin construction.
/// Stored explicitly so the `PreStartup` camera setup can target it
/// without querying a deferred startup spawn.
#[derive(Resource, Debug, Clone, Copy)]
struct SplashWindowEntity(Entity);

/// Marker component on the splash camera. The render system uses
/// this to find the camera entity when calling
/// `EguiContexts::ctx_for_entity_mut(splash_cam_entity)`.
#[derive(Component)]
pub struct SplashCamera;

/// Marker attached to the splash camera when the splash dismisses. The
/// `Last`-schedule cleanup system despawns it only after bevy_egui's pass
/// loop has released its context borrows.
#[derive(Component)]
pub struct SplashCleanupPending;

/// Schedule label for splash-only egui rendering. The splash camera
/// attaches `EguiMultipassSchedule::new(SplashContextPass)`, so
/// systems registered in this schedule run on the splash egui
/// context. This mirrors bevy_egui's `two_windows` example pattern.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SplashContextPass;

/// Real-time elapsed since the splash screen first appeared (seconds).
///
/// Reset whenever the splash window becomes visible again (currently
/// never — splash runs once at startup). The render system uses this
/// together with `splash_min_duration_s` / `splash_max_duration_s`
/// to decide when to dismiss.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct SplashTimer(pub f32);

/// Cached egui texture handle for the splash logo. Loaded once on
/// the first frame the splash is observed; reused on subsequent
/// frames so we don't re-decode the PNG every tick.
#[derive(Resource, Default)]
pub struct SplashImage(pub Option<TextureHandle>);

/// Width / height of the splash window **before** the PNG is
/// decoded to set the real size. Matches the known
/// `assets/logo/logo_splashscreen.png` dimensions (1124 × 800 per
/// the RON manifest comment).
pub const PLACEHOLDER_W: u32 = 1124;
pub const PLACEHOLDER_H: u32 = 800;

/// Padding (px) added around the decoded image. The shipped
/// `logo_splashscreen.png` has its own backdrop, so the default is
/// zero; raise to add a visible theme-colored border.
pub const SPLASH_WINDOW_PADDING: u32 = 0;

/// Bevy plugin that owns the splash window + camera setup and
/// registers the splash render/dismissal systems.
///
/// Lives next to `LaunchPlugin` rather than inside it because the
/// splash must run **before** any other UI plugins boot (the main
/// window is hidden during the splash; if `LaunchPlugin` had the
/// splash, the main menu's render system would try to draw into a
/// hidden window).
pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        // Spawn the splash-owned ECS window before `app.run()`. Bevy
        // winit creates only `Added<Window>` entities on the event
        // loop's initial `resumed` callback; spawning from a startup
        // schedule is too late and can consume the change tick that
        // the native-window creation query relies on.
        let splash_window = app
            .world_mut()
            .spawn((splash_window_descriptor(), SplashWindow))
            .id();
        app.insert_resource(SplashWindowEntity(splash_window));

        app.init_resource::<SplashTimer>()
            .init_resource::<SplashImage>()
            // Configure egui and create the splash camera before its
            // context initialization set. The window itself already
            // exists in the world and will receive a native winit peer
            // together with the primary window on event-loop resume.
            .add_systems(
                PreStartup,
                setup_splash_camera.before(EguiStartupSet::InitContexts),
            )
            .add_systems(
                Startup,
                size_window_to_image.after(super::manifest::load_launch_ui_manifest),
            )
            .add_systems(SplashContextPass, ui_splash_system)
            // `Last` runs after `PreUpdate` (where the egui pass
            // loop iterates egui contexts) and after `Update` (where
            // dismissal tags the splash camera). Despawning it here
            // avoids invalidating a context borrow mid-pass.
            .add_systems(Last, cleanup_dismissed_splash);
    }
}

/// Despawn the splash camera after the egui pass loop has finished for
/// the dismissal frame. The hidden splash window remains alive until the
/// application exits so winit can retain its native WindowId mapping.
fn cleanup_dismissed_splash(
    mut commands: Commands,
    cleanup_pending: Query<Entity, With<SplashCleanupPending>>,
) {
    for entity in &cleanup_pending {
        commands.entity(entity).despawn();
    }
}

/// Build the splash window component before the app enters winit's
/// event loop. Keeping this separate also makes the startup timing
/// requirement explicit in `SplashPlugin::build`.
fn splash_window_descriptor() -> Window {
    Window {
        title: "Helios Ascension".to_string(),
        resolution: WindowResolution::new(PLACEHOLDER_W, PLACEHOLDER_H),
        decorations: false,
        resizable: false,
        window_level: WindowLevel::AlwaysOnTop,
        position: WindowPosition::Centered(MonitorSelection::Primary),
        visible: true,
        ..default()
    }
}

/// `PreStartup` system: spawn the splash camera and disable
/// bevy_egui's auto-create-primary-context before its `InitContexts`
/// startup set runs. The splash window was inserted directly into
/// the world during plugin construction so winit sees it on resume.
fn setup_splash_camera(
    mut commands: Commands,
    splash_window: Res<SplashWindowEntity>,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
) {
    // Disable bevy_egui's auto-attach so neither camera gets an
    // EguiContext implicitly. The main camera carries
    // `PrimaryEguiContext` (handled by the main CameraPlugin); the
    // splash camera carries `EguiMultipassSchedule::new(
    // SplashContextPass)`. bevy_egui sees these markers and creates
    // the contexts accordingly.
    egui_global_settings.auto_create_primary_context = false;

    // Splash camera: matches the bevy_egui 0.39 `two_windows`
    // example exactly (Camera3d, no `order` override). The example
    // pattern is known to render the egui context correctly.
    commands.spawn((
        Camera3d::default(),
        Camera::default(),
        RenderTarget::Window(WindowRef::Entity(splash_window.0)),
        EguiMultipassSchedule::new(SplashContextPass),
        SplashCamera,
    ));
}

/// Startup system: read the PNG from disk via the existing manifest
/// accessor, decode it to learn its real dimensions, and resize the
/// splash window to match. Runs after the manifest loader.
fn size_window_to_image(
    manifest: Res<LaunchUiManifest>,
    mut splash_windows: Query<&mut Window, With<SplashWindow>>,
) {
    let Some(bytes) = load_png_bytes_with_fallback(&manifest) else {
        // No PNG decodable — leave the placeholder in place. The
        // render path falls through to the centered text label.
        return;
    };
    let Ok(img) = image::load_from_memory(&bytes) else {
        return;
    };
    let (w, h) = img.dimensions();
    let padded_w = w + 2 * SPLASH_WINDOW_PADDING;
    let padded_h = h + 2 * SPLASH_WINDOW_PADDING;

    if let Ok(mut window) = splash_windows.single_mut() {
        window.resolution = WindowResolution::new(padded_w, padded_h);
    }
}

/// Public-facing system: render the splash, advance the timer,
/// dismiss on input or timeout. The dismissal action hides the
/// splash window and shows the main window. The system self-gates
/// on the splash window's visibility so it becomes a no-op once
/// dismissed.
///
/// Dismissal logic has three paths, in priority order:
///
/// 1. `force_skip_splash` (manifest kill-switch) — fires
///    immediately, no waiting.
/// 2. **Boot init complete + min duration elapsed** — primary
///    path. We don't dismiss while `BootState::Loading` is still
///    firing game-state init (solar system, baseline techs, debug
///    fleet, etc.) because the player would see those systems
///    continuing to populate the world after the splash is gone.
/// 3. **Max duration** — fallback. If boot-init hangs for some
///    reason, the splash force-dismisses after `splash_max_duration_s`
///    so the player isn't trapped behind a frozen splash.
/// 4. **Input + min duration** — early dismiss on any key/click,
///    letting impatient players skip the splash once the brand has
///    had its mandated on-screen time.
///
/// When `BootState::Loading`, a small "Loading…" label is painted
/// under the logo so the player can see the boot-init chain is in
/// progress. The label disappears on the same frame the splash
/// dismisses.
pub fn ui_splash_system(
    commands: Commands,
    mut contexts: EguiContexts,
    mut splash_window: Query<&mut Window, With<SplashWindow>>,
    mut main_window: Query<&mut Window, (With<PrimaryWindow>, Without<SplashWindow>)>,
    splash_cam: Query<Entity, With<SplashCamera>>,
    manifest: Res<LaunchUiManifest>,
    boot_state: Res<crate::boot_init::BootState>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut splash_timer: ResMut<SplashTimer>,
    mut splash_image: ResMut<SplashImage>,
    mut load_attempted: Local<bool>,
) {
    // Self-gate: stop rendering once the splash is dismissed. The
    // very last paint + the visibility flip happen on the dismissal
    // frame (see `dismiss_splash`); subsequent frames bail here.
    let Ok(splash_w_visible) = splash_window.single().map(|w| w.visible) else {
        return;
    };
    if !splash_w_visible {
        return;
    }

    // Honour the kill-switch first — saves one frame of painting.
    if manifest.force_skip_splash {
        dismiss_splash(commands, &mut splash_window, &mut main_window, &splash_cam);
        return;
    }

    // ── 1. Load texture on first frame (deferred to egui ctx) ─────
    if splash_image.0.is_none() && !*load_attempted {
        *load_attempted = true;
        if let Ok(splash_cam_entity) = splash_cam.single() {
            splash_image.0 = load_splash_texture(&mut contexts, &manifest, splash_cam_entity);
        }
    }

    // ── 2. Advance timer ──────────────────────────────────────────
    let dt = real_time.delta_secs();
    splash_timer.0 += dt;
    let elapsed = splash_timer.0;

    let min_s = manifest.splash_min_seconds();
    let max_s = manifest.splash_max_seconds();

    // ── 3. Dismiss check ──────────────────────────────────────────
    // Primary: boot-init done + min duration elapsed.
    if *boot_state == crate::boot_init::BootState::Ready && elapsed >= min_s {
        dismiss_splash(commands, &mut splash_window, &mut main_window, &splash_cam);
        return;
    }
    // Fallback: max duration.
    if elapsed >= max_s {
        dismiss_splash(commands, &mut splash_window, &mut main_window, &splash_cam);
        return;
    }
    // Early-dismiss on input after the min brand-display time.
    if elapsed >= min_s && first_input(&keyboard_input) {
        dismiss_splash(commands, &mut splash_window, &mut main_window, &splash_cam);
        return;
    }

    // ── 4. Render ─────────────────────────────────────────────────
    let Ok(splash_cam_entity) = splash_cam.single() else {
        return;
    };
    let Ok(ctx) = contexts.ctx_for_entity_mut(splash_cam_entity) else {
        return;
    };

    let still_loading = *boot_state == crate::boot_init::BootState::Loading;

    egui::CentralPanel::default()
        .frame(
            egui::Frame::default()
                .fill(crate::ui::theme::BG)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if let Some(tex) = splash_image.0.as_ref() {
                    let available = ui.available_size();
                    let aspect = tex.aspect_ratio();
                    // Reserve vertical space for the Loading label
                    // when boot-init is still running.
                    let reserve_h = if still_loading { 56.0 } else { 0.0 };
                    let draw_h = (available.y - reserve_h).max(64.0);
                    let max_w = draw_h.min(available.x) * aspect;
                    let size = egui::vec2(max_w, max_w / aspect);
                    ui.add_space((available.y - draw_h - reserve_h).max(0.0) * 0.5);
                    ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                    if still_loading {
                        ui.add_space(crate::ui::theme::Spacing::md);
                        ui.label(
                            egui::RichText::new("Loading…")
                                .color(crate::ui::theme::ACCENT)
                                .size(18.0)
                                .strong(),
                        );
                    }
                } else {
                    ui.label(
                        egui::RichText::new("HELIOS ASCENSION")
                            .color(crate::ui::theme::ACCENT)
                            .size(48.0)
                            .strong(),
                    );
                    if still_loading {
                        ui.add_space(crate::ui::theme::Spacing::md);
                        ui.label(
                            egui::RichText::new("Loading…")
                                .color(crate::ui::theme::ACCENT_DIM)
                                .size(18.0),
                        );
                    }
                }
            });
        });
}

/// Hide the splash window + show the main window. Idempotent —
/// calling twice doesn't double-flip visibility (both transitions
/// are no-ops on the second call).
///
/// The splash camera despawn is **deferred** to the `Last` schedule via
/// [`SplashCleanupPending`]. Despawning it during the egui pass can
/// invalidate bevy_egui's held context references. The hidden splash
/// window remains alive until application exit to avoid late native-window
/// events arriving after Bevy has discarded its WindowId mapping.
fn dismiss_splash(
    mut commands: Commands,
    splash_window: &mut Query<&mut Window, With<SplashWindow>>,
    main_window: &mut Query<&mut Window, (With<PrimaryWindow>, Without<SplashWindow>)>,
    splash_cam: &Query<Entity, With<SplashCamera>>,
) {
    if let Ok(mut main_w) = main_window.single_mut() {
        main_w.visible = true;
    }
    if let Ok(cam_entity) = splash_cam.single() {
        commands.entity(cam_entity).insert(SplashCleanupPending);
    }
    if let Ok(mut splash_w) = splash_window.single_mut() {
        splash_w.visible = false;
    }
}

/// True when the player has pressed any keyboard key. Mouse input
/// is delegated to egui's pointer detection (the egui context's
/// `input(|i| i.pointer.any_pressed())`).
///
/// We use `just_pressed` (not `pressed`) so holding a key down
/// doesn't repeatedly fire the dismiss.
fn first_input(keyboard_input: &Res<ButtonInput<KeyCode>>) -> bool {
    keyboard_input.get_just_pressed().next().is_some()
}

/// Decode the configured PNG (with `logo_clean` as fallback) into
/// an [`egui::ColorImage`], register it with the egui context via
/// `Context::load_texture`, and return the cached [`TextureHandle`].
/// Returns `None` on any decode / IO failure; the caller falls
/// back to a centered text label so the splash never paints blank.
fn load_splash_texture(
    contexts: &mut EguiContexts,
    manifest: &LaunchUiManifest,
    splash_cam_entity: Entity,
) -> Option<TextureHandle> {
    let bytes = load_png_bytes_with_fallback(manifest)?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let color_image = ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
    let ctx = contexts.ctx_for_entity_mut(splash_cam_entity).ok()?;
    Some(ctx.load_texture("splash_logo", color_image, TextureOptions::LINEAR))
}

/// Try the configured splashscreen path first, fall back to the
/// clean logo if the file is missing or corrupt. Both are LGD-verified
/// `assets/logo/*.png` files (see `assets/data/launch_ui.ron`).
pub(crate) fn load_png_bytes_with_fallback(manifest: &LaunchUiManifest) -> Option<Vec<u8>> {
    let primary = std::fs::read(manifest.splash_image_path());
    match primary {
        Ok(bytes) => match image::load_from_memory(&bytes) {
            Ok(_) => Some(bytes),
            Err(e) => {
                warn!(
                    "splash: primary asset {:?} failed to decode ({}); falling back to {:?}",
                    manifest.splash_image_path(),
                    e,
                    manifest.clean_image_path()
                );
                std::fs::read(manifest.clean_image_path()).ok()
            }
        },
        Err(e) => {
            warn!(
                "splash: primary asset {:?} missing ({}); falling back to {:?}",
                manifest.splash_image_path(),
                e,
                manifest.clean_image_path()
            );
            std::fs::read(manifest.clean_image_path()).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::LaunchUiManifest;

    /// Sanity: the manifest accessor for the splash image path
    /// returns a value rooted under `assets/` and ending in `.png`.
    #[test]
    fn splash_image_path_round_trips_through_manifest() {
        let manifest = LaunchUiManifest::default();
        let p = manifest.splash_image_path();
        assert!(
            p.starts_with("assets/"),
            "splash image path must be rooted under assets/ (got {:?})",
            p
        );
        assert!(
            p.ends_with(".png"),
            "splash image must be PNG (got {:?})",
            p
        );
    }

    #[test]
    fn splash_timer_default_is_zero() {
        let t = SplashTimer::default();
        assert_eq!(t.0, 0.0);
    }

    #[test]
    fn splash_image_default_is_none() {
        let i = SplashImage::default();
        assert!(i.0.is_none());
    }

    /// Empty keyboard input isn't a dismiss.
    #[test]
    fn empty_keyboard_input_is_not_a_dismiss() {
        let keys: ButtonInput<KeyCode> = ButtonInput::default();
        assert!(keys.get_just_pressed().next().is_none());
    }
}
