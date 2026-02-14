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
    mut query: Query<(&mut PlanetResources, &MiningOperation, &mut CelestialBody)>,
    mut colony_query: Query<(&Colony, &mut PlanetResources, &mut CelestialBody), Without<MiningOperation>>,
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

    // 1. Process specific MiningOperations (legacy/scenario)
    for (mut resources, op, mut body) in query.iter_mut() {
        if !op.active {
            continue;
        }

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
    
    // 2. Process Colony Mining
    if let Some(data) = buildings_data {
        for (colony, mut resources, mut body) in colony_query.iter_mut() {
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
            
            if total_mining_rate <= 0.0 { continue; }
            
            // Distribute across available deposits
            // Find accessible resources
            let accessible_resources: Vec<ResourceType> = resources.deposits.iter()
                .filter(|(_, d)| d.reserve.proven_crustal > 0.0 || d.reserve.deep_deposits > 0.0)
                .map(|(t, _)| *t)
                .collect();
                
            if accessible_resources.is_empty() { continue; }
            
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

/// System that computes monthly production rates for all resources and
/// research/engineering points, writing them into [`ResourceRateTracker`].
///
/// This is purely informational – it does not move any resources.
pub fn update_resource_rates(
    mut tracker: ResMut<ResourceRateTracker>,
    mining_ops: Query<&MiningOperation>,
    research_buildings: Query<&crate::research::components::ResearchBuilding>,
    engineering_facilities: Query<&crate::research::components::EngineeringFacility>,
    colony_query: Query<(&Colony, Option<&PlanetResources>)>,
    buildings_data: Option<Res<BuildingsData>>,
    research_state: Res<crate::research::ResearchState>,
) {
    // --- Resource rates from mining ---
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
            }
        }
    }
    
    tracker.resource_rates = rates;

    // --- Research point rate ---
    // From components (per second)
    let research_per_second: f64 = research_buildings
        .iter()
        .map(|b| b.points_per_second)
        .sum();
    let research_multiplier = research_state.research_speed_multiplier();
    let mut total_research_monthly = research_per_second * SECONDS_PER_MONTH;
    
    // From colonies (per month assumption)
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

    // --- Engineering point rate ---
    // From components
    let engineering_per_second: f64 = engineering_facilities
        .iter()
        .map(|f| f.points_per_second)
        .sum();
    let engineering_multiplier = research_state.engineering_speed_multiplier();
    let mut total_engineering_monthly = engineering_per_second * SECONDS_PER_MONTH;
    
    // From colonies
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
