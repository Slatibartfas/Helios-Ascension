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

use super::types::{
    default_method_specificity, SurveyDimension, SurveyMethod, DEFAULT_ACTIVATION_THRESHOLD,
};

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
/// `assets/data/survey/anomalies.ron`.
///
/// Holds the 19 hardcoded rows (keyed by `AnomalyType::ron_id`) plus
/// any modder-added rows. The merged view is in `all`; the halves
/// are kept separate for save-load diffability.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurveyAnomalyRegistry {
    /// Hardcoded rows keyed by `AnomalyType::ron_id`. Populated by
    /// `load_anomalies` from `assets/data/survey/anomalies.ron`.
    pub hardcoded: HashMap<String, AnomalyDef>,
    /// Modder rows keyed by RON id.
    pub modder_anomalies: HashMap<String, ModderAnomalyDef>,
    /// `AnomalyDef` lookup that includes modder rows. Rebuilt on
    /// save-load from the two maps above.
    pub all: HashMap<String, AnomalyDef>,
}

impl SurveyAnomalyRegistry {
    /// Look up a row by RON id across both hardcoded and modder
    /// tables. Returns `None` for unknown ids.
    pub fn get(&self, id: &str) -> Option<&AnomalyDef> {
        self.all.get(id)
    }

    /// Iterate every row in registry order (hardcoded first, then
    /// modder rows in insertion order).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AnomalyDef)> {
        self.all.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Rebuild the merged `all` map from the hardcoded and modder
    /// halves. Called by `load_anomalies` and by save-load paths.
    pub fn rebuild_index(&mut self) {
        self.all.clear();
        for (k, v) in &self.hardcoded {
            self.all.insert(k.clone(), v.clone());
        }
        for (k, modder) in &self.modder_anomalies {
            // Modder rows that don't collide with a hardcoded id
            // become their own AnomalyDef using the
            // ModderAnomalyDef shape.
            if self.all.contains_key(k) {
                continue;
            }
            self.all.insert(
                k.clone(),
                AnomalyDef {
                    id: k.clone(),
                    display_name: modder.display_name.clone(),
                    description: modder.description.clone(),
                    detection_axes: Vec::new(),
                    detection_threshold: 2,
                    false_positive_rate: 0.10,
                    activation_threshold: DEFAULT_ACTIVATION_THRESHOLD,
                    evidence_methods: vec![modder.discovery_method],
                    method_specificity: HashMap::new(),
                    effect: AnomalyEffect::None,
                    coolness: modder.coolness,
                },
            );
        }
    }
}

/// One modder-defined anomaly type. Modders add new types by
/// appending rows to the `modder_anomalies` array in
/// `anomalies.ron`; the loader turns them into `AnomalyDef` rows
/// in `SurveyAnomalyRegistry::all`.
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

/// Effect a verified anomaly has on the game. Modders and LGD pick
/// one variant per anomaly. The Coder routes the effect to the right
/// subsystem (research, buildings, events) at activation time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnomalyEffect {
    /// Anomaly activates nothing — a flavor-only discovery.
    #[default]
    None,
    /// Adds `building_id` to the available-buildings list at the
    /// activated body. Tech is NOT bypassed: the building's
    /// `required_tech` (if any) must still be researched.
    UnlocksBuilding { building_id: String },
    /// Adds `tech_id` to the unlocked-technologies set.
    UnlocksTech { tech_id: String },
    /// Triggers an in-game event chain by id. The notification
    /// surface picks the event up via the `SurveyEvent` message.
    TriggersEvent { event_id: String },
    /// Adds a one-time research bonus (percentage) to all RP
    /// generated for the duration of the campaign.
    ResearchBonus { percentage: f32 },
}

