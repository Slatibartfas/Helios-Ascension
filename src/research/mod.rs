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

/// Plugin that adds the research system to the Bevy app
pub struct ResearchPlugin;

impl Plugin for ResearchPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<ResearchState>()
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
