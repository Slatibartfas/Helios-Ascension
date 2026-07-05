//! Save/load consumer plugin (GRA-358 PR-B).
//!
//! PR-A shipped [`crate::persistence::PersistencePlugin`], which
//! sets up the on-disk snapshot/restore helpers but never schedules
//! any `Update` systems — save/load only ran on explicit menu
//! action. PR-B layers [`SaveLoadPlugin`] on top:
//!
//! 1. Register [`crate::persistence::playtime::PlaytimeTracker`] +
//!    [`crate::persistence::autosave::AutosaveTimer`] as resources
//!    (default values, replaced by the Settings subview on edit).
//! 2. Schedule [`crate::persistence::playtime::tick_playtime_tracker`]
//!    in `Update`. Runs on every frame regardless of `LaunchState`.
//! 3. Schedule
//!    [`crate::persistence::autosave::tick_autosave_timer`] in
//!    `Update`, **after** the playtime tick — Bevy 0.18 system
//!    ordering needs `.after(tick_playtime_tracker)` because the
//!    autosave snapshot reads `PlaytimeTracker::total_real_s`.
//!
//! # Why a separate plugin
//!
//! Keeping `SaveLoadPlugin` distinct from `PersistencePlugin` lets
//! downstream code wire the autosave timer **only** when both
//! pieces (settings + game state) are present. The PR-A plugin is
//! safe to register in any context — pure helpers, no side
//! effects. The PR-B plugin assumes `LaunchState` + `TimeScale` +
//! `PersistentSettings` + `GameSeed` + `PlaytimeTracker` all exist.
//!
//! # Schedule placement
//!
//! Both tick systems live in `Update`, not
//! `EguiPrimaryContextPass`. The playtime tracker is a pure resource
//! mutation; the autosave timer touches the filesystem via
//! [`write_save_atomic`] (sync IO) — putting it in the egui context
//! pass would block the UI on every fire, which is exactly the
//! pause the design avoids.

use bevy::prelude::*;

use crate::persistence::autosave::{tick_autosave_timer, AutosaveTimer};
use crate::persistence::playtime::{tick_playtime_tracker, PlaytimeTracker};

/// Plugin that wires PR-B's save/load consumers into Bevy.
///
/// Callers must have already added
/// [`crate::persistence::PersistencePlugin`] (PR-A) and
/// [`crate::ui::launch::LaunchPlugin`] (GRA-309) — this plugin
/// reads resources those plugins own (`LaunchState`,
/// `PersistentSettings`, `GameSeed`) but does not insert them.
pub struct SaveLoadPlugin;

impl Plugin for SaveLoadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlaytimeTracker>()
            .init_resource::<AutosaveTimer>()
            .add_systems(Update, tick_playtime_tracker)
            // The autosave tick is exclusive (takes `&mut World`)
            // because snapshot+save_index re-scan cannot share a
            // `Res<World>` borrow. Ordering is enforced explicitly
            // so the snapshot's `playtime_s` field reflects this
            // frame's playtime contribution.
            .add_systems(Update, tick_autosave_timer.after(tick_playtime_tracker));
    }
}
