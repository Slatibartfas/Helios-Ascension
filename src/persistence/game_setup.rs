//! World-construction plugin — fresh-game and load-save constructors (GRA-358 PR-C).
//!
//! PR-A / PR-B shipped save/load helpers and the load-game subview,
//! but [`crate::ui::launch::subview_kickoff::kickoff_world_system`]
//! only *logged* the decision — the actual world construction was
//! deferred. [`GameSetupPlugin`] turns those log-only branches into
//! real constructors:
//!
//! - [`play_new_game`] — consumes a [`NewGameRequest`], builds a fresh
//!   Bevy [`World`] seeded from the request's params, and emits
//!   [`NewGameCommitted`] so downstream sim plugins can apply the
//!   params when the new world is promoted.
//! - [`restore_save`] — reads the save at `path` via
//!   [`super::restore::restore_world`] into a fresh world and stores
//!   it in [`PendingGameWorld`] for the in-game consumer to swap.
//!   **Failures emit a [`NotificationEvent`]** so the player sees a
//!   toast (per GRA-137 bridge contract) instead of just a log line.
//!
//! # World-swap pattern (R3)
//!
//! Bevy 0.18 manages a single root world via [`App`]. Both play
//! paths build a fresh [`World`] and stash it in
//! [`PendingGameWorld`]; [`promote_pending_world`] acks the swap by
//! advancing [`LaunchState`] and clearing [`PendingGameWorld`].
//! Actually promoting the world's contents is left to a future
//! world-swap PR — PR-C closes the "log-only branches" gap and lands
//! the message-bus plumbing so the swap PR can drop in without
//! touching the kickoff or the subview.
//!
//! Per the design contract, the `restore_save` failure path
//! **surface a [`NotificationEvent`]**, never a panic.

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use std::fs;
use std::path::Path;

use super::io::write_save_atomic;
use super::restore::{restore_world, RestoreError};
use super::snapshot::{snapshot_world, SaveMetadata};
use crate::game_state::GameSeed;
use crate::persistence::playtime::PlaytimeTracker;
use crate::ui::launch::save_index::{SaveIndex, SaveIndexState};
use crate::ui::launch::userdata::{resolve_userdata_dir, PersistentSettings};
use crate::ui::launch::{LaunchState, NewGameRequest};
use crate::ui::notifications::events::{NotificationContextLink, NotificationEvent};
use crate::ui::notifications::NotificationCategoryId;
use crate::ui::time::TimeScale;

/// Bevy 0.18 `Message` broadcast when `play_new_game` succeeds.
///
/// `kickoff_world_system` writes this via
/// `MessageWriter<NewGameCommitted>`; downstream sim plugins (e.g.
/// [`crate::plugins::solar_system`]) read it to apply their
/// per-request initialisation. PR-C keeps the payload narrow because
/// the existing world-spawn code already reads [`GameSeed`] at the
/// call site — only the new [`NewGameParams`] need to ride across
/// the message boundary.
#[derive(Debug, Clone, Message)]
pub struct NewGameCommitted {
    pub request: NewGameRequest,
    pub playtime_s: u64,
    pub helios_version: String,
}

/// Bevy 0.18 `Message` broadcast when `restore_save` produces a
/// fresh world. Same lifecycle as `NewGameCommitted`; the consumer
/// system treats them identically (drain `PendingGameWorld`,
/// advance `LaunchState`).
#[derive(Debug, Clone, Message)]
pub struct RestoreCommitted {
    pub source_path: std::path::PathBuf,
    pub playtime_s: u64,
    pub helios_version: String,
}

/// Resource holding the freshly-constructed world awaiting promotion.
///
/// Storing the world in a resource (rather than passing it through
/// the message payload) is the only way to keep Bevy 0.18's borrow
/// checker happy while still letting the in-game swap run from a
/// normal system. `None` means "nothing pending".
#[derive(Resource, Debug, Default)]
pub struct PendingGameWorld {
    pub world: Option<World>,
}

/// Failure surface for the constructors. The kickoff consumer
/// matches on this; UI surfaces are routed through
/// [`NotificationEvent`] before the error reaches the player.
#[derive(Debug, Clone, PartialEq)]
pub enum GameSetupError {
    /// Save on disk could not be parsed or its version is
    /// incompatible. The detail string is human-readable.
    Restore(String),
    /// Writes directory could not be created or read.
    Io(String),
}

impl std::fmt::Display for GameSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameSetupError::Restore(s) => write!(f, "save restore failed: {s}"),
            GameSetupError::Io(s) => write!(f, "save write failed: {s}"),
        }
    }
}

impl std::error::Error for GameSetupError {}

