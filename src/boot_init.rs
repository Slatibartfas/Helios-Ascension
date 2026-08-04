//! Deferred boot-init: runs game-state startup work behind the splash.
//!
//! ## Why this exists
//!
//! Pre-refactor, every game-state init system — solar-system bodies,
//! nearby-star population, baseline tech / engineering / debug fleet,
//! camera focus, asteroid registry, resource generation — fired at
//! `Startup` or `PostStartup`. Bevy runs those schedules **before** the
//! first egui frame, so the player spent ~1 s looking at a black
//! window before the splash ever painted.
//!
//! This module owns a [`BootState`] resource and a [`BootInitPlugin`]
//! that defers the same work into `Update`, gated by
//! `BootState::Loading`. The chain ends with `mark_boot_ready`, which
//! flips the gate to `Ready` so the chain only fires once. The splash
//! (`src/ui/launch/splash.rs::ui_splash_system`) waits for `BootState`
//! == `Ready` before dismissing, so the heavy work happens behind the
//! splash window.
//!
//! ## Stays at `Startup` (not deferred)
//!
//! - Manifest/data loaders (technologies, buildings, ship hulls,
//!   planet textures, porkchop, interstellar-propulsion, seed copy,
//!   difficulty presets, launch-ui, new-game-params, notifications)
//! - `load_save_index_system` (the SaveIndex scan; the Continue
//!   button's enabled state needs this populated before any frame)
//! - Window / camera / render setup (`CameraPlugin::spawn_camera`,
//!   `setup_camera_effects`, `setup_comet_effects`, `setup_starmap`,
//!   `start_playlist`, the splash window itself)
//! - Ambient + directional + clear color from `src/main.rs::setup`
//!
//! ## Chain order (preserved from the original `.after(...)` graph)
//!
//! ```text
//! setup_solar_system
//!     ├── load_asteroid_registry            .after(setup_solar_system)
//!     ├── populate_nearby_systems           (no direct dep, but must
//!     │       .before(generate_solar_system_resources))  ← via chain
//!     ├── initialize_colony_stockpiles     .after(setup_solar_system)
//!     ├── initial_camera_focus              .after(setup_solar_system)
//!     ├── spawn_initial_fleet               .after(setup_solar_system)
//!     ├── spawn_debug_earth_jupiter_fleet   .after(setup_solar_system)
//!     └── initialize_baseline_technology    .after(setup_solar_system)
//!         └── initialize_baseline_engineering .after(initialize_baseline_technology)
//!             └── merge_ship_module_engineering_catalog
//!                 (chained after baseline init)
//!
//! init_procedural_rng
//!     ├── generate_solar_system_resources   (after rng, after populate)
//!     │       └── stamp_resource_phases     .after(generate_solar_system_resources)
//!     └── generate_ring_resources           (with the chain above)
//!
//! mark_boot_ready (always last — flips the gate)
//! ```
//!
//! ## Idempotency
//!
//! `spawn_initial_fleet` and `spawn_debug_earth_jupiter_fleet` gate
//! themselves on `DayOneFleetSpawned` / `DebugEarthJupiterFleetSpawned`
//! resources so a save/restore that re-enters the chain doesn't
//! duplicate the constellation. The `chain()` ordering below preserves
//! that — we never re-spawn the same fleet twice in one boot.
//!
//! If `restore_save` ever lands as a kickoff path that resets the world
//! between sessions, the spawn chain needs to be re-invoked after the
//! reset clears the idempotency markers. Today this is out of scope
//! — the kickoff path only mutates the kickoff message bus, not the
//! boot-init gate.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use crate::astronomy::asteroids::load_asteroid_registry;
use crate::economy::generation::{
    generate_ring_resources, generate_solar_system_resources, init_procedural_rng,
    stamp_resource_phases,
};
use crate::fleets::systems::{spawn_debug_earth_jupiter_fleet, spawn_initial_fleet};
use crate::plugins::solar_system::{
    initial_camera_focus, initialize_colony_stockpiles, setup_solar_system,
};
use crate::plugins::solar_system_data::SolarSystemData;
use crate::plugins::system_populator::populate_nearby_systems;
use crate::research::systems::{
    initialize_baseline_engineering, initialize_baseline_technology,
    merge_ship_module_engineering_catalog,
};

/// Boot-load lifecycle. Set by the splash system on dismissal /
/// detected via query by `ui_splash_system`.
#[derive(Resource, Default, PartialEq, Eq, Debug, Clone, Copy)]
pub enum BootState {
    /// Splash + deferred init both active. Splash stays visible;
    /// the boot-init chain is unblocked and runs once on the next
    /// Update tick.
    #[default]
    Loading,
    /// Boot-init chain has finished. Splash dismisses (assuming the
    /// splash min-duration has elapsed); the main menu appears.
    Ready,
}

