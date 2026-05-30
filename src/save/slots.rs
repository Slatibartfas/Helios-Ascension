//! Save slot management.
//!
//! Handles the 10 save slots with metadata, quicksave, and slot operations.

use super::{format_timestamp, write_save, read_save, GameSavedState};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum number of save slots
pub const MAX_SLOTS: usize = 10;

/// Save slot metadata (stored separately from the save file).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SlotMetadata {
    /// Slot index (0-9)
    pub slot: usize,
    /// Human-readable save name
    pub name: String,
    /// When the save was created
    pub timestamp: i64,
    /// Elapsed time in seconds
    pub elapsed_seconds: f64,
    /// Save description
    pub description: String,
    /// File size in bytes
    pub file_size: u64,
}

impl SlotMetadata {
    /// Format elapsed time as a human-readable string.
    pub fn format_elapsed(&self) -> String {
        let seconds = self.elapsed_seconds as i64;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }

    /// Format timestamp as a human-readable string.
    pub fn format_date(&self) -> String {
        format_timestamp(self.timestamp)
    }
}

/// Get the path for a save slot file.
fn slot_path(slot: usize) -> PathBuf {
    super::save_directory().join(format!("slot_{}.ron.zst", slot))
}

/// Get the path for a slot's metadata file.
fn metadata_path(slot: usize) -> PathBuf {
    super::save_directory().join(format!("slot_{}.meta.json", slot))
}

/// Quicksave slot index
pub const QUICKSAVE_SLOT: usize = 0;

/// Autosave slot base index (rotates among slots 7, 8, 9)
pub const AUTOSAVE_SLOTS: [usize; 3] = [7, 8, 9];

/// Save game to a slot.
pub fn save_to_slot(slot: usize, state: &GameSavedState, name: &str) -> std::io::Result<SlotMetadata> {
    if slot >= MAX_SLOTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid slot: {} (max {})", slot, MAX_SLOTS - 1),
        ));
    }

    let path = slot_path(slot);
    write_save(&path, state)?;

    // Write metadata
    let metadata = SlotMetadata {
        slot,
        name: name.to_string(),
        timestamp: state.timestamp,
        elapsed_seconds: state.elapsed_seconds,
        description: state.description.clone(),
        file_size: std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0),
    };

    let meta_path = metadata_path(slot);
    let meta_json = serde_json::to_vec_pretty(&metadata)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(meta_path, meta_json)?;

    info!("Game saved to slot {}: {}", slot, name);
    Ok(metadata)
}

/// Load game from a slot.
pub fn load_from_slot(slot: usize) -> std::io::Result<GameSavedState> {
    if slot >= MAX_SLOTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid slot: {} (max {})", slot, MAX_SLOTS - 1),
        ));
    }

    let path = slot_path(slot);
    read_save(&path)
}

/// Get metadata for a slot (without loading the full save).
pub fn get_slot_metadata(slot: usize) -> std::io::Result<Option<SlotMetadata>> {
    if slot >= MAX_SLOTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid slot: {} (max {})", slot, MAX_SLOTS - 1),
        ));
    }

    let path = metadata_path(slot);
    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read(path)?;
    let metadata: SlotMetadata = serde_json::from_slice(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(Some(metadata))
}

/// Delete a save slot.
pub fn delete_slot(slot: usize) -> std::io::Result<()> {
    if slot >= MAX_SLOTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid slot: {} (max {})", slot, MAX_SLOTS - 1),
        ));
    }

    let save_path = slot_path(slot);
    let meta_path = metadata_path(slot);

    if save_path.exists() {
        std::fs::remove_file(save_path)?;
    }
    if meta_path.exists() {
        std::fs::remove_file(meta_path)?;
    }

    info!("Deleted save slot {}", slot);
    Ok(())
}

/// List all occupied slots with their metadata.
pub fn list_slots() -> std::io::Result<Vec<Option<SlotMetadata>>> {
    let mut slots = Vec::with_capacity(MAX_SLOTS);
    for i in 0..MAX_SLOTS {
        slots.push(get_slot_metadata(i)?);
    }
    Ok(slots)
}

/// Perform quicksave (F5).
pub fn quicksave(state: &GameSavedState) -> std::io::Result<SlotMetadata> {
    let name = format!(
        "Quicksave {}",
        chrono_timestamp()
    );
    save_to_slot(QUICKSAVE_SLOT, state, &name)
}

/// Perform quickload (F9).
pub fn quickload() -> std::io::Result<GameSavedState> {
    load_from_slot(QUICKSAVE_SLOT)
}

/// Check if a quicksave exists.
pub fn has_quicksave() -> bool {
    slot_path(QUICKSAVE_SLOT).exists()
}

fn chrono_timestamp() -> i64 {
    super::current_timestamp()
}