//! Categories manifest loader.
//!
//! Reads `assets/data/notifications.ron` once at startup and inserts
//! the parsed [`NotificationCategoriesData`] resource. The RON file
//! lists every category the player can opt into; PR-A doesn't
//! enforce uniqueness at compile time, but the loader panics in dev
//! builds if two rows share an id (`expect_unique`) so a typo can't
//! silently double-load.
//!
//! Schema: see `assets/data/notifications.ron`. Each row has
//! `id`, `display_name`, `default_dismiss_s`, and `enabled`. Future
//! fields (icon key, default severity, sound) can be added without
//! touching this loader — Bevy's RON deserialiser ignores missing
//! fields when the Rust struct uses `#[serde(default)]`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::settings::NotificationCategoryId;

/// One row in the categories manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCategory {
    /// Stable id (e.g. `survey.mission_complete`). Doubles as the
    /// dedup key prefix if an event doesn't supply its own.
    pub id: String,
    /// Player-facing label (e.g. "Survey complete").
    pub display_name: String,
    /// Default auto-dismiss timer in seconds. The per-event
    /// `auto_dismiss_s` field overrides this when present.
    pub default_dismiss_s: f32,
    /// Whether the player has this category turned on by default.
    /// Per-category overrides in `NotificationSettings` win.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Loaded manifest. Keyed by id for O(1) lookup from the spawn system
/// and from the settings UI.
#[derive(Resource, Debug, Clone, Default)]
pub struct NotificationCategoriesData {
    pub categories: HashMap<NotificationCategoryId, NotificationCategory>,
}

impl NotificationCategoriesData {
    pub fn get(&self, id: &NotificationCategoryId) -> Option<&NotificationCategory> {
        self.categories.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&NotificationCategoryId, &NotificationCategory)> {
        self.categories.iter()
    }

    pub fn len(&self) -> usize {
        self.categories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }
}

/// Top-level shape of the RON file.
#[derive(Debug, Deserialize)]
struct NotificationCategoriesFile {
    categories: Vec<NotificationCategory>,
}

/// Startup system that loads `assets/data/notifications.ron` and
/// inserts the resource. Mirrors the pattern in
/// `research::data::load_technologies` — read with `fs::read_to_string`,
/// parse with `ron::from_str`, fall back to an empty manifest on
/// failure so the game still starts if the file is missing.
pub fn load_notification_categories(mut commands: Commands) {
    let path = "assets/data/notifications.ron";

    match std::fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<NotificationCategoriesFile>(&contents) {
            Ok(file) => {
                let mut data = NotificationCategoriesData::default();
                for cat in file.categories {
                    let id = NotificationCategoryId(cat.id.clone());
                    if data.categories.insert(id.clone(), cat).is_some() {
                        // Duplicate id — fail loudly in dev. The
                        // HashMap clobber would silently drop a
                        // category and the player would never see
                        // those toasts.
                        #[cfg(debug_assertions)]
                        panic!(
                            "assets/data/notifications.ron: duplicate category id {:?}",
                            id.0
                        );
                        #[cfg(not(debug_assertions))]
                        warn!(
                            "assets/data/notifications.ron: duplicate category id {:?} (keeping last)",
                            id.0
                        );
                    }
                }
                info!("Loaded {} notification categories", data.categories.len());
                commands.insert_resource(data);
            }
            Err(e) => {
                error!("Failed to parse notifications manifest: {}", e);
                commands.insert_resource(NotificationCategoriesData::default());
            }
        },
        Err(e) => {
            warn!(
                "Notifications manifest not found at {}: {}. Using empty categories.",
                path, e
            );
            commands.insert_resource(NotificationCategoriesData::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_notification_categories_from_disk() {
        // Round-trip: load the real manifest and assert we got the
        // design-floor count. The issue acceptance is "count >= 20".
        let contents = std::fs::read_to_string("assets/data/notifications.ron")
            .expect("notifications.ron must exist for this test");
        let file: NotificationCategoriesFile =
            ron::from_str(&contents).expect("notifications.ron must parse cleanly");
        assert!(
            file.categories.len() >= 20,
            "expected at least 20 categories, got {}",
            file.categories.len()
        );
    }

    #[test]
    fn test_categories_resource_lookup_round_trip() {
        // Build a resource directly from a few rows and assert the
        // HashMap lookup is wired correctly.
        let mut data = NotificationCategoriesData::default();
        let id = NotificationCategoryId("survey.test".to_string());
        data.categories.insert(
            id.clone(),
            NotificationCategory {
                id: "survey.test".to_string(),
                display_name: "Test".to_string(),
                default_dismiss_s: 3.0,
                enabled: true,
            },
        );
        let looked = data.get(&id).expect("inserted category must be findable");
        assert_eq!(looked.display_name, "Test");
        assert!(looked.enabled);
    }
}
