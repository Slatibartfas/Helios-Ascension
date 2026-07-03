//! Save / Load plugin — Bevy world-state persistence for Helios Ascension.
//!
//! GRA-314 PR-A. PR-A ships:
//!
//! - [`PersistencePlugin`] — registers [`AppTypeRegistry`] (PR-A safety net,
//!   production plugin registrations in PR-B/C/D add their own component
//!   registrations via `app.register_type::<T>()`).
//! - [`snapshot::snapshot_world`] — serialise the live world to a RON string.
//! - [`restore::restore_world`] — deserialise a RON string into a fresh
//!   [`World`].
//! - [`migrate::run_migrations`] — version-aware forward migrator chain.
//! - [`format_version::FORMAT_VERSION`] — `1` in PR-A.
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
//! PR-A does NOT touch the disk — the save panel in PR-B will add the
//! `write-to-tmp-then-rename` pattern. PR-A only produces a RON string.
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

pub mod format_version;
pub mod migrate;
pub mod restore;
pub mod snapshot;

pub use format_version::{FORMAT_VERSION, MIN_SUPPORTED_VERSION};
pub use migrate::{Body, MigrateError, SchemaKind};
pub use restore::{restore_world, RestoreError, RestoredWorld};
pub use snapshot::{
    snapshot_world, snapshot_world_with_registry, SaveFile, SaveMetadata, SnapshotError,
};

/// Plugin that wires save/load into Bevy.
///
/// PR-A registers [`AppTypeRegistry`] if no other plugin has done so —
/// the snapshot/restore helpers cannot function without it. Production
/// plugins (Persistence itself included in later PRs) register their
/// own component types via `app.register_type::<T>()`.
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
}
