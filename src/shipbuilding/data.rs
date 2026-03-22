use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::components::ShipDesignDraft;
use super::types::{ConstructionMode, HullSizeTier, ShipModuleCategory};
use crate::economy::ResourceType;
use crate::fleets::{PropulsionType, ShipClass};
use crate::research::ResearchState;

pub type ResourceCostEntry = (ResourceType, f64);
pub type AttributeEntry = (String, f64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HullSlotDefinition {
    pub slot_id: String,
    pub category: ShipModuleCategory,
    pub size: String,
    #[serde(default = "default_required_slot")]
    pub required: bool,
    /// Optional normalized position (0-1) on hull sprite for graphical editor.
    /// x = left to right, y = bottom to top.
    #[serde(default)]
    pub position: Option<(f32, f32)>,
    /// Optional rotation in degrees for weapon/sensor arcs.
    #[serde(default)]
    pub rotation_deg: Option<f32>,
}

fn default_required_slot() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipHullDefinition {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub class: ShipClass,
    pub base_build_points: f64,
    pub base_dry_mass_t: f64,
    pub default_construction_mode: ConstructionMode,
    #[serde(default)]
    pub surface_launchable: bool,
    #[serde(default)]
    pub orbital_only: bool,
    #[serde(default)]
    pub is_station: bool,
    /// Optional explicit size tier. If None, derived from base_dry_mass_t and is_station.
    #[serde(default)]
    pub size_tier: Option<HullSizeTier>,
    #[serde(default)]
    pub required_tech: Option<String>,
    #[serde(default)]
    pub resource_costs: Vec<ResourceCostEntry>,
    #[serde(default)]
    pub slot_layout: Vec<HullSlotDefinition>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ShipHullDefinition {
    /// Effective size tier — uses explicit field if set, otherwise derives from mass.
    pub fn effective_size_tier(&self) -> HullSizeTier {
        self.size_tier
            .unwrap_or_else(|| HullSizeTier::from_mass_t(self.base_dry_mass_t, self.is_station))
    }

    pub fn mode_compatibility_error(&self, mode: ConstructionMode) -> Option<&'static str> {
        match mode {
            ConstructionMode::SurfaceLaunch if self.orbital_only => {
                Some("This hull is orbital-only and cannot be surface launched.")
            }
            ConstructionMode::SurfaceLaunch if !self.surface_launchable => {
                Some("This hull does not support surface launch.")
            }
            _ => None,
        }
    }

    pub fn supports_construction_mode(&self, mode: ConstructionMode) -> bool {
        self.mode_compatibility_error(mode).is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipModuleDefinition {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub category: ShipModuleCategory,
    pub size: String,
    #[serde(default)]
    pub propulsion: Option<PropulsionType>,
    #[serde(default)]
    pub required_tech: Option<String>,
    #[serde(default)]
    pub required_component_design: Option<String>,
    #[serde(default)]
    pub power_generation_mw: f64,
    #[serde(default)]
    pub power_draw_mw: f64,
    #[serde(default)]
    pub thrust_kn: f64,
    #[serde(default)]
    pub isp_s: f64,
    #[serde(default)]
    pub dry_mass_t: f64,
    #[serde(default)]
    pub build_points: f64,
    #[serde(default)]
    pub construction_capacity_bp_per_year: f64,
    #[serde(default)]
    pub launch_capacity_t_per_year: f64,
    #[serde(default)]
    pub resource_costs: Vec<ResourceCostEntry>,
    #[serde(default)]
    pub attribute_values: Vec<AttributeEntry>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ShipHullsFile {
    hulls: Vec<ShipHullDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ShipModulesFile {
    modules: Vec<ShipModuleDefinition>,
}

#[derive(Debug, Clone)]
pub struct ShipDesignSummary {
    pub hull_name: String,
    pub ship_class: ShipClass,
    pub build_points: f64,
    pub dry_mass_t: f64,
    pub launch_mass_t: f64,
    pub fuel_capacity_t: f64,
    pub cargo_capacity_t: f64,
    pub ordnance_capacity_t: f64,
    pub magazine_capacity_t: f64,
    pub crew: f64,
    pub power_generation_mw: f64,
    pub power_draw_mw: f64,
    pub thrust_kn: f64,
    pub isp_s: f64,
    pub acceleration_ms2: f64,
    pub delta_v_ms: f64,
    pub sensor_range_au: f64,
    pub docking_ports: f64,
    pub construction_capacity_bp_per_year: f64,
    pub launch_capacity_t_per_year: f64,
    pub propulsion: Option<PropulsionType>,
    pub resource_costs: Vec<(ResourceType, f64)>,
    pub missing_required_slots: Vec<String>,
    pub is_station: bool,
    // New fields for expanded component categories
    pub isru_rate_t_per_year: f64,
    pub heat_sink_capacity: f64,
    pub maintenance_rate: f64,
}

impl Default for ShipDesignSummary {
    fn default() -> Self {
        Self {
            hull_name: String::new(),
            ship_class: ShipClass::Courier,
            build_points: 0.0,
            dry_mass_t: 0.0,
            launch_mass_t: 0.0,
            fuel_capacity_t: 0.0,
            cargo_capacity_t: 0.0,
            ordnance_capacity_t: 0.0,
            magazine_capacity_t: 0.0,
            crew: 0.0,
            power_generation_mw: 0.0,
            power_draw_mw: 0.0,
            thrust_kn: 0.0,
            isp_s: 0.0,
            acceleration_ms2: 0.0,
            delta_v_ms: 0.0,
            sensor_range_au: 0.0,
            docking_ports: 0.0,
            construction_capacity_bp_per_year: 0.0,
            launch_capacity_t_per_year: 0.0,
            propulsion: None,
            resource_costs: Vec::new(),
            missing_required_slots: Vec::new(),
            is_station: false,
            isru_rate_t_per_year: 0.0,
            heat_sink_capacity: 0.0,
            maintenance_rate: 0.0,
        }
    }
}

impl ShipDesignSummary {
    pub fn power_balance_mw(&self) -> f64 {
        self.power_generation_mw - self.power_draw_mw
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ShipbuildingData {
    pub hulls: HashMap<String, ShipHullDefinition>,
    pub modules: HashMap<String, ShipModuleDefinition>,
}

impl ShipbuildingData {
    pub fn get_hull(&self, hull_id: &str) -> Option<&ShipHullDefinition> {
        self.hulls.get(hull_id)
    }

    pub fn get_module(&self, module_id: &str) -> Option<&ShipModuleDefinition> {
        self.modules.get(module_id)
    }

    pub fn hull_is_unlocked(
        &self,
        hull: &ShipHullDefinition,
        research_state: &ResearchState,
    ) -> bool {
        hull.required_tech
            .as_deref()
            .is_none_or(|tech| research_state.is_unlocked(tech))
    }

    pub fn module_is_unlocked(
        &self,
        module: &ShipModuleDefinition,
        research_state: &ResearchState,
    ) -> bool {
        let tech_ok = module
            .required_tech
            .as_deref()
            .is_none_or(|tech| research_state.is_unlocked(tech));
        let component_ok = module
            .required_component_design
            .as_deref()
            .is_none_or(|component| research_state.is_component_completed(component));

        tech_ok && component_ok
    }

    pub fn available_hulls<'a>(
        &'a self,
        research_state: &ResearchState,
    ) -> Vec<&'a ShipHullDefinition> {
        let mut hulls: Vec<_> = self
            .hulls
            .values()
            .filter(|hull| self.hull_is_unlocked(hull, research_state))
            .collect();
        hulls.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        hulls
    }

    pub fn compatible_modules_for_slot<'a>(
        &'a self,
        slot: &HullSlotDefinition,
        research_state: &ResearchState,
    ) -> Vec<&'a ShipModuleDefinition> {
        let mut modules: Vec<_> = self
            .modules
            .values()
            .filter(|module| self.module_is_unlocked(module, research_state))
            .filter(|module| module.category == slot.category)
            .filter(|module| module.size == slot.size || slot.size == "Any" || module.size == "Any")
            .collect();
        modules.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        modules
    }

    pub fn summarize_design(
        &self,
        design: &ShipDesignDraft,
        research_state: &ResearchState,
    ) -> Option<ShipDesignSummary> {
        let hull = self.get_hull(&design.hull_id)?;
        if !self.hull_is_unlocked(hull, research_state) {
            return None;
        }

        let selected_by_slot: HashMap<&str, &str> = design
            .modules
            .iter()
            .map(|selection| (selection.slot_id.as_str(), selection.module_id.as_str()))
            .collect();

        let mut summary = ShipDesignSummary {
            hull_name: hull.display_name.clone(),
            ship_class: hull.class,
            build_points: hull.base_build_points,
            dry_mass_t: hull.base_dry_mass_t,
            launch_mass_t: hull.base_dry_mass_t,
            resource_costs: hull.resource_costs.clone(),
            is_station: hull.is_station,
            ..Default::default()
        };

        let mut weighted_isp = 0.0;
        let mut thrust_total = 0.0;

        for slot in &hull.slot_layout {
            let Some(module_id) = selected_by_slot.get(slot.slot_id.as_str()) else {
                if slot.required {
                    summary.missing_required_slots.push(slot.slot_id.clone());
                }
                continue;
            };

            let Some(module) = self.get_module(module_id) else {
                if slot.required {
                    summary.missing_required_slots.push(slot.slot_id.clone());
                }
                continue;
            };

            if !self.module_is_unlocked(module, research_state) {
                return None;
            }

            if module.category != slot.category
                || !(module.size == slot.size || slot.size == "Any" || module.size == "Any")
            {
                return None;
            }

            summary.build_points += module.build_points;
            summary.dry_mass_t += module.dry_mass_t;
            summary.power_generation_mw += module.power_generation_mw;
            summary.power_draw_mw += module.power_draw_mw;
            summary.thrust_kn += module.thrust_kn;
            summary.construction_capacity_bp_per_year += module.construction_capacity_bp_per_year;
            summary.launch_capacity_t_per_year += module.launch_capacity_t_per_year;
            accumulate_attributes(&mut summary, &module.attribute_values);

            if module.thrust_kn > 0.0 && module.isp_s > 0.0 {
                thrust_total += module.thrust_kn;
                weighted_isp += module.thrust_kn * module.isp_s;
                if summary.propulsion.is_none() {
                    summary.propulsion = module.propulsion;
                }
            }

            accumulate_costs(&mut summary.resource_costs, &module.resource_costs);
        }

        if thrust_total > 0.0 {
            summary.isp_s = weighted_isp / thrust_total;
        }

        summary.launch_mass_t = summary.dry_mass_t + summary.fuel_capacity_t;

        if summary.launch_mass_t > summary.dry_mass_t && summary.isp_s > 0.0 {
            let wet = summary.launch_mass_t;
            let dry = summary.dry_mass_t.max(1.0);
            summary.acceleration_ms2 = summary.thrust_kn / wet.max(1.0);
            summary.delta_v_ms = summary.isp_s * 9.806_65 * (wet / dry).ln();
        } else if summary.launch_mass_t > 0.0 {
            summary.acceleration_ms2 = summary.thrust_kn / summary.launch_mass_t.max(1.0);
        }

        Some(summary)
    }
}

/// Global library of ship design templates (designs / classes).
/// Loaded/saved with game state. Separate from hull/module definition data.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShipDesignLibrary {
    pub templates: HashMap<uuid::Uuid, crate::shipbuilding::ShipDesignTemplate>,
}

impl ShipDesignLibrary {
    /// Save or update a template and return its ID.
    pub fn save_template(
        &mut self,
        template: crate::shipbuilding::ShipDesignTemplate,
    ) -> uuid::Uuid {
        let id = template.id;
        self.templates.insert(id, template);
        id
    }

    /// Get a template by ID.
    pub fn get_template(
        &self,
        id: &uuid::Uuid,
    ) -> Option<&crate::shipbuilding::ShipDesignTemplate> {
        self.templates.get(id)
    }

    /// Get a template by ID (mutable).
    pub fn get_template_mut(
        &mut self,
        id: &uuid::Uuid,
    ) -> Option<&mut crate::shipbuilding::ShipDesignTemplate> {
        self.templates.get_mut(id)
    }

    /// List all templates sorted by name, then version.
    pub fn all_templates(&self) -> Vec<&crate::shipbuilding::ShipDesignTemplate> {
        let mut list: Vec<_> = self.templates.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));
        list
    }

    /// Get all versions of a design by name prefix.
    pub fn version_history(&self, name: &str) -> Vec<&crate::shipbuilding::ShipDesignTemplate> {
        let mut versions: Vec<_> = self
            .templates
            .values()
            .filter(|t| t.name.starts_with(name))
            .collect();
        versions.sort_by_key(|t| t.version);
        versions
    }

    /// Get the latest version number for a design name.
    pub fn latest_version(&self, name: &str) -> u32 {
        self.version_history(name)
            .last()
            .map(|t| t.version)
            .unwrap_or(0)
    }

    /// Create a new version of an existing template with modified modules.
    pub fn create_new_version(
        &mut self,
        parent_id: &uuid::Uuid,
        new_modules: Vec<crate::shipbuilding::ShipModuleSelection>,
        current_game_time: f64,
    ) -> Option<uuid::Uuid> {
        let parent = self.templates.get(parent_id)?;
        let new_version = parent.version + 1;
        let new_id = uuid::Uuid::new_v4();
        let template = crate::shipbuilding::ShipDesignTemplate {
            id: new_id,
            name: parent.name.clone(),
            hull_id: parent.hull_id.clone(),
            modules: new_modules,
            version: new_version,
            parent_template_id: Some(*parent_id),
            created_at_game_time: current_game_time,
            construction_mode: parent.construction_mode,
        };
        self.templates.insert(new_id, template);
        Some(new_id)
    }
}

