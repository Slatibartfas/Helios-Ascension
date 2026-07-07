//! Autosave timer + atomic write (GRA-358 PR-B).
//!
//! PR-A produced save RON strings but never wrote them to disk. PR-B
//! adds [`AutosaveTimer`] — a `Resource` advanced every frame by
//! [`tick_autosave_timer`] that, when it fires, snapshots the world
//! via [`snapshot_world`], writes the result atomically via
//! [`write_save_atomic`], prunes old autosave files so the disk does
//! not grow without bound, and re-scans the [`SaveIndex`] so the
//! main menu shows the new save without restarting.
//!
//! # Gating
//!
//! The timer only fires when:
//!
//! 1. `PersistentSettings::autosave_enabled` is `true` (so the
//!    player can disable autosave from the Settings subview).
//! 2. `LaunchState::is_in_game()` is `true` (so autosave never runs
//!    while the player is on the splash or main menu).
//! 3. `TimeScale::scale > 0.0` (so a paused game does not write
//!    saves).
//!
//! When any gate is closed, the timer still advances (so opening the
//! menu and walking back into the game resumes the same cadence),
//! but `next_due_s` is rolled forward instead of firing.
//!
//! # Rolling history
//!
//! [`AutosaveTimer::rolling_count`] controls how many autosave files
//! survive on disk. After each successful save, the autosave
//! directory is scanned; any file matching `autosave_*.ron` is
//! sorted by mtime ascending and the oldest `(count - rolling_count)`
//! are removed. The most recent file is always preserved —
//! `rolling_count` is a maximum, not a target.
//!
//! # File naming
//!
//! `autosave_<UTC>.ron` — UTC seconds since the Unix epoch. The
//! naming does not embed the slot number because rolling-count
//! pruning is mtime-driven and the player only sees "the most recent
//! N autosaves" in the menu.

use bevy::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::io::write_save_atomic;
use super::snapshot::{snapshot_world, SaveMetadata};
use crate::game_state::GameSeed;
use crate::persistence::playtime::PlaytimeTracker;
use crate::ui::launch::save_index::{SaveIndex, SAVES_SUBDIR};
use crate::ui::launch::userdata::{resolve_userdata_dir, PersistentSettings};
use crate::ui::launch::LaunchState;
use crate::ui::time::TimeScale;

/// Default interval between autosaves (5 minutes).
pub const DEFAULT_AUTOSAVE_INTERVAL_S: f64 = 300.0;

/// Default number of rolling autosave files retained. Three is the
/// smallest count that lets a player roll back two saves after a bad
/// patch.
pub const DEFAULT_ROLLING_COUNT: u32 = 3;

/// File-name prefix for autosaves. Used by the pruner to distinguish
/// autosaves from player-initiated manual saves (which use the slot
/// picker from the Save Panel, landing later in PR-C).
pub const AUTOSAVE_PREFIX: &str = "autosave_";

/// File-name suffix for autosaves (RON extension matches the
/// manual-save format so the menu scanner can pick them up
/// uniformly).
pub const AUTOSAVE_SUFFIX: &str = ".ron";

/// Autosave cadence + bookkeeping resource.
///
/// `next_due_s` is the wall-clock deadline (seconds since the Bevy
/// Time system started) at which the next save should fire.
/// `interval_s` is the wall-clock gap between saves; the autosave
/// consumer is the only writer.
///
/// Fields are `pub` so tests can poke them and the Settings subview
/// can update `interval_s` on the fly (PR-B does not surface the
/// editable interval in the UI yet — UX owns the copy in
/// `subview_settings`).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct AutosaveTimer {
    /// Wall-clock deadline (seconds since first update) of the next
    /// autosave. Initially set to `interval_s` so the first save
    /// fires one full interval after boot.
    pub next_due_s: f64,
    /// Wall-clock interval between autosaves.
    pub interval_s: f64,
    /// Maximum number of autosave files retained on disk. Must be
    /// >= 1.
    pub rolling_count: u32,
}

impl Default for AutosaveTimer {
    fn default() -> Self {
        Self {
            next_due_s: DEFAULT_AUTOSAVE_INTERVAL_S,
            interval_s: DEFAULT_AUTOSAVE_INTERVAL_S,
            rolling_count: DEFAULT_ROLLING_COUNT,
        }
    }
}

impl AutosaveTimer {
    /// Override `interval_s` from
    /// [`PersistentSettings::autosave_interval_s`]. The persistent
    /// settings value is `f32` (UI-friendly); the timer stores `f64`
    /// so multi-day sessions do not lose precision. We clamp to
    /// `1.0` so a misconfigured UI value cannot trigger a save-flood.
    pub fn apply_settings(&mut self, settings: &PersistentSettings) {
        self.interval_s = (settings.autosave_interval_s as f64).max(1.0);
    }
}

