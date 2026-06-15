//! Ship class and propulsion type definitions for the fleet system.

use serde::{Deserialize, Serialize};

/// Roles that can be assigned to a fleet, changing its icon and primary purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FleetRole {
    /// Default role for unassigned fleets
    #[default]
    Unassigned,
    /// Combat fleet focused on offensive operations
    Attack,
    /// Combat fleet focused on protecting assets
    Defend,
    /// Exploration and scientific survey fleet
    Survey,
    /// Logistics and cargo transport fleet
    Transport,
    /// Long-range exploration fleet
    Explore,
}

impl FleetRole {
    /// Human-readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            FleetRole::Unassigned => "Unassigned",
            FleetRole::Attack => "Attack",
            FleetRole::Defend => "Defend",
            FleetRole::Survey => "Survey",
            FleetRole::Transport => "Transport",
            FleetRole::Explore => "Explore",
        }
    }

    /// UI icon character.
    pub fn icon(self) -> &'static str {
        match self {
            FleetRole::Unassigned => "🚀",
            FleetRole::Attack => "⚔",
            FleetRole::Defend => "🛡",
            FleetRole::Survey => "🔭",
            FleetRole::Transport => "📦",
            FleetRole::Explore => "🧭",
        }
    }
}

/// Classes of ships that can be built and flown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShipClass {
    /// Small, fast courier for urgent cargo and personnel transfer
    Courier,
    /// Medium all-purpose combat and exploration vessel
    Frigate,
    /// Large warship with heavy armament and armour
    Destroyer,
    /// Massive capital ship — the backbone of a war fleet
    Cruiser,
    /// Scientific research vessel with extended survey capability
    ResearchVessel,
    /// Bulk cargo hauler
    Freighter,
    /// Orbital station — stationary platform, managed as a "ship"
    Station,
}

/// Coarse 3-bucket classification used by [`Settings::show_all_fleet_trajectories`]
/// to filter the system-map trajectory overlay (GRA-154 M-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FleetClass {
    Freighter,
    Combat,
    Civilian,
}

impl ShipClass {
    /// Coarse fleet-class bucket for the trajectory overlay filter.
    pub fn fleet_class(self) -> FleetClass {
        match self {
            ShipClass::Freighter => FleetClass::Freighter,
            ShipClass::Frigate | ShipClass::Destroyer | ShipClass::Cruiser => FleetClass::Combat,
            ShipClass::Courier | ShipClass::ResearchVessel | ShipClass::Station => {
                FleetClass::Civilian
            }
        }
    }
}

impl ShipClass {
    /// Human-readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            ShipClass::Courier => "Courier",
            ShipClass::Frigate => "Frigate",
            ShipClass::Destroyer => "Destroyer",
            ShipClass::Cruiser => "Cruiser",
            ShipClass::ResearchVessel => "Research Vessel",
            ShipClass::Freighter => "Freighter",
            ShipClass::Station => "Station",
        }
    }

    /// UI icon character.
    pub fn icon(self) -> &'static str {
        match self {
            ShipClass::Courier => "✈",
            ShipClass::Frigate => "🚀",
            ShipClass::Destroyer => "⚔",
            ShipClass::Cruiser => "🛸",
            ShipClass::ResearchVessel => "🔭",
            ShipClass::Freighter => "📦",
            ShipClass::Station => "🛰",
        }
    }

    /// Default dry mass in tonnes (without propellant).
    pub fn default_dry_mass_t(self) -> f32 {
        match self {
            ShipClass::Courier => 500.0,
            ShipClass::Frigate => 2_000.0,
            ShipClass::Destroyer => 8_000.0,
            ShipClass::Cruiser => 30_000.0,
            ShipClass::ResearchVessel => 3_000.0,
            ShipClass::Freighter => 15_000.0,
            ShipClass::Station => 100_000.0,
        }
    }

    /// Default propellant mass as a fraction of total (wet) mass.
    pub fn default_fuel_fraction(self) -> f32 {
        match self {
            ShipClass::Courier => 0.50,
            ShipClass::Frigate => 0.45,
            ShipClass::Destroyer => 0.40,
            ShipClass::Cruiser => 0.35,
            ShipClass::ResearchVessel => 0.40,
            ShipClass::Freighter => 0.30,
            ShipClass::Station => 0.10,
        }
    }
}

