//! Personnel systems.
//!
//! PR-A scaffold: empty stub. The hire / promotion / assignment
//! systems land in PR-C alongside the analysis queue and the
//! University building.

use bevy::prelude::*;

use super::components::Scientist;

/// Stub for the `hire_scientists` system. No-op in PR-A; wired up
/// in PR-C when the University building is added.
pub fn hire_scientists(_time: Res<SimulationTime>, _commands: Commands, _query: Query<&Scientist>) {
    // PR-A stub.
}

/// Stub for the `seniority_promotion` system. No-op in PR-A; wired
/// up in PR-C.
pub fn seniority_promotion(_query: Query<&mut Scientist>) {
    // PR-A stub.
}

/// Re-export the simulation time type. Mirrors the pattern used by
/// `economy::systems` and `research::systems`.
pub type SimulationTime = crate::ui::time::SimulationTime;
