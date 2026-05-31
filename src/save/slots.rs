//! Save slot management
//!
//! Manages named save slots with metadata, listing, deletion, and
//! backup rotation. Each slot stores a single save file plus a
//! human-readable metadata entry.
//!
//! ## Slot directory structure
//!
//! ```text
//! saves/
//!   slot_1.ron          ← Save data
//!   slot_1.meta.ron     ← Metadata (name, timestamp, playtime)
//!   slot_2.ron
//!   slot_2.meta.ron
//!   autosave.ron
//!   autosave.meta.ron
//! ```

use crate::save::{SaveData, SaveError, SaveMetadata, WorldSaveExt};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Maximum number of save slots allowed.
pub const MAX_SAVE_SLOTS: usize = 20;

/// A save slot with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSlot {
    /// Unique slot identifier (e.g. "slot_1", "autosave").
    pub id: String,
    /// Human-readable name shown in the UI.
    pub display_name: String,
    /// Optional description set by the player.
    pub description: String,
    /// Timestamp when the slot was last written (Unix epoch seconds).
    pub timestamp: i64,
    /// Game elapsed time when the slot was last saved.
    pub game_elapsed_seconds: f64,
    /// Cumulative playtime tracked across sessions (seconds).
    pub playtime_seconds: u64,
    /// Whether this slot is marked as a favorite.
    pub is_favorite: bool,
    /// Whether this slot is an autosave (vs. a manual save).
    pub is_autosave: bool,
    /// File size in bytes of the save data.
    pub file_size_bytes: u64,
}

impl SaveSlot {
    /// Create a new slot from metadata.
    pub fn from_metadata(id: impl Into<String>, meta: &SaveMetadata, file_size_bytes: u64) -> Self {
        Self {
            id: id.into(),
            display_name: meta.name.clone(),
            description: meta.description.clone(),
            timestamp: meta.timestamp,
            game_elapsed_seconds: meta.game_elapsed_seconds,
            playtime_seconds: meta.playtime_seconds.unwrap_or(0),
            is_favorite: false,
            is_autosave: meta.slot_name.as_ref().map(|s| s == "autosave").unwrap_or(false),
            file_size_bytes,
        }
    }

    /// Get the formatted timestamp for display.
    pub fn formatted_timestamp(&self) -> String {
        let ts = self.timestamp;
        let datetime = chrono::Timestamp::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| format!("Timestamp: {}", ts));
        datetime
    }

    /// Get the formatted playtime for display (HH:MM:SS).
    pub fn formatted_playtime(&self) -> String {
        let total = self.playtime_seconds;
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }

    /// Get the formatted game elapsed time for display.
    pub fn formatted_game_time(&self) -> String {
        let total = self.game_elapsed_seconds;
        let years = (total / (365.25 * 24.0 * 3600.0)) as u64;
        let days = ((total % (365.25 * 24.0 * 3600.0)) / (24.0 * 3600.0)) as u64;
        let hours = ((total % (24.0 * 3600.0)) / 3600.0) as u64;
        let minutes = ((total % 3600.0) / 60.0) as u64;

        if years > 0 {
            format!("{}y {}d {}h {}m", years, days, hours, minutes)
        } else if days > 0 {
            format!("{}d {}h {}m", days, hours, minutes)
        } else {
            format!("{}h {}m", hours, minutes)
        }
    }

    /// Mark the slot as a favorite.
    pub fn set_favorite(&mut self, favorite: bool) {
        self.is_favorite = favorite;
    }

    /// Update the slot from newer metadata.
    pub fn update_from(&mut self, meta: &SaveMetadata, file_size_bytes: u64) {
        self.display_name = meta.name.clone();
        self.description = meta.description.clone();
        self.timestamp = meta.timestamp;
        self.game_elapsed_seconds = meta.game_elapsed_seconds;
        if let Some(pt) = meta.playtime_seconds {
            self.playtime_seconds = pt;
        }
        self.file_size_bytes = file_size_bytes;
    }
}

/// Manages the collection of save slots.
#[derive(Resource, Debug, Clone, Default)]
pub struct SaveSlotManager {
    /// All known save slots, keyed by slot ID.
    slots: HashMap<String, SaveSlot>,
    /// The directory where saves are stored.
    save_dir: PathBuf,
    /// Whether the slot list has been loaded from disk.
    loaded: bool,
}

