//! Survey system core enums.
//!
//! These are the data-driven axes of the v0.5.0 survey rework. Modders add
//! new dimensions, methods, and anomaly types by editing
//! `assets/data/survey/*.ron` — these enums are the canonical set the
//! registries know about at compile time. New variants can be added without
//! breaking saved data because every survey state is keyed by string IDs
//! once it leaves the binary.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The eight discovery dimensions defined by SURVEY_REWORK.md §4.
///
/// Each dimension represents an independent axis of "what we know about a
/// body". A body is fully surveyed when every dimension is at tier 5 with
/// confidence ≥ 0.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurveyDimension {
    OrbitalMech,
    Atmosphere,
    SurfaceFeatures,
    MineralClasses,
    MineralDeposits,
    Subsurface,
    Habitability,
    Anomalies,
}

impl SurveyDimension {
    /// All variants in their canonical display order. Used by the dossier
    /// UI to render the dimension table.
    pub const ALL: [SurveyDimension; 8] = [
        SurveyDimension::OrbitalMech,
        SurveyDimension::Atmosphere,
        SurveyDimension::SurfaceFeatures,
        SurveyDimension::MineralClasses,
        SurveyDimension::MineralDeposits,
        SurveyDimension::Subsurface,
        SurveyDimension::Habitability,
        SurveyDimension::Anomalies,
    ];

    /// Short label for UI rows (≤ 24 chars).
    pub fn display_name(self) -> &'static str {
        match self {
            SurveyDimension::OrbitalMech => "Orbital mechanics",
            SurveyDimension::Atmosphere => "Atmosphere",
            SurveyDimension::SurfaceFeatures => "Surface features",
            SurveyDimension::MineralClasses => "Mineral classes",
            SurveyDimension::MineralDeposits => "Mineral deposits",
            SurveyDimension::Subsurface => "Subsurface structure",
            SurveyDimension::Habitability => "Habitability",
            SurveyDimension::Anomalies => "Anomalies",
        }
    }

    /// RON id used by the dimension registry. Stable across saves.
    pub fn ron_id(self) -> &'static str {
        match self {
            SurveyDimension::OrbitalMech => "OrbitalMech",
            SurveyDimension::Atmosphere => "Atmosphere",
            SurveyDimension::SurfaceFeatures => "SurfaceFeatures",
            SurveyDimension::MineralClasses => "MineralClasses",
            SurveyDimension::MineralDeposits => "MineralDeposits",
            SurveyDimension::Subsurface => "Subsurface",
            SurveyDimension::Habitability => "Habitability",
            SurveyDimension::Anomalies => "Anomalies",
        }
    }

    /// Parse from a RON id. Returns `None` for unknown strings so modders
    /// can add new dimensions without breaking the loader.
    pub fn from_ron_id(id: &str) -> Option<Self> {
        match id {
            "OrbitalMech" => Some(SurveyDimension::OrbitalMech),
            "Atmosphere" => Some(SurveyDimension::Atmosphere),
            "SurfaceFeatures" => Some(SurveyDimension::SurfaceFeatures),
            "MineralClasses" => Some(SurveyDimension::MineralClasses),
            "MineralDeposits" => Some(SurveyDimension::MineralDeposits),
            "Subsurface" => Some(SurveyDimension::Subsurface),
            "Habitability" => Some(SurveyDimension::Habitability),
            "Anomalies" => Some(SurveyDimension::Anomalies),
            _ => None,
        }
    }

    /// Whether this dimension gates mining for a given resource class.
    /// Used by `discovered_amount_deposit` to compute mining yield.
    pub fn gates_mining_class(self) -> bool {
        matches!(
            self,
            SurveyDimension::MineralDeposits
                | SurveyDimension::Subsurface
                | SurveyDimension::Atmosphere
                | SurveyDimension::Anomalies
        )
    }
}

impl fmt::Display for SurveyDimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// The nine survey methods defined by SURVEY_REWORK.md §5.
///
/// Methods are how a player invests time and instruments to advance
/// dimensions. Each method is bounded to a subset of dimensions by the
/// tier matrix in `assets/data/survey/tiers.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurveyMethod {
    Flyby,
    Orbital,
    RemoteSensing,
    AtmosphericProbe,
    SurfaceLander,
    Rover,
    Seismic,
    Drill,
    SampleReturn,
}

