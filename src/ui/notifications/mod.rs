//! Player-facing notifications system (toast-style HUD overlay).
//!
//! PR-A (GRA-135) lays the type & data foundation. PR-B (GRA-136)
//! adds the tick + render layers that make toasts appear and
//! dismiss. PR-C / D / E / F add event bridges, coalescing,
//! settings, and pause-on-event respectively. PR-G (GRA-141) adds
//! the click-to-focus dispatcher so a body-click on the toast
//! jumps the player to the relevant context.
//!
//! Module map:
//! - [`events`]    — `NotificationEvent` Bevy `Message` produced by sim
//!   bridges and consumed by the spawn system in PR-B / PR-C. PR-G
//!   adds `NotificationContextLink` + `context_link` field.
//! - [`data`]      — RON loader for `assets/data/notifications.ron`;
//!   `NotificationCategoriesData` resource.
//! - [`settings`]  — `NotificationSettings` resource (per-category
//!   overrides + global knobs).
//! - [`components`] — `ActiveNotification` per-toast component +
//!   `PendingNotificationDismissal` action-queue resource (PR-B) +
//!   `PendingNotificationClicks` action-queue resource (PR-G).
//! - [`systems`]   — `tick` (auto-dismiss + click-dismiss drain +
//!   pause-on-event), `coalesce` (PR-D dedup), `click_handler`
//!   (PR-G click-to-focus dispatch), and `render` (egui top-right
//!   panel).
//! - [`ui_settings`] — the settings panel modal (PR-E / GRA-139).

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod events;
pub mod settings;
pub mod systems;
pub mod ui_settings;

pub use components::{
    ActiveNotification, PendingNotificationClick, PendingNotificationClicks,
    PendingNotificationDismissal,
};
pub use data::{load_notification_categories, NotificationCategoriesData, NotificationCategory};
pub use events::{NotificationContextLink, NotificationEvent, NotificationSeverity};
pub use settings::{NotificationCategoryId, NotificationSettings};
pub use systems::coalesce::coalesce_notifications;
pub use systems::NotificationsSystemSet;
pub use ui_settings::{
    ui_notifications_settings_panel, NotificationsSettingsOpen, NotificationsSettingsPanelPlugin,
};

/// Sub-plugin that owns the notifications feature surface.
///
/// PR-A registers the resources + the dev-introspection `Reflect`
/// handle for `ActiveNotification`. PR-B wires the `Tick` and
/// `Render` system sets. PR-D adds the `coalesce_notifications`
/// system in `Update`; the system-set chain that orders it
/// relative to PR-B's tick + render is wired in PR-B's merge
/// (PR-B owns the canonical `NotificationsSystemSet` enum, so
/// PR-D does not re-define it). PR-E (GRA-139) registers the
/// settings panel modal. PR-C (GRA-137) adds the `EventBridge`
/// set with three source-event → `NotificationEvent` bridges.
/// PR-G (GRA-141) adds `PendingNotificationClicks` resource and
/// the `click_to_focus` system in the `Tick` set.
pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotificationSettings>()
            .init_resource::<NotificationCategoriesData>()
            .init_resource::<PendingNotificationDismissal>()
            // PR-G (GRA-141): the click-to-focus queue. Mirrors
            // `PendingNotificationDismissal` — the render
            // system pushes, the `click_to_focus` system drains.
            .init_resource::<PendingNotificationClicks>()
            .add_systems(Startup, load_notification_categories)
            // Settings panel (PR-E / GRA-139) — modal renderer, the
            // "Notifications" button in the top menu bar toggles its
            // visibility via `NotificationsSettingsOpen`.
            .add_plugins(NotificationsSettingsPanelPlugin)
            // PR-D (GRA-138): the coalesce/grouping pass. Runs in
            // `Update` in the `Coalesce` set; chained before
            // `Tick` below so a brand-new event lands in the live
            // toast before PR-B's auto-dismiss timer can despawn
            // it (otherwise the tick system could despawn a toast
            // that the same-frame coalesce was about to merge
            // into). Kilo Code Review finding.
            .add_systems(
                Update,
                coalesce_notifications.in_set(NotificationsSystemSet::Coalesce),
            )
            // Dev introspection hook for bevy_inspector_egui. The
            // inspector plugin is not currently attached in `main.rs`,
            // but registering the type means a future inspector wiring
            // (or a unit test using `AppTypeRegistry`) can iterate
            // live toasts without further code changes. The nested
            // types (`NotificationCategoryId`, `NotificationSeverity`)
            // are pulled in transitively because their `Reflect` impl
            // is derived; registering them explicitly is harmless
            // and documents the dependency.
            .register_type::<ActiveNotification>()
            .register_type::<NotificationCategoryId>()
            .register_type::<NotificationSeverity>()
            // PR-C (GRA-137): the three event bridges. Run in
            // `Update` *before* `Tick` so the auto-dismiss timer
            // and click-dismiss queue see a fully populated
            // `Messages<NotificationEvent>` buffer on the same
            // frame the bridge emitted. Ordering within the set
            // is irrelevant — each bridge consumes a disjoint
            // source message family.
            .add_systems(
                Update,
                (
                    systems::bridge_survey_events,
                    systems::bridge_construction_events,
                    systems::bridge_research_events,
                )
                    .in_set(NotificationsSystemSet::EventBridge),
            )
            // PR-B + PR-F + PR-G systems. The Tick set runs in
            // Update; the Render set is added to
            // EguiPrimaryContextPass by `UIPlugin::build` so it
            // can chain after `UiSystemSet::Overlays` (see
            // src/ui/mod.rs).
            //
            // PR-D (GRA-138) registered Coalesce above; we
            // chain `Coalesce → Tick` so a brand-new event
            // lands in the live toast before PR-B's auto-dismiss
            // timer can despawn it.
            //
            // PR-F (GRA-140) adds `pause_on_event_toasts` to the
            // same Tick set. The `Coalesce → Tick` chain keeps
            // `pause_on_event_toasts`'s "newly-inserted entity"
            // detection working: the new entity is visible to
            // Tick in the same frame that Coalesce produced it.
            //
            // PR-G (GRA-141) adds `click_to_focus` to the same
            // Tick set. `click_to_focus` chains after
            // `apply_pending_dismissals` so a body-click on a
            // toast that was already despawned by the same
            // frame's click-to-dismiss is silently dropped
            // (stale entity id).
            .configure_sets(
                Update,
                NotificationsSystemSet::Coalesce.before(NotificationsSystemSet::Tick),
            )
            .add_systems(
                Update,
                (
                    systems::auto_dismiss_toasts,
                    systems::apply_pending_dismissals,
                    systems::pause_on_event_toasts,
                    systems::click_to_focus,
                )
                    .in_set(NotificationsSystemSet::Tick)
                    .after(NotificationsSystemSet::EventBridge),
            );
    }
}
