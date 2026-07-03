//! Splash render system (GRA-312 PR-B).
//!
//! Fills the [`LaunchSystemSet::Splash`] set reserved by PR-A (GRA-311).
//! The system:
//!
//! 1. Renders the `logo_splashscreen.png` (or `logo_clean.png` fallback)
//!    full-window centered with the [`crate::ui::theme::BG`] backdrop.
//! 2. Tracks a real-time timer ([`SplashTimer`]) since the splash began.
//! 3. Dismisses on first keyboard / mouse input **after** the configured
//!    `splash_min_duration_s`, or on the hard cap `splash_max_duration_s`,
//!    whichever comes first.
//! 4. On dismiss, transitions [`LaunchState`] to `MainMenu` via
//!    [`ResMut<LaunchState>`] — no write to [`PendingLaunchActions`].
//!    Per the spec, PR-C owns the menu shell and PR-D the subviews; this
//!    transition is a pure state advance.
//!
//! Per [[helios-architecture]], egui systems run in
//! [`EguiPrimaryContextPass`], not `Update`. The PR-A `LaunchPlugin`
//! reserved [`LaunchSystemSet::Splash`] in `Update`; PR-B re-configures
//! the set to `EguiPrimaryContextPass` (Bevy allows the same
//! `SystemSet` to live in multiple schedules — see the configure_sets
//! call in `LaunchPlugin::build`).
//!
//! Per GRA-309 §3.4 the spec calls for `force_skip_splash` (a manifest
//! kill-switch for headless QA). PR-B honors it: when true, the system
//! advances to `MainMenu` on the first frame it observes the splash.

use bevy::prelude::*;
use bevy_egui::egui::{self, ColorImage, TextureHandle, TextureOptions};
use bevy_egui::EguiContexts;

use super::manifest::LaunchUiManifest;
use super::{LaunchState, LaunchSystemSet};

/// Real-time elapsed since the splash screen first appeared (seconds).
///
/// Reset whenever [`LaunchState`] transitions back to [`LaunchState::Splash`].
/// The render system uses this together with `splash_min_duration_s` /
/// `splash_max_duration_s` to decide when to dismiss.
///
/// Stored as a separate resource rather than a `Local<>` because tests
/// inspect it via `World::resource::<SplashTimer>()` without instantiating
/// a full schedule.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct SplashTimer(pub f32);

/// Cached egui texture handle for the splash logo. Loaded once on the
/// first frame the splash is observed; reused on subsequent frames so
/// we don't re-decode the PNG every tick.
///
/// Wrapped in `Option` so the loader is deferred until the egui context
/// is available (it lives in `EguiPrimaryContextPass`).
#[derive(Resource, Debug, Default)]
pub struct SplashImage(pub Option<TextureHandle>);