/// Advance the [`AutosaveTimer`] and fire a save when due.
///
/// Runs in [`Update`], ordered after
/// [`super::playtime::tick_playtime_tracker`] (the SaveLoadPlugin
/// wires the ordering explicitly) so the snapshot's `playtime_s`
/// reflects this frame's playtime contribution.
///
/// On fire:
/// 1. Resolve `<userdata>/saves/` (create if missing).
/// 2. Build a [`SaveMetadata`] from [`PlaytimeTracker`] +
///    [`GameSeed`] + `CARGO_PKG_VERSION`.
/// 3. Snapshot the world via [`snapshot_world`]. Reflection gaps
///    return [`SnapshotError`](super::snapshot::SnapshotError) —
///    the autosave logs at `warn!` and pushes the deadline forward
///    by one interval so we do not spam the log every frame.
/// 4. Compose `autosave_<UTC>.ron` and write atomically via
///    [`write_save_atomic`].
/// 5. Prune older autosaves to `rolling_count`.
/// 6. Re-scan [`SaveIndex`] so the menu sees the new file without a
///    restart.
///
/// The system signature is `&mut World` (exclusive system access)
/// because the snapshot helper needs `&World` while the
/// `SaveIndex` replacement needs `&mut World` — Bevy 0.18 forbids
/// holding both `Res<World>` and `ResMut<SaveIndex>` at once, so
/// the exclusive pattern is the cleanest fit. The codebase already
/// uses this pattern for
/// [`crate::ui::launch::load_save_index_system`].
pub fn tick_autosave_timer(world: &mut World) {
    if !world.resource::<LaunchState>().is_in_game() {
        return;
    }
    if !world.resource::<PersistentSettings>().autosave_enabled {
        advance_only(world);
        return;
    }
    if world.resource::<TimeScale>().scale <= 0.0 {
        advance_only(world);
        return;
    }

    let elapsed = world.resource::<Time<Real>>().elapsed_secs_f64();

    let should_fire = {
        let timer = world.resource::<AutosaveTimer>();
        elapsed >= timer.next_due_s
    };

    if !should_fire {
        return;
    }

    if let Err(e) = fire_autosave(world) {
        warn!("autosave failed: {e}");
    }

    // Roll the deadline forward — even on failure, so we don't
    // hammer the disk on a reflection-coverage gap.
    let mut timer = world.resource_mut::<AutosaveTimer>();
    timer.next_due_s = elapsed + timer.interval_s;
}

/// Roll the deadline forward without firing. Used when a gate
/// (autosave disabled, paused) prevents a save.
fn advance_only(world: &mut World) {
    let elapsed = world.resource::<Time<Real>>().elapsed_secs_f64();
    let mut timer = world.resource_mut::<AutosaveTimer>();
    if timer.next_due_s < elapsed {
        timer.next_due_s = elapsed + timer.interval_s;
    }
}

/// Snapshot the world and write to disk. Returns an error string
/// for the warn-log path.
fn fire_autosave(world: &mut World) -> Result<(), String> {
    let playtime = world.resource::<PlaytimeTracker>().total_real_s as u64;
    let seed = world.resource::<GameSeed>().value;
    let metadata = SaveMetadata::new_now(seed, playtime, env!("CARGO_PKG_VERSION"));

    let saves_dir = resolve_userdata_dir().join(SAVES_SUBDIR);
    if let Err(e) = fs::create_dir_all(&saves_dir) {
        return Err(format!("mkdir {}: {e}", saves_dir.display()));
    }

    let ron = {
        let world_ref: &World = &*world;
        snapshot_world(world_ref, metadata).map_err(|e| format!("snapshot: {e}"))?
    };
    let path = compose_autosave_path(&saves_dir);
    write_save_atomic(&path, &ron).map_err(|e| format!("write {}: {e}", path.display()))?;

    let rolling_count = world.resource::<AutosaveTimer>().rolling_count;
    prune_old_autosaves(&saves_dir, rolling_count)
        .map_err(|e| format!("prune {}: {e}", saves_dir.display()))?;

    let index = SaveIndex::scan(&saves_dir);
    world.insert_resource(index);

    Ok(())
}

/// Compose the autosave file path for "now" (UTC seconds).
fn compose_autosave_path(saves_dir: &Path) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    saves_dir.join(format!("{AUTOSAVE_PREFIX}{now}{AUTOSAVE_SUFFIX}"))
}

