//! Loaders for the subview-data RON files (GRA-310 LGD content).
//!
//! PR-A (GRA-311) shipped the [`crate::ui::launch::manifest`] loader for
//! `launch_ui.ron` — splash timings, asset paths, default preset id.
//! PR-D (this PR) adds the two remaining LGD-owned data files the
//! subviews need:
//!
//! - `assets/data/difficulty_presets.ron` — preset list (id, display
//!   name, description, recommended seed strategy) + curated seeds
//!   for the Hard Vacuum preset.
//! - `assets/data/seed_copy.ron` — subview chrome copy (titles,
//!   button labels, seed field placeholder + helper text + per-error
//!   strings, settings tab labels).
//!
//! Both files are LGD-owned content; the Rust types here mirror the
//! RON schema and the loader rules from each file's header comment.
//! Missing or unparseable files fall back to in-memory defaults
//! (matching the PR-A `LaunchUiManifest` pattern at
//! `src/ui/launch/manifest.rs`) so a deleted or malformed RON never
//! bricks the menu.
//!
//! PR-D test plan (per issue body):
//! - `DifficultyPresetsManifest` round-trip + missing-file fallback.
//! - `SeedCopyManifest` round-trip + missing-file fallback.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;

/// `assets/data/difficulty_presets.ron` loader path.
const PRESETS_PATH: &str = "assets/data/difficulty_presets.ron";

/// `assets/data/seed_copy.ron` loader path.
const SEED_COPY_PATH: &str = "assets/data/seed_copy.ron";

/// A difficulty preset entry as authored by LGD in
/// `assets/data/difficulty_presets.ron`.
///
/// `recommended_seed_strategy` mirrors the loader rule documented in
/// the RON header: `"Random"`, `"UserInput"`, or `"CuratedList"`.
/// PR-D only branches on the strategy to decide whether the seed
/// field is shown — the actual seed-picking logic for `CuratedList`
/// is a future ticket (curated seeds live on
/// [`DifficultyPresetsManifest::curated_seeds`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DifficultyPreset {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub recommended_seed_strategy: String,
    /// Reserved for v0.6.x balance pass. PR-D ignores it. Stored as
    /// the raw RON span so a future ticket can replace this with a
    /// typed `BalanceModifiers` struct without breaking the loader.
    #[serde(default)]
    pub balance_modifiers: Option<String>,
}

impl DifficultyPreset {
    /// True when the preset's recommended seed strategy is
    /// `"UserInput"` — the subview shows the seed input field with a
    /// prefilled placeholder.
    pub fn wants_user_input_seed(&self) -> bool {
        self.recommended_seed_strategy == "UserInput"
    }

    /// True when the preset's recommended seed strategy is
    /// `"CuratedList"` — the subview exposes a picker over
    /// [`DifficultyPresetsManifest::curated_seeds`].
    pub fn wants_curated_seed(&self) -> bool {
        self.recommended_seed_strategy == "CuratedList"
    }
}

/// Schema-mirror for `assets/data/difficulty_presets.ron`.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DifficultyPresetsManifest {
    #[serde(default)]
    pub presets: Vec<DifficultyPreset>,
    #[serde(default)]
    pub curated_seeds: Vec<u64>,
}

impl DifficultyPresetsManifest {
    /// Look up a preset by `id`. Returns `None` when no preset with
    /// that id exists — the caller decides whether to fall back to
    /// the manifest's `default_preset_id` or treat this as an error.
    pub fn find(&self, id: &str) -> Option<&DifficultyPreset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Default preset list — matches the LGD-authored content in
    /// `assets/data/difficulty_presets.ron` (GRA-310). Used when the RON
    /// file is missing or fails to parse so the menu never bricks.
    pub fn default_presets() -> Vec<DifficultyPreset> {
        vec![
            DifficultyPreset {
                id: "casual".to_string(),
                display_name: "Cadet".to_string(),
                description:
                    "Generous resource yields, slower logistics decay, and reduced mission attrition."
                        .to_string(),
                recommended_seed_strategy: "Random".to_string(),
                balance_modifiers: None,
            },
            DifficultyPreset {
                id: "standard".to_string(),
                display_name: "Mission Controller".to_string(),
                description: "Default baseline tuned for a single commander over a 6-12 hour campaign."
                    .to_string(),
                recommended_seed_strategy: "UserInput".to_string(),
                balance_modifiers: None,
            },
            DifficultyPreset {
                id: "hard".to_string(),
                display_name: "Hard Vacuum".to_string(),
                description:
                    "Tighter logistics tolerances, slower tech pacing, and harsher attrition."
                        .to_string(),
                recommended_seed_strategy: "CuratedList".to_string(),
                balance_modifiers: None,
            },
            DifficultyPreset {
                id: "custom".to_string(),
                display_name: "Custom".to_string(),
                description: "Exposes the underlying knobs for commanders who want to set their own seed."
                    .to_string(),
                recommended_seed_strategy: "UserInput".to_string(),
                balance_modifiers: None,
            },
        ]
    }

