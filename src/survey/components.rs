//! Survey system components.
//!
//! These components live on celestial body entities. PR-A adds them as
//! **optional** components — bodies keep their existing `SurveyLevel`
//! enum and `discovered_amount()` is the source of truth until Phase 5
//! of the migration plan (SURVEY_REWORK.md §15). The migration shim is
//! [`SurveyState::from_legacy_level`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{
    AnomalyState, AnomalyType, EvidenceKind, EvidencePoint, SurveyDimension, SurveyMethod,
    DEFAULT_ACTIVATION_THRESHOLD, INITIAL_CONFIDENCE, MAX_TIER, MIN_ACTIVATION_THRESHOLD,
    REFUTATION_REARM_THRESHOLD, RETRY_PRESSURE_PER_VERIFICATION,
    RETRY_PRESSURE_THRESHOLD_REDUCTION, WARNING_CONFIDENCE,
};
use crate::colony::types::BuildingType;
use crate::economy::components::SurveyLevel;

/// Per-dimension survey fidelity on a body.
///
/// Replaces the single-axis `SurveyLevel` enum with a multi-axis
/// structure: a body has eight independent discovery dimensions, each at
/// its own tier (0–5) with its own confidence (0.0–1.0). Confidence rises
/// with more measurements and falls with time (see
/// [`CONFIDENCE_DECAY_PER_YEAR`](super::types::CONFIDENCE_DECAY_PER_YEAR)).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DimensionFidelity {
    /// Resolution tier for this dimension. 0 = unknown, 5 = fully
    /// characterized. Tier semantics are dimension-specific and live in
    /// `assets/data/survey/tiers.ron`.
    pub tier: u8,
    /// Sim-time of the most recent measurement. `None` means the
    /// dimension has never been measured. Used for confidence decay.
    pub last_measured_sim_time: Option<f64>,
    /// Confidence in the current tier reading. Rises with measurements,
    /// falls at `CONFIDENCE_DECAY_PER_YEAR` per sim-year. At confidence
    /// ≤ 0.1 the data is "stale" and a re-survey is recommended.
    pub confidence: f32,
}

impl Default for DimensionFidelity {
    fn default() -> Self {
        Self {
            tier: 0,
            last_measured_sim_time: None,
            confidence: 0.0,
        }
    }
}

impl DimensionFidelity {
    /// A fresh, never-measured dimension (tier 0, confidence 0.0).
    pub const UNKNOWN: Self = Self {
        tier: 0,
        last_measured_sim_time: None,
        confidence: 0.0,
    };

    /// Build a fidelity entry that has just been measured at `tier` with
    /// the default initial confidence. Used by the analysis queue on
    /// completion of a survey job.
    pub fn freshly_measured(tier: u8, sim_time: f64) -> Self {
        Self {
            tier: tier.min(MAX_TIER),
            last_measured_sim_time: Some(sim_time),
            confidence: INITIAL_CONFIDENCE,
        }
    }