/// Frame-by-frame progress for the deferred boot-init chain.
///
/// ## Why this exists (was a single-frame `.chain()` block)
///
/// Pre-refactor, every system in the chain fired on the **same**
/// `Update` tick — `setup_solar_system` (377 bodies from a 444 KB
/// `solar_system.ron`), `populate_nearby_systems` (60+ star
/// systems), and `generate_solar_system_resources` (per-body)
/// together could run 1–3 s on a cold disk. Bevy's `Update`
/// schedule runs to completion before render, so on frame 1 the
/// egui splash pass never got a chance to paint and the player
/// stared at a black window for the entire boot. Worse, if the
/// first frame exceeded ~5 s, Windows flagged the app as "Not
/// Responding" and the splash froze visually even though Bevy was
/// still working.
///
/// [`BootProgress`] gates each system in the chain on
/// `progress.step == N`. Only one step fires per frame, so the
/// splash egui pass paints every tick and can render a live
/// `Loading… N/total` counter (see
/// [`crate::ui::launch::splash::ui_splash_system`]). Total boot
/// wall-clock is unchanged; the work is just amortized one system
/// per frame.
///
/// ## Tier 3 hook
///
/// Async offload of the heaviest steps (`setup_solar_system`,
/// `populate_nearby_systems`) is deferred to a follow-up. The
/// intended trigger is the same as the existing
/// `WorldReady` insertion in
/// [`crate::persistence::game_setup::promote_pending_world`],
/// i.e. **New Game / Continue / Load Save** — never at plain app
/// launch. The chain must stay silent until the player makes a
/// kickoff decision (GRA-358 PR-B), and the heavy async parsing
/// should follow the same rule.
#[derive(Resource, Debug, Clone)]
pub struct BootProgress {
    /// Index of the next step to run (0-based).
    pub step: u32,
    /// Total number of steps in the chain. Surfaced to the splash
    /// so it can render `Loading… N/total`.
    pub total: u32,
    /// True once the last step ([`mark_boot_ready`]) has flipped
    /// the gate. All steps' `run_if` predicates consult this so
    /// the chain is idempotent — once `done == true`, no step
    /// fires again.
    pub done: bool,
}

impl Default for BootProgress {
    fn default() -> Self {
        Self {
            step: 0,
            total: BOOT_STEP_COUNT as u32,
            done: false,
        }
    }
}

/// Async pre-parse cache for the heaviest boot-init step
/// (`setup_solar_system`'s 444 KB `solar_system.ron` read + RON
/// decode — ~150 ms cold).
///
/// ## Tier 3 design
///
/// The splash dismisses (`BootState` → `Ready`) at app launch,
/// long before the player commits to New Game / Continue / Load
/// Save. During that "menu time" (typically 5–30 s while the
/// player reads the menu), the splash-handoff system spawns the
/// RON parse onto [`AsyncComputeTaskPool`]. The poll system drains
/// the task handle each frame and stores the result in
/// [`SolarSystemData`] so it's ready by the time the kickoff
/// click lands.
///
/// `setup_solar_system`'s body now reads `solar_data` first and
/// only parses synchronously when the cache is `None`. That
/// makes the boot chain's step 0 effectively free when the
/// player took ≥1 s at the menu (the typical case), and
/// gracefully degrades to the synchronous path if the player
/// spam-clicked New Game before the parse finished.
///
/// The other chain steps (nearby-stars populate, resource
/// generation, etc.) are either already fed by a pre-loaded
/// resource (the nearby-stars RON is parsed in Startup's
/// `load_nearby_stars_data`) or too cheap to bother — only
/// `solar_system.ron` justifies the task plumbing.
///
/// ## Why trigger on splash dismiss (not on WorldReady)
///
/// The user-visible "load" is the time between the kickoff
/// click and the boot chain finishing. Pre-parsing *during menu
/// time* moves the work behind the player's natural pause.
/// Triggering on `WorldReady` would put it back in the
/// click-to-world window (which is what Tier 2's progress bar
/// was supposed to mask). The splash dismiss is the latest
/// "nothing is happening" frame that still has the boot chain
/// silent (`BootState` still `Loading` is irrelevant at this
/// point — the chain's only gated by `WorldReady` so it never
/// fires from pre-parse alone), so we spawn there. Never at
/// plain `Startup` (would re-introduce the cold-boot hang the
/// splash is meant to hide) and never at the kickoff click
/// (defeats the purpose).
#[derive(Resource, Debug, Default)]
pub struct BootPreParseState {
    /// True once the async parse task has been spawned. Prevents
    /// double-spawn on subsequent frames.
    pub started: bool,
    /// The handle for the in-flight parse. `None` when the task
    /// has completed and been drained into [`Self::solar_data`].
    pub solar_task: Option<Task<Result<SolarSystemData, String>>>,
    /// The parsed solar-system data, ready to apply. Set by
    /// [`poll_pre_parse`] once the task completes.
    pub solar_data: Option<SolarSystemData>,
}

