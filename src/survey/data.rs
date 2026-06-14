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
use crate::personnel::types::{ScientistId, ScientistSpecialty, SeniorityTier};

/// A trimmed scientist view for the survey recommendation heuristic.
///
/// GRA-112: the full `Scientist` component lives in the personnel
/// crate and is heavier than the scoring math needs. The recommender
/// only reads `id`, `specialty`, and `seniority`, so callers pass a
/// thin value-type view rather than the whole component. The dossier
/// UI (which has no per-body roster) passes `None`; the dispatch
/// picker in the Personnel menu (which has the active-body roster in
/// scope) passes `Some(&on_station_roster)`.
///
/// Per the LGD design brief (GRA-112 §1): "Mismatch is 0, not
/// negative." A scientist whose specialty does not match the
/// template's method contributes 0 to the bonus — the heuristic
/// never penalises a player for not having a specialist on station.
#[derive(Debug, Clone, Copy)]
pub struct ScientistSummary {
    pub id: ScientistId,
    pub specialty: ScientistSpecialty,
    pub seniority: SeniorityTier,
}

/// Why a particular mission template was recommended for a body.
///
/// GRA-114: the dossier SURVEY section now renders the *reason* a
/// template was picked, not just the pick itself. The enum is
/// closed (no modder surface) so the priority logic and the
/// `reason_text` helper in `src/ui/dossier_panel.rs` stay stable;
/// templates remain the modder surface.
///
/// Priority for the "single most-applicable reason" selection (per
/// LGD GRA-114 design contract):
/// 1. `SpecialistOnStation` — a roster scientist matches the
///    template's method AND contributes a non-zero roster bonus.
/// 2. `ConfidenceRescue` — primary dim's confidence is below
///    `WARNING_CONFIDENCE`.
/// 3. `CrossDim` — template covers ≥ 2 dimensions.
/// 4. `TierGap { from_tier, to_tier }` — a tier-gap win on the
///    primary dim.
/// 5. `BestFit` — fallback (zero score, zero gap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonTag {
    /// A scientist on the roster matches this template's method and
    /// is at high seniority (i.e. contributes a non-zero roster
    /// bonus). The variant carries the specialty so the dossier can
    /// name the discipline.
    SpecialistOnStation { specialty: ScientistSpecialty },
    /// Primary dim is below `WARNING_CONFIDENCE`; the survey
    /// rescues the dim's confidence back to healthy.
    ConfidenceRescue,
    /// Template covers multiple dimensions; one survey closes
    /// multiple gaps at once.
    CrossDim,
    /// Closes the largest tier gap on the primary dimension. The
    /// from/to tiers are the source of truth; the dossier surfaces
    /// them in the `reason_text` helper.
    TierGap { from_tier: u8, to_tier: u8 },
    /// Fallback when no other reason applies (e.g. all other
    /// factors are 0).
    BestFit,
}

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
    /// Per-dimension, per-day yield before team modifiers. PR-B
    /// (GRA-80) uses this to compute
    /// `delta_progress = yield * team_modifier * (1 - coverage) /
    /// total_days`. Defaults to `1.0` for templates that omit the
    /// field (and for legacy saves loaded before this column
    /// existed).
    #[serde(default = "default_template_yield")]
    pub axis_yield_per_day: f32,
    /// Whether this mission requires a scientist team on the
    /// ground. Used by the failure-roll table: ground-team missions
    /// are eligible for `CrewInjury` (2%). Set automatically based
    /// on the method at the loader (see
    /// [`SurveyMissionTemplate::is_ground_team`]) and stored on the
    /// template for fast dispatch-time lookup.
    #[serde(default)]
    pub is_ground_team: bool,
    /// Per-template failure modes (PR-G, GRA-85). When empty, the
    /// dispatch and finalise systems fall back to the hardcoded
    /// `MissionFailureReason::probability(method)` table from PR-B
    /// — so templates that pre-date PR-G (or modders that haven't
    /// added `failure_modes` to their RON yet) keep the design-
    /// doc rates without a Rust change. When non-empty, the typed
    /// roll in
    /// [`roll_typed_mission_outcome`](crate::survey::systems::roll_typed_mission_outcome)
    /// iterates this list and selects one entry with
    /// probability proportional to its `probability` field. The
    /// sum of the per-entry probabilities must be `<= 1.0`; the
    /// remainder is the success rate.
    #[serde(default)]
    pub failure_modes: Vec<crate::survey::types::FailureMode>,
    /// Optional ship-class gate. When `Some(id)`, the dispatch
    /// system in [`dispatch_survey_mission`](crate::survey::systems::dispatch_survey_mission)
    /// requires at least `requires_min_ship_count` ships of the
    /// matching hull class (`ship_hulls.ron` id) at the body's
    /// starmap location. When `None`, the gate is skipped
    /// (back-compat: legacy / modder RON rows without this field
    /// dispatch without a ship check). The RON edit that adds
    /// this field per template is a follow-on LGD PR — this
    /// Coder PR just adds the field, the gate, and the
    /// `MissionLaunchBlocked` event for the missing-ship case.
    #[serde(default)]
    pub requires_ship_class: Option<String>,
    /// Minimum count of `requires_ship_class` ships at the
    /// body's starmap location. Defaults to `1` (the loader
    /// applies this default even when the field is omitted from
    /// RON, so existing rows still gate single-ship templates).
    /// Set to `0` explicitly in RON to disable the count gate
    /// while keeping the class gate, or to opt out of both
    /// gates set `requires_ship_class: None`.
    #[serde(default = "default_min_ship_count")]
    pub requires_min_ship_count: u32,
    /// Minimum number of scientists the player must assign to
    /// the mission. `0` is the default (no gate), and matches
    /// the behaviour of solo probe missions. Ground-team
    /// templates should set this to `1` or more — the
    /// `is_ground_team` field is still the loader's primary
    /// signal, but the explicit count is the new
    /// design-doc-recommended contract.
    #[serde(default)]
    pub min_assigned_scientists: u32,
}

