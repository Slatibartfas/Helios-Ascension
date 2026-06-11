//! Survey data registries.
//!
//! PR-A scaffold: registry types and `Resources` only. RON loading
//! (`load_dimensions`, `load_instruments`, `load_anomalies`,
//! `load_mission_templates`, `load_tiers`, `load_mining_efficiency`)
//! lands in PR-B alongside the new RON files in
//! `assets/data/survey/*.ron`. The empty registry defaults are
//! loadable so the app starts even before the RON files are written.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{SurveyDimension, SurveyMethod};

/// Registry of discovery dimensions. Loaded from
/// `assets/data/survey/dimensions.ron` (PR-B).
///
/// Empty by default — the eight hardcoded dimensions in
/// [`SurveyDimension::ALL`] are always known to the binary. The
/// registry exists so modders can add a ninth dimension
/// (e.g. "Magnetosphere") without recompiling Rust.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurveyDimensionRegistry {
    /// Modder-added dimensions, keyed by RON id. The eight hardcoded
    /// dimensions are always available even if not in this map.
    pub modder_dimensions: HashMap<String, ModderDimensionDef>,
}

/// One modder-defined survey dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModderDimensionDef {
    /// Stable RON id (e.g. "Magnetosphere").
    pub id: String,
    /// Display name for the dossier.
    pub display_name: String,
    /// Optional description shown in the dossier tooltip.
    pub description: String,
}

/// Registry of survey instruments. Loaded from
/// `assets/data/survey/instruments.ron` (PR-B).
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurveyInstrumentRegistry {
    /// Instrument definitions, keyed by RON id (e.g.
    /// "phased_array_radar"). Empty in PR-A.
    pub instruments: HashMap<String, SurveyInstrumentDef>,
}

/// One survey instrument — corresponds to a single RON row in
/// `assets/data/survey/instruments.ron`. See SURVEY_REWORK.md §5 for
/// the full schema; PR-B writes the rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyInstrumentDef {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub method: SurveyMethod,
    pub required_tech: Option<String>,
    /// Duration in sim-days for a typical mission using this
    /// instrument. Modders can override per-instrument.
    pub base_duration_days: u32,
    /// Number of scientists required to process the data.
    pub scientist_requirement: u32,
    /// Accuracy tier (0-5). Gates the resolution of the data
    /// returned.
    pub accuracy_tier: u8,
    /// Whether the instrument can surface anomalies.
    pub produces_anomalies: bool,
}

/// Registry of survey mission templates. Loaded from
/// `assets/data/survey/missions.ron` (PR-B).
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurveyMissionTemplates {
    /// Mission templates, keyed by RON id. Empty in PR-A.
    pub templates: HashMap<String, SurveyMissionTemplate>,
}

/// One survey mission template — a single "send probe" click. See
/// SURVEY_REWORK.md §5 (Mission templates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyMissionTemplate {
    pub id: String,
    pub display_name: String,
    pub method: SurveyMethod,
    /// RON id of the instrument (must exist in
    /// `SurveyInstrumentRegistry`).
    pub instrument_id: String,
    /// Dimensions advanced by this mission, with the target tier
    /// for each.
    pub target_tiers: HashMap<SurveyDimension, u8>,
    /// Typical mission duration in sim-days.
    pub base_duration_days: u32,
}

/// Registry of anomaly types. Loaded from
/// `assets/data/survey/anomalies.ron` (PR-D).
///
/// The nine hardcoded anomaly types in [`AnomalyType`] are always
/// known. The registry adds modder-defined types.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurveyAnomalyRegistry {
    /// Modder-added anomaly types, keyed by RON id. Empty in PR-A.
    pub modder_anomalies: HashMap<String, ModderAnomalyDef>,
}

/// One modder-defined anomaly type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModderAnomalyDef {
    pub id: String,
    pub display_name: String,
    pub description: String,
    /// Discovery method affinity — which method is most likely to
    /// surface this anomaly.
    pub discovery_method: SurveyMethod,
    /// "Coolness" weight for media coverage (0.0–1.0).
    pub coolness: f32,
}

/// Per-(resource_class, dimension, tier) mining efficiency curve.
/// Loaded from `assets/data/survey/mining_efficiency.ron` (PR-B).
///
/// Powers SURVEY_REWORK.md §11 (Resource Reveal Matrix). The default
/// curve is hardcoded below so the app starts without the RON file.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct MiningEfficiencyRegistry {
    /// Mining efficiency rows, keyed by RON id (e.g.
    /// "proven_crustal_min_deposits_t2"). Empty in PR-A.
    pub rows: HashMap<String, MiningEfficiencyRow>,
}

/// One row in the mining efficiency curve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningEfficiencyRow {
    pub id: String,
    /// Resource class tag (matches a tag in `solar_system.ron`'s
    /// deposit entries). E.g. "ShallowOre", "DeepOre",
    /// "AtmosphericGas", "TraceIsotope".
    pub resource_class: String,
    /// Dimension that gates this row.
    pub dimension: SurveyDimension,
    /// Minimum tier at which mining unlocks for this
    /// (resource_class, dimension) pair.
    pub min_tier: u8,
    /// Efficiency in `[0.0, 1.0]` of nominal yield at `min_tier`.
    /// Modders can lower the early-game yield by setting this to
    /// 0.25 (default: 0.40 per SURVEY_REWORK.md §11).
    pub efficiency_pct: f32,
    /// Whether a follow-up confirmation (e.g. drill rig) is
    /// required before mining actually starts.
    pub requires_confirmation: bool,
}

/// Fast lookup of analysis jobs by scientist and by body. Lives in
/// memory only; rebuilt from `Vec<AnalysisJob>` on save-load.
///
/// Empty in PR-A — the analysis queue (PR-C) populates this.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisQueueIndex {
    /// Active jobs, keyed by job id.
    pub jobs_by_id: HashMap<u64, AnalysisJobRef>,
    /// Job ids assigned to each scientist. Empty in PR-A.
    pub jobs_by_scientist: HashMap<u64, Vec<u64>>,
    /// Job ids for each body. Empty in PR-A.
    pub jobs_by_body: HashMap<Entity, Vec<u64>>,
}

/// Lightweight handle into an [`AnalysisJob`](super::components::AnalysisJob)
/// stored elsewhere (e.g. on a body's [`SurveyState`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJobRef {
    pub job_id: u64,
    pub body: Entity,
    pub assigned_scientist: Option<u64>,
}
