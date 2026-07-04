//! Loader for `assets/data/launch_ui.ron`.
//!
//! The manifest holds splash timings, asset paths, build-label format,
//! the default preset id (GRA-309 §3.2), and the main-menu button copy
//! (GRA-309 §3.4 / GRA-317 PR-C). LGD-owned content; the Rust types
//! here are the wire shape so copy / timings can change without
//! recompilation. Schema lives in `assets/data/launch_ui.ron`; this
//! file only mirrors it.
//!
//! PR-A (GRA-311) registers the loader at Startup and inserts the
//! resulting [`LaunchUiManifest`] resource. PR-B / PR-C / PR-D consume
//! the resource for splash rendering, menu wiring, and the new-game
//! subview. PR-A itself does not touch the manifest values.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;

/// Schema-mirror for `assets/data/launch_ui.ron`.
///
/// All fields match the GRA-309 §3.2 schema and the LGD content shipped
/// in GRA-310 (PR #196). The `menu` block was added by GRA-317 PR-C so
/// the main menu shell can read button copy + shortcut hints from the
/// RON manifest instead of hard-coding English literals.
///
/// Loader rules from the RON header:
///
/// 1. `logo_splashscreen` and `logo_clean` resolve under `assets/`
///    (forward-slash relative).
/// 2. `splash_min_duration_s <= splash_max_duration_s`; both > 0.
/// 3. `build_label_format` is a template with `{version}` / `{sha}`
///    placeholders; unknowns pass through literally.
/// 4. `version`, when `None`, falls back to `env!("CARGO_PKG_VERSION")`.
/// 5. `show_sha_in_release`, when `true`, overrides the
///    `cfg!(debug_assertions)` default for `{sha}` visibility.
/// 6. `menu.{continue,new_game,load_game,settings,quit}_label` /
///    `*_shortcut` may be empty — the `LaunchMenuCopy::resolved_*`
///    helpers fall back to the hard-coded English defaults so a
///    partially-edited RON never renders an empty button.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchUiManifest {
    pub splash_min_duration_s: f32,
    pub splash_max_duration_s: f32,
    pub continue_disabled_until_save_exists: bool,
    pub force_skip_splash: bool,
    pub logo_splashscreen: String,
    pub logo_clean: String,
    pub build_label_format: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub show_sha_in_release: bool,
    pub default_preset_id: String,
    #[serde(default)]
    pub menu: LaunchMenuCopy,
}

/// One row of the main menu action grid (GRA-309 §3.4 / GRA-317 PR-C).
///
/// `label` is the visible button text; `shortcut` is the keycap-style
/// hint rendered on the right side of the button (e.g. `1`, `Esc`).
/// Both are user-visible copy owned by LGD via the
/// `assets/data/launch_ui.ron` `menu` block; the
/// [`LaunchMenuCopy::resolved_*`] helpers substitute the hard-coded
/// English defaults when a RON field is empty (loader rule 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchMenuCopy {
    #[serde(default)]
    pub continue_label: String,
    #[serde(default)]
    pub continue_shortcut: String,
    #[serde(default)]
    pub new_game_label: String,
    #[serde(default)]
    pub new_game_shortcut: String,
    #[serde(default)]
    pub load_game_label: String,
    #[serde(default)]
    pub load_game_shortcut: String,
    #[serde(default)]
    pub settings_label: String,
    #[serde(default)]
    pub settings_shortcut: String,
    #[serde(default)]
    pub quit_label: String,
    #[serde(default)]
    pub quit_shortcut: String,
}

impl Default for LaunchMenuCopy {
    fn default() -> Self {
        Self {
            continue_label: "Continue".to_string(),
            continue_shortcut: "1".to_string(),
            new_game_label: "New Game".to_string(),
            new_game_shortcut: "2".to_string(),
            load_game_label: "Load Game".to_string(),
            load_game_shortcut: "3".to_string(),
            settings_label: "Settings".to_string(),
            settings_shortcut: "4".to_string(),
            quit_label: "Quit".to_string(),
            quit_shortcut: "Esc".to_string(),
        }
    }
}

impl LaunchMenuCopy {
    /// Hard-coded English fallbacks (loader rule 6).
    pub const DEFAULT_CONTINUE_LABEL: &'static str = "Continue";
    pub const DEFAULT_CONTINUE_SHORTCUT: &'static str = "1";
    pub const DEFAULT_NEW_GAME_LABEL: &'static str = "New Game";
    pub const DEFAULT_NEW_GAME_SHORTCUT: &'static str = "2";
    pub const DEFAULT_LOAD_GAME_LABEL: &'static str = "Load Game";
    pub const DEFAULT_LOAD_GAME_SHORTCUT: &'static str = "3";
    pub const DEFAULT_SETTINGS_LABEL: &'static str = "Settings";
    pub const DEFAULT_SETTINGS_SHORTCUT: &'static str = "4";
    pub const DEFAULT_QUIT_LABEL: &'static str = "Quit";
    pub const DEFAULT_QUIT_SHORTCUT: &'static str = "Esc";

