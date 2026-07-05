//! Wall-clock + simulation-time playtime tracker (GRA-358 PR-B).
//!
//! PR-A persisted `SaveMetadata.playtime_s` but nothing wrote to it.
//! PR-B adds [`PlaytimeTracker`] — a `Resource` advanced every frame
//! by [`tick_playtime_tracker`]. The tracker holds two fields:
//!
//! - `total_real_s`: wall-clock seconds since the process started,
//!   ticked via [`Time<Real>`]. Counts even while paused (the player
//!   walked away with the game running).
//! - `total_sim_s`: simulation seconds (real × time scale) the
//!   player has actually experienced. **Frozen while
//!   [`crate::ui::time::TimeScale::scale`] == 0.0** so the playtime
//!   label in the menu matches what the player remembers.
//!
//! The system runs in [`Update`], not
//! [`bevy_egui::EguiPrimaryContextPass`], because it is a pure
//! resource mutation with no egui dependency. Bevy 0.18's schedule
//! order guarantees `Update` ticks before `EguiPrimaryContextPass`,
//! so the menu's "X hours played" label always reads last frame's
//! value.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ui::time::TimeScale;

/// Player-visible playtime totals.
///
/// Both fields accumulate monotonically from process boot. There is
/// no overflow handling — `f64` gives ~9.4e15 seconds of headroom,
/// which exceeds the age of the universe many times over.
///
/// `Serialize`/`Deserialize` are present so a future PR-C save-panel
/// flow can round-trip the totals if it ever needs to (the persist
/// write path is currently [`super::snapshot::SaveMetadata::playtime_s`]).
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaytimeTracker {
    /// Wall-clock seconds since the process started. Counts even
    /// while the simulation is paused.
    pub total_real_s: f64,
    /// Simulation seconds (real × TimeScale::scale) the player has
    /// experienced. Frozen while `TimeScale::scale == 0.0`.
    pub total_sim_s: f64,
}

/// Advance the [`PlaytimeTracker`] from `Time<Real>` + [`TimeScale`].
///
/// Idempotent across frames — every [`Update`] tick adds the
/// current delta. The system intentionally does **not** check
/// `LaunchState::is_in_game()`; playtime continues to accrue while
/// the player is on the main menu (matches the "X hours played"
/// label players expect to see grow whether they're on the menu or
/// in-game). The autosave consumer
/// ([`super::autosave::tick_autosave_timer`]) is the gate for
/// *persisting* — not for *counting*.
pub fn tick_playtime_tracker(
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    mut tracker: ResMut<PlaytimeTracker>,
) {
    let real_delta = real_time.delta_secs_f64();
    tracker.total_real_s += real_delta;
    if time_scale.scale > 0.0 {
        tracker.total_sim_s += real_delta * time_scale.scale as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use std::time::Duration;

    /// Build an App whose `Time<Real>` advances by `frame_dt` on
    /// every `app.update()` call. Bevy 0.18's `Time<Real>` is
    /// normally driven by `Instant::now()` via the time system in
    /// `First`; `TimeUpdateStrategy::ManualDuration` is the
    /// documented escape hatch for deterministic tests.
    fn fresh_app(frame_dt: Duration) -> App {
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(frame_dt));
        app.init_resource::<PlaytimeTracker>();
        app.init_resource::<TimeScale>();
        app.add_systems(Update, tick_playtime_tracker);
        app
    }

    #[test]
    fn default_tracker_is_zero() {
        let t = PlaytimeTracker::default();
        assert_eq!(t.total_real_s, 0.0);
        assert_eq!(t.total_sim_s, 0.0);
    }

    #[test]
    fn sim_playtime_frozen_while_paused() {
        let mut app = fresh_app(Duration::from_secs_f64(60.0));
        app.world_mut().resource_mut::<TimeScale>().pause();

        // First update establishes the time system's last_update
        // (delta == 0); subsequent updates carry the 60-s delta.
        app.update();

        let before_sim = app.world().resource::<PlaytimeTracker>().total_sim_s;

        for _ in 0..5 {
            app.update();
        }

        let tracker = app.world().resource::<PlaytimeTracker>();
        assert!(
            (tracker.total_sim_s - before_sim).abs() < 1e-6,
            "sim playtime must be frozen while paused, got {} → {}",
            before_sim,
            tracker.total_sim_s
        );
        assert!(tracker.total_real_s >= 0.0, "real playtime is non-negative");
    }

    #[test]
    fn playtime_accumulates_when_running() {
        // 1/60 s per frame × 60 frames ≈ 1.0 s real time.
        let mut app = fresh_app(Duration::from_secs_f64(1.0 / 60.0));
        app.world_mut().resource_mut::<TimeScale>().set_speed(1.0);

        // The first update carries a zero delta (Bevy time init);
        // skip it. The remaining 60 carry the configured delta.
        app.update();
        for _ in 0..60 {
            app.update();
        }

        let tracker = app.world().resource::<PlaytimeTracker>();
        assert!(
            (tracker.total_real_s - 1.0).abs() < 0.1,
            "real playtime should be ≈ 1.0 s, got {}",
            tracker.total_real_s
        );
        assert!(
            (tracker.total_sim_s - 1.0).abs() < 0.1,
            "sim playtime at 1× scale should ≈ real, got real={} sim={}",
            tracker.total_real_s,
            tracker.total_sim_s
        );
    }

    #[test]
    fn fast_forward_scales_sim_playtime() {
        // 100× sim speed: 1 real s = 100 sim s.
        let mut app = fresh_app(Duration::from_secs_f64(0.1));
        app.world_mut().resource_mut::<TimeScale>().set_speed(100.0);

        app.update();
        for _ in 0..10 {
            app.update();
        }

        let tracker = app.world().resource::<PlaytimeTracker>();
        assert!(
            (tracker.total_real_s - 1.0).abs() < 0.1,
            "real playtime should be ≈ 1.0 s, got {}",
            tracker.total_real_s
        );
        assert!(
            (tracker.total_sim_s - 100.0).abs() < 10.0,
            "sim playtime at 100× should be ≈ 100.0 s, got {}",
            tracker.total_sim_s
        );
    }
}
