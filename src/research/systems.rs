use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::ui::SimulationTime;

use super::components::{
    ComponentDesign, EngineeringFacility, EngineeringProject, ResearchBuilding, ResearchProject,
    ResearchTeam,
};
use super::data::TechnologiesData;
use super::types::{ModifierType, TechCategory, TechnologyId};

/// Resource that tracks global research state
#[derive(Resource, Debug, Clone, Default)]
pub struct ResearchState {
    /// Technologies that have been unlocked
    pub unlocked_technologies: HashSet<TechnologyId>,
    /// Component designs that have been completed
    pub completed_components: HashSet<String>,
    /// Available research points pool
    pub research_points_available: f64,
    /// Available engineering points pool
    pub engineering_points_available: f64,
    /// Active modifiers from technologies
    pub active_modifiers: HashMap<ModifierType, f64>,
}

impl ResearchState {
    /// Check if a technology is unlocked
    pub fn is_unlocked(&self, tech_id: &str) -> bool {
        self.unlocked_technologies.contains(tech_id)
    }

    /// Check if a component is completed
    pub fn is_component_completed(&self, component_id: &str) -> bool {
        self.completed_components.contains(component_id)
    }

    /// Unlock a technology
    pub fn unlock_tech(&mut self, tech_id: TechnologyId) {
        self.unlocked_technologies.insert(tech_id);
    }

    /// Complete a component design
    pub fn complete_component(&mut self, component_id: String) {
        self.completed_components.insert(component_id);
    }

    /// Add a modifier
    pub fn add_modifier(&mut self, modifier_type: ModifierType, value: f64) {
        *self.active_modifiers.entry(modifier_type).or_insert(0.0) += value;
    }

