use bevy::prelude::*;

use super::components::{Colony, ConstructionProject, PendingConstructionActions};
use super::data::{BuildingsData};
use super::types::BuildingType;
use super::ConstructionDebugSettings;
use crate::astronomy::OceanProperties;
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::economy::components::{LocalStockpile, Population};
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
/// Deducts resource costs from the **same-system** `LocalStockpile` pool
/// (all bodies in the same `SystemId`) when starting construction.
/// Falls back to the global budget if no local stockpiles exist.
///
/// In debug mode with `free_construction`, resource costs are bypassed.
/// In debug mode with `instant_build`, buildings are added immediately.
pub fn process_construction_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingConstructionActions>,
    mut colonies: Query<&mut Colony>,
    mut budget: ResMut<crate::economy::GlobalBudget>,
    buildings_data: Option<Res<BuildingsData>>,
    debug_settings: Res<ConstructionDebugSettings>,
    // For local stockpile support
    colony_sys_query: Query<Option<&crate::astronomy::components::SystemId>>,
    mut local_stockpile_query: Query<(
        Option<&crate::astronomy::components::SystemId>,
        &mut LocalStockpile,
    )>,
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
                let costs_raw = data.resource_costs(&building_type);
                if !costs_raw.is_empty() {
                    // Convert cost list to (ResourceType, f64) pairs; warn on unknown names
                    let costs_typed: Vec<(crate::economy::types::ResourceType, f64)> = costs_raw
                        .iter()
                        .filter_map(|(name, amt)| {
                            let rt = super::data::parse_resource_type(name);
                            if rt.is_none() {
                                warn!("Unknown resource type '{}' in build costs, skipping", name);
                            }
                            rt.map(|rt| (rt, *amt))
                        })
                        .collect();

                    // Find which system this colony is in
                    let sys_id = colony_sys_query
                        .get(colony_entity)
                        .ok()
                        .flatten()
                        .map(|s| s.0);

                    // Single pass: sum available amounts and, if sufficient, deduct in the same
                    // iteration by collecting (entity_index, rt, amount_to_deduct).
                    // Phase 1 — sum totals to check affordability.
                    let mut available: std::collections::HashMap<
                        crate::economy::types::ResourceType,
                        f64,
                    > = std::collections::HashMap::new();
                    for (sid_opt, ls) in local_stockpile_query.iter() {
                        let body_sys = sid_opt.map(|s| s.0);
                        if sys_id.is_none() || body_sys == sys_id {
                            for (rt, &amt) in &ls.stockpiles {
                                *available.entry(*rt).or_insert(0.0) += amt;
                            }
                        }
                    }

                    let can_pay_local = costs_typed
                        .iter()
                        .all(|(rt, need)| available.get(rt).copied().unwrap_or(0.0) >= *need);

                    if can_pay_local {
                        // Phase 2 — deduct. We use a separate `iter_mut` pass but only when
                        // we know the resources are available, so the extra iteration is only
                        // paid when a build actually proceeds.
                        let mut remaining: std::collections::HashMap<
                            crate::economy::types::ResourceType,
                            f64,
                        > = costs_typed.iter().cloned().collect();

                        for (sid_opt, mut ls) in local_stockpile_query.iter_mut() {
                            let body_sys = sid_opt.map(|s| s.0);
                            if sys_id.is_none() || body_sys == sys_id {
                                for (rt, need) in remaining.iter_mut() {
                                    if *need > 0.0 {
                                        let taken = ls.consume(*rt, *need);
                                        *need -= taken;
                                    }
                                }
                            }
                        }
                    } else {
                        // Fallback: try global budget
                        let costs_for_global: Vec<(String, f64)> = costs_raw.to_vec();
                        if !super::data::deduct_resources(&mut budget, &costs_for_global) {
                            warn!(
                                "Cannot build {}: insufficient resources",
                                building_type.display_name()
                            );
                            continue;
                        }
                    }
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

    // Establish new outpost colonies
    let outpost_requests: Vec<(Entity, String)> = actions.establish_outpost.drain(..).collect();
    for (body_entity, colony_name) in outpost_requests {
        // Don't double-establish a colony on an already-colonized body
        if colonies.get(body_entity).is_ok() {
            warn!(
                "Outpost establishment requested for {:?} but it already has a Colony component",
                body_entity
            );
            continue;
        }

        // Create the Colony component (no buildings yet — they will be queued)
        let colony = Colony::new(colony_name.clone(), 0.0);
        commands.entity(body_entity).insert(colony);

        // Insert an initial local stockpile (empty — resources must be transported)
        commands.entity(body_entity).insert(LocalStockpile::default());

        // Insert Population component so the growth system picks it up
        commands.entity(body_entity).insert(Population { count: 0.0 });

        // Queue the starter building package:
        //   - LifeSupport × 1      : basic air/water recycling
        //   - Housing × 1          : sleeping quarters for up to 250 K (outpost uses ~5 K)
        //   - FissionReactor × 2   : power from uranium (baseline + redundancy)
        //   - AgriDome × 2         : food for up to 8 K people (>5 K cap)
        const OUTPOST_BUILDINGS: &[BuildingType] = &[
            BuildingType::LifeSupport,
            BuildingType::Housing,
            BuildingType::FissionReactor,
            BuildingType::FissionReactor,
            BuildingType::AgriDome,
            BuildingType::AgriDome,
        ];
        for &btype in OUTPOST_BUILDINGS {
            commands.spawn(ConstructionProject::new(btype, body_entity));
        }

        info!(
            "Established outpost '{}' on {:?}; queued {} construction projects",
            colony_name,
            body_entity,
            OUTPOST_BUILDINGS.len()
        );
    }
}

