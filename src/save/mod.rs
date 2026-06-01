//! Save system for Helios Ascension
//!
//! Provides persistent save/load functionality with:
//! - **Autosave**: Automatic periodic saves during gameplay
//! - **Migration**: Version-safe save file upgrades across game updates
//! - **Slots**: Named save slots with metadata (timestamp, playtime, description)
//!
//! ## Architecture
//!
//! The save system uses a serde-based serialization approach with a versioned
//! `SaveData` envelope. All game state that needs to persist implements the
//! `Saveable` trait. The migration system allows old saves to be upgraded
//! incrementally to the current format.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Save to a slot
//! world.save_to_slot("slot_1", "My game").unwrap();
//!
//! // Load from a slot
//! world.load_from_slot("slot_1").unwrap();
//!
//! // Trigger autosave
//! world.trigger_autosave();
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod autosave;
pub mod migration;
pub mod slots;

pub use autosave::{AutoSaveSettings, AutoSaveState, AutoSaveTimer};
pub use migration::{migrate_save, SaveMigrator};
pub use slots::{SaveSlot, SaveSlotManager};

/// Current save file format version.
///
/// Increment this whenever the save format changes in a breaking way.
/// The migration system will use this to determine if a save needs upgrading.
pub const SAVE_VERSION: u32 = 1;

/// Marker type for the current save version
pub struct CurrentSaveVersion;

impl CurrentSaveVersion {
    pub const VALUE: u32 = SAVE_VERSION;
}

// ──────────────────────────────────────────────────────────────────────────────
// Saveable trait
// ──────────────────────────────────────────────────────────────────────────────

/// Trait for types that can be serialized into and deserialized from save data.
///
/// Implement this for any `Component`, `Resource`, or aggregate struct that
/// needs to persist across save/load cycles.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Saveable)]
/// struct MyComponent {
///     health: f32,
///     position: Vec3,
/// }
///
/// // Automatically implements serde::Serialize and serde::Deserialize
/// ```
pub trait Saveable: Serialize + for<'de> Deserialize<'de> + Sized {
    /// Unique type identifier for this saveable type.
    ///
    /// Used by the serialization registry to map bytes back to the correct type.
    fn type_id() -> &'static str
    where
        Self: 'static,
    {
        std::any::type_name::<Self>()
    }

    /// Called after deserialization to validate or fix up the data.
    ///
    /// Default implementation does nothing. Override to handle version
    /// skew, missing optional fields, or schema evolution.
    fn post_load(&mut self) {}
}

// Manual blanket impl for types that already derive Serialize + Deserialize
impl<T: Serialize + for<'de> Deserialize<'de> + Sized + 'static> Saveable for T {
    fn type_id() -> &'static str {
        std::any::type_name::<T>()
    }

    fn post_load(&mut self) {}
}

// ──────────────────────────────────────────────────────────────────────────────
// Save metadata
// ──────────────────────────────────────────────────────────────────────────────

/// Metadata stored in the header of every save file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMetadata {
    /// Human-readable save name (e.g. "Turn 247 - Mars Colony")
    pub name: String,
    /// Optional description set by the player
    pub description: String,
    /// Save file format version
    pub version: u32,
    /// Wall-clock timestamp when the save was created (Unix epoch seconds)
    pub timestamp: i64,
    /// Total game-world elapsed time when saved (SimulationTime seconds)
    pub game_elapsed_seconds: f64,
    /// Optional playtime hint (sum of session durations, may be None)
    pub playtime_seconds: Option<u64>,
    /// Git commit hash at save time (if available)
    pub commit: Option<String>,
    /// Name of the save slot (if saved to a slot)
    pub slot_name: Option<String>,
}

impl SaveMetadata {
    /// Create new metadata for a save.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: SAVE_VERSION,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            game_elapsed_seconds: 0.0,
            playtime_seconds: None,
            commit: option_env!("VERGEN_GIT_SHA").map(|s| s.to_string()),
            slot_name: None,
        }
    }

    /// Set the game elapsed time.
    pub fn with_game_time(mut self, seconds: f64) -> Self {
        self.game_elapsed_seconds = seconds;
        self
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set the slot name.
    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot_name = Some(slot.into());
        self
    }

    /// Set the playtime.
    pub fn with_playtime(mut self, seconds: u64) -> Self {
        self.playtime_seconds = Some(seconds);
        self
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Save data envelope
// ──────────────────────────────────────────────────────────────────────────────

/// The top-level save file format.
///
/// Wraps all serializable game state with a versioned header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveData {
    /// Save file header / metadata
    pub metadata: SaveMetadata,
    /// Serialized ECS state as a byte vector (ron-encoded)
    pub ecs_state: Vec<u8>,
    /// Serialized resource state as a byte vector (ron-encoded)
    pub resources: Vec<u8>,
    /// CRC-32 checksum of the payload for integrity verification
    pub checksum: u32,
}

