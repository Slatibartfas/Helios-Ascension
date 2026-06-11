//! Personnel system (v0.5.0 rework, PR-A scaffold).
//!
//! Implements the scientist roster and assignment system from
//! `docs/design/SURVEY_REWORK.md` §8. PR-A adds the data model and
//! module scaffolding; PR-C adds the analysis-queue assignment loop,
//! the University building, the seniority-promotion system, and the
//! Personnel menu UI.
//!
//! ## Phase 3 migration window
//!
//! The existing `GameMenu::Personnel` in `crate::game_state` is
//! currently a stub. The Personnel menu is filled out in PR-C to be
//! the scientist roster and assignments panel.

use bevy::prelude::*;

pub mod components;
pub mod systems;
pub mod types;

pub use components::Scientist;
pub use systems::{hire_scientists, seniority_promotion, SimulationTime};
pub use types::{ScientistId, ScientistSpecialty, SeniorityTier};

/// Plugin that registers the personnel system with the Bevy app.
///
/// PR-A registers nothing functional — the system stubs are
/// no-ops. PR-C adds the hire / promotion / assignment systems and
/// registers them on the appropriate schedules.
pub struct PersonnelPlugin;

impl Plugin for PersonnelPlugin {
    fn build(&self, app: &mut App) {
        // No resources or systems are registered in PR-A. The plugin
        // exists so the registration site in `main.rs` is stable
        // across PRs — PR-C adds resources and systems in place.
        let _ = app;
    }
}