/// One row in `assets/data/survey/anomalies.ron`. Loaded by
/// `load_anomalies` and indexed by RON id (e.g. `magnetic_anomaly`).
///
/// Drives the per-tick detection roll (axes + threshold +
/// false_positive_rate) and the confidence ramp
/// (activation_threshold + evidence_methods + per-method specificity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDef {
    /// Stable RON id, matches `AnomalyType::ron_id` for hardcoded
    /// types or a free-form slug for modder-added ones.
    pub id: String,
    /// Display name for the dossier (e.g. "Magnetic Anomaly").
    pub display_name: String,
    /// Short description (≤ 200 chars) shown in the tooltip.
    pub description: String,
    /// Dimensions whose coverage gates whether the anomaly can be
    /// detected on a body. The detection roll runs when every axis
    /// in this list is at `tier ≥ detection_threshold`.
    pub detection_axes: Vec<SurveyDimension>,
    /// Minimum tier required on every detection axis.
    pub detection_threshold: u8,
    /// Per-detection-roll probability of a false positive. Modders
    /// override per anomaly; the design range is `[0.05, 0.30]`.
    /// A "real" detection is `1 - false_positive_rate`.
    pub false_positive_rate: f32,
    /// Base activation threshold (confidence required for
    /// `Verified`). Retry pressure can drop this down to
    /// `MIN_ACTIVATION_THRESHOLD`.
    #[serde(default = "default_activation_threshold_value")]
    pub activation_threshold: f32,
    /// Methods whose verification mission is most appropriate.
    /// Drives the per-method specificity table.
    pub evidence_methods: Vec<SurveyMethod>,
    /// Per-method evidence specificity multiplier. Used as the
    /// multiplier on `VERIFICATION_CONFIDENCE_BUMP`. Defaults to
    /// `default_method_specificity(method)` for any method the
    /// modder leaves out.
    #[serde(default)]
    pub method_specificity: HashMap<SurveyMethod, f32>,
    /// Gameplay effect on activation. See [`AnomalyEffect`].
    #[serde(default)]
    pub effect: AnomalyEffect,
    /// "Coolness" weight for media coverage (0.0–1.0).
    #[serde(default)]
    pub coolness: f32,
}

fn default_activation_threshold_value() -> f32 {
    DEFAULT_ACTIVATION_THRESHOLD
}

impl AnomalyDef {
    /// Resolve the per-method specificity for `method`, falling back
    /// to [`default_method_specificity`].
    pub fn specificity_for(&self, method: SurveyMethod) -> f32 {
        self.method_specificity
            .get(&method)
            .copied()
            .unwrap_or_else(|| default_method_specificity(method))
    }
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

// ── RON loader ──────────────────────────────────────────────────────

use std::fs;

/// Top-level shape of `assets/data/survey/anomalies.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnomaliesFile {
    hardcoded: Vec<AnomalyDef>,
    #[serde(default)]
    modder_anomalies: Vec<ModderAnomalyDef>,
}

/// System to load anomaly definitions from `assets/data/survey/anomalies.ron`
/// at startup. Populates [`SurveyAnomalyRegistry`] and rebuilds the merged
/// `all` index. Missing or malformed rows are warned; the app still
/// starts with whatever the binary-recognized enum knows about.
pub fn load_anomalies(mut commands: Commands) {
    info!("Loading anomaly definitions...");
    let path = "assets/data/survey/anomalies.ron";

    let mut registry = SurveyAnomalyRegistry::default();
    let mut hardcoded_count = 0usize;
    let mut modder_count = 0usize;

    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<AnomaliesFile>(&contents) {
            Ok(file) => {
                for def in file.hardcoded {
                    hardcoded_count += 1;
                    registry.hardcoded.insert(def.id.clone(), def);
                }
                for modder in file.modder_anomalies {
                    modder_count += 1;
                    registry.modder_anomalies.insert(modder.id.clone(), modder);
                }
                registry.rebuild_index();
                info!(
                    "Loaded {} hardcoded + {} modder anomalies ({} total)",
                    hardcoded_count,
                    modder_count,
                    registry.all.len()
                );
            }
            Err(e) => {
                warn!(
                    "Failed to parse {path}: {e}. The app will start with an empty registry; \
                     detection rolls will be no-ops until the file is fixed."
                );
            }
        },
        Err(e) => {
            warn!(
                "Could not read {path}: {e}. The app will start with an empty registry; \
                 detection rolls will be no-ops until the file is written."
            );
        }
    }

    commands.insert_resource(registry);
}