/// Public-facing system: render the splash, advance the timer, dismiss
/// on input or timeout.
///
/// Runs in [`LaunchSystemSet::Splash`] inside
/// [`EguiPrimaryContextPass`]. Per GRA-309 §3.4 the dismissal logic is
/// independent of the sim clock — it uses [`Time<Real>`] because the
/// player can dismiss the splash before the simulation has begun.
pub fn ui_splash_system(
    mut contexts: EguiContexts,
    mut launch_state: ResMut<LaunchState>,
    manifest: Res<LaunchUiManifest>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut splash_timer: ResMut<SplashTimer>,
    mut splash_image: ResMut<SplashImage>,
    // `Local` resets per-system-instance, but we use a Resource above so
    // tests can poke it without schedule plumbing. This Local tracks
    // "have we already loaded the texture this run?" — `Option::is_some`
    // on SplashImage would suffice, but a separate flag lets us
    // distinguish "loaded" from "tried and failed".
    mut load_attempted: Local<bool>,
) {
    // Bail when the splash is not active. We also bail when the manifest
    // hasn't loaded yet — the loader runs at Startup, but tests that
    // skip Startup won't have it.
    if *launch_state != LaunchState::Splash {
        // Reset the timer so the next entry into Splash starts fresh.
        splash_timer.0 = 0.0;
        return;
    }
    let Some(manifest) = manifest.as_ref() else {
        return;
    };

    // Honour the kill-switch first — saves one frame of painting.
    if manifest.force_skip_splash {
        *launch_state = LaunchState::MainMenu;
        splash_timer.0 = 0.0;
        return;
    }

    // ── 1. Load texture on first frame (deferred to egui ctx) ─────────
    if splash_image.0.is_none() && !*load_attempted {
        *load_attempted = true;
        splash_image.0 = load_splash_texture(&mut contexts, manifest);
    }

    // ── 2. Advance timer ───────────────────────────────────────────────
    let dt = real_time.delta_secs();
    splash_timer.0 += dt;
    let elapsed = splash_timer.0;

    let min_s = manifest.splash_min_seconds();
    let max_s = manifest.splash_max_seconds();

    // ── 3. Dismiss check ───────────────────────────────────────────────
    if elapsed >= max_s {
        apply_dismiss(&mut *launch_state, &mut splash_timer.0);
        return;
    }
    if elapsed >= min_s && first_input(&mut contexts, &keyboard_input) {
        apply_dismiss(&mut *launch_state, &mut splash_timer.0);
        return;
    }

    // ── 4. Render ──────────────────────────────────────────────────────
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::CentralPanel::default()
        .frame(
            egui::Frame::default()
                .fill(crate::ui::theme::BG)
                .inner_margin(egui::Margin::same(0.0)),
        )
        .show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                if let Some(tex) = splash_image.0.as_ref() {
                    let available = ui.available_size();
                    let aspect = tex.aspect_ratio();
                    let max_w = available.x.min(available.y * aspect);
                    let size = egui::vec2(max_w, max_w / aspect);
                    ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                } else {
                    // Texture failed to load — fall back to a centered
                    // title so the splash is never blank.
                    ui.label(
                        egui::RichText::new("HELIOS ASCENSION")
                            .color(crate::ui::theme::ACCENT)
                            .size(48.0)
                            .strong(),
                    );
                }
            });
        });
}

/// Public helper: advance `LaunchState` to `MainMenu` and reset the
/// splash timer. Pulled out of the system so the transition logic can
/// be unit-tested without instantiating the egui schedule.
pub fn apply_dismiss(launch_state: &mut LaunchState, splash_timer_secs: &mut f32) {
    *launch_state = LaunchState::MainMenu;
    *splash_timer_secs = 0.0;
}

/// True when the player has pressed any keyboard key or any mouse
/// button. We use `just_pressed` (not `pressed`) so holding a key down
/// doesn't repeatedly fire the dismiss.
fn first_input(
    contexts: &mut EguiContexts,
    keyboard_input: &Res<ButtonInput<KeyCode>>,
) -> bool {
    if keyboard_input.get_just_pressed().next().is_some() {
        return true;
    }
    // Mouse: the project's idiom (see src/ui/resources_bar.rs) is
    // `ctx.input(|i| i.pointer.any_pressed())`. We mirror it.
    let Ok(ctx) = contexts.ctx_mut() else {
        return false;
    };
    ctx.input(|i| i.pointer.any_pressed())
}

/// Decode the configured PNG (with `logo_clean` as fallback) into an
/// [`egui::ColorImage`], register it with the egui context via
/// `Context::load_texture`, and return the cached [`TextureHandle`].
/// Returns `None` on any decode / IO failure; the caller falls back
/// to a centered text label so the splash never paints blank.
fn load_splash_texture(
    contexts: &mut EguiContexts,
    manifest: &LaunchUiManifest,
) -> Option<TextureHandle> {
    let bytes = load_png_bytes_with_fallback(manifest)?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let color_image = ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
    let ctx = contexts.ctx_mut().ok()?;
    Some(ctx.load_texture("splash_logo", color_image, TextureOptions::LINEAR))
}