/// Prune the oldest autosaves so at most `rolling_count` survive.
///
/// The function never removes the most recently written file —
/// `rolling_count` is a maximum, not a target. A pre-seeded
/// directory with fewer than `rolling_count` autosaves is left
/// untouched.
pub fn prune_old_autosaves(saves_dir: &Path, rolling_count: u32) -> std::io::Result<()> {
    if rolling_count == 0 {
        return Ok(());
    }
    if !saves_dir.exists() {
        return Ok(());
    }

    let mut autosaves: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for entry in fs::read_dir(saves_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_autosave_path(&path) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        autosaves.push((path, modified));
    }

    // Sort ascending by mtime so the oldest is at index 0.
    autosaves.sort_by_key(|(_, t)| *t);

    let to_remove = autosaves.len().saturating_sub(rolling_count as usize);
    for (path, _) in autosaves.iter().take(to_remove) {
        let _ = fs::remove_file(path);
    }

    Ok(())
}

/// True if `path` looks like an autosave filename.
fn is_autosave_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with(AUTOSAVE_PREFIX) && name.ends_with(AUTOSAVE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("helios-autosave-{tag}-{pid}-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Build an App that has the autosave consumer registered. The
    /// `Time<Real>` strategy is `ManualDuration` so the test can pin
    /// the deadline to a known offset.
    fn fresh_app_with_dir(interval_s: f64, rolling_count: u32) -> (App, PathBuf) {
        let dir = fresh_dir("app");
        // SAFETY: tests run single-threaded for env-var mutations.
        unsafe {
            std::env::set_var("HELIOS_USERDATA_DIR", &dir);
        }
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            interval_s,
        )));
        app.init_resource::<PlaytimeTracker>();
        app.init_resource::<TimeScale>();
        app.init_resource::<LaunchState>();
        app.init_resource::<PersistentSettings>();
        app.init_resource::<GameSeed>();
        app.init_resource::<AppTypeRegistry>();
        // No marker entity: the snapshot is intentionally empty in
        // tests. We exercise the autosave cadence + atomic write,
        // not the reflection coverage of `DynamicScene`. The RON
        // envelope is produced correctly either way.

        let timer = AutosaveTimer {
            interval_s,
            rolling_count,
            // Pin the first deadline so `app.update()` always fires.
            next_due_s: 0.0,
        };
        app.insert_resource(timer);

        app.add_systems(Update, tick_autosave_timer);
        (app, dir)
    }

    #[test]
    fn autosave_timer_default_values() {
        let t = AutosaveTimer::default();
        assert_eq!(t.interval_s, DEFAULT_AUTOSAVE_INTERVAL_S);
        assert_eq!(t.rolling_count, DEFAULT_ROLLING_COUNT);
        assert!(
            (t.next_due_s - DEFAULT_AUTOSAVE_INTERVAL_S).abs() < 1e-9,
            "first deadline sits one interval out from boot"
        );
    }

    #[test]
    fn apply_settings_clamps_to_minimum_interval() {
        let mut t = AutosaveTimer::default();
        let s = PersistentSettings {
            autosave_interval_s: -10.0,
            ..PersistentSettings::default()
        };
        t.apply_settings(&s);
        assert!(
            (t.interval_s - 1.0).abs() < 1e-9,
            "negative settings intervals clamp to 1.0"
        );

        let s = PersistentSettings {
            autosave_interval_s: 600.0,
            ..PersistentSettings::default()
        };
        t.apply_settings(&s);
        assert!(
            (t.interval_s - 600.0).abs() < 1e-9,
            "positive settings intervals are honoured verbatim"
        );
    }

    #[test]
    fn autosave_writes_file_and_advances_timer() {
        // Frame delta = 0.1 s; autosave interval = 0.1 s. Single
        // update advances Time<Real> to 0.1 s, the timer (next_due_s
        // = 0) fires, and rolls forward to 0.1 + 0.1 = 0.2 s. No
        // further updates are run so the test stops at exactly one
        // autosave file.
        let (mut app, dir) = fresh_app_with_dir(0.1, 3);
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::InGame;
        app.world_mut().resource_mut::<TimeScale>().set_speed(1.0);

        app.update();

        let saves_dir = dir.join(SAVES_SUBDIR);
        assert!(saves_dir.exists(), "saves dir must be created");
        let autosaves: Vec<_> = fs::read_dir(&saves_dir)
            .expect("read_dir")
            .filter_map(|res| res.ok())
            .map(|e| e.path())
            .filter(|p| is_autosave_path(p))
            .collect();
        assert_eq!(autosaves.len(), 1, "exactly one autosave file");

        let timer = app.world().resource::<AutosaveTimer>();
        assert!(
            timer.next_due_s > 0.0,
            "deadline advanced past 0, got {}",
            timer.next_due_s
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_skipped_when_paused() {
        let (mut app, dir) = fresh_app_with_dir(0.1, 3);
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::InGame;
        app.world_mut().resource_mut::<TimeScale>().pause();

        app.update();
        app.update();

        let saves_dir = dir.join(SAVES_SUBDIR);
        let autosaves = if saves_dir.exists() {
            fs::read_dir(&saves_dir)
                .expect("read_dir")
                .filter_map(|res| res.ok())
                .map(|e| e.path())
                .filter(|p| is_autosave_path(p))
                .count()
        } else {
            0
        };
        assert_eq!(autosaves, 0, "paused games must not autosave");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_skipped_on_main_menu() {
        let (mut app, dir) = fresh_app_with_dir(0.1, 3);
        app.world_mut().resource_mut::<TimeScale>().set_speed(1.0);
        // Leave LaunchState at Splash (the resource default).

        app.update();
        app.update();

        let saves_dir = dir.join(SAVES_SUBDIR);
        let autosaves = if saves_dir.exists() {
            fs::read_dir(&saves_dir)
                .expect("read_dir")
                .filter_map(|res| res.ok())
                .map(|e| e.path())
                .filter(|p| is_autosave_path(p))
                .count()
        } else {
            0
        };
        assert_eq!(autosaves, 0, "main-menu state must not autosave");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_skipped_when_disabled_in_settings() {
        let (mut app, dir) = fresh_app_with_dir(0.1, 3);
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::InGame;
        app.world_mut().resource_mut::<TimeScale>().set_speed(1.0);
        app.world_mut()
            .resource_mut::<PersistentSettings>()
            .autosave_enabled = false;

        app.update();
        app.update();

        let saves_dir = dir.join(SAVES_SUBDIR);
        let autosaves = if saves_dir.exists() {
            fs::read_dir(&saves_dir)
                .expect("read_dir")
                .filter_map(|res| res.ok())
                .map(|e| e.path())
                .filter(|p| is_autosave_path(p))
                .count()
        } else {
            0
        };
        assert_eq!(autosaves, 0, "disabled autosave must not write");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_prunes_old_files_keeping_rolling_count() {
        let dir = fresh_dir("prune");
        let saves_dir = dir.join(SAVES_SUBDIR);
        fs::create_dir_all(&saves_dir).expect("mkdir saves");

        // Pre-seed 4 autosaves, oldest first.
        let base = std::time::SystemTime::now();
        for i in 0..4 {
            let path = saves_dir.join(format!("{AUTOSAVE_PREFIX}{:010}{AUTOSAVE_SUFFIX}", i));
            fs::write(&path, format!("seed-{i}")).expect("write seed");
            let mtime = base - Duration::from_secs((4 - i) as u64);
            filetime_set(&path, mtime);
            std::thread::sleep(Duration::from_millis(50));
        }

        // Also seed a manual save that must NOT be pruned.
        let manual_path = saves_dir.join("manual_save.ron");
        fs::write(&manual_path, "manual").expect("write manual");
        let manual_mtime = base + Duration::from_secs(60);
        filetime_set(&manual_path, manual_mtime);

        prune_old_autosaves(&saves_dir, 3).expect("prune");

        let remaining: Vec<PathBuf> = fs::read_dir(&saves_dir)
            .expect("read_dir")
            .filter_map(|res| res.ok())
            .map(|e| e.path())
            .filter(|p| is_autosave_path(p))
            .collect();

        assert_eq!(
            remaining.len(),
            3,
            "rolling_count = 3 must leave exactly 3 autosave files"
        );

        let names: Vec<String> = remaining
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.iter().any(|n| n.contains("0000000001")));
        assert!(names.iter().any(|n| n.contains("0000000002")));
        assert!(names.iter().any(|n| n.contains("0000000003")));
        assert!(
            !names.iter().any(|n| n.contains("0000000000")),
            "oldest must be pruned"
        );

        assert!(manual_path.exists(), "manual save must not be pruned");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_is_noop_with_zero_autosaves() {
        let dir = fresh_dir("empty");
        let saves_dir = dir.join(SAVES_SUBDIR);
        fs::create_dir_all(&saves_dir).expect("mkdir saves");
        prune_old_autosaves(&saves_dir, 3).expect("prune empty");
        let count = fs::read_dir(&saves_dir).unwrap().count();
        assert_eq!(count, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_updates_save_index_after_fire() {
        let (mut app, dir) = fresh_app_with_dir(0.1, 3);
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::InGame;
        app.world_mut().resource_mut::<TimeScale>().set_speed(1.0);

        app.update();

        let index = app.world().resource::<SaveIndex>();
        let autosaves_in_index = index
            .entries
            .iter()
            .filter(|e| is_autosave_path(e.path()))
            .count();
        assert_eq!(
            autosaves_in_index, 1,
            "SaveIndex must re-scan and pick up the new autosave"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Set a file's mtime via `std::fs::File::set_modified`
    /// (stable since Rust 1.75).
    fn filetime_set(path: &Path, mtime: std::time::SystemTime) {
        let f = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for mtime");
        f.set_modified(mtime).expect("set mtime");
    }
}
