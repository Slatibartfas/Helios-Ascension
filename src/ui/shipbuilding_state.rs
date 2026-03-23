use std::collections::HashMap;

use bevy::prelude::*;

use crate::fleets::FleetRole;
use crate::shipbuilding::ConstructionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShipbuildingTab {
    #[default]
    Design,
    Archive,
    Construction,
    Ships,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DesignSort {
    #[default]
    HullType,
    DeltaV,
    Combat,
    Weight,
}

impl DesignSort {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::HullType => "Hull Type",
            Self::DeltaV => "Delta-V",
            Self::Combat => "Combat",
            Self::Weight => "Weight",
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub(crate) struct ShipbuildingUiState {
    pub active_tab: ShipbuildingTab,
    pub selected_colony: Option<Entity>,
    pub selected_template_id: Option<uuid::Uuid>,
    pub selected_hull_id: Option<String>,
    pub selected_modules: HashMap<String, String>,
    pub design_name: String,
    pub selected_mode: ConstructionMode,
    pub selected_slot: Option<String>,
    pub design_sort: DesignSort,
    pub design_sort_descending: bool,
    pub selected_ship: Option<Entity>,
    pub assignment_target_fleet: Option<Entity>,
    pub new_fleet_name: String,
    pub construction_design_id: Option<uuid::Uuid>,
    pub preview_slot: Option<String>,
    pub preview_module_id: Option<String>,
    pub show_hull_dropdown: bool,
    pub hovered_slot: Option<String>,
    pub hovered_module_id: Option<String>,
}

impl Default for ShipbuildingUiState {
    fn default() -> Self {
        Self {
            active_tab: ShipbuildingTab::Design,
            selected_colony: None,
            selected_template_id: None,
            selected_hull_id: None,
            selected_modules: HashMap::default(),
            design_name: String::new(),
            selected_mode: ConstructionMode::SurfaceLaunch,
            selected_slot: None,
            design_sort: DesignSort::HullType,
            design_sort_descending: false,
            selected_ship: None,
            assignment_target_fleet: None,
            new_fleet_name: String::new(),
            construction_design_id: None,
            preview_slot: None,
            preview_module_id: None,
            show_hull_dropdown: false,
            hovered_slot: None,
            hovered_module_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ShipRosterRow {
    pub ship_entity: Entity,
    pub fleet_entity: Option<Entity>,
    pub fleet_name: String,
    pub ship_name: String,
    pub ship_class: crate::fleets::ShipClass,
    pub dry_mass_t: f64,
    pub delta_v_ms: f64,
    pub fuel_fraction: f32,
    pub role: Option<FleetRole>,
    pub location: String,
    pub parked_body: Entity,
    pub parked_orbit_radius_au: f64,
    pub stationary: bool,
    pub in_transit: bool,
}