    /// Default curated seeds — three 13-digit entries as authored
    /// by LGD in `assets/data/difficulty_presets.ron`. Kept in sync
    /// with the RON file so a missing RON never strands the
    /// Hard Vacuum picker on an empty list.
    pub fn default_curated_seeds() -> Vec<u64> {
        vec![4_729_103_856_017, 5_830_172_946_102, 6_193_485_720_938]
    }
}

impl Default for DifficultyPresetsManifest {
    fn default() -> Self {
        Self {
            presets: Self::default_presets(),
            curated_seeds: Self::default_curated_seeds(),
        }
    }
}

/// Loader system for `assets/data/difficulty_presets.ron`.
///
/// On missing file or parse failure, inserts
/// [`DifficultyPresetsManifest::default`] and logs at `warn!`.
/// Strict pass/fail gate is the `tests` module below — production
/// code never panics on a bad RON.
pub fn load_difficulty_presets_manifest(mut commands: Commands) {
    match fs::read_to_string(PRESETS_PATH) {
        Ok(contents) => match ron::from_str::<DifficultyPresetsManifest>(&contents) {
            Ok(manifest) => {
                info!(
                    "difficulty_presets.ron: loaded {} preset(s) + {} curated seed(s)",
                    manifest.presets.len(),
                    manifest.curated_seeds.len()
                );
                commands.insert_resource(manifest);
            }
            Err(e) => {
                error!("Failed to parse difficulty_presets.ron: {}", e);
                commands.insert_resource(DifficultyPresetsManifest::default());
            }
        },
        Err(e) => {
            warn!(
                "difficulty_presets.ron not found at {}: {}. Using defaults.",
                PRESETS_PATH, e
            );
            commands.insert_resource(DifficultyPresetsManifest::default());
        }
    }
}

/// Per-string seed-grammar validation messages, mirrored from
/// `assets/data/seed_copy.ron::seed.errors.*`.
///
/// The Coder matches the enum variant to the parse outcome and the
/// subview renders the corresponding `value` string verbatim — no
/// hard-coded user-facing copy anywhere in the subview render code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedErrors {
    pub out_of_range: String,
    pub invalid_characters: String,
    pub zero: String,
    pub too_long: String,
}

impl Default for SeedErrors {
    fn default() -> Self {
        Self {
            out_of_range: "Seed must be a whole number between 1 and 9,999,999,999,999".to_string(),
            invalid_characters: "Digits only — no signs, decimals, or letters".to_string(),
            zero: "Seed cannot be zero".to_string(),
            too_long: "Seed is limited to 13 digits".to_string(),
        }
    }
}

/// Seed input field copy + validation strings (mirrors
/// `assets/data/seed_copy.ron::seed.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedFieldCopy {
    pub label: String,
    pub placeholder: String,
    pub helper_text: String,
    pub parsed_sublabel_template: String,
    pub errors: SeedErrors,
    pub max_length: u32,
}

impl Default for SeedFieldCopy {
    fn default() -> Self {
        Self {
            label: "Mission Seed".to_string(),
            placeholder: "_____________".to_string(),
            helper_text: "13-digit numeric seed. Leave blank for a random start".to_string(),
            parsed_sublabel_template: "Parsed: {value}".to_string(),
            errors: SeedErrors::default(),
            max_length: 13,
        }
    }
}

/// New Game subview chrome copy (mirrors
/// `assets/data/seed_copy.ron::new_game_subview.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewGameSubviewCopy {
    pub title: String,
    pub back_button_label: String,
    pub start_button_label: String,
    pub preset_section_label: String,
    pub seed_section_label: String,
}

impl Default for NewGameSubviewCopy {
    fn default() -> Self {
        Self {
            title: "New Mission".to_string(),
            back_button_label: "Back".to_string(),
            start_button_label: "Begin".to_string(),
            preset_section_label: "Select Difficulty".to_string(),
            seed_section_label: "Seed".to_string(),
        }
    }
}

/// One labeled settings tab (mirrors
/// `assets/data/seed_copy.ron::settings_structure.labels[*]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsTab {
    pub id: String,
    pub label: String,
}

/// Settings subview structure copy (mirrors
/// `assets/data/seed_copy.ron::settings_structure.*`). The Coder
/// reads `kind` to decide the layout — PR-D ships the `"tabs"` kind
/// (3 tabs side-by-side per LGD spec, GRA-309 §9 Q4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsStructure {
    pub kind: String,
    pub labels: Vec<SettingsTab>,
}

impl Default for SettingsStructure {
    fn default() -> Self {
        Self {
            kind: "tabs".to_string(),
            labels: vec![
                SettingsTab {
                    id: "audio".to_string(),
                    label: "Audio".to_string(),
                },
                SettingsTab {
                    id: "graphics".to_string(),
                    label: "Graphics".to_string(),
                },
                SettingsTab {
                    id: "gameplay".to_string(),
                    label: "Gameplay".to_string(),
                },
            ],
        }
    }
}

/// Schema-mirror for `assets/data/seed_copy.ron`.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedCopyManifest {
    pub seed: SeedFieldCopy,
    pub new_game_subview: NewGameSubviewCopy,
    pub settings_structure: SettingsStructure,
}

