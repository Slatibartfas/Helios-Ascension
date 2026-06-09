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
        let first = p.queue.pop_front().unwrap();
        assert_eq!(first.slot_name, "a");
        let second = p.queue.pop_front().unwrap();
        assert_eq!(second.slot_name, "b");
        assert!(p.is_idle());
    }
}