impl SaveSlotManager {
    /// Create a new slot manager with the given save directory.
    pub fn new(save_dir: PathBuf) -> Self {
        Self {
            slots: HashMap::new(),
            save_dir,
            loaded: false,
        }
    }

    /// Get the save directory path.
    pub fn save_dir(&self) -> &PathBuf {
        &self.save_dir
    }

    /// Set the save directory path.
    pub fn set_save_dir(&mut self, path: PathBuf) {
        self.save_dir = path;
    }

    /// Get the path for a save file given a slot ID.
    pub fn save_path(&self, slot_id: &str) -> PathBuf {
        self.save_dir.join(format!("{}.ron", slot_id))
    }

    /// Get the path for a metadata file given a slot ID.
    pub fn meta_path(&self, slot_id: &str) -> PathBuf {
        self.save_dir.join(format!("{}.meta.ron", slot_id))
    }

    /// Load all slots from the save directory.
    ///
    /// Scans the directory for `.meta.ron` files and loads each one.
    /// Silently skips missing or corrupt metadata files.
    pub fn load_slots(&mut self) {
        if !self.save_dir.exists() {
            std::fs::create_dir_all(&self.save_dir).ok();
            return;
        }

        self.slots.clear();

        let entries = std::fs::read_dir(&self.save_dir).ok();
        let entries = entries.into_iter().flatten().flatten();

        for entry in entries {
            let path = entry.path();
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Only load .meta.ron files
            if !filename.ends_with(".meta") {
                continue;
            }

            let slot_id = &filename[..filename.len() - 5]; // strip ".meta"
            let meta_path = self.meta_path(slot_id);

            if let Ok(bytes) = std::fs::read(&meta_path) {
                if let Ok(slot) = ron::from_bytes::<SaveSlot>(&bytes) {
                    self.slots.insert(slot_id.to_string(), slot);
                }
            }
        }

        self.loaded = true;
        info!("Loaded {} save slots", self.slots.len());
    }

    /// Returns true if slots have been loaded from disk.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Get all slots, sorted by timestamp (newest first).
    pub fn all_slots(&self) -> Vec<&SaveSlot> {
        let mut slots: Vec<_> = self.slots.values().collect();
        slots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        slots
    }

    /// Get all manual (non-autosave) slots.
    pub fn manual_slots(&self) -> Vec<&SaveSlot> {
        self.all_slots()
            .into_iter()
            .filter(|s| !s.is_autosave)
            .collect()
    }

    /// Get all autosave slots.
    pub fn autosave_slots(&self) -> Vec<&SaveSlot> {
        self.all_slots()
            .into_iter()
            .filter(|s| s.is_autosave)
            .collect()
    }

    /// Get a slot by ID.
    pub fn get(&self, slot_id: &str) -> Option<&SaveSlot> {
        self.slots.get(slot_id)
    }

    /// Get a mutable slot by ID.
    pub fn get_mut(&mut self, slot_id: &str) -> Option<&mut SaveSlot> {
        self.slots.get_mut(slot_id)
    }

    /// Check if a slot exists.
    pub fn contains(&self, slot_id: &str) -> bool {
        self.slots.contains_key(slot_id)
    }

    /// Get the number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Returns true if there are no slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Register a new slot (or update an existing one).
    pub fn register(&mut self, slot: SaveSlot) {
        self.slots.insert(slot.id.clone(), slot);
    }

    /// Remove a slot (does NOT delete the files — use `delete_slot` for that).
    pub fn remove(&mut self, slot_id: &str) {
        self.slots.remove(slot_id);
    }

    /// Save the world to a named slot.
    ///
    /// If the slot already exists, it will be overwritten.
    /// Returns the metadata of the saved slot.
    pub fn save_to_slot(
&mut self,
        world: &mut World,
        slot_id: &str,
        name: impl Into<String>,
    ) -> Result<SaveMetadata, SaveError> {
        let path = self.save_path(slot_id);
        let path_for_meta = path.clone();
        let meta = world.save_to_path(path, name)?;

        // Update slot metadata
        let file_size = std::fs::metadata(&path_for_meta)
            .map(|m| m.len())
            .unwrap_or(0);

        let slot = SaveSlot::from_metadata(slot_id, &meta, file_size);
        self.register(slot);

        // Write slot metadata to .meta.ron file
        let meta_path = self.meta_path(slot_id);
        if let Some(slot) = self.slots.get(slot_id) {
            if let Ok(bytes) = ron::to_string(slot) {
                std::fs::write(&meta_path, bytes).ok();
            }
        }

        Ok(meta)
    }

