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
pub mod events;
pub mod systems;
pub mod types;
pub mod visibility;

pub use components::{
    default_activation_threshold, ActiveSurveyMission, AnalysisJob, DetectedAnomaly,
    DimensionFidelity, EvidencePoint, ExtractionSite, LandingSite, SiteScoreWeights, SiteScores,
    SurveyState, LANDING_SITE_EVAL_THRESHOLD, MAX_SITES_PER_BODY, MIN_SITES_PER_BODY,
};
pub use data::{
    load_anomalies, AnalysisQueueIndex, AnomalyDef, AnomalyEffect, MiningEfficiencyRegistry,
    MiningEfficiencyRow, ModderAnomalyDef, ModderDimensionDef, SurveyAnomalyRegistry,
    SurveyDimensionRegistry, SurveyInstrumentDef, SurveyInstrumentRegistry, SurveyMissionTemplate,
    SurveyMissionTemplates,
};
pub use events::{SurveyEvent, SurveyEventKind};
pub use systems::{
    advance_survey_missions, decay_survey_confidence, evaluate_landing_sites,
    process_analysis_queue, surface_anomaly_events, update_survey_summary, SimulationTime,
};
pub use types::{
    default_method_specificity, AnomalyState, AnomalyType, EvidenceKind, EvidencePoint,
    SurveyDimension, SurveyMethod, CONFIDENCE_DECAY_PER_YEAR, DATA_POINT_CONFIDENCE_BUMP,
    DEFAULT_ACTIVATION_THRESHOLD, INITIAL_CONFIDENCE, MAX_CONFIDENCE, MAX_TIER,
    MIN_ACTIVATION_THRESHOLD, REFUTATION_REARM_THRESHOLD, RETRY_PRESSURE_DECAY_PER_YEAR,
    RETRY_PRESSURE_PER_VERIFICATION, RETRY_PRESSURE_THRESHOLD_REDUCTION, STALE_CONFIDENCE,
    SURVEY_DAYS_PER_YEAR, VERIFICATION_CONFIDENCE_BUMP, WARNING_CONFIDENCE,
};
pub use visibility::{estimate_with_fidelity, is_stale, DepositEstimate, DepositVisibility};

/// Plugin that registers the survey system with the Bevy app.
///
/// PR-A registers the registries as default-initialized resources
/// and the system stubs. The stubs are no-ops in PR-A; they are
/// wired up in PR-B (missions) and PR-C (analysis queue). PR-C
/// registers the `SurveyEvent` message and replaces
/// `surface_anomaly_events` with the r2 detection roll +
/// confidence model.
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
            // `SurveyAnomalyRegistry` is initialized by the
            // `load_anomalies` startup system, which reads
            // `anomalies.ron` and inserts the populated resource. We
            // skip the default-initialization here so the
            // startup system is the single source of truth.
            .init_resource::<MiningEfficiencyRegistry>()
            .init_resource::<AnalysisQueueIndex>()
            // PR-C: load `anomalies.ron` at startup so the registry
            // is populated before the first per-tick detection roll.
            .add_systems(Startup, data::load_anomalies)
            // PR-C: register the `SurveyEvent` message so the
            // notification surface and the per-tick detection
            // system can communicate.
            .add_message::<SurveyEvent>()
            // Update systems — PR-A stubs plus the PR-C
            // `surface_anomaly_events` implementation. The systems
            // are listed in execution order so PR-B/C/D can swap
            // each stub for its real implementation in place.
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
                    evaluate_landing_sites,
                    update_survey_summary,
                )
                    .chain(),
            );
    }
}
