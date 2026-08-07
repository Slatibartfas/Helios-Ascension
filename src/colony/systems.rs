use bevy::prelude::*;
use std::collections::HashMap;

use super::ConstructionEvent;

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
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct DepletionTimeline {
    /// `colony_entity → resource → years_remaining`.
    /// Missing entries mean either no draw or no local stockpile to deplete.
    pub by_colony: HashMap<Entity, HashMap<ResourceType, f64>>,
}

/// Per-colony synergy state, recomputed each tick from current building
/// composition (GRA-22c plan §4.6).  `bonuses` is an additive map keyed
/// by effect name (e.g. `"MiningEfficiency"` → `+0.10`).  Multiple
/// buildings contributing to the same effect sum.
///
/// Read by gameplay systems (mining, construction) and by the
/// construction-panel UI (GRA-22d "Synergies active" badge).
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct ColonySynergies {
    /// `colony_entity → SynergyState`.
    pub by_colony: HashMap<Entity, SynergyState>,
}

/// Additive bonuses per effect name for a single colony.  Multiple
/// bonuses on the same effect from different buildings sum.
#[derive(Debug, Clone, Default, PartialEq, Reflect)]
pub struct SynergyState {
    /// `effect_name → additive_bonus`.  E.g. `+0.10` mining efficiency.
    pub bonuses: HashMap<String, f64>,
}

