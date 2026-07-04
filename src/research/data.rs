use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use crate::shipbuilding::ShipbuildingData;

use super::types::{ComponentDefinition, Technology, TechnologyId};

/// Resource that holds all technology definitions loaded from data files
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct TechnologiesData {
    /// All technologies indexed by ID
    pub technologies: HashMap<TechnologyId, Technology>,
    /// All component definitions
    pub components: HashMap<String, ComponentDefinition>,
    /// Human-readable hull unlocks grouped by technology ID.
    pub hull_unlocks: HashMap<TechnologyId, Vec<String>>,
}

impl TechnologiesData {
    /// Get a technology by ID
    pub fn get_tech(&self, id: &str) -> Option<&Technology> {
        self.technologies.get(id)
    }

    /// Get a component definition by ID
    pub fn get_component(&self, id: &str) -> Option<&ComponentDefinition> {
        self.components.get(id)
    }

    /// Get all technologies in a category
    pub fn get_by_category(&self, category: super::types::TechCategory) -> Vec<&Technology> {
        self.technologies
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Check if all prerequisites for a technology are satisfied
    pub fn check_prerequisites(&self, tech_id: &str, unlocked: &[TechnologyId]) -> bool {
        if let Some(tech) = self.get_tech(tech_id) {
            tech.prerequisites
                .iter()
                .all(|prereq| unlocked.contains(prereq))
        } else {
            false
        }
    }

    /// Pure gate: can a technology at `tech_tier` unlock a ship module at `module_tier`?
    ///
    /// Rule: a module is available iff the unlocking tech's tier is at least the
    /// module's tier. Tiers are 1-based; `tech_tier == 0` is treated as no tech and
    /// cannot unlock any module. Executable documentation of the tier-cap rule
    /// from the GRA-349 audit contract; production wiring is a separate PR.
    pub fn is_module_available_for_module_tier(tech_tier: u32, module_tier: u8) -> bool {
        tech_tier >= module_tier as u32
    }

    fn unlocking_tech_for_component(&self, component_id: &str) -> Option<String> {
        self.technologies.values().find_map(|tech| {
            if tech.unlocks_components.iter().any(|id| id == component_id)
                || tech.unlocks_engineering.iter().any(|id| id == component_id)
            {
                Some(tech.id.clone())
            } else {
                None
            }
        })
    }

    pub fn merge_ship_modules_as_components(&mut self, shipbuilding_data: &ShipbuildingData) {
        // Group every ship module by its resolved engineering-project ID, then pick the
        // lowest-tier candidate. Without this, HashMap iteration order would select a
        // random tier — the GRA-40 ship module catalog (PR #104) added 6 more
        // cargo_module entries, which surfaced in the startup test as a non-deterministic
        // baseline-engineering failure (cargo_module kept the tier-2/3 required_tech).
        let mut candidates: HashMap<String, Vec<(u8, ComponentDefinition)>> = HashMap::new();
        for module in shipbuilding_data.modules.values() {
            let component_id = module.engineering_project_id().to_string();
            let required_tech = module
                .required_tech
                .clone()
                .or_else(|| self.unlocking_tech_for_component(&component_id))
                .unwrap_or_default();
            let definition = ComponentDefinition {
                id: component_id.clone(),
                name: module.display_name.clone(),
                description: module.description.clone(),
                engineering_cost: module.build_points.max(1.0),
                required_tech,
            };
            candidates
                .entry(component_id)
                .or_default()
                .push((module.tier, definition));
        }

        for (component_id, mut entries) in candidates {
            // Sort by tier ascending, then by required_tech ascending for determinism.
            entries.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.required_tech.cmp(&b.1.required_tech))
            });
            let (_, definition) = entries.into_iter().next().expect("non-empty");
            // Only insert if the component is not already present (preserves the prior
            // behavior when `components` is pre-populated from technologies.ron).
            self.components.entry(component_id).or_insert(definition);
        }

        self.hull_unlocks.clear();
        for hull in shipbuilding_data.hulls.values() {
            let Some(tech_id) = hull.required_tech.as_ref() else {
                continue;
            };
            self.hull_unlocks
                .entry(tech_id.clone())
                .or_default()
                .push(format!(
                    "{} ({})",
                    hull.display_name,
                    hull.class.display_name()
                ));
        }

        for hulls in self.hull_unlocks.values_mut() {
            hulls.sort();
        }
    }
}

