use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::types::{ModifierType, TechCategory, TechnologyId};

/// Component for entities that generate research points
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct ResearchBuilding {
    /// Research points generated per second
    pub points_per_second: f64,
}

/// Component for entities that generate engineering points
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringFacility {
    /// Engineering points generated per second
    pub points_per_second: f64,
}

/// Resource tracking research team capacity (placeholder for full team system)
#[derive(Resource, Debug, Clone)]
pub struct ResearchTeamCapacity {
    /// Maximum number of concurrent research projects
    pub max_research_teams: usize,
    /// Maximum number of concurrent engineering projects
    pub max_engineering_teams: usize,
}

impl Default for ResearchTeamCapacity {
    fn default() -> Self {
        Self {
            max_research_teams: 3,
            max_engineering_teams: 2,
        }
    }
}

/// Component for an active research project
#[derive(Component, Debug, Clone)]
pub struct ResearchProject {
    /// Technology being researched
    pub tech_id: TechnologyId,
    /// Research points accumulated
    pub progress: f64,
    /// Research points required to complete
    pub required_points: f64,
    /// The research team working on this project
    pub team_id: Entity,
    /// Fraction of total RP income allocated to this project (0.0 to 1.0)
    pub rp_allocation_percent: f64,
    /// Whether this project is actively receiving RP (false = paused/stopped)
    pub active: bool,
}

impl ResearchProject {
    /// Create a new research project
    pub fn new(tech_id: TechnologyId, required_points: f64, team_id: Entity) -> Self {
        Self {
            tech_id,
            progress: 0.0,
            required_points,
            team_id,
            rp_allocation_percent: 1.0,
            active: true,
        }
    }

    /// Get progress percentage (0.0 to 1.0)
    pub fn progress_percent(&self) -> f32 {
        if self.required_points <= 0.0 {
            return 1.0;
        }
        (self.progress / self.required_points).min(1.0) as f32
    }

    /// Check if project is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= self.required_points
    }
}

/// Component for an active engineering project
#[derive(Component, Debug, Clone)]
pub struct EngineeringProject {
    /// Component design being engineered
    pub component_id: String,
    /// Engineering points accumulated
    pub progress: f64,
    /// Engineering points required to complete
    pub required_points: f64,
    /// The engineering team working on this project
    pub team_id: Entity,
}

impl EngineeringProject {
    /// Create a new engineering project
    pub fn new(component_id: String, required_points: f64, team_id: Entity) -> Self {
        Self {
            component_id,
            progress: 0.0,
            required_points,
            team_id,
        }
    }

    /// Get progress percentage (0.0 to 1.0)
    pub fn progress_percent(&self) -> f32 {
        if self.required_points <= 0.0 {
            return 1.0;
        }
        (self.progress / self.required_points).min(1.0) as f32
    }

    /// Check if project is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= self.required_points
    }
}

/// Component for a research or engineering team
#[derive(Component, Debug, Clone)]
pub struct ResearchTeam {
    /// Team name
    pub name: String,
    /// Lead scientist/engineer name (character)
    pub lead_character: String,
    /// Specialty category (provides bonus to this category)
    pub specialty: Option<TechCategory>,
    /// Efficiency multiplier (1.0 = normal, higher = faster)
    pub efficiency: f32,
    /// Whether this is a research team (true) or engineering team (false)
    pub is_research: bool,
}

impl ResearchTeam {
    /// Create a new research team
    pub fn new_research(
        name: String,
        lead_character: String,
        specialty: Option<TechCategory>,
    ) -> Self {
        Self {
            name,
            lead_character,
            specialty,
            efficiency: 1.0,
            is_research: true,
        }
    }

    /// Create a new engineering team
    pub fn new_engineering(
        name: String,
        lead_character: String,
        specialty: Option<TechCategory>,
    ) -> Self {
        Self {
            name,
            lead_character,
            specialty,
            efficiency: 1.0,
            is_research: false,
        }
    }

    /// Get efficiency for a specific category
    pub fn category_efficiency(&self, category: TechCategory) -> f32 {
        if let Some(spec) = self.specialty {
            if spec == category {
                return self.efficiency * 1.2; // 20% bonus for specialty
            }
        }
        self.efficiency
    }
}

/// Component for a completed component design
#[derive(Component, Debug, Clone)]
pub struct ComponentDesign {
    /// Component identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
}

/// Component that stores active technology modifiers
#[derive(Component, Debug, Clone)]
pub struct TechModifier {
    /// Type of modifier
    pub modifier_type: ModifierType,
    /// Value of the modifier
    pub value: f64,
    /// Technology that granted this modifier
    pub source_tech: TechnologyId,
}

impl TechModifier {
    /// Create a new tech modifier
    pub fn new(modifier_type: ModifierType, value: f64, source_tech: TechnologyId) -> Self {
        Self {
            modifier_type,
            value,
            source_tech,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_research_team_capacity_default() {
        let cap = ResearchTeamCapacity::default();
        assert_eq!(cap.max_research_teams, 3);
        assert_eq!(cap.max_engineering_teams, 2);
    }

    #[test]
    fn test_research_project_progress() {
        let team = Entity::from_raw(1);
        let mut project = ResearchProject::new("test_tech".to_string(), 1000.0, team);
        assert_eq!(project.progress_percent(), 0.0);
        assert!(!project.is_complete());
        assert!(project.active);
        assert_eq!(project.rp_allocation_percent, 1.0);

        project.progress = 500.0;
        assert_eq!(project.progress_percent(), 0.5);
        assert!(!project.is_complete());

        project.progress = 1000.0;
        assert_eq!(project.progress_percent(), 1.0);
        assert!(project.is_complete());
    }

    #[test]
    fn test_engineering_project_progress() {
        let team = Entity::from_raw(1);
        let mut project = EngineeringProject::new("test_component".to_string(), 500.0, team);
        assert_eq!(project.progress_percent(), 0.0);

        project.progress = 250.0;
        assert_eq!(project.progress_percent(), 0.5);

        project.progress = 500.0;
        assert!(project.is_complete());
    }

    #[test]
    fn test_research_team_specialty_bonus() {
        let team = ResearchTeam::new_research(
            "Alpha Team".to_string(),
            "Dr. Smith".to_string(),
            Some(TechCategory::Physics),
        );

        // Should get bonus for specialty
        assert!((team.category_efficiency(TechCategory::Physics) - 1.2).abs() < 0.001);

        // Should not get bonus for other categories
        assert!((team.category_efficiency(TechCategory::Biology) - 1.0).abs() < 0.001);
    }
}
