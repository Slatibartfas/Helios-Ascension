//! Live screenshot capture (Shift+F12).
//!
//! Pressing `Shift+F12` enqueues a capture against the current
//! [`ScreenshotSlots`] slot, advances to the next slot (wrapping at the
//! end), and writes the PNG to `docs/UI/baselines/manual/{slot}.png`.
//!
//! ## Design
//!
//! * `PendingScreenshotAction` holds a FIFO queue + a single in-flight
//!   slot. The pump runs in `Update` so it does not block the egui
//!   pass.
//! * `Screenshot::primary_window()` + a `save_to_disk` observer do the
//!   actual GPU readback. The observer writes the PNG via the
//!   `image` crate.
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
//!   file accordingly. Re-introducing a manifest driver is parked.
//!
//! ## Why `Shift+F12`
//!
//! F1–F11 are bound to menu switches in `src/ui/mod.rs:786-796`; bare
//! F12 is the construction/research debug toggle in
//! `src/ui/research_panel.rs:129` and
//! `src/ui/construction_panel.rs:461`. `Shift+F12` is the only clean
//! slot in that family.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy_egui::egui;
use bevy_egui::EguiContexts;
use bevy_egui::EguiPrimaryContextPass;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Manual capture state for the `Shift+F12` live keybind.
///
/// Slot names default to a sensible 5-name set; `load_slots` overrides
/// from `assets/data/ui/screenshot_slots.ron` if present. The current
/// slot index advances on every capture, wrapping at the end.
#[derive(Resource, Debug, Clone)]
pub struct ScreenshotSlots {
    pub names: Vec<String>,
    pub current: usize,
    pub out_dir: PathBuf,
}

impl Default for ScreenshotSlots {
    fn default() -> Self {
        Self {
            names: vec![
                "overview".into(),
                "shipbuilding".into(),
                "research".into(),
                "construction".into(),
                "starmap".into(),
            ],
            current: 0,
            out_dir: PathBuf::from("docs/UI/baselines/manual"),
        }
    }
}

impl ScreenshotSlots {
    pub fn current_name(&self) -> &str {
        &self.names[self.current % self.names.len()]
    }

    pub fn advance(&mut self) {
        self.current = (self.current + 1) % self.names.len();
    }
}

/// One enqueued capture request. The pump pops from `queue`, parks the
/// request in `inflight`, and waits a fixed number of frames for the
/// Bevy 0.18 render-thread observer to write the PNG.
#[derive(Resource, Debug, Default)]
pub struct PendingScreenshotAction {
    pub queue: Vec<QueuedCapture>,
    pub inflight: Option<InflightCapture>,
}

#[derive(Debug, Clone)]
pub struct QueuedCapture {
    pub slot_name: String,
    pub out_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct InflightCapture {
    pub slot_name: String,
    /// Frames to wait for the render-thread observer to fire. Bevy 0.18
    /// runs the readback async; 10 frames at 60 fps is comfortable on
    /// a desktop GPU and gives a CI readback a chance to queue behind
    /// other work.
    pub frames_remaining: u32,
}

impl PendingScreenshotAction {
    pub fn enqueue(&mut self, capture: QueuedCapture) {
        self.queue.push(capture);
    }

    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.inflight.is_none()
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenshotSlots>()
            .init_resource::<PendingScreenshotAction>()
            // Live keybind runs in egui pass so the context is available.
            .add_systems(EguiPrimaryContextPass, screenshot_keybind_system)
            // Capture pump runs in Update — observer fires async, we drain
            // the queue outside the egui schedule so it does not block UI.
            .add_systems(Update, screenshot_capture_pump);
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
    let Some(next) = pending.queue.pop() else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_cycle_through_all_names() {
        let mut s = ScreenshotSlots::default();
        assert_eq!(s.names.len(), 5);
        let first = s.current_name().to_owned();
        s.advance();
        assert_ne!(s.current_name(), first);
        for _ in 0..4 {
            s.advance();
        }
        assert_eq!(s.current_name(), first, "slots must wrap");
    }

    #[test]
    fn queue_drains_in_fifo_order() {
        let mut p = PendingScreenshotAction::default();
        p.enqueue(QueuedCapture {
            slot_name: "a".into(),
            out_path: "a.png".into(),
        });
        p.enqueue(QueuedCapture {
            slot_name: "b".into(),
            out_path: "b.png".into(),
        });
        let first = p.queue.pop().unwrap();
        assert_eq!(first.slot_name, "a");
        let second = p.queue.pop().unwrap();
        assert_eq!(second.slot_name, "b");
        assert!(p.is_idle());
    }
}
