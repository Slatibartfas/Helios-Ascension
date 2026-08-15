//! Pure-data state for the `Shift+F12` screenshot pipeline.
//!
//! Lives in its own translation unit so the test target can compile
//! `ScreenshotSlots` / `PendingScreenshotAction` without dragging in
//! `bevy::render::view::screenshot` (the heavy GPU readback import that
//! pushes `cargo test --all` over the 4-min compile cliff on
//! `ubuntu-latest`). The `bevy_egui` integration and the actual capture
//! spawn live in `screenshot.rs`.

use bevy::prelude::*;
use std::collections::VecDeque;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Manual capture state for the `Shift+F12` live keybind.
///
/// Slot names default to a 12-name set covering the main menu and every
/// in-game top-level menu; `load_slots` overrides from
/// `assets/data/ui/screenshot_slots.ron` if present. The current slot
/// index advances on every capture, wrapping at the end.
///
/// Order matches the F1–F11 hotkey map in `src/ui/mod.rs:786-1227` so
/// the operator can run the audit by walking F1→F11 (one `Shift+F12`
/// each) and arrive at the same `*.png` filenames in every clone.
/// `main_menu` is the first slot; press `2` (New Game) in the main
/// menu, then walk the in-game menus.
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
                "main_menu".into(),
                "new_game_subview".into(),
                "load_game_subview".into(),
                "settings_subview".into(),
                "save_subview".into(),
                "survey".into(),
                "starmap".into(),
                "settings".into(),
                "construction".into(),
                "research".into(),
                "fleets".into(),
                "shipbuilding".into(),
                "economy".into(),
                "personnel".into(),
                "intel".into(),
                "diplomacy".into(),
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

    /// Map an in-game `GameMenu` to the slot name that should hold its
    /// screenshot. The main menu (which lives outside the in-game
    /// `GameMenu` enum) maps to `main_menu`. Returns `None` for menus
    /// the audit does not cover so the caller leaves the slot alone.
    ///
    /// Slot name list lives in [`Self::default`]; keep this in sync
    /// when adding new entries.
    pub fn slot_for_active_menu(menu: crate::game_state::GameMenu) -> Option<&'static str> {
        use crate::game_state::GameMenu;
        match menu {
            GameMenu::Survey => Some("survey"),
            GameMenu::Starmap => Some("starmap"),
            GameMenu::Main => Some("settings"),
            GameMenu::Construction => Some("construction"),
            GameMenu::Research => Some("research"),
            GameMenu::Fleets => Some("fleets"),
            GameMenu::Shipbuilding => Some("shipbuilding"),
            GameMenu::Economy => Some("economy"),
            GameMenu::Personnel => Some("personnel"),
            GameMenu::Intel => Some("intel"),
            GameMenu::Diplomacy => Some("diplomacy"),
        }
    }

    /// Look up a slot by menu name. Returns `None` if the name is not in
    /// the list, so the operator can re-shoot a single menu without
    /// walking the full list.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Reset the cursor to a specific slot so the next `Shift+F12`
    /// capture lands on that menu. OOB indices are clamped.
    pub fn set_current(&mut self, index: usize) {
        if self.names.is_empty() {
            return;
        }
        self.current = index % self.names.len();
    }
}

/// Save-thumbnail staging state. The in-game frame is captured before the
/// Save panel opens, then moved to the chosen save-slot path after writing.
#[derive(Resource, Debug, Default)]
pub struct PendingSaveThumbnail {
    pub staging_path: Option<PathBuf>,
    pub final_path: Option<PathBuf>,
    pub capture_started: bool,
}

/// One enqueued capture request. The pump pops from `queue`, parks the
/// request in `inflight`, and waits a fixed number of frames for the
/// Bevy 0.18 render-thread observer to write the PNG.
///
/// `queue` is a `VecDeque` so the pump can `pop_front` for FIFO order:
/// captures enqueued first must fire first so the slot sequence
/// matches what the operator sees.
#[derive(Resource, Debug, Default)]
pub struct PendingScreenshotAction {
    pub queue: VecDeque<QueuedCapture>,
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
        self.queue.push_back(capture);
    }

    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.inflight.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_cycle_through_all_names() {
        let mut s = ScreenshotSlots::default();
        // The exact count is owned by `Default::default()` and may
        // grow as new menus are added — assert that it is at least one
        // and wraps after that many advances.
        assert!(!s.names.is_empty(), "slot list must not be empty");
        let first = s.current_name().to_owned();
        let n = s.names.len();
        for _ in 0..(n - 1) {
            s.advance();
        }
        assert_ne!(s.current_name(), first);
        s.advance();
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
        let first = p.queue.pop_front().unwrap();
        assert_eq!(first.slot_name, "a");
        let second = p.queue.pop_front().unwrap();
        assert_eq!(second.slot_name, "b");
        assert!(p.is_idle());
    }

    #[test]
    fn set_current_resets_the_cursor() {
        let mut s = ScreenshotSlots::default();
        s.advance();
        s.advance();
        s.set_current(0);
        assert_eq!(s.current_name(), s.names.first().unwrap());
    }

    #[test]
    fn slot_for_active_menu_covers_every_top_level_menu() {
        // Belt-and-suspenders: every GameMenu variant must map to a
        // real slot. A drift here means the operator's audit runbook
        // silently drops a menu.
        use crate::game_state::GameMenu;
        for menu in [
            GameMenu::Survey,
            GameMenu::Starmap,
            GameMenu::Main,
            GameMenu::Construction,
            GameMenu::Research,
            GameMenu::Fleets,
            GameMenu::Shipbuilding,
            GameMenu::Economy,
            GameMenu::Personnel,
            GameMenu::Intel,
            GameMenu::Diplomacy,
        ] {
            let slot = ScreenshotSlots::slot_for_active_menu(menu)
                .unwrap_or_else(|| panic!("menu {menu:?} has no slot mapping"));
            let slots = ScreenshotSlots::default();
            assert!(
                slots.names.iter().any(|n| n == slot),
                "menu {menu:?} maps to slot {slot} which is not in the default list",
            );
        }
    }
}
