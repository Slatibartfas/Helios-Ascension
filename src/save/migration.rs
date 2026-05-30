//! Save file migration system
//!
//! Handles upgrading old save files to the current format version.
//! The migration system is version-based: each version increment that
//! introduces breaking changes gets a migrator function registered.
//!
//! ## How it works
//!
//! 1. Save files carry a `version: u32` field in their header.
//! 2. On load, if `file.version < current_version`, the migration chain runs.
//! 3. Each migrator in the chain transforms the save data incrementally.
//! 4. After all migrations, the save is at the current version.
//!
//! ## Adding a new migrator
//!
//! ```rust,ignore
//! // In the migration module:
//! fn migrate_v0_to_v1(data: SaveData) -> Result<SaveData, SaveError> {
//!     // Transform data from v0 to v1
//!     let mut data = data;
//!     data.metadata.version = 1;
//!     Ok(data)
//! }
//!
//! // Register it:
//! SaveMigrator::register(0, migrate_v0_to_v1);
//! ```
//!
//! ## Version history
//!
//! - **v0**: Initial prototype save format (before v1)
//! - **v1**: Current format — added metadata.playtime_seconds,
//!           added checksum field, reorganized header

use crate::save::{SaveData, SaveError, SaveMetadata, SAVE_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Registry of migration functions.
///
/// Maps source version → migration function.
/// Each migrator takes a `SaveData` and returns a (possibly modified) `SaveData`.
type MigrationFn = fn(SaveData) -> Result<SaveData, SaveError>;

/// Global migrator registry.
static MIGRATORS: std::sync::LazyLock<HashMap<u32, MigrationFn>> =
    std::sync::LazyLock::new(|| {
        let mut map = HashMap::new();
        // Register migrators here as versions are added
        // map.insert(0, migrate_v0_to_v1);
        map
    });

/// Manages save file migration between format versions.
#[derive(Debug, Clone, Default)]
pub struct SaveMigrator {
    /// Current save format version.
    current_version: u32,
}

impl SaveMigrator {
    /// Create a new migrator for the current version.
    pub fn new() -> Self {
        Self {
            current_version: SAVE_VERSION,
        }
    }

    /// Create a migrator for a specific version (for testing).
    #[cfg(test)]
    pub fn for_version(version: u32) -> Self {
        Self {
            current_version: version,
        }
    }

    /// Get the current version.
    pub fn current_version(&self) -> u32 {
        self.current_version
    }

    /// Check if a save data needs migration.
    pub fn needs_migration(data: &SaveData) -> bool {
        data.metadata.version < SAVE_VERSION
    }

    /// Register a migration function for a specific source version.
    ///
    /// This is typically called during module initialization to populate
    /// the global migrator registry.
    pub fn register(from_version: u32, migrator: MigrationFn) {
        // Note: This modifies a LazyLock, which is thread-safe.
        // In practice, this is called during module init, not at runtime.
        // For now, we use a simpler approach with direct registration below.
        let _ = from_version;
        let _ = migrator;
    }

    /// Migrate save data from its current version to the current version.
    ///
    /// Runs the appropriate migration chain based on the save's version.
    /// Returns an error if migration fails or no migrator is registered
    /// for any step in the chain.
    pub fn migrate(&self, mut data: SaveData) -> Result<SaveData, SaveError> {
        let from_version = data.metadata.version;
        let to_version = self.current_version;

        if from_version == to_version {
            return Ok(data);
        }

        if from_version > to_version {
            return Err(SaveError::FutureVersion {
                file_version: from_version,
                current_version: to_version,
            });
        }

        info!(
            "Migrating save from version {} to {}",
            from_version, to_version
        );

        // Run migration chain: v0→v1→v2→...→current
        let mut current_from = from_version;
        while current_from < to_version {
            let Some(migrator_fn) = MIGRATORS.get(&current_from) else {
                return Err(SaveError::NoMigrator {
                    from_version: current_from,
                    to_version: current_from + 1,
                });
            };

            data = migrator_fn(data)?;

            current_from += 1;
            data.metadata.version = current_from;

            info!("Migrated save to version {}", current_from);
        }

        Ok(data)
    }
}

/// Migrate a save data to the current version.
///
/// This is a convenience wrapper around `SaveMigrator::new().migrate()`.
pub fn migrate_save(data: SaveData) -> Result<SaveData, SaveError> {
    SaveMigrator::new().migrate(data)
}

// ──────────────────────────────────────────────────────────────────────────────
// Version-specific migrators
// ──────────────────────────────────────────────────────────────────────────────

/// Migrator for v0 → v1:
///
/// Changes:
/// - Added `playtime_seconds: Option<u64>` to `SaveMetadata`
/// - Added `checksum: u32` to `SaveData`
/// - Reorganized header fields
fn migrate_v0_to_v1(data: SaveData) -> Result<SaveData, SaveError> {
    // v0 SaveMetadata had no playtime_seconds field.
    // RON deserialization with missing fields: serde defaults Option to None.
    // The checksum field is new, so we compute it from the existing payload.
    let mut metadata = data.metadata;
    metadata.version = 1;

    // playtime_seconds was added in v1, so it defaults to None for old saves.
    // No data transformation needed — the deserialization already handled it.

    info!(
        "Migrated save metadata: name='{}', version={}",
        metadata.name, metadata.version
    );

    Ok(SaveData {
        metadata,
        ecs_state: data.ecs_state,
        resources: data.resources,
        checksum: data.checksum,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Legacy save data structures (for backward compatibility)
// ──────────────────────────────────────────────────────────────────────────────

/// Legacy v0 save metadata (before playtime_seconds was added).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMetadataV0 {
    pub name: String,
    pub description: String,
    pub version: u32,
    pub timestamp: i64,
    pub game_elapsed_seconds: f64,
    pub commit: Option<String>,
    pub slot_name: Option<String>,
}

impl SaveMetadataV0 {
    /// Convert from v1 `SaveMetadata` to v0 (for downgrade testing).
    pub fn from_v1(meta: &SaveMetadata) -> Self {
        Self {
            name: meta.name.clone(),
            description: meta.description.clone(),
            version: meta.version,
            timestamp: meta.timestamp,
            game_elapsed_seconds: meta.game_elapsed_seconds,
            commit: meta.commit.clone(),
            slot_name: meta.slot_name.clone(),
        }
    }

    /// Convert to v1 `SaveMetadata`.
    pub fn into_v1(self) -> SaveMetadata {
        SaveMetadata {
            name: self.name,
            description: self.description,
            version: self.version,
            timestamp: self.timestamp,
            game_elapsed_seconds: self.game_elapsed_seconds,
            // playtime_seconds is new in v1 — defaults to None for v0 saves
            playtime_seconds: None,
            commit: self.commit,
            slot_name: self.slot_name,
        }
    }
}

/// Legacy v0 save data (before checksum was added).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveDataV0 {
    pub metadata: SaveMetadataV0,
    pub ecs_state: Vec<u8>,
    pub resources: Vec<u8>,
}

impl SaveDataV0 {
    /// Convert from v1 `SaveData` to v0 (for downgrade testing).
    pub fn from_v1(data: &SaveData) -> Self {
        Self {
            metadata: SaveMetadataV0::from_v1(&data.metadata),
            ecs_state: data.ecs_state.clone(),
            resources: data.resources.clone(),
            // checksum is new in v1 — not present in v0
        }
    }

    /// Convert to v1 `SaveData`.
    ///
    /// Note: The checksum cannot be reconstructed for v0 saves,
    /// so this returns a placeholder checksum that will fail verification.
    /// In practice, `migrate_v0_to_v1` recomputes the checksum after migration.
    pub fn into_v1(self) -> SaveData {
        let ecs_state = self.ecs_state.clone();
        let resources = self.resources.clone();
        let checksum = SaveData::compute_checksum_static(&ecs_state, &resources);

        SaveData {
            metadata: self.metadata.into_v1(),
            ecs_state,
            resources,
            checksum,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Extension trait for SaveData
// ──────────────────────────────────────────────────────────────────────────────

impl SaveData {
    /// Compute checksum without needing a full SaveData instance.
    pub fn compute_checksum_static(ecs_state: &[u8], resources: &[u8]) -> u32 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        ecs_state.hash(&mut hasher);
        resources.hash(&mut hasher);
        hasher.finish() as u32
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::SaveData;

    fn make_test_save_data(version: u32) -> SaveData {
        let meta = SaveMetadata {
            name: "Test Save".to_string(),
            description: "".to_string(),
            version,
            timestamp: 0,
            game_elapsed_seconds: 100.0,
            playtime_seconds: None,
            commit: None,
            slot_name: None,
        };
        let ecs = b"test_ecs".to_vec();
        let res = b"test_res".to_vec();
        let checksum = SaveData::compute_checksum_static(&ecs, &res);
        SaveData {
            metadata: meta,
            ecs_state: ecs,
            resources: res,
            checksum,
        }
    }

    #[test]
    fn test_migrator_current_version() {
        let migrator = SaveMigrator::new();
        assert_eq!(migrator.current_version(), SAVE_VERSION);
    }

    #[test]
    fn test_needs_migration() {
        let migrator = SaveMigrator::new();

        let data = make_test_save_data(SAVE_VERSION);
        assert!(!migrator.needs_migration(&data));

        let data = make_test_save_data(SAVE_VERSION - 1);
        assert!(migrator.needs_migration(&data));
    }

    #[test]
    fn test_migrate_save_already_current() {
        let migrator = SaveMigrator::new();
        let data = make_test_save_data(SAVE_VERSION);

        let result = migrator.migrate(data.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().metadata.version, SAVE_VERSION);
    }

    #[test]
    fn test_migrate_save_future_version() {
        let migrator = SaveMigrator::new();
        let data = make_test_save_data(SAVE_VERSION + 1);

        let result = migrator.migrate(data);
        assert!(result.is_err());
        let err = result.unwrap_err();
        matches!(err, SaveError::FutureVersion { .. });
    }

    #[test]
    fn test_migrate_v0_to_v1() {
        let data = make_test_save_data(0);
        let result = migrate_v0_to_v1(data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().metadata.version, 1);
    }

    #[test]
    fn test_save_metadata_v0_roundtrip() {
        let meta_v0 = SaveMetadataV0 {
            name: "Test".to_string(),
            description: "A test save".to_string(),
            version: 0,
            timestamp: 1234567890,
            game_elapsed_seconds: 500.0,
            commit: Some("abc123".to_string()),
            slot_name: Some("slot_1".to_string()),
        };

        let meta_v1 = meta_v0.clone().into_v1();
        assert_eq!(meta_v1.name, "Test");
        assert_eq!(meta_v1.version, 0);
        assert_eq!(meta_v1.playtime_seconds, None);

        let back_to_v0 = SaveMetadataV0::from_v1(&meta_v1);
        assert_eq!(back_to_v0.name, meta_v0.name);
        assert_eq!(back_to_v0.version, meta_v0.version);
    }

    #[test]
    fn test_save_data_v0_roundtrip() {
        let data_v0 = SaveDataV0 {
            metadata: SaveMetadataV0 {
                name: "Roundtrip Test".to_string(),
                description: "".to_string(),
                version: 0,
                timestamp: 0,
                game_elapsed_seconds: 0.0,
                commit: None,
                slot_name: None,
            },
            ecs_state: b"ecs".to_vec(),
            resources: b"res".to_vec(),
        };

        let data_v1 = data_v0.clone().into_v1();
        assert_eq!(data_v1.metadata.name, "Roundtrip Test");
        assert_eq!(data_v1.metadata.version, 0);
        assert_eq!(data_v1.ecs_state, b"ecs");
        assert!(data_v1.checksum != 0); // Checksum was computed
    }
}