    /// Get the total value of a modifier type
    pub fn get_modifier(&self, modifier_type: ModifierType) -> f64 {
        self.active_modifiers
            .get(&modifier_type)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get research speed multiplier (1.0 + bonus from modifiers)
    pub fn research_speed_multiplier(&self) -> f64 {
        1.0 + (self.get_modifier(ModifierType::ResearchSpeed) / 100.0)
    }

    /// Get engineering speed multiplier (1.0 + bonus from modifiers)
    pub fn engineering_speed_multiplier(&self) -> f64 {
        1.0 + (self.get_modifier(ModifierType::EngineeringSpeed) / 100.0)
    }

    /// Get research bonus for a category (percentage)
    pub fn category_research_bonus(&self, category: TechCategory) -> f64 {
        self.get_modifier(ModifierType::CategoryResearchBonus(category))
    }
}

/// System to accumulate research and engineering points
pub fn update_research_points(
    sim_time: Res<SimulationTime>,
    mut research_state: ResMut<ResearchState>,
    research_buildings: Query<&ResearchBuilding>,
    engineering_facilities: Query<&EngineeringFacility>,
    mut last_time: Local<f64>,
) {
    let current_time = sim_time.elapsed_seconds();
    let delta_time = current_time - *last_time;
    *last_time = current_time;

    if delta_time <= 0.0 {
        return;
    }

    // Accumulate research points from buildings
    let research_per_second: f64 = research_buildings
        .iter()
        .map(|b| b.points_per_second)
        .sum();

    let research_multiplier = research_state.research_speed_multiplier();
    research_state.research_points_available +=
        research_per_second * delta_time * research_multiplier;

    // Accumulate engineering points from facilities
    let engineering_per_second: f64 = engineering_facilities
        .iter()
        .map(|f| f.points_per_second)
        .sum();

    let engineering_multiplier = research_state.engineering_speed_multiplier();
    research_state.engineering_points_available +=
        engineering_per_second * delta_time * engineering_multiplier;
}

/// System to advance active research projects
pub fn advance_research_projects(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut research_state: ResMut<ResearchState>,
    tech_data: Res<TechnologiesData>,
    mut projects: Query<(Entity, &mut ResearchProject, &ResearchTeam)>,
    mut last_time: Local<f64>,
) {
    let current_time = sim_time.elapsed_seconds();
    let delta_time = current_time - *last_time;
    *last_time = current_time;

    if delta_time <= 0.0 {
        return;
    }

    let mut completed_projects = Vec::new();

    for (entity, mut project, team) in projects.iter_mut() {
        if project.is_complete() {
            continue;
        }

        // Get technology info
        let tech = match tech_data.get_tech(&project.tech_id) {
            Some(t) => t,
            None => {
                warn!("Research project references unknown tech: {}", project.tech_id);
                continue;
            }
        };

        // Calculate effective research rate
        let base_rate = 1.0; // Base research rate per second
        let category_bonus = 1.0 + (research_state.category_research_bonus(tech.category) / 100.0);
        let team_efficiency = team.category_efficiency(tech.category) as f64;
        let global_multiplier = research_state.research_speed_multiplier();

        let effective_rate = base_rate * category_bonus * team_efficiency * global_multiplier;

        // Advance progress
        project.progress += effective_rate * delta_time;

        // Check if complete
        if project.is_complete() {
            info!(
                "Research project completed: {} by team '{}'",
                tech.name, team.name
            );
            completed_projects.push((entity, project.tech_id.clone()));
        }
    }

    // Process completed projects
    for (entity, tech_id) in completed_projects {
        research_state.unlock_tech(tech_id.clone());

        // Apply technology modifiers
        if let Some(tech) = tech_data.get_tech(&tech_id) {
            for modifier_def in &tech.modifiers {
                research_state.add_modifier(modifier_def.modifier_type.clone(), modifier_def.value);
            }
        }

        // Remove the project entity
        commands.entity(entity).despawn();
    }
}

/// System to advance active engineering projects
pub fn advance_engineering_projects(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut research_state: ResMut<ResearchState>,
    tech_data: Res<TechnologiesData>,
    mut projects: Query<(Entity, &mut EngineeringProject, &ResearchTeam)>,
    mut last_time: Local<f64>,
) {
    let current_time = sim_time.elapsed_seconds();
    let delta_time = current_time - *last_time;
    *last_time = current_time;

    if delta_time <= 0.0 {
        return;
    }

    let mut completed_projects = Vec::new();

    for (entity, mut project, team) in projects.iter_mut() {
        if project.is_complete() {
            continue;
        }

        // Calculate effective engineering rate
        let base_rate = 1.0; // Base engineering rate per second
        let team_efficiency = team.efficiency as f64;
        let global_multiplier = research_state.engineering_speed_multiplier();

        let effective_rate = base_rate * team_efficiency * global_multiplier;

        // Advance progress
        project.progress += effective_rate * delta_time;

        // Check if complete
        if project.is_complete() {
            if let Some(component) = tech_data.get_component(&project.component_id) {
                info!(
                    "Engineering project completed: {} by team '{}'",
                    component.name, team.name
                );
                completed_projects.push((entity, project.component_id.clone()));
            }
        }
    }

    // Process completed projects
    for (entity, component_id) in completed_projects {
        research_state.complete_component(component_id.clone());

        // Spawn component design entity
        if let Some(component_def) = tech_data.get_component(&component_id) {
            commands.spawn(ComponentDesign {
                id: component_def.id.clone(),
                name: component_def.name.clone(),
                description: component_def.description.clone(),
            });
        }

        // Remove the project entity
        commands.entity(entity).despawn();
    }
}

/// System to check and display newly unlocked technologies
pub fn check_unlocked_technologies(
    _tech_data: Res<TechnologiesData>,
    research_state: Res<ResearchState>,
    mut last_unlocked_count: Local<usize>,
) {
    let current_count = research_state.unlocked_technologies.len();

    if current_count > *last_unlocked_count {
        let newly_unlocked = current_count - *last_unlocked_count;
        info!("Unlocked {} new technolog(ies)", newly_unlocked);
        *last_unlocked_count = current_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_research_state_unlock() {
        let mut state = ResearchState::default();
        assert!(!state.is_unlocked("test_tech"));

        state.unlock_tech("test_tech".to_string());
        assert!(state.is_unlocked("test_tech"));
    }

    #[test]
    fn test_research_state_modifiers() {
        let mut state = ResearchState::default();

        state.add_modifier(ModifierType::ResearchSpeed, 10.0);
        assert_eq!(state.get_modifier(ModifierType::ResearchSpeed), 10.0);

        state.add_modifier(ModifierType::ResearchSpeed, 5.0);
        assert_eq!(state.get_modifier(ModifierType::ResearchSpeed), 15.0);
    }

    #[test]
    fn test_research_speed_multiplier() {
        let mut state = ResearchState::default();
        assert_eq!(state.research_speed_multiplier(), 1.0);

        state.add_modifier(ModifierType::ResearchSpeed, 20.0);
        assert_eq!(state.research_speed_multiplier(), 1.2);
    }

    #[test]
    fn test_component_completion() {
        let mut state = ResearchState::default();
        assert!(!state.is_component_completed("test_component"));

        state.complete_component("test_component".to_string());
        assert!(state.is_component_completed("test_component"));
    }
}
