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
/// adds severity floor, sound, etc. PR-E (GRA-139) extends with the
/// fields the settings panel renders (pause_on_event, sound_on,
/// auto_dismiss_s, sticky). PR-F (GRA-140) wires the
/// `pause_on_event` field to the actual `TimeScale::pause()` call
/// (the toggle is visually exposed in the settings panel from
/// PR-E).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerCategorySetting {
    pub enabled: bool,
    /// Pause the simulation when an event in this category fires.
    /// Visually exposed in the settings panel from PR-E; PR-F
    /// (GRA-140) wires the actual `TimeScale::pause()` call.
    pub pause_on_event: bool,
    /// Play a sound for this category. Always rendered-on in the
    /// settings panel; the audio backend is a deferred feature.
    pub sound_on: bool,
    /// Override the manifest's `default_dismiss_s`. The spawn system
    /// prefers this over the manifest default.
    pub auto_dismiss_s: f32,
    /// If true, the toast ignores the auto-dismiss timer and stays
    /// until the player dismisses it.
    pub sticky: bool,
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

    /// Resolve whether a category currently requests
    /// `TimeScale::pause()` on insert. The per-category override
    /// wins; the caller passes the manifest's
    /// `NotificationCategory::pause_on_event` as the fallback.
    /// PR-F (GRA-140) drives this from `tick.rs`.
    pub fn is_category_pause_on_event(
        &self,
        category: &NotificationCategoryId,
        manifest_default_pause_on_event: bool,
    ) -> bool {
        match self.per_category.get(category) {
            Some(override_) => override_.pause_on_event,
            None => manifest_default_pause_on_event,
        }
    }

    /// Get the per-category override, inserting the manifest default
    /// if absent. Callers use this to render a settings UI that
    /// matches what the spawn system would see. `sticky` defaults
    /// from `manifest_default_dismiss_s <= 0.0` to match
    /// `reset_all` — categories designed sticky in the manifest
    /// (e.g. `economy.stockpile_critical`, `encounters.hostile_contact`)
    /// stay sticky on first open instead of being silently flipped
    /// to non-sticky.
    pub fn get_or_default(
        &mut self,
        category: &NotificationCategoryId,
        manifest_default_enabled: bool,
        manifest_default_dismiss_s: f32,
    ) -> PerCategorySetting {
        *self
            .per_category
            .entry(category.clone())
            .or_insert(PerCategorySetting {
                enabled: manifest_default_enabled,
                pause_on_event: false,
                sound_on: true,
                auto_dismiss_s: manifest_default_dismiss_s,
                sticky: manifest_default_dismiss_s <= 0.0,
            })
    }

    /// Reset every field back to the same defaults
    /// `get_or_default` would write on first read. Per-category
    /// overrides that already have explicit entries are also reset
    /// to the same values, since the only "default" the player
    /// sees is the manifest's row.
    pub fn reset_all(&mut self, manifest: &super::data::NotificationCategoriesData) {
        self.global_enabled = true;
        self.show_only_in_survey = true;
        self.max_visible_toasts = 5;
        self.default_group_window_s = 2.0;
        self.per_category.clear();
        for (id, cat) in manifest.iter() {
            self.per_category.insert(
                id.clone(),
                PerCategorySetting {
                    enabled: cat.enabled,
                    pause_on_event: false,
                    sound_on: true,
                    auto_dismiss_s: cat.default_dismiss_s,
                    sticky: cat.default_dismiss_s <= 0.0,
                },
            );
        }
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
        let s = NotificationSettings {
            global_enabled: false,
            ..NotificationSettings::default()
        };
        let id = NotificationCategoryId::from("survey.test");
        assert!(!s.is_category_enabled(&id, true));
    }

    #[test]
    fn test_per_category_override_wins_over_manifest() {
        let mut s = NotificationSettings::default();
        let id = NotificationCategoryId::from("survey.test");
        s.per_category.insert(
            id.clone(),
            PerCategorySetting {
                enabled: false,
                pause_on_event: false,
                sound_on: true,
                auto_dismiss_s: 5.0,
                sticky: false,
            },
        );
        // Manifest says on, override says off → off.
        assert!(!s.is_category_enabled(&id, true));
        // Manifest says off, no override → off.
        let other = NotificationCategoryId::from("survey.other");
        assert!(!s.is_category_enabled(&other, false));
    }

    /// PR-F: the per-category override wins over the manifest
    /// default for `pause_on_event`.
    #[test]
    fn test_pause_on_event_override_wins_over_manifest() {
        let mut s = NotificationSettings::default();
        let id = NotificationCategoryId::from("survey.mission_failed");
        s.per_category.insert(
            id.clone(),
            PerCategorySetting {
                enabled: true,
                pause_on_event: true,
                sound_on: true,
                auto_dismiss_s: 6.0,
                sticky: false,
            },
        );
        // Manifest says no-pause, override says pause → pause.
        assert!(s.is_category_pause_on_event(&id, false));
        // Manifest says pause, no override → pause.
        let other = NotificationCategoryId::from("survey.complete");
        assert!(s.is_category_pause_on_event(&other, true));
        // Manifest says no-pause, no override → no-pause.
        let other2 = NotificationCategoryId::from("survey.dimension_unlocked");
        assert!(!s.is_category_pause_on_event(&other2, false));
    }

    #[test]
    fn test_category_id_from_str() {
        let id: NotificationCategoryId = "foo".into();
        assert_eq!(id.as_str(), "foo");
    }

    #[test]
    fn test_get_or_default_sticky_matches_manifest_sticky() {
        // Kilo finding: `get_or_default` previously hard-coded
        // `sticky: false`, so categories designed sticky in the
        // manifest (e.g. `economy.stockpile_critical`,
        // `encounters.hostile_contact` with `default_dismiss_s = 0.0`)
        // were silently flipped to non-sticky on first open. This
        // mirrors the `reset_all` rule (line 146) so the per-category
        // map the settings panel renders matches the manifest.
        let mut s = NotificationSettings::default();
        let sticky_id = NotificationCategoryId::from("economy.stockpile_critical");
        let normal_id = NotificationCategoryId::from("survey.mission_complete");

        let sticky_row = s.get_or_default(&sticky_id, true, 0.0);
        assert!(
            sticky_row.sticky,
            "default_dismiss_s = 0.0 must mean sticky"
        );

        let normal_row = s.get_or_default(&normal_id, true, 5.0);
        assert!(
            !normal_row.sticky,
            "default_dismiss_s > 0.0 must mean non-sticky"
        );
    }

    #[test]
    fn test_reset_to_defaults_restores_initial_values() {
        // GRA-139 acceptance: set every field to a non-default value,
        // call `reset_all`, assert the resource matches the freshly
        // constructed `NotificationSettings::default()` (modulo the
        // per_category map, which is populated from the manifest on
        // reset).
        use super::super::data::NotificationCategoriesData;

        let mut s = NotificationSettings {
            global_enabled: false,
            show_only_in_survey: false,
            max_visible_toasts: 1,
            default_group_window_s: 0.0,
            per_category: std::iter::once((
                NotificationCategoryId::from("survey.test"),
                PerCategorySetting {
                    enabled: false,
                    pause_on_event: true,
                    sound_on: false,
                    auto_dismiss_s: 99.0,
                    sticky: true,
                },
            ))
            .collect(),
        };

        // Build a tiny manifest to feed the reset.
        let mut data = NotificationCategoriesData::default();
        data.categories.insert(
            NotificationCategoryId::from("survey.test"),
            super::super::data::NotificationCategory {
                id: "survey.test".to_string(),
                display_name: "Test".to_string(),
                default_dismiss_s: 6.0,
                enabled: true,
                pause_on_event: false,
            },
        );
        s.reset_all(&data);

        let fresh = NotificationSettings::default();
        assert_eq!(s.global_enabled, fresh.global_enabled);
        assert_eq!(s.show_only_in_survey, fresh.show_only_in_survey);
        assert_eq!(s.max_visible_toasts, fresh.max_visible_toasts);
        assert!((s.default_group_window_s - fresh.default_group_window_s).abs() < f32::EPSILON);

        // Per-category map is now populated from the manifest and
        // matches the manifest row for "survey.test".
        let row = s
            .per_category
            .get(&NotificationCategoryId::from("survey.test"))
            .expect("reset must populate the manifest rows");
        assert!(row.enabled, "manifest default enabled = true");
        assert!(!row.pause_on_event);
        assert!(row.sound_on);
        assert!((row.auto_dismiss_s - 6.0).abs() < f32::EPSILON);
        assert!(!row.sticky, "default_dismiss_s=6.0 → not sticky");
    }
}