/// Data file format for technologies
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TechnologiesFile {
    technologies: Vec<Technology>,
    components: Vec<ComponentDefinition>,
}

/// System to load technologies from data file at startup
pub fn load_technologies(mut commands: Commands) {
    info!("Loading technology definitions...");

    let path = "assets/data/technologies.ron";

    match fs::read_to_string(path) {
        Ok(contents) => {
            match ron::from_str::<TechnologiesFile>(&contents) {
                Ok(data) => {
                    let tech_count = data.technologies.len();
                    let component_count = data.components.len();

                    let mut tech_data = TechnologiesData::default();
                    let mut component_to_tech = std::collections::HashMap::new();

                    // Index technologies by ID and build component mapping
                    for tech in data.technologies {
                        for comp_id in &tech.unlocks_components {
                            component_to_tech.insert(comp_id.clone(), tech.id.clone());
                        }
                        for comp_id in &tech.unlocks_engineering {
                            component_to_tech.insert(comp_id.clone(), tech.id.clone());
                        }
                        tech_data.technologies.insert(tech.id.clone(), tech);
                    }

                    // Index components by ID and backfill required_tech
                    for mut component in data.components {
                        if component.required_tech.is_empty() {
                            if let Some(tech_id) = component_to_tech.get(&component.id) {
                                component.required_tech = tech_id.clone();
                            } else {
                                warn!("Component {} has no required_tech and is not unlocked by any technology", component.id);
                            }
                        }
                        tech_data.components.insert(component.id.clone(), component);
                    }

                    info!(
                        "Loaded {} technologies and {} component definitions",
                        tech_count, component_count
                    );

                    commands.insert_resource(tech_data);
                }
                Err(e) => {
                    error!("Failed to parse technology data file: {}", e);
                    // Insert empty resource so the game doesn't crash
                    commands.insert_resource(TechnologiesData::default());
                }
            }
        }
        Err(e) => {
            warn!(
                "Technology data file not found at {}: {}. Using empty tech tree.",
                path, e
            );
            // Insert empty resource so the game doesn't crash
            commands.insert_resource(TechnologiesData::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::types::TechCategory;
    use crate::shipbuilding::data::ShipModuleDefinition;
    use crate::shipbuilding::types::ShipModuleCategory;
    use crate::shipbuilding::ShipbuildingData;

    #[test]
    fn test_technologies_data_get_tech() {
        let mut data = TechnologiesData::default();
        let tech = Technology {
            id: "test_tech".to_string(),
            name: "Test Technology".to_string(),
            category: TechCategory::Physics,
            description: "A test".to_string(),
            research_cost: 1000.0,
            prerequisites: vec![],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 1,
        };

        data.technologies.insert("test_tech".to_string(), tech);

        assert!(data.get_tech("test_tech").is_some());
        assert!(data.get_tech("nonexistent").is_none());
    }

    #[test]
    fn test_check_prerequisites() {
        let mut data = TechnologiesData::default();

        let tech1 = Technology {
            id: "tech1".to_string(),
            name: "Tech 1".to_string(),
            category: TechCategory::Physics,
            description: "First tech".to_string(),
            research_cost: 1000.0,
            prerequisites: vec![],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 1,
        };

        let tech2 = Technology {
            id: "tech2".to_string(),
            name: "Tech 2".to_string(),
            category: TechCategory::Physics,
            description: "Second tech".to_string(),
            research_cost: 2000.0,
            prerequisites: vec!["tech1".to_string()],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 2,
        };

        data.technologies.insert("tech1".to_string(), tech1);
        data.technologies.insert("tech2".to_string(), tech2);

        // tech2 requires tech1
        let unlocked = vec![];
        assert!(!data.check_prerequisites("tech2", &unlocked));

        let unlocked = vec!["tech1".to_string()];
        assert!(data.check_prerequisites("tech2", &unlocked));
    }

    #[test]
    fn test_is_module_available_for_module_tier() {
        // GRA-349 audit contract §5 child 7 — executable documentation of
        // the tier-cap rule. A module is available iff the unlocking tech's
        // tier is at least the module's tier. Production wiring of this gate
        // into merge_ship_modules_as_components is a separate PR (GRA-349 backlog).
        assert!(!TechnologiesData::is_module_available_for_module_tier(3, 5));
        assert!(TechnologiesData::is_module_available_for_module_tier(6, 6));
        assert!(TechnologiesData::is_module_available_for_module_tier(7, 6));
        assert!(TechnologiesData::is_module_available_for_module_tier(1, 1));
        assert!(!TechnologiesData::is_module_available_for_module_tier(0, 1));
        assert!(TechnologiesData::is_module_available_for_module_tier(5, 4));
        assert!(TechnologiesData::is_module_available_for_module_tier(5, 1));
    }

    fn stub_module(id: &str, tier: u8, required_tech: Option<&str>) -> ShipModuleDefinition {
        ShipModuleDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: String::new(),
            category: ShipModuleCategory::CargoStorage,
            size: "Medium".to_string(),
            tier,
            propulsion: None,
            required_tech: required_tech.map(str::to_string),
            required_component_design: Some("cargo_module".to_string()),
            power_generation_mw: 0.0,
            power_draw_mw: 0.0,
            thrust_kn: 0.0,
            isp_s: 0.0,
            dry_mass_t: 0.0,
            build_points: 30.0,
            construction_capacity_bp_per_year: 0.0,
            launch_capacity_t_per_year: 0.0,
            resource_costs: Vec::new(),
            attribute_values: Vec::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn test_merge_ship_modules_picks_lowest_tier_required_tech() {
        // GRA-46 regression: cargo_pod_medium (tier 1, basic_space_tech) and
        // cargo_pod_mk2_medium (tier 2, cargo_hold_mk2) both resolve to the
        // engineering project "cargo_module". The merge must always pick the
        // tier-1 module so cargo_module is baseline-completable.
        let mut shipbuilding = ShipbuildingData::default();
        shipbuilding.modules.insert(
            "cargo_pod_mk2_medium".to_string(),
            stub_module("cargo_pod_mk2_medium", 2, Some("cargo_hold_mk2")),
        );
        shipbuilding.modules.insert(
            "cargo_pod_medium".to_string(),
            stub_module("cargo_pod_medium", 1, Some("basic_space_tech")),
        );

        let mut data = TechnologiesData::default();
        data.merge_ship_modules_as_components(&shipbuilding);

        let cargo_module = data
            .get_component("cargo_module")
            .expect("cargo_module should be merged");
        assert_eq!(
            cargo_module.required_tech, "basic_space_tech",
            "merge must pick the tier-1 module's required_tech regardless of insertion order"
        );
    }

    #[test]
    fn test_merge_ship_modules_survives_reverse_insertion_order() {
        // Insert the tier-3 module first to exercise the determinism guarantee —
        // before GRA-46 the merge would have kept tier 3 (cargo_hold_mk3).
        let mut shipbuilding = ShipbuildingData::default();
        shipbuilding.modules.insert(
            "cargo_pod_mk3_medium".to_string(),
            stub_module("cargo_pod_mk3_medium", 3, Some("cargo_hold_mk3")),
        );
        shipbuilding.modules.insert(
            "cargo_bay_mk2_large".to_string(),
            stub_module("cargo_bay_mk2_large", 2, Some("cargo_hold_mk2")),
        );
        shipbuilding.modules.insert(
            "cargo_pod_medium".to_string(),
            stub_module("cargo_pod_medium", 1, Some("basic_space_tech")),
        );

        let mut data = TechnologiesData::default();
        data.merge_ship_modules_as_components(&shipbuilding);

        let cargo_module = data.get_component("cargo_module").unwrap();
        assert_eq!(cargo_module.required_tech, "basic_space_tech");
    }
}
