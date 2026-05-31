//! Autosave system
//!
//! Provides automatic periodic saving during gameplay. The autosave system
//! runs independently of the main game loop and can be configured to save at
//! regular intervals (wall-clock time or game-time based).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Settings that control autosave behavior.
#[derive(Resource, Debug, Clone)]
pub struct AutoSaveSettings {
    /// Whether autosave is enabled. Default: true.
    pub enabled: bool,
    /// Interval in wall-clock seconds between autosaves. Default: 300 (5 min).
    pub interval_seconds: u64,
    /// Maximum number of autosave backups to keep. Default: 3.
    pub max_backups: usize,
    /// The save slot name to use for autosaves. Default: "autosave".
    pub slot_name: String,
    /// Whether to pause the game during autosave. Default: false.
    pub pause_during_save: bool,
    /// Whether to show a notification when autosave completes. Default: true.
    pub notify_on_complete: bool,
 /// Whether to save on game events (turn end, major battle, etc.). Default: true.
    pub save_on_events: bool,
}

impl Default for AutoSaveSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 300,
            max_backups: 3,
            slot_name: "autosave".to_string(),
            pause_during_save: false,
            notify_on_complete: true,
            save_on_events: true,
        }
    }
}

impl AutoSaveSettings {
    /// Create settings with a custom interval.
    pub fn with_interval(mut self, seconds: u64) -> Self {
        self.interval_seconds = seconds;
        self
    }

    /// Create settings with a custom slot name.
    pub fn with_slot_name(mut self, name: impl Into<String>) -> Self {
        self.slot_name = name.into();
        self
    }

    /// Disable autosave.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Runtime state for the autosave system.
#[derive(Resource, Debug, Clone)]
pub struct AutoSaveState {
    /// Whether an autosave is currently in progress.
    pub is_saving: bool,
    /// Timestamp of the last successful autosave (Unix epoch seconds).
    pub last_save_timestamp: i64,
    /// Number of autosaves performed this session.
    pub session_save_count: u32,
    /// Whether the next autosave was triggered by a game event.
    pub triggered_by_event: bool,
    /// Error message from the last failed autosave, if any.
    pub last_error: Option<String>,
}

impl Default for AutoSaveState {
    fn default() -> Self {
        Self {
            is_saving: false,
            last_save_timestamp: 0,
            session_save_count: 0,
            triggered_by_event: false,
            last_error: None,
        }
    }
}

impl AutoSaveState {
    /// Returns true if enough time has elapsed to trigger another autosave.
    pub fn should_autosave(&self, settings: &AutoSaveSettings) -> bool {
        if !settings.enabled || self.is_saving {
            return false;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let elapsed = now - self.last_save_timestamp;
        elapsed >= settings.interval_seconds as i64 || self.triggered_by_event
    }

    /// Mark the start of an autosave operation.
    pub fn start_save(&mut self) {
        self.is_saving = true;
        self.triggered_by_event = false;
        self.last_error = None;
    }

    /// Mark the successful completion of an autosave.
    pub fn finish_save(&mut self) {
        self.is_saving = false;
        self.last_save_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.session_save_count += 1;
    }

    /// Mark a failed autosave.
    pub fn fail_save(&mut self, error: impl Into<String>) {
        self.is_saving = false;
        self.last_error = Some(error.into());
    }

    /// Trigger autosave on the next tick.
    pub fn trigger_event_save(&mut self) {
        self.triggered_by_event = true;
    }
}

/// Timer resource for tracking autosave intervals.
///
/// This is separate from `AutoSaveState` because it needs to be updated
/// by the system scheduler, not manually.
#[derive(Resource, Debug, Clone, Default)]
pub struct AutoSaveTimer {
    /// Accumulated time since last autosave (wall-clock seconds).
    accumulated_seconds: f64,
    /// Whether the timer is paused.
    paused: bool,
}

impl AutoSaveTimer {
    /// Returns the accumulated time in seconds.
    pub fn elapsed(&self) -> f64 {
        self.accumulated_seconds
    }

    /// Resets the timer to zero.
    pub fn reset(&mut self) {
        self.accumulated_seconds = 0.0;
    }