impl SurveyMethod {
    /// RON id for the method registry.
    pub fn ron_id(self) -> &'static str {
        match self {
            SurveyMethod::Flyby => "flyby",
            SurveyMethod::Orbital => "orbital",
            SurveyMethod::RemoteSensing => "remote_sensing",
            SurveyMethod::AtmosphericProbe => "atmospheric_probe",
            SurveyMethod::SurfaceLander => "surface_lander",
            SurveyMethod::Rover => "rover",
            SurveyMethod::Seismic => "seismic",
            SurveyMethod::Drill => "drill",
            SurveyMethod::SampleReturn => "sample_return",
        }
    }

    pub fn from_ron_id(id: &str) -> Option<Self> {
        match id {
            "flyby" => Some(SurveyMethod::Flyby),
            "orbital" => Some(SurveyMethod::Orbital),
            "remote_sensing" => Some(SurveyMethod::RemoteSensing),
            "atmospheric_probe" => Some(SurveyMethod::AtmosphericProbe),
            "surface_lander" => Some(SurveyMethod::SurfaceLander),
            "rover" => Some(SurveyMethod::Rover),
            "seismic" => Some(SurveyMethod::Seismic),
            "drill" => Some(SurveyMethod::Drill),
            "sample_return" => Some(SurveyMethod::SampleReturn),
            _ => None,
        }
    }
}

/// Anomaly types defined by SURVEY_REWORK.md §12.
///
/// Each anomaly has a discovery method affinity, a "coolness" weight, and
/// a gameplay effect (research unlock, building unlock, or event chain).
/// PR-C extends the r1 set with 6 additional terrestrial anomalies and
/// 4 gas-giant anomalies; the full set is now 19 types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnomalyType {
    // ── r1 terrestrial anomalies (9 from PR-A) ──────────────────────
    WaterIceDeposit,
    HydratedSilicates,
    MethanePlume,
    TholinSignature,
    MagneticAnomaly,
    RadioactiveHotspot,
    FossilMicrobeSignature,
    CryovolcanicFeature,
    UnidentifiedReflectance,
    // ── r2 terrestrial anomalies (6 new) ────────────────────────────
    /// Evaporite deposits indicating past liquid water.
    EvaporiteMineralogy,
    /// Frozen CO2 / dry ice deposit at the pole.
    PolarVolatiles,
    /// Sub-surface briny aquifer detectable by deep-penetration radar.
    BrineAquifer,
    /// Iron-oxide spectral signature (e.g. Mars hematite).
    IronOxideOutcrop,
    /// Tidal-flexing heat signature from a captured-rotational resonance.
    TidallyHeatedFracture,
    /// S-band radar bright spot (putative subsurface ice lens).
    RadarBrightSpot,
    // ── r2 gas-giant anomalies (4 new) ──────────────────────────────
    /// Deep convective storm tower with sustained 400 m/s winds.
    GiantStormTower,
    /// Metallic-hydrogen phase transition detectable by magnetometry.
    MetallicHydrogenLayer,
    /// Helium rain-out at the hydrogen/helium phase boundary.
    HeliumRainOut,
    /// Radiative-layer diamond precipitation (carbon-rich giants).
    DiamondRainSignature,
}

impl AnomalyType {
    /// Canonical 19-anomaly set. Modders can add new types via
    /// `anomalies.ron` without touching this enum; this list is the
    /// set the binary recognizes at compile time.
    pub const ALL: [AnomalyType; 19] = [
        AnomalyType::WaterIceDeposit,
        AnomalyType::HydratedSilicates,
        AnomalyType::MethanePlume,
        AnomalyType::TholinSignature,
        AnomalyType::MagneticAnomaly,
        AnomalyType::RadioactiveHotspot,
        AnomalyType::FossilMicrobeSignature,
        AnomalyType::CryovolcanicFeature,
        AnomalyType::UnidentifiedReflectance,
        AnomalyType::EvaporiteMineralogy,
        AnomalyType::PolarVolatiles,
        AnomalyType::BrineAquifer,
        AnomalyType::IronOxideOutcrop,
        AnomalyType::TidallyHeatedFracture,
        AnomalyType::RadarBrightSpot,
        AnomalyType::GiantStormTower,
        AnomalyType::MetallicHydrogenLayer,
        AnomalyType::HeliumRainOut,
        AnomalyType::DiamondRainSignature,
    ];

