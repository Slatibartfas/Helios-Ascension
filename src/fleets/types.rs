//! Ship class and propulsion type definitions for the fleet system.

/// Classes of ships that can be built and flown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        }
    }

    /// Effective specific impulse in seconds.
    pub fn isp_s(self) -> f32 {
        match self {
            PropulsionType::Chemical => 450.0,
            PropulsionType::NuclearThermal => 900.0,
            PropulsionType::IonDrive => 5_000.0,
            PropulsionType::NuclearPulse => 10_000.0,
            PropulsionType::FusionTorch => 50_000.0,
        }
    }

    /// Thrust in kilonewtons for a ship of the given dry mass.
    ///
    /// Uses a simplified thrust-to-weight ratio per propulsion type.
    pub fn thrust_kn(self, dry_mass_t: f32) -> f32 {
        // Thrust-to-weight ratio (unitless)
        let twr = match self {
            PropulsionType::Chemical => 10.0_f32,
            PropulsionType::NuclearThermal => 5.0,
            PropulsionType::IonDrive => 0.001,
            PropulsionType::NuclearPulse => 50.0,
            PropulsionType::FusionTorch => 1.0,
        };
        // F = twr × m × g₀  (g₀ = 9.81 m/s², convert from kN)
        dry_mass_t * 9.81 / 1_000.0 * twr
    }
}