impl From<RestoreError> for GameSetupError {
    fn from(e: RestoreError) -> Self {
        GameSetupError::Restore(e.to_string())
    }
}

/// Bevy plugin that wires `GameSetupPlugin` into the app.
pub struct GameSetupPlugin;

impl Plugin for GameSetupPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingGameWorld>()
            .init_resource::<SaveIndexState>()
            .add_message::<NewGameCommitted>()
            .add_message::<RestoreCommitted>()
            .add_systems(
                EguiPrimaryContextPass,
                promote_pending_world.in_set(crate::ui::launch::LaunchSystemSet::Menu),
            );
    }
}

/// Resolve the request's `seed` — `0` is the auto-roll sentinel
/// carried by `NewGameRequest` (the player pressed "New Game"
/// without picking a seed). PR-C does not own auto-rolling; we
/// derive a deterministic seed from the preset name so a player who
/// hits "New Game" without picking a seed still gets a working
/// world. The TODO is captured in the unit tests.
fn resolve_seed(request: &NewGameRequest) -> u64 {
    if request.seed != 0 {
        return request.seed;
    }
    let mut buf: [u8; 8] = [0; 8];
    for (i, b) in request.preset.as_bytes().iter().enumerate().take(8) {
        buf[i] = *b;
    }
    let derived = u64::from_le_bytes(buf);
    if derived == 0 {
        0xDEAD_BEEF_CAFE_F00D
    } else {
        derived
    }
}

/// Construct a fresh Bevy [`World`] from `request` and stash it in
/// [`PendingGameWorld`]. Emits [`NewGameCommitted`] so downstream
/// sim plugins can pick up the params.
pub fn play_new_game(world: &mut World, request: NewGameRequest) -> Result<u64, GameSetupError> {
    let seed = resolve_seed(&request);
    let request = NewGameRequest { seed, ..request };

    let fresh = build_minimal_world(seed);

    world.insert_resource(PendingGameWorld { world: Some(fresh) });

    world.write_message(NewGameCommitted {
        request: request.clone(),
        playtime_s: 0,
        helios_version: env!("CARGO_PKG_VERSION").to_string(),
    });

    Ok(seed)
}

/// Build a minimal [`World`] with the resources the rest of the
/// engine reads at the moment of promotion. Tests use this directly;
/// production callers supply their own factory through
/// [`play_new_game_with_factory`].
fn build_minimal_world(seed: u64) -> World {
    let mut w = World::new();
    w.init_resource::<AppTypeRegistry>();
    w.insert_resource(GameSeed { value: seed });
    w.insert_resource(PlaytimeTracker::default());
    w.insert_resource(LaunchState::InGame);
    w.insert_resource(TimeScale::default());
    w.insert_resource(PersistentSettings::default());
    w.insert_resource(SaveIndex::default());
    w.init_resource::<SaveIndexState>();
    w
}

/// World factory used by the kickoff's `restore_save` call site.
/// Differs from [`build_minimal_world`] only in that it ignores its
/// `seed` argument (the restored world carries its own metadata).
/// Exposed at the module scope so the kickoff system can name it
/// without an inline closure.
pub fn build_minimal_world_for_restore() -> World {
    build_minimal_world(0)
}

/// Production constructor: same as [`play_new_game`] but with a
/// caller-supplied [`World`] factory. PR-C's UI calls
/// [`play_new_game`] because we don't yet have a
/// build-the-whole-app-world factory wired in; the slot exists so
/// the next PR can lift it in without touching the message bus.
pub fn play_new_game_with_factory<F>(
    world: &mut World,
    request: NewGameRequest,
    world_factory: F,
) -> Result<u64, GameSetupError>
where
    F: FnOnce() -> World,
{
    let seed = resolve_seed(&request);
    let request = NewGameRequest { seed, ..request };

    let mut fresh = world_factory();
    fresh.insert_resource(GameSeed { value: seed });
    fresh.insert_resource(PlaytimeTracker::default());
    fresh.insert_resource(LaunchState::InGame);
    fresh.insert_resource(TimeScale::default());
    fresh.insert_resource(PersistentSettings::default());
    fresh.insert_resource(SaveIndex::default());
    fresh.init_resource::<SaveIndexState>();

    world.insert_resource(PendingGameWorld { world: Some(fresh) });

    world.write_message(NewGameCommitted {
        request,
        playtime_s: 0,
        helios_version: env!("CARGO_PKG_VERSION").to_string(),
    });

    Ok(seed)
}

