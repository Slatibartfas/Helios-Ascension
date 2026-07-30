//! Live screenshot capture (Shift+F12).
//!
//! Pressing `Shift+F12` enqueues a capture against the current
//! [`ScreenshotSlots`] slot, advances to the next slot (wrapping at the
//! end), and writes the PNG to `docs/UI/baselines/manual/{slot}.png`.
//!
//! ## Design
//!
//! * The pure-data resources ([`ScreenshotSlots`],
//!   [`PendingScreenshotAction`]) live in `screenshot_state.rs` so the
//!   test target can compile them without pulling in
//!   `bevy::render::view::screenshot` — the heavy GPU readback import
//!   that pushes `cargo test --all` over the 4-min compile cliff on
//!   `ubuntu-latest`. The `cargo test` step in `.github/workflows/cargo.yml`
//!   compiles the lib with `cfg(test)`; gating the heavy import under
//!   `#[cfg(not(test))]` drops the test target's incremental compile
//!   from ~4 min back to the pre-PR-112 baseline.
//! * `screenshot_capture_pump` is also `#[cfg(not(test))]` since it
//!   spawns `Screenshot` observers; the plugin's `build()` adds it
//!   only outside the test target.
//! * The capture is **manual** — a human runs the game locally, hits
//!   `Shift+F12` while looking at the menu they care about, and
//!   commits the resulting PNG. A previous headless bin
//!   (`src/bin/screenshot.rs`, removed 2026-06-09) tried to drive this
//!   from a RON manifest under `xvfb-run`, but its test target
//!   compile footprint exceeded the runner's 30-min window. The
//!   pipeline is unchanged; the manifest is gone.
//! * Submenus (Construction: buildings/ships/defenses, Research: tech
//!   tree/available/engineering, Economy: logistics/mining/resources/
//!   ..., Fleets: list/details, etc.) do not need a separate code
//!   path — the operator captures each one manually and names the
//!   file accordingly. Re-introducing a manifest driver is parked
//!   (see [[GRA-60]] follow-up).
//!
//! ## Why `Shift+F12`
//!
//! F1–F11 are bound to menu switches in `src/ui/mod.rs:786-796`; bare
//! F12 is the construction/research debug toggle in
//! `src/ui/research_panel.rs:129` and
//! `src/ui/construction_panel.rs:461`. `Shift+F12` is the only clean
//! slot in that family.

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiContexts;
use bevy_egui::EguiPrimaryContextPass;

use super::screenshot_state;
pub use screenshot_state::{
    InflightCapture, PendingSaveThumbnail, PendingScreenshotAction, QueuedCapture, ScreenshotSlots,
};

// ---------------------------------------------------------------------------
// Heavy imports — gated behind `#[cfg(not(test))]` so the test target's
// `cargo test --all` step does not pull in `bevy::render::view::screenshot`.
// The lib target (non-test build) still sees the import.
// ---------------------------------------------------------------------------
#[cfg(not(test))]
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenshotSlots>()
            .init_resource::<PendingScreenshotAction>()
            .init_resource::<PendingSaveThumbnail>()
            // In-game keybind (Shift+F12) — dispatches on ActiveMenu so
            // the file lands under the active menu's slot name.
            .add_systems(EguiPrimaryContextPass, screenshot_keybind_system)
            // Launch / menu keybind. The main menu lives outside the
            // in-game `GameMenu` enum, so the in-game handler cannot
            // route a capture to `main_menu.png` on its own. The launch
            // handler covers `LaunchState::MainMenu / NewGame /
            // LoadGame / Settings / SaveGame` and routes them to the
            // matching slot.
            .add_systems(
                EguiPrimaryContextPass,
                launch_screenshot_keybind_system,
            );
        // Capture pump is `#[cfg(not(test))]` — it spawns Bevy 0.18
        // `Screenshot` observers. Tests don't build a renderer, so
        // there is nothing to drain.
        #[cfg(not(test))]
        app.add_systems(Update, screenshot_capture_pump);
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Watch for `Shift+F12` and enqueue a capture targeting the active
/// menu's slot. If the active menu has no mapped slot (e.g. an
/// in-game subview, or a context where the main menu is hidden by a
/// modal), fall back to the cursor's current slot so the operator
/// still gets a file — just named whatever was next in the round-robin.
fn screenshot_keybind_system(
    mut contexts: EguiContexts,
    mut pending: ResMut<PendingScreenshotAction>,
    mut slots: ResMut<ScreenshotSlots>,
    active_menu: Option<Res<crate::game_state::ActiveMenu>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let triggered = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F12));

    if !triggered {
        return;
    }

    // Pick the slot from the active menu when one is mapped. The
    // audit runbook documents a deterministic order, but the helper
    // also makes ad-hoc re-shoots trivial: open the menu, press
    // Shift+F12, the file lands under that menu's name.
    if let Some(active_menu) = active_menu.as_deref() {
        if let Some(slot) = ScreenshotSlots::slot_for_active_menu(active_menu.current) {
            if let Some(idx) = slots.index_of(slot) {
                slots.set_current(idx);
            }
        }
    }

    let slot_name = slots.current_name().to_owned();
    let out_path = slots.out_dir.join(format!("{slot_name}.png"));
    pending.enqueue(QueuedCapture {
        slot_name,
        out_path,
    });
    slots.advance();
}