    /// Build a fidelity at a given tier and confidence. Used by the
    /// migration shim and by save-load round-trips.
    pub fn at_tier(tier: u8, confidence: f32, sim_time: Option<f64>) -> Self {
        Self {
            tier: tier.min(MAX_TIER),
            last_measured_sim_time: sim_time,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// Whether this dimension is "known" (tier ≥ 1).
    pub fn is_known(&self) -> bool {
        self.tier > 0
    }

    /// Whether the data is stale (confidence below the warning
    /// threshold).
    pub fn is_stale(&self) -> bool {
        self.confidence < WARNING_CONFIDENCE
    }

    /// Whether the dimension has been fully characterized (tier 5 with
    /// confidence ≥ 0.8).
    pub fn is_fully_characterized(&self) -> bool {
        self.tier >= MAX_TIER && self.confidence >= 0.8
    }
}

/// Per-body survey state. Replaces the old `SurveyLevel` enum (kept
/// during the Phase 1–4 migration window).
///
/// `SurveyState` carries:
/// - the dimension map ([`SurveyDimension`] → [`DimensionFidelity`])
/// - the list of currently-running missions
/// - a sim-time stamp for confidence decay scheduling
/// - the cumulative science investment (for the dossier "Science
///   Points" readout and the system summary)
#[derive(Debug, Clone, Component, Serialize, Deserialize)]
pub struct SurveyState {
    /// Per-dimension fidelity. Dimensions not present in the map are
    /// treated as [`DimensionFidelity::UNKNOWN`] (tier 0, confidence
    /// 0.0). The map is sparse to keep save size small — most bodies
    /// have 0–3 known dimensions, not 8.
    pub dimensions: HashMap<SurveyDimension, DimensionFidelity>,
    /// Currently-running survey missions on this body. The
    /// `advance_survey_missions` system ticks these.
    pub active_missions: Vec<ActiveSurveyMission>,
    /// Sim-time of the last survey state update. Used for confidence
    /// decay bookkeeping.
    pub last_updated_sim_time: f64,
    /// Total science points invested on this body across all
    /// instruments and missions.
    pub total_science_points_invested: f64,
    /// Anomalies detected on this body. Persists across the migration
    /// window so that reloading a v0.4.x save preserves any anomalies
    /// the player discovered (currently always empty in PR-A — anomaly
    /// events land in PR-D).
    pub detected_anomalies: Vec<DetectedAnomaly>,
    /// Candidate landing sites on this body. Populated by the
    /// `evaluate_landing_sites` system (PR-D) when the body is a
    /// planet / dwarf planet / moon and the
    /// `landing_site_eval_coverage` crosses the
    /// [`LANDING_SITE_EVAL_THRESHOLD`]. Empty until then.
    pub landing_sites: Vec<LandingSite>,
    /// Candidate extraction sites on this body. Populated by
    /// `evaluate_landing_sites` for asteroids at the same coverage
    /// threshold. Gas giants stay empty (no sites are generated
    /// for them — see GRA-82 acceptance).
    pub extraction_sites: Vec<ExtractionSite>,
    /// Sim-time of the last landing-site evaluation. Used by the
    /// system to throttle the regeneration check to once per
    /// simulation day (avoids re-rolling sites every frame as
    /// confidence rises).
    pub last_landing_site_eval_sim_time: f64,
}

impl Default for SurveyState {
    fn default() -> Self {
        Self {
            dimensions: HashMap::new(),
            active_missions: Vec::new(),
            last_updated_sim_time: 0.0,
            total_science_points_invested: 0.0,
            detected_anomalies: Vec::new(),
            landing_sites: Vec::new(),
            extraction_sites: Vec::new(),
            last_landing_site_eval_sim_time: 0.0,
        }
    }
}

impl SurveyState {
    /// A fresh, fully-unsurveyed state. Equivalent to
    /// `SurveyLevel::Unsurveyed`.
    pub fn unsurveyed() -> Self {
        Self::default()
    }

    /// A state where every dimension is at tier 5 with full
    /// confidence. Used by the migration shim for bodies that were at
    /// `SurveyLevel::CoreSample` (e.g. Earth in
    /// `src/plugins/solar_system.rs`).
    pub fn fully_surveyed(sim_time: f64) -> Self {
        let mut dimensions = HashMap::new();
        for dim in SurveyDimension::ALL {
            dimensions.insert(
                dim,
                DimensionFidelity::at_tier(MAX_TIER, 1.0, Some(sim_time)),
            );
        }
        Self {
            dimensions,
            active_missions: Vec::new(),
            last_updated_sim_time: sim_time,
            total_science_points_invested: 0.0,
            detected_anomalies: Vec::new(),
            landing_sites: Vec::new(),
            extraction_sites: Vec::new(),
            last_landing_site_eval_sim_time: 0.0,
        }
    }

    /// Migration shim: map a legacy `SurveyLevel` to a `SurveyState`.
    /// See SURVEY_REWORK.md §15 (Backward compat) for the mapping
    /// table.
    pub fn from_legacy_level(level: SurveyLevel, sim_time: f64) -> Self {
        let mut dimensions = HashMap::new();
        match level {
            SurveyLevel::Unsurveyed => {}
            SurveyLevel::OrbitalScan => {
                dimensions.insert(
                    SurveyDimension::OrbitalMech,
                    DimensionFidelity::freshly_measured(1, sim_time),
                );
                dimensions.insert(
                    SurveyDimension::Atmosphere,
                    DimensionFidelity::freshly_measured(1, sim_time),
                );
                dimensions.insert(
                    SurveyDimension::MineralClasses,
                    DimensionFidelity::freshly_measured(1, sim_time),
                );
            }
            SurveyLevel::SeismicSurvey => {
                for dim in [
                    SurveyDimension::OrbitalMech,
                    SurveyDimension::Atmosphere,
                    SurveyDimension::MineralClasses,
                ] {
                    dimensions.insert(dim, DimensionFidelity::freshly_measured(1, sim_time));
                }
                dimensions.insert(
                    SurveyDimension::Subsurface,
                    DimensionFidelity::freshly_measured(2, sim_time),
                );
            }
            SurveyLevel::CoreSample => {
                return Self::fully_surveyed(sim_time);
            }
        }
        Self {
            dimensions,
            active_missions: Vec::new(),
            last_updated_sim_time: sim_time,
            total_science_points_invested: 0.0,
            detected_anomalies: Vec::new(),
            landing_sites: Vec::new(),
            extraction_sites: Vec::new(),
            last_landing_site_eval_sim_time: 0.0,
        }
    }

    /// Look up a dimension's fidelity, returning `UNKNOWN` for
    /// dimensions not yet in the map. The map is sparse on purpose.
    pub fn fidelity(&self, dim: SurveyDimension) -> DimensionFidelity {
        self.dimensions
            .get(&dim)
            .copied()
            .unwrap_or(DimensionFidelity::UNKNOWN)
    }

    /// Mutate a dimension's fidelity. Creates the entry if absent.
    pub fn set_fidelity(&mut self, dim: SurveyDimension, fidelity: DimensionFidelity) {
        self.dimensions.insert(dim, fidelity);
    }

    /// Weighted-average tier across all dimensions, used by the system
    /// summary "SURVEY %" stat. Result is in `0.0..=1.0` (divide by
    /// `MAX_TIER` to get a percentage).
    pub fn average_tier(&self) -> f32 {
        if self.dimensions.is_empty() {
            return 0.0;
        }
        let sum: u32 = self.dimensions.values().map(|f| f.tier as u32).sum();
        // Average across all 8 dimensions, not just the ones the map
        // happens to know about — a body with 3 known dimensions is
        // more surveyed than a body with 0.
        let denom = SurveyDimension::ALL.len() as u32;
        (sum as f32) / (denom as f32 * MAX_TIER as f32)
    }

    /// Number of dimensions at tier ≥ 1.
    pub fn known_dimension_count(&self) -> usize {
        self.dimensions.values().filter(|f| f.is_known()).count()
    }

    /// Number of dimensions fully characterized (tier 5, confidence
    /// ≥ 0.8).
    pub fn fully_characterized_count(&self) -> usize {
        self.dimensions
            .values()
            .filter(|f| f.is_fully_characterized())
            .count()
    }

    /// Whether at least one mission is currently running.
    pub fn has_active_missions(&self) -> bool {
        !self.active_missions.is_empty()
    }

    /// Landing-site evaluation coverage in `0.0..=1.0`.
    ///
    /// Derived (not stored) from the per-dimension tiers: site
    /// evaluation needs surface + habitability + atmosphere data. The
    /// weighting matches the 0.6 threshold requirement in GRA-82 —
    /// SurfaceFeatures dominates because slope/roughness/regolith all
    /// come from the surface characterization, with Habitability and
    /// Atmosphere as supporting axes. Other dimensions contribute a
    /// small slice (10% combined) so that a body with all surface +
    /// habitability data but no mineral survey still crosses the
    /// threshold at high coverage.
    ///
    /// Result is in `0.0..=1.0`; callers compare against
    /// [`LANDING_SITE_EVAL_THRESHOLD`].
    pub fn landing_site_eval_coverage(&self) -> f32 {
        const W_SURFACE: f32 = 0.5;
        const W_HABITABILITY: f32 = 0.25;
        const W_ATMOSPHERE: f32 = 0.15;
        const W_OTHER: f32 = 0.10;

        let norm =
            |dim: SurveyDimension| -> f32 { (self.fidelity(dim).tier as f32) / (MAX_TIER as f32) };

        let surface = norm(SurveyDimension::SurfaceFeatures);
        let hab = norm(SurveyDimension::Habitability);
        let atmo = norm(SurveyDimension::Atmosphere);

        // Other 5 dimensions equally share the W_OTHER slice.
        let other_dims = [
            SurveyDimension::OrbitalMech,
            SurveyDimension::MineralClasses,
            SurveyDimension::MineralDeposits,
            SurveyDimension::Subsurface,
            SurveyDimension::Anomalies,
        ];
        let other_sum: f32 = other_dims.iter().map(|d| norm(*d)).sum();
        let other_avg = other_sum / (other_dims.len() as f32);

        (surface * W_SURFACE + hab * W_HABITABILITY + atmo * W_ATMOSPHERE + other_avg * W_OTHER)
            .clamp(0.0, 1.0)
    }
}

/// A running survey mission on a body. Lives on [`SurveyState`].
///
/// PR-A: the struct is defined for shape stability, but no system
/// creates or ticks missions yet — that lands in PR-B (instruments)
/// and PR-C (analysis queue).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSurveyMission {
    /// Stable id so the UI can correlate a row in the "Active Missions"
    /// list with the underlying mission.
    pub id: u64,
    /// Display name (e.g. "Mare Imbrium 1", "Ariel-2"). Free-form.
    pub name: String,
    /// Method used by this mission.
    pub method: SurveyMethod,
    /// Sim-time the mission was launched.
    pub launched_sim_time: f64,
    /// Sim-time the mission is expected to complete. The
    /// `advance_survey_missions` system ticks the mission toward this
    /// timestamp; on completion, the mission is removed and a
    /// [`AnalysisJob`] is enqueued.
    pub expected_completion_sim_time: f64,
    /// Mission progress in `[0.0, 1.0]`. The dossier UI shows this as
    /// a bar.
    pub progress: f32,
}

/// A pending data analysis job. The analysis queue (PR-C) drives these.
///
/// While a job is unassigned, the underlying data sits unprocessed.
/// When a scientist is assigned, the job's `progress` advances at a
/// rate determined by the scientist's seniority × specialty match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    /// Stable id (also used by [`Scientist::current_analysis`](
    /// crate::personnel::components::Scientist::current_analysis)).
    pub id: u64,
    /// Body the data was collected from.
    pub body: Entity,
    /// Mission that produced the data. Optional because a job can be
    /// created from sources other than an active mission (e.g. a
    /// one-off lab analysis).
    pub source_mission: Option<u64>,
    /// Display label for the dossier / analysis queue.
    pub label: String,
    /// Method that produced the data. Drives the specialty-match
    /// multiplier.
    pub method: SurveyMethod,
    /// Sim-time the job was enqueued.
    pub enqueued_sim_time: f64,
    /// Sim-time the job completed. `None` while in flight.
    pub completed_sim_time: Option<f64>,
    /// Progress in `[0.0, 1.0]`. Reaches 1.0 on completion.
    pub progress: f32,
    /// Whether an anomaly was flagged by this analysis. Set by the
    /// analysis queue on completion if a discovery method affinity
    /// matches an anomaly present on the body.
    pub anomaly_flagged: Option<AnomalyType>,
}

