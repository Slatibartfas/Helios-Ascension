use crate::economy::budget::GlobalBudget;
use crate::economy::components::PlanetResources;
use crate::economy::types::ResourceType;
use crate::ui::TimeScale;
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
    mut query: Query<(&mut PlanetResources, &MiningOperation)>,
    time: Res<Time<Real>>,
    time_scale: Option<Res<TimeScale>>,
) {
    // Time<Real> is required for Bevy 0.14
    let scale = time_scale.map(|s| s.scale).unwrap_or(1.0);
    if scale == 0.0 {
        return;
    }

    let dt = time.delta_seconds() as f64 * scale as f64;
    // 1 year = 365.25 days * 24 * 60 * 60
    let years_elapsed = dt / 31_557_600.0;

    if years_elapsed <= 0.0 {
        return;
    }

    for (mut resources, op) in query.iter_mut() {
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
            // Note: GlobalBudget stockpiles are likely in relevant units (unknown if Mt or tons)
            // The budget uses `f64`. Assuming units match (Mt).
            if total_extracted > 0.0 {
                budget.add_resource(op.resource_type, total_extracted);
            }
        }
    }
}
