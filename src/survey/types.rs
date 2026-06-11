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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnomalyType {
    WaterIceDeposit,
    HydratedSilicates,
    MethanePlume,
    TholinSignature,
    MagneticAnomaly,
    RadioactiveHotspot,
    FossilMicrobeSignature,
    CryovolcanicFeature,
    UnidentifiedReflectance,
}

impl AnomalyType {
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