fn accumulate_costs(target: &mut Vec<(ResourceType, f64)>, added: &[(ResourceType, f64)]) {
    for (resource, amount) in added {
        if let Some((_, existing)) = target
            .iter_mut()
            .find(|(existing_resource, _)| existing_resource == resource)
        {
            *existing += *amount;
        } else {
            target.push((*resource, *amount));
        }
    }
}

fn accumulate_attributes(summary: &mut ShipDesignSummary, attributes: &[AttributeEntry]) {
    for (name, value) in attributes {
        match name.as_str() {
            "crew" | "crew_capacity" => summary.crew += *value,
            "cargo_capacity_t" => summary.cargo_capacity_t += *value,
            "fuel_capacity_t" => summary.fuel_capacity_t += *value,
            "ordnance_capacity_t" => summary.ordnance_capacity_t += *value,
            "magazine_capacity_t" => summary.magazine_capacity_t += *value,
            "sensor_range_au" => summary.sensor_range_au += *value,
            "docking_ports" => summary.docking_ports += *value,
            "isru_rate_t_per_year" => summary.isru_rate_t_per_year += *value,
            "heat_sink_capacity" => summary.heat_sink_capacity += *value,
            "maintenance_rate" => summary.maintenance_rate += *value,
            _ => {}
        }
    }
}

