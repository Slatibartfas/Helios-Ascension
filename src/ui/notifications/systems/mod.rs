//! Per-frame systems for the notifications feature.
//!
//! PR-B wires the toast panel and the timer/click tick layers.
//! PR-C (GRA-137) will add `event_bridge` for Survey +
//! Construction + Research; PR-D (GRA-138) adds the
//! coalesce/grouping pass. The system sets declared here give
//! every layer a stable slot in the frame schedule.
//!
//! Ordering (PR-B):
//! - `NotificationsSystemSet::Tick` runs in `Update` after the
//!   sim tick. Two systems share the set:
//!   - `auto_dismiss_toasts` — timer-based despawn.
//!   - `apply_pending_dismissals` — drain click-to-dismiss
//!     queue.
//! - `NotificationsSystemSet::Render` runs in
//!   `EguiPrimaryContextPass`, chained after
//!   `UiSystemSet::Overlays` so toasts paint on top of every
//!   other panel.
//!
//! PR-D (GRA-138) adds [`coalesce`] — the grouping/dedup layer that
//! runs in `Update` before PR-B's tick + render systems.

use bevy::prelude::*;

pub mod coalesce;
pub mod render;
pub mod tick;

pub use render::render_notification_toasts;
pub use tick::{apply_pending_dismissals, auto_dismiss_toasts};

/// System-set taxonomy for the notifications feature.
///
/// Each variant is `.configure_set`d in `NotificationsPlugin`.
/// `Render` is chained in the egui pass after
/// `UiSystemSet::Overlays`; `Tick` is added to `Update`.
/// `EventBridge` and `Coalesce` are placeholders for the
/// later PRs (C and D).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum NotificationsSystemSet {
    /// Consumes source events, produces `NotificationEvent`
    /// messages. Reserved for PR-C (GRA-137).
    #[allow(dead_code)]
    EventBridge,
    /// Coalesces/grouping pass. Reserved for PR-D (GRA-138).
    #[allow(dead_code)]
    Coalesce,
    /// Auto-dismiss timer + click-dismiss queue drain. PR-B.
    Tick,
    /// egui top-right toast panel. PR-B.
    Render,
}
