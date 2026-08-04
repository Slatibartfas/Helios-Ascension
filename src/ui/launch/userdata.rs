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

fn default_autosave_interval_s() -> f32 {
    300.0
}

fn default_autosave_enabled() -> bool {
    true
}

/// Coder-side mirror of `bevy::window::WindowMode`.
///
/// Bevy 0.18's `WindowMode` is a plain enum (not a `Component`) but
/// two of its variants carry a `MonitorSelection` / `VideoModeSelection`,
/// none of which are `Default`. Storing those variant payloads in
/// `settings.ron` would require persisting monitor ids, which is
/// not portable across machines. We sidestep that by storing only
/// the three intent-level variants here and translating to the
/// Bevy enum at the point of use (see [`From<PersistentWindowMode>
/// for bevy::window::WindowMode`]).
///
/// Order matters: `Windowed` is the default and the first variant so
/// the `#[derive(Default)]` impl matches the player-facing default.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PersistentWindowMode {
    #[default]
    Windowed,
    Fullscreen,
    BorderlessFullscreen,
}

impl PersistentWindowMode {
    /// Stable id string used by the Settings subview's combo box and
    /// in the RON migrations. `as_str` is the inverse of
    /// [`Self::from_str`].
    pub fn as_str(self) -> &'static str {
        match self {
            PersistentWindowMode::Windowed => "windowed",
            PersistentWindowMode::Fullscreen => "fullscreen",
            PersistentWindowMode::BorderlessFullscreen => "borderless",
        }
    }

    /// Parse a stable id back into a [`PersistentWindowMode`]. Unknown
    /// strings fall back to `Windowed` so a typo in the RON file
    /// never leaves the game stuck in a mode the player can't undo.
    pub fn from_str(s: &str) -> Self {
        match s {
            "windowed" => PersistentWindowMode::Windowed,
            "fullscreen" => PersistentWindowMode::Fullscreen,
            "borderless" => PersistentWindowMode::BorderlessFullscreen,
            _ => PersistentWindowMode::Windowed,
        }
    }

    /// All variants in settings-menu order, for combo-box population.
    pub const ALL: &'static [PersistentWindowMode] = &[
        PersistentWindowMode::Windowed,
        PersistentWindowMode::Fullscreen,
        PersistentWindowMode::BorderlessFullscreen,
    ];
}

impl From<PersistentWindowMode> for bevy::window::WindowMode {
    fn from(value: PersistentWindowMode) -> Self {
        use bevy::window::{MonitorSelection, VideoModeSelection, WindowMode};
        match value {
            PersistentWindowMode::Windowed => WindowMode::Windowed,
            // `Current` picks the monitor the window is currently on
            // at the moment the mode flips. Both variants below are
            // the "newest sane choice" — fullscreen picks the
            // current video mode, which lets the OS pick the refresh
            // rate / resolution rather than freezing it from a stale
            // settings file.
            PersistentWindowMode::Fullscreen => {
                WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
            }
            PersistentWindowMode::BorderlessFullscreen => {
                WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            }
        }
    }
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
    /// Windowed / Fullscreen / Borderless. The Settings subview's
    /// graphics tab drives this; the `apply_window_mode_to_primary`
    /// system in `src/plugins/window_mode_bridge.rs` pushes it to
    /// `Window::mode` on the primary window.
    #[serde(default)]
    pub window_mode: PersistentWindowMode,
    /// Legacy `fullscreen: bool` field. Read on load for backward
    /// compatibility with settings.ron files written before
    /// `window_mode` existed; never written on save. The migration
    /// step in [`load_persistent_settings_from`] promotes
    /// `fullscreen: true` to `window_mode: Fullscreen` and clears
    /// the legacy field.
    #[serde(default, skip_serializing)]
    pub fullscreen: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub tutorial_enabled: bool,
    /// GRA-358 PR-B: whether the autosave timer is allowed to fire.
    /// UX surfaces this as a toggle in the Settings subview.
    /// Defaults to `true` so a fresh install starts saving.
    #[serde(default = "default_autosave_enabled")]
    pub autosave_enabled: bool,
    /// GRA-358 PR-B: how often the autosave timer fires, in
    /// wall-clock seconds. The Settings subview exposes this as a
    /// numeric input. UX owns the input validation (must be
    /// `>= 1.0`); the autosave consumer clamps via
    /// [`crate::persistence::autosave::AutosaveTimer::apply_settings`].
    #[serde(default = "default_autosave_interval_s")]
    pub autosave_interval_s: f32,
}