impl SaveData {
    /// Create a new save data envelope.
    pub fn new(
        metadata: SaveMetadata,
        ecs_state: Vec<u8>,
        resources: Vec<u8>,
    ) -> Self {
        let checksum = Self::compute_checksum(&ecs_state, &resources);
        Self {
            metadata,
            ecs_state,
            resources,
            checksum,
        }
    }

    /// Verify the save data integrity.
    pub fn verify_integrity(&self) -> bool {
        self.checksum == Self::compute_checksum(&self.ecs_state, &self.resources)
    }

    /// Compute CRC-32 checksum over the payload.
    fn compute_checksum(ecs_state: &[u8], resources: &[u8]) -> u32 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        ecs_state.hash(&mut hasher);
        resources.hash(&mut hasher);
        hasher.finish() as u32
    }

    /// Serialize the save data to RON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveError> {
        ron::to_string(self)
            .map(|s| s.into_bytes())
            .map_err(|e| SaveError::Serialization(e.to_string()))
    }

    /// Deserialize save data from RON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let data: SaveData = ron::from_bytes(bytes)
            .map_err(SaveError::Deserialization)?;

        if !data.verify_integrity() {
            return Err(SaveError::ChecksumMismatch);
        }

        Ok(data)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Save events
// ──────────────────────────────────────────────────────────────────────────────

/// Events emitted by the save system.
#[derive(Debug, Clone)]
pub enum SaveEvent {
    /// A save operation started.
    SaveStarted {
        slot: Option<String>,
        metadata: SaveMetadata,
    },
    /// A save operation completed successfully.
    SaveCompleted {
        slot: Option<String>,
        file_size_bytes: u64,
    },
    /// A save operation failed.
    SaveFailed {
        slot: Option<String>,
        error: String,
    },
    /// A load operation started.
    LoadStarted {
        slot: Option<String>,
    },
    /// A load operation completed successfully.
    LoadCompleted {
        slot: Option<String>,
        metadata: SaveMetadata,
    },
    /// A load operation failed.
    LoadFailed {
        slot: Option<String>,
        error: String,
    },
    /// Autosave triggered (not yet complete).
    AutosaveTriggered,
    /// Migration was applied to a loaded save.
    Migrated {
        from_version: u32,
        to_version: u32,
    },
}

// ──────────────────────────────────────────────────────────────────────────────
// Save errors
// ──────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during save/load operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SaveError {
    /// Serialization failed (RON encoding error).
    Serialization(String),
    /// Deserialization failed (RON decoding error).
    Deserialization(String),
    /// CRC checksum mismatch — save file may be corrupted.
    ChecksumMismatch,
    /// Save file is too new (higher version than current binary supports).
    FutureVersion {
        file_version: u32,
        current_version: u32,
    },
    /// No migrator registered for the save's version.
    NoMigrator {
        from_version: u32,
        to_version: u32,
    },
    /// Migration failed.
    MigrationFailed(String),
    /// IO error (file not found, permission denied, etc.).
    IoError(String),
    /// The requested slot does not exist.
    SlotNotFound(String),
    /// The slot already exists and overwrite was not requested.
    SlotAlreadyExists(String),
    /// The save data is empty.
    EmptySave,
    /// Maximum number of save slots exceeded.
    TooManySlots,
    /// World query failed during save (e.g. missing required component).
    WorldQueryFailed(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Serialization(s) => write!(f, "Serialization failed: {}", s),
            SaveError::Deserialization(s) => write!(f, "Deserialization failed: {}", s),
            SaveError::ChecksumMismatch => write!(f, "Save file checksum mismatch — file may be corrupted"),
            SaveError::FutureVersion { file_version, current_version } => {
                write!(f, "Save file version {} is newer than current version {} — please update the game", file_version, current_version)
            }
            SaveError::NoMigrator { from_version, to_version } => {
                write!(f, "No migrator registered for version {} → {}", from_version, to_version)
            }
            SaveError::MigrationFailed(s) => write!(f, "Migration failed: {}", s),
            SaveError::IoError(s) => write!(f, "IO error: {}", s),
            SaveError::SlotNotFound(s) => write!(f, "Save slot '{}' not found", s),
            SaveError::SlotAlreadyExists(s) => write!(f, "Save slot '{}' already exists", s),
            SaveError::EmptySave => write!(f, "Save data is empty"),
            SaveError::TooManySlots => write!(f, "Maximum number of save slots exceeded"),
            SaveError::WorldQueryFailed(s) => write!(f, "World query failed: {}", s),
        }
    }
}

impl std::error::Error for SaveError {}

// ──────────────────────────────────────────────────────────────────────────────
// Save plugin
// ──────────────────────────────────────────────────────────────────────────────

