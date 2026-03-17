use crate::colony::{BuildingsData, Colony};
use crate::economy::budget::{
    GlobalBudget, ResourceRateTracker, SECONDS_PER_MONTH, SECONDS_PER_YEAR,
};
use crate::economy::components::{LocalStockpile, PlanetResources};
use crate::economy::types::ResourceType;
use crate::plugins::solar_system::CelestialBody;
use crate::research::ResearchState;
use crate::ui::SimulationTime;
use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct MiningOperation {
    pub resource_type: ResourceType,
    /// Base extraction rate in Megatons per year
    pub base_rate_mt_per_year: f64,
    pub active: bool,
}

impl Default for MiningOperation {
    fn default() -> Self {
        Self {
            resource_type: ResourceType::Iron,
            base_rate_mt_per_year: 1.0,
            active: true,
        }
    }
}

struct IndustrialProcessRule {
    output: ResourceType,
    required_tech: Option<&'static str>,
    inputs_per_output: &'static [(ResourceType, f64)],
}

fn industrial_process_rule(modifier_type: &str) -> Option<IndustrialProcessRule> {
    match modifier_type {
        "HydrogenSynthesis" => Some(IndustrialProcessRule {
            output: ResourceType::Hydrogen,
            required_tech: None,
            inputs_per_output: &[(ResourceType::Methane, 2.0)],
        }),
        "AmmoniaSynthesis" => Some(IndustrialProcessRule {
            output: ResourceType::Ammonia,
            required_tech: None,
            inputs_per_output: &[(ResourceType::Nitrogen, 0.82), (ResourceType::Methane, 0.71)],
        }),
        "PolymerSynthesis" => Some(IndustrialProcessRule {
            output: ResourceType::Polymers,
            required_tech: None,
            inputs_per_output: &[(ResourceType::Methane, 1.15)],
        }),
        "TritiumBreeding" => Some(IndustrialProcessRule {
            output: ResourceType::Tritium,
            required_tech: Some("fusion_power"),
            inputs_per_output: &[(ResourceType::Lithium, 0.6)],
        }),
        "PlutoniumBreeding" => Some(IndustrialProcessRule {
            output: ResourceType::Plutonium,
            required_tech: Some("breeder_reactors"),
            inputs_per_output: &[(ResourceType::Uranium, 1.2)],
        }),
        _ => None,
    }
}

fn process_is_unlocked(rule: &IndustrialProcessRule, research_state: Option<&ResearchState>) -> bool {
    match rule.required_tech {
        Some(tech_id) => research_state.is_some_and(|state| state.is_unlocked(tech_id)),
        None => true,
    }
}

fn combined_available(
    local_opt: &Option<Mut<LocalStockpile>>,
    budget: &GlobalBudget,
    resource: ResourceType,
) -> f64 {
    local_opt
        .as_ref()
        .map_or(0.0, |local| local.get(&resource))
        + budget.get_stockpile(&resource)
}

fn consume_with_fallback(
    local_opt: &mut Option<Mut<LocalStockpile>>,
    budget: &mut GlobalBudget,
    resource: ResourceType,
    amount: f64,
) -> f64 {
    let mut remaining = amount.max(0.0);
    if remaining <= 0.0 {
        return 0.0;
    }

    if let Some(local) = local_opt.as_deref_mut() {
        let consumed_local = local.consume(resource, remaining);
        remaining -= consumed_local;
    }

    if remaining > 0.0 {
        let available_global = budget.get_stockpile(&resource);
        let consumed_global = remaining.min(available_global);
        if consumed_global > 0.0 {
            budget.consume_resource(resource, consumed_global);
            remaining -= consumed_global;
        }
    }

    amount.max(0.0) - remaining
}

fn deposit_with_fallback(
    local_opt: &mut Option<Mut<LocalStockpile>>,
    budget: &mut GlobalBudget,
    resource: ResourceType,
    amount: f64,
) {
    let amount = amount.max(0.0);
    if amount <= 0.0 {
        return;
    }

    if let Some(local) = local_opt.as_deref_mut() {
        let cap = budget.effective_stockpile_cap(resource);
        local.add_capped(resource, amount, cap);
    } else {
        budget.add_resource_capped(resource, amount);
    }
}

