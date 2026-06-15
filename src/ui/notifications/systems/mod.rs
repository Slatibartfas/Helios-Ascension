//! Per-frame systems for the notifications feature.
//!
//! PR-B wires the toast panel and the timer/click tick layers.
//! PR-C (GRA-137) adds `event_bridge` for Survey +
//! Construction + Research; PR-D (GRA-138) adds the
//! coalesce/grouping pass; PR-F (GRA-140) adds `pause_on_event`;
//! PR-G (GRA-141) adds `click_handler` for body-click → focus.
//! The system sets declared here give every layer a stable slot
//! in the frame schedule.
//!
//! Ordering (PR-B + PR-C + PR-D + PR-F + PR-G):
//! - `NotificationsSystemSet::EventBridge` runs in `Update`
//!   after the sim tick. Three systems share the set:
//!   - `bridge_survey_events` — SurveyEvent → NotificationEvent.
//!   - `bridge_construction_events` — ConstructionEvent →
//!     NotificationEvent.
//!   - `bridge_research_events` — ResearchEvent →
//!     NotificationEvent.
//! - `NotificationsSystemSet::Coalesce` runs in `Update` after
//!   `EventBridge`. PR-D adds `coalesce_notifications`.
//! - `NotificationsSystemSet::Tick` runs in `Update` after
//!   `Coalesce`. Four systems share the set:
//!   - `auto_dismiss_toasts` — timer-based despawn.
//!   - `apply_pending_dismissals` — drain click-to-dismiss
//!     queue.
//!   - `pause_on_event_toasts` (PR-F) — pause sim on
//!     `pause_on_event` category events.
//!   - `click_to_focus` (PR-G) — drain body-click queue and
//!     dispatch to `Selected` / `ActiveMenu`.
//! - `NotificationsSystemSet::Render` runs in
//!   `EguiPrimaryContextPass`, chained after
//!   `UiSystemSet::Overlays` so toasts paint on top of every
//!   other panel.
//!
//! PR-D (GRA-138) adds [`coalesce`] — the grouping/dedup layer
//! that runs in `Update` before PR-B's tick + render systems.

use bevy::prelude::*;

pub mod click_handler;
pub mod coalesce;
pub mod event_bridge;
pub mod render;
pub mod tick;

pub use click_handler::click_to_focus;
pub use coalesce::coalesce_notifications;
pub use event_bridge::{bridge_construction_events, bridge_research_events, bridge_survey_events};
pub use render::render_notification_toasts;
pub use tick::{apply_pending_dismissals, auto_dismiss_toasts, pause_on_event_toasts};

/// System-set taxonomy for the notifications feature.
///
/// Each variant is `.configure_set`d in `NotificationsPlugin`.
/// `Render` is chained in the egui pass after
/// `UiSystemSet::Overlays`; `Tick` and `EventBridge` are added
/// to `Update`. `Coalesce` is a placeholder for the later
/// PR-D (GRA-138).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum NotificationsSystemSet {
    /// Consumes source events, produces `NotificationEvent`
    /// messages. PR-C (GRA-137).
    EventBridge,
    /// Coalesces/grouping pass. PR-D (GRA-138) registers
    /// `coalesce_notifications` in this set; the plugin chains
    /// `Coalesce → Tick` so a brand-new event lands in the live
    /// toast before PR-B's auto-dismiss timer can despawn it.
    Coalesce,
    /// Auto-dismiss timer (PR-B) + click-dismiss queue drain
    /// (PR-B) + pause-on-event (PR-F) + click-to-focus
    /// dispatcher (PR-G, GRA-141).
    Tick,
    /// egui top-right toast panel. PR-B.
    Render,
}