/// An anomaly that has been detected and logged on a body's dossier.
///
/// PR-C extends the r1 shape with the r2 confidence model: state,
/// per-anomaly `activation_threshold`, an `evidence: Vec<EvidencePoint>`
/// trail, `retry_pressure` (the player leaning in with more missions),
/// and a `last_updated_sim_time` stamp for retry-pressure decay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAnomaly {
    /// Anomaly type. Modders add new types via `anomalies.ron`.
    pub anomaly_type: AnomalyType,
    /// Sim-time of detection.
    pub detected_sim_time: f64,
    /// Sim-time of the most recent evidence point. Used to drive
    /// `retry_pressure` decay and the "time since last data point"
    /// dossier readout.
    pub last_updated_sim_time: f64,
    /// Lifecycle state. `Suspected` → `Verified` (or `Refuted`).
    pub state: AnomalyState,
    /// Confidence in the detection (rises with re-observation,
    /// capped at `MAX_CONFIDENCE`).
    pub confidence: f32,
    /// Base activation threshold (loaded from `anomalies.ron`).
    /// Effective threshold = this minus `retry_pressure ×
    /// RETRY_PRESSURE_THRESHOLD_REDUCTION`, clamped to
    /// `MIN_ACTIVATION_THRESHOLD`.
    pub activation_threshold: f32,
    /// Player-applied retry pressure. Each additional verification
    /// mission adds `RETRY_PRESSURE_PER_VERIFICATION`; decays at
    /// `RETRY_PRESSURE_DECAY_PER_YEAR` per sim-year.
    pub retry_pressure: f32,
    /// Number of verification missions that have been dispatched.
    /// Powers the dossier's "Tries" badge.
    pub verification_count: u32,
    /// Trail of every evidence point that contributed to the
    /// confidence total. UI shows the latest 3.
    pub evidence: Vec<EvidencePoint>,
    /// Whether the player has acknowledged the anomaly in the dossier
    /// (used to suppress repeat notifications).
    pub acknowledged: bool,
}

