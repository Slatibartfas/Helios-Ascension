use crate::economy::budget::{GlobalBudget, ResourceRateTracker, SECONDS_PER_MONTH, SECONDS_PER_YEAR};
use crate::economy::components::PlanetResources;
use crate::economy::types::ResourceType;
use crate::plugins::solar_system::CelestialBody;
use crate::ui::SimulationTime;
use crate::colony::{Colony, BuildingsData};
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

pub fn extract_resources(
    mut budget: ResMut<GlobalBudget>,
    mut all_query: Query<(
        &mut PlanetResources,
        &mut CelestialBody,
        Option<&MiningOperation>,
        Option<&Colony>,
    )>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    buildings_data: Option<Res<BuildingsData>>,
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

    for (mut resources, mut body, op_opt, colony_opt) in all_query.iter_mut() {
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

                    // Add to global budget
                    if total_extracted > 0.0 {
                        budget.add_resource(op.resource_type, total_extracted);
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
                let mut deep_rate    = 0.0_f64;
                let mut bulk_rate    = 0.0_f64;
                // Calculate total atmospheric harvesting capacity (Mt/year)
                let mut total_atmo_rate = 0.0;

                for (building_type, &count) in &colony.buildings {
                    if count == 0 { continue; }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            match modifier.modifier_type.as_str() {
                                "MiningEfficiency"     => { surface_rate  += modifier.value * count as f64; }
                                "DeepMiningEfficiency" => { deep_rate     += modifier.value * count as f64; }
                                "BulkMiningEfficiency" => { bulk_rate     += modifier.value * count as f64; }
                                "AtmosphericHarvesting"=> { total_atmo_rate += modifier.value * count as f64; }
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
                    let eligible: Vec<(ResourceType, f32)> = resources.deposits.iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !eligible.is_empty() {
                        let total_weight: f64 = eligible.iter().map(|(_, c)| (*c as f64).max(1e-10)).sum();
                        for (r_type, concentration) in &eligible {
                            let share = (*concentration as f64).max(1e-10) / total_weight;
                            let demand = surface_rate * share * years_elapsed;
                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let taking = demand.min(deposit.reserve.proven_crustal);
                                deposit.reserve.proven_crustal -= taking;
                                if taking > 0.0 {
                                    budget.add_resource(*r_type, taking);
                                    body.mass -= taking * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Tier 2: Deep Deposits ---
                if deep_rate > 0.0 {
                    let eligible: Vec<(ResourceType, f32)> = resources.deposits.iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.deep_deposits > 0.001)
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !eligible.is_empty() {
                        let total_weight: f64 = eligible.iter().map(|(_, c)| (*c as f64).max(1e-10)).sum();
                        for (r_type, concentration) in &eligible {
                            let share = (*concentration as f64).max(1e-10) / total_weight;
                            let demand = deep_rate * share * years_elapsed;
                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let taking = demand.min(deposit.reserve.deep_deposits);
                                deposit.reserve.deep_deposits -= taking;
                                if taking > 0.0 {
                                    budget.add_resource(*r_type, taking);
                                    body.mass -= taking * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Tier 3: Planetary Bulk ---
                if bulk_rate > 0.0 {
                    let eligible: Vec<(ResourceType, f32)> = resources.deposits.iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.planetary_bulk > 0.001)
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !eligible.is_empty() {
                        let total_weight: f64 = eligible.iter().map(|(_, c)| (*c as f64).max(1e-10)).sum();
                        for (r_type, concentration) in &eligible {
                            let share = (*concentration as f64).max(1e-10) / total_weight;
                            let demand = bulk_rate * share * years_elapsed;
                            if let Some(deposit) = resources.deposits.get_mut(r_type) {
                                let taking = demand.min(deposit.reserve.planetary_bulk);
                                deposit.reserve.planetary_bulk -= taking;
                                if taking > 0.0 {
                                    budget.add_resource(*r_type, taking);
                                    body.mass -= taking * 1e9;
                                }
                            }
                        }
                    }
                }

                // --- Atmospheric gas harvesting (AtmosphericProcessor) ---
                if total_atmo_rate > 0.0 {
                    let harvestable: Vec<(ResourceType, f32)> = resources.deposits.iter()
                        .filter(|(_, d)| d.is_atmospheric
                            && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001))
                        .map(|(t, d)| (*t, d.reserve.concentration))
                        .collect();

                    if !harvestable.is_empty() {
                        let total_weight: f64 = harvestable.iter()
                            .map(|(_, c)| (*c as f64).max(1e-10))
                            .sum();

                        for (r_type, concentration) in &harvestable {
                            let weight = (*concentration as f64).max(1e-10);
                            let share = weight / total_weight;
                            let effective_rate = total_atmo_rate * share;

                            if let Some(deposit) = resources.deposits.get_mut(&r_type) {
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
                                    budget.add_resource(*r_type, extracted);
                                    body.mass -= extracted * 1e9;
                                }
                            }
                        }
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
    mining_ops: Query<(&MiningOperation, Option<&PlanetResources>)>,
    research_buildings: Query<&crate::research::components::ResearchBuilding>,
    engineering_facilities: Query<&crate::research::components::EngineeringFacility>,
    colony_query: Query<(&Colony, Option<&PlanetResources>)>,
    buildings_data: Option<Res<BuildingsData>>,
    research_state: Res<crate::research::ResearchState>,
) {
    // --- Resource rates from mining (production) ---
    let mut rates = std::collections::HashMap::new();
    
    // 1. MiningOperation components
    for (op, resources_opt) in mining_ops.iter() {
        if !op.active {
            continue;
        }
        // Skip if the targeted deposit is fully depleted
        let depleted = resources_opt.map_or(false, |res| {
            res.deposits.get(&op.resource_type).map_or(true, |d| {
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
    }
    
    // 2. Colony mining & atmospheric harvesting
    if let Some(data) = &buildings_data {
        for (colony, resources_opt) in colony_query.iter() {
            if let Some(resources) = resources_opt {
                let mut surface_rate    = 0.0_f64;
                let mut deep_rate       = 0.0_f64;
                let mut bulk_rate       = 0.0_f64;
                let mut total_atmo_rate = 0.0_f64;

                for (building_type, &count) in &colony.buildings {
                    if count == 0 { continue; }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            match modifier.modifier_type.as_str() {
                                "MiningEfficiency"      => { surface_rate    += modifier.value * count as f64; }
                                "DeepMiningEfficiency"  => { deep_rate       += modifier.value * count as f64; }
                                "BulkMiningEfficiency"  => { bulk_rate       += modifier.value * count as f64; }
                                "AtmosphericHarvesting" => { total_atmo_rate += modifier.value * count as f64; }
                                _ => {}
                            }
                        }
                    }
                }

                // Solid mining rates (weighted by concentration) — one pool per tier
                // Tier 1: Surface / Proven Crustal (MiningEfficiency buildings)
                if surface_rate > 0.0 {
                    let monthly_surface = surface_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let eligible: Vec<(ResourceType, f64)> = resources.deposits.iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &eligible {
                            let share = weight / total_weight;
                            *rates.entry(*r_type).or_insert(0.0) += monthly_surface * share;
                        }
                    }
                }

                // Tier 2: Deep Deposits (DeepMiningEfficiency buildings)
                if deep_rate > 0.0 {
                    let monthly_deep = deep_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let eligible: Vec<(ResourceType, f64)> = resources.deposits.iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.deep_deposits > 0.001)
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &eligible {
                            let share = weight / total_weight;
                            *rates.entry(*r_type).or_insert(0.0) += monthly_deep * share;
                        }
                    }
                }

                // Tier 3: Planetary Bulk (BulkMiningEfficiency buildings)
                if bulk_rate > 0.0 {
                    let monthly_bulk = bulk_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let eligible: Vec<(ResourceType, f64)> = resources.deposits.iter()
                        .filter(|(_, d)| !d.is_atmospheric && d.reserve.planetary_bulk > 0.001)
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &eligible {
                            let share = weight / total_weight;
                            *rates.entry(*r_type).or_insert(0.0) += monthly_bulk * share;
                        }
                    }
                }

                // Atmospheric harvesting rates (weighted by concentration)
                if total_atmo_rate > 0.0 {
                    let monthly_total = total_atmo_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

                    let harvestable: Vec<(ResourceType, f64)> = resources.deposits.iter()
                        .filter(|(_, d)| d.is_atmospheric
                            && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001))
                        .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
                        .collect();

                    let total_weight: f64 = harvestable.iter().map(|(_, w)| w).sum();
                    if total_weight > 0.0 {
                        for (r_type, weight) in &harvestable {
                            let share = weight / total_weight;
                            *rates.entry(*r_type).or_insert(0.0) += monthly_total * share;
                        }
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
    let total_food_net_per_year: f64 = colony_query
        .iter()
        .map(|(colony, _)| colony.food_production_per_year() - colony.food_consumption_per_year())
        .sum();
    let total_food_net_per_month = total_food_net_per_year * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
    if total_food_net_per_month.abs() > f64::EPSILON {
        *rates.entry(ResourceType::Food).or_insert(0.0) += total_food_net_per_month;
    }

    // 4. Subtract maintenance consumption so rates show NET balance
    if let Some(data) = &buildings_data {
        for (colony, _) in colony_query.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 { continue; }
                let maintenance = data.maintenance_resources(building_type);
                for (resource_name, annual_amount) in maintenance {
                    if let Some(rt) = crate::colony::data::parse_resource_type(resource_name) {
                        // annual → monthly
                        let monthly_cost = annual_amount * (count as f64)
                            * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                        *rates.entry(rt).or_insert(0.0) -= monthly_cost;
                    }
                }
            }
        }
    }
    
    tracker.resource_rates = rates;

    // --- Research point rate (include base rate) ---
    // Base RP per month (same constant used in research::systems)
    const BASE_RP_PER_YEAR: f64 = 2000.0;
    let base_rp_monthly = BASE_RP_PER_YEAR * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

    // From ResearchBuilding components (per second → per month)
    let research_per_second: f64 = research_buildings
        .iter()
        .map(|b| b.points_per_second)
        .sum();
    let research_multiplier = research_state.research_speed_multiplier();
    let mut total_research_monthly = base_rp_monthly + research_per_second * SECONDS_PER_MONTH;
    
    // From colony buildings
    if let Some(data) = &buildings_data {
        for (colony, _) in colony_query.iter() {
             for (building_type, &count) in &colony.buildings {
                if count == 0 { continue; }
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
    let mut total_engineering_monthly = base_ep_monthly + engineering_per_second * SECONDS_PER_MONTH;
    
    // From colony buildings
    if let Some(data) = &buildings_data {
        for (colony, _) in colony_query.iter() {
             for (building_type, &count) in &colony.buildings {
                if count == 0 { continue; }
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
    use crate::economy::components::{MineralDeposit, PlanetResources, ResourceReserve};
    use crate::economy::types::ResourceType;

    /// Helper: create a deposit with specific proven/deep/bulk, concentration, and atmospheric flag
    fn make_deposit(proven: f64, deep: f64, bulk: f64, concentration: f32, atmo: bool) -> MineralDeposit {
        let mut d = MineralDeposit::new(proven, deep, bulk, concentration, 0.8);
        d.is_atmospheric = atmo;
        d
    }

    #[test]
    fn test_mines_only_extract_non_atmospheric() {
        // Iron (solid) and O2 (atmospheric) both present
        let mut resources = PlanetResources::new();
        resources.add_deposit(ResourceType::Iron, make_deposit(1000.0, 500.0, 0.0, 0.5, false));
        resources.add_deposit(ResourceType::Oxygen, make_deposit(2000.0, 100.0, 0.0, 0.9, true));

        // Simulate what mining does: only mine non-atmospheric
        let minable: Vec<ResourceType> = resources.deposits.iter()
            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, _)| *t)
            .collect();

        assert!(minable.contains(&ResourceType::Iron), "Iron should be minable");
        assert!(!minable.contains(&ResourceType::Oxygen), "Atmospheric O2 should NOT be minable");
    }

    #[test]
    fn test_atmo_processor_only_extracts_atmospheric() {
        let mut resources = PlanetResources::new();
        resources.add_deposit(ResourceType::Iron, make_deposit(1000.0, 500.0, 0.0, 0.5, false));
        resources.add_deposit(ResourceType::Nitrogen, make_deposit(5000.0, 200.0, 0.0, 0.7, true));
        resources.add_deposit(ResourceType::Oxygen, make_deposit(2000.0, 100.0, 0.0, 0.9, true));

        let harvestable: Vec<ResourceType> = resources.deposits.iter()
            .filter(|(_, d)| d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, _)| *t)
            .collect();

        assert!(!harvestable.contains(&ResourceType::Iron), "Iron should NOT be harvestable");
        assert!(harvestable.contains(&ResourceType::Nitrogen), "N2 should be harvestable");
        assert!(harvestable.contains(&ResourceType::Oxygen), "O2 should be harvestable");
    }

    #[test]
    fn test_concentration_weights_mining_distribution() {
        let mut resources = PlanetResources::new();
        // Iron: 50% concentration, Titanium: 10% concentration
        resources.add_deposit(ResourceType::Iron, make_deposit(1000.0, 0.0, 0.0, 0.5, false));
        resources.add_deposit(ResourceType::Titanium, make_deposit(1000.0, 0.0, 0.0, 0.1, false));

        let minable: Vec<(ResourceType, f64)> = resources.deposits.iter()
            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
            .collect();

        let total_weight: f64 = minable.iter().map(|(_, w)| w).sum();
        assert!((total_weight - 0.6).abs() < 0.01, "Total weight should be 0.6");

        for (r_type, weight) in &minable {
            let share = weight / total_weight;
            match r_type {
                ResourceType::Iron => {
                    // Iron gets 0.5/0.6 ≈ 83% of mining effort
                    assert!(share > 0.8 && share < 0.9,
                        "Iron (50% conc.) should get ~83% share, got {:.1}%", share * 100.0);
                }
                ResourceType::Titanium => {
                    // Titanium gets 0.1/0.6 ≈ 17%
                    assert!(share > 0.15 && share < 0.2,
                        "Titanium (10% conc.) should get ~17% share, got {:.1}%", share * 100.0);
                }
                _ => panic!("Unexpected resource type"),
            }
        }
    }

    #[test]
    fn test_trace_deposits_not_extracted() {
        let mut resources = PlanetResources::new();
        // Sub-kiloton deposit should be filtered out
        resources.add_deposit(ResourceType::Gold, make_deposit(0.0005, 0.0, 0.0, 0.01, false));

        let minable: Vec<ResourceType> = resources.deposits.iter()
            .filter(|(_, d)| !d.is_atmospheric
                && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001))
            .map(|(t, _)| *t)
            .collect();

        assert!(minable.is_empty(), "Sub-kiloton Gold should not be minable");
    }
}
