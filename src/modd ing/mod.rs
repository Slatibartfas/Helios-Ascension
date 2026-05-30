//! Modding infrastructure for Helios Ascension.
//!
//! This module provides the core modding API:
//! - Mod discovery and loading from `mods/` directory
//! - Merging mod data over base game data
//! - Event hooks for game events
//! - Mod metadata and load ordering
//!
//! # Mod Directory Structure
//!
//! ```
//! mods/
//! ├── my_mod/
//! │   ├── mod.ron          # Required: mod metadata
//! │   ├── buildings.ron    # Optional: adds/modifies buildings
//! │   ├── technologies.ron # Optional: adds/modifies technologies
//! │   ├── bodies.ron       # Optional: adds/modifies celestial bodies
//! │   ├── hooks.rs         # Optional: Rust hook functions
//! │   └── assets/          # Optional: custom textures, audio, etc.
//! └── another_mod/
//!     └── mod.ron
//! ```
//!
//! # Mod Metadata Format (mod.ron)
//!
//! ```ron
//! (
//!     id: "my_mod",
//!     name: "My Awesome Mod",
//!     version: "1.0.0",
//!     author: "Your Name",
//!     description: "Adds X and Y to the game",
//!     load_after: [],  // List of mod IDs that must load before this mod
//!     // Supported: "buildings", "technologies", "bodies"
//!     provides: ["buildings", "technologies"],
//! )
//! ```
//!
//! # Event Hooks
//!
//! Mods can register callbacks for game events. Hooks are defined in `hooks.rs`
//! and registered via the mod's `mod.ron` or at runtime.
//!
//! Available events:
//! - `on_colony_built`: Called when a new colony is established
//! - `on_combat_end`: Called when combat concludes
//! - `on_research_complete`: Called when a technology is researched
//! - `on_resource_discovered`: Called when a new resource deposit is found
//! - `on_ship_built`: Called when a ship or station is constructed
//! - `on_year_tick`: Called at the start of each game year

pub mod hooks;
pub mod loader;
pub mod metadata;

use bevy::prelude::*;
use std::collections::HashMap;
use std::fs;

/// Resource containing all loaded mods
#[derive(Resource, Default)]
pub struct ModRegistry {
    /// Active mods in load order
    pub mods: Vec<ModInfo>,
    /// Lookup by mod ID
    pub by_id: HashMap<String, ModInfo>,
}

/// Information about a loaded mod
#[derive(Debug, Clone)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub path: std::path::PathBuf,
    pub provides: Vec<String>,
    pub load_after: Vec<String>,
}

impl ModInfo {
    pub fn is_compatible(&self, provides: &str) -> bool {
        self.provides.iter().any(|p| p == provides)
    }
}

/// Discovers all mods in the `mods/` directory.
fn discover_mods() -> Vec<ModInfo> {
    let mods_dir = std::path::Path::new("mods");
    if !mods_dir.exists() {
        debug!("No mods directory found at `mods/`");
        return vec![];
    }

    let mut discovered = Vec::new();

    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let mod_ron_path = path.join("mod.ron");
                if mod_ron_path.exists() {
                    match fs::read_to_string(&mod_ron_path) {
                        Ok(contents) => {
                            if let Ok(meta) = ron::from_str::<metadata::ModMetadata>(&contents) {
                                let info = ModInfo {
                                    id: meta.id,
                                    name: meta.name,
                                    version: meta.version,
                                    author: meta.author,
                                    description: meta.description,
                                    path,
                                    provides: meta.provides,
                                    load_after: meta.load_after,
                                };
                                debug!("Discovered mod: {} v{}", info.id, info.version);
                                discovered.push(info);
                            } else {
                                warn!("Failed to parse mod.ron in: {:?}", path);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to read mod.ron in {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }

    // Topological sort by load_after dependencies
    sort_mods_by_load_order(discovered)
}

/// Sorts mods by load order based on `load_after` dependencies.
fn sort_mods_by_load_order(mods: Vec<ModInfo>) -> Vec<ModInfo> {
    // Build dependency graph
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for mod_info in &mods {
        in_degree.insert(mod_info.id.as_str(), 0);
    }

    for mod_info in &mods {
        for dep in &mod_info.load_after {
            if in_degree.contains_key(dep.as_str()) {
                *in_degree.entry(mod_info.id.as_str()).or_insert(0) += 1;
                dependents.entry(dep.as_str()).or_default().push(mod_info.id.as_str());
            }
        }
    }

    // Kahn's algorithm
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &degree)| degree == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut sorted = Vec::new();

    while let Some(mod_id) = queue.pop() {
        if let Some(pos) = mods.iter().position(|m| m.id == mod_id) {
            sorted.push(mods[pos].clone());
        }

        if let Some(deps) = dependents.get(mod_id) {
            for &dep in deps {
                if let Some(degree) = in_degree.get_mut(dep) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push(dep);
                    }
                }
            }
        }
    }

    if sorted.len() != mods.len() {
        warn!(
            "Mod load order has cycles or missing dependencies, using original order"
        );
        return mods;
    }

    sorted
}

/// Plugin that manages mod loading
pub struct ModdingPlugin;

impl Plugin for ModdingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, Self::load_mods_system);
    }
}

impl ModdingPlugin {
    fn load_mods_system(mut commands: Commands) {
        let mods = discover_mods();

        let count = mods.len();
        if count > 0 {
            info!("Loaded {} mod(s): {:?}", count, mods.iter().map(|m| &m.id).collect::<Vec<_>>());
        }

        let mut registry = ModRegistry::default();
        for mod_info in mods {
            registry.by_id.insert(mod_info.id.clone(), mod_info.clone());
            registry.mods.push(mod_info);
        }

        commands.insert_resource(registry);

        // Register hooks after mods are loaded
        hooks::register_mod_hooks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_mods_by_load_order_empty() {
        let result = sort_mods_by_load_order(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_sort_mods_by_load_order_single() {
        let mods = vec![ModInfo {
            id: "mod1".to_string(),
            name: "Mod 1".to_string(),
            version: "1.0.0".to_string(),
            author: "Test".to_string(),
            description: "".to_string(),
            path: std::path::PathBuf::from("mods/mod1"),
            provides: vec!["buildings".to_string()],
            load_after: vec![],
        }];
        let result = sort_mods_by_load_order(mods);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "mod1");
    }

    #[test]
    fn test_sort_mods_by_load_order_dependency() {
        let mods = vec![
            ModInfo {
                id: "mod_a".to_string(),
                name: "Mod A".to_string(),
                version: "1.0.0".to_string(),
                author: "Test".to_string(),
                description: "".to_string(),
                path: std::path::PathBuf::from("mods/mod_a"),
                provides: vec!["buildings".to_string()],
                load_after: vec![],
            },
            ModInfo {
                id: "mod_b".to_string(),
                name: "Mod B".to_string(),
                version: "1.0.0".to_string(),
                author: "Test".to_string(),
                description: "".to_string(),
                path: std::path::PathBuf::from("mods/mod_b"),
                provides: vec!["buildings".to_string()],
                load_after: vec!["mod_a".to_string()],
            },
        ];
        let result = sort_mods_by_load_order(mods);
        assert_eq!(result.len(), 2);
        // mod_a should come before mod_b due to dependency
        let mod_a_pos = result.iter().position(|m| m.id == "mod_a").unwrap();
        let mod_b_pos = result.iter().position(|m| m.id == "mod_b").unwrap();
        assert!(mod_a_pos < mod_b_pos);
    }
}
