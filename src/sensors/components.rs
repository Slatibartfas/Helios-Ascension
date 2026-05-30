//! ECS components for the sensor system.

use bevy::prelude::*;

/// Signature bands for a ship — measures how detectable it is in each band.
///
/// Calculated as: `base × (1 + emission_factor) × (1 − reduction_factor)`
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Signature {
    /// Thermal signature (heat radiation).
    pub thermal: f32,
    /// Electromagnetic signature (radio, radar cross-section).
    pub em: f32,
    /// Visual signature (reflected light, silhouette).
    pub visual: f32,
    /// Neutrino signature (Tier 6+ sensors only).
    pub neutrino: f32,
}

impl Signature {
    /// Total combined signature across all bands.
    pub fn total(&self) -> f32 {
        self.thermal + self.em + self.visual + self.neutrino
    }

    /// Effective signature for a given sensor tier.
    /// Neutrino sensors see only the neutrino band; all others see thermal+EM+visual.
    pub fn effective_for(&self, neutrino_sensor: bool) -> f32 {
        if neutrino_sensor {
            self.neutrino
        } else {
            self.thermal + self.em + self.visual
        }
    }
}

/// Stealth mode emission multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StealthMode {
    /// Normal emissions (1.0× multiplier).
    #[default]
    FullPower,
    /// Reduced emissions (0.3× multiplier).
    RunningSilent,
    /// Minimal emissions (0.1× multiplier).
    DarkDrive,
    /// Near-zero emissions (0.05× multiplier).
    Hidden,
}

impl StealthMode {
    pub fn emission_multiplier(&self) -> f32 {
        match self {
            StealthMode::FullPower => 1.0,
            StealthMode::RunningSilent => 0.3,
            StealthMode::DarkDrive => 0.1,
            StealthMode::Hidden => 0.05,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            StealthMode::FullPower => "Full Power",
            StealthMode::RunningSilent => "Running Silent",
            StealthMode::DarkDrive => "Dark Drive",
            StealthMode::Hidden => "Hidden",
        }
    }

    pub fn ui_color(&self) -> (u8, u8, u8) {
        match self {
            StealthMode::FullPower => (200, 200, 200),
            StealthMode::RunningSilent => (100, 180, 255),
            StealthMode::DarkDrive => (180, 100, 255),
            StealthMode::Hidden => (50, 50, 200),
        }
    }
}

/// Detection/identification state of a contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ContactState {
    /// Detected but not yet explained — signature picked up but not identified.
    Unexplained,
    /// Detected and being tracked (1–99% tracking).
    #[default]
    Detected,
    /// Fully identified (100% tracking).
    Identified,
    /// Weapons locked (tracking maintained).
    Locked,
}

/// Contact record — a ship that has been detected by our sensors.
#[derive(Component, Debug, Clone)]
pub struct Contact {
    /// Entity of the detected target ship/fleet.
    pub target_entity: Entity,
    /// Display name of the target.
    pub name: String,
    /// Current tracking percentage (0–100).
    pub tracking_pct: f32,
    /// Detection/identification state.
    pub state: ContactState,
    /// Signature at time of last detection.
    pub last_signature: Signature,
    /// Simulation time when last detected.
    pub last_detection_time: f64,
    /// Tracking quality accumulation — +10% per second while contact is held.
    pub tracking_quality: f32,
    /// Whether this contact is a friendly (same faction).
    pub friendly: bool,
    /// Whether the target is currently within identification range.
    pub in_id_range: bool,
}

impl Contact {
    /// Create a new contact at initial detection.
    pub fn new(
        target_entity: Entity,
        name: String,
        signature: Signature,
        sim_time: f64,
        friendly: bool,
        in_id_range: bool,
    ) -> Self {
        Self {
            target_entity,
            name,
            tracking_pct: 1.0,
            state: ContactState::Unexplained,
            last_signature: signature,
            last_detection_time: sim_time,
            tracking_quality: 0.0,
            friendly,
            in_id_range,
        }
    }

    /// Advance tracking quality by `dt` seconds.
    pub fn accumulate_tracking(&mut self, dt: f64) {
        self.tracking_quality += 10.0 * dt as f32;
    }

    /// Reset tracking quality (e.g., on contact loss).
    pub fn reset_tracking(&mut self) {
        self.tracking_quality = 0.0;
    }

    /// Update state based on current tracking percentage.
    /// Spec: Unexplained → Detected (1-99%) → Identified (100%) → Locked
    pub fn update_state(&mut self) {
        if self.tracking_pct >= 100.0 {
            self.state = ContactState::Locked;
        } else if self.tracking_pct >= 1.0 {
            // Unexplained contacts transition to Detected on first positive detection
            if self.state == ContactState::Unexplained {
                self.state = ContactState::Detected;
            }
        }
    }
}

/// Active sensor component — provides active ping capability.
#[derive(Component, Debug, Clone, Copy)]
pub struct ActiveSensor {
    /// Ping reveal radius in km.
    pub ping_range_km: f32,
}

impl ActiveSensor {
    /// Effective ping radius (80% of stated range per design doc).
    pub fn effective_ping_radius(&self) -> f32 {
        self.ping_range_km * 0.8
    }
}

/// Sensor suite slot on a ship.
#[derive(Component, Debug, Clone)]
pub struct SensorSuite {
    /// Sensor tier ID (e.g., "basic", "advanced", "neutrino").
    pub tier_id: String,
    /// Detection range in km.
    pub detection_range_km: f32,
    /// Identification range in km.
    pub id_range_km: f32,
    /// Sensor strength multiplier.
    pub strength: f32,
    /// Whether this sensor can detect neutrino signatures (Tier 6).
    pub neutrino: bool,
    /// Whether the sensor suite is actively scanning.
    pub is_active: bool,
}

impl SensorSuite {
    /// Effective detection range at a given distance squared.
    pub fn detection_factor(&self, distance_km: f32) -> f32 {
        // Detection check: sensor_strength / (target_signature × distance²)
        // Returns a factor; if > 1.0, target is detected at this distance.
        self.strength / distance_km.powi(2)
    }
}
