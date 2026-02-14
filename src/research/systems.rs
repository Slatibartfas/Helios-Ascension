use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::ui::SimulationTime;
use crate::colony::{Colony, BuildingsData};

use super::components::{
    ComponentDesign, EngineeringFacility, EngineeringProject, ResearchBuilding, ResearchProject,
    ResearchTeam, ResearchTeamCapacity,
};
use super::data::TechnologiesData;
use super::types::{ModifierType, TechCategory, TechnologyId};
use super::PendingResearchActions;

/// Resource that tracks global research state
#[derive(Resource, Debug, Clone, Default)]
pub struct ResearchState {
    /// Technologies that have been unlocked
    pub unlocked_technologies: HashSet<TechnologyId>,
    /// Component designs that have been completed
    pub completed_components: HashSet<String>,
    /// Available research points pool (unallocated RP accumulates here)
    pub research_points_available: f64,
    /// Available engineering points pool
    pub engineering_points_available: f64,
    /// Active modifiers from technologies
    pub active_modifiers: HashMap<ModifierType, f64>,
    /// Current RP generation rate (RP per game-second) for UI display
    pub rp_rate_per_second: f64,
    /// Current EP generation rate (EP per game-second) for UI display
    pub ep_rate_per_second: f64,
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

/// Base RP generated per year of game time (without buildings)
const BASE_RP_PER_YEAR: f64 = 2000.0;
/// Base EP generated per year of game time (without buildings)
const BASE_EP_PER_YEAR: f64 = 1000.0;
/// Seconds in a Julian year
const SECONDS_PER_YEAR: f64 = 31_557_600.0;

/// System to compute current RP/EP generation rates and accumulate EP.
/// RP accumulation is handled in advance_research_projects to account for allocations.
pub fn update_research_points(
    sim_time: Res<SimulationTime>,
    mut research_state: ResMut<ResearchState>,
    research_buildings: Query<&ResearchBuilding>,
    engineering_facilities: Query<&EngineeringFacility>,
    colony_query: Query<&Colony>,
    buildings_data: Option<Res<BuildingsData>>,
    mut last_time: Local<f64>,
) {
    let current_time = sim_time.elapsed_seconds();
    let delta_time = current_time - *last_time;
    *last_time = current_time;

    if delta_time <= 0.0 {
        return;
    }
    
    let seconds_per_month = SECONDS_PER_YEAR / 12.0;

    // Compute RP rate (for display; actual distribution is in advance_research_projects)
    let base_rp_rate = BASE_RP_PER_YEAR / SECONDS_PER_YEAR;
    let mut building_rp: f64 = research_buildings.iter().map(|b| b.points_per_second).sum();
    
    // Add colony RP
    if let Some(data) = &buildings_data {
        for colony in colony_query.iter() {
             for (building_type, &count) in &colony.buildings {
                if count == 0 { continue; }
                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "ResearchSpeed" {
                             // Value is RP/month -> RP/sec
                            building_rp += (modifier.value * count as f64) / seconds_per_month;
                        }
                    }
                }
            }
        }
    }

    let rp_multiplier = research_state.research_speed_multiplier();
    research_state.rp_rate_per_second = (base_rp_rate + building_rp) * rp_multiplier;

    // Compute and accumulate engineering points
    let base_ep_rate = BASE_EP_PER_YEAR / SECONDS_PER_YEAR;
    let mut building_ep: f64 = engineering_facilities.iter().map(|f| f.points_per_second).sum();
    
    // Add colony EP
    if let Some(data) = &buildings_data {
        for colony in colony_query.iter() {
             for (building_type, &count) in &colony.buildings {
                if count == 0 { continue; }
                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "EngineeringSpeed" {
                            // Value is EP/month -> EP/sec
                            building_ep += (modifier.value * count as f64) / seconds_per_month;
                        }
                    }
                }
            }
        }
    }

    let ep_multiplier = research_state.engineering_speed_multiplier();
    research_state.ep_rate_per_second = (base_ep_rate + building_ep) * ep_multiplier;
    research_state.engineering_points_available +=
        research_state.ep_rate_per_second * delta_time;
}

