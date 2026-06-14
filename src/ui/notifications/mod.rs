//! Player-facing notifications system (toast-style HUD overlay).
//!
//! PR-A (GRA-135) lays the type & data foundation. No systems render or
//! consume events in this PR — wiring lands in PR-B once the tick and
//! render layers are designed.
//!
//! Module map:
//! - [`events`]    — `NotificationEvent` Bevy `Message` produced by sim
//!   bridges and consumed by the spawn system in PR-B.
//! - [`data`]      — RON loader for `assets/data/notifications.ron`;
//!   `NotificationCategoriesData` resource.
//! - [`settings`]  — `NotificationSettings` resource (per-category
//!   overrides + global knobs).
//! - [`components`] — `ActiveNotification` per-toast component.
//! - [`systems`]   — empty sub-module; systems land in PR-B.

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod events;
pub mod settings;
pub mod systems;

pub use components::{ActiveNotification, PendingNotificationDismissal};
pub use data::{load_notification_categories, NotificationCategoriesData, NotificationCategory};
pub use events::{NotificationEvent, NotificationSeverity};
pub use settings::{NotificationCategoryId, NotificationSettings};

/// Sub-plugin that owns the notifications feature surface.
///
/// PR-A registers the resource + the dev-introspection `Reflect`
/// handle for `ActiveNotification`. Systems are added in PR-B.
pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotificationSettings>()
            .init_resource::<NotificationCategoriesData>()
            .init_resource::<PendingNotificationDismissal>()
            .add_systems(Startup, load_notification_categories)
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
            .register_type::<NotificationSeverity>();
    }
}