/// Watch for `Shift+F12` in the launch / main-menu state machine and
/// enqueue a capture targeting the active launch state's slot.
///
/// This is the launch-side counterpart of [`screenshot_keybind_system`].
/// The in-game handler dispatches on `ActiveMenu`, but the main menu
/// lives in the separate `LaunchState` resource, so the audit runbook
/// needs this system to produce `main_menu.png`. The same handler also
/// covers the NewGame / LoadGame / Settings subviews so the operator
/// can audit subviews without leaving the launch state machine.
///
/// Slot mapping:
///   `MainMenu`   → `main_menu`
///   `NewGame`    → `new_game_subview`
///   `LoadGame`   → `load_game_subview`
///   `Settings`   → `settings_subview`
///   `SaveGame`   → `save_subview`
///   `InGame`     → no-op (in-game handler covers this)
fn launch_screenshot_keybind_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    launch_state: Res<super::launch::LaunchState>,
    mut pending: ResMut<PendingScreenshotAction>,
    mut slots: ResMut<ScreenshotSlots>,
) {
    // Raw `just_pressed(Shift+F12)` works in every launch state without
    // needing a text-field focus check; the egui path through
    // `EguiContexts` would be tripped by an in-flight TextField (e.g.
    // a partially-typed save name in the Save panel).
    let shift_held = keyboard_input.pressed(KeyCode::ShiftLeft)
        || keyboard_input.pressed(KeyCode::ShiftRight);
    if !shift_held || !keyboard_input.just_pressed(KeyCode::F12) {
        return;
    }

    let slot = match *launch_state {
        super::launch::LaunchState::MainMenu => "main_menu",
        super::launch::LaunchState::NewGame => "new_game_subview",
        super::launch::LaunchState::LoadGame => "load_game_subview",
        super::launch::LaunchState::Settings => "settings_subview",
        super::launch::LaunchState::SaveGame => "save_subview",
        // In-game is handled by `screenshot_keybind_system`; the
        // operator does not need a duplicate capture from this
        // handler. Stay quiet to avoid a double-fire.
        super::launch::LaunchState::InGame => return,
    };

    if let Some(idx) = slots.index_of(slot) {
        slots.set_current(idx);
    } else {
        // Slot wasn't found in the operator's custom list — fall back
        // to the round-robin cursor. The operator can re-shoot this
        // subview later by editing `assets/data/ui/screenshot_slots.ron`
        // or by re-running the audit with a fresh default.
    }

    let slot_name = slots.current_name().to_owned();
    let out_path = slots.out_dir.join(format!("{slot_name}.png"));
    pending.enqueue(QueuedCapture {
        slot_name,
        out_path,
    });
    slots.advance();
}

/// Drain the capture queue.
///
/// State machine:
///   1. `inflight` is `Some` → decrement its frame counter, drop when done.
///   2. `inflight` is `None` and `queue` has an entry → spawn `Screenshot`
///      and move the entry to `inflight`.
///   3. Both empty → idle.
#[cfg(not(test))]
fn screenshot_capture_pump(
    mut commands: Commands,
    mut pending: ResMut<PendingScreenshotAction>,
    mut save_thumbnail: ResMut<PendingSaveThumbnail>,
) {
    if let (Some(staging), Some(final_path)) = (
        save_thumbnail.staging_path.as_ref(),
        save_thumbnail.final_path.as_ref(),
    ) {
        if staging.exists() {
            if let Some(parent) = final_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::copy(staging, final_path) {
                Ok(_) => {
                    info!(
                        "[screenshot] installed save thumbnail at {}",
                        final_path.display()
                    );
                    save_thumbnail.staging_path = None;
                    save_thumbnail.final_path = None;
                    save_thumbnail.capture_started = false;
                }
                Err(error) => warn!("[screenshot] could not install save thumbnail: {error}"),
            }
        }
    }

    if pending.inflight.is_none() && !save_thumbnail.capture_started {
        if let Some(path) = save_thumbnail.staging_path.clone() {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            info!(
                "[screenshot] capturing in-game save thumbnail to {}",
                path.display()
            );
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
            save_thumbnail.capture_started = true;
            pending.inflight = Some(InflightCapture {
                slot_name: "save-thumbnail".to_string(),
                frames_remaining: 10,
            });
            return;
        }
    }

    // Step 1: drain an in-flight capture.
    if let Some(mut inflight) = pending.inflight.take() {
        if inflight.frames_remaining > 0 {
            inflight.frames_remaining -= 1;
            pending.inflight = Some(inflight);
        }
        // else: observer already wrote the PNG; drop and advance.
        return;
    }

    // Step 2: pop the next capture.
    let Some(next) = pending.queue.pop_front() else {
        return;
    };

    // Spawn the screenshot. The observer writes the PNG.
    let path = next.out_path.clone();
    info!(
        "[screenshot] capturing slot '{}' to {}",
        next.slot_name,
        path.display()
    );
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
    pending.inflight = Some(InflightCapture {
        slot_name: next.slot_name,
        frames_remaining: 10,
    });
}
