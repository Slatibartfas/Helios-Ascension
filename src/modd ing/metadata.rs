//! Mod metadata format for Helios Ascension.
//!
//! Every mod must have a `mod.ron` file in its root directory with this structure.
//!
//! # Example mod.ron
//!
//! ```ron
//! (
//!     id: "my_awesome_mod",
//!     name: "My Awesome Mod",
//!     version: "1.0.0",
//!     author: "Your Name",
//!     description: "Adds awesome new buildings and technologies",
//!     // Mod IDs that must load before this mod (for dependency ordering)
//!     load_after: [],
//!     // What this mod provides: "buildings", "technologies", "bodies"
//!     provides: ["buildings", "technologies"],
//! )
//! ```

use serde::{Deserialize, Serialize};

/// Mod metadata parsed from `mod.ron`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModMetadata {
    /// Unique identifier for this mod (kebab-case recommended)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Semantic version string (e.g., "1.0.0")
    pub version: String,
    /// Author name(s)
    pub author: String,
    /// Short description of what the mod does
    pub description: String,
    /// List of mod IDs that must load before this mod
    /// Use this to ensure proper load order when mods depend on each other
    #[serde(default)]
    pub load_after: Vec<String>,
    /// List of data types this mod provides
    /// Valid values: "buildings", "technologies", "bodies"
    #[serde(default)]
    pub provides: Vec<String>,
}

impl Default for ModMetadata {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version: "1.0.0".to_string(),
            author: String::new(),
            description: String::new(),
            load_after: vec![],
            provides: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mod_metadata() {
        let data = r#"
            (
                id: "test_mod",
                name: "Test Mod",
                version: "0.1.0",
                author: "Tester",
                description: "A test mod",
                load_after: [],
                provides: ["buildings"],
            )
        "#;
        let meta: ModMetadata = ron::from_str(data).unwrap();
        assert_eq!(meta.id, "test_mod");
        assert_eq!(meta.name, "Test Mod");
        assert_eq!(meta.version, "0.1.0");
        assert_eq!(meta.author, "Tester");
        assert!(meta.load_after.is_empty());
        assert_eq!(meta.provides, vec!["buildings"]);
    }

    #[test]
    fn test_parse_mod_metadata_with_deps() {
        let data = r#"
            (
                id: "dependent_mod",
                name: "Dependent Mod",
                version: "1.0.0",
                author: "Tester",
                description: "A mod that depends on others",
                load_after: ["base_mod", "core_expansion"],
                provides: ["technologies"],
            )
        "#;
        let meta: ModMetadata = ron::from_str(data).unwrap();
        assert_eq!(meta.load_after, vec!["base_mod", "core_expansion"]);
        assert_eq!(meta.provides, vec!["technologies"]);
    }
}