/// Propulsion technologies available for ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropulsionType {
    /// Chemical rockets — high thrust, low specific impulse (~450 s)
    Chemical,
    /// Nuclear thermal — high thrust, moderate specific impulse (~900 s)
    NuclearThermal,
    /// Ion drive — very low thrust, very high specific impulse (~5 000 s)
    IonDrive,
    /// Nuclear pulse — high thrust and specific impulse (~10 000 s)
    NuclearPulse,
    /// Fusion torch — very high thrust and specific impulse (~50 000 s)
    FusionTorch,
    /// Antimatter drive — extreme specific impulse (~500 000 s) and high thrust.
    /// Matter–antimatter annihilation provides the highest theoretical exhaust
    /// velocity achievable without exotic physics.
    AntimatterDrive,
}

impl PropulsionType {
    /// Human-readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            PropulsionType::Chemical => "Chemical",
            PropulsionType::NuclearThermal => "Nuclear Thermal",
            PropulsionType::IonDrive => "Ion Drive",
            PropulsionType::NuclearPulse => "Nuclear Pulse",
            PropulsionType::FusionTorch => "Fusion Torch",
            PropulsionType::AntimatterDrive => "Antimatter Drive",
        }
    }

    /// Effective specific impulse in seconds.
    ///
    /// Reference values:
    /// - Chemical:        450 s  (kerosene/LOX)
    /// - Nuclear Thermal: 900 s  (NERVA-class)
    /// - Ion Drive:     5 000 s  (Hall-effect thruster)
    /// - Nuclear Pulse: 10 000 s (Orion-class)
    /// - Fusion Torch:  50 000 s (inertial-confinement fusion)
    /// - Antimatter:  1 000 000 s (matter–antimatter annihilation; gives mass
    ///   ratio < 2 for a 1 g Saturn run, per published estimates)
    pub fn isp_s(self) -> f32 {
        match self {
            PropulsionType::Chemical => 450.0,
            PropulsionType::NuclearThermal => 900.0,
            PropulsionType::IonDrive => 5_000.0,
            PropulsionType::NuclearPulse => 10_000.0,
            PropulsionType::FusionTorch => 50_000.0,
            PropulsionType::AntimatterDrive => 1_000_000.0,
        }
    }

    /// Thrust in kilonewtons for a ship of the given dry mass (tonnes).
    ///
    /// `thrust_kN = TWR_vs_dry × dry_mass_t × g₀`
    ///
    /// TWR values are calibrated so that a fully-fuelled Frigate (dry 2 000 t,
    /// fuel fraction 0.45 → wet 3 636 t) achieves the following initial
    /// acceleration:
    ///
    /// | Drive            | TWR_vs_dry | Frigate accel | Game role                  |
    /// |------------------|-----------|---------------|----------------------------|
    /// | Chemical         | 10.0      | ~54 m/s²      | High thrust, very limited ΔV (no Full Thrust) |
    /// | Nuclear Thermal  | 2.0       | ~11 m/s²      | Moderate thrust, medium ΔV |
    /// | Ion Drive        | 0.001     | ~0.005 m/s²   | Near-zero accel, vast ΔV   |
    /// | Nuclear Pulse    | 0.3       | ~1.6 m/s²     | High-thrust nuclear option  |
    /// | Fusion Torch     | 0.02      | ~0.11 m/s²    | ≈0.01 g → ~17-day Mars trip|
    /// | Antimatter Drive | 2.0       | ~10.8 m/s²    | ≈1 g   → ~1.7-day Mars trip|
    pub fn thrust_kn(self, dry_mass_t: f32) -> f32 {
        let twr = match self {
            PropulsionType::Chemical => 10.0_f32,
            PropulsionType::NuclearThermal => 2.0,
            PropulsionType::IonDrive => 0.001,
            // Nuclear pulse (Orion-class): continuous high-thrust pulse drive.
            // 0.3 × dry mass gives a Frigate ~1.6 m/s² (~0.16 g) initial accel.
            PropulsionType::NuclearPulse => 0.3,
            // Fusion torch: sustained low-to-mid thrust with excellent Isp.
            // 0.02 × dry mass gives a Frigate ~0.11 m/s² (~0.01 g) initial accel,
            // matching published estimates of ~17-day brachistochrone trips to Mars.
            PropulsionType::FusionTorch => 0.02,
            // Antimatter: high thrust AND very high Isp.
            // 2.0 × dry mass gives a Frigate ~11 m/s² (~1.1 g) initial accel,
            // matching ~1.7-day Mars and ~5.8-day Jupiter brachistochrone trips.
            PropulsionType::AntimatterDrive => 2.0,
        };
        dry_mass_t * 9.81 * twr
    }
}
