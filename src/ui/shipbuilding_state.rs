use std::collections::HashMap;

use bevy::prelude::*;

use crate::shipbuilding::ConstructionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShipbuildingTab {
    #[default]
    Design,
    Archive,
    Construction,
    Components,
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
    pub upgrade_source_template_id: Option<uuid::Uuid>,
    pub selected_hull_id: Option<String>,
    pub selected_modules: HashMap<String, String>,
    pub design_name: String,
    pub selected_mode: ConstructionMode,
    pub selected_slot: Option<String>,
    pub design_sort: DesignSort,
    pub design_sort_descending: bool,
    pub construction_target_fleet: Option<Entity>,
    pub construction_design_id: Option<uuid::Uuid>,
    pub selected_component_module_id: Option<String>,
    pub preview_slot: Option<String>,
    pub preview_module_id: Option<String>,
    pub show_hull_dropdown: bool,
    pub hovered_slot: Option<String>,
    pub hovered_module_id: Option<String>,
    pub library_filter_query: String,
    pub slot_hover_started_at: Option<f64>,
    pub module_hover_started_at: Option<f64>,
}

impl Default for ShipbuildingUiState {
    fn default() -> Self {
        Self {
            active_tab: ShipbuildingTab::Design,
            selected_colony: None,
            selected_template_id: None,
            upgrade_source_template_id: None,
            selected_hull_id: None,
            selected_modules: HashMap::default(),
            design_name: String::new(),
            selected_mode: ConstructionMode::SurfaceLaunch,
            selected_slot: None,
            design_sort: DesignSort::HullType,
            design_sort_descending: false,
            construction_target_fleet: None,
            construction_design_id: None,
            selected_component_module_id: None,
            preview_slot: None,
            preview_module_id: None,
            show_hull_dropdown: false,
            hovered_slot: None,
            hovered_module_id: None,
            library_filter_query: String::new(),
            slot_hover_started_at: None,
            module_hover_started_at: None,
        }
    }
}
