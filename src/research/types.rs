use serde::{Deserialize, Serialize};

/// Unique identifier for a technology
pub type TechnologyId = String;

/// Technology categories for organization in the tech tree
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TechCategory {
    Electronics,
    Military,
    SpaceTechnology,
    Biology,
    Physics,
    Energy,
    Sociology,
    Construction,
    Propulsion,
    Materials,
    Sensors,
    Weapons,
    DefensiveSystems,
    LifeSupport,
    Industry,
}

impl TechCategory {
    /// Get display name for the category
    pub fn display_name(&self) -> &'static str {
        match self {
            TechCategory::Electronics => "Electronics",
            TechCategory::Military => "Military",
            TechCategory::SpaceTechnology => "Space Technology",
            TechCategory::Biology => "Biology",
            TechCategory::Physics => "Physics",
            TechCategory::Energy => "Energy",
            TechCategory::Sociology => "Sociology",
            TechCategory::Construction => "Construction",
            TechCategory::Propulsion => "Propulsion",
            TechCategory::Materials => "Materials",
            TechCategory::Sensors => "Sensors",
            TechCategory::Weapons => "Weapons",
            TechCategory::DefensiveSystems => "Defensive Systems",
            TechCategory::LifeSupport => "Life Support",
            TechCategory::Industry => "Industry",
        }
    }

    /// Get icon for the category
    pub fn icon(&self) -> &'static str {
        match self {
            TechCategory::Electronics => "💻",
            TechCategory::Military => "⚔️",
            TechCategory::SpaceTechnology => "🛰️",
            TechCategory::Biology => "🧬",
            TechCategory::Physics => "⚛️",
            TechCategory::Energy => "⚡",
            TechCategory::Sociology => "👥",
            TechCategory::Construction => "🏗️",
            TechCategory::Propulsion => "🚀",
            TechCategory::Materials => "🔧",
            TechCategory::Sensors => "📡",
            TechCategory::Weapons => "💣",
            TechCategory::DefensiveSystems => "🛡️",
            TechCategory::LifeSupport => "🌱",
            TechCategory::Industry => "🏭",
        }
    }

    /// Get all categories
    pub fn all() -> &'static [TechCategory] {
        &[
            TechCategory::Electronics,
            TechCategory::Military,
            TechCategory::SpaceTechnology,
            TechCategory::Biology,
            TechCategory::Physics,
            TechCategory::Energy,
            TechCategory::Sociology,
            TechCategory::Construction,
            TechCategory::Propulsion,
            TechCategory::Materials,
            TechCategory::Sensors,
            TechCategory::Weapons,
            TechCategory::DefensiveSystems,
            TechCategory::LifeSupport,
            TechCategory::Industry,
        ]
    }
}

/// A technology in the tech tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Technology {
    /// Unique identifier
    pub id: TechnologyId,
    /// Display name
    pub name: String,
    /// Category
    pub category: TechCategory,
    /// Description
    pub description: String,
    /// Research points required
    pub research_cost: f64,
    /// Technologies that must be researched first
    pub prerequisites: Vec<TechnologyId>,
    /// Component designs unlocked by this technology
    pub unlocks_components: Vec<String>,
    /// Engineering projects unlocked by this technology
    pub unlocks_engineering: Vec<String>,
    /// Modifiers applied when this tech is researched
    pub modifiers: Vec<TechModifierDef>,
    /// Tier/level of the technology (for UI organization)
    pub tier: u32,
}

/// Definition of a technology modifier (from data file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechModifierDef {
    /// Type of modifier
    pub modifier_type: ModifierType,
    /// Value to apply (meaning depends on type)
    pub value: f64,
}

/// Type of modifier a technology can provide
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModifierType {
    /// Increase research point generation (%)
    ResearchSpeed,
    /// Increase engineering point generation (%)
    EngineeringSpeed,
    /// Reduce construction cost (%)
    ConstructionCost,
    /// Reduce research cost for specific category (%)
    CategoryResearchBonus(TechCategory),
    /// Increase mining output (%)
    MiningEfficiency,
    /// Increase power generation (%)
    PowerGeneration,
    /// Reduce ship maintenance cost (%)
    ShipMaintenance,
    /// Increase population growth rate (%)
    PopulationGrowth,
    /// Unlock new game mechanics
    UnlockMechanic(String),
}

impl ModifierType {
    /// Get display name for the modifier type
    pub fn display_name(&self) -> String {
        match self {
            ModifierType::ResearchSpeed => "Research Speed".to_string(),
            ModifierType::EngineeringSpeed => "Engineering Speed".to_string(),
            ModifierType::ConstructionCost => "Construction Cost".to_string(),
            ModifierType::CategoryResearchBonus(cat) => {
                format!("{} Research Bonus", cat.display_name())
            }
            ModifierType::MiningEfficiency => "Mining Efficiency".to_string(),
            ModifierType::PowerGeneration => "Power Generation".to_string(),
            ModifierType::ShipMaintenance => "Ship Maintenance Cost".to_string(),
            ModifierType::PopulationGrowth => "Population Growth".to_string(),
            ModifierType::UnlockMechanic(name) => format!("Unlock: {}", name),
        }
    }
}

/// Component design that requires engineering after research
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDefinition {
    /// Unique identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Engineering points required
    pub engineering_cost: f64,
    /// Technology that unlocks this component
    pub required_tech: TechnologyId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tech_category_display() {
        assert_eq!(TechCategory::Electronics.display_name(), "Electronics");
        assert_eq!(
            TechCategory::SpaceTechnology.display_name(),
            "Space Technology"
        );
    }

    #[test]
    fn test_tech_category_all() {
        let all = TechCategory::all();
        assert_eq!(all.len(), 15);
    }

    #[test]
    fn test_modifier_type_display() {
        let mod_type = ModifierType::ResearchSpeed;
        assert_eq!(mod_type.display_name(), "Research Speed");

        let cat_bonus = ModifierType::CategoryResearchBonus(TechCategory::Physics);
        assert_eq!(cat_bonus.display_name(), "Physics Research Bonus");
    }
}