    /// Resolved Continue button label.
    pub fn resolved_continue_label(&self) -> &str {
        if self.continue_label.is_empty() {
            Self::DEFAULT_CONTINUE_LABEL
        } else {
            &self.continue_label
        }
    }

    /// Resolved Continue shortcut hint.
    pub fn resolved_continue_shortcut(&self) -> &str {
        if self.continue_shortcut.is_empty() {
            Self::DEFAULT_CONTINUE_SHORTCUT
        } else {
            &self.continue_shortcut
        }
    }

    pub fn resolved_new_game_label(&self) -> &str {
        if self.new_game_label.is_empty() {
            Self::DEFAULT_NEW_GAME_LABEL
        } else {
            &self.new_game_label
        }
    }

    pub fn resolved_new_game_shortcut(&self) -> &str {
        if self.new_game_shortcut.is_empty() {
            Self::DEFAULT_NEW_GAME_SHORTCUT
        } else {
            &self.new_game_shortcut
        }
    }

    pub fn resolved_load_game_label(&self) -> &str {
        if self.load_game_label.is_empty() {
            Self::DEFAULT_LOAD_GAME_LABEL
        } else {
            &self.load_game_label
        }
    }

    pub fn resolved_load_game_shortcut(&self) -> &str {
        if self.load_game_shortcut.is_empty() {
            Self::DEFAULT_LOAD_GAME_SHORTCUT
        } else {
            &self.load_game_shortcut
        }
    }

    pub fn resolved_settings_label(&self) -> &str {
        if self.settings_label.is_empty() {
            Self::DEFAULT_SETTINGS_LABEL
        } else {
            &self.settings_label
        }
    }

    pub fn resolved_settings_shortcut(&self) -> &str {
        if self.settings_shortcut.is_empty() {
            Self::DEFAULT_SETTINGS_SHORTCUT
        } else {
            &self.settings_shortcut
        }
    }

    pub fn resolved_quit_label(&self) -> &str {
        if self.quit_label.is_empty() {
            Self::DEFAULT_QUIT_LABEL
        } else {
            &self.quit_label
        }
    }

    pub fn resolved_quit_shortcut(&self) -> &str {
        if self.quit_shortcut.is_empty() {
            Self::DEFAULT_QUIT_SHORTCUT
        } else {
            &self.quit_shortcut
        }
    }
}

impl Default for LaunchUiManifest {
    fn default() -> Self {
        Self {
            splash_min_duration_s: 1.5,
            splash_max_duration_s: 3.0,
            continue_disabled_until_save_exists: true,
            force_skip_splash: false,
            logo_splashscreen: "logo/logo_splashscreen.png".to_string(),
            logo_clean: "logo/logo_large.png".to_string(),
            build_label_format: "v{version}  •  build {sha}".to_string(),
            version: None,
            show_sha_in_release: false,
            default_preset_id: "standard".to_string(),
            menu: LaunchMenuCopy::default(),
        }
    }
}

