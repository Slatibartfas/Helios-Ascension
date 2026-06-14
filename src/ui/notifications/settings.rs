//! Player-facing notification settings.
//!
//! `NotificationSettings` is the runtime analogue of the categories
//! manifest: the manifest is read-only data, the settings are what
//! the player has toggled at runtime. The two are queried together
//! by PR-B's spawn system to decide whether to actually show a toast.

use bevy::prelude::*;
use std::collections::HashMap;

/// Stable category id, with a `From<&str>` for ergonomic call sites.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Reflect)]
pub struct NotificationCategoryId(pub String);

impl NotificationCategoryId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for NotificationCategoryId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for NotificationCategoryId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Per-category override. PR-A only models the on/off toggle; PR-D
/// adds severity floor, sound, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerCategorySetting {
    pub enabled: bool,
}

/// Global player preferences for notifications.
#[derive(Resource, Debug, Clone)]
pub struct NotificationSettings {
    /// Master switch. If `false`, PR-B's spawn system discards every
    /// incoming event regardless of category.
    pub global_enabled: bool,
    /// If `true`, only events raised while the player is on the
    /// survey tab surface. The dossier/menu contexts remain quiet.
    /// PR-B reads this; PR-A just stores it.
    pub show_only_in_survey: bool,
    /// Cap on the number of on-screen toasts. When exceeded, the
    /// oldest non-sticky toast is evicted.
    pub max_visible_toasts: u32,
    /// Window (seconds) in which two events with the same `dedup_key`
    /// are folded into a single toast by PR-D.
    pub default_group_window_s: f32,
    /// Per-category overrides. Populated lazily on first write by
    /// PR-B; PR-A just exposes the empty map and `get_or_default`.
    pub per_category: HashMap<NotificationCategoryId, PerCategorySetting>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            global_enabled: true,
            show_only_in_survey: true,
            max_visible_toasts: 5,
            default_group_window_s: 2.0,
            per_category: HashMap::new(),
        }
    }
}

impl NotificationSettings {
    /// Resolve whether a category is currently on. Falls back to the
    /// categories manifest's `enabled` flag (passed in by the caller)
    /// when the player has no explicit override yet.
    pub fn is_category_enabled(
        &self,
        category: &NotificationCategoryId,
        manifest_default_enabled: bool,
    ) -> bool {
        if !self.global_enabled {
            return false;
        }
        match self.per_category.get(category) {
            Some(override_) => override_.enabled,
            None => manifest_default_enabled,
        }
    }

    /// Get the per-category override, inserting the manifest default
    /// if absent. Callers use this to render a settings UI that
    /// matches what the spawn system would see.
    pub fn get_or_default(
        &mut self,
        category: &NotificationCategoryId,
        manifest_default_enabled: bool,
    ) -> PerCategorySetting {
        self.per_category
            .entry(category.clone())
            .or_insert(PerCategorySetting {
                enabled: manifest_default_enabled,
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = NotificationSettings::default();
        assert!(s.global_enabled);
        assert!(s.show_only_in_survey);
        assert_eq!(s.max_visible_toasts, 5);
        assert!((s.default_group_window_s - 2.0).abs() < f32::EPSILON);
        assert!(s.per_category.is_empty());
    }

    #[test]
    fn test_global_off_wins() {
        let mut s = NotificationSettings::default();
        s.global_enabled = false;
        let id = NotificationCategoryId::from("survey.test");
        assert!(!s.is_category_enabled(&id, true));
    }

    #[test]
    fn test_per_category_override_wins_over_manifest() {
        let mut s = NotificationSettings::default();
        let id = NotificationCategoryId::from("survey.test");
        s.per_category
            .insert(id.clone(), PerCategorySetting { enabled: false });
        // Manifest says on, override says off → off.
        assert!(!s.is_category_enabled(&id, true));
        // Manifest says off, no override → off.
        let other = NotificationCategoryId::from("survey.other");
        assert!(!s.is_category_enabled(&other, false));
    }

    #[test]
    fn test_category_id_from_str() {
        let id: NotificationCategoryId = "foo".into();
        assert_eq!(id.as_str(), "foo");
    }
}