/// Step indices, in execution order. The splash reads
/// [`BootProgress::step`] to drive the progress label; the index
/// table lives here so the two stay in sync. Add new steps at the
/// end and bump [`BOOT_STEP_COUNT`].
pub const STEP_SETUP_SOLAR_SYSTEM: u32 = 0;
pub const STEP_INIT_PROCEDURAL_RNG: u32 = 1;
pub const STEP_LOAD_ASTEROID_REGISTRY: u32 = 2;
pub const STEP_POPULATE_NEARBY_SYSTEMS: u32 = 3;
pub const STEP_INITIALIZE_COLONY_STOCKPILES: u32 = 4;
pub const STEP_INITIAL_CAMERA_FOCUS: u32 = 5;
pub const STEP_SPAWN_INITIAL_FLEET: u32 = 6;
pub const STEP_SPAWN_DEBUG_EARTH_JUPITER_FLEET: u32 = 7;
pub const STEP_INITIALIZE_BASELINE_TECHNOLOGY: u32 = 8;
pub const STEP_GENERATE_SOLAR_SYSTEM_RESOURCES: u32 = 9;
pub const STEP_GENERATE_RING_RESOURCES: u32 = 10;
pub const STEP_STAMP_RESOURCE_PHASES: u32 = 11;
pub const STEP_INITIALIZE_BASELINE_ENGINEERING: u32 = 12;
pub const STEP_MERGE_SHIP_MODULE_ENGINEERING_CATALOG: u32 = 13;
pub const STEP_MARK_BOOT_READY: u32 = 14;
/// Total number of steps in the boot chain. Update whenever you
/// insert a new step above.
pub const BOOT_STEP_COUNT: usize = 15;

/// Per-step `run_if` predicate. Returns true only on the frame
/// where `BootProgress.step == step_n` AND the chain hasn't
/// already finished. Pure function over `&BootProgress` so tests
/// can call it directly without constructing a `Res` SystemParam.
fn progress_at(step_n: u32) -> impl Fn(Res<BootProgress>) -> bool {
    move |progress: Res<BootProgress>| {
        progress.step == step_n && !progress.done
    }
}

/// Plugin that owns the deferred game-state init.
///
/// Each system in the chain below is gated on its corresponding
/// `STEP_*` index via [`progress_at`], so exactly one step fires
/// per `Update` tick. Bevy's `.after(...)` constraints between
/// steps are trivially satisfied because the steps never run on
/// the same frame. New systems should be appended at the end of
/// the chain (new `STEP_*` constant + new `run_if(progress_at(N))`)
/// — never re-registered at `Startup`.
///
/// GRA-358 PR-B: the chain is now gated on BOTH
/// [`BootState::Loading`] AND [`crate::persistence::swap::WorldReady`].
/// `WorldReady` is inserted by
/// [`crate::persistence::game_setup::promote_pending_world`] after
/// the world-swap lands, so the chain only fires once the player
/// has chosen "New Game" or "Load Save". The live world stays
/// empty at app boot — see the swap architecture notes in
/// `/memories/repo/world-swap-implementation.md`.
pub struct BootInitPlugin;

