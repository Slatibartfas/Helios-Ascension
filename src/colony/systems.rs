use bevy::prelude::*;
use std::collections::HashMap;

use super::components::{
    Colony, ColonyEnvironmentCosts, ConstructionProject, PendingConstructionActions,
};
use super::data::BuildingsData;
use super::types::BuildingType;
use super::ConstructionDebugSettings;
use crate::astronomy::OceanProperties;
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::economy::components::LocalStockpile;
use crate::economy::components::Population;
use crate::economy::logistics::{
    PendingResourceRequests, RequestPriority, RequestState, ResourceRequest,
};
use crate::economy::types::ResourceType;
use crate::ui::SimulationTime;

/// Snapshot of "years remaining at the current draw" for every consumed
/// resource on every colony.  Written by [`compute_depletion_timeline`] and
/// read by the construction-panel UI (GRA-22d).
#[derive(Resource, Debug, Clone, Default)]
pub struct DepletionTimeline {
    /// `colony_entity → resource → years_remaining`.
    /// Missing entries mean either no draw or no local stockpile to deplete.
    pub by_colony: HashMap<Entity, HashMap<ResourceType, f64>>,
}

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
                // Skip projects waiting for a resource delivery.
                if project.awaiting_resources {
                    continue;
                }

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
///
/// Resource handling:
/// - If the colony's **local** `LocalStockpile` can afford the costs, resources
///   are deducted immediately and the project starts normally.
/// - If the local stockpile is insufficient, the system generates one
///   `ResourceRequest` per missing resource (Construction priority) and spawns
///   the project with `awaiting_resources = true`.  The project will not
///   accumulate build points until the delivery arrives.
/// - Same-system pool fallback is retained as a secondary check when no
///   colony-local stockpile component exists (legacy / debug path).
/// - In debug mode with `free_construction`, resource costs are bypassed.
/// - In debug mode with `instant_build`, buildings are added immediately.
pub fn process_construction_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingConstructionActions>,
    mut colonies: Query<&mut Colony>,
    buildings_data: Option<Res<BuildingsData>>,
    debug_settings: Res<ConstructionDebugSettings>,
    colony_sys_query: Query<Option<&crate::astronomy::components::SystemId>>,
    mut local_stockpile_query: Query<(
        Entity,
        Option<&crate::astronomy::components::SystemId>,
        &mut LocalStockpile,
    )>,
    mut resource_requests: ResMut<PendingResourceRequests>,
    sim_time: Res<SimulationTime>,
) {
    let now = sim_time.elapsed_seconds();

    // Start new projects
    for (colony_entity, building_type) in actions.start_construction.drain(..) {
        if colonies.get(colony_entity).is_err() {
            continue;
        }

        // Check and deduct resource costs (unless debug free_construction)
        let free = debug_settings.enabled && debug_settings.free_construction;

        // Track whether this project is blocked waiting for resource deliveries.
        let mut awaiting = false;
        let mut blocking_request_ids: Vec<u64> = Vec::new();

        if !free {
            if let Some(ref data) = buildings_data {
                let costs_raw = data.resource_costs(&building_type);
                if !costs_raw.is_empty() {
                    // Convert cost list to (ResourceType, f64) pairs.
                    let costs_typed: Vec<(ResourceType, f64)> = costs_raw
                        .iter()
                        .filter_map(|(name, amt)| {
                            let rt = super::data::parse_resource_type(name);
                            if rt.is_none() {
                                warn!("Unknown resource type '{}' in build costs, skipping", name);
                            }
                            rt.map(|rt| (rt, *amt))
                        })
                        .collect();

                    // Get the local stockpile for this specific colony entity.
                    let can_pay_local = local_stockpile_query
                        .get(colony_entity)
                        .map(|(_, _, ls)| costs_typed.iter().all(|(rt, need)| ls.get(rt) >= *need))
                        .unwrap_or(false);

                    if can_pay_local {
                        // Deduct from the colony's own stockpile.
                        if let Ok((_, _, mut ls)) = local_stockpile_query.get_mut(colony_entity) {
                            for (rt, need) in &costs_typed {
                                ls.consume(*rt, *need);
                            }
                        }
                    } else {
                        // Determine which resources are missing from the colony's local stockpile
                        // and which can be covered by the same-system pool.
                        let sys_id = colony_sys_query
                            .get(colony_entity)
                            .ok()
                            .flatten()
                            .map(|s| s.0);

                        // Get current local stockpile for the colony.
                        let colony_local: std::collections::HashMap<ResourceType, f64> =
                            local_stockpile_query
                                .get(colony_entity)
                                .map(|(_, _, ls)| {
                                    ls.stockpiles.iter().map(|(k, v)| (*k, *v)).collect()
                                })
                                .unwrap_or_default();

                        // Sum the available resources across the same system (for pool check).
                        let mut system_available: std::collections::HashMap<ResourceType, f64> =
                            std::collections::HashMap::new();
                        for (_, sid_opt, ls) in local_stockpile_query.iter() {
                            let body_sys = sid_opt.map(|s| s.0);
                            if sys_id.is_none() || body_sys == sys_id {
                                for (rt, &amt) in &ls.stockpiles {
                                    *system_available.entry(*rt).or_insert(0.0) += amt;
                                }
                            }
                        }

                        let can_pay_system = costs_typed.iter().all(|(rt, need)| {
                            system_available.get(rt).copied().unwrap_or(0.0) >= *need
                        });

                        if can_pay_system {
                            // Draw from system pool (current behaviour preserved).
                            let mut remaining: std::collections::HashMap<ResourceType, f64> =
                                costs_typed.iter().cloned().collect();
                            for (_, sid_opt, mut ls) in local_stockpile_query.iter_mut() {
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
                            // Resources not fully available in the system.
                            // Do NOT partially deduct from the system pool — that would leave
                            // stockpiles inconsistent while the project waits for delivery.
                            // Instead, request the full cost and let the delivery system add
                            // resources to the local stockpile before construction advances.
                            let colony_name = colonies
                                .get(colony_entity)
                                .map(|c| c.name.clone())
                                .unwrap_or_else(|_| format!("{colony_entity:?}"));

                            // Generate one request per missing resource type.
                            // `costs_typed` contains the *full* cost; we request the full
                            // amount so that construction can proceed once everything arrives.
                            for (rt, full_cost) in &costs_typed {
                                if *full_cost <= 0.0 {
                                    continue;
                                }
                                // Only add a request if there isn't one already for this colony+resource.
                                if resource_requests.has_open_request_for(colony_entity, *rt) {
                                    awaiting = true;
                                    continue;
                                }

                                // Credit the colony's existing local stock toward the cost;
                                // only request the remainder that truly needs to be delivered.
                                let already_local: f64 =
                                    colony_local.get(rt).copied().unwrap_or(0.0);
                                let need_delivered = (*full_cost - already_local).max(0.0);

                                if need_delivered > 0.0 {
                                    let req_id = resource_requests.add(ResourceRequest {
                                        id: 0,
                                        destination_body: colony_entity,
                                        destination_name: colony_name.clone(),
                                        resource: *rt,
                                        amount_mt: need_delivered,
                                        priority: RequestPriority::Construction,
                                        state: RequestState::Pending,
                                        in_transit_mt: 0.0,
                                        eta_seconds: None,
                                        assigned_company_idx: None,
                                        created_at_seconds: now,
                                        source_body: None,
                                        linked_project: None, // filled in after project spawn
                                        payment_made: false,
                                        completed_at_seconds: None,
                                    });
                                    blocking_request_ids.push(req_id);
                                    awaiting = true;

                                    warn!(
                                        "Construction '{}' at {}: {:?} {:.1} Mt not available in system — requesting delivery",
                                        building_type.display_name(),
                                        colony_name,
                                        rt,
                                        need_delivered
                                    );
                                }
                            }

                            // If nothing was actually missing (somehow), don't block.
                            if blocking_request_ids.is_empty() {
                                awaiting = false;
                            }
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
            let mut project = ConstructionProject::new(building_type, colony_entity);
            project.awaiting_resources = awaiting;
            // Store first blocking request ID for status display.
            project.blocking_request_id = blocking_request_ids.first().copied();

            let proj_entity = commands.spawn(project).id();

            // Back-fill linked_project on every generated request.
            for req_id in &blocking_request_ids {
                if let Some(req) = resource_requests.find_by_id_mut(*req_id) {
                    req.linked_project = Some(proj_entity);
                }
            }

            if awaiting {
                info!(
                    "Construction '{}' queued but awaiting resource deliveries",
                    building_type.display_name()
                );
            } else {
                info!("Started construction: {}", building_type.display_name());
            }
        }
    }

    // Cancel projects
    for entity in actions.cancel_construction.drain(..) {
        commands.entity(entity).despawn();
    }

    // Establish new outpost colonies
    let outpost_requests: Vec<_> = actions.establish_outpost.drain(..).collect();
    for req in outpost_requests {
        let (body_entity, colony_name, needs_oxygen) =
            (req.body_entity, req.colony_name, req.needs_oxygen);

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
        commands
            .entity(body_entity)
            .insert(LocalStockpile::default());

        // Attach a MinimumStockpile with basic life-support thresholds so that
        // private freighters automatically keep the outpost stocked.
        let mut minimum = crate::economy::MinimumStockpile::default();
        minimum.set(ResourceType::Food, 5_000.0);
        minimum.set(ResourceType::Water, 2.0);
        commands.entity(body_entity).insert(minimum);

        // Insert Population component so the growth system picks it up
        commands
            .entity(body_entity)
            .insert(Population { count: 0.0 });

        // Attach environment costs:
        //   • O₂: 0.0001 Mt/person/yr on vacuum/non-breathable worlds (same scale as food)
        //   • Water: 0.00005 Mt/person/yr on all outposts
        commands
            .entity(body_entity)
            .insert(crate::colony::components::ColonyEnvironmentCosts {
                oxygen_per_person_per_year: if needs_oxygen { 0.0001 } else { 0.0 },
                water_per_person_per_year: 0.00005,
            });

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
            // Queue through the normal construction action pipeline so that
            // resource costs are checked and ResourceRequests are generated when
            // the outpost stockpile is empty (i.e. always for a new outpost).
            actions.start_construction.push((body_entity, btype));
        }

        info!(
            "Established outpost '{}' on {:?} (needs_oxygen={}); queued {} construction projects",
            colony_name,
            body_entity,
            needs_oxygen,
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
        // ColonyDevelopment yield multiplier (Outpost × 0.10 … Civilisation × 1.00)
        // scales production and population growth.  Food *consumption* is
        // per-capita and not yield-scaled — a population of N eats the same
        // amount of food regardless of how industrialised their colony is.
        let yield_mult = colony.effective_yield_multiplier();
        let food_prod = colony.food_production_per_year() * yield_mult * years_elapsed;
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

        let base_growth =
            colony.population_growth_per_year(food_factor) * yield_mult * years_elapsed;
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
        // Per GRA-22 §4.7: wealth and operating cost both scale with the
        // colony's development yield multiplier.  An outpost at ×0.10
        // produces one-tenth of a civilisation's wealth *and* costs
        // one-tenth to maintain.
        let yield_mult = colony.effective_yield_multiplier();
        total_income += colony.wealth_generation_per_year() * yield_mult;
        total_expenses += colony.operating_cost_per_year() * yield_mult;
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
        // Per GRA-22 §4.7: maintenance is scaled by the colony's development
        // yield multiplier.  Same factor as output — a small base costs less
        // in absolute terms to keep alive.
        let yield_mult = colony.effective_yield_multiplier();
        for (building_type, count) in &colony.buildings {
            let maintenance = data.maintenance_resources(building_type);
            for (resource_name, annual_amount) in maintenance {
                let amount = annual_amount * f64::from(*count) * years_elapsed * yield_mult;
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
pub fn sync_population_from_colony(mut query: Query<(&Colony, &mut Population)>) {
    for (colony, mut pop) in query.iter_mut() {
        pop.count = colony.population;
    }
}

/// System that deducts per-person environment costs (O₂, water) from colonies
/// that carry a [`ColonyEnvironmentCosts`] component.
///
/// These costs apply to all outposts:
/// - **Water**: always consumed (recycling losses in a closed habitat).
/// - **Oxygen**: consumed only when the body has no breathable atmosphere
///   (the component stores 0.0 for breathable worlds).
///
/// Resources are drawn from the body's `LocalStockpile` when present, or from
/// the global budget as a fallback.
pub fn deduct_environment_costs(
    mut colonies: Query<(
        &Colony,
        &crate::colony::components::ColonyEnvironmentCosts,
        Option<&mut LocalStockpile>,
    )>,
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

    for (colony, env_costs, mut local_opt) in colonies.iter_mut() {
        let pop = colony.population;
        if pop <= 0.0 {
            continue;
        }

        // Water consumption
        let water_needed = env_costs.water_per_person_per_year * pop * years_elapsed;
        if water_needed > 0.0 {
            if let Some(ref mut ls) = local_opt {
                ls.consume(ResourceType::Water, water_needed);
            } else {
                let avail = budget.get_stockpile(&ResourceType::Water);
                budget.consume_resource(ResourceType::Water, water_needed.min(avail));
            }
        }

        // Oxygen consumption (only on non-breathable worlds)
        let o2_needed = env_costs.oxygen_per_person_per_year * pop * years_elapsed;
        if o2_needed > 0.0 {
            if let Some(ref mut ls) = local_opt {
                ls.consume(ResourceType::Oxygen, o2_needed);
            } else {
                let avail = budget.get_stockpile(&ResourceType::Oxygen);
                budget.consume_resource(ResourceType::Oxygen, o2_needed.min(avail));
            }
        }
    }
}

/// System that derives "years remaining at the current draw" for every
/// consumed resource on every colony and writes the snapshot to
/// [`DepletionTimeline`].
///
/// Joins `ColonyPlugin::build` in the same `chain()` group as the other
/// colony systems; runs after [`deduct_maintenance_resources`] and
/// [`deduct_environment_costs`] so the per-colony draw it sees is the
/// same draw that has just been applied.  Output is consumed by the
/// construction-panel UI (GRA-22d).
///
/// Population-driven draws (food, water, O₂) are *not* yield-scaled —
/// they are per-capita biological needs.  Building-driven draws
/// (maintenance) are yield-scaled by the same factor as the production
/// they cost.
pub fn compute_depletion_timeline(
    colonies: Query<(
        Entity,
        &Colony,
        Option<&ColonyEnvironmentCosts>,
        Option<&LocalStockpile>,
    )>,
    buildings_data: Option<Res<BuildingsData>>,
    mut timeline: ResMut<DepletionTimeline>,
) {
    timeline.by_colony.clear();

    let data = match buildings_data {
        Some(ref d) if !d.definitions.is_empty() => d,
        _ => return,
    };

    for (entity, colony, env_opt, local_opt) in colonies.iter() {
        let mut annual_draw: HashMap<ResourceType, f64> = HashMap::new();
        let yield_mult = colony.effective_yield_multiplier();

        // 1. Building maintenance — yield-scaled (per GRA-22 §4.7).
        for (building_type, count) in &colony.buildings {
            if *count == 0 {
                continue;
            }
            let maintenance = data.maintenance_resources(building_type);
            for (resource_name, annual_amount) in maintenance {
                if let Some(rt) = super::data::parse_resource_type(resource_name) {
                    let amount = annual_amount * f64::from(*count) * yield_mult;
                    *annual_draw.entry(rt).or_insert(0.0) += amount;
                }
            }
        }

        // 2. Per-capita environment draws (water, O₂) — biological, not scaled.
        if let Some(env) = env_opt {
            let pop = colony.population;
            if pop > 0.0 {
                if env.water_per_person_per_year > 0.0 {
                    let water = env.water_per_person_per_year * pop;
                    *annual_draw.entry(ResourceType::Water).or_insert(0.0) += water;
                }
                if env.oxygen_per_person_per_year > 0.0 {
                    let o2 = env.oxygen_per_person_per_year * pop;
                    *annual_draw.entry(ResourceType::Oxygen).or_insert(0.0) += o2;
                }
            }
        }

        // 3. Food consumption — per-capita, not scaled.
        let food_cons = colony.food_consumption_per_year();
        if food_cons > 0.0 {
            *annual_draw.entry(ResourceType::Food).or_insert(0.0) += food_cons;
        }

        // Compute years_remaining against the colony's local stockpile.
        let mut per_resource: HashMap<ResourceType, f64> = HashMap::new();
        if let Some(ls) = local_opt {
            for (rt, &stockpile) in &ls.stockpiles {
                if stockpile <= 0.0 {
                    continue;
                }
                if let Some(&draw) = annual_draw.get(rt) {
                    if draw > 0.0 {
                        per_resource.insert(*rt, stockpile / draw);
                    }
                }
            }
        }

        timeline.by_colony.insert(entity, per_resource);
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

    // ── compute_depletion_timeline (GRA-24) ─────────────────────────────

    /// Spin up a minimal Bevy app that owns a `BuildingsData` resource, a `DepletionTimeline`
    /// resource, and one colony with a known stockpile and a known draw. Returns the `App`
    /// so the test can query the timeline after running the system.
    fn build_depletion_app(
        colony: Colony,
        stockpile: std::collections::HashMap<ResourceType, f64>,
    ) -> App {
        use crate::colony::data::BuildingsData;
        let mut app = App::new();
        app.init_resource::<DepletionTimeline>();
        // Synthesise an empty BuildingsData (the system early-returns when
        // the resource is empty; the assertion below uses a custom draw
        // that doesn't depend on RON data, so an empty resource is fine
        // for the negative case).
        app.insert_resource(BuildingsData::default());

        let world = app.world_mut();
        let mut entity = world.spawn((colony, LocalStockpile::default()));
        // Populate the LocalStockpile via direct field write — the helper
        // `add` method is on `LocalStockpile` itself.
        if let Some(mut ls) = entity.get_mut::<LocalStockpile>() {
            for (rt, amt) in &stockpile {
                ls.stockpiles.insert(*rt, *amt);
            }
        }
        let _ = entity.id();
        app
    }

    #[test]
    fn test_compute_depletion_timeline_known_draw() {
        // Known draw test (GRA-24 acceptance criterion).
        // Setup: a Civilisation colony with a single Farm and a 2,000 Mt
        // local food stockpile.  At ×1.00 yield the Farm draws exactly
        // 1,000 Mt/yr → years_remaining = 2.000 yr.
        let mut colony = Colony::new_civilisation("Earth".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::Farm);

        let mut stockpile = std::collections::HashMap::new();
        stockpile.insert(ResourceType::Food, 2_000.0);

        let mut app = build_depletion_app(colony, stockpile);

        // Drive the system manually — we only need a single tick of the
        // computation, not the full chain.
        use bevy::ecs::schedule::Schedule;
        let mut sched = Schedule::default();
        sched.add_systems(compute_depletion_timeline);
        sched.run(app.world_mut());

        // Read the timeline.
        let timeline = app.world().resource::<DepletionTimeline>();
        assert_eq!(timeline.by_colony.len(), 1, "expected one colony entry");
        let per_colony = timeline
            .by_colony
            .values()
            .next()
            .expect("colony should have a timeline entry");
        let years = per_colony
            .get(&ResourceType::Food)
            .expect("Food should have a years-remaining entry");
        assert!(
            (years - 2.0).abs() < 1e-6,
            "2,000 Mt / 1,000 Mt/yr = 2.0 yr, got {}",
            years,
        );
    }

    #[test]
    fn test_compute_depletion_timeline_outpost_yields_ten_year_runway() {
        // Same draw as the test above, but the colony is an Outpost (×0.10).
        // The Farm draws 1,000 × 0.10 = 100 Mt/yr, so a 2,000 Mt stockpile
        // lasts 20 years — i.e. 10× longer than the civilisation case.
        let mut colony = Colony::new("Moon".to_string(), 5_000.0);
        colony.add_building(BuildingType::Farm);

        let mut stockpile = std::collections::HashMap::new();
        stockpile.insert(ResourceType::Food, 2_000.0);

        let mut app = build_depletion_app(colony, stockpile);

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(compute_depletion_timeline);
        sched.run(app.world_mut());

        let timeline = app.world().resource::<DepletionTimeline>();
        let per_colony = timeline
            .by_colony
            .values()
            .next()
            .expect("colony should have a timeline entry");
        let years = per_colony
            .get(&ResourceType::Food)
            .expect("Food should have a years-remaining entry");
        assert!(
            (years - 20.0).abs() < 1e-6,
            "2,000 Mt / 100 Mt/yr = 20.0 yr (Outpost ×0.10), got {}",
            years,
        );
    }

    #[test]
    fn test_compute_depletion_timeline_empty_when_no_buildings_data() {
        // Negative case: no BuildingsData → system early-returns with an
        // empty timeline, so the UI chip renders as "no data" instead of
        // false negatives.
        let colony = Colony::new("Moon".to_string(), 5_000.0);
        let mut app = build_depletion_app(colony, std::collections::HashMap::new());

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(compute_depletion_timeline);
        sched.run(app.world_mut());

        let timeline = app.world().resource::<DepletionTimeline>();
        assert!(timeline.by_colony.is_empty());
    }
}
