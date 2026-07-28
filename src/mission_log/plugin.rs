//! Mission-log plugin — registers the resources and consumer systems.
//!
//! See [`crate::mission_log`] for the module-level doc. The plugin is
//! intentionally tiny: the load-bearing work is in [`crate::mission_log::components`]
//! and [`crate::mission_log::systems`].

use bevy::prelude::*;

use crate::mission_log::components::{MissionLog, MissionLogConfig};
use crate::mission_log::systems::{
    apply_construction_events_to_mission_log, apply_milestone_events_to_mission_log,
    apply_research_events_to_mission_log, apply_survey_events_to_mission_log, MissionLogSystemSet,
};

/// Plugin that wires the mission-log data layer.
///
/// Must be added AFTER [`crate::ui::notifications::NotificationsPlugin`]
/// so the milestone consumer can read `Messages<NotificationEvent>`.
/// (Today the milestone consumer is a no-op stub; once GRA-804 lands
/// the `NotificationEvent::MilestoneReached` variant, this ordering
/// constraint becomes load-bearing.)
pub struct MissionLogPlugin;

impl Plugin for MissionLogPlugin {
    fn build(&self, app: &mut App) {
        // Register the mission-log types so the live `App`'s
        // `AppTypeRegistry` reflects the data shape. The
        // restore-path mirror lives in
        // `PersistencePlugin::build` because the restore factory
        // builds a bare `World::new()` without our plugin chain.
        app.register_type::<MissionLog>()
            .register_type::<MissionEntry>()
            .register_type::<GoalEntry>()
            .register_type::<MissionKind>()
            .register_type::<MissionSource>()
            .register_type::<MissionOutcome>()
            .register_type::<GoalStatus>()
            .register_type::<MissionLogConfig>()
            .init_resource::<MissionLog>()
            .init_resource::<MissionLogConfig>()
            .add_systems(
                Update,
                (
                    apply_survey_events_to_mission_log,
                    apply_construction_events_to_mission_log,
                    apply_research_events_to_mission_log,
                    apply_milestone_events_to_mission_log,
                )
                    .in_set(MissionLogSystemSet)
                    .chain(),
            );
    }
}