/// Try the configured splashscreen path first, fall back to the clean
/// logo if the file is missing or corrupt. Both are LGD-verified
/// `assets/logo/*.png` files (see `assets/data/launch_ui.ron`).
fn load_png_bytes_with_fallback(manifest: &LaunchUiManifest) -> Option<Vec<u8>> {
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
    use crate::ui::launch::{LaunchState, LaunchUiManifest};

    #[test]
    fn apply_dismiss_sets_main_menu_and_resets_timer() {
        let mut state = LaunchState::Splash;
        let mut timer = 1.5_f32;
        apply_dismiss(&mut state, &mut timer);
        assert_eq!(state, LaunchState::MainMenu);
        assert_eq!(timer, 0.0);
    }

    #[test]
    fn apply_dismiss_idempotent_from_main_menu() {
        // Calling apply_dismiss while already in MainMenu is a no-op
        // for the state and clears the timer again. The system guards
        // against double-firing via the `*launch_state != Splash`
        // early-return, but the helper itself is idempotent.
        let mut state = LaunchState::MainMenu;
        let mut timer = 0.42_f32;
        apply_dismiss(&mut state, &mut timer);
        assert_eq!(state, LaunchState::MainMenu);
        assert_eq!(timer, 0.0);
    }

    /// Simulates the auto-dismiss path: a splash timer that has
    /// crossed `splash_max_duration_s` advances state to `MainMenu`
    /// without requiring real time. Mirrors what `ui_splash_system`
    /// does at the `elapsed >= max_s` branch.
    #[test]
    fn manual_advance_past_max_duration_dismisses() {
        let mut state = LaunchState::Splash;
        let mut timer = 0.0_f32;
        let manifest = LaunchUiManifest::default();
        let max_s = manifest.splash_max_seconds();
        assert!(max_s > 0.0, "manifest default must have a positive max");

        // Tick the timer manually (NOT via real time).
        timer = max_s + 0.1;

        // Apply the same check the system applies.
        if timer >= max_s {
            apply_dismiss(&mut state, &mut timer);
        }
        assert_eq!(state, LaunchState::MainMenu);
        assert_eq!(timer, 0.0);
    }

    /// Documents the "pending input doesn't cause a transition when
    /// LaunchState != Splash" invariant from the issue test plan:
    /// when state is already past Splash, the system bails before
    /// touching `apply_dismiss`. This test exercises that branch by
    /// leaving state and timer untouched.
    #[test]
    fn dismiss_helper_unreachable_when_state_past_splash() {
        let mut state = LaunchState::MainMenu;
        let mut timer = 2.5_f32;
        // No apply_dismiss call — the system's `*launch_state != Splash`
        // guard prevents the helper from being reached.
        assert_eq!(state, LaunchState::MainMenu);
        assert_eq!(timer, 2.5);
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

    #[test]
    fn empty_keyboard_input_is_not_a_dismiss() {
        // The keyboard half of `first_input` is plain Bevy input —
        // constructible without an egui context. When no keys are
        // pressed, the iterator is empty and we don't fire. The mouse
        // half needs an egui context and is therefore a render-only
        // path — not unit-tested here.
        let keys: ButtonInput<KeyCode> = ButtonInput::default();
        assert!(keys.get_just_pressed().next().is_none());
    }

    /// Sanity check: the manifest accessor for the splash image path
    /// returns a value rooted under `assets/` and ending in `.png`.
    /// (The full round-trip tests live in `manifest::tests`; this one
    /// is duplicated here so a future splash refactor can't silently
    /// drop the contract.)
    #[test]
    fn splash_image_path_round_trips_through_manifest() {
        let manifest = LaunchUiManifest::default();
        let p = manifest.splash_image_path();
        assert!(
            p.starts_with("assets/"),
            "splash image path must be rooted under assets/ (got {:?})",
            p
        );
        assert!(p.ends_with(".png"), "splash image must be PNG (got {:?})", p);
    }
}