use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A single construction berth at a shipyard.
/// One slipway can build one ship at a time.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct Slipway {
    /// 0-indexed within the shipyard
    pub id: u32,
    /// Currently assigned design template, if any
    pub assigned_template_id: Option<uuid::Uuid>,
    /// Currently building project entity, if any
    pub current_project: Option<bevy::prelude::Entity>,
    /// BP accumulated this year
    pub progress: f64,
    /// Years remaining to retool to a new class
    pub retool_time_remaining: f64,
    /// True if currently retooling
    pub is_retooling: bool,
}

impl Slipway {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            assigned_template_id: None,
            current_project: None,
            progress: 0.0,
            retool_time_remaining: 0.0,
            is_retooling: false,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.assigned_template_id.is_none() && self.current_project.is_none() && !self.is_retooling
    }

    pub fn is_building(&self) -> bool {
        self.current_project.is_some()
    }

    pub fn is_retooling(&self) -> bool {
        self.is_retooling
    }
}

/// Per-colony shipyard facility with multiple slipways.
#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ShipyardFacility {
    /// All slipways at this facility
    pub slipways: Vec<Slipway>,
    /// Base BP/year per slipway
    pub base_bp_per_slipway: f64,
    /// Bonus BP/year from factories
    pub factory_support_bp: f64,
    /// Multiplier from engineering bays
    pub engineering_bonus: f64,
}

impl ShipyardFacility {
    pub fn new(slipway_count: u32, base_bp: f64) -> Self {
        Self {
            slipways: (0..slipway_count).map(Slipway::new).collect(),
            base_bp_per_slipway: base_bp,
            factory_support_bp: 0.0,
            engineering_bonus: 1.0,
        }
    }

    /// Total BP/year capacity for this facility
    pub fn total_capacity_bp_per_year(&self) -> f64 {
        let per_slipway = self.base_bp_per_slipway + self.factory_support_bp;
        per_slipway * self.engineering_bonus * self.slipways.len() as f64
    }

    /// BP/year for a specific slipway
    pub fn slipway_capacity_bp_per_year(&self) -> f64 {
        (self.base_bp_per_slipway + self.factory_support_bp) * self.engineering_bonus
    }

    /// Get a mutable reference to a slipway by index
    pub fn slipway_mut(&mut self, id: u32) -> Option<&mut Slipway> {
        self.slipways.iter_mut().find(|s| s.id == id)
    }

    /// Get a slipway by index
    pub fn slipway(&self, id: u32) -> Option<&Slipway> {
        self.slipways.iter().find(|s| s.id == id)
    }

    /// Count of idle slipways
    pub fn idle_count(&self) -> usize {
        self.slipways.iter().filter(|s| s.is_idle()).count()
    }

    /// Count of slipways currently building
    pub fn building_count(&self) -> usize {
        self.slipways.iter().filter(|s| s.is_building()).count()
    }

    /// Count of slipways currently retooling
    pub fn retooling_count(&self) -> usize {
        self.slipways.iter().filter(|s| s.is_retooling()).count()
    }
}