/// System to advance active research projects using RP income.
///
/// RP is generated from a base rate plus research buildings, then distributed
/// among active projects according to their allocation percentages.
/// Unallocated RP accumulates in `research_points_available`.
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

    // Total RP income this frame (rate was computed in update_research_points)
    let total_rp_this_frame = research_state.rp_rate_per_second * delta_time;

    // First pass: compute total allocation of active, incomplete projects
    let total_allocation: f64 = projects
        .iter()
        .filter(|(_, p, _)| !p.is_complete() && p.active)
        .map(|(_, p, _)| p.rp_allocation_percent)
        .sum();

    let mut completed_projects = Vec::new();

    // Second pass: distribute RP and advance projects
    for (entity, mut project, team) in projects.iter_mut() {
        if project.is_complete() || !project.active {
            continue;
        }

        // Skip zero-allocation projects
        if project.rp_allocation_percent <= 0.0 || total_allocation <= 0.0 {
            continue;
        }

        // Calculate this project's share of RP income
        let share = total_rp_this_frame * (project.rp_allocation_percent / total_allocation);

        // Apply team efficiency and category bonuses
        let tech = tech_data.get_tech(&project.tech_id);
        let category_bonus = tech
            .map(|t| 1.0 + (research_state.category_research_bonus(t.category) / 100.0))
            .unwrap_or(1.0);
        let team_efficiency = tech
            .map(|t| team.category_efficiency(t.category) as f64)
            .unwrap_or(1.0);

        project.progress += share * category_bonus * team_efficiency;

        if project.is_complete() {
            if let Some(t) = tech {
                info!(
                    "Research project completed: {} by team '{}'",
                    t.name, team.name
                );
            }
            completed_projects.push((entity, project.tech_id.clone()));
        }
    }

    // Unallocated RP goes to the pool
    let unallocated_fraction = (1.0 - total_allocation).max(0.0);
    research_state.research_points_available += total_rp_this_frame * unallocated_fraction;

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

        // Redistribute allocation among remaining active projects
        redistribute_allocations(&mut projects);
    }
}

