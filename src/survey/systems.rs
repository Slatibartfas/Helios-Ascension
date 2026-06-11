//! Survey systems.
//!
//! PR-A scaffold: system function stubs only. The actual logic lands
//! in subsequent PRs:
//!
//! - PR-B (instruments + mission templates): `advance_survey_missions`
//!   is wired up.
//! - PR-C (analysis queue): `process_analysis_queue` becomes real.
//! - PR-D (anomalies): `surface_anomaly_events` becomes real.
//! - PR-F (mining efficiency): `compute_mining_efficiency` becomes
//!   real.
//!
//! The stubs are present so the plugin registration site is stable
//! across PRs — adding a new system is a one-line change in `mod.rs`
//! rather than a structural refactor.

use bevy::prelude::*;

use super::components::SurveyState;

/// Tick confidence decay for all known dimensions. No-op in PR-A
/// because no `SurveyState` components are attached to bodies yet;
/// wired up in PR-B once the system populator starts inserting
/// `SurveyState`.
pub fn decay_survey_confidence(_time: Res<SimulationTime>, _query: Query<&mut SurveyState>) {
    // PR-A stub. Implementation in PR-B:
    //
    //   use super::types::{CONFIDENCE_DECAY_PER_YEAR, SURVEY_DAYS_PER_YEAR};
    //   let elapsed_years = (time.elapsed_seconds() - state.last_updated_sim_time)
    //       / SURVEY_DAYS_PER_YEAR;
    //   let decay = elapsed_years * CONFIDENCE_DECAY_PER_YEAR as f64;
    //   for fidelity in state.dimensions.values_mut() {
    //       fidelity.confidence = (fidelity.confidence - decay as f32).max(0.0);
    //   }
    //   state.last_updated_sim_time = time.elapsed_seconds();
}

/// Tick active survey missions. No-op in PR-A; wired up in PR-B.
pub fn advance_survey_missions(_time: Res<SimulationTime>, _query: Query<&mut SurveyState>) {
    // PR-A stub.
}

/// Drive the analysis queue. No-op in PR-A; wired up in PR-C.
pub fn process_analysis_queue(_time: Res<SimulationTime>, _query: Query<&mut SurveyState>) {
    // PR-A stub.
}

/// Fire events for newly-detected anomalies. No-op in PR-A; wired up
/// in PR-D.
pub fn surface_anomaly_events(_time: Res<SimulationTime>, _query: Query<&SurveyState>) {
    // PR-A stub.
}

/// Update the system-wide "SURVEY %" stat. No-op in PR-A; wired up
/// in PR-B.
pub fn update_survey_summary(_query: Query<&SurveyState>) {
    // PR-A stub.
}

/// Re-export the simulation time type from the `ui::time` module
/// so systems can declare `Res<SimulationTime>` without taking a
/// hard dependency on the `ui` module.
///
/// Mirrors the pattern used by `economy::systems` and
/// `research::systems`.
pub type SimulationTime = crate::ui::time::SimulationTime;
