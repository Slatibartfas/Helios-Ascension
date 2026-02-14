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
    
        // 2. Process Colony Mining
        if let Some(colony) = colony_opt {
            if let Some(data) = &buildings_data {
                // Calculate total mining capacity (Mt/year)
                let mut total_mining_rate = 0.0;
                for (building_type, &count) in &colony.buildings {
                    if count == 0 { continue; }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            if modifier.modifier_type == "MiningEfficiency" {
                                total_mining_rate += modifier.value * count as f64;
                            }
                        }
                    }
                }
                
                if total_mining_rate > 0.0 {
                    // Distribute across available deposits
                    // Find accessible resources
                    let accessible_resources: Vec<ResourceType> = resources.deposits.iter()
                        .filter(|(_, d)| d.reserve.proven_crustal > 0.0 || d.reserve.deep_deposits > 0.0)
                        .map(|(t, _)| *t)
                        .collect();
                        
                    if !accessible_resources.is_empty() {
                        let rate_per_resource = total_mining_rate / accessible_resources.len() as f64;
                        
                        for r_type in accessible_resources {
                            if let Some(deposit) = resources.deposits.get_mut(&r_type) {
                                 let mut demand = rate_per_resource * years_elapsed;
                                 let mut extracted = 0.0;
                                 
                                 // Proven
                                 let taking_proven = demand.min(deposit.reserve.proven_crustal);
                                 deposit.reserve.proven_crustal -= taking_proven;
                                 extracted += taking_proven;
                                 demand -= taking_proven;
                                 
                                 // Deep
                                 if demand > 0.0 {
                                     let taking_deep = demand.min(deposit.reserve.deep_deposits);
                                     deposit.reserve.deep_deposits -= taking_deep;
                                     extracted += taking_deep;
                                 }
                                 
                                 if extracted > 0.0 {
                                     budget.add_resource(r_type, extracted);
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
/// Production comes from `MiningOperation` components and colony mining
/// buildings. Consumption comes from building maintenance costs (the same
/// costs deducted by `deduct_maintenance_resources`). The displayed rate
/// is production − maintenance so the UI shows the true net balance.
///
/// RP/EP rates include the base generation rates defined in
/// `research::systems` (`BASE_RP_PER_YEAR`, `BASE_EP_PER_YEAR`) so the
/// bar always reflects actual accumulation.
pub fn update_resource_rates(
    mut tracker: ResMut<ResourceRateTracker>,
    mining_ops: Query<&MiningOperation>,
    research_buildings: Query<&crate::research::components::ResearchBuilding>,
    engineering_facilities: Query<&crate::research::components::EngineeringFacility>,
    colony_query: Query<(&Colony, Option<&PlanetResources>)>,
    buildings_data: Option<Res<BuildingsData>>,
    research_state: Res<crate::research::ResearchState>,
) {
    // --- Resource rates from mining (production) ---
    let mut rates = std::collections::HashMap::new();
    
    // 1. MiningOperation components
    for op in mining_ops.iter() {
        if !op.active {
            continue;
        }
        // base_rate_mt_per_year → per month = rate * (month / year)
        let monthly = op.base_rate_mt_per_year * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
        *rates.entry(op.resource_type).or_insert(0.0) += monthly;
    }
    
    // 2. Colony mining
    if let Some(data) = &buildings_data {
        for (colony, resources_opt) in colony_query.iter() {
            if let Some(resources) = resources_opt {
                 let mut total_mining_rate = 0.0;
                 for (building_type, &count) in &colony.buildings {
                    if count == 0 { continue; }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            if modifier.modifier_type == "MiningEfficiency" {
                                total_mining_rate += modifier.value * count as f64;
                            }
                        }
                    }
                }
                
                if total_mining_rate > 0.0 {
                    let monthly_total = total_mining_rate * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                    
                    let accessible_resources: Vec<ResourceType> = resources.deposits.iter()
                        .filter(|(_, d)| d.reserve.proven_crustal > 0.0 || d.reserve.deep_deposits > 0.0)
                        .map(|(t, _)| *t)
                        .collect();
                        
                    if !accessible_resources.is_empty() {
                        let rate_per_resource = monthly_total / accessible_resources.len() as f64;
                        for r_type in accessible_resources {
                            *rates.entry(r_type).or_insert(0.0) += rate_per_resource;
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

    // 3. Subtract maintenance consumption so rates show NET balance
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
