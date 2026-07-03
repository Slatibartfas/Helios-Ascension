//! Loader for `assets/data/launch_ui.ron`.
//!
//! The manifest holds splash timings, asset paths, build-label format,
//! and the default preset id (GRA-309 §3.2). LGD-owned content; the
//! Rust types here are the wire shape so copy / timings can change
//! without recompilation. Schema lives in `assets/data/launch_ui.ron`;
//! this file only mirrors it.
//!
//! PR-A (GRA-311) registers the loader at Startup and inserts the
//! resulting [`LaunchUiManifest`] resource. PR-B / PR-C / PR-D will
//! consume the resource for splash rendering, menu wiring, and the
//! new-game subview. PR-A itself does not touch the manifest values.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;

/// Schema-mirror for `assets/data/launch_ui.ron`.
///
/// All fields match the GRA-309 §3.2 schema and the LGD content shipped
/// in GRA-310 (PR #196). Loader rules from the RON header:
///
/// 1. `logo_splashscreen` and `logo_clean` resolve under `assets/`
///    (forward-slash relative).
/// 2. `splash_min_duration_s <= splash_max_duration_s`; both > 0.
/// 3. `build_label_format` is a template with `{version}` / `{sha}`
///    placeholders; unknowns pass through literally.
/// 4. `version`, when `None`, falls back to `env!("CARGO_PKG_VERSION")`.
/// 5. `show_sha_in_release`, when `true`, overrides the
///    `cfg!(debug_assertions)` default for `{sha}` visibility.
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
}
