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
use super::snapshot::{snapshot_world, SaveMetadata, SavePreview};
use crate::economy::{kardashev_scale_from_watts, ResourceType, SimulationHistory};
use crate::game_state::GameSeed;
use crate::persistence::playtime::PlaytimeTracker;
use crate::ui::launch::save_index::{SaveIndex, SaveIndexState};
use crate::ui::launch::userdata::{resolve_userdata_dir, PersistentSettings};
use crate::ui::launch::{LaunchState, NewGameRequest};
use crate::ui::notifications::events::{NotificationContextLink, NotificationEvent};
use crate::ui::notifications::NotificationCategoryId;
use crate::ui::time::{SimulationTime, TimeScale};

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
            // GRA-358 PR-B: `promote_pending_world` is an
            // exclusive system (`&mut World`). Bevy 0.18 exclusive
            // systems can't chain `.in_set(...)`.
            //
            // Schedule: `EguiPrimaryContextPass` (which runs
            // inside `PreUpdate::EguiPreUpdateSet::BeginPass`).
            // The kickoff system (`kickoff_world_system`) runs in
            // `Update` — see `register_kickoff_system` for why.
            // The promote runs on the next frame's
            // `EguiPrimaryContextPass` (1-frame lag is
            // acceptable; `WorldReady` is what gates the
            // boot-init chain, and the chain waits for it).
            //
            // We use `.after(LaunchSystemSet::Menu)` so the menu
            // subview render systems finish first — they populate
            // `PendingLaunchActions` which `kickoff_world_system`
            // reads in `Update`.
            .add_systems(
                EguiPrimaryContextPass,
                promote_pending_world.after(crate::ui::launch::LaunchSystemSet::Menu),
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
///
/// GRA-XXX (2026-07-24): this used to be a bare `World::new()` +
/// `init_resource` boilerplate. **That's the wrong shape for the
/// restore path** — Bevy's `SceneDeserializer` (called by
/// [`super::restore::restore_world`]) reads the world-local
/// `AppTypeRegistry` to resolve every type path in the RON body,
/// and the registry starts empty when no plugin ever ran on the
/// world. So even though [`super::PersistencePlugin::build`]
/// registers every simulation-state type we care about (see
/// `src/persistence/mod.rs`), those `register_type::<…>()` calls
/// never fire on the bare-WORLD factory — the loader sees an
/// empty registry and aborts on the first unknown type path.
///
/// Player-visible symptom: 2026-07-24T09:33Z — saves written by
/// the patched binary still fail to load with `no registration
/// found for
/// `helios_ascension::astronomy::components::CurrentStarSystem``.
///
/// Fix: construct an `App`, add `MinimalPlugins` +
/// `PersistencePlugin`, and swap its world contents into the
/// return value. `PersistencePlugin::build` then populates the
/// `AppTypeRegistry` exactly the way it does on the live `App`,
/// and the deserializer can resolve every type path the snapshot
/// references. The swap pattern (`mem::swap` with a sentinel
/// `World::default()`) is necessary because Bevy 0.18's `App`
/// holds its `World` by `&mut` reference — there's no
/// `into_world()`. See the regression test
/// `build_minimal_world_runs_persistence_plugin` below for the
/// behavioural assertion.
///
/// The chain is kept narrow on purpose — only
/// `MinimalPlugins` + `PersistencePlugin`, not the full
/// astronomy / colony / economy / fleets / etc. plugin
/// stack. Those plugins do also call `register_type::<…>()`
/// at startup, but they init resources, queue systems, and
/// pull in render / winit dependencies. The 29-entry
/// `register_type` list inside [`super::PersistencePlugin::build`]
/// is the curated registry of "what does the snapshot's RON
/// actually reference" — which is exactly what we need for the
/// loader to succeed. Adding the full plugin stack would
/// side-effect the world with render / windowing state that
/// the restore path doesn't want to carry.
fn build_minimal_world(seed: u64) -> World {
    use bevy::app::App;
    use bevy::MinimalPlugins;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    // `PersistencePlugin::build` is the crucial call — it
    // populates the world-local `AppTypeRegistry` with every
    // simulation-state Component/Resource/Enum-key the
    // snapshot's RON body references. Without this, every
    // `SceneDeserializer` lookup on a `helios_ascension::*`
    // type path returns `None` and the restore aborts. The
    // existing test in `super::plugin_registers_type_registry`
    // is the foundation; this is the closure that uses the
    // populated registry end-to-end.
    app.add_plugins(super::PersistencePlugin);

    // The `App` lives in `app`, but the restore factory
    // contract is `FnOnce() -> World`. Bevy 0.18 doesn't
    // expose `App::into_world()`; the canonical swap is
    // `mem::swap` with a sentinel `World::default()`. The
    // sentinel gets the now-empty post-swap internals; the
    // return value gets the populated world (with the
    // `AppTypeRegistry` already populated).
    let mut world = World::default();
    std::mem::swap(&mut world, app.world_mut());
    drop(app);
    // `PersistencePlugin::build` already initialised
    // `AppTypeRegistry` (with the 29-entry curated chain from
    // `src/persistence/mod.rs`) on the world, plus the
    // `MinimalPlugins` baseline (Time<Real>, Time<Virtual>,
    // etc.). We don't want to clobber the registry with an
    // empty one — just confirm the post-swap shape and add
    // the Helios-managed resources on top.
    assert!(
        world.get_resource::<AppTypeRegistry>().is_some(),
        "PersistencePlugin::build must insert AppTypeRegistry; \
         bare-World factory without plugin chain would defeat the \
         restore path. See game_setup.rs doc comments."
    );
    world.insert_resource(GameSeed { value: seed });
    world.insert_resource(PlaytimeTracker::default());
    world.insert_resource(LaunchState::InGame);
    world.insert_resource(TimeScale::default());
    world.insert_resource(PersistentSettings::default());
    world.insert_resource(SaveIndex::default());
    world.init_resource::<SaveIndexState>();
    world
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
#[allow(dead_code)]
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
            return Err(GameSetupError::Io(format!(
                "read {p}: {e}",
                p = path.display()
            )));
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
        dedup_key: Some(format!("restore_failed:{p}", p = path.display())),
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
    let mut metadata = SaveMetadata::new_now(seed, playtime, env!("CARGO_PKG_VERSION"));
    metadata.preview = build_save_preview(world, path);

    let ron = snapshot_world(world, metadata).map_err(|e| GameSetupError::Io(e.to_string()))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| GameSetupError::Io(format!("mkdir {p}: {e}", p = parent.display())))?;
    }

    write_save_atomic(path, &ron).map_err(|e| GameSetupError::Io(e.to_string()))?;
    Ok(())
}

