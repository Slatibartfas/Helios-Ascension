//! Player-facing settings that span multiple UI panels.
//!
//! The game has no save/load layer yet, so "persistence" is currently
//! in-session only — the resource lives for the lifetime of the `App`.
//! When a save system is added, the existing fields will ride along in
//! the save payload without any further code changes.

use bevy::prelude::*;

/// Top-level player settings.
#[derive(Resource, Debug, Clone)]
pub struct Settings {
    /// When `true` (default), in-transit freighter fleets appear in the
    /// fleets list and on the system map. When `false`, freighter fleets
    /// are filtered out of both the list and the trajectory gizmo so the
    /// player can read the map for combat / colony planning without
    /// civilian auto-freight traffic (GRA-37.a / GRA-41).
    pub show_freighters_in_transit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_freighters_in_transit: true,
        }
    }
}
