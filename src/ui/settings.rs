//! Player-facing settings that span multiple UI panels.
//!
//! The game has no save/load layer yet, so "persistence" is currently
//! in-session only — the resource lives for the lifetime of the `App`.
//! When a save system is added, the existing fields will ride along in
//! the save payload without any further code changes.

use bevy::prelude::*;

/// Per-class filter for the system-map trajectory overlay. Used by
/// [`Settings::show_all_fleet_trajectories`] to control which fleet
/// classes get their amber arc drawn when the overlay is enabled.
/// (GRA-154 M-7.)
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrajectoryClassFilter {
    pub freighter: bool,
    pub combat: bool,
    pub civilian: bool,
}

impl Default for TrajectoryClassFilter {
    fn default() -> Self {
        Self {
            freighter: true,
            combat: true,
            // Civilian = everything not a freighter or combat ship (colony
            // ships, science ships, etc.). Default on so the overlay shows
            // the full traffic picture, consistent with the legacy default
            // that always drew the selected fleet's trajectory.
            civilian: true,
        }
    }
}

/// Top-level player settings.
#[derive(Resource, Debug, Clone)]
pub struct Settings {
    /// When `true` (default), in-transit freighter fleets appear in the
    /// fleets list and on the system map. When `false`, freighter fleets
    /// are filtered out of both the list and the trajectory gizmo so the
    /// player can read the map for combat / colony planning without
    /// civilian auto-freight traffic (GRA-37.a / GRA-41).
    pub show_freighters_in_transit: bool,

    /// When `true`, every in-transit fleet (subject to `trajectory_class_filter`)
    /// gets its amber arc drawn in System view, not just the selected fleet.
    /// `false` (default) preserves the legacy "selected fleet only" behavior.
    /// (GRA-154 M-7.)
    pub show_all_fleet_trajectories: bool,

    /// Per-class toggle for the trajectory overlay. Has no effect when
    /// `show_all_fleet_trajectories` is `false`. (GRA-154 M-7.)
    pub trajectory_class_filter: TrajectoryClassFilter,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_freighters_in_transit: true,
            show_all_fleet_trajectories: false,
            trajectory_class_filter: TrajectoryClassFilter::default(),
        }
    }
}
