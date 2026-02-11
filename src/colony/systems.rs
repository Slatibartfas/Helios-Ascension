use bevy::prelude::*;

use super::components::{Colony, ConstructionProject, PendingConstructionActions};
use super::types::BuildingType;
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::ui::SimulationTime;

/// System that advances construction projects based on factory output.
///
/// Each factory on the colony contributes 10 build points per year of
/// simulation time. Projects are advanced in queue order (oldest first).
pub fn advance_construction(
    mut commands: Commands,
    mut colonies: Query<&mut Colony>,
    mut projects: Query<(Entity, &mut ConstructionProject)>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
) {
    let current_elapsed = sim_time.elapsed_seconds();
    let dt = current_elapsed - *last_elapsed;
    *last_elapsed = current_elapsed;

    if dt <= 0.0 {
        return;
    }

    let years_elapsed = dt / SECONDS_PER_YEAR;
    if years_elapsed <= 0.0 {
        return;
    }

    // Gather per-colony build points
    // Factory count determines build rate; minimum 1 BP/year for ship-supplied colonies
    let mut colony_bp: Vec<(Entity, f64)> = Vec::new();
    for (_entity, mut _project) in projects.iter_mut() {
        let colony_entity = _project.colony_entity;
        if !colony_bp.iter().any(|(e, _)| *e == colony_entity) {
            if let Ok(colony) = colonies.get(colony_entity) {
                let factories = colony.building_count(BuildingType::Factory) as f64;
                // Base: 1 BP/year (ship supply), + 10 per factory
                let bp = (1.0 + factories * 10.0) * years_elapsed;
                colony_bp.push((colony_entity, bp));
            }
        }
    }

    // Distribute build points to projects (oldest first via entity order)
    for (colony_entity, mut available_bp) in colony_bp {
        // Collect project entities for this colony
        let mut project_entities: Vec<Entity> = projects
            .iter()
            .filter(|(_, p)| p.colony_entity == colony_entity)
            .map(|(e, _)| e)
            .collect();
        project_entities.sort(); // deterministic order

        for proj_entity in project_entities {
            if available_bp <= 0.0 {
                break;
            }

            if let Ok((_, mut project)) = projects.get_mut(proj_entity) {
                let needed = project.required - project.progress;
                let applied = needed.min(available_bp);
                project.progress += applied;
                available_bp -= applied;

                if project.is_complete() {
                    // Add building to colony
                    if let Ok(mut colony) = colonies.get_mut(colony_entity) {
                        colony.add_building(project.building_type);
                        info!(
                            "Construction complete: {} at {}",
                            project.building_type.display_name(),
                            colony.name
                        );
                    }
                    commands.entity(proj_entity).despawn();
                }
            }
        }
    }
}

/// System that processes pending construction actions from the UI.
///
/// Creates new `ConstructionProject` entities and handles cancellations.
pub fn process_construction_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingConstructionActions>,
    colonies: Query<&Colony>,
) {
    // Start new projects
    for (colony_entity, building_type) in actions.start_construction.drain(..) {
        if colonies.get(colony_entity).is_ok() {
            commands.spawn(ConstructionProject::new(building_type, colony_entity));
            info!(
                "Started construction: {}",
                building_type.display_name()
            );
        }
    }

    // Cancel projects
    for entity in actions.cancel_construction.drain(..) {
        commands.entity(entity).despawn();
    }
}

/// System that applies colony population growth each tick.
///
/// Uses `SimulationTime` to calculate elapsed time and applies the
/// growth calculated by `Colony::population_growth_per_year`.
pub fn update_colony_growth(
    mut colonies: Query<&mut Colony>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
) {
    let current_elapsed = sim_time.elapsed_seconds();
    let dt = current_elapsed - *last_elapsed;
    *last_elapsed = current_elapsed;

    if dt <= 0.0 {
        return;
    }

    let years_elapsed = dt / SECONDS_PER_YEAR;
    if years_elapsed <= 0.0 {
        return;
    }

    for mut colony in colonies.iter_mut() {
        let growth = colony.population_growth_per_year() * years_elapsed;
        colony.population += growth;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::components::Colony;

    #[test]
    fn test_colony_growth_calculation() {
        let mut colony = Colony::new("Test".to_string(), 10_000.0);
        colony.add_building(BuildingType::HabitatDome); // 50k capacity
        colony.add_building(BuildingType::AgriDome); // food

        let growth = colony.population_growth_per_year();
        assert!(
            growth > 0.0,
            "Colony with housing and food should grow: {}",
            growth
        );

        // Growth should be reasonable (< 10% per year for small colony)
        let rate = growth / colony.population;
        assert!(
            rate < 0.10,
            "Growth rate should be < 10%: {}",
            rate
        );
    }

    #[test]
    fn test_logistics_penalty_on_mining() {
        let mut colony = Colony::new("Test".to_string(), 1000.0);
        // No mines, no logistics → no demand → 1.0
        assert_eq!(colony.mining_output_multiplier(), 1.0);

        // Add mines without logistics
        for _ in 0..5 {
            colony.add_building(BuildingType::Mine);
        }
        let without_logistics = colony.mining_output_multiplier();
        assert!(without_logistics < 1.0, "Should be penalised without logistics");

        // Add mass driver
        colony.add_building(BuildingType::MassDriver);
        let with_logistics = colony.mining_output_multiplier();
        assert!(
            with_logistics > without_logistics,
            "Should improve with logistics"
        );
    }

    #[test]
    fn test_construction_project_progress_percent() {
        let entity = Entity::from_raw(1);
        let mut project = ConstructionProject::new(BuildingType::Factory, entity);

        assert_eq!(project.progress_percent(), 0.0);

        project.progress = project.required / 2.0;
        assert!((project.progress_percent() - 0.5).abs() < 0.001);

        project.progress = project.required;
        assert_eq!(project.progress_percent(), 1.0);
        assert!(project.is_complete());
    }
}
