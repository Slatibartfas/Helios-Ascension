//! Colony and Construction System
//!
//! Provides a comprehensive colony management system including:
//! - Colony establishment on celestial bodies (planets, moons, asteroids)
//! - Building construction with queue management and resource costs
//! - Logistics penalty system (mass drivers, orbital lifts, cargo terminals)
//! - Population growth with housing, food and medical modifiers
//! - Data-driven building definitions loaded from assets/data/buildings.ron
//! - Integration with the global resource budget
//! - F12 debug menu for development (including building editor)

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod systems;
pub mod types;

pub use components::{
    Colony, ColonyDevelopment, ColonyEnvironmentCosts, ColonyTier, ConstructionProject,
    EstablishOutpostRequest, PendingConstructionActions, CITY_YIELD_MULTIPLIER,
    CIVILISATION_YIELD_MULTIPLIER, OUTPOST_YIELD_MULTIPLIER, SETTLEMENT_YIELD_MULTIPLIER,
};
pub use data::{BuildingDefinition, BuildingModifierDef, BuildingsData};
pub use systems::DepletionTimeline;
pub use systems::{
    advance_construction, compute_depletion_timeline, deduct_environment_costs,
    process_construction_actions, sync_population_from_colony, update_colony_growth,
    update_treasury,
};
pub use types::{BuildingCategory, BuildingType};

/// Debug settings for construction system (toggled with F12 on Construction menu)
#[derive(Resource, Debug, Clone, Default)]
pub struct ConstructionDebugSettings {
    /// Whether debug mode is enabled
    pub enabled: bool,
    /// Free construction: bypass resource costs
    pub free_construction: bool,
    /// Instant build: complete construction immediately
    pub instant_build: bool,
    /// Bypass tech prerequisites
    pub bypass_tech_requirements: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Building editor state (mirrors TechTreeEditState for consistency)
// ──────────────────────────────────────────────────────────────────────────────

/// In-memory edit state for the building editor debug panel.
#[derive(Resource, Debug, Clone, Default)]
pub struct BuildingEditState {
    /// Building currently open in the "Edit" dialog
    pub editing: Option<BuildingEditData>,
    /// Status message shown after a save/error (text + timestamp)
    pub status_message: Option<(String, f64)>,
    /// Which `BuildingType` is selected in the editor list
    pub selected_type: Option<BuildingType>,
}

/// Editable copy of a `BuildingDefinition` for the editor dialog.
#[derive(Debug, Clone)]
pub struct BuildingEditData {
    /// The `BuildingType` enum key (can't be changed at runtime)
    pub building_type: BuildingType,
    // ── editable text fields ──
    pub display_name: String,
    pub description: String,
    pub icon: String,
    pub category_index: usize,
    pub build_points: String,
    pub workforce: String,
    pub required_tech: String,
    pub power_demand_mw: String,
    // ── lists (current entries) ──
    pub resource_costs: Vec<(String, String)>,
    pub maintenance_resources: Vec<(String, String)>,
    pub modifiers: Vec<BuildingModifierDef>,
    // ── "add row" scratch fields ──
    pub new_cost_name: String,
    pub new_cost_amount: String,
    pub new_maint_name: String,
    pub new_maint_amount: String,
    pub new_modifier_type: String,
    pub new_modifier_value: String,
}

impl BuildingEditData {
    /// Populate from an existing `BuildingDefinition`.
    pub fn from_def(building_type: BuildingType, def: &BuildingDefinition) -> Self {
        let cat_idx = BuildingCategory::all()
            .iter()
            .position(|c| c.display_name() == def.category)
            .unwrap_or(0);
        Self {
            building_type,
            display_name: def.display_name.clone(),
            description: def.description.clone(),
            icon: def.icon.clone(),
            category_index: cat_idx,
            build_points: format!("{:.0}", def.build_points),
            workforce: format!("{}", def.workforce),
            required_tech: def.required_tech.clone(),
            power_demand_mw: format!("{:.0}", def.power_demand_mw),
            resource_costs: def
                .resource_costs
                .iter()
                .map(|(n, v)| (n.clone(), format!("{}", v)))
                .collect(),
            maintenance_resources: def
                .maintenance_resources
                .iter()
                .map(|(n, v)| (n.clone(), format!("{}", v)))
                .collect(),
            modifiers: def.modifiers.clone(),
            new_cost_name: String::new(),
            new_cost_amount: String::new(),
            new_maint_name: String::new(),
            new_maint_amount: String::new(),
            new_modifier_type: String::new(),
            new_modifier_value: String::new(),
        }
    }

    /// Apply edited fields back into a `BuildingDefinition`.
    pub fn apply_to(&self, def: &mut BuildingDefinition) {
        def.display_name = self.display_name.clone();
        def.description = self.description.clone();
        def.icon = self.icon.clone();
        if let Some(cat) = BuildingCategory::all().get(self.category_index) {
            def.category = cat.display_name().to_string();
        }
        if let Ok(v) = self.build_points.parse::<f64>() {
            def.build_points = v;
        }
        if let Ok(v) = self.workforce.parse::<u32>() {
            def.workforce = v;
        }
        def.required_tech = self.required_tech.clone();
        if let Ok(v) = self.power_demand_mw.parse::<f64>() {
            def.power_demand_mw = v;
        }
        def.resource_costs = self
            .resource_costs
            .iter()
            .filter_map(|(n, v)| v.parse::<f64>().ok().map(|fv| (n.clone(), fv)))
            .collect();
        def.maintenance_resources = self
            .maintenance_resources
            .iter()
            .filter_map(|(n, v)| v.parse::<f64>().ok().map(|fv| (n.clone(), fv)))
            .collect();
        def.modifiers = self.modifiers.clone();
    }
}

/// Plugin that adds the colony and construction system to the Bevy app
pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<PendingConstructionActions>()
            .init_resource::<ConstructionDebugSettings>()
            .init_resource::<BuildingEditState>()
            .init_resource::<DepletionTimeline>()
            // Startup systems
            .add_systems(Startup, data::load_buildings)
            // Update systems
            .add_systems(
                Update,
                (
                    process_construction_actions,
                    advance_construction,
                    update_colony_growth,
                    sync_population_from_colony.after(update_colony_growth),
                    update_treasury,
                    systems::deduct_maintenance_resources,
                    systems::deduct_environment_costs,
                    systems::compute_depletion_timeline,
                )
                    .chain()
                    .after(crate::economy::extract_resources),
            );
    }
}