fn build_save_preview(world: &World, path: &Path) -> SavePreview {
    let current_date = world
        .get_resource::<SimulationTime>()
        .map(SimulationTime::format_date_time)
        .unwrap_or_default();
    let Some(history) = world.get_resource::<SimulationHistory>() else {
        return SavePreview {
            current_date,
            ..default()
        };
    };
    let Some(latest) = history.latest() else {
        return SavePreview {
            current_date,
            ..default()
        };
    };

    let resources = ResourceType::all()
        .iter()
        .copied()
        .filter_map(|resource| {
            let amount = latest.resource_amount(resource);
            (amount.abs() > f64::EPSILON).then(|| (resource.display_name().to_string(), amount))
        })
        .collect();
    let current_sim_seconds = world
        .get_resource::<SimulationTime>()
        .map(SimulationTime::elapsed_seconds)
        .unwrap_or(latest.sim_seconds);
    let kardashev_history = history.kardashev_history_for_preview(current_sim_seconds);
    let screenshot_file = path
        .file_stem()
        .map(|stem| format!("{}.png", stem.to_string_lossy()));

    SavePreview {
        current_date,
        colony_count: latest.colony_count,
        total_population: latest.total_population,
        ship_count: latest.ship_count,
        power_produced_watts: latest.power_produced_watts,
        kardashev_value: kardashev_scale_from_watts(latest.power_produced_watts),
        resources,
        kardashev_history,
        screenshot_file,
    }
}