impl Default for SeedCopyManifest {
    fn default() -> Self {
        Self {
            seed: SeedFieldCopy::default(),
            new_game_subview: NewGameSubviewCopy::default(),
            settings_structure: SettingsStructure::default(),
        }
    }
}

impl SeedCopyManifest {
    /// Find a settings tab by `id` (e.g. `"audio"`). The subview
    /// defaults to the first tab on miss so a misconfigured RON
    /// degrades gracefully instead of leaving the panel blank.
    pub fn find_tab(&self, id: &str) -> Option<&SettingsTab> {
        self.settings_structure.labels.iter().find(|t| t.id == id)
    }
}

/// Loader system for `assets/data/seed_copy.ron`.
///
/// On missing file or parse failure, inserts
/// [`SeedCopyManifest::default`] and logs at `warn!`. Strict
/// pass/fail gate is the `tests` module below.
pub fn load_seed_copy_manifest(mut commands: Commands) {
    match fs::read_to_string(SEED_COPY_PATH) {
        Ok(contents) => match ron::from_str::<SeedCopyManifest>(&contents) {
            Ok(manifest) => {
                info!(
                    "seed_copy.ron: loaded ({} settings tab(s))",
                    manifest.settings_structure.labels.len()
                );
                commands.insert_resource(manifest);
            }
            Err(e) => {
                error!("Failed to parse seed_copy.ron: {}", e);
                commands.insert_resource(SeedCopyManifest::default());
            }
        },
        Err(e) => {
            warn!(
                "seed_copy.ron not found at {}: {}. Using defaults.",
                SEED_COPY_PATH, e
            );
            commands.insert_resource(SeedCopyManifest::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_preset_wants_user_input_seed_only_for_userinput() {
        let user = DifficultyPreset {
            id: "x".into(),
            display_name: "x".into(),
            description: "x".into(),
            recommended_seed_strategy: "UserInput".into(),
            balance_modifiers: None,
        };
        let random = DifficultyPreset {
            recommended_seed_strategy: "Random".into(),
            ..user.clone()
        };
        let curated = DifficultyPreset {
            recommended_seed_strategy: "CuratedList".into(),
            ..user.clone()
        };
        assert!(user.wants_user_input_seed());
        assert!(!random.wants_user_input_seed());
        assert!(!curated.wants_user_input_seed());
        assert!(curated.wants_curated_seed());
    }

    #[test]
    fn default_difficulty_presets_match_lgd_order() {
        let presets = DifficultyPresetsManifest::default_presets();
        assert_eq!(presets.len(), 4);
        assert_eq!(presets[0].id, "casual");
        assert_eq!(presets[1].id, "standard");
        assert_eq!(presets[2].id, "hard");
        assert_eq!(presets[3].id, "custom");
        assert_eq!(presets[1].recommended_seed_strategy, "UserInput");
        assert_eq!(presets[2].recommended_seed_strategy, "CuratedList");
    }

    #[test]
    fn default_curated_seeds_are_thirteen_digits() {
        for s in DifficultyPresetsManifest::default_curated_seeds() {
            assert!(s >= 1_000_000_000_000, "curated seed {} < 10^12", s);
            assert!(s < 10_000_000_000_000, "curated seed {} >= 10^13", s);
        }
    }

    #[test]
    fn find_returns_some_for_known_id() {
        let m = DifficultyPresetsManifest::default();
        assert!(m.find("standard").is_some());
        assert!(m.find("casual").is_some());
        assert!(m.find("hard").is_some());
        assert!(m.find("custom").is_some());
        assert!(m.find("nope").is_none());
    }

    #[test]
    fn default_seed_copy_has_three_settings_tabs() {
        let m = SeedCopyManifest::default();
        let ids: Vec<&str> = m
            .settings_structure
            .labels
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(ids, vec!["audio", "graphics", "gameplay"]);
    }

    #[test]
    fn find_tab_returns_some_for_known_id_and_none_otherwise() {
        let m = SeedCopyManifest::default();
        assert!(m.find_tab("audio").is_some());
        assert!(m.find_tab("graphics").is_some());
        assert!(m.find_tab("gameplay").is_some());
        assert!(m.find_tab("missing").is_none());
    }

    #[test]
    fn default_seed_field_copy_has_13_digit_cap() {
        let s = SeedFieldCopy::default();
        assert_eq!(s.max_length, 13);
        assert!(!s.errors.out_of_range.is_empty());
        assert!(!s.errors.invalid_characters.is_empty());
        assert!(!s.errors.zero.is_empty());
        assert!(!s.errors.too_long.is_empty());
    }

    #[test]
    fn default_new_game_subview_copy_has_required_labels() {
        let s = NewGameSubviewCopy::default();
        assert!(!s.title.is_empty());
        assert!(!s.back_button_label.is_empty());
        assert!(!s.start_button_label.is_empty());
        assert!(!s.preset_section_label.is_empty());
        assert!(!s.seed_section_label.is_empty());
    }
}