/// System that applies colony population growth each tick.
///
/// Uses `SimulationTime` to calculate elapsed time and applies the
/// growth calculated by `Colony::population_growth_per_year`.
/// Bodies with liquid-water oceans receive a habitability bonus.
///
/// Food economy:
/// - Farm/AgriDome buildings produce Food into the colony's `LocalStockpile`
/// - Population consumes Food from the colony's `LocalStockpile`
/// - `food_factor` (0.5–1.0) is derived from stockpile adequacy:
///   - Stockpile ≥ 1 year of consumption → food_factor = 1.0
///   - Stockpile = 0 → food_factor = 0.5 (ship-supplied minimum)
pub fn update_colony_growth(
    mut colonies: Query<(Entity, &mut Colony, Option<&mut LocalStockpile>)>,
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

    // Process food per-colony when a LocalStockpile is present.
    // If no LocalStockpile exists (legacy / newly spawned colony), fall back
    // to the global budget so gameplay continues uninterrupted.
    for (entity, mut colony, local_opt) in colonies.iter_mut() {
        let food_prod = colony.food_production_per_year() * years_elapsed;
        let food_cons = colony.food_consumption_per_year() * years_elapsed;

        let food_factor = if let Some(mut ls) = local_opt {
            // --- Local stockpile path ---
            let cap = budget.effective_stockpile_cap(ResourceType::Food);
            if food_prod > 0.0 {
                ls.add_capped(ResourceType::Food, food_prod, cap);
            }
            if food_cons > 0.0 {
                ls.consume(ResourceType::Food, food_cons);
            }
            let reserve = ls.get(&ResourceType::Food);
            let annual_cons = colony.food_consumption_per_year();
            if annual_cons > 0.0 {
                let years_reserve = reserve / annual_cons;
                (0.5 + 0.5 * years_reserve.min(1.0)).min(1.0)
            } else {
                1.0
            }
        } else {
            // --- Global budget fallback ---
            if food_prod > 0.0 {
                budget.add_resource_capped(ResourceType::Food, food_prod);
            }
            if food_cons > 0.0 {
                let available = budget.get_stockpile(&ResourceType::Food);
                let consumed = food_cons.min(available);
                if consumed > 0.0 {
                    budget.consume_resource(ResourceType::Food, consumed);
                }
            }
            let reserve = budget.get_stockpile(&ResourceType::Food);
            let annual_cons = colony.food_consumption_per_year();
            if annual_cons > 0.0 {
                let years_reserve = reserve / annual_cons;
                (0.5 + 0.5 * years_reserve.min(1.0)).min(1.0)
            } else {
                1.0
            }
        };

        let base_growth = colony.population_growth_per_year(food_factor) * years_elapsed;
        let ocean_modifier = ocean_query
            .get(entity)
            .map(|o| o.habitability_modifier())
            .unwrap_or(1.0);
        colony.population += base_growth * ocean_modifier;

        // Hard cap: population cannot exceed available housing capacity.
        // Colonies without any housing buildings (housing == 0) are uncapped
        // to allow the player time to build infrastructure for brand-new outposts.
        let housing = colony.housing_capacity();
        if housing > 0.0 {
            colony.population = colony.population.min(housing);
        }
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

/// System that deducts maintenance resources from the colony's `LocalStockpile`.
///
/// Each building consumes a small amount of resources per year for upkeep.
/// Resources are deducted proportionally based on elapsed simulation time.
/// If the local stockpile runs out, the global budget is used as fallback.
/// If that is also empty, buildings still operate (no hard shutdown).
pub fn deduct_maintenance_resources(
    mut budget: ResMut<crate::economy::GlobalBudget>,
    mut colonies: Query<(&Colony, Option<&mut LocalStockpile>)>,
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

    for (colony, mut local_opt) in colonies.iter_mut() {
        for (building_type, count) in &colony.buildings {
            let maintenance = data.maintenance_resources(building_type);
            for (resource_name, annual_amount) in maintenance {
                let amount = annual_amount * f64::from(*count) * years_elapsed;
                if let Some(rt) = super::data::parse_resource_type(resource_name) {
                    if let Some(ref mut ls) = local_opt {
                        // Use local stockpile
                        ls.consume(rt, amount);
                    } else {
                        // Fallback to global budget
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
}

/// System that syncs `Population.count` from `Colony.population`.
///
/// `Colony.population` is the authoritative population value (updated by
/// `update_colony_growth`).  The `Population` ECS component is what the UI
/// queries to display population counts.  This system keeps them in sync so
/// the top-right population counter and dossier panel stay up-to-date.
pub fn sync_population_from_colony(
    mut query: Query<(&Colony, &mut Population)>,
) {
    for (colony, mut pop) in query.iter_mut() {
        pop.count = colony.population;
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
