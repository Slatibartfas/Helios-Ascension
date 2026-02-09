use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::types::{ComponentDefinition, Technology, TechnologyId};

/// Resource that holds all technology definitions loaded from data files
#[derive(Resource, Debug, Clone, Default)]
pub struct TechnologiesData {
    /// All technologies indexed by ID
    pub technologies: HashMap<TechnologyId, Technology>,
    /// All component definitions
    pub components: HashMap<String, ComponentDefinition>,
}

impl TechnologiesData {
    /// Get a technology by ID
    pub fn get_tech(&self, id: &str) -> Option<&Technology> {
        self.technologies.get(id)
    }

    /// Get a component definition by ID
    pub fn get_component(&self, id: &str) -> Option<&ComponentDefinition> {
        self.components.get(id)
    }

    /// Get all technologies in a category
    pub fn get_by_category(&self, category: super::types::TechCategory) -> Vec<&Technology> {
        self.technologies
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Check if all prerequisites for a technology are satisfied
    pub fn check_prerequisites(&self, tech_id: &str, unlocked: &[TechnologyId]) -> bool {
        if let Some(tech) = self.get_tech(tech_id) {
            tech.prerequisites
                .iter()
                .all(|prereq| unlocked.contains(prereq))
        } else {
            false
        }
    }
}

/// Data file format for technologies
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TechnologiesFile {
    technologies: Vec<Technology>,
    components: Vec<ComponentDefinition>,
}

/// System to load technologies from data file at startup
pub fn load_technologies(mut commands: Commands) {
    info!("Loading technology definitions...");

    let path = "assets/data/technologies.ron";
    
    match fs::read_to_string(path) {
        Ok(contents) => {
            match ron::from_str::<TechnologiesFile>(&contents) {
                Ok(data) => {
                    let tech_count = data.technologies.len();
                    let component_count = data.components.len();

                    let mut tech_data = TechnologiesData::default();

                    // Index technologies by ID
                    for tech in data.technologies {
                        tech_data.technologies.insert(tech.id.clone(), tech);
                    }

                    // Index components by ID
                    for component in data.components {
                        tech_data.components.insert(component.id.clone(), component);
                    }

                    info!(
                        "Loaded {} technologies and {} component definitions",
                        tech_count, component_count
                    );

                    commands.insert_resource(tech_data);
                }
                Err(e) => {
                    error!("Failed to parse technology data file: {}", e);
                    // Insert empty resource so the game doesn't crash
                    commands.insert_resource(TechnologiesData::default());
                }
            }
        }
        Err(e) => {
            warn!(
                "Technology data file not found at {}: {}. Using empty tech tree.",
                path, e
            );
            // Insert empty resource so the game doesn't crash
            commands.insert_resource(TechnologiesData::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::types::{TechCategory, TechModifierDef, ModifierType};

    #[test]
    fn test_technologies_data_get_tech() {
        let mut data = TechnologiesData::default();
        let tech = Technology {
            id: "test_tech".to_string(),
            name: "Test Technology".to_string(),
            category: TechCategory::Physics,
            description: "A test".to_string(),
            research_cost: 1000.0,
            prerequisites: vec![],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 1,
        };

        data.technologies.insert("test_tech".to_string(), tech);

        assert!(data.get_tech("test_tech").is_some());
        assert!(data.get_tech("nonexistent").is_none());
    }

    #[test]
    fn test_check_prerequisites() {
        let mut data = TechnologiesData::default();
        
        let tech1 = Technology {
            id: "tech1".to_string(),
            name: "Tech 1".to_string(),
            category: TechCategory::Physics,
            description: "First tech".to_string(),
            research_cost: 1000.0,
            prerequisites: vec![],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 1,
        };

        let tech2 = Technology {
            id: "tech2".to_string(),
            name: "Tech 2".to_string(),
            category: TechCategory::Physics,
            description: "Second tech".to_string(),
            research_cost: 2000.0,
            prerequisites: vec!["tech1".to_string()],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 2,
        };

        data.technologies.insert("tech1".to_string(), tech1);
        data.technologies.insert("tech2".to_string(), tech2);

        // tech2 requires tech1
        let unlocked = vec![];
        assert!(!data.check_prerequisites("tech2", &unlocked));

        let unlocked = vec!["tech1".to_string()];
        assert!(data.check_prerequisites("tech2", &unlocked));
    }
}