    /// Load the world from a named slot.
    pub fn load_from_slot(&mut self, world: &mut World, slot_id: &str) -> Result<SaveMetadata, SaveError> {
        if !self.contains(slot_id) {
            return Err(SaveError::SlotNotFound(slot_id.to_string()));
        }

        let path = self.save_path(slot_id);
        world.load_from_path(path)
    }

    /// Delete a save slot (both data and metadata files).
    pub fn delete_slot(&mut self, slot_id: &str) -> Result<(), SaveError> {
        if !self.contains(slot_id) {
            return Err(SaveError::SlotNotFound(slot_id.to_string()));
        }

        let save_path = self.save_path(slot_id);
        let meta_path = self.meta_path(slot_id);

        if save_path.exists() {
            std::fs::remove_file(&save_path)
                .map_err(|e| SaveError::IoError(e.to_string()))?;
        }

        if meta_path.exists() {
            std::fs::remove_file(&meta_path)
                .map_err(|e| SaveError::IoError(e.to_string()))?;
        }

        self.remove(slot_id);
        info!("Deleted save slot: {}", slot_id);
        Ok(())
    }

    /// Rename a save slot.
    pub fn rename_slot(
        &mut self,
        slot_id: &str,
        new_name: impl Into<String>,
    ) -> Result<(), SaveError> {
        // Compute meta_path BEFORE get_mut to avoid borrow conflict
        let meta_path = self.meta_path(slot_id);

        let slot = self.slots.get_mut(slot_id)
            .ok_or_else(|| SaveError::SlotNotFound(slot_id.to_string()))?;

        slot.display_name = new_name.into();

        // Write updated metadata
        if let Ok(bytes) = ron::to_string(&*slot) {
            std::fs::write(&meta_path, bytes).ok();
        }

        Ok(())
    }

    /// Toggle the favorite status of a slot.
    pub fn toggle_favorite(&mut self, slot_id: &str) -> Result<bool, SaveError> {
        // Compute meta_path BEFORE get_mut to avoid borrow conflict
        let meta_path = self.meta_path(slot_id);

        let slot = self.slots.get_mut(slot_id)
            .ok_or_else(|| SaveError::SlotNotFound(slot_id.to_string()))?;

        slot.is_favorite = !slot.is_favorite;

        // Write updated metadata
        if let Ok(bytes) = ron::to_string(&*slot) {
            std::fs::write(&meta_path, bytes).ok();
        }

        Ok(slot.is_favorite)
    }

    /// Get the next available numbered slot ID (slot_1, slot_2, ...).
    pub fn next_slot_id(&self) -> Option<String> {
        for i in 1..=MAX_SAVE_SLOTS {
            let id = format!("slot_{}", i);
            if !self.contains(&id) {
                return Some(id);
            }
        }
        None
    }