/// Bevy system: drain [`PendingGameWorld`] once a fresh world is
/// stashed, advance [`LaunchState`] to `InGame`, and clear message
/// buffer bookkeeping so the next frame's reader starts empty.
///
/// PR-B (GRA-358) replaces the previous "drop the pending world on
/// the floor" behaviour with a real world-swap via
/// [`super::swap::swap_world_into`]. On success the system inserts
/// [`super::swap::WorldReady`], the marker resource that gates the
/// `Startup`-time content spawns (see [`crate::boot_init::BootInitPlugin`]
/// for the chain that fires after `WorldReady` is present).
///
/// # Exclusive system
///
/// `swap_world_into` needs `&mut World`, and so does inserting
/// `WorldReady` once the swap completes. Bevy 0.18 exposes
/// `&mut World` only as an `ExclusiveSystemParam`, so the whole
/// promotion runs through the exclusive-system path
/// (`app.add_systems(EguiPrimaryContextPass, promote_pending_world)`).
/// `tick_autosave_timer` in `autosave.rs:185` uses the same
/// pattern.
///
/// Failures surface as a [`GameSetupError`] via the surrounding
/// `kickoff_world_system` consumer; the swap already logged the
/// root cause at the source.
pub fn promote_pending_world(world: &mut World) {
    // Snapshot the resource handles up front so we don't hold a
    // long-lived `ResMut` borrow across the swap.
    let has_pending = world
        .get_resource::<PendingGameWorld>()
        .map(|p| p.world.is_some())
        .unwrap_or(false);
    let has_new_game_msg = !world
        .get_resource::<Messages<NewGameCommitted>>()
        .map(|m| m.is_empty())
        .unwrap_or(true);
    let has_restore_msg = !world
        .get_resource::<Messages<RestoreCommitted>>()
        .map(|m| m.is_empty())
        .unwrap_or(true);
    if !has_pending && !has_new_game_msg && !has_restore_msg {
        return;
    }

    // Drain the message readers so MessageReader::len() returns 0
    // on the next frame. `read()` already consumes; we just need
    // a no-op iteration.
    if let Some(mut msgs) = world.get_resource_mut::<Messages<NewGameCommitted>>() {
        let _ = msgs.drain().count();
    }
    if let Some(mut msgs) = world.get_resource_mut::<Messages<RestoreCommitted>>() {
        let _ = msgs.drain().count();
    }

    // Run the swap into the *live* world. We take the pending
    // world OUT of the PendingGameWorld resource first so the
    // borrow on `world` is released before we re-borrow to run
    // the swap (Bevy 0.18's borrow checker rejects
    // `&mut PendingGameWorld + &mut World` simultaneously).
    //
    // `swap_world_into` itself takes `&mut PendingGameWorld`, so
    // we hand it a tiny fresh resource with the inner world.
    let swap_result = {
        let pending_world = world
            .get_resource_mut::<PendingGameWorld>()
            .and_then(|mut p| p.world.take());
        match pending_world {
            None => Err(super::swap::SwapError::NothingPending),
            Some(pending_world) => {
                let mut pending = PendingGameWorld {
                    world: Some(pending_world),
                };
                super::swap::swap_world_into(&mut pending, world)
            }
        }
    };

    match swap_result {
        Ok(()) => {}
        Err(super::swap::SwapError::NothingPending) => {
            // No world to swap — message-only transition. Fall
            // through to the LaunchState flip.
        }
        Err(err) => {
            // Swap failed (unregistered component, etc.). Log
            // loudly + emit a player toast. The swap already
            // drained the pending world, so we can't retry on
            // this frame.
            error!(
                target: "persistence::promote_pending_world",
                "world swap failed: {err} — falling through to LaunchState::InGame anyway \
                 so the menu dismisses; the live world may be in an undefined state"
            );
            // Reuse the `persistence.restore_failed` category id
            // so the notification row in
            // assets/data/notifications.ron covers both save
            // restore and world swap failures.
            world.write_message(NotificationEvent {
                category: NotificationCategoryId::from("persistence.restore_failed"),
                severity: crate::ui::notifications::events::NotificationSeverity::Critical,
                title: "World swap failed".to_string(),
                body: format!(
                    "{err} — the live world may be in an undefined state. Save before quitting."
                ),
                dedup_key: Some(format!("swap_failed:{err}")),
                auto_dismiss_s: Some(8.0),
                sticky: false,
                context_link: NotificationContextLink::None,
            });
        }
    }

    // Insert the WorldReady marker so the deferred-init chain
    // (`BootInitPlugin`) fires. On swap-error paths we still
    // insert it so the player gets a working main menu; the
    // boot_init chain is idempotent and the toast above surfaces
    // the corruption.
    if !world.contains_resource::<super::swap::WorldReady>() {
        world.insert_resource(super::swap::WorldReady);
    }

    // Flip LaunchState to InGame so the menu dismisses and the
    // resource-bar / fleet / research panels become visible.
    if let Some(mut launch) = world.get_resource_mut::<LaunchState>() {
        *launch = LaunchState::InGame;
    }
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
        //
        // The production system is registered in
        // `EguiPrimaryContextPass` (per
        // `[[feedback-bevy-018-egui-context-pass]]`). The test
        // schedules it in `Update` so `app.update()` ticks it
        // without dragging in `bevy_egui::EguiPlugin` (which
        // requires `Assets<Shader>` and a render device, neither
        // of which the unit test needs). The production
        // schedule choice is unchanged.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::persistence::PersistencePlugin);
        app.init_resource::<LaunchState>();
        app.init_resource::<PendingGameWorld>();
        app.add_message::<NewGameCommitted>();
        app.add_message::<RestoreCommitted>();
        // `promote_pending_world` is an exclusive system in
        // GRA-358 PR-B (it takes `&mut World`).
        app.add_systems(Update, promote_pending_world);

        // Pre-load a fresh world + a NewGameCommitted so the
        // promote system has work to do. `build_minimal_world`
        // already populates AppTypeRegistry via
        // PersistencePlugin::build, so the swap's
        // `ReflectComponent::copy` lookups succeed.
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

        // PR-A's contract: pending drains, launch state advances.
        assert!(app.world().resource::<PendingGameWorld>().world.is_none());
        assert_eq!(*app.world().resource::<LaunchState>(), LaunchState::InGame);

        // PR-B's addition: WorldReady is inserted (gates the
        // deferred-init chain in BootInitPlugin), and the live
        // world carries the swapped resource set. Resource-level
        // copy semantics are covered by the dedicated swap unit
        // tests (`swap_pending_into_empty_target_copies_three_entities`
        // and friends) — this test focuses on the orchestration
        // contract (drain + state advance + marker insert).
        assert!(
            app.world().contains_resource::<crate::persistence::swap::WorldReady>(),
            "promote_pending_world must insert WorldReady after a successful swap"
        );
    }

    fn build_minimal_world_seed() -> World {
        build_minimal_world(1)
    }

    /// Regression test for the player-visible failure from
    /// 2026-07-24T09:33Z: `restore_save failed: save restore
    /// failed: scene deserialise failed: no registration
    /// found for
    /// `helios_ascension::astronomy::components::CurrentStarSystem``.
    ///
    /// The `PersistencePlugin` register list expanded a lot
    /// in commit `4a46a76`, but that fix was
    /// structurally insufficient because the restore
    /// factory `build_minimal_world_for_restore()` built a
    /// **bare `World::new()`** that never ran any plugins —
    /// so `PersistencePlugin::build`'s `register_type::<…>()`
    /// chain never fired, and the deserializer's
    /// `AppTypeRegistry` lookup failed the same way it did
    /// before the fix. This test pins the contract: the
    /// factory returned by `build_minimal_world_for_restore`
    /// must carry a world-local `AppTypeRegistry` whose
    /// `helios_ascension::astronomy::components::
    /// CurrentStarSystem` entry exists, mirroring what the
    /// live `App`'s plugin chain would have populated.
    ///
    /// If a future maintainer reverts
    /// `build_minimal_world` to its pre-fix bare-WORLD
    /// shape, this test fails deterministically — the
    /// same regression the player reported. Mismatches
    /// appear at compile time (if the factory signature
    /// drifts) and run time (if the registry contents drift).
    /// Mirrors `package_astronomy_registration_into_persistence`
    /// in `src/persistence/mod.rs` but exercises the factory
    /// end-to-end rather than the plugin registration step.
    #[test]
    fn build_minimal_world_runs_persistence_plugin() {
        // Resolve the post-2026-07-24 fix shape. The
        // factory now constructs an App, runs
        // PersistencePlugin::build, and swaps the
        // populated world out into the return value.
        let world = build_minimal_world_for_restore();

        // Sanity: AppTypeRegistry must be present and populated.
        // A bare `World::new()` would not have one — that's
        // the pre-fix failure mode this test pins against.
        // `world.resource::<T>()` panics on a missing
        // resource; the type-id check on the read guard
        // gives a clearer failure message when the assert
        // path runs in a hot loop.
        let registry = world.resource::<AppTypeRegistry>();
        let registry_handle = registry.clone();
        let registry_locked = registry_handle.read();

        // The exact type path the player-reported load
        // failure surfaced. Must resolve, or the test
        // (and the production restore) reverts to the
        // pre-fix behaviour.
        let path = "helios_ascension::astronomy::components::CurrentStarSystem";
        assert!(
            registry_locked.get_with_type_path(path).is_some(),
            "build_minimal_world_for_restore must register \
             `{path}` in AppTypeRegistry. The factory builds an \
             App, runs PersistencePlugin::build (which calls \
             register_type for this type), and swaps the \
             populated world out. If this assert fails, the \
             factory has reverted to the bare-World::new() \
             shape and the restore path will abort on the \
             first scene-deserialise type lookup."
        );
    }
}
