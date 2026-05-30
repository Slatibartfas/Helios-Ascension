//! Sensor data loaded from `assets/data/sensors.ron`.

use bevy::prelude::Resource;
use serde::Deserialize;
use std::collections::HashMap;

///1 km in meters — internal storage is in km for precision.
pub const KM_IN_METERS: f64 = 1_000.0;

/// 1 AU in km (approximate).
pub const AU_IN_KM: f64 = 149_597.870;

/// Sensor tier definition.
#[derive(Debug, Clone, Deserialize)]
pub struct SensorTierDef {
    pub id: String,
    pub name: String,
    pub tier: u32,
    pub detection_range_km: f64,
    pub id_range_km: f64,
    pub strength: f64,
    pub neutrino: bool,
    pub active_ping_range_km: f64,
}

/// Signature class for a ship class.
#[derive(Debug, Clone, Deserialize)]
pub struct SignatureClassDef {
    pub id: String,
    pub ship_class: String,
    pub base_thermal: f64,
    pub base_em: f64,
    pub base_visual: f64,
    pub base_neutrino: f64,
}

/// Stealth mode definition.
#[derive(Debug, Clone, Deserialize)]
pub struct StealthModeDef {
    pub id: String,
    pub name: String,
    pub emission_multiplier: f64,
}

/// Root sensor data file structure.
#[derive(Debug, Clone, Deserialize, Resource)]
pub struct SensorData {
    pub sensor_tiers: Vec<SensorTierDef>,
    pub signature_classes: Vec<SignatureClassDef>,
    pub stealth_modes: Vec<StealthModeDef>,
}

impl Default for SensorData {
    fn default() -> Self {
        Self::load()
    }
}

impl SensorData {
    /// Load sensor data from the RON file.
    pub fn load() -> Self {
        let path = "assets/data/sensors.ron";
        let bytes = std::fs::read(path).unwrap_or_else(|_| {
            panic!("sensor data file not found: {path}");
        });
        let text = String::from_utf8(bytes).expect("sensor data must be valid UTF-8");
        ron::from_str(&text).expect("failed to parse sensors.ron")
    }

    /// Build a map from tier ID to SensorTierDef.
    pub fn tier_map(&self) -> HashMap<String, &SensorTierDef> {
        self.sensor_tiers
            .iter()
            .map(|t| (t.id.clone(), t))
            .collect()
    }

    /// Build a map from ship class name to SignatureClassDef.
    pub fn signature_class_map(&self) -> HashMap<String, &SignatureClassDef> {
        self.signature_classes
            .iter()
            .map(|c| (c.ship_class.clone(), c))
            .collect()
    }
}
