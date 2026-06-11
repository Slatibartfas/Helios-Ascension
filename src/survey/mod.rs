//! Survey system (v0.5.0 rework, PR-A scaffold).
//!
//! Implements the progressive, multi-instrument survey design from
//! `docs/design/SURVEY_REWORK.md`. PR-A adds the data model and
//! module scaffolding; subsequent PRs add RON registries, the
//! analysis queue, the personnel system, anomaly events, and the
//! mining-efficiency ramp.
//!
//! ## Phase 1 migration window
//!
//! The legacy `SurveyLevel` enum in `crate::economy::components` is
//! kept and remains the source of truth for `discovered_amount()`
//! until Phase 5 of the migration plan. New bodies can carry a
//! [`SurveyState`] component alongside their `SurveyLevel`; the
//! [`SurveyState::from_legacy_level`] migration shim maps the old
//! enum values to the new state.

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod systems;
pub mod types;

pub use components::{
    ActiveSurveyMission, AnalysisJob, DetectedAnomaly, DimensionFidelity, SurveyState,
};
pub use data::{
    AnalysisQueueIndex, MiningEfficiencyRegistry, MiningEfficiencyRow, ModderAnomalyDef,
    ModderDimensionDef, SurveyAnomalyRegistry, SurveyDimensionRegistry, SurveyInstrumentDef,
    SurveyInstrumentRegistry, SurveyMissionTemplate, SurveyMissionTemplates,
};
pub use systems::{
    advance_survey_missions, decay_survey_confidence, process_analysis_queue,
    surface_anomaly_events, update_survey_summary, SimulationTime,
};
pub use types::{
    AnomalyType, SurveyDimension, SurveyMethod, CONFIDENCE_DECAY_PER_YEAR, INITIAL_CONFIDENCE,
    MAX_TIER, STALE_CONFIDENCE, SURVEY_DAYS_PER_YEAR, WARNING_CONFIDENCE,
};

/// Plugin that registers the survey system with the Bevy app.
///
/// PR-A registers the registries as default-initialized resources
/// and the system stubs. The stubs are no-ops in PR-A; they are
/// wired up in PR-B (missions) and PR-C (analysis queue).
pub struct SurveyPlugin;

impl Plugin for SurveyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources — default-initialized empty registries. The
            // RON loaders land in PR-B; until then the app starts
            // with the hardcoded defaults from the binary (the eight
            // dimensions in `SurveyDimension::ALL`, the nine methods
            // in `SurveyMethod`, etc.).
            .init_resource::<SurveyDimensionRegistry>()
            .init_resource::<SurveyInstrumentRegistry>()
            .init_resource::<SurveyMissionTemplates>()
            .init_resource::<SurveyAnomalyRegistry>()
            .init_resource::<MiningEfficiencyRegistry>()
            .init_resource::<AnalysisQueueIndex>()
            // Update systems — PR-A stubs only. The systems are
            // listed in execution order so PR-B/C/D can swap each
            // stub for its real implementation in place.
            //
            // Schedule: `Update` for now. Once `SimulationTime` is
            // tick-based (it already is — see `ui::time`), these
            // could move to a fixed-tick schedule. PR-B will make
            // that call.
            .add_systems(
                Update,
                (
                    decay_survey_confidence,
                    advance_survey_missions,
                    process_analysis_queue,
                    surface_anomaly_events,
                    update_survey_summary,
                )
                    .chain(),
            );
    }
}