    pub fn ron_id(self) -> &'static str {
        match self {
            AnomalyType::WaterIceDeposit => "water_ice_deposit",
            AnomalyType::HydratedSilicates => "hydrated_silicates",
            AnomalyType::MethanePlume => "methane_plume",
            AnomalyType::TholinSignature => "tholin_signature",
            AnomalyType::MagneticAnomaly => "magnetic_anomaly",
            AnomalyType::RadioactiveHotspot => "radioactive_hotspot",
            AnomalyType::FossilMicrobeSignature => "fossil_microbe_signature",
            AnomalyType::CryovolcanicFeature => "cryovolcanic_feature",
            AnomalyType::UnidentifiedReflectance => "unidentified_reflectance",
            AnomalyType::EvaporiteMineralogy => "evaporite_mineralogy",
            AnomalyType::PolarVolatiles => "polar_volatiles",
            AnomalyType::BrineAquifer => "brine_aquifer",
            AnomalyType::IronOxideOutcrop => "iron_oxide_outcrop",
            AnomalyType::TidallyHeatedFracture => "tidally_heated_fracture",
            AnomalyType::RadarBrightSpot => "radar_bright_spot",
            AnomalyType::GiantStormTower => "giant_storm_tower",
            AnomalyType::MetallicHydrogenLayer => "metallic_hydrogen_layer",
            AnomalyType::HeliumRainOut => "helium_rain_out",
            AnomalyType::DiamondRainSignature => "diamond_rain_signature",
        }
    }

    pub fn from_ron_id(id: &str) -> Option<Self> {
        match id {
            "water_ice_deposit" => Some(AnomalyType::WaterIceDeposit),
            "hydrated_silicates" => Some(AnomalyType::HydratedSilicates),
            "methane_plume" => Some(AnomalyType::MethanePlume),
            "tholin_signature" => Some(AnomalyType::TholinSignature),
            "magnetic_anomaly" => Some(AnomalyType::MagneticAnomaly),
            "radioactive_hotspot" => Some(AnomalyType::RadioactiveHotspot),
            "fossil_microbe_signature" => Some(AnomalyType::FossilMicrobeSignature),
            "cryovolcanic_feature" => Some(AnomalyType::CryovolcanicFeature),
            "unidentified_reflectance" => Some(AnomalyType::UnidentifiedReflectance),
            "evaporite_mineralogy" => Some(AnomalyType::EvaporiteMineralogy),
            "polar_volatiles" => Some(AnomalyType::PolarVolatiles),
            "brine_aquifer" => Some(AnomalyType::BrineAquifer),
            "iron_oxide_outcrop" => Some(AnomalyType::IronOxideOutcrop),
            "tidally_heated_fracture" => Some(AnomalyType::TidallyHeatedFracture),
            "radar_bright_spot" => Some(AnomalyType::RadarBrightSpot),
            "giant_storm_tower" => Some(AnomalyType::GiantStormTower),
            "metallic_hydrogen_layer" => Some(AnomalyType::MetallicHydrogenLayer),
            "helium_rain_out" => Some(AnomalyType::HeliumRainOut),
            "diamond_rain_signature" => Some(AnomalyType::DiamondRainSignature),
            _ => None,
        }
    }
}

/// Maximum tier for any single dimension. Fully characterized = tier 5
/// with confidence ≥ 0.8.
pub const MAX_TIER: u8 = 5;

/// Threshold below which a dimension is shown as "stale" in the UI.
pub const STALE_CONFIDENCE: f32 = 0.1;

/// Threshold below which a dimension is shown with a warning icon.
pub const WARNING_CONFIDENCE: f32 = 0.3;

/// Confidence decay rate per sim-year for an unmeasured dimension.
pub const CONFIDENCE_DECAY_PER_YEAR: f32 = 0.005;

/// Default confidence assigned when a dimension is first measured.
pub const INITIAL_CONFIDENCE: f32 = 0.5;

/// Number of sim-days per year. Used by `decay_survey_confidence` to
/// convert elapsed sim-time to decay multipliers.
pub const SURVEY_DAYS_PER_YEAR: f64 = 365.25;

// ── Anomaly confidence model (PR-C, r2 design) ────────────────────────

