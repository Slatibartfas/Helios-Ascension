//! Player-facing notifications system (toast-style HUD overlay).
//!
//! PR-A (GRA-135) lays the type & data foundation. PR-B (GRA-136)
//! adds the tick + render layers that make toasts appear and
//! dismiss. PR-C / D / E add event bridges, coalescing, and a
//! settings panel respectively.
//!
//! Module map:
//! - [`events`]    — `NotificationEvent` Bevy `Message` produced by sim
//!   bridges and consumed by the spawn system in PR-B / PR-C.
//! - [`data`]      — RON loader for `assets/data/notifications.ron`;
//!   `NotificationCategoriesData` resource.
//! - [`settings`]  — `NotificationSettings` resource (per-category
//!   overrides + global knobs).
//! - [`components`] — `ActiveNotification` per-toast component +
//!   `PendingNotificationDismissal` action-queue resource.
//! - [`systems`]   — `tick` (auto-dismiss + click-dismiss drain)
//!   and `render` (egui top-right panel), `coalesce` (PR-D).
//! - [`ui_settings`] — the settings panel modal (PR-E / GRA-139).

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod events;
pub mod settings;
pub mod systems;
pub mod ui_settings;

pub use components::{ActiveNotification, PendingNotificationDismissal};
pub use data::{load_notification_categories, NotificationCategoriesData, NotificationCategory};
pub use events::{NotificationEvent, NotificationSeverity};
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
/// settings panel modal.
pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotificationSettings>()
            .init_resource::<NotificationCategoriesData>()
            .init_resource::<PendingNotificationDismissal>()
            .add_systems(Startup, load_notification_categories)
            // Settings panel (PR-E / GRA-139) — modal renderer, the
            // "Notifications" button in the top menu bar toggles its
            // visibility via `NotificationsSettingsOpen`.
            .add_plugins(NotificationsSettingsPanelPlugin)
            // PR-D (GRA-138): the coalesce/grouping pass. Runs in
            // `Update`, ungrouped for now — PR-B's
            // `NotificationsSystemSet` (defined in PR-B's
            // `src/ui/notifications/systems/mod.rs`) re-groups
            // this system into the `Coalesce` set on top of
            // PR-D's merge.
            .add_systems(Update, coalesce_notifications)
            // Dev introspection hook for bevy_inspector_egi. The
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
            // PR-B systems. The Tick set runs in Update; the
            // Render set is added to EguiPrimaryContextPass by
            // `UIPlugin::build` so it can chain after
            // `UiSystemSet::Overlays` (see src/ui/mod.rs).
            .add_systems(
                Update,
                (
                    systems::auto_dismiss_toasts,
                    systems::apply_pending_dismissals,
                )
                    .in_set(NotificationsSystemSet::Tick),
            );
    }
}
