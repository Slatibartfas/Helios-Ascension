//! Snapshot the live Bevy [`World`] into a RON string.
//!
//! PR-A ships a single [`snapshot_world`] helper that:
//!
//! 1. Builds a [`DynamicScene`] from the [`World`] (Bevy engine reflection
//!    pulls every `#[reflect(Component)]` component and
//!    `#[reflect(Resource)]` resource registered with `AppTypeRegistry`),
//!    minus a small denylist of Bevy-runtime resources whose inner
//!    types are not (and cannot reasonably be) reflect-serialised
//!    (e.g. `bevy_a11y::AccessibilityRequested` wraps an
//!    `Arc<AtomicBool>` that Bevy's [`SceneSerializer`] refuses).
//! 2. Wraps the scene in [`SceneSerializer`] and pipes it through `ron`.
//! 3. Wraps the RON blob in a [`SaveFile`] envelope that also carries
//!    [`SaveMetadata`] for the menu to read without parsing the body.
//!
//! Out of scope for PR-A:
//!
//! - Atomic on-disk write (write to `.tmp` then rename) — that lands in
//!   PR-B alongside the slot manager.
//! - Compression — saves are KB-scale.

use bevy::prelude::*;
use bevy_scene::serde::SceneSerializer;
use bevy_scene::DynamicSceneBuilder;
use serde::{Deserialize, Serialize};
use std::fmt;

use super::format_version::FORMAT_VERSION;
use super::migrate::{Body, SchemaKind};

/// Top-level save file envelope written to disk.
///
/// The on-disk layout is a single RON document with two fields:
/// `metadata` ([`SaveMetadata`]) and `body` ([`Body`]).
///
/// The split is intentional: the menu's [`SaveIndex`](crate::ui::launch::save_index::SaveIndex)
/// scanner (GRA-311 PR-A) reads the first 4 KB of every save to discover
/// what's on disk. Putting `metadata` first means the menu can list saves
/// without paying the cost of a full [`Body`] round-trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFile {
    pub metadata: SaveMetadata,
    pub body: Body,
}

/// Player-visible header stored at the top of every save.
///
/// Fields are intentionally narrow and stable: GRA-311's menu reads this
/// struct to populate the Load Game list. Bumping the schema here is
/// exactly what the format-version + migrator chain exists to handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMetadata {
    /// Format version. Must equal [`FORMAT_VERSION`] on write.
    /// On read, the value is checked by the loader.
    pub format_version: u32,
    /// When the save was produced, in seconds since the Unix epoch
    /// (UTC). The menu formats this for the player.
    pub saved_at_unix_s: u64,
    /// Total in-game playtime at the moment of the save, in seconds.
    /// Useful for "1h 23m" UI labels.
    pub playtime_s: u64,
    /// Stable game-seed the save was started with. Players can share
    /// seeds; future patches may use this to deterministically
    /// reproduce a save on a different binary.
    pub seed: u64,
    /// Helios version (Cargo package version) that produced the save.
    /// For humans reading a bug report — the loader does not gate on this.
    pub helios_version: String,
}

impl SaveMetadata {
    /// Build a [`SaveMetadata`] from the current wall clock + the game's
    /// seed resource. Caller is responsible for filling in `playtime_s`
    /// from whatever the play-time tracker resource turns out to be
    /// (PR-C will introduce [`AutosaveTimer`](super::autosave::AutosaveTimer);
    /// for PR-A the caller passes `0` or a known elapsed-seconds value).
    pub fn new_now(seed: u64, playtime_s: u64, helios_version: impl Into<String>) -> Self {
        let saved_at_unix_s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            format_version: FORMAT_VERSION,
            saved_at_unix_s,
            playtime_s,
            seed,
            helios_version: helios_version.into(),
        }
    }
}

/// Errors from snapshot/restore operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// RON could not serialise the scene — almost always a Reflect
    /// coverage gap (R2 in the CTO design).
    Serialize(String),
    /// The type registry is missing — the [`App`] was never built
    /// with `AppTypeRegistry`. PR-A does not auto-register it; the
    /// caller must add `.init_resource::<AppTypeRegistry>()`.
    MissingTypeRegistry,
    /// Bevy returned an error we cannot classify. Wrapped for
    /// ergonomic matching.
    Other(String),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Serialize(s) => write!(f, "save serialise failed: {s}"),
            SnapshotError::MissingTypeRegistry => write!(
                f,
                "AppTypeRegistry is not present on the World — call \
                 init_resource::<AppTypeRegistry>() before snapshotting"
            ),
            SnapshotError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Resources the snapshot must **skip**.
