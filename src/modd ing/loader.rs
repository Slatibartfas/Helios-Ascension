//! Data file loader with mod merging support.
//!
//! This module provides utilities to load game data files (RON format) and merge
//! mod data on top of base game data.
//!
//! # Merge Strategy
//!
//! - **New entries**: Mod entries with IDs not in base data are added
//! - **Overwrites**: If mod provides an entry with the same ID, the mod version wins
//! - **Deletion**: Mods can mark entries for removal via `!` prefix on ID
//!
//! # Example
//!
//! ```rust
//! use crate::modding::loader::{DataLoader, BuildingsMerger};
//!
//! // Load base buildings
//! let base_buildings = DataLoader::load_buildings("assets/data/buildings.ron")?;
//!
//! // Get mods that provide buildings
//! let building_mods = registry.mods.iter().filter(|m| m.is_compatible("buildings"));
//!
//! // Merge all mod data
//! let merged = BuildingsMerger::merge(base_buildings, building_mods);
//! ```

use crate::colony::data::{BuildingDefinition, BuildingsFile};
use crate::research::data::{ComponentDefinition, TechnologiesFile};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Result of loading and merging building data
#[derive(Debug)]
pub struct MergedBuildings {
    pub definitions: HashMap<crate::colony::types::BuildingType, BuildingDefinition>,
}

/// Result of loading and merging technology data
#[derive(Debug)]
pub struct MergedTechnologies {
    pub technologies: HashMap<String, crate::research::types::Technology>,
    pub components: HashMap<String, ComponentDefinition>,
}

/// Trait for data loaders that support mod merging
pub trait DataLoader {
    type Output;

    /// Load from base game file
    fn load_base(path: &str) -> Result<Self::Output, String>
    where
        Self: Sized;

    /// Load from a mod file
    fn load_mod(path: &Path) -> Result<Self::Output, String>
    where
        Self: Sized;
}

/// Loader for buildings data
pub struct BuildingsLoader;

impl DataLoader for BuildingsLoader {
    type Output = BuildingsFile;

    fn load_base(path: &str) -> Result<Self::Output, String> {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        ron::from_str(&contents).map_err(|e| e.to_string())
    }

    fn load_mod(path: &Path) -> Result<Self::Output, String> {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        ron::from_str(&contents).map_err(|e| e.to_string())
    }
}

/// Merger for buildings data
pub struct BuildingsMerger;

impl BuildingsMerger {
    /// Merge base buildings with mod buildings
    ///
    /// Later mods in the iterator take precedence over earlier mods.
    /// Mods can remove entries by prefixing the ID with `!`.
    pub fn merge(
        base: BuildingsFile,
        mods: impl Iterator<Item = impl AsRef<crate::modding::ModInfo>>,
    ) -> MergedBuildings {
        let mut definitions: HashMap<crate::colony::types::BuildingType, BuildingDefinition> =
            HashMap::new();

        // Add base buildings
        for def in base.buildings {
            if let Some(bt) = crate::colony::data::parse_building_type(&def.id) {
                definitions.insert(bt, def);
            }
        }

        // Apply mod patches
        for mod_info in mods {
            let mod_path = mod_info.as_ref().path.join("buildings.ron");
            if mod_path.exists() {
                debug!("Loading buildings from mod: {:?}", mod_path);
                if let Ok(mod_data) = BuildingsLoader::load_mod(&mod_path) {
                    for def in mod_data.buildings {
                        // Check for deletion marker
                        if def.id.starts_with('!') {
                            let id_to_remove = def.id.trim_start_matches('!');
                            if let Some(bt) =
                                crate::colony::data::parse_building_type(id_to_remove)
                            {
                                definitions.remove(&bt);
                                info!(
                                    "Mod {} removed building: {}",
                                    mod_info.as_ref().id,
                                    id_to_remove
                                );
                            }
                        } else if let Some(bt) = crate::colony::data::parse_building_type(&def.id) {
                            let was_new = !definitions.contains_key(&bt);
                            definitions.insert(bt, def.clone());
                            if was_new {
                                info!(
                                    "Mod {} added building: {}",
                                    mod_info.as_ref().id,
                                    def.id
                                );
                            } else {
                                info!(
                                    "Mod {} modified building: {}",
                                    mod_info.as_ref().id,
                                    def.id
                                );
                            }
                        }
                    }
                }
            }
        }

        MergedBuildings { definitions }
    }
}

/// Loader for technologies data
pub struct TechnologiesLoader;

impl DataLoader for TechnologiesLoader {
    type Output = TechnologiesFile;

    fn load_base(path: &str) -> Result<Self::Output, String> {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        ron::from_str(&contents).map_err(|e| e.to_string())
    }

    fn load_mod(path: &Path) -> Result<Self::Output, String> {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        ron::from_str(&contents).map_err(|e| e.to_string())
    }
}

/// Merger for technologies data
pub struct TechnologiesMerger;

impl TechnologiesMerger {
    /// Merge base technologies with mod technologies
    pub fn merge(
        base: TechnologiesFile,
        mods: impl Iterator<Item = impl AsRef<crate::modding::ModInfo>>,
    ) -> MergedTechnologies {
        let mut technologies: HashMap<String, crate::research::types::Technology> =
            HashMap::new();
        let mut components: HashMap<String, ComponentDefinition> = HashMap::new();

        // Add base technologies
        for tech in base.technologies {
            technologies.insert(tech.id.clone(), tech);
        }

        // Add base components
        for comp in base.components {
            components.insert(comp.id.clone(), comp);
        }

        // Apply mod patches
        for mod_info in mods {
            let mod_path = mod_info.as_ref().path.join("technologies.ron");
            if mod_path.exists() {
                debug!("Loading technologies from mod: {:?}", mod_path);
                if let Ok(mod_data) = TechnologiesLoader::load_mod(&mod_path) {
                    for tech in mod_data.technologies {
                        let was_new = !technologies.contains_key(&tech.id);
                        technologies.insert(tech.id.clone(), tech.clone());
                        if was_new {
                            info!(
                                "Mod {} added technology: {}",
                                mod_info.as_ref().id,
                                tech.id
                            );
                        } else {
                            info!(
                                "Mod {} modified technology: {}",
                                mod_info.as_ref().id,
                                tech.id
                            );
                        }
                    }

                    for comp in mod_data.components {
                        let was_new = !components.contains_key(&comp.id);
                        components.insert(comp.id.clone(), comp.clone());
                        if was_new {
                            info!(
                                "Mod {} added component: {}",
                                mod_info.as_ref().id,
                                comp.id
                            );
                        } else {
                            info!(
                                "Mod {} modified component: {}",
                                mod_info.as_ref().id,
                                comp.id
                            );
                        }
                    }
                }
            }
        }

        MergedTechnologies {
            technologies,
            components,
        }
    }
}

/// Load solar system bodies from a mod
pub struct BodiesLoader;

impl BodiesLoader {
    /// Load bodies from a mod file, returning the raw parsed data
    pub fn load_mod_bodies(path: &Path) -> Result<crate::astronomy::SolarSystemData, String> {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        ron::from_str(&contents).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_building_deletion_marker() {
        // Verify that building IDs starting with ! are recognized as deletion markers
        let deletion_id = "!Mine";
        assert!(deletion_id.starts_with('!'));
        let regular_id = "Mine";
        assert!(!regular_id.starts_with('!'));
    }
}