/// Restore a save from `path`. Reads the file, runs
/// [`super::restore::restore_world`] into a fresh world via
/// `world_factory`, stashes the fresh world in [`PendingGameWorld`],
/// emits [`RestoreCommitted`], and re-scans [`SaveIndex`].
///
/// On **any failure** (file unreadable, RON malformed, version
/// mismatch, missing `AppTypeRegistry`), emits a
/// [`NotificationEvent`] with category `persistence.restore_failed`
/// so the player sees a toast — never just a log line.
pub fn restore_save<F>(
    world: &mut World,
    path: &Path,
    world_factory: F,
) -> Result<(), GameSetupError>
where
    F: FnOnce() -> World,
{
    let ron_text = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            emit_restore_failed(world, path, &format!("read failed: {e}"));
            return Err(GameSetupError::Io(format!("read {}: {e}", path.display())));
        }
    };

    let restored = match restore_world(&ron_text, world_factory) {
        Ok(r) => r,
        Err(e) => {
            emit_restore_failed(world, path, &e.to_string());
            // Re-scan so the menu list reflects the failed load
            // (entries may have been touched at file-system level).
            rescan_save_index(world);
            return Err(GameSetupError::from(e));
        }
    };

    world.insert_resource(PendingGameWorld {
        world: Some(restored.world),
    });

    rescan_save_index(world);

    world.write_message(RestoreCommitted {
        source_path: path.to_path_buf(),
        playtime_s: restored.metadata.playtime_s,
        helios_version: restored.metadata.helios_version,
    });

    Ok(())
}

/// Emit a player-facing toast for a restore failure. The category id
/// `persistence.restore_failed` is the LGD-owned row added to
/// `assets/data/notifications.ron` (GRA-362 PR-C).
fn emit_restore_failed(world: &mut World, path: &Path, detail: &str) {
    world.write_message(NotificationEvent {
        category: NotificationCategoryId::from("persistence.restore_failed"),
        severity: crate::ui::notifications::events::NotificationSeverity::Critical,
        title: "Save could not be loaded".to_string(),
        body: format!("{detail} — the save file has not been modified."),
        dedup_key: Some(format!("restore_failed:{}", path.display())),
        auto_dismiss_s: Some(8.0),
        sticky: false,
        context_link: NotificationContextLink::None,
    });
}

/// Re-scan the saves directory and stamp
/// [`SaveIndexState::last_scanned`].
pub fn rescan_save_index(world: &mut World) {
    let dir = resolve_userdata_dir().join(crate::ui::launch::save_index::SAVES_SUBDIR);
    let index = SaveIndex::scan(&dir);
    world.insert_resource(index);
    if let Some(mut state) = world.get_resource_mut::<SaveIndexState>() {
        state.last_scanned = std::time::Instant::now();
    }
}

/// Write the live [`World`] to `path` via
/// [`super::io::write_save_atomic`]. Re-scans [`SaveIndex`] on
/// success so the menu list updates immediately.
pub fn write_save_to_path(world: &World, path: &Path) -> Result<(), GameSetupError> {
    let seed = world
        .get_resource::<GameSeed>()
        .map(|g| g.value)
        .unwrap_or(0);
    let playtime = world
        .get_resource::<PlaytimeTracker>()
        .map(|p| p.total_real_s as u64)
        .unwrap_or(0);
    let metadata = SaveMetadata::new_now(seed, playtime, env!("CARGO_PKG_VERSION"));

    let ron = snapshot_world(world, metadata).map_err(|e| GameSetupError::Io(e.to_string()))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| GameSetupError::Io(format!("mkdir {}: {e}", parent.display())))?;
    }

    write_save_atomic(path, &ron).map_err(|e| GameSetupError::Io(e.to_string()))?;
    Ok(())
}

