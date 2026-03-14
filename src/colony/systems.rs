use bevy::prelude::*;

use super::components::{Colony, ConstructionProject, PendingConstructionActions};
use super::data::{deduct_resources, BuildingsData};
use super::types::BuildingType;
use super::ConstructionDebugSettings;
use crate::astronomy::OceanProperties;
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::economy::types::ResourceType;
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
/// Deducts resource costs from the global stockpile when starting construction.
/// In debug mode with `free_construction`, resource costs are bypassed.
/// In debug mode with `instant_build`, buildings are added immediately.
pub fn process_construction_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingConstructionActions>,
    mut colonies: Query<&mut Colony>,
    mut budget: ResMut<crate::economy::GlobalBudget>,
    buildings_data: Option<Res<BuildingsData>>,
    debug_settings: Res<ConstructionDebugSettings>,
) {
    // Start new projects
    for (colony_entity, building_type) in actions.start_construction.drain(..) {
        if colonies.get(colony_entity).is_err() {
            continue;
        }

        // Check and deduct resource costs (unless debug free_construction)
        let free = debug_settings.enabled && debug_settings.free_construction;
        if !free {
            if let Some(ref data) = buildings_data {
                let costs = data.resource_costs(&building_type);
                if !costs.is_empty() && !deduct_resources(&mut budget, costs) {
                    warn!(
                        "Cannot build {}: insufficient resources",
                        building_type.display_name()
                    );
                    continue;
                }
            }
        }

        // Instant build in debug mode
        if debug_settings.enabled && debug_settings.instant_build {
            if let Ok(mut colony) = colonies.get_mut(colony_entity) {
                colony.add_building(building_type);
                info!(
                    "Instant build: {} at {}",
                    building_type.display_name(),
                    colony.name
                );
            }
        } else {
            commands.spawn(ConstructionProject::new(building_type, colony_entity));
            info!("Started construction: {}", building_type.display_name());
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
/// Bodies with liquid-water oceans receive a habitability bonus.
///
/// Food economy:
/// - Farm/AgriDome buildings produce Food into the global stockpile
/// - Population consumes Food proportional to colony population
/// - `food_factor` (0.5–1.0) is derived from stockpile adequacy:
///   - Stockpile ≥ 1 year of consumption → food_factor = 1.0
///   - Stockpile = 0 → food_factor = 0.5 (ship-supplied minimum)
pub fn update_colony_growth(
    mut colonies: Query<(Entity, &mut Colony)>,
    ocean_query: Query<&OceanProperties>,
    mut budget: ResMut<crate::economy::GlobalBudget>,
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

    // --- Food production: add Food from agricultural buildings ---
    let mut total_food_production = 0.0_f64;
    let mut total_food_consumption = 0.0_f64;

    for (_entity, colony) in colonies.iter() {
        total_food_production += colony.food_production_per_year();
        total_food_consumption += colony.food_consumption_per_year();
    }

    // Add produced food to stockpile, capped so it doesn't accumulate infinitely
    let food_produced = total_food_production * years_elapsed;
    if food_produced > 0.0 {
        budget.add_resource_capped(ResourceType::Food, food_produced);
    }

    // Consume food from stockpile
    let food_to_consume = total_food_consumption * years_elapsed;
    if food_to_consume > 0.0 {
        let available = budget.get_stockpile(&ResourceType::Food);
        let consumed = food_to_consume.min(available);
        if consumed > 0.0 {
            budget.consume_resource(ResourceType::Food, consumed);
        }
    }

    // Compute global food_factor: how adequate is the food supply?
    // Stockpile measured against 1 year of consumption.
    // food_factor ranges from 0.5 (starvation) to 1.0 (fully fed).
    let food_stockpile = budget.get_stockpile(&ResourceType::Food);
    let food_factor = if total_food_consumption > 0.0 {
        let years_of_reserve = food_stockpile / total_food_consumption;
        // 0 reserves → 0.5, 1+ year reserves → 1.0 (linear interpolation)
        (0.5 + 0.5 * years_of_reserve.min(1.0)).min(1.0)
    } else {
        1.0 // No population to feed
    };

    for (entity, mut colony) in colonies.iter_mut() {
        let base_growth = colony.population_growth_per_year(food_factor) * years_elapsed;
        let ocean_modifier = ocean_query
            .get(entity)
            .map(|o| o.habitability_modifier())
            .unwrap_or(1.0);
        colony.population += base_growth * ocean_modifier;
    }
}

/// System that updates the global treasury from colony income and expenses.
///
/// Aggregates wealth generation and operating costs from all colonies,
/// updates income/expenses on the global budget, and adjusts the treasury.
pub fn update_treasury(
    mut budget: ResMut<crate::economy::GlobalBudget>,
    colonies: Query<&Colony>,
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

    let mut total_income = 0.0;
    let mut total_expenses = 0.0;

    for colony in colonies.iter() {
        total_income += colony.wealth_generation_per_year();
        total_expenses += colony.operating_cost_per_year();
    }

    budget.income_per_year = total_income;
    budget.expenses_per_year = total_expenses;

    let balance = total_income - total_expenses;
    budget.treasury += balance * years_elapsed;
}

/// System that deducts maintenance resources from the global stockpile.
///
/// Each building consumes a small amount of resources per year for upkeep.
/// Resources are deducted proportionally based on elapsed simulation time.
/// If resources run out, buildings still operate but the stockpile goes to zero.
pub fn deduct_maintenance_resources(
    mut budget: ResMut<crate::economy::GlobalBudget>,
    colonies: Query<&Colony>,
    buildings_data: Option<Res<BuildingsData>>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
) {
    let data = match buildings_data {
        Some(ref d) if !d.definitions.is_empty() => d,
        _ => return,
    };

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

    // Aggregate maintenance costs across all colonies
    for colony in colonies.iter() {
        for (building_type, count) in &colony.buildings {
            let maintenance = data.maintenance_resources(building_type);
            for (resource_name, annual_amount) in maintenance {
                let amount = annual_amount * f64::from(*count) * years_elapsed;
                if let Some(rt) = super::data::parse_resource_type(resource_name) {
                    // Deduct what we can; don't prevent operation if stockpile is empty
                    let available = budget.get_stockpile(&rt);
                    let to_deduct = amount.min(available);
                    if to_deduct > 0.0 {
                        budget.consume_resource(rt, to_deduct);
                    }
                }
            }
        }
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

        let growth = colony.population_growth_per_year(1.0);
        assert!(
            growth > 0.0,
            "Colony with housing and food should grow: {}",
            growth
        );

        // Growth should be reasonable (< 10% per year for small colony)
        let rate = growth / colony.population;
        assert!(rate < 0.10, "Growth rate should be < 10%: {}", rate);
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
        assert!(
            without_logistics < 1.0,
            "Should be penalised without logistics"
        );

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
        let entity = Entity::from_raw_u32(1).unwrap();
        let mut project = ConstructionProject::new(BuildingType::Factory, entity);

        assert_eq!(project.progress_percent(), 0.0);

        project.progress = project.required / 2.0;
        assert!((project.progress_percent() - 0.5).abs() < 0.001);

        project.progress = project.required;
        assert_eq!(project.progress_percent(), 1.0);
        assert!(project.is_complete());
    }
}
