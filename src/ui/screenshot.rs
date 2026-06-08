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

mod screenshot_state;
pub use screenshot_state::{
    InflightCapture, PendingScreenshotAction, QueuedCapture, ScreenshotSlots,
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
            // Live keybind runs in egui pass so the context is available.
            .add_systems(EguiPrimaryContextPass, screenshot_keybind_system);
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

/// Watch for `Shift+F12` and enqueue a capture targeting the current slot.
fn screenshot_keybind_system(
    mut contexts: EguiContexts,
    mut pending: ResMut<PendingScreenshotAction>,
    mut slots: ResMut<ScreenshotSlots>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let triggered = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F12));

    if !triggered {
        return;
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
fn screenshot_capture_pump(mut commands: Commands, mut pending: ResMut<PendingScreenshotAction>) {
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