impl Default for PersistentSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            window_mode: PersistentWindowMode::default(),
            fullscreen: false,
            ui_scale: 1.0,
            tutorial_enabled: false,
            autosave_enabled: default_autosave_enabled(),
            autosave_interval_s: default_autosave_interval_s(),
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
///
/// **Legacy migration**: settings files written before the
/// `window_mode` field existed used `fullscreen: bool`. The legacy
/// field is still accepted via `#[serde(default, skip_serializing)]`
/// on `PersistentSettings::fullscreen`, so a file with `fullscreen:
/// true` deserializes successfully. After deserialization this
/// function promotes the legacy bool into the new enum (`true` →
/// `Fullscreen`) and clears the legacy field so a subsequent save
/// drops it. A file with `fullscreen: false` and `window_mode:
/// Windowed` (the default) is unchanged.
pub fn load_persistent_settings_from(dir: &Path) -> PersistentSettings {
    let path = settings_path_in(dir);
    match fs::read_to_string(&path) {
        Ok(contents) => match ron::from_str::<PersistentSettings>(&contents) {
            Ok(mut s) => {
                if s.fullscreen && s.window_mode == PersistentWindowMode::Windowed {
                    info!(
                        "settings: migrating legacy `fullscreen: true` → `window_mode: Fullscreen`"
                    );
                    s.window_mode = PersistentWindowMode::Fullscreen;
                    s.fullscreen = false;
                }
                s
            }
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
    use crate::test_util::USERDATA_ENV_LOCK;
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
            window_mode: PersistentWindowMode::BorderlessFullscreen,
            // The legacy `fullscreen: true` is the "before migration"
            // helper value — keep it true here so the
            // `legacy_fullscreen_true_migrates_to_window_mode_fullscreen`
            // test below doesn't double-load underneath the round-trip.
            // The round-trip must keep `window_mode` and the legacy
            // bool in sync.
            fullscreen: false,
            ui_scale: 1.25,
            tutorial_enabled: true,
            autosave_enabled: false,
            autosave_interval_s: 120.0,
        };

        let written_path = save_persistent_settings_to(&dir, &original)
            .expect("save must succeed in a writable temp dir");
        assert!(written_path.exists(), "settings file must exist on disk");

        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(loaded, original, "loaded settings must equal what we wrote");

        // The legacy `fullscreen: bool` field must NOT be written
        // back to disk by `save_persistent_settings_to` because it
        // is marked `#[serde(skip_serializing)]`. Round-tripping and
        // re-reading the raw file proves the migration won't keep
        // re-applying on every save.
        let raw = fs::read_to_string(&written_path).expect("read back");
        assert!(
            !raw.contains("fullscreen"),
            "legacy `fullscreen` field must not be re-serialized; got:\n{}",
            raw
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_fullscreen_true_migrates_to_window_mode_fullscreen() {
        // A settings.ron file written by the pre-window_mode build
        // has `fullscreen: true` and no `window_mode` key. The
        // loader must promote it to `window_mode: Fullscreen` and
        // clear the legacy field so the next save drops the legacy
        // key.
        let dir = fresh_temp_dir("legacy-migrate");
        let path = settings_path_in(&dir);
        let legacy = r#"(
            master_volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            fullscreen: true,
            ui_scale: 1.0,
            tutorial_enabled: false,
            autosave_enabled: true,
            autosave_interval_s: 300.0,
        )"#;
        fs::write(&path, legacy).expect("write legacy file");

        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(
            loaded.window_mode,
            PersistentWindowMode::Fullscreen,
            "legacy `fullscreen: true` must migrate to `window_mode: Fullscreen`"
        );
        assert!(
            !loaded.fullscreen,
            "legacy `fullscreen` field must be cleared after migration"
        );

        // Re-save through the canonical path and re-read the raw
        // file. The new save must NOT contain the legacy key.
        let _ = save_persistent_settings_to(&dir, &loaded).expect("resave");
        let raw = fs::read_to_string(&path).expect("re-read");
        assert!(
            !raw.contains("fullscreen"),
            "post-migration resave must drop the legacy `fullscreen` key; got:\n{}",
            raw
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_fullscreen_false_does_not_override_window_mode() {
        // A legacy file with `fullscreen: false` must not accidentally
        // demote a freshly-set `window_mode: BorderlessFullscreen`
        // back to `Windowed`. The migration only fires when the
        // `window_mode` is at its default AND `fullscreen` is true.
        let dir = fresh_temp_dir("legacy-no-migrate");
        let path = settings_path_in(&dir);
        let mixed = r#"(
            master_volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            window_mode: BorderlessFullscreen,
            fullscreen: false,
            ui_scale: 1.0,
            tutorial_enabled: false,
            autosave_enabled: true,
            autosave_interval_s: 300.0,
        )"#;
        fs::write(&path, mixed).expect("write mixed file");

        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(
            loaded.window_mode,
            PersistentWindowMode::BorderlessFullscreen,
            "mixing `fullscreen: false` with an explicit `window_mode` must not override the new field"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn persistent_window_mode_into_bevy_window_mode() {
        // The `From` impl is the bridge to `Window::mode`. Each
        // variant must map to a valid `bevy::window::WindowMode`.
        use bevy::window::{MonitorSelection, VideoModeSelection, WindowMode};
        let from = |m: PersistentWindowMode| -> WindowMode { m.into() };
        assert_eq!(from(PersistentWindowMode::Windowed), WindowMode::Windowed);
        assert!(matches!(
            from(PersistentWindowMode::Fullscreen),
            WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
        ));
        assert!(matches!(
            from(PersistentWindowMode::BorderlessFullscreen),
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        ));
    }

    #[test]
    fn persistent_window_mode_as_str_round_trips() {
        for variant in PersistentWindowMode::ALL {
            assert_eq!(
                PersistentWindowMode::from_str(variant.as_str()),
                *variant,
                "as_str / from_str must round-trip for {:?}",
                variant
            );
        }
        // Unknown strings fall back to Windowed.
        assert_eq!(
            PersistentWindowMode::from_str("nope"),
            PersistentWindowMode::Windowed
        );
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
        // RAII guard pattern (mirrors `subview_save_game::UserdataDirGuard`):
        // without restoring the prior `HELIOS_USERDATA_DIR` value on drop,
        // parallel test execution leaks the override across tests and
        // `save_panel_save_writes_file_and_rescans_index` intermittently
        // rescans the wrong dir.
        struct OverrideGuard {
            prior: Option<std::ffi::OsString>,
            /// Held across the test body so a parallel test in another
            /// module cannot overwrite the env var mid-assertion.
            _env_lock: std::sync::MutexGuard<'static, ()>,
        }
        impl Drop for OverrideGuard {
            fn drop(&mut self) {
                // SAFETY: env-var access is single-threaded by Rust's
                // documented `set_var` contract; we always restore a
                // value we previously observed.
                unsafe {
                    match &self.prior {
                        Some(v) => std::env::set_var("HELIOS_USERDATA_DIR", v),
                        None => std::env::remove_var("HELIOS_USERDATA_DIR"),
                    }
                }
            }
        }
        let _env_lock = USERDATA_ENV_LOCK
            .lock()
            .expect("USERDATA_ENV_LOCK poisoned");
        let prior = std::env::var_os("HELIOS_USERDATA_DIR");
        let override_dir =
            std::env::temp_dir().join(format!("helios-userdata-override-{}", std::process::id()));
        // SAFETY: see `OverrideGuard::drop` — we restore `prior` on
        // drop, so no env var leakage across tests.
        unsafe {
            std::env::set_var("HELIOS_USERDATA_DIR", &override_dir);
        }
        let _guard = OverrideGuard { prior, _env_lock };
        let resolved = resolve_userdata_dir();
        assert_eq!(resolved, override_dir);
    }
}