/// Bevy system: drain [`PendingGameWorld`] once a fresh world is
/// stashed, advance [`LaunchState`] to `InGame`, and clear message
/// buffer bookkeeping so the next frame's reader starts empty.
pub fn promote_pending_world(
    mut pending: ResMut<PendingGameWorld>,
    mut launch_state: ResMut<LaunchState>,
    mut new_game: MessageReader<NewGameCommitted>,
    mut restore: MessageReader<RestoreCommitted>,
) {
    let has_message = !new_game.is_empty() || !restore.is_empty();
    if pending.world.is_none() && !has_message {
        return;
    }
    // Drain readers so MessageReader::len() returns 0 on the next
    // frame. `read()` already consumes; we just need a no-op iteration.
    let _ = new_game.read().count();
    let _ = restore.read().count();
    pending.world.take();
    *launch_state = LaunchState::InGame;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::params::NewGameParams;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    // `add_message` is on `App`, not `World` — see
    // [[feedback-bevy-018-tests-app-not-schedule]].
    use bevy::app::App;
    use bevy::prelude::MinimalPlugins;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("helios-gamesetup-{tag}-{pid}-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn install_userdata_dir(tag: &str) -> PathBuf {
        let dir = fresh_dir(tag);
        // SAFETY: tests run single-threaded for env-var mutations.
        unsafe {
            std::env::set_var("HELIOS_USERDATA_DIR", &dir);
        }
        dir
    }

    #[test]
    fn resolve_seed_zero_is_replaced_with_preset_derived_seed() {
        let request = NewGameRequest {
            params: NewGameParams::default(),
            seed: 0,
            preset: "standard".to_string(),
        };
        assert_eq!(resolve_seed(&request), u64::from_le_bytes(*b"standard"));
    }

    #[test]
    fn resolve_seed_zero_short_preset_falls_back_to_constant() {
        let request = NewGameRequest {
            params: NewGameParams::default(),
            seed: 0,
            preset: String::new(),
        };
        assert_eq!(resolve_seed(&request), 0xDEAD_BEEF_CAFE_F00D);
    }

    #[test]
    fn resolve_seed_nonzero_round_trips() {
        let request = NewGameRequest {
            params: NewGameParams::default(),
            seed: 0x1234_5678_9ABC_DEF0,
            preset: "casual".to_string(),
        };
        assert_eq!(resolve_seed(&request), 0x1234_5678_9ABC_DEF0);
    }

    #[test]
    fn play_new_game_stashes_world_and_emits_message() {
        // `add_message` lives on `App`, not `World`. Per
        // [[feedback-bevy-018-tests-app-not-schedule]] we route
        // through `App::new()` so the message bus exists for the
        // `play_new_game` exclusive-system call to write to.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<PendingGameWorld>();
        app.add_message::<NewGameCommitted>();
        let request = NewGameRequest {
            params: NewGameParams::default(),
            seed: 42,
            preset: "standard".to_string(),
        };
        let world_mut = app.world_mut();
        let seed = play_new_game(world_mut, request).expect("ok");
        assert_eq!(seed, 42);
        assert!(app.world().resource::<PendingGameWorld>().world.is_some());
        // A message has been written to the bus (no readers yet).
        assert_eq!(
            app.world().resource::<Messages<NewGameCommitted>>().len(),
            1
        );
    }

    #[test]
    fn play_new_game_auto_seed_paths() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<PendingGameWorld>();
        app.add_message::<NewGameCommitted>();
        let request = NewGameRequest {
            params: NewGameParams::default(),
            seed: 0,
            preset: "standard".to_string(),
        };
        let seed = play_new_game(app.world_mut(), request).expect("ok");
        assert_eq!(seed, u64::from_le_bytes(*b"standard"));
    }

    #[test]
    fn restore_missing_path_emits_notification_and_returns_err() {
        let _dir = install_userdata_dir("missing");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<PendingGameWorld>();
        app.init_resource::<SaveIndexState>();
        app.add_message::<RestoreCommitted>();
        app.add_message::<NotificationEvent>();
        let bad_path = PathBuf::from("/tmp/definitely-not-a-real-save-zzz.ron");
        let result = restore_save(app.world_mut(), &bad_path, build_minimal_world_seed);
        assert!(matches!(result, Err(GameSetupError::Io(_))));
        // NotificationEvent was emitted on the message bus.
        assert_eq!(
            app.world().resource::<Messages<NotificationEvent>>().len(),
            1
        );
        let _ = fs::remove_dir_all(&_dir);
    }

    #[test]
    fn promote_drains_pending_and_advances_launch_state() {
        // Per [[feedback-bevy-018-tests-app-not-schedule]]: use
        // `App::new()` + `app.update()` rather than the raw
        // `World::run_schedule` path. Messages double-buffer on
        // Schedule, and deferred Commands need a frame to apply.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<LaunchState>();
        app.init_resource::<PendingGameWorld>();
        app.add_message::<NewGameCommitted>();
        app.add_message::<RestoreCommitted>();
        app.add_systems(Update, promote_pending_world);

        // Pre-load a fresh world + a NewGameCommitted so the
        // promote system has work to do.
        let fresh = build_minimal_world(7);
        app.insert_resource(PendingGameWorld { world: Some(fresh) });
        app.world_mut().write_message(NewGameCommitted {
            request: NewGameRequest {
                params: NewGameParams::default(),
                seed: 7,
                preset: "standard".to_string(),
            },
            playtime_s: 0,
            helios_version: "0.4.0".to_string(),
        });

        app.update();

        assert!(app.world().resource::<PendingGameWorld>().world.is_none());
        assert_eq!(*app.world().resource::<LaunchState>(), LaunchState::InGame);
    }

    fn build_minimal_world_seed() -> World {
        build_minimal_world(1)
    }
}