fn default_template_yield() -> f32 {
    1.0
}

fn default_min_ship_count() -> u32 {
    1
}

impl SurveyMissionTemplate {
    /// Heuristic: surface methods (Flyby, Orbital, RemoteSensing,
    /// AtmosphericProbe) are not ground-team. Everything else is.
    /// Used by the RON loader to set `is_ground_team` from the
    /// method if the field is missing.
    pub fn method_is_ground_team(method: SurveyMethod) -> bool {
        !matches!(
            method,
            SurveyMethod::Flyby
                | SurveyMethod::Orbital
                | SurveyMethod::RemoteSensing
                | SurveyMethod::AtmosphericProbe
        )
    }
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

/// On-disk shape of `assets/data/survey/missions.ron`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SurveyMissionTemplatesFile {
    pub templates: Vec<SurveyMissionTemplate>,
}

/// Load `assets/data/survey/missions.ron` into the
/// [`SurveyMissionTemplates`] resource. PR-B (GRA-80) is the first
/// PR to ship this loader; a future PR extends the same pattern
/// for `dimensions.ron`, `instruments.ron`, `anomalies.ron`, and
/// `mining_efficiency.ron`.
///
/// Returns silently on missing-file (the registry stays empty,
/// the app starts). Logs `warn!` on parse error so the dev can
/// spot a malformed RON.
pub fn load_mission_templates(mut commands: Commands) {
    let path = "assets/data/survey/missions.ron";
    let templates = match std::fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<SurveyMissionTemplatesFile>(&contents) {
            Ok(file) => file.templates,
            Err(error) => {
                warn!("Failed to parse {}: {}", path, error);
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    };
    let mut map: HashMap<String, SurveyMissionTemplate> = HashMap::new();
    for t in templates {
        // If the RON omits `is_ground_team`, infer from the
        // method. Keeps modder-edited RONs short.
        let t = SurveyMissionTemplate {
            is_ground_team: t.is_ground_team
                || SurveyMissionTemplate::method_is_ground_team(t.method),
            ..t
        };
        map.insert(t.id.clone(), t);
    }
    info!("Loaded {} survey mission templates", map.len());
    commands.insert_resource(SurveyMissionTemplates { templates: map });
}

// ── PR-G: RecoveryMission + RecoveryMissionRegistry ──────────────

/// What a recovery mission does. Drives the dossier UI's
/// "Recovery" CTA label and the recovery-mission icon. Maps to
/// the issue body's three recovery kinds
/// (`equipment_recovery`, `crew_extraction`,
/// `data_relay_replacement`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMissionKind {
    /// Send a retrieval ship / rover to recover stuck equipment
    /// (rover, drill rig, etc.). 1 chemical survey ship + 60-180
    /// sim-days for a stuck rover; 1 NTR ship + 1 sim-year for a
    /// stranded drill rig.
    EquipmentRecovery,
    /// Extract a stranded or injured crew member. Same ship
    /// profile as `EquipmentRecovery` but the player's incentive
    /// is personnel (the scientist's 90-day injury cooldown is
    /// the cost of failing to extract).
    CrewExtraction,
    /// Replace a lost probe or comms relay. Faster than the
    /// other two (the replacement is a fresh dispatch of the
    /// original mission template), so the recovery-mission
    /// `base_duration_days` is typically half the original
    /// template's duration.
    DataRelayReplacement,
}

