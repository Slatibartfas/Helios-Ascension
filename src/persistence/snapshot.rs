//! Snapshot the live Bevy [`World`] into a RON string.
//!
//! PR-A ships a single [`snapshot_world`] helper that:
//!
//! 1. Builds a [`DynamicScene`] from the [`World`] (Bevy engine reflection
//!    pulls every `#[reflect(Component)]` component and
//!    `#[reflect(Resource)]` resource registered with `AppTypeRegistry`).
//! 2. Wraps the scene in [`SceneSerializer`] and pipes it through `ron`.
//! 3. Wraps the RON blob in a [`SaveFile`] envelope that also carries
//!    [`SaveMetadata`] for the menu to read without parsing the body.
//!
//! Out of scope for PR-A:
//!
//! - Atomic on-disk write (write to `.tmp` then rename) — that lands in
//!   PR-B alongside the slot manager.
//! - Compression — saves are KB-scale.
//! - Filtering — PR-A snapshots *everything* registered with reflection.
//!   Filtering specific components/resources (e.g. the renderer's
//!   short-lived ones) is a future PR.

use bevy::prelude::*;
use bevy_scene::serde::SceneSerializer;
use bevy_scene::DynamicScene;
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

/// Snapshot the given [`World`] into a RON string ready to write to disk.
///
/// PR-A's snapshot is unconditional — every `#[reflect(Component)]` and
/// `#[reflect(Resource)]` type registered with `AppTypeRegistry` is
/// included. Filtering is out of scope for this PR.
pub fn snapshot_world(world: &World, metadata: SaveMetadata) -> Result<String, SnapshotError> {
    let scene = DynamicScene::from_world(world);

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
    let scene = DynamicScene::from_world(world);
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
}
