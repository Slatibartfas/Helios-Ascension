use bevy::prelude::*;

/// Selection resource to track which celestial body is currently selected
#[derive(Resource, Debug, Clone, Default)]
pub struct Selection {
    /// The currently selected entity, if any
    pub selected: Option<Entity>,
}

impl Selection {
    /// Create a new empty selection
    pub fn new() -> Self {
        Self { selected: None }
    }

    /// Select an entity
    pub fn select(&mut self, entity: Entity) {
        self.selected = Some(entity);
    }

    /// Clear the selection
    pub fn clear(&mut self) {
        self.selected = None;
    }

    /// Check if an entity is selected
    pub fn is_selected(&self, entity: Entity) -> bool {
        self.selected == Some(entity)
    }

    /// Check if anything is selected
    pub fn has_selection(&self) -> bool {
        self.selected.is_some()
    }

    /// Get the selected entity
    pub fn get(&self) -> Option<Entity> {
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_creation() {
        let selection = Selection::new();
        assert!(!selection.has_selection());
    }

    #[test]
    fn test_selection_select() {
        let mut selection = Selection::new();
        let entity = Entity::from_raw(42);
        
        selection.select(entity);
        
        assert!(selection.has_selection());
        assert!(selection.is_selected(entity));
    }

    #[test]
    fn test_selection_clear() {
        let mut selection = Selection::new();
        let entity = Entity::from_raw(42);
        
        selection.select(entity);
        selection.clear();
        
        assert!(!selection.has_selection());
    }
}