impl RecoveryMissionKind {
    /// Stable RON id. Used in tests and for modder overrides.
    pub fn ron_id(self) -> &'static str {
        match self {
            RecoveryMissionKind::EquipmentRecovery => "equipment_recovery",
            RecoveryMissionKind::CrewExtraction => "crew_extraction",
            RecoveryMissionKind::DataRelayReplacement => "data_relay_replacement",
        }
    }

    /// Parse from a RON id. Returns `None` for unknown strings
    /// so modders can add new kinds without breaking the loader.
    pub fn from_ron_id(id: &str) -> Option<Self> {
        match id {
            "equipment_recovery" => Some(Self::EquipmentRecovery),
            "crew_extraction" => Some(Self::CrewExtraction),
            "data_relay_replacement" => Some(Self::DataRelayReplacement),
            _ => None,
        }
    }
}

/// One recovery mission template. Dispatched automatically when
/// the matching failure mode fires (`RoverStuck`,
/// `DrillBitStuck`) or by the dossier "DISPATCH RECOVERY"
/// button. The recovery mission runs as a regular
/// `ActiveSurveyMission` on the same body; on success the
/// `recover_of: Some(original_mission_id)` link flips the
/// original back from `Failed` to `Active`.
///
/// Loaded from `assets/data/survey/recovery_missions.ron` via
/// [`load_recovery_missions`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryMission {
    pub id: String,
    pub display_name: String,
    /// Drives the dossier UI label and the recovery-mission
    /// icon. Modders can introduce new kinds in code; RON
    /// loaders accept any id and fall back to
    /// `EquipmentRecovery` for unknown ones (with a `warn!`).
    pub kind: RecoveryMissionKind,
    /// Failure kinds this template can recover from. The auto-
    /// spawn path picks a recovery template by matching
    /// `failure_modes[i].recovery_mission_id` to `id`; the
    /// manual "DISPATCH RECOVERY" UI button shows a filtered
    /// list of templates whose `recovers_from` includes the
    /// original mission's failure reason.
    pub recovers_from: Vec<crate::survey::types::MissionFailureReason>,
    /// Typical mission duration in sim-days. The dispatch
    /// system uses this for the recovery mission's
    /// `expected_completion_sim_time`. The issue body's
    /// defaults: 60-180 sim-days for rover rescue, 1 sim-year
    /// for drill retrieval.
    pub base_duration_days: u32,
    /// Brief description for the dossier tooltip. Surfaced as
    /// a single-line hint under the recovery-mission row in
    /// the "FAILED MISSIONS" section.
    pub description: String,
}

/// Registry of recovery mission templates. Loaded from
/// `assets/data/survey/recovery_missions.ron` (PR-G).
///
/// Modders add new recovery templates by appending rows; the
/// loader inserts them into `missions` keyed by `id`. The
/// dispatch system consults the registry when a failure
/// auto-spawns a recovery mission, and the dossier UI uses
/// the same registry to populate the "DISPATCH RECOVERY"
/// button.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoveryMissionRegistry {
    /// Recovery mission definitions, keyed by RON id. Empty
    /// until `load_recovery_missions` runs.
    pub missions: HashMap<String, RecoveryMission>,
}

impl RecoveryMissionRegistry {
    /// Look up a recovery mission by RON id. Returns `None` for
    /// unknown ids so the auto-spawn path can `warn!` and fall
    /// through to the manual-dispatch path.
    pub fn get(&self, id: &str) -> Option<&RecoveryMission> {
        self.missions.get(id)
    }
}

/// On-disk shape of `assets/data/survey/recovery_missions.ron`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecoveryMissionsFile {
    pub missions: Vec<RecoveryMission>,
}

/// Load `assets/data/survey/recovery_missions.ron` into the
/// [`RecoveryMissionRegistry`] resource. Mirrors the
/// `load_mission_templates` pattern: returns silently on
/// missing-file, logs `warn!` on parse error.
///
/// Called from `SurveyPlugin::build` so the registry is
/// populated before any `advance_survey_missions` tick that
/// might need to look up a recovery template.
pub fn load_recovery_missions(mut commands: Commands) {
    let path = "assets/data/survey/recovery_missions.ron";
    let missions = match std::fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<RecoveryMissionsFile>(&contents) {
            Ok(file) => file.missions,
            Err(error) => {
                warn!("Failed to parse {}: {}", path, error);
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    };
    let mut map: HashMap<String, RecoveryMission> = HashMap::new();
    for m in missions {
        map.insert(m.id.clone(), m);
    }
    info!("Loaded {} recovery mission templates", map.len());
    commands.insert_resource(RecoveryMissionRegistry { missions: map });
}