///
/// Bevy's `DefaultPlugins` install resources whose inner types are
/// not reflect-serialisable:
///
/// - [`bevy_a11y::AccessibilityRequested`] wraps an
///   `Arc<AtomicBool>` whose field-reflection Bevy won't register —
///   the [`SceneSerializer`] fails when it encounters it.
/// - [`bevy_a11y::ManageAccessibilityUpdates`] has `Reflect` + the
///   `ReflectSerialize` type data registered, but its sibling resource
///   triggers the same chain. We deny both for symmetry because Bevy
///   is free to swap which one it visits first across versions.
/// - [`bevy_winit::WinitMonitors`], [`bevy_winit::WinitSettings`],
///   [`bevy_winit::DisplayHandleWrapper`], and
///   [`bevy_winit::EventLoopProxyWrapper`] hold platform / event-loop
///   handles that have no meaningful serialised form and are
///   reconstructed on every Bevy startup anyway.
///
/// All of these are runtime plumbing that the player has zero
/// observable interest in persisting — letting the in-memory state
/// drift after the next launch is correct.
fn configure_builder(builder: DynamicSceneBuilder) -> DynamicSceneBuilder {
    builder
        .deny_resource::<bevy_a11y::AccessibilityRequested>()
        .deny_resource::<bevy_a11y::ManageAccessibilityUpdates>()
        .deny_resource::<bevy_winit::WinitMonitors>()
        .deny_resource::<bevy_winit::WinitSettings>()
        .deny_resource::<bevy_winit::DisplayHandleWrapper>()
        .deny_resource::<bevy_winit::EventLoopProxyWrapper>()
}

/// Snapshot the given [`World`] into a RON string ready to write to disk.
///
/// PR-B's snapshot uses [`DynamicSceneBuilder`] and an internal denylist
/// to skip Bevy-runtime resources that fail Bevy's reflect-serialise
/// pipeline. Game state (components / resources we own) is included.
pub fn snapshot_world(world: &World, metadata: SaveMetadata) -> Result<String, SnapshotError> {
    let scene = configure_builder(DynamicSceneBuilder::from_world(world)).build();

    let registry = world
        .get_resource::<AppTypeRegistry>()
        .ok_or(SnapshotError::MissingTypeRegistry)?;
    let registry_locked = registry.read();

    let serializer = SceneSerializer::new(&scene, &registry_locked);
    let scene_ron = ron::ser::to_string_pretty(&serializer, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))?;

    let body = Body {
        schema: SchemaKind::SceneRon,
        data: scene_ron,
    };
    let file = SaveFile { metadata, body };
    ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))
}

/// Snapshot variant for tests: takes the registry lock by reference so
/// the helper works against a bare [`World`] that has had
/// `init_resource::<AppTypeRegistry>()` called but no Plugin has run yet.
pub fn snapshot_world_with_registry(
    world: &World,
    registry: &bevy::ecs::reflect::AppTypeRegistry,
    metadata: SaveMetadata,
) -> Result<String, SnapshotError> {
    let scene = configure_builder(DynamicSceneBuilder::from_world(world)).build();
    let registry_locked = registry.read();
    let serializer = SceneSerializer::new(&scene, &registry_locked);
    let scene_ron = ron::ser::to_string_pretty(&serializer, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))?;
    let body = Body {
        schema: SchemaKind::SceneRon,
        data: scene_ron,
    };
    let file = SaveFile { metadata, body };
    ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_metadata_carries_format_version_constant() {
        let md = SaveMetadata::new_now(42, 100, "0.4.0");
        assert_eq!(md.format_version, FORMAT_VERSION);
        assert_eq!(md.seed, 42);
        assert_eq!(md.playtime_s, 100);
        assert_eq!(md.helios_version, "0.4.0");
    }

    #[test]
    fn save_metadata_serde_ron_round_trip() {
        let md = SaveMetadata::new_now(7, 3600, "0.4.0-test");
        let ron = ron::to_string(&md).expect("serialize");
        let back: SaveMetadata = ron::from_str(&ron).expect("deserialize");
        assert_eq!(md, back);
    }

    #[test]
    fn snapshot_requires_type_registry() {
        let world = World::new();
        // Deliberately do NOT insert AppTypeRegistry.
        let err = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect_err("missing AppTypeRegistry must error");
        assert_eq!(err, SnapshotError::MissingTypeRegistry);
    }

    #[test]
    fn snapshot_skips_a11y_resource_inner_arc_atomic() {
        // Regression test for the in-game save failure: the engine
        // installs `bevy_a11y::AccessibilityRequested` whose inner
        // `Arc<AtomicBool>` is not reflect-serialisable. Without the
        // denylist in `configure_builder`, `SceneSerializer` blows up
        // with "did not register the ReflectSerialize" — see the
        // SavePanel "save serialise failed" warnings.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.insert_resource(bevy_a11y::AccessibilityRequested::default());
        // The presence of the resource plus our denylist must let
        // the snapshot complete cleanly.
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip a11y resource");
        assert!(!ron.is_empty());
        // Smoke check: the snapshot should NOT mention a11y types —
        // the denylist kept them out of the scene blob.
        assert!(
            !ron.contains("AccessibilityRequested"),
            "denylist failed — a11y resource serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_winit_resources() {
        // `bevy_winit` resources hold platform/event-loop handles
        // that have no business in a save file. Confirm the denylist
        // covers them so a save doesn't depend on transient Bevy
        // internals.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.init_resource::<bevy_winit::WinitSettings>();
        world.init_resource::<bevy_winit::WinitMonitors>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip winit resources");
        assert!(!ron.contains("WinitSettings"));
        assert!(!ron.contains("WinitMonitors"));
    }
}
