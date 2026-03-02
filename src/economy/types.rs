use serde::{Deserialize, Serialize};
use std::fmt;

/// Phase state of a resource deposit (ice, liquid, or vapor).
///
/// Determined by surface temperature and pressure of the host body.
/// Affects extraction difficulty, accessibility bonuses, and visual representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ResourcePhase {
    /// Solid / frozen state (default for most deposits)
    #[default]
    Solid,
    /// Liquid state — easier extraction, enables ocean formation
    Liquid,
    /// Gaseous / vapor state — atmospheric harvesting
    Vapor,
}

impl fmt::Display for ResourcePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourcePhase::Solid => write!(f, "Solid"),
            ResourcePhase::Liquid => write!(f, "Liquid"),
            ResourcePhase::Vapor => write!(f, "Vapor"),
        }
    }
}

/// Determine the phase of a volatile resource based on temperature and pressure.
///
/// Uses simplified phase boundary logic:
/// - **Water**: liquid at 0–100°C with pressure ≥ 6.1 mbar (triple point)
/// - **Methane**: liquid at −182 to −161°C
/// - **Ammonia**: liquid at −78 to −33°C
/// - **Hydrogen**: liquid at −259 to −253°C (rare, requires extreme cold)
///
/// Non-volatile resources always return `Solid`.
pub fn determine_resource_phase(
    resource: ResourceType,
    temp_celsius: f32,
    pressure_mbar: f32,
) -> ResourcePhase {
    match resource {
        ResourceType::Water => {
            // Water triple point: 0.01°C at 6.11 mbar
            // Boiling point varies with pressure, but simplified to 100°C at 1013 mbar
            if pressure_mbar < 6.1 {
                // Below triple point pressure: sublimation only (ice ↔ vapor)
                if temp_celsius > 0.0 {
                    ResourcePhase::Vapor
                } else {
                    ResourcePhase::Solid
                }
            } else if temp_celsius >= 0.0 && temp_celsius <= 100.0 + (pressure_mbar - 1013.0) * 0.03 {
                ResourcePhase::Liquid
            } else if temp_celsius > 100.0 + (pressure_mbar - 1013.0) * 0.03 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Methane => {
            // Methane: melts at −182.5°C, boils at −161.5°C (at 1 atm)
            if temp_celsius >= -182.5 && temp_celsius <= -161.5 {
                ResourcePhase::Liquid
            } else if temp_celsius > -161.5 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Ammonia => {
            // Ammonia: melts at −77.7°C, boils at −33.3°C (at 1 atm)
            if temp_celsius >= -78.0 && temp_celsius <= -33.0 {
                ResourcePhase::Liquid
            } else if temp_celsius > -33.0 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Hydrogen => {
            // Hydrogen: melts at −259°C, boils at −253°C
            if temp_celsius >= -259.0 && temp_celsius <= -253.0 {
                ResourcePhase::Liquid
            } else if temp_celsius > -253.0 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Deuterium => {
            // Deuterium: melts at −254.4°C, boils at −249.5°C
            if temp_celsius >= -254.4 && temp_celsius <= -249.5 {
                ResourcePhase::Liquid
            } else if temp_celsius > -249.5 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Sulfur => {
            // Sulfur: melts at 115°C, boils at 445°C
            if temp_celsius >= 115.0 && temp_celsius <= 445.0 {
                ResourcePhase::Liquid
            } else if temp_celsius > 445.0 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Phosphorus => {
            // White phosphorus: melts at 44°C, boils at 280°C
            if temp_celsius >= 44.0 && temp_celsius <= 280.0 {
                ResourcePhase::Liquid
            } else if temp_celsius > 280.0 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        ResourceType::Fluorine => {
            // Fluorine: melts at −220°C, boils at −188°C
            if temp_celsius >= -220.0 && temp_celsius <= -188.0 {
                ResourcePhase::Liquid
            } else if temp_celsius > -188.0 {
                ResourcePhase::Vapor
            } else {
                ResourcePhase::Solid
            }
        }
        // Non-volatile resources are always solid minerals
        _ => ResourcePhase::Solid,
    }
}

/// Resource types in the Helios Ascension economy
/// Categorized by their geological and industrial properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    // Volatiles - Found beyond the frost line (>2.5 AU)
    Water,
    Hydrogen,
    Ammonia,
    Methane,
    /// The hard limit on hydroponics and population growth
    Phosphorus,

    // Biological - Produced by colonies, consumed by population
    /// Aggregate food supply: crops, algae, cultured protein.
    /// Produced by Farms and AgriDomes; consumed per-capita.
    Food,

    // Atmospheric gases - Essential for terraforming
    Nitrogen,
    Oxygen,
    CarbonDioxide,
    Argon,

    // Construction materials - Common in inner solar system (<2.5 AU)
    Iron,
    Aluminum,
    Titanium,
    Silicates,
    /// Found in M-type asteroids; essential for non-brittle alloys
    Nickel,
    /// Kinetic weapons (railguns) and high-heat engineering
    Tungsten,
    /// Graphene/nanotubes for lightweight, ultra-strong hulls
    Carbon,
    /// Stainless steel and corrosion-resistant alloys (Fe+Cr)
    Chromium,
    /// Lightweight structural alloys (Mg-Al), sacrificial anodes
    Magnesium,

    // Fusion fuel
    Helium3,
    /// Easier fusion than He-3; the "oil" of the 22nd century
    Deuterium,

    // Fissile materials - Rare, inner solar system
    Uranium,
    Thorium,

    // Precious metals - High value, rare
    Gold,
    Silver,
    Platinum,

    // Strategic materials - Critical for advanced technology
    Copper,
    RareEarths,
    /// Battery tech and fusion reactor maintenance
    Lithium,
    /// Industrial chemistry, sulfuric acid, and battery electrolytes
    Sulfur,
    /// Li-Co-oxide cathodes, superalloys for turbopumps and reactor components
    Cobalt,
    /// FLOX oxidiser, UF₆ enrichment, semiconductor etching
    Fluorine,
    /// Manufactured plastics, lubricants, and chemical feedstocks
    Polymers,

    // Exotic materials - Late-game / post-fusion technology
    /// Produced in particle accelerators; fuel for antimatter drives (1 000 000 s Isp)
    Antimatter,
    /// Negative-energy-density matter required for warp bubbles and wormholes
    ExoticMatter,
    /// Engineered composite materials with unnatural optical/EM properties;
    /// enables cloaking, perfect lenses, and advanced shielding
    Metamaterials,
    /// Optimised computational substrate; required for Culture-level AI minds
    /// and post-singularity automation
    Computronium,
}

impl ResourceType {
    /// Returns all resource types in a stable order
    pub fn all() -> &'static [ResourceType] {
        use ResourceType::*;
        &[
            Water,
            Hydrogen,
            Ammonia,
            Methane,
            Phosphorus,
            Food,
            Nitrogen,
            Oxygen,
            CarbonDioxide,
            Argon,
            Iron,
            Aluminum,
            Titanium,
            Silicates,
            Nickel,
            Tungsten,
            Carbon,
            Chromium,
            Magnesium,
            Helium3,
            Deuterium,
            Uranium,
            Thorium,
            Gold,
            Silver,
            Platinum,
            Copper,
            RareEarths,
            Lithium,
            Sulfur,
            Cobalt,
            Fluorine,
            Polymers,
            Antimatter,
            ExoticMatter,
            Metamaterials,
            Computronium,
        ]
    }

    /// Returns true if this is a volatile resource
    pub fn is_volatile(&self) -> bool {
        matches!(
            self,
            ResourceType::Water
                | ResourceType::Hydrogen
                | ResourceType::Ammonia
                | ResourceType::Methane
                | ResourceType::Phosphorus
        )
    }

    /// Returns true if this is a biological resource (produced, not mined)
    pub fn is_biological(&self) -> bool {
        matches!(self, ResourceType::Food)
    }

    /// Returns true if this is an atmospheric gas (for terraforming)
    pub fn is_atmospheric_gas(&self) -> bool {
        matches!(
            self,
            ResourceType::Nitrogen
                | ResourceType::Oxygen
                | ResourceType::CarbonDioxide
                | ResourceType::Argon
        )
    }

    /// Returns true if this is a construction material
    pub fn is_construction(&self) -> bool {
        matches!(
            self,
            ResourceType::Iron
                | ResourceType::Aluminum
                | ResourceType::Titanium
                | ResourceType::Silicates
                | ResourceType::Nickel
                | ResourceType::Tungsten
                | ResourceType::Carbon
                | ResourceType::Chromium
                | ResourceType::Magnesium
        )
    }

    /// Returns true if this is a fusion fuel resource
    pub fn is_fusion_fuel(&self) -> bool {
        matches!(self, ResourceType::Helium3 | ResourceType::Deuterium)
    }

    /// Returns true if this is a fissile material
    pub fn is_fissile(&self) -> bool {
        matches!(self, ResourceType::Uranium | ResourceType::Thorium)
    }

    /// Returns true if this is a precious metal
    pub fn is_precious_metal(&self) -> bool {
        matches!(
            self,
            ResourceType::Gold | ResourceType::Silver | ResourceType::Platinum
        )
    }

    /// Returns true if this is a strategic material
    pub fn is_strategic(&self) -> bool {
        matches!(
            self,
            ResourceType::Copper
                | ResourceType::RareEarths
                | ResourceType::Lithium
                | ResourceType::Sulfur
                | ResourceType::Cobalt
                | ResourceType::Fluorine
                | ResourceType::Polymers
        )
    }

    /// Returns true if this is an exotic material (late-game)
    pub fn is_exotic(&self) -> bool {
        matches!(
            self,
            ResourceType::Antimatter
                | ResourceType::ExoticMatter
                | ResourceType::Metamaterials
                | ResourceType::Computronium
        )
    }

    /// Returns the display name of the resource
    pub fn display_name(&self) -> &'static str {
        match self {
            ResourceType::Water => "Water",
            ResourceType::Hydrogen => "Hydrogen",
            ResourceType::Ammonia => "Ammonia",
            ResourceType::Methane => "Methane",
            ResourceType::Nitrogen => "Nitrogen",
            ResourceType::Oxygen => "Oxygen",
            ResourceType::CarbonDioxide => "Carbon Dioxide",
            ResourceType::Argon => "Argon",
            ResourceType::Iron => "Iron",
            ResourceType::Aluminum => "Aluminum",
            ResourceType::Titanium => "Titanium",
            ResourceType::Silicates => "Silicates",
            ResourceType::Helium3 => "Helium-3",
            ResourceType::Uranium => "Uranium",
            ResourceType::Thorium => "Thorium",
            ResourceType::Gold => "Gold",
            ResourceType::Silver => "Silver",
            ResourceType::Platinum => "Platinum",
            ResourceType::Copper => "Copper",
            ResourceType::RareEarths => "Rare Earths",
            ResourceType::Phosphorus => "Phosphorus",
            ResourceType::Nickel => "Nickel",
            ResourceType::Tungsten => "Tungsten",
            ResourceType::Carbon => "Carbon",
            ResourceType::Deuterium => "Deuterium",
            ResourceType::Lithium => "Lithium",
            ResourceType::Sulfur => "Sulfur",
            ResourceType::Food => "Food",
            ResourceType::Chromium => "Chromium",
            ResourceType::Magnesium => "Magnesium",
            ResourceType::Cobalt => "Cobalt",
            ResourceType::Fluorine => "Fluorine",
            ResourceType::Polymers => "Polymers",
            ResourceType::Antimatter => "Antimatter",
            ResourceType::ExoticMatter => "Exotic Matter",
            ResourceType::Metamaterials => "Metamaterials",
            ResourceType::Computronium => "Computronium",
        }
    }

    /// Returns the short symbol for the resource
    pub fn symbol(&self) -> &'static str {
        match self {
            ResourceType::Water => "H2O",
            ResourceType::Hydrogen => "H2",
            ResourceType::Ammonia => "NH3",
            ResourceType::Methane => "CH4",
            ResourceType::Nitrogen => "N2",
            ResourceType::Oxygen => "O2",
            ResourceType::CarbonDioxide => "CO2",
            ResourceType::Argon => "Ar",
            ResourceType::Iron => "Fe",
            ResourceType::Aluminum => "Al",
            ResourceType::Titanium => "Ti",
            ResourceType::Silicates => "SiO2",
            ResourceType::Helium3 => "He3",
            ResourceType::Uranium => "U",
            ResourceType::Thorium => "Th",
            ResourceType::Gold => "Au",
            ResourceType::Silver => "Ag",
            ResourceType::Platinum => "Pt",
            ResourceType::Copper => "Cu",
            ResourceType::RareEarths => "REE",
            ResourceType::Phosphorus => "P",
            ResourceType::Nickel => "Ni",
            ResourceType::Tungsten => "W",
            ResourceType::Carbon => "C",
            ResourceType::Deuterium => "D",
            ResourceType::Lithium => "Li",
            ResourceType::Sulfur => "S",
            ResourceType::Food => "Fd",
            ResourceType::Chromium => "Cr",
            ResourceType::Magnesium => "Mg",
            ResourceType::Cobalt => "Co",
            ResourceType::Fluorine => "F",
            ResourceType::Polymers => "Py",
            ResourceType::Antimatter => "Am\u{0305}",
            ResourceType::ExoticMatter => "Xm",
            ResourceType::Metamaterials => "Mm",
            ResourceType::Computronium => "Qb",
        }
    }

    /// Returns true if this is a critical resource for display
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            ResourceType::Water
                | ResourceType::Oxygen
                | ResourceType::Iron
                | ResourceType::Helium3
                | ResourceType::Deuterium
                | ResourceType::Uranium
                | ResourceType::Food
        )
    }

    /// Returns the category name for UI grouping
    pub fn category(&self) -> &'static str {
        if self.is_biological() {
            "Biological"
        } else if self.is_volatile() {
            "Volatiles"
        } else if self.is_atmospheric_gas() {
            "Atmospheric Gases"
        } else if self.is_construction() {
            "Construction"
        } else if self.is_fusion_fuel() {
            "Fusion Fuel"
        } else if self.is_fissile() {
            "Fissiles"
        } else if self.is_precious_metal() {
            "Precious Metals"
        } else if self.is_strategic() {
            "Strategic"
        } else if self.is_exotic() {
            "Exotic"
        } else {
            "Other"
        }
    }

    /// Returns all resources by category
    pub fn by_category() -> Vec<(&'static str, Vec<ResourceType>)> {
        vec![
            (
                "Biological",
                vec![
                    ResourceType::Food,
                ],
            ),
            (
                "Volatiles",
                vec![
                    ResourceType::Water,
                    ResourceType::Hydrogen,
                    ResourceType::Ammonia,
                    ResourceType::Methane,
                    ResourceType::Phosphorus,
                ],
            ),
            (
                "Atmospheric Gases",
                vec![
                    ResourceType::Nitrogen,
                    ResourceType::Oxygen,
                    ResourceType::CarbonDioxide,
                    ResourceType::Argon,
                ],
            ),
            (
                "Construction",
                vec![
                    ResourceType::Iron,
                    ResourceType::Aluminum,
                    ResourceType::Titanium,
                    ResourceType::Silicates,
                    ResourceType::Nickel,
                    ResourceType::Tungsten,
                    ResourceType::Carbon,
                    ResourceType::Chromium,
                    ResourceType::Magnesium,
                ],
            ),
            (
                "Fusion Fuel",
                vec![ResourceType::Helium3, ResourceType::Deuterium],
            ),
            (
                "Fissiles",
                vec![ResourceType::Uranium, ResourceType::Thorium],
            ),
            (
                "Precious Metals",
                vec![
                    ResourceType::Gold,
                    ResourceType::Silver,
                    ResourceType::Platinum,
                ],
            ),
            (
                "Strategic",
                vec![
                    ResourceType::Copper,
                    ResourceType::RareEarths,
                    ResourceType::Lithium,
                    ResourceType::Sulfur,
                    ResourceType::Cobalt,
                    ResourceType::Fluorine,
                    ResourceType::Polymers,
                ],
            ),
            (
                "Exotic",
                vec![
                    ResourceType::Antimatter,
                    ResourceType::ExoticMatter,
                    ResourceType::Metamaterials,
                    ResourceType::Computronium,
                ],
            ),
        ]
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_all() {
        let all = ResourceType::all();
        assert_eq!(all.len(), 37, "Should have exactly 37 resource types");
    }

    #[test]
    fn test_resource_categorization() {
        assert!(ResourceType::Water.is_volatile());
        assert!(ResourceType::Phosphorus.is_volatile());
        assert!(ResourceType::Food.is_biological());
        assert!(ResourceType::Nitrogen.is_atmospheric_gas());
        assert!(ResourceType::Oxygen.is_atmospheric_gas());
        assert!(ResourceType::Iron.is_construction());
        assert!(ResourceType::Nickel.is_construction());
        assert!(ResourceType::Tungsten.is_construction());
        assert!(ResourceType::Carbon.is_construction());
        assert!(ResourceType::Chromium.is_construction());
        assert!(ResourceType::Magnesium.is_construction());
        assert!(ResourceType::Helium3.is_fusion_fuel());
        assert!(ResourceType::Deuterium.is_fusion_fuel());
        assert!(ResourceType::Uranium.is_fissile());
        assert!(ResourceType::Gold.is_precious_metal());
        assert!(ResourceType::Copper.is_strategic());
        assert!(ResourceType::Lithium.is_strategic());
        assert!(ResourceType::Sulfur.is_strategic());
        assert!(ResourceType::Cobalt.is_strategic());
        assert!(ResourceType::Fluorine.is_strategic());
        assert!(ResourceType::Polymers.is_strategic());
        assert!(ResourceType::Antimatter.is_exotic());
        assert!(ResourceType::ExoticMatter.is_exotic());
        assert!(ResourceType::Metamaterials.is_exotic());
        assert!(ResourceType::Computronium.is_exotic());
    }

    #[test]
    fn test_critical_resources() {
        let critical_count = ResourceType::all()
            .iter()
            .filter(|r| r.is_critical())
            .count();
        assert_eq!(
            critical_count, 7,
            "Should have exactly 7 critical resources"
        );
    }

    #[test]
    fn test_display_names() {
        assert_eq!(ResourceType::Water.display_name(), "Water");
        assert_eq!(ResourceType::Nitrogen.display_name(), "Nitrogen");
        assert_eq!(ResourceType::Helium3.display_name(), "Helium-3");
        assert_eq!(ResourceType::Gold.display_name(), "Gold");
    }

    #[test]
    fn test_symbols() {
        assert_eq!(ResourceType::Water.symbol(), "H2O");
        assert_eq!(ResourceType::Nitrogen.symbol(), "N2");
        assert_eq!(ResourceType::Iron.symbol(), "Fe");
        assert_eq!(ResourceType::Helium3.symbol(), "He3");
        assert_eq!(ResourceType::Gold.symbol(), "Au");
    }

    #[test]
    fn test_resource_category() {
        assert_eq!(ResourceType::Food.category(), "Biological");
        assert_eq!(ResourceType::Water.category(), "Volatiles");
        assert_eq!(ResourceType::Phosphorus.category(), "Volatiles");
        assert_eq!(ResourceType::Nitrogen.category(), "Atmospheric Gases");
        assert_eq!(ResourceType::Iron.category(), "Construction");
        assert_eq!(ResourceType::Nickel.category(), "Construction");
        assert_eq!(ResourceType::Tungsten.category(), "Construction");
        assert_eq!(ResourceType::Carbon.category(), "Construction");
        assert_eq!(ResourceType::Chromium.category(), "Construction");
        assert_eq!(ResourceType::Magnesium.category(), "Construction");
        assert_eq!(ResourceType::Helium3.category(), "Fusion Fuel");
        assert_eq!(ResourceType::Deuterium.category(), "Fusion Fuel");
        assert_eq!(ResourceType::Uranium.category(), "Fissiles");
        assert_eq!(ResourceType::Gold.category(), "Precious Metals");
        assert_eq!(ResourceType::Copper.category(), "Strategic");
        assert_eq!(ResourceType::Lithium.category(), "Strategic");
        assert_eq!(ResourceType::Sulfur.category(), "Strategic");
        assert_eq!(ResourceType::Cobalt.category(), "Strategic");
        assert_eq!(ResourceType::Fluorine.category(), "Strategic");
        assert_eq!(ResourceType::Polymers.category(), "Strategic");
        assert_eq!(ResourceType::Antimatter.category(), "Exotic");
        assert_eq!(ResourceType::ExoticMatter.category(), "Exotic");
    }

    #[test]
    fn test_by_category() {
        let categories = ResourceType::by_category();

        // Should have 9 categories
        assert_eq!(categories.len(), 9);

        // Check category names
        assert_eq!(categories[0].0, "Biological");
        assert_eq!(categories[1].0, "Volatiles");
        assert_eq!(categories[2].0, "Atmospheric Gases");
        assert_eq!(categories[3].0, "Construction");
        assert_eq!(categories[4].0, "Fusion Fuel");
        assert_eq!(categories[5].0, "Fissiles");
        assert_eq!(categories[6].0, "Precious Metals");
        assert_eq!(categories[7].0, "Strategic");
        assert_eq!(categories[8].0, "Exotic");

        // Check total resources (should be all 37)
        let total_resources: usize = categories
            .iter()
            .map(|(_, resources)| resources.len())
            .sum();
        assert_eq!(total_resources, 37);
    }
}