fn feasible_output_amount(
    desired_output: f64,
    rule: &IndustrialProcessRule,
    local_opt: &Option<Mut<LocalStockpile>>,
    budget: &GlobalBudget,
) -> f64 {
    if desired_output <= 0.0 {
        return 0.0;
    }

    let mut scale = 1.0_f64;
    for (input_resource, input_per_output) in rule.inputs_per_output {
        let required_input = desired_output * *input_per_output;
        if required_input <= 0.0 {
            continue;
        }
        let available_input = combined_available(local_opt, budget, *input_resource);
        scale = scale.min((available_input / required_input).clamp(0.0, 1.0));
    }

    desired_output * scale
}

pub fn extract_resources(
    mut budget: ResMut<GlobalBudget>,
    mut all_query: Query<(
        &mut PlanetResources,
        &mut CelestialBody,
        Option<&MiningOperation>,
        Option<&Colony>,
        Option<&mut LocalStockpile>,
    )>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    buildings_data: Option<Res<BuildingsData>>,
    research_state: Option<Res<ResearchState>>,
) {
    let current_elapsed = sim_time.elapsed_seconds();
    let dt = current_elapsed - *last_elapsed;
    *last_elapsed = current_elapsed;

    if dt <= 0.0 {
        return;
    }

    // 1 year = 365.25 days * 24 * 60 * 60
    let years_elapsed = dt / 31_557_600.0;

    if years_elapsed <= 0.0 {
        return;
    }

    // Helper: deposit extracted amount into the body's LocalStockpile if present,
    // otherwise fall back to the global budget.
    // Closure captures `budget` by mut-ref which is not possible with an iter_mut
    // borrow also active; instead we collect extractions and apply them afterwards.
    // We handle this via a simple inline deposit in the loop with an explicit split.

    for (mut resources, mut body, op_opt, colony_opt, mut local_opt) in all_query.iter_mut() {
        /// Deposit helper: goes to LocalStockpile when present, GlobalBudget otherwise.
        macro_rules! deposit {
            ($rt:expr, $amount:expr) => {
                if $amount > 0.0 {
                    if let Some(ref mut ls) = local_opt {
                        let cap = budget.effective_stockpile_cap($rt);
                        ls.add_capped($rt, $amount, cap);
                    } else {
                        budget.add_resource_capped($rt, $amount);
                    }
                }
            };
        }

        // 1. Process specific MiningOperations (legacy/scenario)
        if let Some(op) = op_opt {
            if op.active {
                let mut total_extracted = 0.0;

                if let Some(deposit) = resources.deposits.get_mut(&op.resource_type) {
                    let mut demand = op.base_rate_mt_per_year * years_elapsed;

                    // 1. Proven Crustal (Cheapest)
                    let taking_proven = demand.min(deposit.reserve.proven_crustal);
                    deposit.reserve.proven_crustal -= taking_proven;
                    total_extracted += taking_proven;
                    demand -= taking_proven;

                    // 2. Deep Deposits (Expensive)
                    if demand > 0.0 {
                        let taking_deep = demand.min(deposit.reserve.deep_deposits);
                        deposit.reserve.deep_deposits -= taking_deep;
                        total_extracted += taking_deep;
                        demand -= taking_deep;
                    }

                    // 3. Planetary Bulk (Exorbitant)
                    if demand > 0.0 {
                        let taking_bulk = demand.min(deposit.reserve.planetary_bulk);
                        deposit.reserve.planetary_bulk -= taking_bulk;
                        total_extracted += taking_bulk;
                    }

                    if total_extracted > 0.0 {
                        deposit!(op.resource_type, total_extracted);
                        // Reduce body mass (1 Mt = 1e9 kg)
                        body.mass -= total_extracted * 1e9;
                    }
                }
            }
        }

        // 2. Process Colony Mining & Atmospheric Harvesting
        if let Some(colony) = colony_opt {
            if let Some(data) = &buildings_data {
                // Three independent extraction tiers — no overflow between them.
                // MiningEfficiency    → Proven Crustal  (Mine, Refinery, etc.)
                // DeepMiningEfficiency→ Deep Deposits   (DeepDrill, LaserDrill)
                // BulkMiningEfficiency→ Planetary Bulk  (StripMine, BulkExcavator)
                let mut surface_rate = 0.0_f64;
                let mut deep_rate = 0.0_f64;
                let mut bulk_rate = 0.0_f64;
                // Calculate total atmospheric harvesting capacity (Mt/year)
                let mut total_atmo_rate = 0.0;

                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            match modifier.modifier_type.as_str() {
                                "MiningEfficiency" => {
                                    surface_rate += modifier.value * count as f64;
                                }
                                "DeepMiningEfficiency" => {
                                    deep_rate += modifier.value * count as f64;
                                }
                                "BulkMiningEfficiency" => {
                                    bulk_rate += modifier.value * count as f64;
                                }
                                "AtmosphericHarvesting" => {
                                    total_atmo_rate += modifier.value * count as f64;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Helper: extract from a single tier across all eligible deposits,
                // weighted by concentration. Returns nothing — mutates resources & budget in place.
                // We call this three times, once per tier.

                // --- Tier 1: Proven Crustal ---
                if surface_rate > 0.0 {
                    let eligible: Vec<(ResourceType, f32)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !eligible.is_empty() {
                        let total_weight: f64 =
                            eligible.iter().map(|(_, c)| (*c as f64).max(1e-10)).sum();
                        for (r_type, concentration) in &eligible {
                            let share = (*concentration as f64).max(1e-10) / total_weight;
                            let demand = surface_rate * share * years_elapsed;
                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let taking = demand.min(deposit.reserve.proven_crustal);
                                deposit.reserve.proven_crustal -= taking;
                                if taking > 0.0 {
                                    deposit!(*r_type, taking);
                                    body.mass -= taking * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Tier 2: Deep Deposits ---
                if deep_rate > 0.0 {
                    let eligible: Vec<(ResourceType, f32)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.deep_deposits > 0.001)
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !eligible.is_empty() {
                        let total_weight: f64 =
                            eligible.iter().map(|(_, c)| (*c as f64).max(1e-10)).sum();
                        for (r_type, concentration) in &eligible {
                            let share = (*concentration as f64).max(1e-10) / total_weight;
                            let demand = deep_rate * share * years_elapsed;
                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let taking = demand.min(deposit.reserve.deep_deposits);
                                deposit.reserve.deep_deposits -= taking;
                                if taking > 0.0 {
                                    deposit!(*r_type, taking);
                                    body.mass -= taking * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Tier 3: Planetary Bulk ---
                if bulk_rate > 0.0 {
                    let eligible: Vec<(ResourceType, f32)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.planetary_bulk > 0.001)
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !eligible.is_empty() {
                        let total_weight: f64 =
                            eligible.iter().map(|(_, c)| (*c as f64).max(1e-10)).sum();
                        for (r_type, concentration) in &eligible {
                            let share = (*concentration as f64).max(1e-10) / total_weight;
                            let demand = bulk_rate * share * years_elapsed;
                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let taking = demand.min(deposit.reserve.planetary_bulk);
                                deposit.reserve.planetary_bulk -= taking;
                                if taking > 0.0 {
                                    deposit!(*r_type, taking);
                                    body.mass -= taking * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Atmospheric gas harvesting (AtmosphericProcessor) ---
                if total_atmo_rate > 0.0 {
                    let harvestable: Vec<(ResourceType, f32)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| {
                            d.is_atmospheric
                                && (d.reserve.proven_crustal > 0.001
                                    || d.reserve.deep_deposits > 0.001)
                        })
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !harvestable.is_empty() {
                        let total_weight: f64 = harvestable
                            .iter()
                            .map(|(_, c)| (*c as f64).max(1e-10))
                            .sum();

                        for (r_type, concentration) in &harvestable {
                            let weight = (*concentration as f64).max(1e-10);
                            let share = weight / total_weight;
                            let effective_rate = total_atmo_rate * share;

                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let mut demand = effective_rate * years_elapsed;
                                let mut extracted = 0.0;

                                // Atmospheric (proven tier)
                                let taking = demand.min(deposit.reserve.proven_crustal);
                                deposit.reserve.proven_crustal -= taking;
                                extracted += taking;
                                demand -= taking;

                                // Trapped/Dissolved (deep tier)
                                if demand > 0.0 {
                                    let taking_deep = demand.min(deposit.reserve.deep_deposits);
                                    deposit.reserve.deep_deposits -= taking_deep;
                                    extracted += taking_deep;
                                }

                                if extracted > 0.0 {
                                    deposit!(*r_type, extracted);
                                    body.mass -= extracted * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Industrial synthesis / breeding ---
                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    let Some(def) = data.get(building_type) else {
                        continue;
                    };

                    for modifier in &def.modifiers {
                        let Some(rule) = industrial_process_rule(&modifier.modifier_type) else {
                            continue;
                        };
                        if !process_is_unlocked(&rule, research_state.as_deref()) {
                            continue;
                        }

                        let desired_output = modifier.value * count as f64 * years_elapsed;
                        let actual_output =
                            feasible_output_amount(desired_output, &rule, &local_opt, &budget);

                        if actual_output <= 0.0 {
                            continue;
                        }

                        for (input_resource, input_per_output) in rule.inputs_per_output {
                            let input_amount = actual_output * *input_per_output;
                            consume_with_fallback(
                                &mut local_opt,
                                &mut budget,
                                *input_resource,
                                input_amount,
                            );
                        }

                        deposit_with_fallback(
                            &mut local_opt,
                            &mut budget,
                            rule.output,
                            actual_output,
                        );
                    }
                }
            }
        }
    }
}

/// System that computes **net** monthly rates for all resources and
/// research/engineering points, writing them into [`ResourceRateTracker`].
///
/// Production comes from `MiningOperation` components, colony mining
/// buildings, and colony food production. Consumption comes from building
/// maintenance costs plus colony food consumption. The displayed rate is
/// production − consumption so the UI shows the true net balance.
///
/// RP/EP rates include the base generation rates defined in
/// `research::systems` (`BASE_RP_PER_YEAR`, `BASE_EP_PER_YEAR`) so the
/// bar always reflects actual accumulation.
pub fn update_resource_rates(
    mut tracker: ResMut<ResourceRateTracker>,
    mining_ops: Query<(Entity, &MiningOperation, Option<&PlanetResources>)>,
    research_buildings: Query<&crate::research::components::ResearchBuilding>,
    engineering_facilities: Query<&crate::research::components::EngineeringFacility>,
    colony_query: Query<(Entity, &Colony, Option<&PlanetResources>, Option<&LocalStockpile>)>,
    buildings_data: Option<Res<BuildingsData>>,
    budget: Res<GlobalBudget>,
    research_state: Res<crate::research::ResearchState>,
) {
    // --- Resource rates from mining (production) ---
    let mut rates = std::collections::HashMap::new();
    let mut per_entity: std::collections::HashMap<
        Entity,
        std::collections::HashMap<ResourceType, f64>,
    > = std::collections::HashMap::new();

    // 1. MiningOperation components
    for (entity, op, resources_opt) in mining_ops.iter() {
        if !op.active {
            continue;
        }
        // Skip if the targeted deposit is fully depleted
        let depleted = resources_opt.is_some_and(|res| {
            res.deposits.get(&op.resource_type).is_none_or(|d| {
                d.reserve.proven_crustal < 0.001
                    && d.reserve.deep_deposits < 0.001
                    && d.reserve.planetary_bulk < 0.001
            })
        });
        if depleted {
            continue;
        }
        // base_rate_mt_per_year → per month = rate * (month / year)
        let monthly = op.base_rate_mt_per_year * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
        *rates.entry(op.resource_type).or_insert(0.0) += monthly;
        *per_entity
            .entry(entity)
            .or_default()
            .entry(op.resource_type)
            .or_insert(0.0) += monthly;
    }

    // Helper macro-like closure to add to both global and per-entity rates
    let add_rate = |rates: &mut std::collections::HashMap<ResourceType, f64>,
                    per_entity: &mut std::collections::HashMap<
        Entity,
        std::collections::HashMap<ResourceType, f64>,
    >,
                    entity: Entity,
                    r_type: ResourceType,
                    amount: f64| {
        *rates.entry(r_type).or_insert(0.0) += amount;
        *per_entity
            .entry(entity)
            .or_default()
            .entry(r_type)
            .or_insert(0.0) += amount;
    };

    // 2. Colony mining & atmospheric harvesting
    if let Some(data) = &buildings_data {
        for (entity, colony, resources_opt, local_opt) in colony_query.iter() {
            if let Some(resources) = resources_opt {
                let mut surface_rate = 0.0_f64;
                let mut deep_rate = 0.0_f64;
                let mut bulk_rate = 0.0_f64;
                let mut total_atmo_rate = 0.0_f64;

                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            match modifier.modifier_type.as_str() {
                                "MiningEfficiency" => {
                                    surface_rate += modifier.value * count as f64;
                                }
                                "DeepMiningEfficiency" => {
                                    deep_rate += modifier.value * count as f64;
                                }
                                "BulkMiningEfficiency" => {
                                    bulk_rate += modifier.value * count as f64;
                                }
                                "AtmosphericHarvesting" => {
                                    total_atmo_rate += modifier.value * count as f64;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Solid mining rates (weighted by concentration) — one pool per tier
                // Tier 1: Surface / Proven Crustal (MiningEfficiency buildings)
                if surface_rate > 0.0 {
                    let monthly_surface = surface_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let eligible: Vec<(ResourceType, f64)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &eligible {
                            let share = weight / total_weight;
                            add_rate(
                                &mut rates,
                                &mut per_entity,
                                entity,
                                *r_type,
                                monthly_surface * share,
                            );
                        }
                    }
                }

                // Tier 2: Deep Deposits (DeepMiningEfficiency buildings)
                if deep_rate > 0.0 {
                    let monthly_deep = deep_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let eligible: Vec<(ResourceType, f64)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.deep_deposits > 0.001)
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &eligible {
                            let share = weight / total_weight;
                            add_rate(
                                &mut rates,
                                &mut per_entity,
                                entity,
                                *r_type,
                                monthly_deep * share,
                            );
                        }
                    }
                }

                // Tier 3: Planetary Bulk (BulkMiningEfficiency buildings)
                if bulk_rate > 0.0 {
                    let monthly_bulk = bulk_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let eligible: Vec<(ResourceType, f64)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.planetary_bulk > 0.001)
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &eligible {
                            let share = weight / total_weight;
                            add_rate(
                                &mut rates,
                                &mut per_entity,
                                entity,
                                *r_type,
                                monthly_bulk * share,
                            );
                        }
                    }
                }

                // Atmospheric harvesting rates (weighted by concentration)
                if total_atmo_rate > 0.0 {
                    let monthly_total = total_atmo_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let harvestable: Vec<(ResourceType, f64)> = resources
                        .deposits
                        .iter()
                        .filter(|(_, d)| {
                            d.is_atmospheric
                                && (d.reserve.proven_crustal > 0.001
                                    || d.reserve.deep_deposits > 0.001)
                        })
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = harvestable.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &harvestable {
                            let share = weight / total_weight;
                            add_rate(
                                &mut rates,
                                &mut per_entity,
                                entity,
                                *r_type,
                                monthly_total * share,
                            );
                        }
                    }
                }

                let mut simulated_available: std::collections::HashMap<ResourceType, f64> =
                    ResourceType::all()
                        .iter()
                        .copied()
                        .map(|resource| {
                            (
                                resource,
                                local_opt.map_or(0.0, |local| local.get(&resource))
                                    + budget.get_stockpile(&resource),
                            )
                        })
                        .collect();

                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    let Some(def) = data.get(building_type) else {
                        continue;
                    };

                    for modifier in &def.modifiers {
                        let Some(rule) = industrial_process_rule(&modifier.modifier_type) else {
                            continue;
                        };
                        if !process_is_unlocked(&rule, Some(&research_state)) {
                            continue;
                        }

                        let desired_monthly_output =
                            modifier.value * count as f64 * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                        if desired_monthly_output <= 0.0 {
                            continue;
                        }

                        let mut scale = 1.0_f64;
                        for (input_resource, input_per_output) in rule.inputs_per_output {
                            let required = desired_monthly_output * *input_per_output;
                            if required <= 0.0 {
                                continue;
                            }
                            let available =
                                simulated_available.get(input_resource).copied().unwrap_or(0.0);
                            scale = scale.min((available / required).clamp(0.0, 1.0));
                        }

                        let actual_output = desired_monthly_output * scale;
                        if actual_output <= 0.0 {
                            continue;
                        }

                        for (input_resource, input_per_output) in rule.inputs_per_output {
                            let consumed = actual_output * *input_per_output;
                            *simulated_available.entry(*input_resource).or_insert(0.0) -= consumed;
                            add_rate(
                                &mut rates,
                                &mut per_entity,
                                entity,
                                *input_resource,
                                -consumed,
                            );
                        }

                        *simulated_available.entry(rule.output).or_insert(0.0) += actual_output;
                        add_rate(
                            &mut rates,
                            &mut per_entity,
                            entity,
                            rule.output,
                            actual_output,
                        );
                    }
                }
            } else {
                warn!("Colony {} has no PlanetResources!", colony.name);
            }
        }
    } else {
        warn!("BuildingsData missing in update_resource_rates");
    }

    // 3. Add net colony food rate (production - population consumption)
    for (entity, colony, _, _) in colony_query.iter() {
        let food_net_per_year =
            colony.food_production_per_year() - colony.food_consumption_per_year();
        let food_net_per_month = food_net_per_year * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
        if food_net_per_month.abs() > f64::EPSILON {
            *rates.entry(ResourceType::Food).or_insert(0.0) += food_net_per_month;
            *per_entity
                .entry(entity)
                .or_default()
                .entry(ResourceType::Food)
                .or_insert(0.0) += food_net_per_month;
        }
    }

    // 4. Subtract maintenance consumption so rates show NET balance
    if let Some(data) = &buildings_data {
        for (entity, colony, _, _) in colony_query.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }
                let maintenance = data.maintenance_resources(building_type);
                for (resource_name, annual_amount) in maintenance {
                    if let Some(rt) = crate::colony::data::parse_resource_type(resource_name) {
                        // annual → monthly
                        let monthly_cost =
                            annual_amount * (count as f64) * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                        *rates.entry(rt).or_insert(0.0) -= monthly_cost;
                        *per_entity
                            .entry(entity)
                            .or_default()
                            .entry(rt)
                            .or_insert(0.0) -= monthly_cost;
                    }
                }
            }
        }
    }

    tracker.resource_rates = rates;
    tracker.per_entity_rates = per_entity;

    // --- Research point rate (include base rate) ---
    // Base RP per month (same constant used in research::systems)
    const BASE_RP_PER_YEAR: f64 = 2000.0;
    let base_rp_monthly = BASE_RP_PER_YEAR * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

    // From ResearchBuilding components (per second → per month)
    let research_per_second: f64 = research_buildings.iter().map(|b| b.points_per_second).sum();
    let research_multiplier = research_state.research_speed_multiplier();
    let mut total_research_monthly = base_rp_monthly + research_per_second * SECONDS_PER_MONTH;

    // From colony buildings
    if let Some(data) = &buildings_data {
        for (_entity, colony, _, _) in colony_query.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }
                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "ResearchSpeed" {
                            total_research_monthly += modifier.value * count as f64;
                        }
                    }
                }
            }
        }
    }

    tracker.research_rate_per_month = total_research_monthly * research_multiplier;

    // --- Engineering point rate (include base rate) ---
    const BASE_EP_PER_YEAR: f64 = 1000.0;
    let base_ep_monthly = BASE_EP_PER_YEAR * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

    // From EngineeringFacility components
    let engineering_per_second: f64 = engineering_facilities
        .iter()
        .map(|f| f.points_per_second)
        .sum();
    let engineering_multiplier = research_state.engineering_speed_multiplier();
    let mut total_engineering_monthly =
        base_ep_monthly + engineering_per_second * SECONDS_PER_MONTH;

    // From colony buildings
    if let Some(data) = &buildings_data {
        for (_entity, colony, _, _) in colony_query.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }
                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "EngineeringSpeed" {
                            total_engineering_monthly += modifier.value * count as f64;
                        }
                    }
                }
            }
        }
    }

    tracker.engineering_rate_per_month = total_engineering_monthly * engineering_multiplier;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::components::{MineralDeposit, PlanetResources};
    use crate::economy::types::ResourceType;

    /// Helper: create a deposit with specific proven/deep/bulk, concentration, and atmospheric flag
    fn make_deposit(
        proven: f64,
        deep: f64,
        bulk: f64,
        concentration: f32,
        atmo: bool,
    ) -> MineralDeposit {
        let mut d = MineralDeposit::new(proven, deep, bulk, concentration, 0.8);
        d.is_atmospheric = atmo;
        d
    }

    #[test]
    fn test_mines_only_extract_non_atmospheric() {
        // Iron (solid) and O2 (atmospheric) both present
        let mut resources = PlanetResources::new();
        resources.add_deposit(
            ResourceType::Iron,
            make_deposit(1000.0, 500.0, 0.0, 0.5, false),
        );
        resources.add_deposit(
            ResourceType::Oxygen,
            make_deposit(2000.0, 100.0, 0.0, 0.9, true),
        );

        // Simulate what mining does: only mine non-atmospheric
        let minable: Vec<ResourceType> = resources
            .deposits
            .iter()
            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, _)| *t)
            .collect();

        assert!(
            minable.contains(&ResourceType::Iron),
            "Iron should be minable"
        );
        assert!(
            !minable.contains(&ResourceType::Oxygen),
            "Atmospheric O2 should NOT be minable"
        );
    }

    #[test]
    fn test_atmo_processor_only_extracts_atmospheric() {
        let mut resources = PlanetResources::new();
        resources.add_deposit(
            ResourceType::Iron,
            make_deposit(1000.0, 500.0, 0.0, 0.5, false),
        );
        resources.add_deposit(
            ResourceType::Nitrogen,
            make_deposit(5000.0, 200.0, 0.0, 0.7, true),
        );
        resources.add_deposit(
            ResourceType::Oxygen,
            make_deposit(2000.0, 100.0, 0.0, 0.9, true),
        );

        let harvestable: Vec<ResourceType> = resources
            .deposits
            .iter()
            .filter(|(_, d)| d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, _)| *t)
            .collect();

        assert!(
            !harvestable.contains(&ResourceType::Iron),
            "Iron should NOT be harvestable"
        );
        assert!(
            harvestable.contains(&ResourceType::Nitrogen),
            "N2 should be harvestable"
        );
        assert!(
            harvestable.contains(&ResourceType::Oxygen),
            "O2 should be harvestable"
        );
    }

    #[test]
    fn test_concentration_weights_mining_distribution() {
        let mut resources = PlanetResources::new();
        // Iron: 50% concentration, Titanium: 10% concentration
        resources.add_deposit(
            ResourceType::Iron,
            make_deposit(1000.0, 0.0, 0.0, 0.5, false),
        );
        resources.add_deposit(
            ResourceType::Titanium,
            make_deposit(1000.0, 0.0, 0.0, 0.1, false),
        );

        let minable: Vec<(ResourceType, f64)> = resources
            .deposits
            .iter()
            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
            .collect();

        let total_weight: f64 = minable.iter().map(|(_, w)| w).sum();
        assert!(
            (total_weight - 0.6).abs() < 0.01,
            "Total weight should be 0.6"
        );

        for (r_type, weight) in &minable {
            let share = weight / total_weight;
            match r_type {
                ResourceType::Iron => {
                    // Iron gets 0.5/0.6 ≈ 83% of mining effort
                    assert!(
                        share > 0.8 && share < 0.9,
                        "Iron (50% conc.) should get ~83% share, got {:.1}%",
                        share * 100.0
                    );
                }
                ResourceType::Titanium => {
                    // Titanium gets 0.1/0.6 ≈ 17%
                    assert!(
                        share > 0.15 && share < 0.2,
                        "Titanium (10% conc.) should get ~17% share, got {:.1}%",
                        share * 100.0
                    );
                }
                _ => panic!("Unexpected resource type"),
            }
        }
    }

    #[test]
    fn test_trace_deposits_not_extracted() {
        let mut resources = PlanetResources::new();
        // Sub-kiloton deposit should be filtered out
        resources.add_deposit(
            ResourceType::Gold,
            make_deposit(0.0005, 0.0, 0.0, 0.01, false),
        );

        let minable: Vec<ResourceType> = resources
            .deposits
            .iter()
            .filter(|(_, d)| {
                !d.is_atmospheric
                    && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001)
            })
            .map(|(t, _)| *t)
            .collect();

        assert!(minable.is_empty(), "Sub-kiloton Gold should not be minable");
    }
}
