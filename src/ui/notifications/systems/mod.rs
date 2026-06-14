//! Per-frame systems for the notifications feature.
//!
//! PR-A is type-only — this sub-module is intentionally empty. PR-B
//! adds:
//! - `consume_notification_events` — reads `Messages<NotificationEvent>`,
//!   respects `NotificationSettings`, spawns `ActiveNotification`
//!   entities.
//! - `auto_dismiss_toasts` — despawns toasts whose `auto_dismiss_s`
//!   has elapsed.
//! - `apply_pending_dismissals` — drains `PendingNotificationDismissal`
//!   and despawns the listed entity ids.
//! - `render_toast_panel` — egui panel that draws the live toasts in
//!   `EguiPrimaryContextPass` (per the UI-render rule).
//!
//! PR-D adds the grouping/dedup logic on top of those.

// Placeholder so the sub-module is non-empty in PR-A. The marker
// keeps the file in version control without committing a no-op
// function that clippy would flag as `dead_code`.
#[allow(dead_code)]
const PR_A_NO_SYSTEMS_YET: &str = "PR-A: systems land in PR-B";
