use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::refit::QueueRefitAction;
use super::types::ConstructionMode;
use crate::economy::ResourceType;
use crate::fleets::{PropulsionType, ShipClass};

/// Module choice for a specific hull slot.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct ShipModuleSelection {
    pub slot_id: String,
    pub module_id: String,
}

/// Player-authored or UI-authored design draft queued for construction.
#[derive(Debug, Clone)]
pub struct ShipDesignDraft {
    pub name: String,
    pub hull_id: String,
    pub modules: Vec<ShipModuleSelection>,
    pub construction_mode: ConstructionMode,
}

/// Pending request to queue a construction project at a specific build site.
#[derive(Debug, Clone)]
pub struct QueueShipConstructionAction {
    pub build_site: Entity,
    pub template_id: uuid::Uuid,
    pub integration_target_fleet: Option<Entity>,
}

/// Current lifecycle state for a ship construction project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipConstructionState {
    Building,
    ReadyForLaunch,
    CompletedInOrbit,
}

impl ShipConstructionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Building => "Building",
            Self::ReadyForLaunch => "Ready For Launch",
            Self::CompletedInOrbit => "Completed In Orbit",
        }
    }
}

/// A ship or station currently progressing through the construction queue.
#[derive(Component, Debug, Clone)]
pub struct ShipConstructionProject {
    /// Link to the design template this was built from.
    pub template_id: uuid::Uuid,
    pub design_name: String,
    pub hull_id: String,
    pub build_site: Entity,
    pub integration_target_fleet: Option<Entity>,
    pub selected_modules: Vec<ShipModuleSelection>,
    pub ship_class: ShipClass,
    pub propulsion: Option<PropulsionType>,
    pub progress: f64,
    pub required_build_points: f64,
    pub dry_mass_t: f64,
    pub launch_mass_t: f64,
    pub fuel_capacity_t: f64,
    pub cargo_capacity_t: f64,
    pub ordnance_capacity_t: f64,
    pub magazine_capacity_t: f64,
    pub crew: f64,
    pub power_generation_mw: f64,
    pub power_draw_mw: f64,
    pub thrust_kn: f64,
    pub isp_s: f64,
    pub acceleration_ms2: f64,
    pub delta_v_ms: f64,
    pub sensor_range_au: f64,
    pub docking_ports: f64,
    pub construction_capacity_bp_per_year: f64,
    pub launch_capacity_t_per_year: f64,
    pub is_station: bool,
    pub construction_mode: ConstructionMode,
    pub state: ShipConstructionState,
    pub awaiting_resources: bool,
    pub blocking_request_ids: Vec<u64>,
    pub module_count: usize,
    pub resource_costs: Vec<(ResourceType, f64)>,
    pub launch_resource_costs: Vec<(ResourceType, f64)>,
    pub launch_credit_cost_mc: f64,
    /// GRA-39: when this build was queued by a `ShippingCompany` AI
    /// (auto-build), the index into `ShippingCompanies::companies` that
    /// owns the project.  `None` for player-queued builds.  Used by the
    /// auto-build system to count active builds per company and enforce
    /// the per-company cap.
    pub building_company_idx: Option<usize>,
}

impl ShipConstructionProject {
    pub fn progress_percent(&self) -> f32 {
        if self.required_build_points <= 0.0 {
            return 1.0;
        }

        (self.progress / self.required_build_points).min(1.0) as f32
    }

    pub fn is_building(&self) -> bool {
        self.state == ShipConstructionState::Building
    }
}

/// Marker for fleets that should behave as stations once fleet spawning is wired in.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component)]
pub struct OrbitalStation;

/// Stable linkage between a constructed ship and the design template it currently uses.
#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ShipDesignAssignment {
    pub template_id: uuid::Uuid,
}

/// UI-originated shipbuilding actions to be processed in the update schedule.
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingShipbuildingActions {
    pub queue_projects: Vec<QueueShipConstructionAction>,
    pub queue_refits: Vec<QueueRefitAction>,
    pub cancel_projects: Vec<Entity>,
}

/// Rolling per-build-site launch capacity measured in tonnes to orbit.
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct LaunchCapacityState {
    pub available_mass_t: HashMap<Entity, f64>,
}
