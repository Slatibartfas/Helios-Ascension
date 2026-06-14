//! Notification event surface.
//!
//! PR-A defines the `NotificationEvent` Bevy `Message` and its severity
//! enum. Sim-layer bridges (survey/construction/research) will
//! `Messages::write` these in PR-B; the spawn system will consume
//! them and create `ActiveNotification` entities.

use bevy::prelude::*;

/// How loud a notification should be when it surfaces.
///
/// PR-A only needs the variant set; PR-B maps each variant to a colour
/// token in the toast panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationSeverity {
    /// Informational only — no action required.
    Info,
    /// A state change the player should glance at.
    Notice,
    /// A problem the player probably wants to fix soon.
    Warning,
    /// Loss of life, broken project, hostile contact — demand attention.
    Critical,
}

/// One notification request, emitted by a sim bridge.
///
/// PR-B's spawn system reads `Messages<NotificationEvent>`, decides
/// whether the category is enabled in [`crate::ui::notifications::settings::NotificationSettings`],
/// and spawns an [`ActiveNotification`](crate::ui::notifications::ActiveNotification)
/// entity per unique `dedup_key` (within `default_group_window_s`).
///
/// All fields are value types so the message is cheap to broadcast.
#[derive(Debug, Clone, Message)]
pub struct NotificationEvent {
    /// Stable id from `assets/data/notifications.ron`. The settings
    /// map and the dedup key both key off this.
    pub category: crate::ui::notifications::NotificationCategoryId,
    pub severity: NotificationSeverity,
    /// Short, headline-style. PR-B renders this bold.
    pub title: String,
    /// One-or-two-sentence body. PR-B renders it under the title.
    pub body: String,
    /// Optional grouping key. If two events share the same key within
    /// the `default_group_window_s` window, PR-D groups them and
    /// increments the toast's `count`.
    pub dedup_key: Option<String>,
    /// `auto_dismiss_s` override. If `None`, the category's
    /// `default_dismiss_s` from RON applies.
    pub auto_dismiss_s: Option<f32>,
    /// `true` overrides the auto-dismiss timer (e.g. critical alerts).
    pub sticky: bool,
}
