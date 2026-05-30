//! Autosave system with rotation.
//!
//! Automatically saves the game every N minutes, keeping the last 3 autosaves
//! by rotating through slots 7, 8, and 9.

use super::slots::{self, AUTOSAVE_SLOTS};
use super::{extract_game_state, GameSavedState};
use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Autosave interval in seconds (5 minutes)
pub const AUTOSAVE_INTERVAL_SECS: u64 = 300;

/// Maximum autosave files to keep
pub const MAX_AUTOSAVES: usize = 3;

/// Resource to track autosave timing.
#[derive(Resource, Debug)]
pub struct AutosaveTimer {
    /// Time since last autosave in seconds
    elapsed_secs: f64,
    /// Whether an autosave is in progress
    saving: AtomicBool,
}

impl AutosaveTimer {
    /// Create a new autosave timer.
    pub fn new() -> Self {
        Self {
            elapsed_secs: 0.0,
            saving: AtomicBool::new(false),
        }
    }

    /// Check if it's time to autosave.
    pub fn should_autosave(&self) -> bool {
        !self.saving.load(Ordering::Relaxed) && self.elapsed_secs >= AUTOSAVE_INTERVAL_SECS as f64
    }

    /// Reset the timer after an autosave.
    pub fn reset(&mut self) {
        self.elapsed_secs = 0.0;
    }

    /// Increment the elapsed time.
    pub fn add_time(&mut self, delta_secs: f64) {
        self.elapsed_secs += delta_secs;
    }

    /// Mark autosave as started.
    pub fn start_save(&self) {
        self.saving.store(true, Ordering::Relaxed);
    }

    /// Mark autosave as finished.
    pub fn end_save(&self) {
        self.saving.store(false, Ordering::Relaxed);
    }
}

impl Default for AutosaveTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Track which autosave slot to use next (rotates 7 → 8 → 9 → 7).
#[derive(Resource, Debug)]
pub struct AutosaveRotation {
    /// Index into AUTOSAVE_SLOTS for the next autosave
    next_slot_index: usize,
}

impl AutosaveRotation {
    /// Create a new rotation tracker.
    pub fn new() -> Self {
        Self { next_slot_index: 0 }
    }

    /// Get the next slot to use and advance rotation.
    pub fn next_slot(&mut self) -> usize {
        let slot = AUTOSAVE_SLOTS[self.next_slot_index];
        self.next_slot_index = (self.next_slot_index + 1) % AUTOSAVE_SLOTS.len();
        slot
    }

    /// Reset rotation to first slot (useful after loading a game).
    pub fn reset(&mut self) {
        self.next_slot_index = 0;
    }
}

impl Default for AutosaveRotation {
    fn default() -> Self {
        Self::new()
    }
}

/// Perform an autosave, rotating through the autosave slots.
pub fn do_autosave(
    world: &World,
    rotation: &mut AutosaveRotation,
    timer: &AutosaveTimer,
) -> std::io::Result<()> {
    timer.start_save();

    let state = extract_game_state(world);
    let slot = rotation.next_slot();
    let name = format!("Autosave {}", chrono_timestamp_string());

    let result = slots::save_to_slot(slot, &state, &name);

    timer.end_save();
    timer.reset();

    result.map(|_| ())
}

/// System to check and perform autosaves.
/// Run this in the Update schedule.
pub fn autosave_system(
    time: Res<Time<Real>>,
    mut timer: ResMut<AutosaveTimer>,
    mut rotation: ResMut<AutosaveRotation>,
    world: &World,
) {
    timer.add_time(time.delta_secs_f64());

    if timer.should_autosave() {
        if let Err(e) = do_autosave(world, &mut rotation, &timer) {
            error!("Autosave failed: {:?}", e);
        } else {
            info!("Autosave completed");
        }
    }
}

/// Get a formatted timestamp string for autosave names.
fn chrono_timestamp_string() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}", now)
}

/// Initialize autosave resources.
pub fn setup_autosave(app: &mut App) {
    app.init_resource::<AutosaveTimer>()
        .init_resource::<AutosaveRotation>()
        .add_systems(Update, autosave_system);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotation() {
        let mut rot = AutosaveRotation::new();
        let slots: Vec<usize> = (0..6).map(|_| rot.next_slot()).collect();
        // Should cycle through 7, 8, 9, 7, 8, 9
        assert_eq!(slots, vec![7, 8, 9, 7, 8, 9]);
    }

    #[test]
    fn test_autosave_timer() {
        let mut timer = AutosaveTimer::new();
        assert!(!timer.should_autosave());

        timer.add_time(AUTOSAVE_INTERVAL_SECS as f64);
        assert!(timer.should_autosave());
    }
}