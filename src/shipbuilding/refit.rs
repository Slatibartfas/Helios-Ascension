use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::economy::ResourceType;

/// A refit project: replacing components on an existing ship with a new design.
/// Refits on the same hull cost only 20% of removed BP + 100% of added BP.
/// Refits to a different hull require full reconstruction (not a RefitProject).
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct RefitProject {
    /// The ship entity being refit
    pub ship_entity: Entity,
    /// The design currently installed on the ship.
    pub old_template_id: uuid::Uuid,
    /// The new design template being applied
    pub new_template_id: uuid::Uuid,
    /// Build points cost for this refit
    pub bp_cost: f64,
    /// Resource costs for the refit
    pub resource_costs: Vec<(ResourceType, f64)>,
    /// Current refit progress in BP
    pub progress: f64,
    /// The shipyard where refit is occurring
    pub build_site: Entity,
    /// Slipway index (if applicable)
    pub slipway_id: u32,
    /// Whether the refit is stalled waiting for resources.
    pub awaiting_resources: bool,
    /// Outstanding logistics requests blocking work.
    pub blocking_request_ids: Vec<u64>,
}

impl RefitProject {
    /// Calculate refit cost for swapping modules on the same hull.
    /// Formula: 20% of removed module BP + 100% of added module BP.
    #[allow(unused_variables)]
    pub fn calculate_refit_bp(
        removed_module_ids: &[&str],
        added_module_ids: &[&str],
        shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    ) -> f64 {
        let mut removed_bp = 0.0;
        let mut added_bp = 0.0;

        for module_id in removed_module_ids {
            if let Some(module) = shipbuilding_data.get_module(module_id) {
                removed_bp += module.build_points;
            }
        }

        for module_id in added_module_ids {
            if let Some(module) = shipbuilding_data.get_module(module_id) {
                added_bp += module.build_points;
            }
        }

        // Same-hull refit: 20% of removed + 100% of added
        removed_bp * 0.20 + added_bp
    }

    /// Calculate resource costs for refit (same formula as construction for new modules).
    pub fn calculate_refit_resources(
        _removed_module_ids: &[&str],
        added_module_ids: &[&str],
        shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    ) -> Vec<(ResourceType, f64)> {
        // For refits, we only pay for new modules being added (removed modules are salvaged)
        let mut costs: Vec<(ResourceType, f64)> = Vec::new();

        for module_id in added_module_ids {
            if let Some(module) = shipbuilding_data.get_module(module_id) {
                for (resource, amount) in &module.resource_costs {
                    if let Some((_, existing)) = costs.iter_mut().find(|(r, _)| r == resource) {
                        *existing += *amount;
                    } else {
                        costs.push((*resource, *amount));
                    }
                }
            }
        }

        costs
    }

    pub fn progress_percent(&self) -> f32 {
        if self.bp_cost <= 0.0 {
            return 1.0;
        }
        (self.progress / self.bp_cost).min(1.0) as f32
    }
}

/// Pending refit action from UI.
#[derive(Debug, Clone)]
pub struct QueueRefitAction {
    pub ship_entity: Entity,
    pub new_template_id: uuid::Uuid,
    pub build_site: Entity,
}

/// Check if a design change constitutes a same-hull refit or requires full reconstruction.
pub fn determine_refit_type(existing_hull_id: &str, new_hull_id: &str) -> RefitType {
    if existing_hull_id == new_hull_id {
        RefitType::SameHull
    } else {
        RefitType::DifferentHull
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefitType {
    /// Same hull - cheap component swap
    SameHull,
    /// Different hull - full reconstruction required
    DifferentHull,
}
