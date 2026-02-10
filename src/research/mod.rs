//! Research and Technology System
//!
//! Provides a comprehensive research system including:
//! - Tech tree with 1000+ technologies across multiple categories
//! - Separation of Research (discovery) and Engineering (application)
//! - Research teams with limited slots (Aurora 4X style)
//! - Component designs that require engineering
//! - Technology modifiers that affect civilization stats
//! - Data-driven technology definitions for easy modding

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod systems;
pub mod types;

pub use components::{
    ComponentDesign, EngineeringFacility, EngineeringProject, ResearchBuilding, ResearchProject,
    ResearchTeam,
};
pub use data::{load_technologies, TechnologiesData};
pub use systems::{
    advance_engineering_projects, advance_research_projects, check_unlocked_technologies,
    update_research_points, ResearchState,
};
pub use types::{TechCategory, Technology, TechnologyId};

/// Debug settings for research system
#[derive(Resource, Debug, Clone)]
pub struct ResearchDebugSettings {
    /// Whether debug mode is enabled
    pub enabled: bool,
    /// Whether to show all technologies (ignore prerequisites)
    pub show_all_techs: bool,
    /// Instant research (0 cost)
    pub instant_research: bool,
    /// Instant engineering (0 cost)
    pub instant_engineering: bool,
}

impl Default for ResearchDebugSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            show_all_techs: false,
            instant_research: false,
            instant_engineering: false,
        }
    }
}

/// State for the tech tree debug editing UI (context menus, edit dialogs)
#[derive(Resource, Debug, Clone)]
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

impl Default for TechTreeEditState {
    fn default() -> Self {
        Self {
            editing: None,
            adding: None,
            context_menu: None,
            delete_confirm: None,
            status_message: None,
        }
    }
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
        }
    }
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
            // Startup systems
            .add_systems(Startup, load_technologies)
            // Update systems
            .add_systems(
                Update,
                (
                    update_research_points,
                    advance_research_projects,
                    advance_engineering_projects,
                    check_unlocked_technologies,
                ),
            );
    }
}