/// Lifecycle of a single detected anomaly on a body.
///
/// `Suspected` is the initial state right after a detection roll passes
/// the false-positive check. `Verified` is reached when confidence
/// climbs past the (pressure-adjusted) activation threshold. `Refuted`
/// is reached when a contradicting verification mission drops
/// confidence below the re-roll flag. `Dormant` is reserved for
/// anomalies that lose confidence and won't be re-rolled until new
/// evidence arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnomalyState {
    Suspected,
    Verified,
    Refuted,
    Dormant,
}

impl AnomalyState {
    /// Short upper-case badge label for the dossier UI.
    pub fn badge_label(self) -> &'static str {
        match self {
            AnomalyState::Suspected => "CANDIDATE",
            AnomalyState::Verified => "VERIFIED",
            AnomalyState::Refuted => "REFUTED",
            AnomalyState::Dormant => "DORMANT",
        }
    }
}

/// One piece of evidence that contributed to a detected anomaly's
/// confidence. PR-C's confidence model is the sum over these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePoint {
    /// What produced the evidence.
    pub kind: EvidenceKind,
    /// Sim-time the evidence was recorded.
    pub sim_time: f64,
    /// How many detection axes this evidence matched (used for
    /// `0.10 × axis_match_count` data-point contribution).
    pub axis_match_count: u8,
    /// Magnitude multiplier applied to the base confidence bump
    /// (0.10 for data points, 0.40 × specificity for verification
    /// missions). Always positive.
    pub magnitude: f32,
}

/// Source of one piece of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceKind {
    /// Routine data-point collection (per-tick dimension check).
    DataPoint,
    /// Explicit verification mission dispatched to confirm/refute.
    Verification,
    /// Contradicting verification mission (drops confidence by 0.50).
    Refutation,
    /// Effect re-activation from a follow-up event chain.
    Reactivation,
}

/// Default activation threshold before retry_pressure is applied.
pub const DEFAULT_ACTIVATION_THRESHOLD: f32 = 0.7;

/// Confidence bump per data-point tick, scaled by axis match count.
pub const DATA_POINT_CONFIDENCE_BUMP: f32 = 0.10;

/// Confidence bump per verification mission, scaled by method
/// specificity (the registry's `evidence_specificity`).
pub const VERIFICATION_CONFIDENCE_BUMP: f32 = 0.40;

/// Confidence drop on a refutation mission.
pub const REFUTATION_CONFIDENCE_DROP: f32 = 0.50;

/// Retry-pressure added per additional verification mission beyond
/// the first. Decays at `RETRY_PRESSURE_DECAY_PER_YEAR` per sim-year.
pub const RETRY_PRESSURE_PER_VERIFICATION: f32 = 0.10;

/// Per-year decay rate for `retry_pressure` (sim-year units).
pub const RETRY_PRESSURE_DECAY_PER_YEAR: f32 = 0.02;

/// Per-axis reduction of activation threshold from retry pressure.
/// Effective threshold = `activation_threshold - retry_pressure ×
/// RETRY_PRESSURE_THRESHOLD_REDUCTION`, clamped to a minimum of 0.3.
pub const RETRY_PRESSURE_THRESHOLD_REDUCTION: f32 = 0.1;

/// Lower bound on the effective activation threshold.
pub const MIN_ACTIVATION_THRESHOLD: f32 = 0.3;

/// Hard cap on accumulated confidence.
pub const MAX_CONFIDENCE: f32 = 1.0;

/// Minimum confidence at which a refuted anomaly re-arms for a new
/// detection roll. Below this the anomaly is moved to `Dormant` until
/// new evidence arrives.
pub const REFUTATION_REARM_THRESHOLD: f32 = 0.20;