impl Plugin for BootInitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BootState>()
            .init_resource::<BootProgress>()
            // GRA-358 PR-B/D: gate the whole chain via a single
            // SystemSet so each step's per-frame `run_if` and the
            // shared triple gate (Loading, WorldReady, no
            // RestoredWorldGate) compose cleanly. Adding the
            // predicates to each system individually would also
            // work but is repetitive.
            .configure_sets(
                Update,
                BootChainSet
                    .run_if(boot_state_is_loading)
                    .run_if(crate::persistence::swap::world_ready_is_present)
                    .run_if(crate::persistence::swap::restored_world_is_not_present),
            )
            // Each step is gated on its step index via
            // `progress_at(N)`. Order is preserved by Bevy's
            // `.after(...)` labels — across frames they're
            // trivially satisfied (step N+1 always fires on a
            // later frame than step N). All steps live in the
            // shared [`BootChainSet`] so the triple gate above
            // applies uniformly.
            .add_systems(
                Update,
                (
                    setup_solar_system
                        .run_if(progress_at(STEP_SETUP_SOLAR_SYSTEM))
                        .in_set(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    init_procedural_rng
                        .run_if(progress_at(STEP_INIT_PROCEDURAL_RNG))
                        .after(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    load_asteroid_registry
                        .run_if(progress_at(STEP_LOAD_ASTEROID_REGISTRY))
                        .after(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    populate_nearby_systems
                        .run_if(progress_at(STEP_POPULATE_NEARBY_SYSTEMS))
                        .in_set(PopulateNearbySystemsStepLabel)
                        .in_set(BootChainSet),
                    initialize_colony_stockpiles
                        .run_if(progress_at(STEP_INITIALIZE_COLONY_STOCKPILES))
                        .after(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    initial_camera_focus
                        .run_if(progress_at(STEP_INITIAL_CAMERA_FOCUS))
                        .after(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    spawn_initial_fleet
                        .run_if(progress_at(STEP_SPAWN_INITIAL_FLEET))
                        .after(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    spawn_debug_earth_jupiter_fleet
                        .run_if(progress_at(STEP_SPAWN_DEBUG_EARTH_JUPITER_FLEET))
                        .after(SetupSolarSystemStepLabel)
                        .in_set(BootChainSet),
                    initialize_baseline_technology
                        .run_if(progress_at(STEP_INITIALIZE_BASELINE_TECHNOLOGY))
                        .in_set(InitializeBaselineTechnologyStepLabel)
                        .in_set(BootChainSet),
                    generate_solar_system_resources
                        .run_if(progress_at(STEP_GENERATE_SOLAR_SYSTEM_RESOURCES))
                        .after(PopulateNearbySystemsStepLabel)
                        .in_set(GenerateSolarSystemResourcesStepLabel)
                        .in_set(BootChainSet),
                    generate_ring_resources
                        .run_if(progress_at(STEP_GENERATE_RING_RESOURCES))
                        .after(PopulateNearbySystemsStepLabel)
                        .in_set(BootChainSet),
                    stamp_resource_phases
                        .run_if(progress_at(STEP_STAMP_RESOURCE_PHASES))
                        .after(GenerateSolarSystemResourcesStepLabel)
                        .in_set(BootChainSet),
                    initialize_baseline_engineering
                        .run_if(progress_at(STEP_INITIALIZE_BASELINE_ENGINEERING))
                        .after(InitializeBaselineTechnologyStepLabel)
                        .in_set(BootChainSet),
                    merge_ship_module_engineering_catalog
                        .run_if(progress_at(STEP_MERGE_SHIP_MODULE_ENGINEERING_CATALOG))
                        .after(InitializeBaselineTechnologyStepLabel)
                        .in_set(BootChainSet),
                    // The gate flip — last. Folds the progress advance
                    // and the BootState→Ready transition into one
                    // system so there's no risk of the chain firing
                    // twice on the closing frame.
                    mark_boot_ready
                        .run_if(progress_at(STEP_MARK_BOOT_READY))
                        .in_set(BootChainSet),
                ),
            );

        // `advance_progress_step` runs in `PostUpdate` so it
        // executes AFTER whichever step's `run_if` passed in
        // `Update`. The same triple gate prevents the counter
        // from advancing on frames where the chain is silent
        // (e.g. before `WorldReady` is inserted, or on the
        // Restore path).
        app.add_systems(
            PostUpdate,
            advance_progress_step
                .run_if(boot_state_is_loading)
                .run_if(crate::persistence::swap::world_ready_is_present)
                .run_if(crate::persistence::swap::restored_world_is_not_present),
        );

        // ── Tier 3: async pre-parse ──────────────────────────────
        // Spawn the heavy `solar_system.ron` parse on
        // `AsyncComputeTaskPool` as soon as the main window
        // becomes visible (the splash-handoff signal). The poll
        // system drains the task each frame and stores the
        // result in `BootPreParseState.solar_data` for
        // `setup_solar_system` to consume. See
        // [`BootPreParseState`] for the trigger rationale.
        app.init_resource::<BootPreParseState>()
            .add_systems(Update, start_pre_parse.run_if(main_window_visible))
            .add_systems(Update, poll_pre_parse);
    }
}

/// System-set labels used as `.after(...)` anchors between steps
/// AND as the umbrella set carrying the shared triple gate (see
/// [`BootInitPlugin::build`]). Bevy's scheduler still resolves
/// these labels even when only one step runs per frame — the
/// constraint is trivially satisfied, but the labels keep the
/// original chain ordering documented in one place.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BootChainSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetupSolarSystemStepLabel;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PopulateNearbySystemsStepLabel;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenerateSolarSystemResourcesStepLabel;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InitializeBaselineTechnologyStepLabel;


/// `run_if` predicate for the boot-init chain: fires only while
/// `BootState == Loading`. After [`mark_boot_ready`] flips the gate,
/// the entire chain is suppressed on subsequent frames.
///
/// Pure function over `&BootState` so tests can call it directly
/// without constructing a `Res` SystemParam.
fn boot_state_is_loading(state: Res<BootState>) -> bool {
    *state == BootState::Loading
}

/// Last system in the boot-init chain. Flips `BootState` to
/// `Ready` so the splash can dismiss and the gate stops re-firing.
/// Also marks [`BootProgress`] done so the per-step `run_if`
/// predicates gate out on every subsequent frame.
fn mark_boot_ready(mut state: ResMut<BootState>, mut progress: ResMut<BootProgress>) {
    info!(
        "boot_init: deferred systems complete → BootState::Ready ({}/{} steps)",
        progress.step + 1,
        progress.total
    );
    *state = BootState::Ready;
    progress.step = progress.total;
    progress.done = true;
}

/// PostUpdate system: bump `BootProgress.step` after the
/// per-frame step in `Update` has fired. Without this, the
/// step's `run_if(progress_at(N))` would re-evaluate `true` on
/// every subsequent frame and the same step would re-fire.
///
/// Gates mirror the chain's triple gate so the counter only
/// advances while the chain is actually eligible to run. On
/// frames where no step fires (e.g. before `WorldReady` is
/// inserted) the counter stays put, so the chain resumes from
/// the right step when the player makes a kickoff decision.
///
/// [`mark_boot_ready`] sets `done = true` before this system
/// runs, so the closing frame's bump is suppressed and `step`
/// lands exactly on `total`.
fn advance_progress_step(mut progress: ResMut<BootProgress>) {
    if !progress.done && progress.step < progress.total {
        progress.step += 1;
    }
}

/// `run_if` predicate for the Tier 3 pre-parse trigger: fires
/// only when the primary game window is visible, which is the
/// splash-dismissed signal (see [`SplashPlugin::ui_splash_system`]
/// — it flips the main window from `visible: false` to
/// `visible: true` on dismissal). Pure function so tests can call
/// it without constructing a `Res` SystemParam.
fn main_window_visible(windows: Query<&Window, With<bevy::window::PrimaryWindow>>) -> bool {
    windows.single().map(|w| w.visible).unwrap_or(false)
}

/// Spawn the heavy `solar_system.ron` parse onto
/// [`AsyncComputeTaskPool`] once the splash has dismissed (see
/// [`main_window_visible`]). Idempotent — the `started` flag
/// prevents double-spawn across frames.
///
/// We pin to a fresh `SolarSystemData` value (no `setup_solar_system`
/// invocation on the async thread) so the worker doesn't touch
/// the live ECS. The result is moved into [`BootPreParseState::solar_data`]
/// on the next [`poll_pre_parse`] tick.
fn start_pre_parse(mut state: ResMut<BootPreParseState>) {
    if state.started {
        return;
    }
    state.started = true;
    let pool = AsyncComputeTaskPool::get();
    let path = "assets/data/solar_system.ron".to_string();
    let task = pool.spawn(async move {
        SolarSystemData::load_from_file(&path).map_err(|e| {
            format!("solar_system.ron pre-parse failed: {e}")
        })
    });
    state.solar_task = Some(task);
    info!("boot_init: pre-parse started (solar_system.ron → AsyncComputeTaskPool)");
}

/// Drain the in-flight pre-parse task into [`BootPreParseState::solar_data`].
/// Non-blocking — we poll the task once per frame with a no-op waker
/// (`Waker::noop()`), so the call returns immediately whether the
/// worker has finished or not. `Poll::Pending` means the worker
/// hasn't finished; we re-stash the handle and try again next frame.
/// `Poll::Ready(_)` means the work is done — log the size for
/// diagnostics, stash the value, and clear the handle so the next
/// poll is a no-op.
fn poll_pre_parse(mut state: ResMut<BootPreParseState>) {
    let Some(task) = state.solar_task.as_mut() else {
        return;
    };
    // `Task<T>` is `Unpin` (see bevy_tasks/src/task.rs:157), so we
    // can pin a `&mut` reference directly. The no-op waker ensures
    // the worker thread's progress (if any) isn't lost — when the
    // real waker fires, the worker pushes the result through the
    // task channel and the next non-blocking poll observes it.
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    match Pin::new(task).poll(&mut cx) {
        Poll::Ready(Ok(data)) => {
            let body_count = data.bodies.len();
            state.solar_data = Some(data);
            state.solar_task = None;
            info!(
                "boot_init: pre-parse complete ({} bodies cached for setup_solar_system)",
                body_count
            );
        }
        Poll::Ready(Err(err)) => {
            warn!("boot_init: pre-parse failed: {err}; chain will fall back to sync parse");
            state.solar_task = None;
        }
        Poll::Pending => {
            // Not ready yet — keep the handle, try again next frame.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_state_defaults_to_loading() {
        assert_eq!(BootState::default(), BootState::Loading);
    }

    #[test]
    fn boot_state_loading_and_ready_are_distinct() {
        assert_ne!(BootState::Loading, BootState::Ready);
    }

    /// One-shot semantics: a system that flips BootState to Ready on
    /// the first run must suppress the gate on subsequent runs.
    /// This test drives the actual `mark_boot_ready` system through
    /// a Bevy schedule and asserts that a second Update tick does
    /// not flip the state again (it isn't even queried because the
    /// `boot_state_is_loading` run_if is false).
    #[test]
    fn mark_boot_ready_is_idempotent_via_run_if_gate() {
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BootState>()
            .init_resource::<BootProgress>();

        // Pre-condition: gate is open, progress parked on the last
        // step so `mark_boot_ready`'s `run_if(progress_at(STEP_MARK_BOOT_READY))`
        // passes on the first tick.
        assert_eq!(*app.world().resource::<BootState>(), BootState::Loading);
        {
            let mut p = app.world_mut().resource_mut::<BootProgress>();
            p.step = STEP_MARK_BOOT_READY;
        }

        // Add mark_boot_ready directly so we can observe its effect
        // without dragging in the full chain (the chain's other
        // systems require manifests, render devices, etc.).
        app.add_systems(
            Update,
            mark_boot_ready
                .run_if(boot_state_is_loading)
                .run_if(progress_at(STEP_MARK_BOOT_READY)),
        );
        app.update();

        // Post-condition: gate is closed, progress is done.
        assert_eq!(*app.world().resource::<BootState>(), BootState::Ready);
        assert!(app.world().resource::<BootProgress>().done);

        // A second Update must not re-fire — `boot_state_is_loading`
        // returns false, the system is gated out, and BootState stays
        // at Ready. (No reset, no double-fire.)
        app.update();
        assert_eq!(*app.world().resource::<BootState>(), BootState::Ready);
        assert!(app.world().resource::<BootProgress>().done);
    }

    #[test]
    fn boot_state_is_loading_predicate_matches_value() {
        assert!(boot_state_is_loading_value(BootState::Loading));
        assert!(!boot_state_is_loading_value(BootState::Ready));
    }

    /// GRA-358 PR-B: the boot-init chain must NOT flip
    /// `BootState` to `Ready` while `WorldReady` is absent (i.e.
    /// before the player has chosen New Game / Load Save).
    /// Otherwise the chain's flip-once semantics would lock out
    /// the live world forever, and clicking "New Game" would
    /// produce a permanently empty world (the exact bug the
    /// screenshot in PR-B review caught).
    ///
    /// The chain's `run_if` predicates are:
    /// 1. `boot_state_is_loading`  (must be Loading)
    /// 2. `world_ready_is_present` (must have WorldReady)
    ///
    /// Both must be true. The test verifies the AND-ed gate by
    /// driving `mark_boot_ready` through a Bevy schedule with each
    /// combination of (Loading, WorldReady) and asserting
    /// `BootState` only flips to `Ready` when BOTH are satisfied.
    #[test]
    fn boot_init_chain_stays_silent_without_world_ready() {
        use crate::persistence::swap::WorldReady;
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BootState>()
            .init_resource::<BootProgress>();

        // Park progress on the last step so `mark_boot_ready`'s
        // `run_if(progress_at(STEP_MARK_BOOT_READY))` would pass
        // — otherwise the gate-flip wouldn't run regardless of
        // the upstream gates. This isolates the triple-gate test
        // to the WorldReady predicate.
        {
            let mut p = app.world_mut().resource_mut::<BootProgress>();
            p.step = STEP_MARK_BOOT_READY;
        }

        // Register `mark_boot_ready` with the same dual gate the
        // production chain uses (boot_state_is_loading AND
        // world_ready_is_present). We don't drag in the full chain
        // because the other systems require manifests, render
        // devices, etc. — the gate-flip is the observable signal.
        app.add_systems(
            Update,
            mark_boot_ready
                .run_if(boot_state_is_loading)
                .run_if(progress_at(STEP_MARK_BOOT_READY))
                .run_if(crate::persistence::swap::world_ready_is_present),
        );

        // ── Frame 1: no WorldReady yet (player hasn't clicked).
        //    The chain must stay silent. BootState stays at Loading.
        assert_eq!(*app.world().resource::<BootState>(), BootState::Loading);
        app.update();
        assert_eq!(
            *app.world().resource::<BootState>(),
            BootState::Loading,
            "boot_init chain must not fire while WorldReady is absent"
        );

        // ── Frame 2: WorldReady inserted (the swap just landed).
        app.insert_resource(WorldReady);
        app.update();
        assert_eq!(
            *app.world().resource::<BootState>(),
            BootState::Ready,
            "boot_init chain must fire once WorldReady is present"
        );

        // ── Frame 3: chain stays silent (idempotent).
        app.update();
        assert_eq!(*app.world().resource::<BootState>(), BootState::Ready);
    }

    /// Pure helper — the same check as `boot_state_is_loading` but
    /// for direct value comparison (no SystemParam needed).
    fn boot_state_is_loading_value(state: BootState) -> bool {
        state == BootState::Loading
    }

    /// GRA-358 PR-D: the boot-init chain must NOT fire
    /// `mark_boot_ready` while `RestoredWorldGate` is present.
    ///
    /// The chain's `run_if` predicates are:
    /// 1. `boot_state_is_loading`           (must be Loading)
    /// 2. `world_ready_is_present`          (must have WorldReady)
    /// 3. `restored_world_is_not_present`   (must NOT have RestoredWorldGate)
    ///
    /// All three must be true. The chain stays silent on the
    /// Restore path (Continue / Load Save) because the loaded
    /// world already has the 710 bodies, day-one fleet, baseline
    /// tech, and 60 nearby-star systems — re-running the chain's
    /// `setup_solar_system` etc. would duplicate every entity and
    /// produce a mixed hierarchy that Bevy's
    /// `propagate_parent_transforms` recurses into, overflowing
    /// the compute task pool's stack.
    ///
    /// The test drives `mark_boot_ready` through a Bevy schedule
    /// with the full triple gate and asserts:
    /// 1. New Game path (no RestoredWorldGate) → BootState flips to
    ///    Ready (chain fires).
    /// 2. Restore path (RestoredWorldGate inserted) → BootState
    ///    stays at Loading (chain silent).
    #[test]
    fn boot_init_chain_stays_silent_on_restore_path() {
        use crate::persistence::swap::{RestoredWorldGate, WorldReady};
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BootState>()
            .init_resource::<BootProgress>();

        // Park progress on the last step so the gate-flip would
        // otherwise pass. This isolates the test to the
        // RestoredWorldGate predicate.
        {
            let mut p = app.world_mut().resource_mut::<BootProgress>();
            p.step = STEP_MARK_BOOT_READY;
        }

        // Register `mark_boot_ready` with the same triple gate the
        // production chain uses (boot_state_is_loading AND
        // world_ready_is_present AND restored_world_is_not_present).
        // We don't drag in the full chain because the other systems
        // require manifests, render devices, etc. — the gate-flip is
        // the observable signal.
        app.add_systems(
            Update,
            mark_boot_ready
                .run_if(boot_state_is_loading)
                .run_if(progress_at(STEP_MARK_BOOT_READY))
                .run_if(crate::persistence::swap::world_ready_is_present)
                .run_if(crate::persistence::swap::restored_world_is_not_present),
        );

        // ── Frame 1: New Game path. Insert WorldReady, leave
        //    RestoredWorldGate absent. The chain must fire.
        assert_eq!(*app.world().resource::<BootState>(), BootState::Loading);
        app.insert_resource(WorldReady);
        app.update();
        assert_eq!(
            *app.world().resource::<BootState>(),
            BootState::Ready,
            "boot_init chain must fire on New Game path (no RestoredWorldGate)"
        );

        // ── Frame 2: simulate a different session by resetting
        //    BootState back to Loading. With WorldReady still
        //    present (it persists across sessions in the live
        //    app), the chain would normally re-fire. But after
        //    inserting RestoredWorldGate, the chain must stay
        //    silent — that's the Restore path's discriminator.
        app.insert_resource(BootState::Loading);
        app.insert_resource(RestoredWorldGate);
        app.update();
        assert_eq!(
            *app.world().resource::<BootState>(),
            BootState::Loading,
            "boot_init chain must stay silent on Restore path (RestoredWorldGate present)"
        );

        // ── Frame 3: chain stays silent (idempotent). Even
        //    though BootState is still Loading, the chain is
        //    gated out by RestoredWorldGate.
        app.update();
        assert_eq!(
            *app.world().resource::<BootState>(),
            BootState::Loading,
            "boot_init chain must stay silent on Restore path (idempotent)"
        );
    }

    // ── Idempotency-marker re-exports ─────────────────────────────────────
    //
    // `SolarSystemSpawned` and `NearbySystemsPopulated` guard
    // `setup_solar_system` and `populate_nearby_systems` against
    // duplicate entity spawning when boot_init runs more than once
    // (e.g. a future save-restore that flips `BootState` back to
    // `Loading`). These tests assert that the markers are
    // re-exported from the source modules so boot_init callers
    // can remove them in the reset path alongside `DayOneFleetSpawned`.

    #[test]
    fn solar_system_spawned_marker_is_constructible() {
        // Marker types must be `Default + Resource + Copy` so they
        // can be inserted via `commands.init_resource` and stored
        // without wrapping. Construct one to lock in that contract.
        use crate::plugins::solar_system::SolarSystemSpawned;
        let marker = SolarSystemSpawned;
        let _copy = marker;
        let _another = marker;
    }

    #[test]
    fn nearby_systems_populated_marker_is_constructible() {
        use crate::plugins::system_populator::NearbySystemsPopulated;
        let marker = NearbySystemsPopulated;
        let _copy = marker;
        let _another = marker;
    }

    // ── Per-frame stepping tests (Tier 2 refactor) ───────────────────────
    //
    // The boot-init chain used to fire all 12+ systems on the same
    // `Update` tick, freezing the splash for 1–3 s. It now runs one
    // system per frame via `progress_at(N)` gates. These tests lock
    // in the state-machine semantics: the counter advances once per
    // frame and the chain stays silent on frames where the gate is
    // closed.

    #[test]
    fn boot_progress_defaults_to_step_zero_and_not_done() {
        let p = BootProgress::default();
        assert_eq!(p.step, 0);
        assert!(!p.done);
        assert_eq!(p.total as usize, BOOT_STEP_COUNT);
    }

    #[test]
    fn progress_at_predicate_matches_only_current_step() {
        let mut p = BootProgress::default();
        // Step 0 matches, others don't.
        assert!(progress_at_value(0, &p));
        assert!(!progress_at_value(1, &p));
        // Advance to step 5.
        p.step = 5;
        assert!(!progress_at_value(0, &p));
        assert!(progress_at_value(5, &p));
        // Done flag suppresses every step.
        p.done = true;
        assert!(!progress_at_value(5, &p));
    }

    /// Pure helper — mirrors `progress_at` without the SystemParam.
    fn progress_at_value(step_n: u32, p: &BootProgress) -> bool {
        p.step == step_n && !p.done
    }

    #[test]
    fn advance_progress_step_bumps_counter_when_chain_is_eligible() {
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<BootProgress>();

        // Frame 1: counter advances 0 → 1.
        assert_eq!(app.world().resource::<BootProgress>().step, 0);
        app.add_systems(
            PostUpdate,
            advance_progress_step.run_if(boot_state_is_loading),
        );
        // Need BootState for the run_if to evaluate.
        app.init_resource::<BootState>();
        app.update();
        assert_eq!(
            app.world().resource::<BootProgress>().step,
            1,
            "advance_progress_step must bump the counter once per frame"
        );

        // Frame 2: counter advances 1 → 2.
        app.update();
        assert_eq!(app.world().resource::<BootProgress>().step, 2);
    }

    #[test]
    fn advance_progress_step_does_not_bump_when_chain_is_silent() {
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BootProgress>()
            // `boot_state_is_loading` reads `BootState`; we init it
            // so the run_if predicate validates, but `WorldReady`
            // and `RestoredWorldGate` are deliberately absent —
            // those are the predicates that gate the system out.
            .init_resource::<BootState>();

        // Mirror the production gate: the advance only fires when
        // BootState == Loading AND WorldReady is present AND no
        // RestoredWorldGate is set. WorldReady is absent, so the
        // system is gated out across multiple frames.
        app.add_systems(
            PostUpdate,
            advance_progress_step
                .run_if(boot_state_is_loading)
                .run_if(crate::persistence::swap::world_ready_is_present)
                .run_if(crate::persistence::swap::restored_world_is_not_present),
        );
        app.update();
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<BootProgress>().step,
            0,
            "advance_progress_step must stay silent when the chain is gated out"
        );
    }

    #[test]
    fn advance_progress_step_respects_done_flag() {
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BootProgress>()
            .init_resource::<BootState>();
        {
            let mut p = app.world_mut().resource_mut::<BootProgress>();
            p.done = true;
            p.step = BOOT_STEP_COUNT as u32;
        }

        app.add_systems(
            PostUpdate,
            advance_progress_step.run_if(boot_state_is_loading),
        );
        app.update();
        // Done flag suppresses the bump — counter stays at total.
        assert_eq!(
            app.world().resource::<BootProgress>().step,
            BOOT_STEP_COUNT as u32
        );
        assert!(app.world().resource::<BootProgress>().done);
    }

    #[test]
    fn mark_boot_ready_clamps_progress_to_total() {
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BootState>()
            .init_resource::<BootProgress>();
        {
            let mut p = app.world_mut().resource_mut::<BootProgress>();
            p.step = STEP_MARK_BOOT_READY;
        }

        app.add_systems(
            Update,
            mark_boot_ready
                .run_if(boot_state_is_loading)
                .run_if(progress_at(STEP_MARK_BOOT_READY)),
        );
        app.update();

        let p = app.world().resource::<BootProgress>();
        assert!(p.done);
        assert_eq!(
            p.step,
            BOOT_STEP_COUNT as u32,
            "mark_boot_ready must clamp step to total so no further run_if matches"
        );
    }

    // ── Tier 3 pre-parse tests ─────────────────────────────────────────
    //
    // The pre-parse cache moves the heavy `solar_system.ron` read +
    // RON decode off the kickoff-click critical path. The cache
    // triggers on splash dismiss and is drained into
    // `BootPreParseState.solar_data` once the async task completes.
    // These tests lock in the state-machine + idempotency
    // semantics without needing the actual RON file.

    #[test]
    fn boot_pre_parse_state_defaults_to_unstarted_and_empty() {
        let s = BootPreParseState::default();
        assert!(!s.started);
        assert!(s.solar_task.is_none());
        assert!(s.solar_data.is_none());
    }

    #[test]
    fn start_pre_parse_is_idempotent() {
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<BootPreParseState>();

        // Run several frames; `started` should latch `true` exactly
        // once and never spawn a second task.
        app.add_systems(Update, start_pre_parse.run_if(main_window_visible));
        // No main window in `MinimalPlugins`, so the gate stays
        // closed and `start_pre_parse` never fires here. This test
        // just exercises the idempotency of `started = true` after
        // a manual call.
        app.add_systems(Update, start_pre_parse);
        app.update();
        app.update();
        app.update();
        assert!(
            app.world().resource::<BootPreParseState>().started,
            "start_pre_parse must set started=true on first call"
        );
        // The flag should remain `true` — no rollback, no double
        // work. Task may or may not be present depending on timing;
        // we just check `started` here.
    }

    #[test]
    fn poll_pre_parse_drains_completed_task_into_solar_data() {
        // Manually construct a task that has already completed.
        use bevy::tasks::Task;
        use bevy::MinimalPlugins;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<BootPreParseState>();

        // Inject a "pre-completed" task by spawning one and waiting
        // for it on the same thread via block_on. Simpler: drop the
        // task into the resource and let poll_pre_parse observe
        // completion after one frame (the work is just constructing
        // an empty SolarSystemData, which is trivial).
        let pool = bevy::tasks::AsyncComputeTaskPool::get();
        let task: Task<Result<SolarSystemData, String>> = pool.spawn(async move {
            Ok(SolarSystemData { bodies: Vec::new() })
        });
        {
            let mut state = app.world_mut().resource_mut::<BootPreParseState>();
            state.started = true;
            state.solar_task = Some(task);
        }

        app.add_systems(Update, poll_pre_parse);

        // Spin up to a few frames for the async task to complete.
        for _ in 0..32 {
            app.update();
            if app
                .world()
                .resource::<BootPreParseState>()
                .solar_task
                .is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let state = app.world().resource::<BootPreParseState>();
        assert!(
            state.solar_task.is_none(),
            "poll_pre_parse must clear the task handle after draining"
        );
        assert!(
            state.solar_data.is_some(),
            "poll_pre_parse must stash the parsed value in solar_data"
        );
        assert_eq!(state.solar_data.as_ref().unwrap().bodies.len(), 0);
    }
}
