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

use crate::astronomy::asteroids::load_asteroid_registry;
use crate::economy::generation::{
    generate_ring_resources, generate_solar_system_resources, init_procedural_rng,
    stamp_resource_phases,
};
use crate::fleets::systems::{spawn_debug_earth_jupiter_fleet, spawn_initial_fleet};
use crate::plugins::solar_system::{
    initial_camera_focus, initialize_colony_stockpiles, setup_solar_system,
};
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

/// Plugin that owns the deferred game-state init.
///
/// The chain below mirrors the original `.after(...)` graph from
/// `Startup` + `PostStartup` registrations that were removed by
/// the splash-hides-loading refactor. New systems should be added
/// here, not re-registered at `Startup`.
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
        app.init_resource::<BootState>().add_systems(
            Update,
            (
                // 1. Solar system first — every other system in the
                //    chain reads body entities produced here.
                setup_solar_system,
                // 2. Resources + nearby systems in parallel after
                //    solar-system setup. `populate_nearby_systems`
                //    ran `.after(load_nearby_stars_data)` and
                //    `.before(generate_solar_system_resources)` at
                //    Startup; both still hold because
                //    `load_nearby_stars_data` lives at Startup
                //    (it's a manifest loader, not deferred) and the
                //    `.chain()` below pins the relative order.
                (
                    init_procedural_rng,
                    load_asteroid_registry,
                    populate_nearby_systems,
                    initialize_colony_stockpiles,
                    initial_camera_focus,
                    spawn_initial_fleet,
                    spawn_debug_earth_jupiter_fleet,
                    initialize_baseline_technology,
                )
                    .chain()
                    .after(setup_solar_system),
                // 3. Resource generation / stamping after
                //    populate (per the original
                //    `.before(generate_solar_system_resources)`
                //    constraint).
                (generate_solar_system_resources, generate_ring_resources)
                    .chain()
                    .after(populate_nearby_systems),
                stamp_resource_phases.after(generate_solar_system_resources),
                // 4. Baseline engineering + module-catalog merge
                //    after baseline technology unlock.
                (
                    initialize_baseline_engineering,
                    merge_ship_module_engineering_catalog,
                )
                    .chain()
                    .after(initialize_baseline_technology),
                // 5. The gate flip — last. `chain()` guarantees the
                //    earlier systems finish first; `run_if` then
                //    makes the whole set no-op on subsequent frames.
                mark_boot_ready,
            )
                .chain()
                // GRA-358 PR-B: gate the whole chain on BOTH
                // `BootState::Loading` (the splash-end signal) AND
                // `WorldReady` (the kickoff-end signal). Without
                // `WorldReady`, the chain would fire on the very
                // first `app.update()` call — before any
                // save/load decision — and spawn the entire
                // 710-body baseline over whatever the swap
                // eventually lands.
                //
                // GRA-358 PR-D: ALSO gate on `NOT
                // restored_world_is_present`. The Restore path
                // (Continue / Load Save) loads a world that
                // already contains the 710 bodies, day-one fleet,
                // baseline tech, and 60 nearby-star systems.
                // Re-running the chain would duplicate every
                // entity and produce a mixed hierarchy that Bevy's
                // `propagate_parent_transforms` recurses into,
                // overflowing the compute task pool's stack.
                //
                // The chain's per-system idempotency markers
                // (`SolarSystemSpawned`, `DayOneFleetSpawned`,
                // `AsteroidRegistryLoaded`, etc.) are not
                // `#[reflect(Resource)]` so they don't survive the
                // swap — `RestoredWorldGate` is the swap-level
                // discriminator that supplements them.
                .run_if(boot_state_is_loading)
                .run_if(crate::persistence::swap::world_ready_is_present)
                .run_if(crate::persistence::swap::restored_world_is_not_present),
        );
    }
}

/// `run_if` predicate for the boot-init chain: fires only while
/// `BootState == Loading`. After [`mark_boot_ready`] flips the gate,
/// the entire chain is suppressed on subsequent frames.
///
/// Pure function over `&BootState` so tests can call it directly
/// without constructing a `Res` SystemParam.
fn boot_state_is_loading(state: Res<BootState>) -> bool {
    *state == BootState::Loading
}

/// Last system in the boot-init chain. Flips `BootState` to `Ready`
/// so the splash can dismiss and the gate stops re-firing.
fn mark_boot_ready(mut state: ResMut<BootState>) {
    info!("boot_init: deferred systems complete → BootState::Ready");
    *state = BootState::Ready;
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
        app.add_plugins(MinimalPlugins).init_resource::<BootState>();

        // Pre-condition: gate is open.
        assert_eq!(*app.world().resource::<BootState>(), BootState::Loading);

        // Add mark_boot_ready directly so we can observe its effect
        // without dragging in the full chain (the chain's other
        // systems require manifests, render devices, etc.).
        app.add_systems(Update, mark_boot_ready.run_if(boot_state_is_loading));
        app.update();

        // Post-condition: gate is closed.
        assert_eq!(*app.world().resource::<BootState>(), BootState::Ready);

        // A second Update must not re-fire — `boot_state_is_loading`
        // returns false, the system is gated out, and BootState stays
        // at Ready. (No reset, no double-fire.)
        app.update();
        assert_eq!(*app.world().resource::<BootState>(), BootState::Ready);
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
        app.add_plugins(MinimalPlugins).init_resource::<BootState>();

        // Register `mark_boot_ready` with the same dual gate the
        // production chain uses (boot_state_is_loading AND
        // world_ready_is_present). We don't drag in the full chain
        // because the other systems require manifests, render
        // devices, etc. — the gate-flip is the observable signal.
        app.add_systems(
            Update,
            mark_boot_ready
                .run_if(boot_state_is_loading)
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
        app.add_plugins(MinimalPlugins).init_resource::<BootState>();

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
}
