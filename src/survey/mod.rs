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
pub mod milestones;
pub mod systems;
pub mod types;
pub mod visibility;

pub use components::{
    ActiveSurveyMission, AnalysisJob, ContinuousStationBonus, ContinuousSurveyStation,
    DetectedAnomaly, DimensionFidelity, ExtractionSite, FailedMissionRecord, LandingSite,
    SiteScoreWeights, SiteScores, SurveyState, LANDING_SITE_EVAL_THRESHOLD, MAX_SITES_PER_BODY,
    MIN_SITES_PER_BODY,
};
pub use data::{
    load_mission_templates, load_recovery_missions, AnalysisJobRef, AnalysisQueueIndex, AnomalyDef,
    AnomalyEffect, MiningEfficiencyRegistry, MiningEfficiencyRow, ModderAnomalyDef,
    ModderDimensionDef, ReasonTag, RecoveryMission, RecoveryMissionKind, RecoveryMissionRegistry,
    ScientistSummary, SurveyAnomalyRegistry, SurveyDimensionRegistry, SurveyInstrumentDef,
    SurveyInstrumentRegistry, SurveyMissionTemplate, SurveyMissionTemplates,
    SurveyMissionTemplatesFile,
};
pub use events::{
    AbortSurveyMission, DismissFailedMission, DismissSurveyMission, DispatchSurveyMission,
    SurveyEvent,
};
pub use milestones::{
    advance_construction_milestones, advance_research_milestones, advance_survey_milestones,
    is_probe_using_method, EarlyGameMilestones, MilestoneStep, MilestonesSystemSet,
    MILESTONE_ORDER,
};
pub use systems::{
    abort_survey_mission, advance_survey_missions, apply_continuous_station_bonus,
    decay_survey_confidence, dismiss_failed_mission, dismiss_survey_mission,
    dispatch_survey_mission, evaluate_landing_sites, process_analysis_queue,
    surface_anomaly_events, update_survey_summary, SimulationTime, ARCHIVE_LINGER_DAYS,
    INJURY_DURATION_DAYS,
};
pub use types::{
    axis_advance_rate_for_tier, mining_yield_delta_for_tier, AnomalyState, AnomalyType,
    EvidenceKind, EvidencePoint, FailureKind, FailureMode, MissionFailureReason, MissionStatus,
    SurveyDimension, SurveyMethod, CONFIDENCE_DECAY_PER_YEAR, DEFAULT_INJURY_DURATION_DAYS,
    DEFAULT_SOLAR_STORM_PENALTY, INITIAL_CONFIDENCE, MAX_TIER, STALE_CONFIDENCE,
    SURVEY_DAYS_PER_YEAR, WARNING_CONFIDENCE,
};
pub use visibility::{estimate_with_fidelity, is_stale, DepositEstimate, DepositVisibility};

/// Plugin that registers the survey system with the Bevy app.
///
/// PR-A registers the registries as default-initialized resources
/// and the system stubs. The stubs are no-ops in PR-A; they are
/// wired up in PR-B (missions) and PR-C (analysis queue).
pub struct SurveyPlugin;

impl Plugin for SurveyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Startup — load RON data. Mission templates land in
            // PR-B; other registries (dimensions, instruments,
            // anomalies, mining efficiency) load in follow-up
            // PRs.
            .add_systems(Startup, (load_mission_templates, load_recovery_missions))
            // Messages — registered in PR-B. The dispatch/abort
            // handlers and the tick system read/write these.
            // Bevy 0.18's `Message` derive replaces the older
            // `Event` derive; see
            // `src/economy/auto_build.rs` for the same pattern.
            .add_message::<SurveyEvent>()
            .add_message::<DispatchSurveyMission>()
            .add_message::<AbortSurveyMission>()
            .add_message::<DismissFailedMission>()
            // PR-F (GRA-117): player-driven archive of completed
            // missions from the dossier ACTIVE MISSIONS list. The
            // tick system's archive pass also handles time-based
            // pruning for any mission the player has ignored.
            .add_message::<DismissSurveyMission>()
            // Resources — default-initialized empty registries.
            // The RON loaders land in a follow-up PR; until then
            // the app starts with the hardcoded defaults from the
            // binary (the eight dimensions in `SurveyDimension::ALL`,
            // the nine methods in `SurveyMethod`, etc.).
            .init_resource::<SurveyDimensionRegistry>()
            .init_resource::<SurveyInstrumentRegistry>()
            .init_resource::<SurveyMissionTemplates>()
            .init_resource::<SurveyAnomalyRegistry>()
            .init_resource::<MiningEfficiencyRegistry>()
            .init_resource::<AnalysisQueueIndex>()
            .init_resource::<RecoveryMissionRegistry>()
            // GRA-787: the early-game milestone resource + reflective
            // registration. `init_resource` is the fresh-world
            // initialisation path the architecture points to — old
            // saves that lack the resource restore to `Default`
            // through this call (the snapshot is additive, so
            // `FORMAT_VERSION` does not need to bump). The
            // `register_type` call is what binds the type into
            // `AppTypeRegistry`, which `DynamicScene::from_world`
            // walks to discover reflected resources at save time.
            .init_resource::<EarlyGameMilestones>()
            .register_type::<EarlyGameMilestones>()
            .register_type::<MilestoneStep>()
            // Update systems — PR-A stubs remain, PR-B replaces
            // `advance_survey_missions` with the real tick and
            // adds the dispatch/abort handlers. The systems are
            // ordered so dispatch runs before advance: a mission
            // dispatched in frame N is available for the tick
            // system in frame N+1.
            //
            // Each of these systems takes `&mut World` rather than
            // separate `Res` / `Query` system params. Bevy 0.18
            // forbids two `Query<...>` params that both yield
            // mutable access to the same component (B0001), and
            // the tick / dispatch / abort handlers all need to
            // mutate scientists via a `QueryState` constructed on
            // the fly. Going through `&mut World` keeps the borrow
            // graph simple.
            .add_systems(
                Update,
                (
                    dispatch_survey_mission,
                    abort_survey_mission,
                    dismiss_failed_mission,
                    dismiss_survey_mission,
                    decay_survey_confidence,
                    advance_survey_missions,
                    process_analysis_queue,
                    surface_anomaly_events,
                    evaluate_landing_sites,
                    apply_continuous_station_bonus,
                    update_survey_summary,
                )
                    .chain(),
            )
            // GRA-787: three milestone consumers, one per source
            // message family. They run in `Update` after the sim
            // tick has emitted events for this frame. The set is
            // ordered after `NotificationsSystemSet::EventBridge`
            // (wired below) so the toast and the flag flip on the
            // same frame; the `.chain()` inside the tuple is for
            // documentation — the three systems target disjoint
            // message families.
            .add_systems(
                Update,
                (
                    advance_survey_milestones,
                    advance_construction_milestones,
                    advance_research_milestones,
                )
                    .in_set(MilestonesSystemSet)
                    .chain(),
            )
            // Cross-set ordering: bridge first, then milestones.
            // `NotificationsSystemSet::EventBridge` is configured in
            // `NotificationsPlugin`; the `.after` here documents
            // the intended pipeline even when the bridge plugin is
            // not registered (e.g. test harnesses).
            .configure_sets(
                Update,
                MilestonesSystemSet
                    .after(crate::ui::notifications::systems::NotificationsSystemSet::EventBridge),
            );
    }
}
