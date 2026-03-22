use serde::{Deserialize, Serialize};

/// High-level category for a ship module slot or component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShipModuleCategory {
    Command,
    Propulsion,
    Power,
    Fuel,
    Cargo,
    Sensor,
    Defense,
    Weapon,
    Construction,
    Habitat,
    Utility,
    // Aurora/TI/DW-inspired categories:
    Bridge,
    CrewQuarters,
    MaintenanceBay,
    ISRU,
    Scanner,
    HeatRadiator,
}

impl ShipModuleCategory {
    pub fn all() -> &'static [ShipModuleCategory] {
        use ShipModuleCategory::*;

        &[
            Command,
            Propulsion,
            Power,
            Fuel,
            Cargo,
            Sensor,
            Defense,
            Weapon,
            Construction,
            Habitat,
            Utility,
            Bridge,
            CrewQuarters,
            MaintenanceBay,
            ISRU,
            Scanner,
            HeatRadiator,
        ]
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Propulsion => "Propulsion",
            Self::Power => "Power",
            Self::Fuel => "Fuel",
            Self::Cargo => "Cargo",
            Self::Sensor => "Sensors",
            Self::Defense => "Defense",
            Self::Weapon => "Weapons",
            Self::Construction => "Construction",
            Self::Habitat => "Habitat",
            Self::Utility => "Utility",
            Self::Bridge => "Bridge",
            Self::CrewQuarters => "Crew Quarters",
            Self::MaintenanceBay => "Maintenance",
            Self::ISRU => "ISRU",
            Self::Scanner => "Scanner",
            Self::HeatRadiator => "Heat Radiator",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Command => "🧠",
            Self::Propulsion => "🚀",
            Self::Power => "⚡",
            Self::Fuel => "⛽",
            Self::Cargo => "📦",
            Self::Sensor => "📡",
            Self::Defense => "🛡",
            Self::Weapon => "🎯",
            Self::Construction => "🏗",
            Self::Habitat => "🏠",
            Self::Utility => "🔧",
            Self::Bridge => "🎛",
            Self::CrewQuarters => "🛏",
            Self::MaintenanceBay => "🔩",
            Self::ISRU => "⚙️",
            Self::Scanner => "🔍",
            Self::HeatRadiator => "🌡",
        }
    }
}

/// Size-based tier for hulls — used for UI grouping and construction assignment.
/// Mirrors Aurora 4X / Distant Worlds 2 conventions:
///   Small Craft  — < 1,000 t  (probes, fighters, pickets, shuttles)
///   Medium Craft — 1,000–10,000 t  (frigates, corvettes, survey vessels)
///   Large Craft  — 10,000–50,000 t  (destroyers, light cruisers)
///   Capital Ship — > 50,000 t  (cruisers, battleships, carriers)
///   Station      — orbital stations (constructed in place, no launch needed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HullSizeTier {
    SmallCraft,
    MediumCraft,
    LargeCraft,
    CapitalShip,
    Station,
}

impl HullSizeTier {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::SmallCraft => "Small Craft",
            Self::MediumCraft => "Medium Craft",
            Self::LargeCraft => "Large Craft",
            Self::CapitalShip => "Capital Ship",
            Self::Station => "Station",
        }
    }

    /// Determine tier from approximate dry mass in tonnes.
    /// Stations override this via explicit field.
    pub fn from_mass_t(mass_t: f64, is_station: bool) -> Self {
        if is_station {
            return Self::Station;
        }
        if mass_t < 1_000.0 {
            Self::SmallCraft
        } else if mass_t < 10_000.0 {
            Self::MediumCraft
        } else if mass_t < 50_000.0 {
            Self::LargeCraft
        } else {
            Self::CapitalShip
        }
    }
}

/// Construction path for a ship or station design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ConstructionMode {
    /// Fabricate on a planetary body, then await launch capacity to reach orbit.
    #[default]
    SurfaceLaunch,
    /// Assemble in orbit from delivered modules and materials.
    OrbitalAssembly,
    /// Build at an established orbital shipyard or construction station.
    OrbitalShipyard,
}

impl ConstructionMode {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::SurfaceLaunch => "Surface Build / Launch",
            Self::OrbitalAssembly => "Orbital Assembly",
            Self::OrbitalShipyard => "Orbital Shipyard",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::SurfaceLaunch => "Surface",
            Self::OrbitalAssembly => "Assembly",
            Self::OrbitalShipyard => "Shipyard",
        }
    }
}

/// Immutable design template — a "class" in Aurora/DW terminology.
/// Stored in ShipDesignLibrary and referenced by construction projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipDesignTemplate {
    /// Stable UUID for versioning and linking
    pub id: uuid::Uuid,
    /// Display name, e.g. "Survey Probe Mk-I", "Frigate Block 2"
    pub name: String,
    /// Points to a ShipHullDefinition id
    pub hull_id: String,
    /// Module selections for each slot
    pub modules: Vec<crate::shipbuilding::ShipModuleSelection>,
    /// Version number within the same design lineage
    pub version: u32,
    /// Parent template id for version history (None for original)
    pub parent_template_id: Option<uuid::Uuid>,
    /// Game time when this version was created
    pub created_at_game_time: f64,
    /// Construction mode for this design
    pub construction_mode: ConstructionMode,
}