/// Coverage threshold below which no landing / extraction sites are
/// generated. Mirrors the 0.6 trigger called out in GRA-82.
pub const LANDING_SITE_EVAL_THRESHOLD: f32 = 0.6;

/// Min / max count of candidate sites generated per body when
/// coverage crosses the threshold. Bounds the visible list in the
/// dossier to 2–5 rows by default (see GRA-82 acceptance criteria).
pub const MIN_SITES_PER_BODY: usize = 2;
pub const MAX_SITES_PER_BODY: usize = 5;

/// Per-site subscores that feed the composite. All values are
/// `0.0..=1.0` where 1.0 is the most favorable for the player.
///
/// The composite is the weighted average defined in
/// [`LandingSite::composite_score`]; the dossier surfaces both the
/// composite and the per-axis bars so the player can see why a site
/// ranks where it does.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SiteScores {
    /// Surface slope (1.0 = flat, 0.0 = impassable cliffs).
    pub slope: f32,
    /// Surface roughness (1.0 = smooth regolith, 0.0 = boulder field).
    pub roughness: f32,
    /// Background radiation (1.0 = safe, 0.0 = lethal).
    pub radiation: f32,
    /// Surface temperature (1.0 = in life-support band, 0.0 = cryogenic
    /// or molten).
    pub temperature: f32,
    /// Regolith quality for foundations / excavation (1.0 = solid
    /// bedrock with shallow regolith, 0.0 = unconsolidated dust).
    pub regolith: f32,
    /// Comm line-of-sight to the parent body / relay (1.0 = always
    /// visible, 0.0 = occluded most of the day).
    pub comm: f32,
}

impl SiteScores {
    /// Default weights for the composite (see SURVEY_REWORK follow-up
    /// spec for GRA-82). Sum is 1.0; tweak here to rebalance the
    /// ranking without changing the data model.
    pub const WEIGHTS: SiteScoreWeights = SiteScoreWeights {
        slope: 0.20,
        roughness: 0.15,
        radiation: 0.20,
        temperature: 0.20,
        regolith: 0.15,
        comm: 0.10,
    };