impl SynergyState {
    /// Add `bonus` to the entry for `effect`, initialising to 0.0 first.
    pub fn add(&mut self, effect: &str, bonus: f64) {
        *self.bonuses.entry(effect.to_string()).or_insert(0.0) += bonus;
    }
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
    mut construction_events: MessageWriter<ConstructionEvent>,
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
                    // Fire the construction event for the notifications
                    // bridge (PR-C, GRA-137) and any future sim
                    // listeners. The colony reference is borrowed
                    // above; we emit unconditionally (even if the
                    // colony reference failed) so the player still
                    // gets a toast for the building being added.
                    construction_events.write(ConstructionEvent::Completed {
                        colony: colony_entity,
                        building: project.building_type,
                    });
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
/// - Local-only model (GRA-31 PR-A): same-system bodies are **not** drained
///   to fund construction.  Resources must be delivered from external sources
///   (player freighters, AI shipping companies) via the `ResourceRequest`
///   pipeline.
/// - In debug mode with `free_construction`, resource costs are bypassed.
/// - In debug mode with `instant_build`, buildings are added immediately.
pub fn process_construction_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingConstructionActions>,
    mut colonies: Query<&mut Colony>,
    buildings_data: Option<Res<BuildingsData>>,
    debug_settings: Res<ConstructionDebugSettings>,
    mut local_stockpile_query: Query<(
        Entity,
        Option<&crate::astronomy::components::SystemId>,
        &mut LocalStockpile,
    )>,
    mut resource_requests: ResMut<PendingResourceRequests>,
    sim_time: Res<SimulationTime>,
    mut dirty: ResMut<crate::economy::DirtyBodies>,
    mut construction_events: MessageWriter<ConstructionEvent>,
) {
    let now = sim_time.elapsed_seconds();

    // v0.5.2 Mining tab: direct inventory edits (no BP / build time).
    // Applied immediately so the next frame's UI shows the new count.
    // Positive delta = add N, negative = remove N (clamped to current).
    for (colony_entity, building_type, delta) in actions.mining_edits.drain(..) {
        let Ok(mut colony) = colonies.get_mut(colony_entity) else {
            continue;
        };
        if delta > 0 {
            for _ in 0..delta {
                colony.add_building(building_type);
            }
        } else if delta < 0 {
            colony.remove_buildings(building_type, (-delta) as u32);
        }
        // Emit a single ConstructionEvent per edit batch so the
        // production / upkeep / workforce systems can refresh.
        construction_events.write(ConstructionEvent::Completed {
            colony: colony_entity,
            building: building_type,
        });
        // Mark this body dirty so the v2 extract path picks up
        // the new production rate on the next tick.
        dirty.mark(colony_entity, crate::economy::DirtyReason::Body);
    }

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
                            // Mark the colony's body dirty — the
                            // v2 extract path needs to know this
                            // body's stockpile changed (even if
                            // it ends up empty after the consume).
                            dirty.mark_stockpile(colony_entity);
                        }
                    } else {
                        // Local stockpile is short.  Local-only construction model
                        // (GRA-31 PR-A): do not drain resources from other bodies in
                        // the same system.  Instead, request the full missing cost
                        // and let the delivery system add resources to the local
                        // stockpile before construction advances.

                        // Snapshot the colony's local stockpile so we only request
                        // the *remainder* (cost minus what is already on hand).
                        let colony_local: std::collections::HashMap<ResourceType, f64> =
                            local_stockpile_query
                                .get(colony_entity)
                                .map(|(_, _, ls)| {
                                    ls.stockpiles.iter().map(|(k, v)| (*k, *v)).collect()
                                })
                                .unwrap_or_default();

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
                            let already_local: f64 = colony_local.get(rt).copied().unwrap_or(0.0);
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
                                    assignee_fleet_id: None,
                                });
                                blocking_request_ids.push(req_id);
                                awaiting = true;

                                warn!(
                                    "Construction '{}' at {}: {:?} {:.1} Mt not available locally — requesting delivery",
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

        // GRA-25 tier replacement (per GRA-22c plan §4.6): when a player
        // queues a building whose `replaces` field names a tier-(N-1)
        // predecessor and the colony has at least one of it, decrement
        // the predecessor by one *before* the project is spawned.  The
        // new building still pays its own resource cost (its own
        // `resource_costs`); the predecessor is removed because the
        // upgrade is strictly better, not because the cost is refunded.
        if let Some(ref data) = buildings_data {
            if let Some(def) = data.get(&building_type) {
                if let Some(prev_id) = def.replaces.as_deref() {
                    if let Some(prev_type) = super::data::parse_building_type(prev_id) {
                        if let Ok(mut colony) = colonies.get_mut(colony_entity) {
                            if colony.remove_one_building(prev_type) {
                                info!(
                                    "Tier replacement: {} removed from {} (replaced by {})",
                                    prev_id,
                                    colony.name,
                                    building_type.display_name()
                                );
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
        // Defaults match the GRA-31 life-support scale: Water=100 (O₂ parity)
        // and Food=500 (~5× O₂ default, comfortably below starting stockpile
        // so the auto-freight loop does not fire on day 1).
        let mut minimum = crate::economy::MinimumStockpile::default();
        minimum.set(ResourceType::Food, 500.0);
        minimum.set(ResourceType::Water, 100.0);
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

        // GRA-787: notify the milestone consumer that an outpost now
        // exists. We use a dedicated `ConstructionEvent` variant rather
        // than a synthetic `BuildingType::Outpost` completion because the
        // building-completion path is reserved for the
        // `ConstructionProject::progress >= required` finish line. See
        // `src/colony/events.rs` for the producer-gap rationale. The
        // milestone consumer in `crate::survey::milestones` is
        // idempotent, so a duplicate emit from re-running the system
        // (e.g. a re-established outpost) is a no-op.
        construction_events.write(ConstructionEvent::OutpostEstablished {
            colony: body_entity,
            body: body_entity,
        });
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
    mut dirty: ResMut<crate::economy::DirtyBodies>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
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
        // v3.7: use yearly *rates* (Mt/yr) for the food ratio so
        // growth is driven by supply/demand balance, not by
        // stockpile reserves alone.
        let food_prod_per_year = colony.food_production_per_year(&buildings_data) * yield_mult;
        let food_cons_per_year = colony.food_consumption_per_year(&buildings_data);
        let food_ratio = if food_cons_per_year > 0.0 {
            food_prod_per_year / food_cons_per_year
        } else {
            1.0
        };

        // Convert to per-tick values for the stockpile/budget update.
        let food_prod = food_prod_per_year * years_elapsed;
        let food_cons = food_cons_per_year * years_elapsed;

        if let Some(mut ls) = local_opt {
            // --- Local stockpile path ---
            let cap = budget.effective_stockpile_cap(ResourceType::Food);
            if food_prod > 0.0 {
                ls.add_capped(ResourceType::Food, food_prod, cap);
            }
            if food_cons > 0.0 {
                ls.consume(ResourceType::Food, food_cons);
            }
            // Colony grew / shrank — the LocalStockpile
            // changed (even if net-zero) AND the Colony
            // / Population may have changed. Mark dirty
            // with `Multiple` so the extract path
            // populates every applicable divergence
            // field.
            dirty.mark(entity, crate::economy::DirtyReason::Multiple);
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
        }

        let base_growth =
            colony.population_growth_per_year(food_ratio, &buildings_data) * years_elapsed;
        let ocean_modifier = ocean_query
            .get(entity)
            .map(|o| o.habitability_modifier())
            .unwrap_or(1.0);
        colony.population += base_growth * ocean_modifier;

        // Hard cap: population cannot exceed available housing capacity.
        // Colonies without any housing buildings (housing == 0) are uncapped
        // to allow the player time to build infrastructure for brand-new outposts.
        let housing = colony.housing_capacity(&buildings_data);
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
    buildings_data: Res<crate::colony::data::BuildingsData>,
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
        total_income += colony.wealth_generation_per_year(&buildings_data) * yield_mult;
        total_expenses += colony.operating_cost_per_year(&buildings_data) * yield_mult;
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
    mut colonies: Query<(Entity, &Colony, Option<&mut LocalStockpile>)>,
    buildings_data: Option<Res<BuildingsData>>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    mut dirty: ResMut<crate::economy::DirtyBodies>,
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

    for (entity, colony, mut local_opt) in colonies.iter_mut() {
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
        // Maintenance drained the LocalStockpile. Mark
        // dirty so the v2 extract path captures the
        // mutation.
        dirty.mark_stockpile(entity);
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
        Entity,
        &Colony,
        &crate::colony::components::ColonyEnvironmentCosts,
        Option<&mut LocalStockpile>,
    )>,
    mut budget: ResMut<crate::economy::GlobalBudget>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    mut dirty: ResMut<crate::economy::DirtyBodies>,
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

    for (entity, colony, env_costs, mut local_opt) in colonies.iter_mut() {
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

        // Mark dirty — environment drain is a
        // player-visible mutation that the regen chain
        // would otherwise revert on next load. Covers
        // both water and oxygen branches.
        dirty.mark_stockpile(entity);
    }
}

/// v3.7: System that deducts per-capita consumer consumption
/// (Iron, Copper, Aluminum, Polymers, Methane, etc.) from each
/// colony's `LocalStockpile` (or `GlobalBudget` fallback).
///
/// The per-capita rates come from the RON `colony_constants.
/// per_capita_consumption` block, calibrated so 8.2B people
/// consume ~70% of USGS 2024 / worldsteel 2024 / OECD 2024 / WNA
/// 2024 world demand; the remaining ~30% goes to industry,
/// maintenance, feedstock, and power generation.
///
/// This is what makes population *drive* the consumer economy
/// rather than just consume food: as the player expands, the
/// stockpile draw scales linearly with population.
pub fn deduct_population_consumption(
    mut colonies: Query<(Entity, &Colony, Option<&mut LocalStockpile>)>,
    mut budget: ResMut<crate::economy::GlobalBudget>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    mut dirty: ResMut<crate::economy::DirtyBodies>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
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

    for (entity, colony, mut local_opt) in colonies.iter_mut() {
        let pop = colony.population;
        if pop <= 0.0 {
            continue;
        }

        // Per-capita consumer draw for this tick.
        let per_capita = colony.per_capita_consumption_per_year(&buildings_data);

        for (resource, per_year) in per_capita {
            let needed = per_year * years_elapsed;
            if needed <= 0.0 {
                continue;
            }
            if let Some(ref mut ls) = local_opt {
                ls.consume(resource, needed);
            } else {
                let avail = budget.get_stockpile(&resource);
                budget.consume_resource(resource, needed.min(avail));
            }
        }

        // Mark dirty so the regen chain doesn't revert
        // the consumption on the next load.
        dirty.mark_stockpile(entity);
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

    for (entity, colony, env_opt, local_opt) in colonies.iter() {
        let mut annual_draw: HashMap<ResourceType, f64> = HashMap::new();
        let yield_mult = colony.effective_yield_multiplier();

        // 1. Building maintenance — yield-scaled (per GRA-22 §4.7).
        //    The maintenance loop is a no-op when `BuildingsData` is
        //    absent or empty; the per-capita and food loops below still
        //    run so the UI gets biological needs even before the data
        //    file is loaded.
        if let Some(ref data) = buildings_data {
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
        let food_cons = colony.food_consumption_per_year(buildings_data.as_ref().unwrap());
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

        // Only register colonies that have at least one tracked draw, so
        // the UI can distinguish "no draw to show" from "draw present".
        if !per_resource.is_empty() {
            timeline.by_colony.insert(entity, per_resource);
        }
    }
}

/// Recompute per-colony synergy bonuses (GRA-22c plan §4.6).
///
/// For each colony:
///   - For every owned building with a non-empty `synergy` list, walk the
///     rules.  For each rule, count how many buildings in the colony
///     belong to `requires_line` (per the `line` field on the def).  If
///     that count meets `rule.count`, add `rule.bonus` to
///     `state.bonuses[rule.effect]`.
///   - Store the resulting `SynergyState` in `ColonySynergies.by_colony`.
///
/// O(buildings²) per colony in the worst case; at 51 buildings and a
/// handful of synergy rules per building the absolute cost is
/// microseconds.  The system is cheap enough to run every tick from
/// the colony chain — no event-driven scheduler needed.
pub fn recompute_synergies(
    colonies: Query<(Entity, &Colony)>,
    buildings_data: Option<Res<BuildingsData>>,
    mut synergies: ResMut<ColonySynergies>,
) {
    synergies.by_colony.clear();

    let Some(data) = buildings_data else {
        return;
    };
    if data.definitions.is_empty() {
        return;
    }

    for (entity, colony) in colonies.iter() {
        let mut state = SynergyState::default();
        for building_type in colony.buildings.keys() {
            let Some(def) = data.get(building_type) else {
                continue;
            };
            for rule in &def.synergy {
                let have = data.count_in_line(&colony.buildings, &rule.requires_line);
                if have >= u32::from(rule.count) {
                    state.add(&rule.effect, rule.bonus);
                }
            }
        }
        if !state.bonuses.is_empty() {
            synergies.by_colony.insert(entity, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::components::Colony;
    use crate::colony::data::BuildingsData;

    /// v3.6: Colony methods now take `&BuildingsData`; load the real
    /// RON data so per-build values match the production calibration.
    fn data() -> BuildingsData {
        BuildingsData::load_for_tests()
    }

    #[test]
    fn test_colony_growth_calculation() {
        let mut colony = Colony::new("Test".to_string(), 10_000.0);
        colony.add_building(BuildingType::HabitatDome); // 50k capacity
        colony.add_building(BuildingType::AgriDome); // food

        let growth = colony.population_growth_per_year(1.0, &data());
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
        assert_eq!(colony.mining_output_multiplier(&data()), 1.0);

        // Add mines without logistics
        for _ in 0..5 {
            colony.add_building(BuildingType::IronMine);
        }
        let without_logistics = colony.mining_output_multiplier(&data());
        assert!(
            without_logistics < 1.0,
            "Should be penalised without logistics"
        );

        // Add mass driver
        colony.add_building(BuildingType::MassDriver);
        let with_logistics = colony.mining_output_multiplier(&data());
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
    ///
    /// `buildings_data` carries the maintenance draw profile; pass an
    /// empty `BuildingsData::default()` to test the no-draw case.
    fn build_depletion_app(
        colony: Colony,
        stockpile: std::collections::HashMap<ResourceType, f64>,
        buildings_data: BuildingsData,
    ) -> App {
        let mut app = App::new();
        app.init_resource::<DepletionTimeline>();
        app.insert_resource(buildings_data);

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

    /// Build a `BuildingsData` containing a single Refinery definition
    /// with the maintenance profile used in the GRA-24 acceptance test
    /// (Water 0.5 Mt/yr, plus a few trace materials so the test would
    /// also catch accidental zero-allocation).
    fn refinery_buildings_data() -> BuildingsData {
        use crate::colony::data::AtmosphereKind;
        use crate::colony::data::BuildingDefinition;
        use std::collections::HashMap;
        let mut defs: HashMap<BuildingType, BuildingDefinition> = HashMap::new();
        defs.insert(
            BuildingType::CopperMine,
            BuildingDefinition {
                id: "Refinery".to_string(),
                display_name: "Refinery".to_string(),
                description: "Test refines raw ores into usable materials".to_string(),
                icon: "🏭".to_string(),
                category: "Industry".to_string(),
                build_points: 600.0,
                workforce: 6000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 133.0)],
                maintenance_resources: vec![
                    ("Water".to_string(), 0.5),
                    ("Sulfur".to_string(), 0.008),
                ],
                modifiers: vec![],
                power_demand_mw: 500.0,
                tier: 0,
                line: None,
                replaces: None,
                synergy: vec![],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        BuildingsData {
            definitions: defs,
            ..Default::default()
        }
    }

    #[test]
    fn test_compute_depletion_timeline_known_draw() {
        // Known draw test (GRA-24 acceptance criterion).
        // Setup: a Civilisation colony with one Refinery and a 5.0 Mt
        // local Water stockpile.  At ×1.00 yield the Refinery draws
        // exactly 0.5 Mt/yr of Water → years_remaining = 10.0 yr.
        let mut colony = Colony::new_civilisation("Earth".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::CopperMine);

        let mut stockpile = std::collections::HashMap::new();
        stockpile.insert(ResourceType::Water, 5.0);

        let mut app = build_depletion_app(colony, stockpile, refinery_buildings_data());

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
            .get(&ResourceType::Water)
            .expect("Water should have a years-remaining entry");
        assert!(
            (years - 10.0).abs() < 1e-6,
            "5.0 Mt / 0.5 Mt/yr = 10.0 yr (Civilisation ×1.00), got {}",
            years,
        );
    }

    #[test]
    fn test_compute_depletion_timeline_outpost_yields_ten_year_runway() {
        // Same draw as the test above, but the colony is an Outpost
        // (×0.10).  The Refinery draws 0.5 × 0.10 = 0.05 Mt/yr of Water,
        // so a 5.0 Mt stockpile lasts 100 years — i.e. 10× longer than
        // the civilisation case.
        let mut colony = Colony::new("Moon".to_string(), 5_000.0);
        colony.add_building(BuildingType::CopperMine);

        let mut stockpile = std::collections::HashMap::new();
        stockpile.insert(ResourceType::Water, 5.0);

        let mut app = build_depletion_app(colony, stockpile, refinery_buildings_data());

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
            .get(&ResourceType::Water)
            .expect("Water should have a years-remaining entry");
        assert!(
            (years - 100.0).abs() < 1e-6,
            "5.0 Mt / 0.05 Mt/yr = 100.0 yr (Outpost ×0.10), got {}",
            years,
        );
    }

    #[test]
    fn test_compute_depletion_timeline_empty_when_no_demand() {
        // Negative case: a colony with no buildings, no local stockpile,
        // and no per-capita draw against anything it owns → the timeline
        // is empty.  (Without the `if !per_resource.is_empty()` guard the
        // system would still insert an empty entry per colony; the guard
        // keeps the UI's "no data" signal distinct from "draws are
        // tracked but trivial".)
        let colony = Colony::new("Moon".to_string(), 5_000.0);
        let mut app = build_depletion_app(
            colony,
            std::collections::HashMap::new(),
            BuildingsData::default(),
        );

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(compute_depletion_timeline);
        sched.run(app.world_mut());

        let timeline = app.world().resource::<DepletionTimeline>();
        assert!(timeline.by_colony.is_empty());
    }

    // ── GRA-25 (GRA-22c): tier replacement + synergy recompute ─────────

    /// Build a `BuildingsData` with Farm and HydroponicsFarm in the same
    /// line, where HydroponicsFarm declares `replaces: "Farm"` and tier 1.
    /// Maintenance entries are 4 distinct resources so the audit accepts
    /// them.
    fn farm_line_buildings_data() -> BuildingsData {
        use crate::colony::data::AtmosphereKind;
        use crate::colony::data::BuildingDefinition;
        use std::collections::HashMap;
        let mut defs: HashMap<BuildingType, BuildingDefinition> = HashMap::new();
        defs.insert(
            BuildingType::Farm,
            BuildingDefinition {
                id: "Farm".to_string(),
                display_name: "Farm".to_string(),
                description: "T0 farm".to_string(),
                icon: "🌾".to_string(),
                category: "Industry".to_string(),
                build_points: 200.0,
                workforce: 1000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 1.0)],
                maintenance_resources: vec![
                    ("Water".to_string(), 0.5),
                    ("Phosphorus".to_string(), 0.01),
                    ("Polymers".to_string(), 0.002),
                    ("Sulfur".to_string(), 0.001),
                    ("Food".to_string(), 0.05),
                ],
                modifiers: vec![],
                power_demand_mw: 50.0,
                tier: 0,
                line: Some("Farm".to_string()),
                replaces: None,
                synergy: vec![],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        defs.insert(
            BuildingType::AgriDome,
            BuildingDefinition {
                id: "AgriDome".to_string(),
                display_name: "Hydroponics Farm".to_string(),
                description: "T1 farm".to_string(),
                icon: "🌱".to_string(),
                category: "Industry".to_string(),
                build_points: 400.0,
                workforce: 800,
                required_tech: "hydroponics".to_string(),
                resource_costs: vec![("Iron".to_string(), 1.5), ("Polymers".to_string(), 0.5)],
                maintenance_resources: vec![
                    ("Water".to_string(), 0.4),
                    ("Phosphorus".to_string(), 0.008),
                    ("Polymers".to_string(), 0.005),
                    ("Sulfur".to_string(), 0.0008),
                    ("Food".to_string(), 0.04),
                ],
                modifiers: vec![],
                power_demand_mw: 80.0,
                tier: 1,
                line: Some("Farm".to_string()),
                replaces: Some("Farm".to_string()),
                synergy: vec![],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        BuildingsData {
            definitions: defs,
            ..Default::default()
        }
    }

    /// Build a `BuildingsData` with Mine (with synergy rules),
    /// Refinery, and Factory — enough to exercise the Civ-VI-style
    /// "Mine with 2 Refineries" + "Mine with 1 Factory" pattern from
    /// the plan §4.6.
    fn mine_synergy_buildings_data() -> BuildingsData {
        use crate::colony::data::AtmosphereKind;
        use crate::colony::data::{BuildingDefinition, SynergyRule};
        use std::collections::HashMap;
        let mut defs: HashMap<BuildingType, BuildingDefinition> = HashMap::new();
        defs.insert(
            BuildingType::IronMine,
            BuildingDefinition {
                id: "Mine".to_string(),
                display_name: "Mine".to_string(),
                description: "Surface mine".to_string(),
                icon: "⛏".to_string(),
                category: "Industry".to_string(),
                build_points: 400.0,
                workforce: 2000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 5.0)],
                maintenance_resources: vec![
                    ("Iron".to_string(), 0.01),
                    ("Copper".to_string(), 0.005),
                    ("Water".to_string(), 0.05),
                    ("Polymers".to_string(), 0.002),
                    ("Sulfur".to_string(), 0.0005),
                ],
                modifiers: vec![],
                power_demand_mw: 250.0,
                tier: 0,
                line: Some("Mine".to_string()),
                replaces: None,
                synergy: vec![
                    SynergyRule {
                        requires_line: "Refinery".to_string(),
                        count: 2,
                        effect: "MiningEfficiency".to_string(),
                        bonus: 0.10,
                    },
                    SynergyRule {
                        requires_line: "Factory".to_string(),
                        count: 1,
                        effect: "ConstructionSpeed".to_string(),
                        bonus: 0.05,
                    },
                ],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        defs.insert(
            BuildingType::CopperMine,
            BuildingDefinition {
                id: "Refinery".to_string(),
                display_name: "Refinery".to_string(),
                description: "Refines ore".to_string(),
                icon: "🏭".to_string(),
                category: "Industry".to_string(),
                build_points: 600.0,
                workforce: 6000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 1.0)],
                maintenance_resources: vec![
                    ("Water".to_string(), 0.5),
                    ("Iron".to_string(), 0.01),
                    ("Copper".to_string(), 0.005),
                    ("Polymers".to_string(), 0.002),
                    ("Sulfur".to_string(), 0.008),
                ],
                modifiers: vec![],
                power_demand_mw: 500.0,
                tier: 0,
                line: Some("Refinery".to_string()),
                replaces: None,
                synergy: vec![],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        defs.insert(
            BuildingType::Factory,
            BuildingDefinition {
                id: "Factory".to_string(),
                display_name: "Factory".to_string(),
                description: "Builds stuff".to_string(),
                icon: "🏗".to_string(),
                category: "Industry".to_string(),
                build_points: 300.0,
                workforce: 12000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 2.0)],
                maintenance_resources: vec![
                    ("Iron".to_string(), 0.02),
                    ("Copper".to_string(), 0.01),
                    ("Water".to_string(), 0.1),
                    ("Polymers".to_string(), 0.005),
                    ("RareEarths".to_string(), 0.001),
                ],
                modifiers: vec![],
                power_demand_mw: 300.0,
                tier: 0,
                line: Some("Factory".to_string()),
                replaces: None,
                synergy: vec![],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        BuildingsData {
            definitions: defs,
            ..Default::default()
        }
    }

    /// End-to-end test: queue a HydroponicsFarm while a Farm exists in
    /// the colony.  With debug `instant_build` + `free_construction` the
    /// new building is added immediately and the Farm is replaced
    /// (GRA-22c plan §4.6 acceptance test #1).
    #[test]
    fn test_tier_replacement_hydroponics_farm_replaces_farm() {
        use crate::colony::components::PendingConstructionActions;
        use crate::colony::events::ConstructionEvent;
        use crate::colony::ConstructionDebugSettings;
        use crate::economy::logistics::PendingResourceRequests;

        let mut colony = Colony::new("Test".to_string(), 1_000.0);
        colony.add_building(BuildingType::Farm);
        colony.add_building(BuildingType::Farm);
        assert_eq!(colony.building_count(BuildingType::Farm), 2);

        let mut app = App::new();
        app.init_resource::<PendingConstructionActions>();
        app.init_resource::<PendingResourceRequests>();
        // GRA-358 PR-I: dirty-body tracker required by
        // process_construction_actions.
        app.init_resource::<crate::economy::DirtyBodies>();
        // process_construction_actions writes `ConstructionEvent`; the
        // message registry must be initialized for `MessageWriter` to
        // validate.
        app.add_message::<ConstructionEvent>();
        app.insert_resource(ConstructionDebugSettings {
            enabled: true,
            free_construction: true,
            instant_build: true,
            bypass_tech_requirements: false,
        });
        app.insert_resource(farm_line_buildings_data());
        app.init_resource::<crate::ui::SimulationTime>();
        // `free_construction` above makes the resource path a no-op so
        // the test does not need to seed a `LocalStockpile` or system.

        let colony_entity = app.world_mut().spawn(colony).id();

        // Queue a HydroponicsFarm
        app.world_mut()
            .resource_mut::<PendingConstructionActions>()
            .start_construction
            .push((colony_entity, BuildingType::AgriDome));

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(process_construction_actions);
        sched.run(app.world_mut());

        let colony = app
            .world()
            .entity(colony_entity)
            .get::<Colony>()
            .expect("colony still exists");

        // 2 Farms − 1 (replaced) = 1 Farm, 1 HydroponicsFarm added.
        assert_eq!(
            colony.building_count(BuildingType::Farm),
            1,
            "one Farm should have been replaced by the HydroponicsFarm"
        );
        assert_eq!(
            colony.building_count(BuildingType::AgriDome),
            1,
            "HydroponicsFarm should be present (instant build)"
        );
    }

    /// Counter-test: building a tier-N when the colony has *no*
    /// predecessor must NOT add a phantom decrement or error out.
    /// The HydroponicsFarm is still added, but no Farm is removed
    /// (there was none to remove).
    #[test]
    fn test_tier_replacement_no_predecessor_is_no_op() {
        use crate::colony::components::PendingConstructionActions;
        use crate::colony::events::ConstructionEvent;
        use crate::colony::ConstructionDebugSettings;
        use crate::economy::logistics::PendingResourceRequests;

        let colony = Colony::new("Moon".to_string(), 1_000.0);
        // No Farm present.

        let mut app = App::new();
        app.init_resource::<PendingConstructionActions>();
        app.init_resource::<PendingResourceRequests>();
        // GRA-358 PR-I: dirty-body tracker required by
        // process_construction_actions.
        app.init_resource::<crate::economy::DirtyBodies>();
        // process_construction_actions writes `ConstructionEvent`; the
        // message registry must be initialized for `MessageWriter` to
        // validate.
        app.add_message::<ConstructionEvent>();
        app.insert_resource(ConstructionDebugSettings {
            enabled: true,
            free_construction: true,
            instant_build: true,
            bypass_tech_requirements: false,
        });
        app.insert_resource(farm_line_buildings_data());
        app.init_resource::<crate::ui::SimulationTime>();

        let colony_entity = app.world_mut().spawn(colony).id();

        app.world_mut()
            .resource_mut::<PendingConstructionActions>()
            .start_construction
            .push((colony_entity, BuildingType::AgriDome));

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(process_construction_actions);
        sched.run(app.world_mut());

        let colony = app
            .world()
            .entity(colony_entity)
            .get::<Colony>()
            .expect("colony still exists");

        assert_eq!(colony.building_count(BuildingType::Farm), 0);
        assert_eq!(colony.building_count(BuildingType::AgriDome), 1);
    }

    /// Acceptance test #2: a Mine with 2 Refineries and 1 Factory in the
    /// same colony gets +10% mining and +5% construction (synergy
    /// recompute, GRA-22c plan §4.6).
    #[test]
    fn test_synergy_recompute_mine_with_refineries_and_factory() {
        let mut colony = Colony::new_civilisation("Earth".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::IronMine);
        colony.add_building(BuildingType::CopperMine);
        colony.add_building(BuildingType::CopperMine);
        colony.add_building(BuildingType::Factory);

        let mut app = App::new();
        app.init_resource::<ColonySynergies>();
        app.insert_resource(mine_synergy_buildings_data());
        app.world_mut().spawn(colony);

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(recompute_synergies);
        sched.run(app.world_mut());

        let synergies = app.world().resource::<ColonySynergies>();
        assert_eq!(synergies.by_colony.len(), 1, "one colony with synergies");
        let state = synergies
            .by_colony
            .values()
            .next()
            .expect("colony should have a synergy entry");

        let mining = state
            .bonuses
            .get("MiningEfficiency")
            .copied()
            .unwrap_or(0.0);
        let construction = state
            .bonuses
            .get("ConstructionSpeed")
            .copied()
            .unwrap_or(0.0);

        assert!(
            (mining - 0.10).abs() < 1e-9,
            "expected +10% mining (2 Refineries), got {}",
            mining
        );
        assert!(
            (construction - 0.05).abs() < 1e-9,
            "expected +5% construction (1 Factory), got {}",
            construction
        );
    }

    /// Counter-test: a Mine with 1 Refinery and 0 Factories must NOT
    /// activate either synergy (under threshold).
    #[test]
    fn test_synergy_recompute_under_threshold_inactive() {
        let mut colony = Colony::new_civilisation("Mars".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::IronMine);
        colony.add_building(BuildingType::CopperMine);
        // 1 Refinery: MiningEfficiency rule needs 2 → inactive
        // 0 Factories: ConstructionSpeed rule needs 1 → inactive

        let mut app = App::new();
        app.init_resource::<ColonySynergies>();
        app.insert_resource(mine_synergy_buildings_data());
        app.world_mut().spawn(colony);

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(recompute_synergies);
        sched.run(app.world_mut());

        let synergies = app.world().resource::<ColonySynergies>();
        // No synergies active → colony omitted from by_colony.
        assert!(
            synergies.by_colony.is_empty(),
            "no bonuses should activate; got {:?}",
            synergies.by_colony
        );
    }

    /// Counter-test: an empty BuildingsData (or one with no synergy
    /// rules) produces an empty `ColonySynergies`.  Guards against the
    /// "load before data" race.
    #[test]
    fn test_synergy_recompute_no_data_is_no_op() {
        let colony = Colony::new("Test".to_string(), 1_000.0);

        let mut app = App::new();
        app.init_resource::<ColonySynergies>();
        // Note: no BuildingsData resource inserted.
        app.world_mut().spawn(colony);

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(recompute_synergies);
        sched.run(app.world_mut());

        let synergies = app.world().resource::<ColonySynergies>();
        assert!(synergies.by_colony.is_empty());
    }

    // ── GRA-31 PR-A: local-only construction ────────────────────────────

    /// Build a `BuildingsData` with a single `Factory` that costs 100 Mt of
    /// Iron — the simplest cost profile that exercises the request path.
    fn iron_factory_buildings_data() -> BuildingsData {
        use crate::colony::data::AtmosphereKind;
        use crate::colony::data::BuildingDefinition;
        let mut defs: HashMap<BuildingType, BuildingDefinition> = HashMap::new();
        defs.insert(
            BuildingType::Factory,
            BuildingDefinition {
                id: "Factory".to_string(),
                display_name: "Factory".to_string(),
                description: "Test factory for local-only construction test".to_string(),
                icon: "🏭".to_string(),
                category: "Industry".to_string(),
                build_points: 600.0,
                workforce: 6000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 100.0)],
                maintenance_resources: vec![
                    ("Water".to_string(), 0.5),
                    ("Sulfur".to_string(), 0.008),
                ],
                modifiers: vec![],
                power_demand_mw: 500.0,
                tier: 0,
                line: None,
                replaces: None,
                synergy: vec![],
                available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                required_anomalies: vec![],
                allowed_body_types: vec![],
                replaces_in_line: None,
            },
        );
        BuildingsData {
            definitions: defs,
            ..Default::default()
        }
    }

    /// Regression test for GRA-31 PR-A: when a colony's local stockpile is
    /// short, construction must **not** drain resources from other bodies in
    /// the same system.  A `ResourceRequest` is created instead, with the
    /// full cost (the local shortfall in this case = the full cost).
    #[test]
    fn test_no_system_pool_fallback() {
        use crate::astronomy::components::SystemId;
        use crate::colony::events::ConstructionEvent;
        use crate::economy::logistics::PendingResourceRequests;

        let mut app = App::new();
        app.init_resource::<crate::ui::SimulationTime>();
        app.insert_resource(iron_factory_buildings_data());
        app.init_resource::<PendingResourceRequests>();
        app.init_resource::<PendingConstructionActions>();
        // GRA-358 PR-I: dirty-body tracker is required
        // by `process_construction_actions`.
        app.init_resource::<crate::economy::DirtyBodies>();
        // process_construction_actions writes `ConstructionEvent`; the
        // message registry must be initialized for `MessageWriter` to
        // validate.
        app.add_message::<ConstructionEvent>();
        // Disable debug-mode cost bypass so the cost path actually runs.
        app.insert_resource(ConstructionDebugSettings {
            enabled: false,
            free_construction: false,
            instant_build: false,
            bypass_tech_requirements: false,
        });

        // Two bodies in the same system (sys 0).  The colony body has an
        // empty local stockpile; the sister body has plenty of Iron.
        // Pre-PR-A, the `can_pay_system` branch would have drained 100 Mt
        // from the sister body.  Post-PR-A, the sister body must be
        // untouched and a `ResourceRequest` for 100 Mt Iron must be created.
        let sister_entity = app
            .world_mut()
            .spawn((
                SystemId(0),
                LocalStockpile::with_stockpiles([(ResourceType::Iron, 1_000.0)]),
            ))
            .id();

        let colony = Colony::new("TestColony".to_string(), 100.0);
        let colony_entity = app
            .world_mut()
            .spawn((
                colony,
                SystemId(0),
                LocalStockpile::new(), // empty
            ))
            .id();

        // Queue a Factory (costs 100 Mt Iron).  Colony's local is empty,
        // sister body has 1000 Mt Iron — old code would drain from sister.
        app.world_mut()
            .resource_mut::<PendingConstructionActions>()
            .start_construction
            .push((colony_entity, BuildingType::Factory));

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(process_construction_actions);
        sched.run(app.world_mut());

        // Sister body's Iron must be UNCHANGED.  This is the assertion the
        // PR-A change is meant to guarantee.
        let sister_stockpile = app
            .world()
            .entity(sister_entity)
            .get::<LocalStockpile>()
            .expect("sister body still has LocalStockpile");
        assert_eq!(
            sister_stockpile.get(&ResourceType::Iron),
            1_000.0,
            "sister body's Iron must not be drained by colony construction"
        );

        // Colony's local Iron must still be zero (we only requested
        // delivery, not deducted).
        let colony_stockpile = app
            .world()
            .entity(colony_entity)
            .get::<LocalStockpile>()
            .expect("colony still has LocalStockpile");
        assert_eq!(
            colony_stockpile.get(&ResourceType::Iron),
            0.0,
            "colony's local Iron must be unchanged (delivery is pending)"
        );

        // A `ResourceRequest` must exist for the colony with amount_mt == 100.
        // Copy the matching request into an owned value so we don't hold a
        // long-lived borrow of the world.
        let (req_amount, req_priority, req_id) = {
            let requests = app.world().resource::<PendingResourceRequests>();
            let iron_requests: Vec<&ResourceRequest> = requests
                .requests
                .iter()
                .filter(|r| r.destination_body == colony_entity && r.resource == ResourceType::Iron)
                .collect();
            assert_eq!(
                iron_requests.len(),
                1,
                "expected exactly one Iron request for the colony, got {}",
                iron_requests.len()
            );
            let req = iron_requests[0];
            (req.amount_mt, req.priority, req.id)
        };
        assert!(
            (req_amount - 100.0).abs() < 1e-9,
            "request amount must be 100 Mt (full cost), got {}",
            req_amount
        );
        assert_eq!(
            req_priority,
            RequestPriority::Construction,
            "construction-cost requests must be Construction priority"
        );

        // A blocking `ConstructionProject` should be spawned, with
        // `awaiting_resources = true` because the Iron is not yet on hand.
        let projects: Vec<(Entity, bool, Option<u64>)> = {
            let world = app.world_mut();
            world
                .query::<&ConstructionProject>()
                .iter(world)
                .map(|p| (p.colony_entity, p.awaiting_resources, p.blocking_request_id))
                .collect()
        };
        let mut found_blocked_project = false;
        for (colony_e, awaiting, blocking_id) in projects {
            if colony_e == colony_entity {
                found_blocked_project = true;
                assert!(awaiting, "project must be marked awaiting_resources");
                assert_eq!(
                    blocking_id,
                    Some(req_id),
                    "project must record the blocking request id"
                );
            }
        }
        assert!(
            found_blocked_project,
            "expected a ConstructionProject to be spawned for the colony"
        );
    }

    /// Counter-test: when the colony's own `LocalStockpile` does have
    /// enough Iron, construction must deduct locally and **not** create a
    /// `ResourceRequest`.  This guards the other branch of the same
    /// system from regressing.
    #[test]
    fn test_local_only_deducts_from_colony_when_affordable() {
        use crate::astronomy::components::SystemId;
        use crate::colony::events::ConstructionEvent;
        use crate::economy::logistics::PendingResourceRequests;

        let mut app = App::new();
        app.init_resource::<crate::ui::SimulationTime>();
        app.insert_resource(iron_factory_buildings_data());
        app.init_resource::<PendingResourceRequests>();
        app.init_resource::<PendingConstructionActions>();
        // GRA-358 PR-I: dirty-body tracker is required
        // by `process_construction_actions`'s system
        // signature (mutates LocalStockpile on
        // construction). Tests mirror the production
        // plugin's init.
        app.init_resource::<crate::economy::DirtyBodies>();
        // process_construction_actions writes `ConstructionEvent`; the
        // message registry must be initialized for `MessageWriter` to
        // validate.
        app.add_message::<ConstructionEvent>();
        app.insert_resource(ConstructionDebugSettings {
            enabled: false,
            free_construction: false,
            instant_build: false,
            bypass_tech_requirements: false,
        });

        // Sister body in the same system, with 1000 Mt Iron — to verify it
        // is not touched (control case: neither branch should drain it).
        let sister_entity = app
            .world_mut()
            .spawn((
                SystemId(0),
                LocalStockpile::with_stockpiles([(ResourceType::Iron, 1_000.0)]),
            ))
            .id();

        // Colony with enough Iron locally.
        let colony = Colony::new("TestColony".to_string(), 100.0);
        let colony_entity = app
            .world_mut()
            .spawn((
                colony,
                SystemId(0),
                LocalStockpile::with_stockpiles([(ResourceType::Iron, 200.0)]),
            ))
            .id();

        app.world_mut()
            .resource_mut::<PendingConstructionActions>()
            .start_construction
            .push((colony_entity, BuildingType::Factory));

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(process_construction_actions);
        sched.run(app.world_mut());

        // Sister body untouched.
        let sister = app
            .world()
            .entity(sister_entity)
            .get::<LocalStockpile>()
            .unwrap();
        assert_eq!(sister.get(&ResourceType::Iron), 1_000.0);

        // Colony's Iron must be deducted by exactly 100 Mt.
        let colony_stockpile = app
            .world()
            .entity(colony_entity)
            .get::<LocalStockpile>()
            .unwrap();
        assert!(
            (colony_stockpile.get(&ResourceType::Iron) - 100.0).abs() < 1e-9,
            "colony Iron should drop from 200 to 100, got {}",
            colony_stockpile.get(&ResourceType::Iron)
        );

        // No requests should have been created.
        let requests = app.world().resource::<PendingResourceRequests>();
        assert!(
            requests.requests.is_empty(),
            "no ResourceRequest should be created when the colony can pay locally"
        );
    }
}