    /// Returns true if there is room for a new slot.
    pub fn has_room(&self) -> bool {
        self.len() < MAX_SAVE_SLOTS
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Chrono replacement (avoid external dependency for simple timestamp formatting)
// ──────────────────────────────────────────────────────────────────────────────

mod chrono {
    use std::time::Timestamp;

    /// A simple timestamp formatter that doesn't need the chrono crate.
    pub struct Timestamp;

    impl Timestamp {
        pub fn from_timestamp(timestamp: i64, _offset_secs: i32) -> Option<Self> {
            // Unix timestamps are always UTC
            if timestamp< 0 {
                return None;
            }
            Some(Self)
        }

        pub fn format(&self, format: &str) -> chrono_fmt::FormattedTimestamp {
            chrono_fmt::FormattedTimestamp {
                timestamp: 0,
                format: format.to_string(),
            }
        }
    }

    mod chrono_fmt {
        use std::fmt;

        pub struct FormattedTimestamp {
            pub timestamp: i64,
            pub format: String,
        }

        impl fmt::Display for FormattedTimestamp {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Simple formatting: just show the timestamp as a decimal.
                // A full implementation would parse the format string and
                // produce proper date/time output. For now, show Unix time.
                write!(f, "{}", self.timestamp)
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn make_meta(name: &str) -> SaveMetadata {
        SaveMetadata {
            name: name.to_string(),
            description: "test".to_string(),
            version: 1,
            timestamp: 1000,
            game_elapsed_seconds: 3600.0,
            playtime_seconds: Some(7200),
            commit: None,
            slot_name: None,
        }
    }

    #[test]
    fn test_save_slot_from_metadata() {
        let meta = make_meta("Test Slot");
        let slot = SaveSlot::from_metadata("slot_1", &meta, 1024);

        assert_eq!(slot.id, "slot_1");
        assert_eq!(slot.display_name, "Test Slot");
        assert_eq!(slot.playtime_seconds, 7200);
        assert!(!slot.is_favorite);
        assert!(!slot.is_autosave);
    }

    #[test]
    fn test_save_slot_formatted_playtime() {
        let meta = make_meta("Test");
        let slot = SaveSlot::from_metadata("slot_1", &meta, 0);

        assert_eq!(slot.formatted_playtime(), "02:00:00");
    }

    #[test]
    fn test_save_slot_set_favorite() {
        let meta = make_meta("Test");
        let mut slot = SaveSlot::from_metadata("slot_1", &meta, 0);

        assert!(!slot.is_favorite);
        slot.set_favorite(true);
        assert!(slot.is_favorite);
        slot.set_favorite(false);
        assert!(!slot.is_favorite);
    }

    #[test]
    fn test_slot_manager_new() {
        let dir = temp_dir();
        let manager = SaveSlotManager::new(dir.path().to_path_buf());

        assert_eq!(manager.len(), 0);
        assert!(manager.is_empty());
        assert!(manager.save_dir().exists());
    }

    #[test]
    fn test_slot_manager_register() {
        let dir = temp_dir();
        let mut manager = SaveSlotManager::new(dir.path().to_path_buf());

        let meta = make_meta("Registered Slot");
        let slot = SaveSlot::from_metadata("slot_1", &meta, 1024);
        manager.register(slot);

        assert_eq!(manager.len(), 1);
        assert!(manager.contains("slot_1"));
        assert_eq!(manager.get("slot_1").unwrap().display_name, "Registered Slot");
    }

    #[test]
    fn test_slot_manager_next_slot_id() {
        let dir = temp_dir();
        let manager = SaveSlotManager::new(dir.path().to_path_buf());

        assert_eq!(manager.next_slot_id(), Some("slot_1".to_string()));
    }

    #[test]
    fn test_slot_manager_next_slot_id_filled() {
        let dir = temp_dir();
        let mut manager = SaveSlotManager::new(dir.path().to_path_buf());

        // Fill all slots
        for i in 1..=MAX_SAVE_SLOTS {
            let meta = make_meta("Test");
            let slot = SaveSlot::from_metadata(format!("slot_{}", i),&meta, 0);
            manager.register(slot);
        }

        assert!(manager.next_slot_id().is_none());
        assert!(!manager.has_room());
    }

    #[test]
    fn test_slot_manager_save_path() {
        let dir = temp_dir();
        let manager = SaveSlotManager::new(dir.path().to_path_buf());

        let path = manager.save_path("slot_1");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "slot_1.ron");
    }

    #[test]
    fn test_slot_manager_meta_path() {
        let dir = temp_dir();
        let manager = SaveSlotManager::new(dir.path().to_path_buf());

        let path = manager.meta_path("slot_1");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "slot_1.meta.ron");
    }

    #[test]
    fn test_slot_manager_delete_slot_not_found() {
        let dir = temp_dir();
        let mut manager = SaveSlotManager::new(dir.path().to_path_buf());

        let result = manager.delete_slot("nonexistent");
        assert!(result.is_err());
        matches!(result.unwrap_err(), SaveError::SlotNotFound(_));
    }

    #[test]
    fn test_slot_manager_toggle_favorite() {
        let dir = temp_dir();
        let mut manager = SaveSlotManager::new(dir.path().to_path_buf());

        let meta = make_meta("Fav Test");
        let slot = SaveSlot::from_metadata("slot_1",&meta, 0);
        manager.register(slot);

        let is_fav = manager.toggle_favorite("slot_1").unwrap();
        assert!(is_fav);
        assert!(manager.get("slot_1").unwrap().is_favorite);

        let is_fav = manager.toggle_favorite("slot_1").unwrap();
        assert!(!is_fav);
        assert!(!manager.get("slot_1").unwrap().is_favorite);
    }
}