    /// Pause the timer.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the timer.
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Returns true if the timer is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

/// System that ticks the autosave timer and triggers saves.
pub fn autosave_tick(
    mut timer: ResMut<AutoSaveTimer>,
    mut state: ResMut<AutoSaveState>,
    settings: Res<AutoSaveSettings>,
    time: Res<Time>,
) {
    if timer.is_paused() || !settings.enabled {
        return;
    }

    timer.accumulated_seconds += time.delta().as_secs_f64();

    let interval = settings.interval_seconds as f64;
    if timer.accumulated_seconds >= interval || state.triggered_by_event {
        // Trigger autosave
        timer.reset();
        state.trigger_event_save();

        info!(
            "Autosave triggered (interval: {}s, event: {})",
            settings.interval_seconds,
            state.triggered_by_event
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Autosave slot management
// ──────────────────────────────────────────────────────────────────────────────

/// An autosave backup entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutosaveBackup {
    /// Slot index (0 = most recent, 1 = second most recent, etc.)
    pub index: usize,
    /// Timestamp when this backup was created.
    pub timestamp: i64,
    /// Game elapsed time when the backup was created.
    pub game_elapsed_seconds: f64,
    /// File path to the backup.
    pub path: String,
}

impl AutosaveBackup {
    /// Create a new backup entry.
    pub fn new(index: usize, path: impl Into<String>) -> Self {
        Self {
            index,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            game_elapsed_seconds: 0.0,
            path: path.into(),
        }
    }

    /// Set the game elapsed time.
    pub fn with_game_time(mut self, seconds: f64) -> Self {
        self.game_elapsed_seconds = seconds;
        self
    }
}

/// Manages autosave backup rotation.
pub struct AutosaveRotator {
    /// The autosave slot name.
    slot_name: String,
    /// The directory where autosaves are stored.
    save_dir: PathBuf,
    /// The maximum number of backups to keep.
    max_backups: usize,
}

impl AutosaveRotator {
    /// Create a new autosave rotator.
    pub fn new(slot_name: impl Into<String>, save_dir: PathBuf, max_backups: usize) -> Self {
        Self {
            slot_name: slot_name.into(),
            save_dir,
            max_backups,
        }
    }

    /// Get the path for a given backup index.
    pub fn backup_path(&self, index: usize) -> PathBuf {
        if index == 0 {
            self.save_dir.join(format!("{}.ron", self.slot_name))
        } else {
            self.save_dir.join(format!("{}_backup_{}.ron", self.slot_name, index))
        }
    }

    /// Rotate backups, deleting the oldest if max is exceeded.
    pub fn rotate(&self) -> std::io::Result<()> {
        // Check if the primary autosave exists
        let primary = self.backup_path(0);
        if !primary.exists() {
            return Ok(());
        }

        // Rotate existing backups
        for i in (1..self.max_backups).rev() {
            let from = self.backup_path(i - 1);
            let to = self.backup_path(i);

            if from.exists() {
                if to.exists() {
                    std::fs::remove_file(&to)?;
                }
                std::fs::rename(&from, &to)?;
            }
        }

        // Create a new backup_0 from the current primary
        let new_backup = self.backup_path(0);
        if primary.exists() {
            std::fs::copy(&primary, &new_backup)?;
        }

        Ok(())
    }

    /// Prune excess backups beyond max_backups.
    pub fn prune(&self) -> std::io::Result<usize> {
        let mut removed = 0;
        for i in self.max_backups.. {
            let path = self.backup_path(i);
            if path.exists() {
                std::fs::remove_file(&path)?;
                removed += 1;
            } else {
                break;
            }
        }
        Ok(removed)
    }

    /// List all existing backups.
    pub fn list_backups(&self) -> Vec<AutosaveBackup> {
        let mut backups = Vec::new();

        for i in 0.. {
            let path = self.backup_path(i);
            if path.exists() {
                let backup = AutosaveBackup::new(i, path.to_string_lossy().to_string());
                backups.push(backup);
            } else {
                break;
            }
        }

        backups
    }
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_autosave_settings_default() {
        let settings = AutoSaveSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.interval_seconds, 300);
        assert_eq!(settings.max_backups, 3);
        assert_eq!(settings.slot_name, "autosave");
    }

    #[test]
    fn test_autosave_settings_builder() {
        let settings = AutoSaveSettings::default()
            .with_interval(600)
            .with_slot_name("quicksave");

        assert_eq!(settings.interval_seconds, 600);
        assert_eq!(settings.slot_name, "quicksave");
    }

    #[test]
    fn test_autosave_state_should_autosave() {
        let state = AutoSaveState::default();
        let settings = AutoSaveSettings::default();

        assert!(state.should_autosave(&settings));

        let mut state = AutoSaveState::default();
        state.is_saving = true;
        assert!(!state.should_autosave(&settings));
    }

    #[test]
    fn test_autosave_rotator_rotate() {
        let dir = temp_dir();
        let rotator = AutosaveRotator::new("test_autosave", dir.path().to_path_buf(), 3);

        // Create the primary autosave file
        let primary = rotator.backup_path(0);
        std::fs::write(&primary, "save data").unwrap();

        rotator.rotate().unwrap();

        // Backup 0 should now exist
        assert!(rotator.backup_path(0).exists());
        // Backup 1 should now exist (copied from old primary)
        assert!(rotator.backup_path(1).exists());
    }

    #[test]
    fn test_autosave_rotator_prune() {
        let dir = temp_dir();
        let rotator = AutosaveRotator::new("test_autosave", dir.path().to_path_buf(), 2);

        // Create 4 backup files
        for i in 0..4 {
            let path = rotator.backup_path(i);
            std::fs::write(&path, format!("backup {}", i)).unwrap();
        }

        let removed = rotator.prune().unwrap();
        assert_eq!(removed, 2); // Removed indices 2 and 3

        assert!(rotator.backup_path(0).exists());
        assert!(rotator.backup_path(1).exists());
        assert!(!rotator.backup_path(2).exists());
        assert!(!rotator.backup_path(3).exists());
    }

    #[test]
    fn test_autosave_rotator_list_backups() {
        let dir = temp_dir();
        let rotator = AutosaveRotator::new("test_autosave", dir.path().to_path_buf(), 3);

        // Create 2 backup files
        std::fs::write(&rotator.backup_path(0), "backup 0").unwrap();
        std::fs::write(&rotator.backup_path(1), "backup 1").unwrap();

        let backups = rotator.list_backups();
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].index, 0);
        assert_eq!(backups[1].index, 1);
    }
}
