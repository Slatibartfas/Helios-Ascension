//! Research and Technology System
//!
//! Provides a comprehensive research system including:
//! - Tech tree with 1000+ technologies across multiple categories
//! - Separation of Research (discovery) and Engineering (application)
//! - Research teams with limited slots
//! - Component designs that require engineering
//! - Technology modifiers that affect civilization stats
//! - Data-driven technology definitions for easy modding

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod events;
pub mod systems;
pub mod types;

pub use components::{
    ComponentDesign, EngineeringFacility, EngineeringProject, ResearchBuilding, ResearchProject,
    ResearchTeam, ResearchTeamCapacity,
};
pub use data::{load_technologies, TechnologiesData};
pub use events::ResearchEvent;
pub use systems::{
    advance_engineering_projects, advance_research_projects, apply_debug_modifiers,
    check_unlocked_technologies, initialize_baseline_engineering, initialize_baseline_technology,
    update_research_points, ResearchState,
};
pub use types::{ModifierType, TechCategory, TechModifierDef, Technology, TechnologyId};

/// Debug settings for research system
#[derive(Resource, Debug, Clone, Default)]
pub struct ResearchDebugSettings {
    /// Whether debug mode is enabled
    pub enabled: bool,
    /// Whether to show all technologies (ignore prerequisites)
    pub show_all_techs: bool,
    /// Instant research (0 cost)
    pub instant_research: bool,
    /// Instant engineering (0 cost)
    pub instant_engineering: bool,
    /// Debug modifiers to apply (type, value)
    pub debug_modifiers: std::collections::HashMap<types::ModifierType, f64>,
    /// Whether the "Add Debug Modifier" dialog is open
    pub modifier_dialog_show: bool,
    /// Currently selected modifier type index in the dialog
    pub modifier_dialog_type_index: usize,
    /// Text input value for the modifier percentage
    pub modifier_dialog_value_input: String,
}

/// State for the tech tree debug editing UI (context menus, edit dialogs)
#[derive(Resource, Debug, Clone, Default)]
pub struct TechTreeEditState {
    /// Whether the "Edit Technology" window is open
    pub editing: Option<TechEditData>,
    /// Whether the "Add Technology" window is open
    pub adding: Option<TechEditData>,
    /// Whether a context menu is showing, and where
    pub context_menu: Option<ContextMenuState>,
    /// Whether we need to confirm a deletion
    pub delete_confirm: Option<String>,
    /// Status message to show (e.g. "Saved", "Error: ...")
    pub status_message: Option<(String, f64)>,
}

/// Context menu state
#[derive(Debug, Clone)]
pub struct ContextMenuState {
    /// Screen position where the context menu was opened
    pub pos: (f32, f32),
    /// Tech ID if right-clicked on a node, None if right-clicked on empty space
    pub tech_id: Option<String>,
}

/// Editable copy of a technology's fields for the edit/add dialog
#[derive(Debug, Clone)]
pub struct TechEditData {
    /// Original ID (for edits), empty for new techs
    pub original_id: String,
    pub id: String,
    pub name: String,
    pub category_index: usize,
    pub description: String,
    pub research_cost: String,
    pub tier: String,
    pub prerequisites: Vec<String>,
    /// Text field for adding a new prerequisite
    pub new_prereq: String,
    /// Modifiers granted when this tech is researched
    pub modifiers: Vec<types::TechModifierDef>,
    /// Index into ModifierType::all_for_debug() for the "add modifier" row
    pub new_modifier_type_index: usize,
    /// Value text field for the "add modifier" row
    pub new_modifier_value: String,
}

impl TechEditData {
    /// Create from an existing technology
    pub fn from_tech(tech: &types::Technology) -> Self {
        Self {
            original_id: tech.id.clone(),
            id: tech.id.clone(),
            name: tech.name.clone(),
            category_index: TechCategory::all()
                .iter()
                .position(|c| *c == tech.category)
                .unwrap_or(0),
            description: tech.description.clone(),
            research_cost: format!("{:.0}", tech.research_cost),
            tier: format!("{}", tech.tier),
            prerequisites: tech.prerequisites.clone(),
            new_prereq: String::new(),
            modifiers: tech.modifiers.clone(),
            new_modifier_type_index: 0,
            new_modifier_value: String::new(),
        }
    }

    /// Create a blank template for adding a new technology
    pub fn new_blank() -> Self {
        Self {
            original_id: String::new(),
            id: String::new(),
            name: String::new(),
            category_index: 0,
            description: String::new(),
            research_cost: "1000".to_string(),
            tier: "1".to_string(),
            prerequisites: Vec::new(),
            new_prereq: String::new(),
            modifiers: Vec::new(),
            new_modifier_type_index: 0,
            new_modifier_value: String::new(),
        }
    }
}

/// Collects research start requests from the UI to be processed by a Bevy system.
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingResearchActions {
    /// Tech IDs that the user wants to begin researching.
    pub start_research: Vec<TechnologyId>,
    /// Component or engineering target IDs that the user wants to begin engineering.
    pub start_engineering: Vec<String>,
    /// Tech IDs that the user wants to pause researching (preserves progress, frees team slot).
    pub stop_research: Vec<TechnologyId>,
    /// Tech IDs that the user wants to resume researching.
    pub resume_research: Vec<TechnologyId>,
    /// Tech IDs that the user wants to cancel/remove entirely (despawn entity, progress lost).
    pub cancel_research: Vec<TechnologyId>,
    /// Whether to navigate to the Available Research tab.
    pub navigate_to_available_tab: bool,
    /// Whether to navigate to the Available Engineering tab.
    pub navigate_to_available_engineering_tab: bool,
    /// Preferred engineering target to preselect after navigating to the engineering tab.
    pub navigate_to_engineering_target: Option<String>,
    /// Updated allocation percentages: (tech_id, new_percent)
    pub update_allocations: Vec<(TechnologyId, f64)>,
}

/// Plugin that adds the research system to the Bevy app
pub struct ResearchPlugin;

impl Plugin for ResearchPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<ResearchState>()
            .init_resource::<ResearchDebugSettings>()
            .init_resource::<TechTreeEditState>()
            .init_resource::<PendingResearchActions>()
            .init_resource::<ResearchTeamCapacity>()
            // Startup systems
            .add_systems(Startup, load_technologies)
            .add_systems(
                PostStartup,
                (
                    systems::merge_ship_module_engineering_catalog,
                    initialize_baseline_technology,
                    initialize_baseline_engineering,
                )
                    .chain(),
            )
            // Update systems
            .add_systems(
                Update,
                (
                    update_research_points,
                    systems::process_pending_research,
                    systems::process_pending_engineering,
                    systems::process_stop_research,
                    systems::process_allocation_updates,
                    advance_research_projects,
                    advance_engineering_projects,
                    check_unlocked_technologies,
                    systems::apply_debug_modifiers, // Apply debug modifiers after other systems
                )
                    .chain(),
            );
    }
}