    /// Weighted average. Each component is clamped to `0.0..=1.0`
    /// defensively (a buggy RON bias should never produce a score
    /// outside the display range).
    pub fn composite(&self) -> f32 {
        let w = Self::WEIGHTS;
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        clamp(self.slope) * w.slope
            + clamp(self.roughness) * w.roughness
            + clamp(self.radiation) * w.radiation
            + clamp(self.temperature) * w.temperature
            + clamp(self.regolith) * w.regolith
            + clamp(self.comm) * w.comm
    }
}

/// Plain-old-data record of the composite weights. Kept separate from
/// [`SiteScores`] so the const initializer can be a public constant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SiteScoreWeights {
    pub slope: f32,
    pub roughness: f32,
    pub radiation: f32,
    pub temperature: f32,
    pub regolith: f32,
    pub comm: f32,
}

/// A candidate landing site on a planet or moon. Generated by
/// [`evaluate_landing_sites`](super::systems::evaluate_landing_sites)
/// when a body's [`SurveyState::landing_site_eval_coverage`] crosses
/// the [`LANDING_SITE_EVAL_THRESHOLD`].
///
/// Sites are immutable once generated: when survey improves, the
/// composite ranking of *known* sites stays stable. Re-survey work
/// would land in a follow-up PR (likely PR-F "mining yield" /
//// dossier refresh).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandingSite {
    /// Stable id (unique per body, but not globally). Index into the
    /// parent [`SurveyState::landing_sites`] vec, kept explicit so
    /// save/load round-trips don't depend on Vec order.
    pub id: u32,
    /// Display name (e.g. "Mare Imbrium Alpha", "Hellas Basin — Site
    /// 2"). Generated from a stable name table plus the body name.
    pub name: String,
    /// Latitude in degrees (-90..=90). Stored in the body-local
    /// rotating frame; the starmap focus call projects to ecliptic
    /// using the body's `AxialTilt` (PR-E work).
    pub latitude: f32,
    /// Longitude in degrees (-180..=180).
    pub longitude: f32,
    /// Per-axis sub-scores + weighted composite.
    pub scores: SiteScores,
    /// Buildings that can be constructed at this site.
    pub feasible_for: Vec<BuildingType>,
    /// Buildings that are explicitly blocked (e.g. HeavyIndustry on
    /// steep slopes). Surfaced in the dossier as a tooltip warning.
    pub blockers: Vec<BuildingType>,
}

impl LandingSite {
    /// Convenience: the composite score for this site.
    pub fn composite_score(&self) -> f32 {
        self.scores.composite()
    }
}

/// A candidate extraction site on an asteroid. Same data shape as
/// [`LandingSite`] but with a different default
/// `feasible_for` list (mining equipment, mass-driver pads).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionSite {
    /// See [`LandingSite::id`].
    pub id: u32,
    /// See [`LandingSite::name`].
    pub name: String,
    /// See [`LandingSite::latitude`].
    pub latitude: f32,
    /// See [`LandingSite::longitude`].
    pub longitude: f32,
    /// Per-axis sub-scores (slope/roughness/regolith are the dominant
    /// drivers on an asteroid; radiation/temperature/comm matter
    /// less, but the same six-axis schema is reused so the UI
    /// table renders the same way).
    pub scores: SiteScores,
    /// Mining / extraction buildings feasible here.
    pub feasible_for: Vec<BuildingType>,
    /// See [`LandingSite::blockers`].
    pub blockers: Vec<BuildingType>,
}

impl ExtractionSite {
    /// Convenience: the composite score for this site.
    pub fn composite_score(&self) -> f32 {
        self.scores.composite()
    }
}

impl DetectedAnomaly {
    /// Build a fresh `Suspected` anomaly from a successful detection
    /// roll. Used by `surface_anomaly_events` on the first pass through
    /// `false_positive_rate`. Evidence is seeded with a single data
    /// point whose `axis_match_count` is whatever the caller observed
    /// at detection time.
    pub fn detected(
        anomaly_type: AnomalyType,
        sim_time: f64,
        base_threshold: f32,
        axis_match_count: u8,
    ) -> Self {
        let initial_confidence = (super::types::DATA_POINT_CONFIDENCE_BUMP
            * axis_match_count as f32)
            .min(super::types::MAX_CONFIDENCE);
        let evidence = vec![EvidencePoint {
            kind: EvidenceKind::DataPoint,
            sim_time,
            axis_match_count,
            magnitude: super::types::DATA_POINT_CONFIDENCE_BUMP,
        }];
        Self {
            anomaly_type,
            detected_sim_time: sim_time,
            last_updated_sim_time: sim_time,
            state: AnomalyState::Suspected,
            confidence: initial_confidence,
            activation_threshold: base_threshold,
            retry_pressure: 0.0,
            verification_count: 0,
            evidence,
            acknowledged: false,
        }
    }

    /// Effective activation threshold after retry-pressure reduction.
    /// Clamped to [`MIN_ACTIVATION_THRESHOLD`].
    pub fn effective_threshold(&self) -> f32 {
        (self.activation_threshold - self.retry_pressure * RETRY_PRESSURE_THRESHOLD_REDUCTION)
            .max(MIN_ACTIVATION_THRESHOLD)
    }