pub fn load_shipbuilding_data(mut commands: Commands) {
    info!("Loading shipbuilding definitions...");

    let hulls = load_hulls_file("assets/data/ship_hulls.ron");
    let modules = load_modules_file("assets/data/ship_modules.ron");

    info!(
        "Loaded {} hulls and {} ship modules",
        hulls.len(),
        modules.len()
    );

    commands.insert_resource(ShipbuildingData { hulls, modules });
}

fn load_hulls_file(path: &str) -> HashMap<String, ShipHullDefinition> {
    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<ShipHullsFile>(&contents) {
            Ok(file) => file
                .hulls
                .into_iter()
                .map(|hull| (hull.id.clone(), hull))
                .collect(),
            Err(error) => {
                error!("Failed to parse ship hull data file {}: {}", path, error);
                HashMap::default()
            }
        },
        Err(error) => {
            warn!("Ship hull data file not found at {}: {}", path, error);
            HashMap::default()
        }
    }
}

fn load_modules_file(path: &str) -> HashMap<String, ShipModuleDefinition> {
    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<ShipModulesFile>(&contents) {
            Ok(file) => file
                .modules
                .into_iter()
                .map(|module| (module.id.clone(), module))
                .collect(),
            Err(error) => {
                error!("Failed to parse ship module data file {}: {}", path, error);
                HashMap::default()
            }
        },
        Err(error) => {
            warn!("Ship module data file not found at {}: {}", path, error);
            HashMap::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shipbuilding::ShipModuleSelection;

    #[test]
    fn summarize_design_accumulates_module_stats() {
        let mut data = ShipbuildingData::default();
        data.hulls.insert(
            "test_hull".to_string(),
            ShipHullDefinition {
                id: "test_hull".to_string(),
                display_name: "Test Hull".to_string(),
                description: String::new(),
                class: ShipClass::Courier,
                base_build_points: 100.0,
                base_dry_mass_t: 10.0,
                default_construction_mode: ConstructionMode::SurfaceLaunch,
                surface_launchable: true,
                orbital_only: false,
                is_station: false,
                size_tier: None,
                required_tech: None,
                resource_costs: vec![(ResourceType::Iron, 3.0)],
                slot_layout: vec![HullSlotDefinition {
                    slot_id: "drive".to_string(),
                    category: ShipModuleCategory::Propulsion,
                    size: "Small".to_string(),
                    required: true,
                    position: None,
                    rotation_deg: None,
                }],
                tags: Vec::new(),
            },
        );
        data.modules.insert(
            "drive_1".to_string(),
            ShipModuleDefinition {
                id: "drive_1".to_string(),
                display_name: "Drive".to_string(),
                description: String::new(),
                category: ShipModuleCategory::Propulsion,
                size: "Small".to_string(),
                propulsion: Some(PropulsionType::Chemical),
                required_tech: None,
                required_component_design: None,
                power_generation_mw: 0.0,
                power_draw_mw: 2.0,
                thrust_kn: 4.0,
                isp_s: 300.0,
                dry_mass_t: 5.0,
                build_points: 20.0,
                construction_capacity_bp_per_year: 0.0,
                launch_capacity_t_per_year: 0.0,
                resource_costs: vec![(ResourceType::Iron, 2.0)],
                attribute_values: Vec::new(),
                tags: Vec::new(),
            },
        );

        let summary = data
            .summarize_design(
                &ShipDesignDraft {
                    name: "Test".to_string(),
                    hull_id: "test_hull".to_string(),
                    modules: vec![ShipModuleSelection {
                        slot_id: "drive".to_string(),
                        module_id: "drive_1".to_string(),
                    }],
                    construction_mode: ConstructionMode::SurfaceLaunch,
                },
                &ResearchState::default(),
            )
            .expect("design summary should exist");

        assert_eq!(summary.build_points, 120.0);
        assert_eq!(summary.dry_mass_t, 15.0);
        assert_eq!(summary.thrust_kn, 4.0);
        assert_eq!(summary.resource_costs.len(), 1);
        assert_eq!(summary.resource_costs[0], (ResourceType::Iron, 5.0));
    }

    #[test]
    fn orbital_only_hulls_reject_surface_launch_mode() {
        let hull = ShipHullDefinition {
            id: "orbital_hull".to_string(),
            display_name: "Orbital Hull".to_string(),
            description: String::new(),
            class: ShipClass::Frigate,
            base_build_points: 100.0,
            base_dry_mass_t: 10.0,
            default_construction_mode: ConstructionMode::OrbitalAssembly,
            surface_launchable: false,
            orbital_only: true,
            is_station: false,
            size_tier: None,
            required_tech: None,
            resource_costs: Vec::new(),
            slot_layout: Vec::new(),
            tags: Vec::new(),
        };

        assert!(!hull.supports_construction_mode(ConstructionMode::SurfaceLaunch));
        assert!(hull.supports_construction_mode(ConstructionMode::OrbitalAssembly));
    }
}