/// Evenly redistribute allocation percentages among all active, incomplete projects.
fn redistribute_allocations(
    projects: &mut Query<(Entity, &mut ResearchProject, &ResearchTeam)>,
) {
    let active_count = projects
        .iter()
        .filter(|(_, p, _)| !p.is_complete() && p.active)
        .count();

    if active_count == 0 {
        return;
    }

    let equal_share = 1.0 / active_count as f64;
    for (_, mut project, _) in projects.iter_mut() {
        if !project.is_complete() && project.active {
            project.rp_allocation_percent = equal_share;
        }
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

/// System to process pending research actions queued from the UI.
///
/// For each requested tech ID it spawns an entity with a [`ResearchProject`]
/// and a default [`ResearchTeam`], which the existing
/// [`advance_research_projects`] system will then advance every frame.
pub fn process_pending_research(
    mut commands: Commands,
    mut pending: ResMut<PendingResearchActions>,
    tech_data: Res<TechnologiesData>,
    research_state: Res<ResearchState>,
    team_capacity: Res<ResearchTeamCapacity>,
    mut existing_projects: Query<(Entity, &mut ResearchProject, &ResearchTeam)>,
) {
    if pending.start_research.is_empty() {
        return;
    }

    // Collect tech IDs already being researched so we don't duplicate.
    let active_tech_ids: HashSet<&str> = existing_projects
        .iter()
        .map(|(_, p, _)| p.tech_id.as_str())
        .collect();

    // Count currently active research projects (for team capacity)
    let active_count = existing_projects
        .iter()
        .filter(|(_, p, _)| p.active)
        .count();

    let mut spawned = 0usize;

    for tech_id in pending.start_research.drain(..) {
        // Guard: skip if already unlocked or already in progress.
        if research_state.is_unlocked(&tech_id) || active_tech_ids.contains(tech_id.as_str()) {
            continue;
        }

        // Check team capacity
        if active_count + spawned >= team_capacity.max_research_teams {
            warn!(
                "Cannot start research: all {} team slots are in use",
                team_capacity.max_research_teams
            );
            continue;
        }

        let tech = match tech_data.get_tech(&tech_id) {
            Some(t) => t,
            None => {
                warn!("Pending research references unknown tech: {}", tech_id);
                continue;
            }
        };

        // Verify prerequisites are met.
        let unlocked: Vec<_> = research_state.unlocked_technologies.iter().cloned().collect();
        if !tech_data.check_prerequisites(&tech_id, &unlocked) {
            warn!(
                "Cannot start research on '{}': prerequisites not met",
                tech.name
            );
            continue;
        }

        info!("Starting research on: {}", tech.name);

        // Spawn a combined entity with project + default team.
        commands.spawn((
            ResearchProject {
                tech_id: tech_id.clone(),
                progress: 0.0,
                required_points: tech.research_cost,
                team_id: Entity::PLACEHOLDER,
                rp_allocation_percent: 1.0, // Will be redistributed below
                active: true,
            },
            ResearchTeam::new_research(
                format!("Research: {}", tech.name),
                "Default Scientist".to_string(),
                Some(tech.category),
            ),
        ));

        spawned += 1;
    }

    // Redistribute allocation evenly after spawning new projects
    if spawned > 0 {
        let new_active_count = existing_projects
            .iter()
            .filter(|(_, p, _)| p.active && !p.is_complete())
            .count()
            + spawned;
        if new_active_count > 0 {
            let equal_share = 1.0 / new_active_count as f64;
            for (_, mut project, _) in existing_projects.iter_mut() {
                if project.active && !project.is_complete() {
                    project.rp_allocation_percent = equal_share;
                }
            }
        }
    }
}

/// System to process stop/resume research actions.
pub fn process_stop_research(
    mut pending: ResMut<PendingResearchActions>,
    mut projects: Query<(Entity, &mut ResearchProject, &ResearchTeam)>,
) {
    // Process stops
    if !pending.stop_research.is_empty() {
        let stop_ids: HashSet<String> = pending.stop_research.drain(..).collect();
        for (_, mut project, _) in projects.iter_mut() {
            if stop_ids.contains(&project.tech_id) {
                project.active = false;
                project.rp_allocation_percent = 0.0;
                info!("Stopped research on: {}", project.tech_id);
            }
        }
        // Redistribute among remaining active projects
        redistribute_allocations(&mut projects);
    }

    // Process resumes
    if !pending.resume_research.is_empty() {
        let resume_ids: HashSet<String> = pending.resume_research.drain(..).collect();
        for (_, mut project, _) in projects.iter_mut() {
            if resume_ids.contains(&project.tech_id) {
                project.active = true;
                info!("Resumed research on: {}", project.tech_id);
            }
        }
        redistribute_allocations(&mut projects);
    }
}

/// System to process allocation percentage updates from the UI.
pub fn process_allocation_updates(
    mut pending: ResMut<PendingResearchActions>,
    mut projects: Query<(Entity, &mut ResearchProject, &ResearchTeam)>,
) {
    if pending.update_allocations.is_empty() {
        return;
    }

    for (tech_id, new_alloc) in pending.update_allocations.drain(..) {
        for (_, mut project, _) in projects.iter_mut() {
            if project.tech_id == tech_id && project.active {
                project.rp_allocation_percent = new_alloc.clamp(0.0, 1.0);
            }
        }
    }

    // Normalize allocations so they sum to 1.0
    let total: f64 = projects
        .iter()
        .filter(|(_, p, _)| p.active && !p.is_complete())
        .map(|(_, p, _)| p.rp_allocation_percent)
        .sum();

    if total > 0.0 && (total - 1.0).abs() > 0.001 {
        let scale = 1.0 / total;
        for (_, mut project, _) in projects.iter_mut() {
            if project.active && !project.is_complete() {
                project.rp_allocation_percent *= scale;
            }
        }
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
