//! Screenshot pipeline + headless baseline capture.
//!
//! Two capture surfaces, one shared write path:
//!
//! 1. **Live manual capture** — `Shift+F12` cycles through 5 named slots and
//!    writes to `docs/UI/baselines/manual/{slot}.png`. Useful for transient
//!    states the manifest cannot predict (tooltips, dropdowns, dialogs).
//!
//! 2. **Headless manifest capture** — `cargo run --bin screenshot --
//!    --manifest tools/screenshot_manifest_v1.ron --out docs/UI/baselines/v1`
//!    iterates a RON manifest of `(slot, menu, submenu_path, wait_frames)`
//!    entries, switching the active menu between captures.
//!
//! The submenu-extension baseline (GRA-60) does not require new code: it ships
//! a second manifest (`tools/screenshot_manifest_v1.1.ron`) that reuses this
//! pipeline unchanged.
//!
//! Both paths enqueue `QueuedCapture` items; the capture pump drains the queue
//! using Bevy 0.18's `Screenshot` + `ScreenshotCaptured` observer API and
//! writes PNGs via the `image` crate (via `save_to_disk`).

use bevy::app::AppExit;
use bevy::ecs::event::EventWriter;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy_egui::egui;
use bevy_egui::EguiContexts;
use bevy_egui::EguiPrimaryContextPass;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::game_state::{ActiveMenu, GameMenu};

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Manual capture state for the `Shift+F12` live keybind.
///
/// Slots default to a sensible 5-name set; `load_slots` overrides from
/// `assets/data/ui/screenshot_slots.ron` if present. The current slot index
/// advances on every capture, wrapping at the end.
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
    /// When true, the app exits once the queue drains. Set by the
    /// headless binary; ignored in live use.
    pub exit_when_drained: bool,
    /// Frames to wait *after* the queue + inflight both drain before
    /// the app actually exits. Gives the last `Screenshot`'s observer
    /// time to fire. Set by the headless binary.
    pub post_drain_frames: u32,
}

#[derive(Debug, Clone)]
pub struct QueuedCapture {
    pub slot_name: String,
    pub menu: Option<GameMenu>,
    pub out_path: PathBuf,
    /// Frames to wait *after* the menu switch before triggering the
    /// capture. The egui panel needs a few frames to settle its layout.
    pub wait_frames: u32,
    pub frames_remaining: u32,
}

#[derive(Debug, Clone)]
pub struct InflightCapture {
    pub slot_name: String,
    /// Frames to wait for the render-thread observer to fire. Bevy 0.18
    /// runs the readback async; 10 frames at 60 fps is comfortable on
    /// a CI machine where the GPU readback may queue behind other work.
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
// RON manifest types (used by the headless binary)
// ---------------------------------------------------------------------------

/// Top-level manifest. Loaded from `tools/screenshot_manifest_v1.ron`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenshotManifest {
    pub version: String,
    pub out_dir: String,
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Filename (without `.png`). Becomes `docs/UI/baselines/{out_dir}/{name}.png`.
    pub name: String,
    /// One of the 11 `GameMenu` variants (case-insensitive). Aliases are
    /// supported — see `parse_menu`.
    pub menu: String,
    /// Optional submenu path. Reserved for the v1.1 manifest (GRA-60). For
    /// v1, leave empty — the manifest drives the top-level baseline.
    #[serde(default)]
    pub submenu_path: Vec<String>,
    /// Frames to wait after switching the menu before capturing. Default 60
    /// (1 s at 60 fps). Bump for heavier panels (Construction, Research).
    #[serde(default = "default_wait")]
    pub wait_frames: u32,
}

fn default_wait() -> u32 {
    60
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
        menu: None, // capture whatever the current active menu is
        out_path,
        wait_frames: 0,
        frames_remaining: 0,
    });
    slots.advance();
}

/// Drain the capture queue.
///
/// State machine:
///   1. `inflight` is `Some` → decrement its frame counter, drop when done.
///   2. `inflight` is `None` and `queue` has an entry → apply menu switch,
///      wait out the requested `wait_frames`, then spawn `Screenshot` and
///      move the entry to `inflight`.
///   3. Both empty + `exit_when_drained` → wait `post_drain_frames` for the
///      last observer to fire, then send `AppExit::Success`.
fn screenshot_capture_pump(
    mut commands: Commands,
    mut pending: ResMut<PendingScreenshotAction>,
    mut active_menu: ResMut<ActiveMenu>,
    mut exit: EventWriter<AppExit>,
) {
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
    let Some(mut next) = pending.queue.pop() else {
        // Step 3: idle. If we are exiting, hold for post_drain_frames
        // before signalling AppExit so the final Screenshot's observer
        // has a chance to fire.
        if pending.exit_when_drained {
            if pending.post_drain_frames > 0 {
                pending.post_drain_frames -= 1;
                return;
            }
            info!("[screenshot] queue drained, exiting");
            exit.send(AppExit::Success);
        }
        return;
    };

    // Apply the menu switch up front; the wait_frames countdown lets the
    // panel settle before we trigger the readback.
    if let Some(menu) = next.menu {
        active_menu.current = menu;
    }
    if next.frames_remaining > 0 {
        next.frames_remaining -= 1;
        pending.queue.push(next);
        return;
    }

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a manifest's string menu name to a `GameMenu`. Returns `None` for
/// unknown names — the manifest loader should error out on those.
pub fn parse_menu(s: &str) -> Option<GameMenu> {
    Some(match s.to_ascii_lowercase().as_str() {
        "survey" => GameMenu::Survey,
        "starmap" => GameMenu::Starmap,
        "main" | "menu" => GameMenu::Main,
        "construction" => GameMenu::Construction,
        "research" => GameMenu::Research,
        "fleets" | "fleet" => GameMenu::Fleets,
        "shipbuilding" | "ship_design" | "ship-design" => GameMenu::Shipbuilding,
        "economy" | "statistics" | "stats" => GameMenu::Economy,
        "personnel" | "officers" => GameMenu::Personnel,
        "intel" | "intelligence" => GameMenu::Intel,
        "diplomacy" => GameMenu::Diplomacy,
        _ => return None,
    })
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
    fn parse_menu_handles_aliases() {
        assert_eq!(parse_menu("shipbuilding"), Some(GameMenu::Shipbuilding));
        assert_eq!(parse_menu("ship_design"), Some(GameMenu::Shipbuilding));
        assert_eq!(parse_menu("SHIP_DESIGN"), Some(GameMenu::Shipbuilding));
        assert_eq!(parse_menu("logistics"), None);
    }

    #[test]
    fn queue_drains_in_fifo_order() {
        let mut p = PendingScreenshotAction::default();
        p.enqueue(QueuedCapture {
            slot_name: "a".into(),
            menu: None,
            out_path: "a.png".into(),
            wait_frames: 0,
            frames_remaining: 0,
        });
        p.enqueue(QueuedCapture {
            slot_name: "b".into(),
            menu: None,
            out_path: "b.png".into(),
            wait_frames: 0,
            frames_remaining: 0,
        });
        let first = p.queue.pop().unwrap();
        assert_eq!(first.slot_name, "a");
        let second = p.queue.pop().unwrap();
        assert_eq!(second.slot_name, "b");
        assert!(p.is_idle());
    }
}
