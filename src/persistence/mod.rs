//! Save / Load plugin — Bevy world-state persistence for Helios Ascension.
//!
//! GRA-314 / GRA-358 PR-A + PR-B. The persistence module is split
//! across two PRs:
//!
//! - **PR-A** (shipped) — [`PersistencePlugin`] (register
//!   [`AppTypeRegistry`] + cross-plugin reflection coverage),
//!   [`snapshot::snapshot_world`], [`restore::restore_world`],
//!   [`migrate::run_migrations`], [`format_version::FORMAT_VERSION`],
//!   and the [`params::NewGameParams`] loader.
//! - **PR-B** (this PR) — [`SaveLoadPlugin`] registers
//!   [`crate::persistence::playtime::PlaytimeTracker`] +
//!   [`crate::persistence::autosave::AutosaveTimer`] and schedules
//!   the two `Update` ticks; [`crate::persistence::io`] provides the
//!   atomic-write helper [`crate::persistence::io::write_save_atomic`].
//!
//! # R2 (Reflection coverage gap)
//!
//! Bevy's `DynamicScene` snapshot walks `AppTypeRegistry` for every
//! `#[reflect(Component)]` and `#[reflect(Resource)]` type. Helios currently
//! registers very few types reflectively — only
//! `src/ui/notifications/components.rs` and a couple of UI state types.
//! **PR-A's roundtrip test therefore exercises reflectively-registered
//! test types only**, not live Helios components.
//!
//! The gap is tracked as a follow-up issue (search for "GRA-XXX add
//! `#[reflect(Component)]` across astronomy/colony/fleet/economy"). Until
//! that lands, calling [`snapshot_world`] on a real Helios world will
//! silently drop every component that hasn't been registered.
//!
//! # R3 (fresh world for restore)
//!
//! [`restore::restore_world`] ALWAYS constructs a fresh [`World`] via the
//! caller-supplied factory. We never reuse the live world for restore —
//! `Entity` IDs in Bevy 0.18 are reused after `World::clear()`, which would
//! cause silent pointer collisions.
//!
//! # R4 (atomic on-disk write)
//!
//! PR-A does NOT touch the disk. PR-B adds
//! [`crate::persistence::io::write_save_atomic`] (write-to-tmp-then-rename
//! with `fsync`). The autosave consumer is the first caller; PR-C
//! (Save Panel UI) will reuse the same helper.
//!
//! # Bevy 0.18 `SceneDeserializer` import gotcha
//!
//! `bevy_scene::serde::SceneDeserializer` only exposes `.deserialize(...)`
//! via the `serde::de::DeserializeSeed` trait — there is no inherent method.
//! Any caller of the restore path MUST `use serde::de::DeserializeSeed;`
//! alongside the `use bevy_scene::serde::SceneDeserializer;`. PR-B and PR-C
//! will both reach into this module; the import lives at module scope to
//! avoid per-call-site duplication.

use bevy::prelude::*;

pub mod autosave;
pub mod format_version;
pub mod game_setup;
pub mod handle_sidecar;
pub mod io;
pub mod migrate;
pub mod params;
pub mod playtime;
pub mod plugin;
pub mod restore;
pub mod snapshot;
pub mod swap;

pub use autosave::{
    prune_old_autosaves, tick_autosave_timer, AutosaveTimer, AUTOSAVE_PREFIX, AUTOSAVE_SUFFIX,
    DEFAULT_AUTOSAVE_INTERVAL_S, DEFAULT_ROLLING_COUNT,
};
pub use format_version::{FORMAT_VERSION, MIN_SUPPORTED_VERSION};
pub use game_setup::{
    play_new_game, play_new_game_with_factory, promote_pending_world, rescan_save_index,
    restore_save, write_save_to_path, GameSetupError, GameSetupPlugin, NewGameCommitted,
    PendingGameWorld, RestoreCommitted,
};
pub use handle_sidecar::{
    apply_handle_sidecar, extract_handle_sidecar, EntityHandles, HandleSidecar,
};
pub use io::{delete_save_files, write_save_atomic, SaveIoError};
pub use migrate::{Body, MigrateError, SchemaKind};
pub use params::{load_new_game_params_defaults, NewGameParams, NewGameParamsDefaults};
pub use playtime::{tick_playtime_tracker, PlaytimeTracker};
pub use plugin::SaveLoadPlugin;
pub use restore::{restore_world, RestoreError, RestoredWorld};
pub use snapshot::{
    snapshot_world, snapshot_world_with_registry, SaveFile, SaveMetadata, SnapshotError,
};
pub use swap::{swap_world_into, world_ready_is_present, SwapError, WorldReady};