/// How strong each survey method's evidence counts when a
/// verification mission is dispatched. Used as the multiplier on
/// `VERIFICATION_CONFIDENCE_BUMP`. Modders override per-anomaly in
/// `anomalies.ron`; the table here is the canonical default.
pub fn default_method_specificity(method: SurveyMethod) -> f32 {
    match method {
        SurveyMethod::Flyby => 0.4,
        SurveyMethod::Orbital => 0.5,
        SurveyMethod::RemoteSensing => 0.6,
        SurveyMethod::AtmosphericProbe => 0.7,
        SurveyMethod::SurfaceLander => 0.8,
        SurveyMethod::Rover => 0.9,
        SurveyMethod::Seismic => 0.9,
        SurveyMethod::Drill => 1.0,
        SurveyMethod::SampleReturn => 1.0,
    }
}
/// Mission lifecycle states. A mission walks
/// `Queued → Inflight → Active → Completing → Succeeded` on the happy
/// path, or branches to `Failed` / `Aborted` on the unhappy path.
///
/// PR-B (GRA-80) wires these into the
/// [`advance_survey_missions`](crate::survey::systems) tick system.
/// `Aborted` is set by the abort handler in PR-B; further statuses
/// (e.g. `Analyzing`) land with PR-C's analysis queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MissionStatus {
    /// Mission has been dispatched but the probe/rover has not yet
    /// started its timeline. The duration is 0 in this state — used
    /// for queued missions that are waiting for a free dispatch slot.
    #[default]
    Queued,
    /// Probe/rover is travelling to the body. The mission progress
    /// bar shows the travel fraction.
    Inflight,
    /// Probe/rover is on-station, gathering data. This is the bulk
    /// of the mission duration.
    Active,
    /// Data has been collected; the system is finalizing the mission
    /// (rolling for failure, awarding XP, freeing scientists). This
    /// is a single-tick state.
    Completing,
    /// Mission finished successfully. The terminal state on the
    /// happy path. Dimensional tiers have been advanced.
    Succeeded,
    /// Mission failed (probe loss, rover stuck, drill bit stuck,
    /// solar storm, or crew injury). Some partial progress may be
    /// retained. The terminal state on the unhappy path.
    Failed,
    /// Player aborted the mission. The terminal state for an
    /// explicit abort.
    Aborted,
}

impl MissionStatus {
    /// Whether the mission is still ticking (progress is advancing).
    pub fn is_in_progress(self) -> bool {
        matches!(
            self,
            MissionStatus::Queued
                | MissionStatus::Inflight
                | MissionStatus::Active
                | MissionStatus::Completing
        )
    }

    /// Whether the mission has reached a terminal state. Succeeded,
    /// Failed, and Aborted are all terminal.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            MissionStatus::Succeeded | MissionStatus::Failed | MissionStatus::Aborted
        )
    }
}

/// The reason a mission ended in `Failed`. Mirrors
/// [`SurveyEvent::MissionFailed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MissionFailureReason {
    /// A probe or atmospheric probe was lost (5% on probe-using
    /// methods). No data is returned; the probe entity is consumed.
    ProbeLoss,
    /// A rover became stuck in terrain (8% on Rover). Mission ends
    /// early; partial data retained.
    RoverStuck,
    /// A drill bit jammed in the formation (10% on Drill). Mission
    /// ends early; partial data retained.
    DrillBitStuck,
    /// A solar storm hit during the survey (2% on any method).
    /// Electronics may have been damaged; partial data retained.
    SolarStorm,
    /// A crew member was injured on a ground-team mission (2% on
    /// SurfaceLander, Rover, Drill, SampleReturn). The mission ends
    /// in `Failed`; the scientist transitions to `Injured`.
    CrewInjury,
}

impl MissionFailureReason {
    /// Per-mission probability of this failure mode, by method.
    /// Returns `0.0` for method/reason pairs that don't apply (e.g.
    /// `ProbeLoss` on a Rover mission).
    ///
    /// Numbers come from the issue's GRA-80 scope and
    /// `docs/design/SURVEY_REWORK.md` §Gameplay Loop.
    pub fn probability(self, method: SurveyMethod) -> f32 {
        use SurveyMethod::*;
        match (self, method) {
            (MissionFailureReason::ProbeLoss, Flyby)
            | (MissionFailureReason::ProbeLoss, Orbital)
            | (MissionFailureReason::ProbeLoss, RemoteSensing)
            | (MissionFailureReason::ProbeLoss, AtmosphericProbe) => 0.05,
            (MissionFailureReason::RoverStuck, Rover) => 0.08,
            (MissionFailureReason::DrillBitStuck, Drill) => 0.10,
            (MissionFailureReason::SolarStorm, _) => 0.02,
            (MissionFailureReason::CrewInjury, SurfaceLander | Rover | Drill | SampleReturn) => {
                0.02
            }
            _ => 0.0,
        }
    }

    /// Whether this reason applies to the given method at all.
    pub fn applies_to(self, method: SurveyMethod) -> bool {
        self.probability(method) > 0.0
    }
}
