use serde::{Deserialize, Serialize};
use std::fmt;

/// Resource types in the Helios Ascension economy
/// Categorized by their geological and industrial properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    // Volatiles - Found beyond the frost line (>2.5 AU)
    Water,
    Hydrogen,
    Ammonia,
    Methane,
    
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
    
    // Noble gases - Fusion fuel
    Helium3,
    
    // Fissile materials - Rare, inner solar system
    Uranium,
    Thorium,
    
    // Precious metals - High value, rare
    Gold,
    Silver,
    Platinum,
    
    // Specialty materials - Advanced technology
    Copper,
    RareEarths,
}

impl ResourceType {
    /// Returns all resource types in a stable order
    pub fn all() -> &'static [ResourceType] {
        use ResourceType::*;
        &[
            Water, Hydrogen, Ammonia, Methane,
            Nitrogen, Oxygen, CarbonDioxide, Argon,
            Iron, Aluminum, Titanium, Silicates,
            Helium3,
            Uranium, Thorium,
            Gold, Silver, Platinum,
            Copper, RareEarths,
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
        )
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
        )
    }

    /// Returns true if this is a noble gas
    pub fn is_noble_gas(&self) -> bool {
        matches!(self, ResourceType::Helium3)
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

    /// Returns true if this is a specialty material
    pub fn is_specialty(&self) -> bool {
        matches!(self, ResourceType::Copper | ResourceType::RareEarths)
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
                | ResourceType::Uranium
        )
    }

    /// Returns the category name for UI grouping
    pub fn category(&self) -> &'static str {
        if self.is_volatile() {
            "Volatiles"
        } else if self.is_atmospheric_gas() {
            "Atmospheric Gases"
        } else if self.is_construction() {
            "Construction"
        } else if self.is_noble_gas() {
            "Fusion Fuel"
        } else if self.is_fissile() {
            "Fissiles"
        } else if self.is_precious_metal() {
            "Precious Metals"
        } else if self.is_specialty() {
            "Specialty"
        } else {
            "Other"
        }
    }

    /// Returns all resources by category
    pub fn by_category() -> Vec<(&'static str, Vec<ResourceType>)> {
        vec![
            ("Volatiles", vec![
                ResourceType::Water,
                ResourceType::Hydrogen,
                ResourceType::Ammonia,
                ResourceType::Methane,
            ]),
            ("Atmospheric Gases", vec![
                ResourceType::Nitrogen,
                ResourceType::Oxygen,
                ResourceType::CarbonDioxide,
                ResourceType::Argon,
            ]),
            ("Construction", vec![
                ResourceType::Iron,
                ResourceType::Aluminum,
                ResourceType::Titanium,
                ResourceType::Silicates,
            ]),
            ("Fusion Fuel", vec![
                ResourceType::Helium3,
            ]),
            ("Fissiles", vec![
                ResourceType::Uranium,
                ResourceType::Thorium,
            ]),
            ("Precious Metals", vec![
                ResourceType::Gold,
                ResourceType::Silver,
                ResourceType::Platinum,
            ]),
            ("Specialty", vec![
                ResourceType::Copper,
                ResourceType::RareEarths,
            ]),
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
        assert_eq!(all.len(), 20, "Should have exactly 20 resource types");
    }

    #[test]
    fn test_resource_categorization() {
        assert!(ResourceType::Water.is_volatile());
        assert!(ResourceType::Nitrogen.is_atmospheric_gas());
        assert!(ResourceType::Oxygen.is_atmospheric_gas());
        assert!(ResourceType::Iron.is_construction());
        assert!(ResourceType::Helium3.is_noble_gas());
        assert!(ResourceType::Uranium.is_fissile());
        assert!(ResourceType::Gold.is_precious_metal());
        assert!(ResourceType::Copper.is_specialty());
    }

    #[test]
    fn test_critical_resources() {
        let critical_count = ResourceType::all()
            .iter()
            .filter(|r| r.is_critical())
            .count();
        assert_eq!(critical_count, 5, "Should have exactly 5 critical resources");
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
        assert_eq!(ResourceType::Water.category(), "Volatiles");
        assert_eq!(ResourceType::Nitrogen.category(), "Atmospheric Gases");
        assert_eq!(ResourceType::Iron.category(), "Construction");
        assert_eq!(ResourceType::Helium3.category(), "Fusion Fuel");
        assert_eq!(ResourceType::Uranium.category(), "Fissiles");
        assert_eq!(ResourceType::Gold.category(), "Precious Metals");
        assert_eq!(ResourceType::Copper.category(), "Specialty");
    }

    #[test]
    fn test_by_category() {
        let categories = ResourceType::by_category();
        
        // Should have 7 categories
        assert_eq!(categories.len(), 7);
        
        // Check category names
        assert_eq!(categories[0].0, "Volatiles");
        assert_eq!(categories[1].0, "Atmospheric Gases");
        assert_eq!(categories[2].0, "Construction");
        assert_eq!(categories[3].0, "Fusion Fuel");
        assert_eq!(categories[4].0, "Fissiles");
        assert_eq!(categories[5].0, "Precious Metals");
        assert_eq!(categories[6].0, "Specialty");
        
        // Check total resources (should be all 20)
        let total_resources: usize = categories.iter().map(|(_, resources)| resources.len()).sum();
        assert_eq!(total_resources, 20);
    }
}