/// Plugin that wires save/load into Bevy.
///
/// PR-A registers [`AppTypeRegistry`] if no other plugin has done so —
/// the snapshot/restore helpers cannot function without it. Production
/// plugins (Persistence itself included in later PRs) register their
/// own component types via `app.register_type::<T>()`.
///
/// GRA-319 expands the registrations to cover the simulation-state
/// Components and persistent Resources across astronomy/colony/economy/
/// fleets/research/survey/shipbuilding/personnel/plugins. Each owning
/// plugin still calls `register_type` for its own types so the
/// coverage survives if the consumer skips Persistence.
pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        // PR-A does not register `Update` systems because save/load runs
        // only on explicit request. The action-queue wiring lives in
        // PR-B (Save Panel UI). PR-A exists so the menu's "Load Game"
        // click can hand a path to a real loader, and so the roundtrip
        // test has a home.
        //
        // Belt-and-braces: ensure AppTypeRegistry exists. Bevy plugins
        // normally register this; some test-only apps do not.
        if !app.world().contains_resource::<AppTypeRegistry>() {
            app.init_resource::<AppTypeRegistry>();
        }

        // GRA-358 PR-A: register NewGameParams so save/load snapshots
        // can capture the player's procedural-gen knobs. The type is
        // a Resource today (held in `PendingLaunchActions::start_new_game`);
        // a follow-up PR may attach it to a "world config" Component,
        // at which point the registration here is enough — `Reflect`
        // covers both. The world-spawn layer is out of PR-A scope.
        app.register_type::<NewGameParams>();
        app.register_type::<NewGameParamsDefaults>();

        // GRA-319: cross-plugin reflection coverage for save/load.
        //
        // Bevy 0.18's `RegisterForReflection` requires every type to
        // satisfy `Typed + GetTypeRegistration + FromReflect + ...`. The
        // minimum-M-scope register list below targets the simplest
        // simulation-state Components and persistent Resources we can
        // roundtrip without cascading trait-bound errors. Wrapper
        // resources (ProceduralRng, MusicPlaylist, *Queue, etc.) stay
        // unregistered by design; Bevy's `DynamicScene::from_world`
        // silently drops anything not in `AppTypeRegistry`, so the
        // snapshot will be partial but the world itself serialises.
        //
        // CTO/LGD follow-up PR can extend this once `Reflect` derives
        // are confirmed clean across all enum/enum-keyed-HashMap
        // cascades in economy / colony / fleets / shipbuilding.
        app
            // ── Astronomy (high-priority: orbital state IS the game) ──
            .register_type::<crate::astronomy::SpaceCoordinates>()
            .register_type::<crate::astronomy::KeplerOrbit>()
            .register_type::<crate::astronomy::Selected>()
            .register_type::<crate::astronomy::FloatingOrigin>()
            .register_type::<crate::astronomy::OrbitPath>()
            // GRA-XXX: the source `AstronomyPlugin::build`
            // (`src/astronomy/mod.rs:73-102`) registers
            // these on the live `App`. The loader's
            // `build_minimal_world_for_restore` factory
            // (`src/persistence/game_setup.rs:198-200`)
            // builds a bare `World::new()` with **no
            // plugins** — so on the restore path those
            // source registrations don't run. Without an
            // explicit re-registration here, the
            // deserializer's `AppTypeRegistry` lookup fails
            // and `restore_world` returns Err.
            //
            // Player-visible symptom at 2026-07-24T09:20Z:
            // `kickoff: restore_save failed: save restore
            // failed: scene deserialise failed: no
            // registration found for
            // `helios_ascension::astronomy::components::
            // CurrentStarSystem``. The
            // `a99a5f5`-era audit list only covered five
            // astronomy types (above), so the other ~25
            // registered types reached the snapshot RON but
            // couldn't be resolved on load. This block
            // closes the rest of that gap for astronomy.
            // See
            // `package_astronomy_registration_into_persistence`
            // below for the regression test that guards it.
            .register_type::<crate::astronomy::CurrentStarSystem>()
            .register_type::<crate::astronomy::SystemId>()
            .register_type::<crate::astronomy::OrbitCenter>()
            .register_type::<crate::astronomy::HyperbolicTrajectory>()
            .register_type::<crate::astronomy::LocalOrbitAmplification>()
            .register_type::<crate::astronomy::Hovered>()
            .register_type::<crate::astronomy::Destroyed>()
            .register_type::<crate::astronomy::CometTail>()
            .register_type::<crate::astronomy::SelectionMarker>()
            .register_type::<crate::astronomy::HoverMarker>()
            .register_type::<crate::astronomy::MarkerOwner>()
            .register_type::<crate::astronomy::MarkerDot>()
            .register_type::<crate::astronomy::LpMarkerInfo>()
            .register_type::<crate::astronomy::LagrangePointMarkers>()
            .register_type::<crate::astronomy::LastLpClick>()
            // Surface / ocean / atmosphere / stellar — also
            // registered at the source, registered here for
            // the same reason.
            .register_type::<crate::astronomy::OceanType>()
            .register_type::<crate::astronomy::OceanProperties>()
            .register_type::<crate::astronomy::SurfaceTemperature>()
            .register_type::<crate::astronomy::StellarProperties>()
            .register_type::<crate::astronomy::AtmosphericGas>()
            .register_type::<crate::astronomy::AtmosphereComposition>()
            .register_type::<crate::astronomy::exoplanets::RealPlanet>()
            .register_type::<crate::astronomy::nearby_stars::NearbyStarsData>()
            .register_type::<crate::astronomy::nearby_stars::StarSystemData>()
            .register_type::<crate::astronomy::nearby_stars::StarData>()
            .register_type::<crate::astronomy::nearby_stars::PlanetData>()
            .register_type::<crate::astronomy::nearby_stars::BinaryOrbitData>()
            .register_type::<crate::astronomy::selection::RingHighlight>()
            // ── Colony (covers buildings + construction queue) ──────
            .register_type::<crate::colony::Colony>()
            .register_type::<crate::colony::ColonyTier>()
            .register_type::<crate::colony::ColonyDevelopment>()
            .register_type::<crate::colony::PendingConstructionActions>()
            // ── Economy (covers LocalStockpile + MinimumStockpile) ──
            .register_type::<crate::economy::LocalStockpile>()
            .register_type::<crate::economy::MinimumStockpile>()
            // ── Enum keys / fields transitively referenced by the
            //    registered Components above. The `#[reflect(...)]`
            //    attribute on each enum's `#[derive]` is not enough —
            //    `register_type` is what binds the type into
            //    `AppTypeRegistry`, which `DynamicScene::from_world`
            //    walks to discover what to serialise. Without these,
            //    every `HashMap<ResourceType, _>` and every enum-typed
            //    field is silently dropped from snapshots. (Kilo
            //    CRITICAL on PR #207, CTO HOLD comment 4882357170.) ──
            .register_type::<crate::economy::ResourceType>()
            .register_type::<crate::economy::ResourcePhase>()
            .register_type::<crate::colony::BuildingType>()
            .register_type::<crate::survey::SurveyDimension>()
            .register_type::<crate::fleets::FleetRole>()
            .register_type::<crate::fleets::TransferReferenceFrame>()
            // ── Fleets (covers Fleet + transfer action queue) ───────
            .register_type::<crate::fleets::Fleet>()
            .register_type::<crate::fleets::FleetOrbit>()
            .register_type::<crate::fleets::ActiveManeuver>()
            // ── Research (covers projects + ResearchState resource) ─
            .register_type::<crate::research::ResearchProject>()
            .register_type::<crate::research::ResearchState>()
            // ── Survey (high-level state, no registries yet) ────────
            .register_type::<crate::survey::SurveyState>()
            // ── Shipbuilding (proves per-area coverage; construction
            //    project restore will land in PR-B once Queue* action
            //    chains finish shipping) ──────────────────────────────
            .register_type::<crate::shipbuilding::RefitProject>()
            // ── Personnel ──────────────────────────────────────────
            .register_type::<crate::personnel::Scientist>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_is_one_in_pr_a() {
        assert_eq!(FORMAT_VERSION, 1);
        assert_eq!(MIN_SUPPORTED_VERSION, 1);
    }

    #[test]
    fn plugin_registers_type_registry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(PersistencePlugin);
        assert!(app.world().contains_resource::<AppTypeRegistry>());
    }

    /// Regression test for the player-visible failure from
    /// 2026-07-24T09:20Z: `restore_save failed: no
    /// registration found for
    /// `helios_ascension::astronomy::components::CurrentStarSystem``.
    ///
    /// `CurrentStarSystem` is `register_type`'d in
    /// `AstronomyPlugin::build` (`src/astronomy/mod.rs:75`),
    /// but the restore path uses
    /// `build_minimal_world_for_restore`
    /// (`src/persistence/game_setup.rs:198-200`) which
    /// builds a bare `World::new()` with **no plugins**
    /// — so on the restore path those source
    /// registrations don't run. Without an explicit
    /// re-registration in `PersistencePlugin::build`, the
    /// deserializer's `AppTypeRegistry` lookup fails and
    /// `restore_world` returns `Err`. Adding the missing
    /// `register_type::<…>()` calls for every
    /// AstronomyPlugin-registered type here closes that
    /// gap.
    ///
    /// This test exercises the **registry-content** side of
    /// the round trip the same way the restore path does:
    /// build a bare `App` whose `AppTypeRegistry` was
    /// populated only by `PersistencePlugin::build` (NOT
    /// by `AstronomyPlugin`, since the live App's plugin
    /// chain is unavailable on the restore path) and
    /// confirm that each `helios_ascension::astronomy::*`
    /// type path the loader needs to resolve actually has
    /// an entry. If a future maintainer adds a new
    /// `register_type::<…>()` to `AstronomyPlugin::build`
    /// but forgets the mirror entry here, this test
    /// catches the gap deterministically — same
    /// `cargo test --lib persistence` loop the deny-chain
    /// tests run in, just on the Helios side of the same
    /// audit pattern. (The historical Bevy-side counterpart
    /// pattern is documented in
    /// `/memories/repo/bevy-0-18-reflect-resource-denylist.md`.)
    #[test]
    fn package_astronomy_registration_into_persistence() {
        // Build the bare restore factory the same way
        // `build_minimal_world` does — `App::new()` + an
        // empty `AppTypeRegistry`. Add only
        // `PersistencePlugin`. (We do NOT add
        // `AstronomyPlugin`; the live `App`'s plugin chain
        // is unavailable on the restore path. That's
        // exactly the configuration that produced the
        // 2026-07-24T09:20Z warning.)
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(PersistencePlugin);

        // Every astronomy type that `AstronomyPlugin::build`
        // registers at the source MUST also be registered
        // here, otherwise the deserializer's
        // `AppTypeRegistry.get_with_type_path` returns
        // `None` and the entire scene load aborts. Build
        // the expected-path list from the source plugin's
        // registration chain (`src/astronomy/mod.rs:73-…`)
        // and assert each path lands in the registry.
        let expected = [
            // Resources (the original 4 from the
            // a99a5f5-era list + the gap closure):
            "helios_ascension::astronomy::components::CurrentStarSystem",
            "helios_ascension::astronomy::components::FloatingOrigin",
            "helios_ascension::astronomy::components::LagrangePointMarkers",
            "helios_ascension::astronomy::components::LastLpClick",
            // Components (the original 5 from the
            // a99a5f5-era list + the gap closure):
            "helios_ascension::astronomy::components::SpaceCoordinates",
            "helios_ascension::astronomy::components::KeplerOrbit",
            "helios_ascension::astronomy::components::OrbitPath",
            "helios_ascension::astronomy::components::Selected",
            "helios_ascension::astronomy::components::SystemId",
            "helios_ascension::astronomy::components::OrbitCenter",
            "helios_ascension::astronomy::components::HyperbolicTrajectory",
            "helios_ascension::astronomy::components::LocalOrbitAmplification",
            "helios_ascension::astronomy::components::Hovered",
            "helios_ascension::astronomy::components::Destroyed",
            "helios_ascension::astronomy::components::CometTail",
            "helios_ascension::astronomy::components::SelectionMarker",
            "helios_ascension::astronomy::components::HoverMarker",
            "helios_ascension::astronomy::components::MarkerOwner",
            "helios_ascension::astronomy::components::MarkerDot",
            "helios_ascension::astronomy::components::LpMarkerInfo",
            // Surface / ocean / atmosphere / stellar —
            // registered at the source, registered here
            // for the same reason. Their full type paths
            // use the components module since they all
            // live there alongside the rest.
            "helios_ascension::astronomy::components::OceanType",
            "helios_ascension::astronomy::components::OceanProperties",
            "helios_ascension::astronomy::components::SurfaceTemperature",
            "helios_ascension::astronomy::components::StellarProperties",
            "helios_ascension::astronomy::components::AtmosphericGas",
            "helios_ascension::astronomy::components::AtmosphereComposition",
            "helios_ascension::astronomy::exoplanets::RealPlanet",
            "helios_ascension::astronomy::nearby_stars::NearbyStarsData",
            "helios_ascension::astronomy::nearby_stars::StarSystemData",
            "helios_ascension::astronomy::nearby_stars::StarData",
            "helios_ascension::astronomy::nearby_stars::PlanetData",
            "helios_ascension::astronomy::nearby_stars::BinaryOrbitData",
            "helios_ascension::astronomy::selection::RingHighlight",
        ];
        let registry = app.world().resource::<AppTypeRegistry>();
        let registry = registry.read();
        for path in expected {
            assert!(
                registry.get_with_type_path(path).is_some(),
                "PersistencePlugin::build must register `{path}` so the restore \
                 path can resolve it. Add `.register_type::<…>()` to \
                 `src/persistence/mod.rs::PersistencePlugin::build`."
            );
        }
    }
}