    /// Whether the anomaly currently meets the activation criterion
    /// (`confidence ≥ effective_threshold`).
    pub fn is_activation_ready(&self) -> bool {
        self.confidence >= self.effective_threshold()
    }

    /// Drop a `Verification` evidence point on the anomaly. Adds
    /// `0.40 × specificity` to confidence (capped at 1.0), bumps
    /// `retry_pressure` and `verification_count`, and rolls a state
    /// transition if the threshold is now met.
    ///
    /// Returns `true` if the call promoted the anomaly to `Verified`.
    pub fn add_verification(&mut self, sim_time: f64, method_specificity: f32) -> bool {
        let bump = super::types::VERIFICATION_CONFIDENCE_BUMP * method_specificity;
        self.confidence = (self.confidence + bump).min(super::types::MAX_CONFIDENCE);
        self.retry_pressure = (self.retry_pressure + RETRY_PRESSURE_PER_VERIFICATION)
            .min(super::types::MAX_CONFIDENCE);
        self.verification_count = self.verification_count.saturating_add(1);
        self.evidence.push(EvidencePoint {
            kind: EvidenceKind::Verification,
            sim_time,
            axis_match_count: 0,
            magnitude: bump,
        });
        self.last_updated_sim_time = sim_time;
        self.maybe_activate()
    }

    /// Drop a `Refutation` evidence point: subtract
    /// `REFUTATION_CONFIDENCE_DROP` and transition to `Refuted`.
    /// The per-tick surface-anomaly system later promotes
    /// `Refuted` → `Dormant` when confidence falls below
    /// `REFUTATION_REARM_THRESHOLD`. New data points re-arm
    /// `Refuted` → `Suspected` via [`Self::add_data_point`].
    ///
    /// Returns `true` if the refutation flipped the state out of
    /// `Verified`.
    pub fn add_refutation(&mut self, sim_time: f64) -> bool {
        let was_verified = self.state == AnomalyState::Verified;
        self.confidence = (self.confidence - super::types::REFUTATION_CONFIDENCE_DROP).max(0.0);
        self.evidence.push(EvidencePoint {
            kind: EvidenceKind::Refutation,
            sim_time,
            axis_match_count: 0,
            magnitude: -super::types::REFUTATION_CONFIDENCE_DROP,
        });
        self.last_updated_sim_time = sim_time;
        self.state = AnomalyState::Refuted;
        was_verified
    }

    /// Drop a `DataPoint` evidence point on the anomaly. Adds
    /// `0.10 × axis_match_count` to confidence (capped at 1.0).
    /// Drives the per-tick confidence climb in `process_analysis_queue`.
    pub fn add_data_point(&mut self, sim_time: f64, axis_match_count: u8) {
        let bump = super::types::DATA_POINT_CONFIDENCE_BUMP * axis_match_count as f32;
        self.confidence = (self.confidence + bump).min(super::types::MAX_CONFIDENCE);
        self.evidence.push(EvidencePoint {
            kind: EvidenceKind::DataPoint,
            sim_time,
            axis_match_count,
            magnitude: bump,
        });
        self.last_updated_sim_time = sim_time;
        // Re-arm a `Dormant` or `Refuted` anomaly back to `Suspected`
        // if the player is actively surveying the body again.
        if matches!(self.state, AnomalyState::Dormant | AnomalyState::Refuted)
            && self.confidence > 0.0
        {
            self.state = AnomalyState::Suspected;
        }
        self.maybe_activate();
    }

    /// Tick `retry_pressure` decay. Called once per sim-year by the
    /// surface-anomaly system. Mods with different decay rates can
    /// call this from a custom system.
    pub fn decay_retry_pressure(&mut self, years: f32) {
        self.retry_pressure =
            (self.retry_pressure - years * super::types::RETRY_PRESSURE_DECAY_PER_YEAR).max(0.0);
    }

    /// State transition: if `confidence ≥ effective_threshold` and
    /// the anomaly is `Suspected`, promote to `Verified`. Returns
    /// `true` on promotion.
    fn maybe_activate(&mut self) -> bool {
        if self.state == AnomalyState::Suspected && self.is_activation_ready() {
            self.state = AnomalyState::Verified;
            true
        } else {
            false
        }
    }
}

/// Pure helper: resolve a default activation threshold for an anomaly
/// that doesn't have one set in the RON. Mirrors the r2 design's
/// `DEFAULT_ACTIVATION_THRESHOLD`. Exposed as a function so tests can
/// call it without a registry.
pub fn default_activation_threshold() -> f32 {
    DEFAULT_ACTIVATION_THRESHOLD
}

#[cfg(test)]
mod tests {
    //! Migration-shim tests for PR-A.
    //!
    //! These tests lock in the SURVEY_REWORK.md §15 (Backward compat)
    //! mapping table. Future PRs that touch the shim will fail these
    //! tests if they break save compatibility.

