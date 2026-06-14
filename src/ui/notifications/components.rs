//! Per-toast ECS component.
//!
//! `ActiveNotification` is a Component, not a Resource: each visible
//! toast is its own entity. PR-B's spawn system despawns entities on
//! dismiss / auto-expiry. The component derives `Reflect` so the
//! `bevy_inspector_egui` integration can introspect live toasts
//! during dev (the inspector plugin is not currently attached in
//! `main.rs` — registration is forward-looking).
//!
//! PR-G (GRA-141) adds `context_link` so a body-click can route the
//! player to the relevant context, and `PendingNotificationClicks`
//! as the action-queue resource the render system pushes into.

use bevy::prelude::*;

use super::events::{NotificationContextLink, NotificationSeverity};
use super::settings::NotificationCategoryId;
use crate::ui::time::SimulationTime;

/// One visible toast. The entity also carries an `OnDismiss`
/// observer in PR-B; the dismissal flow reads
/// `PendingNotificationDismissal` rather than despawning directly,
/// to keep the action-queue decoupling invariant.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct ActiveNotification {
    pub category: NotificationCategoryId,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
    /// Game-world time the toast was created. Stored as the
    /// simulation-clock elapsed seconds (same epoch as
    /// `SimulationTime.elapsed`).
    pub created_at: f64,
    /// When the spawn system should auto-despawn this entity if the
    /// player hasn't dismissed it. `0.0` and `f32::INFINITY` are
    /// both treated as "no auto-dismiss" by PR-B.
    pub auto_dismiss_s: f32,
    /// `true` skips the auto-dismiss timer (e.g. critical alerts).
    pub sticky: bool,
    /// Optional grouping key. Two toasts that share a key inside
    /// the same `default_group_window_s` are merged by PR-D.
    pub dedup_key: Option<String>,
    /// How many events have folded into this toast. PR-A starts at
    /// 1; PR-D increments on each subsequent match.
    pub count: u32,
    /// PR-G (GRA-141): where a body-click on the toast should jump.
    /// The render system reads this when it sees a click on the
    /// toast body (not the dismiss button). `None` is the
    /// common case for informational toasts.
    ///
    /// `#[reflect(ignore)]` because `NotificationContextLink`
    /// intentionally does not implement `Reflect` (the `OpenMenu`
    /// variant would force `GameMenu` to also implement
    /// `Reflect`/`TypePath` — a separate, larger design change).
    /// The field is consumed at click time, never inspected.
    #[reflect(ignore)]
    pub context_link: NotificationContextLink,
}

impl ActiveNotification {
    /// Convenience constructor used by PR-B's spawn system.
    pub fn from_event(
        event: &super::events::NotificationEvent,
        sim_time: &SimulationTime,
        default_dismiss_s: f32,
    ) -> Self {
        Self {
            category: event.category.clone(),
            severity: event.severity,
            title: event.title.clone(),
            body: event.body.clone(),
            created_at: sim_time.elapsed_seconds(),
            auto_dismiss_s: event.auto_dismiss_s.unwrap_or(default_dismiss_s),
            sticky: event.sticky,
            dedup_key: event.dedup_key.clone(),
            count: 1,
            context_link: event.context_link,
        }
    }
}

/// Action-queue decoupling for click-to-dismiss and the auto-dismiss timer.
///
/// PR-B's render system pushes the toast entity id into this vec on
/// click; the `apply_pending_dismissals` system drains it and despawns
/// the entities. The UI never despawns toast entities directly, so the
/// action lives in the same dataflow as `PendingResearchActions` /
/// `PendingConstructionActions`.
#[derive(Resource, Debug, Default, Clone)]
pub struct PendingNotificationDismissal {
    pub to_dismiss: Vec<u64>,
}

impl PendingNotificationDismissal {
    pub fn push(&mut self, entity_id: u64) {
        self.to_dismiss.push(entity_id);
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, u64> {
        self.to_dismiss.drain(..)
    }
}

/// Action-queue decoupling for click-to-focus (PR-G, GRA-141).
///
/// The render system pushes a `(entity_bits, context_link)` pair when
/// the player clicks the toast body (not the dismiss "×" button).
/// The `click_handler` system in `Update` drains the queue and
/// dispatches: `SelectBody` → insert `Selected` + switch to
/// `GameMenu::Survey`; `OpenMenu` → switch `active_menu.current`;
/// `SelectMission` / `None` → no-op. Mirrors the
/// `PendingNotificationDismissal` pattern.
#[derive(Resource, Debug, Default, Clone)]
pub struct PendingNotificationClicks {
    pub to_focus: Vec<PendingNotificationClick>,
}

/// One click-to-focus request. `entity_bits` is the packed
/// `Entity::to_bits()` of the toast that was clicked; `context_link`
/// is copied off the `ActiveNotification` at click time so the
/// handler doesn't have to re-query the (now possibly despawned)
/// entity.
#[derive(Debug, Clone, Copy)]
pub struct PendingNotificationClick {
    pub entity_bits: u64,
    pub context_link: NotificationContextLink,
}

impl PendingNotificationClicks {
    pub fn push(&mut self, click: PendingNotificationClick) {
        self.to_focus.push(click);
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, PendingNotificationClick> {
        self.to_focus.drain(..)
    }
}