impl LaunchUiManifest {
    /// Apply GRA-309 §3.2 loader rule 4: a `None` `version` field falls
    /// back to `env!("CARGO_PKG_VERSION")`. PR-A does not consume the
    /// result (rendering is PR-B/C/D); the method is here so consumers
    /// can rely on the fallback without re-implementing it.
    pub fn resolved_version(&self) -> String {
        match &self.version {
            Some(v) => v.clone(),
            None => env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Returns the configured splash asset path (`logo_splashscreen`)
    /// prefixed with `assets/`, falling back to the hardcoded default
    /// when the manifest field is empty or whitespace-only. The
    /// fallback path is the same one PR-A uses in
    /// [`LaunchUiManifest::default`].
    pub fn splash_image_path(&self) -> String {
        let trimmed = self.logo_splashscreen.trim();
        if trimmed.is_empty() {
            return "assets/logo/logo_splashscreen.png".to_string();
        }
        if trimmed.starts_with("assets/") {
            trimmed.to_string()
        } else {
            format!("assets/{}", trimmed)
        }
    }

    /// Returns the configured clean (no-backdrop) logo path
    /// (`logo_clean`) prefixed with `assets/`, falling back to the
    /// hardcoded default when the manifest field is empty. PR-B uses
    /// this when the splashscreen PNG fails to load (e.g. corrupt
    /// alpha channel on certain drivers).
    pub fn clean_image_path(&self) -> String {
        let trimmed = self.logo_clean.trim();
        if trimmed.is_empty() {
            return "assets/logo/logo_large.png".to_string();
        }
        if trimmed.starts_with("assets/") {
            trimmed.to_string()
        } else {
            format!("assets/{}", trimmed)
        }
    }

    /// Minimum on-screen duration (s). First-input dismissal is
    /// ignored before this elapses, so the splash is always visible
    /// long enough for the player to register the brand.
    pub fn splash_min_seconds(&self) -> f32 {
        self.splash_min_duration_s.max(0.0)
    }

    /// Hard cap (s). If the player hasn't dismissed by this point the
    /// splash force-transitions to `MainMenu`. Mirrors the LGD copy
    /// in `assets/data/launch_ui.ron` (`splash_max_duration_s = 3.0`).
    pub fn splash_max_seconds(&self) -> f32 {
        self.splash_max_duration_s.max(self.splash_min_duration_s)
    }

    /// GRA-309 §3.2 loader rule 2: `splash_min_duration_s <= splash_max_duration_s`.
    /// Both must be > 0 for the splash to make sense. Returns a list of
    /// human-readable violations; the loader still inserts the resource
    /// even when violations are present, mirroring the porkchop loader
    /// pattern at `src/fleets/data.rs`.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if self.splash_min_duration_s <= 0.0 {
            violations.push(format!(
                "splash_min_duration_s must be > 0 (got {})",
                self.splash_min_duration_s
            ));
        }
        if self.splash_max_duration_s <= 0.0 {
            violations.push(format!(
                "splash_max_duration_s must be > 0 (got {})",
                self.splash_max_duration_s
            ));
        }
        if self.splash_min_duration_s > self.splash_max_duration_s {
            violations.push(format!(
                "splash_min_duration_s ({}) > splash_max_duration_s ({})",
                self.splash_min_duration_s, self.splash_max_duration_s
            ));
        }
        if self.logo_splashscreen.is_empty() {
            violations.push("logo_splashscreen must not be empty".to_string());
        }
        if self.logo_clean.is_empty() {
            violations.push("logo_clean must not be empty".to_string());
        }
        if self.default_preset_id.is_empty() {
            violations.push("default_preset_id must not be empty".to_string());
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

/// Loader system: reads `assets/data/launch_ui.ron` at Startup and
/// inserts a [`LaunchUiManifest`] resource. On missing file or parse
/// failure, falls back to [`LaunchUiManifest::default`] and logs at
/// `warn!`. The strict pass/fail gate is the `tests` module at the
/// bottom of this file — production code never panics on a bad RON.
pub fn load_launch_ui_manifest(mut commands: Commands) {
    let path = "assets/data/launch_ui.ron";
    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<LaunchUiManifest>(&contents) {
            Ok(manifest) => {
                if let Err(violations) = manifest.validate() {
                    for v in &violations {
                        warn!("launch_ui.ron validation: {}", v);
                    }
                    warn!(
                        "launch_ui.ron: {} validation violation(s); loader is using the file anyway",
                        violations.len()
                    );
                } else {
                    info!(
                        "launch_ui.ron: loaded (splash {}-{} s, default preset {:?})",
                        manifest.splash_min_duration_s,
                        manifest.splash_max_duration_s,
                        manifest.default_preset_id
                    );
                }
                commands.insert_resource(manifest);
            }
            Err(e) => {
                error!("Failed to parse launch_ui.ron: {}", e);
                commands.insert_resource(LaunchUiManifest::default());
            }
        },
        Err(e) => {
            warn!(
                "launch_ui.ron not found at {}: {}. Using defaults.",
                path, e
            );
            commands.insert_resource(LaunchUiManifest::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manifest_is_sane() {
        let m = LaunchUiManifest::default();
        assert!(m.validate().is_ok(), "default manifest must validate");
        assert!(m.splash_min_duration_s > 0.0);
        assert!(m.splash_max_duration_s >= m.splash_min_duration_s);
        assert!(!m.logo_splashscreen.is_empty());
        assert!(!m.logo_clean.is_empty());
        assert!(!m.default_preset_id.is_empty());
    }

    #[test]
    fn resolved_version_falls_back_to_cargo_pkg() {
        let m = LaunchUiManifest::default();
        assert_eq!(m.resolved_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn resolved_version_uses_explicit_override() {
        let m = LaunchUiManifest {
            version: Some("0.4.0-rc1".to_string()),
            ..Default::default()
        };
        assert_eq!(m.resolved_version(), "0.4.0-rc1");
    }

    #[test]
    fn validate_rejects_min_greater_than_max() {
        let m = LaunchUiManifest {
            splash_min_duration_s: 5.0,
            splash_max_duration_s: 2.0,
            ..Default::default()
        };
        let v = m.validate().unwrap_err();
        assert!(
            v.iter().any(|s| s.contains("splash_min_duration_s")),
            "expected a min-vs-max violation, got {:?}",
            v
        );
    }

    #[test]
    fn validate_rejects_zero_durations() {
        let m = LaunchUiManifest {
            splash_min_duration_s: 0.0,
            splash_max_duration_s: 0.0,
            ..Default::default()
        };
        let v = m.validate().unwrap_err();
        assert!(v.iter().any(|s| s.contains("> 0")));
    }

    #[test]
    fn splash_image_path_uses_configured_value() {
        let m = LaunchUiManifest {
            logo_splashscreen: "logo/splashscreen.png".to_string(),
            ..Default::default()
        };
        assert_eq!(m.splash_image_path(), "assets/logo/splashscreen.png");
    }

    #[test]
    fn splash_image_path_passes_through_already_prefixed() {
        let m = LaunchUiManifest {
            logo_splashscreen: "assets/custom/logo.png".to_string(),
            ..Default::default()
        };
        assert_eq!(m.splash_image_path(), "assets/custom/logo.png");
    }

    #[test]
    fn splash_image_path_falls_back_when_empty() {
        let m = LaunchUiManifest {
            logo_splashscreen: "".to_string(),
            ..Default::default()
        };
        assert_eq!(m.splash_image_path(), "assets/logo/logo_splashscreen.png");
    }

    #[test]
    fn clean_image_path_falls_back_when_empty() {
        let m = LaunchUiManifest {
            logo_clean: "  ".to_string(),
            ..Default::default()
        };
        assert_eq!(m.clean_image_path(), "assets/logo/logo_large.png");
    }

    #[test]
    fn splash_seconds_clamp_negative_inputs() {
        let m = LaunchUiManifest {
            splash_min_duration_s: -1.0,
            splash_max_duration_s: 5.0,
            ..Default::default()
        };
        assert_eq!(m.splash_min_seconds(), 0.0);
        // max gets re-pinned above the clamped min, so still 5.0
        assert_eq!(m.splash_max_seconds(), 5.0);
    }

    #[test]
    fn splash_max_seconds_pinned_at_least_min() {
        // Inverted values should still leave max >= min so the
        // auto-dismiss loop in PR-B can't underflow.
        let m = LaunchUiManifest {
            splash_min_duration_s: 5.0,
            splash_max_duration_s: 1.0,
            ..Default::default()
        };
        assert!(m.splash_max_seconds() >= m.splash_min_seconds());
    }

    #[test]
    fn menu_copy_defaults_resolve_to_english_strings() {
        let m = LaunchMenuCopy::default();
        assert_eq!(m.resolved_continue_label(), "Continue");
        assert_eq!(m.resolved_continue_shortcut(), "1");
        assert_eq!(m.resolved_new_game_label(), "New Game");
        assert_eq!(m.resolved_new_game_shortcut(), "2");
        assert_eq!(m.resolved_load_game_label(), "Load Game");
        assert_eq!(m.resolved_load_game_shortcut(), "3");
        assert_eq!(m.resolved_settings_label(), "Settings");
        assert_eq!(m.resolved_settings_shortcut(), "4");
        assert_eq!(m.resolved_quit_label(), "Quit");
        assert_eq!(m.resolved_quit_shortcut(), "Esc");
    }

    #[test]
    fn menu_copy_uses_ron_value_when_non_empty() {
        let m = LaunchMenuCopy {
            continue_label: "Reprendre".to_string(),
            continue_shortcut: "C".to_string(),
            quit_label: "Quitter".to_string(),
            ..Default::default()
        };
        assert_eq!(m.resolved_continue_label(), "Reprendre");
        assert_eq!(m.resolved_continue_shortcut(), "C");
        assert_eq!(m.resolved_quit_label(), "Quitter");
        // Untouched fields still resolve to the English defaults.
        assert_eq!(m.resolved_new_game_label(), "New Game");
        assert_eq!(m.resolved_settings_shortcut(), "4");
    }

    #[test]
    fn menu_copy_empty_field_falls_back_to_english_default() {
        let m = LaunchMenuCopy {
            continue_label: String::new(),
            continue_shortcut: String::new(),
            quit_label: String::new(),
            ..Default::default()
        };
        assert_eq!(m.resolved_continue_label(), "Continue");
        assert_eq!(m.resolved_continue_shortcut(), "1");
        assert_eq!(m.resolved_quit_label(), "Quit");
    }

    #[test]
    fn default_manifest_includes_menu_copy() {
        let m = LaunchUiManifest::default();
        // Struct-update form requires the menu field to round-trip via Default.
        assert_eq!(m.menu.resolved_continue_label(), "Continue");
    }
}