    use super::*;
    use crate::economy::components::SurveyLevel;

    fn sim_time() -> f64 {
        // Fixed sim-time so the tests are deterministic.
        1_000.0
    }

    #[test]
    fn unsurveyed_maps_to_empty_state() {
        let state = SurveyState::from_legacy_level(SurveyLevel::Unsurveyed, sim_time());
        assert!(state.dimensions.is_empty());
        assert_eq!(state.last_updated_sim_time, sim_time());
        assert!(state.active_missions.is_empty());
        assert!(state.detected_anomalies.is_empty());
        assert_eq!(state.average_tier(), 0.0);
        assert_eq!(state.known_dimension_count(), 0);
    }

    #[test]
    fn orbital_scan_maps_to_three_dimensions_at_tier_one() {
        let state = SurveyState::from_legacy_level(SurveyLevel::OrbitalScan, sim_time());
        assert_eq!(state.known_dimension_count(), 3);
        for dim in [
            SurveyDimension::OrbitalMech,
            SurveyDimension::Atmosphere,
            SurveyDimension::MineralClasses,
        ] {
            let f = state.fidelity(dim);
            assert_eq!(f.tier, 1, "{dim} should be tier 1");
            assert_eq!(f.last_measured_sim_time, Some(sim_time()));
            assert!(f.confidence > 0.0, "{dim} should have nonzero confidence");
        }
        // Subsurface and MineralDeposits stay unknown.
        assert_eq!(state.fidelity(SurveyDimension::Subsurface).tier, 0);
        assert_eq!(state.fidelity(SurveyDimension::MineralDeposits).tier, 0);
    }

    #[test]
    fn seismic_survey_adds_subsurface_at_tier_two() {
        let state = SurveyState::from_legacy_level(SurveyLevel::SeismicSurvey, sim_time());
        assert_eq!(state.known_dimension_count(), 4);
        assert_eq!(state.fidelity(SurveyDimension::Subsurface).tier, 2);
        // The three tier-1 dims from OrbitalScan are preserved.
        for dim in [
            SurveyDimension::OrbitalMech,
            SurveyDimension::Atmosphere,
            SurveyDimension::MineralClasses,
        ] {
            assert_eq!(state.fidelity(dim).tier, 1);
        }
    }

    #[test]
    fn core_sample_maps_to_fully_surveyed() {
        let state = SurveyState::from_legacy_level(SurveyLevel::CoreSample, sim_time());
        // All 8 dimensions at tier 5 with confidence 1.0.
        for dim in SurveyDimension::ALL {
            let f = state.fidelity(dim);
            assert_eq!(f.tier, 5, "{dim} should be tier 5");
            assert!(
                f.is_fully_characterized(),
                "{dim} should be fully characterized"
            );
        }
        assert_eq!(
            state.fully_characterized_count(),
            SurveyDimension::ALL.len()
        );
        assert_eq!(state.average_tier(), 1.0);
    }

    #[test]
    fn average_tier_normalizes_over_all_dimensions() {
        // A body with 1 dimension at tier 4 (and 7 unknown) has
        // average_tier = 4 / (8 * 5) = 0.1.
        let mut state = SurveyState::default();
        state.set_fidelity(
            SurveyDimension::OrbitalMech,
            DimensionFidelity::at_tier(4, 0.9, Some(sim_time())),
        );
        let avg = state.average_tier();
        assert!((avg - 0.1).abs() < 1e-6, "expected 0.1, got {avg}");
        assert_eq!(state.known_dimension_count(), 1);
    }

    #[test]
    fn fidelity_clamp_keeps_tier_in_range() {
        // Calling freshly_measured with tier > MAX_TIER clamps to MAX_TIER.
        let f = DimensionFidelity::freshly_measured(99, sim_time());
        assert_eq!(f.tier, MAX_TIER);
        // at_tier clamps confidence into [0, 1].
        let over = DimensionFidelity::at_tier(1, 5.0, Some(sim_time()));
        assert_eq!(over.confidence, 1.0);
        let under = DimensionFidelity::at_tier(1, -1.0, Some(sim_time()));
        assert_eq!(under.confidence, 0.0);
    }

    // ── GRA-82 PR-D tests: landing-site coverage, score weights, and
    //    blocker derivation. ────────────────────────────────────────

    // Shared state builder for the per-dimension coverage tests below.
    // Reserved for GRA-82 PR-F (eval system consumers) — not yet wired
    // into the round-D tests themselves.
    #[allow(dead_code)]
    fn state_with_dim(dim: SurveyDimension, tier: u8) -> SurveyState {
        let mut s = SurveyState::default();
        s.set_fidelity(dim, DimensionFidelity::at_tier(tier, 1.0, Some(sim_time())));
        s
    }

    #[test]
    fn coverage_is_zero_for_unsurveyed_body() {
        let s = SurveyState::default();
        assert_eq!(s.landing_site_eval_coverage(), 0.0);
    }

