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
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_egui::egui::{self, ColorImage};
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiMultipassSchedule, EguiStartupSet, EguiTextureHandle,
};

/// Raw decoded splash pixels, decoded synchronously during plugin
/// construction so the very first painted frame can show the logo.
///
/// The decode used to happen lazily inside the splash egui system on the
/// first rendered frame. Because the heavy boot-init chain (solar-system
/// spawn, 60-system population, resource generation) now runs behind the
/// splash in `Update`, the first egui frame could be delayed by hundreds of
/// milliseconds — leaving the splash window painted solid black for the
/// whole boot. Decoding here (plugin `build`, before `app.run()`) means the
/// pixels are ready before winit ever shows the window.
///
/// We store both the raw [`ColorImage`] (for the window-size helper) and,
/// once the splash render system runs, a Bevy `Handle<Image>` registered in
/// `Assets<Image>` — the latter is handed to `EguiContexts::add_image`,
/// which produces a `TextureId` that Bevy's egui render node draws reliably
/// across **all** egui contexts (including this secondary splash context).
/// The previous `ctx.load_texture` path produced a context-local
/// `TextureHandle` that the splash's multi-pass egui node never painted,
/// which is why the splash showed solid black even though the PNG decoded.
#[derive(Resource, Clone)]
pub struct SplashImageData(pub Option<ColorImage>);

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

        // Decode the splash PNG now — before `app.run()` — so the first
        // painted frame already has the pixels. This eliminates the black
        // flash that used to appear while the boot-init chain blocked the
        // first egui frame. The decode is synchronous disk IO (one PNG,
        // ~1 MB); it's fast and happens before the window is shown.
        let splash_pixels = decode_splash_color_image();
        info!(
            "splash: decoded logo at build = {}",
            splash_pixels
                .as_ref()
                .map(|c| format!("{}x{}", c.width(), c.height()))
                .unwrap_or_else(|| "FAILED".to_string())
        );

        app.init_resource::<SplashTimer>()
            .insert_resource(SplashImageData(splash_pixels))
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
    splash_pixels: Res<SplashImageData>,
    mut images: ResMut<Assets<Image>>,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
) {
    // Disable bevy_egui's auto-attach so neither camera gets an
    // EguiContext implicitly. The main camera carries
    // `PrimaryEguiContext` (handled by the main CameraPlugin); the
    // splash camera carries `EguiMultipassSchedule::new(
    // SplashContextPass)`. bevy_egui sees these markers and creates
    // the contexts accordingly.
    egui_global_settings.auto_create_primary_context = false;

    // A 2D camera is intentional here: `Sprite` is a camera-facing quad,
    // so the splash is independent of the 3D camera projection and cannot
    // disappear because of a near/far-plane or face-orientation mismatch.
    // Camera3d gives the splash window a render target + camera entity
    // and lets the splash window clear with our background color. The
    // in-game world meshes are hidden by `menu_backdrop` while the
    // splash is up (no Sun/planet visible behind the logo), so the
    // only thing the player sees on the splash is what the splash
    // egui system paints via the secondary multipass egui context
    // (`EguiMultipassSchedule::new(SplashContextPass)`) on this camera.
    // The actual logo artwork is registered via
    // `EguiContexts::add_image` in `setup_splash_camera` and drawn as
    // a fullscreen egui image by `ui_splash_system`. This sidesteps
    // the Camera2d + Sprite + render-layer ordering that was painting
    // the splash solid black.
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.02, 0.02, 0.03)),
            ..default()
        },
        RenderTarget::Window(WindowRef::Entity(splash_window.0)),
        EguiMultipassSchedule::new(SplashContextPass),
        SplashCamera,
    ));

    // ── Splash logo (egui texture) ────────────────────────────────────
    // The PNG is decoded into a Bevy `Image` asset and registered with
    // `EguiContexts::add_image` so it shows up as an `egui::TextureId`
    // in the splash context. Drawing the logo via egui sidesteps the
    // Camera2d sprite + render-layer + sprite-pipeline ordering that
    // was painting the splash solid black.
    if let Some(pixels) = splash_pixels.0.as_ref() {
        let bevy_image = Image::new(
            Extent3d {
                width: pixels.width() as u32,
                height: pixels.height() as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.as_raw().to_vec(),
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        let handle = images.add(bevy_image);
        commands.insert_resource(SplashLogoImage(handle));
        info!(
            "splash: registered logo image ({}x{})",
            pixels.width(),
            pixels.height()
        );
    } else {
        warn!("splash: no decoded pixels; logo image not registered");
    }
}

/// `Handle<Image>` for the decoded splash PNG, registered with
/// `EguiContexts::add_image` so it shows up as an `egui::TextureId` in
/// the splash's secondary multipass egui context.
#[derive(Resource, Clone)]
pub struct SplashLogoImage(pub Handle<Image>);

/// Startup system: read the PNG from disk via the existing manifest
/// accessor, decode it to learn its real dimensions, and resize the
/// splash window to match. Runs after the manifest loader.
fn size_window_to_image(
    splash_pixels: Option<Res<SplashImageData>>,
    mut splash_windows: Query<&mut Window, With<SplashWindow>>,
) {
    // Prefer the pixels decoded at plugin build. Fall back to nothing
    // (leave the placeholder size) when no decodable PNG was found.
    let Some(image_data) = splash_pixels.and_then(|d| d.0.clone()) else {
        return;
    };
    let (w, h) = (image_data.width() as u32, image_data.height() as u32);
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
    splash_logo_image: Option<Res<SplashLogoImage>>,
    manifest: Res<LaunchUiManifest>,
    boot_state: Res<crate::boot_init::BootState>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut splash_timer: ResMut<SplashTimer>,
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

    // The logo itself is rendered as a Bevy `Sprite` in the splash camera's
    // 3D view (see `setup_splash_camera`), NOT via egui — the splash's
    // secondary multipass egui context does not reliably draw user textures,
    // which is why egui-based attempts painted solid black. This egui panel
    // only draws the small "Loading…" label over the top.

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

    // Register the splash PNG with the splash egui context up-front so
    // the painter inside the CentralPanel closure can paint it without
    // borrowing `contexts` again (the closure already holds a mutable
    // borrow on `ctx`). `EguiContexts::add_image` is the bevy_egui API
    // that produces a context-stable `TextureId` registered in the
    // global `EguiUserTextures` resource; unlike `ctx.load_texture` it
    // doesn't depend on the per-context texture registry, which is what
    // the secondary multipass context couldn't honour.
    let logo_texture_id = splash_logo_image.as_ref().map(|img| {
        contexts.add_image(EguiTextureHandle::Strong(img.0.clone()))
    });

    let Ok(ctx) = contexts.ctx_for_entity_mut(splash_cam_entity) else {
        return;
    };

    let still_loading = *boot_state == crate::boot_init::BootState::Loading;

    // CRITICAL: `Frame::NONE` (NOT `Frame::default()`) — a default
    // frame paints an opaque dark background, which would cover the
    // splash artwork. The logo image paints only where its pixels
    // exist, so the background shows through everywhere else.
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            if let Some(texture_id) = logo_texture_id {
                let rect = ui.max_rect();
                ui.painter().image(
                    texture_id,
                    rect,
                    egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::Vec2::new(1.0, 1.0),
                    ),
                    egui::Color32::WHITE,
                );
            }
            if still_loading {
                let rect = ui.max_rect();
                ui.painter().text(
                    egui::pos2(rect.center().x, rect.max.y - 28.0),
                    egui::Align2::CENTER_CENTER,
                    "Loading…",
                    egui::FontId::proportional(18.0),
                    crate::ui::theme::ACCENT,
                );
            }
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

/// Decode the configured splash PNG (with the clean logo as fallback) into
/// an [`egui::ColorImage`]. Called once during plugin construction so the
/// pixels are ready before the first frame. Returns `None` on any decode /
/// IO failure; the caller falls back to a centered text label so the splash
/// never paints blank.
///
/// Reads the same paths the manifest declares (`assets/data/launch_ui.ron`)
/// via [`LaunchUiManifest::default`], which is the manifest state at plugin
/// build time. The real manifest resource is loaded later at `Startup`; the
/// two only differ if a runtime override mutates them, which the launch flow
/// never does for the logo paths.
fn decode_splash_color_image() -> Option<ColorImage> {
    let manifest = LaunchUiManifest::default();
    let bytes = load_png_bytes_with_fallback(&manifest)?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    Some(ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw()))
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

    /// Empty keyboard input isn't a dismiss.
    #[test]
    fn empty_keyboard_input_is_not_a_dismiss() {
        let keys: ButtonInput<KeyCode> = ButtonInput::default();
        assert!(keys.get_just_pressed().next().is_none());
    }
}
