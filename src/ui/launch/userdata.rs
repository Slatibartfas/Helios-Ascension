//! On-disk persistence for player-facing settings.
//!
//! First real persistence layer for the game (GRA-309 §1.4). All writes
//! are best-effort: missing or read-only directories log at `warn!`
//! and the game falls back to in-memory defaults — a bad disk is not
//! a crashable condition.
//!
//! PR-A (GRA-311) defines the [`PersistentSettings`] struct + RON
//! round-trip and the [`resolve_userdata_dir`] helper. PR-E (GRA-314)
//! wires the debounced writer + the settings subview; PR-A does not
//! add either.
//!
//! Design §7 calls for `dirs = "5"` to find the platform config dir,
//! with a zero-dep fallback. PR-A ships the zero-dep fallback only
//! (no new `dirs` dep) — the fallback covers Linux/macOS/Windows
//! targets without touching `Cargo.toml`. If a future PR needs
//! `dirs::data_dir()` semantics, swap the helper.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Game folder under the user's config dir, e.g. `~/.config/HeliosAscension`.
const USERDATA_DIR_NAME: &str = "HeliosAscension";

/// Default file name for persisted settings, relative to the userdata dir.
pub const SETTINGS_FILE_NAME: &str = "settings.ron";

fn default_volume() -> f32 {
    1.0
}

fn default_ui_scale() -> f32 {
    1.0
}

/// Player-facing settings persisted to `<userdata>/settings.ron`.
///
/// Reads happen at boot after the resource is inserted as
/// `PersistentSettings::default()`, so a missing file is never an
/// error. Writes are debounced (PR-E); PR-A exposes the loader +
/// saver but does not schedule the writer.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistentSettings {
    #[serde(default = "default_volume")]
    pub master_volume: f32,
    #[serde(default = "default_volume")]
    pub music_volume: f32,
    #[serde(default = "default_volume")]
    pub sfx_volume: f32,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub tutorial_enabled: bool,
}

impl Default for PersistentSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            fullscreen: false,
            ui_scale: 1.0,
            tutorial_enabled: false,
        }
    }
}

/// Resolve the platform-specific userdata directory.
///
/// Zero-dep fallback (no `dirs` crate) that hits the right path on
/// Linux (`$XDG_CONFIG_HOME` or `$HOME/.config`), macOS
/// (`$HOME/Library/Application Support`), and Windows
/// (`%APPDATA%`). Returns `<dir>/HeliosAscension`.
///
/// The directory is **not** created on call; callers (loaders/savers)
/// handle `mkdir -p` semantics as needed.
pub fn resolve_userdata_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("HELIOS_USERDATA_DIR") {
        let p = PathBuf::from(path);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    platform_userdata_dir().join(USERDATA_DIR_NAME)
}

#[cfg(target_os = "linux")]
fn platform_userdata_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config");
    }
    PathBuf::from(".")
}

#[cfg(target_os = "macos")]
fn platform_userdata_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support");
    }
    PathBuf::from(".")
}

#[cfg(target_os = "windows")]
fn platform_userdata_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let p = PathBuf::from(appdata);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(home).join("AppData/Roaming");
    }
    PathBuf::from(".")
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_userdata_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config");
    }
    PathBuf::from(".")
}

/// Path to the persistent settings file inside `dir`.
pub fn settings_path_in(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE_NAME)
}

/// Load `PersistentSettings` from `<dir>/<SETTINGS_FILE_NAME>`.
///
/// On missing file or parse failure, returns `PersistentSettings::default()`.
/// The function does not create the directory; the caller is responsible
/// for `fs::create_dir_all` if creation is desired.
pub fn load_persistent_settings_from(dir: &Path) -> PersistentSettings {
    let path = settings_path_in(dir);
    match fs::read_to_string(&path) {
        Ok(contents) => match ron::from_str::<PersistentSettings>(&contents) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "Failed to parse persistent settings at {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                PersistentSettings::default()
            }
        },
        Err(_) => PersistentSettings::default(),
    }
}

/// Serialize `settings` to RON and write to `<dir>/<SETTINGS_FILE_NAME>`.
///
/// Creates `dir` if missing. Returns `Ok(path)` on success or
/// `Err(message)` describing the failure. Failures are not fatal —
/// the caller is expected to log and continue.
pub fn save_persistent_settings_to(
    dir: &Path,
    settings: &PersistentSettings,
) -> Result<PathBuf, String> {
    if let Err(e) = fs::create_dir_all(dir) {
        return Err(format!(
            "could not create userdata dir {}: {}",
            dir.display(),
            e
        ));
    }
    let path = settings_path_in(dir);
    let ron = ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("could not serialize persistent settings: {}", e))?;
    fs::write(&path, ron).map_err(|e| {
        format!(
            "could not write persistent settings to {}: {}",
            path.display(),
            e
        )
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Build a unique temp directory for a test. Returns the path;
    /// the caller is responsible for `fs::remove_dir_all` cleanup.
    fn fresh_temp_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("helios-launch-{}-{}-{}", tag, pid, n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn persistent_settings_round_trip() {
        let dir = fresh_temp_dir("rt");
        let original = PersistentSettings {
            master_volume: 0.42,
            music_volume: 0.0,
            sfx_volume: 0.95,
            fullscreen: true,
            ui_scale: 1.25,
            tutorial_enabled: true,
        };

        let written_path = save_persistent_settings_to(&dir, &original)
            .expect("save must succeed in a writable temp dir");
        assert!(written_path.exists(), "settings file must exist on disk");

        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(loaded, original, "loaded settings must equal what we wrote");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let dir = fresh_temp_dir("missing");
        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(loaded, PersistentSettings::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_corrupt_file_returns_defaults() {
        let dir = fresh_temp_dir("corrupt");
        let path = settings_path_in(&dir);
        fs::write(&path, "this is not valid ron {").expect("write corrupt file");
        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(loaded, PersistentSettings::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_creates_missing_directory() {
        let dir = fresh_temp_dir("create");
        let nested = dir.join("nested").join("deeper");
        let s = PersistentSettings {
            master_volume: 0.5,
            ..Default::default()
        };
        let result = save_persistent_settings_to(&nested, &s);
        assert!(result.is_ok(), "save must create nested dirs");
        assert!(settings_path_in(&nested).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_path_in_uses_settings_file_name() {
        let dir = PathBuf::from("/tmp/example");
        assert_eq!(settings_path_in(&dir), dir.join("settings.ron"));
    }

    #[test]
    fn resolve_userdata_dir_respects_override() {
        // SAFETY: tests run single-threaded by default for env vars;
        // an override set here cannot race with other tests because
        // each test sets and restores its own value.
        let prior = std::env::var_os("HELIOS_USERDATA_DIR");
        // SAFETY: see above.
        unsafe {
            std::env::set_var("HELIOS_USERDATA_DIR", "/tmp/helios-override");
        }
        let resolved = resolve_userdata_dir();
        assert_eq!(resolved, PathBuf::from("/tmp/helios-override"));
        match prior {
            Some(v) => unsafe { std::env::set_var("HELIOS_USERDATA_DIR", v) },
            None => unsafe { std::env::remove_var("HELIOS_USERDATA_DIR") },
        }
    }
}