    #[test]
    fn coverage_stays_below_threshold_for_orbital_only() {
        // Just OrbitalMech + MineralClasses at tier 1 (the OrbitalScan
        // migration shim). Surface/Habitability/Atmosphere are zero,
        // so coverage should be far below the 0.6 trigger.
        let mut s = SurveyState::default();
        s.set_fidelity(
            SurveyDimension::OrbitalMech,
            DimensionFidelity::at_tier(1, 1.0, Some(sim_time())),
        );
        s.set_fidelity(
            SurveyDimension::MineralClasses,
            DimensionFidelity::at_tier(1, 1.0, Some(sim_time())),
        );
        let cov = s.landing_site_eval_coverage();
        assert!(cov < LANDING_SITE_EVAL_THRESHOLD, "got {cov}");
    }

    #[test]
    fn coverage_crosses_threshold_for_fully_surveyed_body() {
        // A fully-surveyed body should clear the 0.6 gate with
        // margin (all eight dimensions at tier 5).
        let s = SurveyState::fully_surveyed(sim_time());
        let cov = s.landing_site_eval_coverage();
        assert!(
            cov >= LANDING_SITE_EVAL_THRESHOLD,
            "fully_surveyed coverage {cov} should be ≥ {LANDING_SITE_EVAL_THRESHOLD}"
        );
    }

    #[test]
    fn coverage_weights_surface_heavily() {
        // A body with only SurfaceFeatures at tier 5 should clear the
        // threshold (SurfaceFeatures is weighted 0.5 by itself, so
        // 5/5 * 0.5 = 0.5; we need a bit more from the other axes
        // to reach 0.6 — push SurfaceFeatures even harder and add
        // Habitability at low tier to confirm Surface dominates).
        let mut s = SurveyState::default();
        s.set_fidelity(
            SurveyDimension::SurfaceFeatures,
            DimensionFidelity::at_tier(5, 1.0, Some(sim_time())),
        );
        s.set_fidelity(
            SurveyDimension::Habitability,
            DimensionFidelity::at_tier(1, 1.0, Some(sim_time())),
        );
        let cov_with_surface = s.landing_site_eval_coverage();

        // Compare to a body with NO surface data but other dims at
        // tier 5. The surface-dominated body must score higher.
        let mut other = SurveyState::default();
        for dim in [
            SurveyDimension::OrbitalMech,
            SurveyDimension::Atmosphere,
            SurveyDimension::Habitability,
            SurveyDimension::MineralClasses,
            SurveyDimension::MineralDeposits,
            SurveyDimension::Subsurface,
            SurveyDimension::Anomalies,
        ] {
            other.set_fidelity(dim, DimensionFidelity::at_tier(5, 1.0, Some(sim_time())));
        }
        let cov_other = other.landing_site_eval_coverage();
        assert!(
            cov_with_surface > cov_other,
            "surface-dominated {cov_with_surface} should beat other-only {cov_other}"
        );
    }

    #[test]
    fn site_scores_weights_sum_to_one() {
        // The composite must be a proper weighted average; if a
        // future PR rebalances the weights this test catches a
        // missing close-paren.
        let w = SiteScores::WEIGHTS;
        let sum = w.slope + w.roughness + w.radiation + w.temperature + w.regolith + w.comm;
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "weights sum to {sum}, expected 1.0"
        );
    }

    #[test]
    fn site_scores_composite_matches_weighted_average() {
        // Spot-check the composite calculation against a hand
        // computation.
        let s = SiteScores {
            slope: 0.8,
            roughness: 0.6,
            radiation: 0.7,
            temperature: 0.5,
            regolith: 0.9,
            comm: 1.0,
        };
        let w = SiteScores::WEIGHTS;
        let expected = 0.8 * w.slope
            + 0.6 * w.roughness
            + 0.7 * w.radiation
            + 0.5 * w.temperature
            + 0.9 * w.regolith
            + 1.0 * w.comm;
        let got = s.composite();
        assert!(
            (got - expected).abs() < 1e-6,
            "expected {expected}, got {got}"
        );
    }

    #[test]
    fn site_scores_composite_clamps_out_of_range_inputs() {
        // A buggy RON bias should not produce a score outside [0, 1].
        let s = SiteScores {
            slope: 2.0,
            roughness: -1.0,
            radiation: 0.5,
            temperature: 0.5,
            regolith: 0.5,
            comm: 0.5,
        };
        let c = s.composite();
        assert!((0.0..=1.0).contains(&c), "composite {c} not in [0,1]");
    }

    #[test]
    fn site_count_constants_bound_the_per_body_window() {
        // The dossier renders MIN..MAX rows; locking these to
        // GRA-82's 2-5 acceptance range catches accidental
        // rebalances.
        assert_eq!(MIN_SITES_PER_BODY, 2);
        assert_eq!(MAX_SITES_PER_BODY, 5);
        const {
            assert!(LANDING_SITE_EVAL_THRESHOLD > 0.0 && LANDING_SITE_EVAL_THRESHOLD < 1.0);
        }
    }
}