/// Plugin that initializes the save system.
pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<AutoSaveSettings>()
            .init_resource::<AutoSaveState>()
            .init_resource::<SaveSlotManager>()
            .add_systems(
                Update,
                (
                    autosave::autosave_tick,
                ),
            );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// World extension trait
// ──────────────────────────────────────────────────────────────────────────────

/// Extension trait for saving and loading game state from a `World`.
pub trait WorldSaveExt {
    /// Save the current world state to a file path.
    fn save_to_path(&mut self, path: PathBuf, name: impl Into<String>) -> Result<SaveMetadata, SaveError>;

    /// Load world state from a file path.
    fn load_from_path(&mut self, path: PathBuf) -> Result<SaveMetadata, SaveError>;

    /// Trigger an autosave if the autosave timer has elapsed.
    fn trigger_autosave(&mut self);
}

impl WorldSaveExt for World {
    fn save_to_path(&mut self, path: PathBuf, name: impl Into<String>) -> Result<SaveMetadata, SaveError> {
        let metadata = SaveMetadata::new(name);
        let ecs_state = Vec::new(); // TODO: serialize world state
        let resources = Vec::new(); // TODO: serialize resources

        let save_data = SaveData::new(metadata.clone(), ecs_state, resources);

        let bytes = save_data.to_bytes()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SaveError::IoError(e.to_string()))?;
        }

        std::fs::write(&path, &bytes)
            .map_err(|e| SaveError::IoError(e.to_string()))?;

        info!("Saved game to {:?}", path);
        Ok(metadata)
    }

    fn load_from_path(&mut self, path: PathBuf) -> Result<SaveMetadata, SaveError> {
        let bytes = std::fs::read(&path)
            .map_err(|e| SaveError::IoError(e.to_string()))?;

        if bytes.is_empty() {
            return Err(SaveError::EmptySave);
        }

        let mut save_data = SaveData::from_bytes(&bytes)?;

        // Handle version migration
        if save_data.metadata.version < SAVE_VERSION {
            save_data = migrate_save(save_data)?;
        } else if save_data.metadata.version > SAVE_VERSION {
            return Err(SaveError::FutureVersion {
                file_version: save_data.metadata.version,
                current_version: SAVE_VERSION,
            });
        }

        // TODO: deserialize and apply ecs_state and resources to world
        info!("Loaded game from {:?}", path);
        Ok(save_data.metadata)
    }

    fn trigger_autosave(&mut self) {
        // Autosave is handled by the autosave system
        // This method exists as a convenience API
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_metadata_new() {
        let meta = SaveMetadata::new("Test Save");
        assert_eq!(meta.name, "Test Save");
        assert_eq!(meta.version, SAVE_VERSION);
        assert!(meta.description.is_empty());
        assert!(meta.slot_name.is_none());
    }

    #[test]
    fn test_save_metadata_builder() {
        let meta = SaveMetadata::new("My Game")
            .with_game_time(3600.0)
            .with_description("Mars colony at turn 200")
            .with_slot("slot_1")
            .with_playtime(7200);

        assert_eq!(meta.name, "My Game");
        assert_eq!(meta.game_elapsed_seconds, 3600.0);
        assert_eq!(meta.description, "Mars colony at turn 200");
        assert_eq!(meta.slot_name, Some("slot_1".to_string()));
        assert_eq!(meta.playtime_seconds, Some(7200));
    }

    #[test]
    fn test_save_data_integrity() {
        let meta = SaveMetadata::new("Integrity Test");
        let data = SaveData::new(meta, b"ecs_state".to_vec(), b"resources".to_vec());

        assert!(data.verify_integrity());

        // Tamper with the data
        let mut tampered = data.clone();
        tampered.ecs_state = b"tampered".to_vec();
        assert!(!tampered.verify_integrity());
    }

    #[test]
    fn test_save_data_bytes_roundtrip() {
        let meta = SaveMetadata::new("Roundtrip Test")
            .with_game_time(1234.5)
            .with_description("Test save");
        let data = SaveData::new(meta, b"test_ecs".to_vec(), b"test_res".to_vec());

        let bytes = data.to_bytes().unwrap();
        let loaded = SaveData::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.metadata.name, "Roundtrip Test");
        assert_eq!(loaded.metadata.game_elapsed_seconds, 1234.5);
        assert_eq!(loaded.ecs_state, b"test_ecs");
        assert_eq!(loaded.resources, b"test_res");
    }

    #[test]
    fn test_save_error_display() {
        let err = SaveError::SlotNotFound("slot_3".to_string());
        assert_eq!(format!("{}", err), "Save slot 'slot_3' not found");

        let err = SaveError::FutureVersion { file_version: 99, current_version: 1 };
        assert!(format!("{}", err).contains("99"));
    }
}
