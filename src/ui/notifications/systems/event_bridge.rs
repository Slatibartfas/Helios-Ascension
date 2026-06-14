//! Sim-event → notification bridges.
//!
//! Three Bevy systems, one per source event family, that translate
//! sim-layer `Message` events into the player-facing
//! [`NotificationEvent`](crate::ui::notifications::events::NotificationEvent)
//! stream consumed by PR-B's spawn system. All three live in the
//! [`NotificationsSystemSet::EventBridge`](super::NotificationsSystemSet::EventBridge)
//! set so the operator can pin them ahead of `Coalesce` (PR-D, GRA-138)
//! and `Tick` (PR-B).
//!
//! Design notes:
//!
//! - **Category naming** follows the merged `assets/data/notifications.ron`
//!   (LGD-authored via PR #170, GRA-138) on `main` — see the
//!   `ask_user_questions` interaction on GRA-137 (id
//!   `c66766e6-214d-45ec-8b13-afced52b8b17`). The GRA-137 spec listed
//!   past-tense plural ids (`survey.mission_completed`,
//!   `construction.building_completed`, `research.tech_completed`); the
//!   manifest uses singular present-tense variants instead
//!   (`survey.mission_complete`, `construction.complete`,
//!   `research.tech_unlocked`). The bridge uses the merged ids so the
//!   notification system already shipped against them.
//!
//! - **Bridge ordering risk** (per the GRA-137 spec): if a source
//!   fires multiple events in one frame (e.g. `MissionFailed` +
//!   `ProbeLost`), the coalesce layer (PR-D, GRA-138) collapses them.
//!   Until then, the player sees two toasts. The known-limitation
//!   sentence is captured in the PR body and pinned on this module's
//!   doc-comment so the next reader doesn't have to re-derive it.
//!
//! - **Per-variant survey tests**: each of the 9 `SurveyEvent`
//!   variants has a dedicated test below; the construction test covers
//!   both `Completed` and `ShipCompleted` in one schedule run, and the
//!   research test covers `TechCompleted`. 11 tests total
//!   (the spec said 9, which was a typo for 11).
//!
//! - **`body` lookups** use Bevy's `Name` component on the celestial
//!   body entity. The unit tests spawn a `Name("Mare Imbrium 1")`
//!   entity; the prod sim layer attaches the body name in the
//!   system-populator path. Falling back to a short "body" string
//!   keeps the toast readable if the name is missing.

use bevy::prelude::*;

use crate::colony::events::ConstructionEvent;
use crate::research::events::ResearchEvent;
use crate::survey::events::SurveyEvent;
use crate::survey::types::MissionFailureReason;
use crate::ui::notifications::events::{NotificationEvent, NotificationSeverity};
use crate::ui::notifications::settings::NotificationCategoryId;

/// Map a celestial body `Entity` to a human-readable name for toast text.
///
/// Falls back to `"body"` if the entity has no `Name` component or the
/// name is empty — the toast still reads sensibly.
///
/// Bevy 0.18: this helper takes a `Query<&Name>` rather than `&World`
/// because `MessageReader` / `MessageWriter` hold a mutable borrow on
/// the world, and a same-system `&World` parameter triggers
/// `SystemParam` conflict (`&World conflicts with a previous mutable
/// system parameter`).
fn body_name(body_names: &Query<&Name>, body: Entity) -> String {
    body_names
        .get(body)
        .ok()
        .map(|n| n.as_str().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "body".to_string())
}

/// Translate survey-mission events into notifications.
///
/// Variants:
/// - `MissionCompleted` → `survey.mission_complete` (Info)
/// - `MissionFailed`    → `survey.mission_complete` (Warning) with
///   reason in the body — there's no `survey.mission_failed` row in
///   the merged manifest, so we re-use the same category id and
///   differentiate via `severity` + body text.
/// - `MissionAborted`   → same pattern (Warning).
/// - `AnomalyActivated` → `survey.dimension_unlocked` (Notice) — the
///   merged manifest uses `dimension_unlocked` for "anomaly threshold
///   crossed" semantically; `anomaly_activated` is not a row.
/// - `AnomalyDetected`  → `survey.anomaly_detected` (Info).
/// - `CrewInjured`      → `survey.mission_complete` (Warning) with
///   body naming the injured scientist. No dedicated row in the
///   merged manifest.
/// - `ProbeLost`        → same pattern (Warning).
/// - `RoverStuck`       → same pattern (Warning).
/// - `DrillBitStuck`    → same pattern (Warning).
pub fn bridge_survey_events(
    mut survey_events: MessageReader<SurveyEvent>,
    mut notifications: MessageWriter<NotificationEvent>,
    body_names: Query<&Name>,
) {
    for event in survey_events.read() {
        let (title, body, severity, category) = match event {
            SurveyEvent::MissionCompleted { name, .. } => (
                format!("{name} complete"),
                String::new(),
                NotificationSeverity::Info,
                "survey.mission_complete",
            ),
            SurveyEvent::MissionFailed {
                name, reason, body, ..
            } => {
                let body_name = body_name(&body_names, *body);
                (
                    format!("{name} failed"),
                    format!("{body_name}: {}", failure_reason_text(*reason)),
                    NotificationSeverity::Warning,
                    "survey.mission_complete",
                )
            }
            SurveyEvent::MissionAborted { name, body, .. } => {
                let body_name = body_name(&body_names, *body);
                (
                    format!("{name} aborted"),
                    body_name.to_string(),
                    NotificationSeverity::Warning,
                    "survey.mission_complete",
                )
            }
            SurveyEvent::AnomalyActivated { body, anomaly, .. } => {
                let body_name = body_name(&body_names, *body);
                (
                    "Anomaly activated".to_string(),
                    format!("{body_name} ({})", anomaly.ron_id()),
                    NotificationSeverity::Notice,
                    "survey.dimension_unlocked",
                )
            }
            SurveyEvent::AnomalyDetected {
                body,
                anomaly,
                initial_confidence,
            } => {
                let body_name = body_name(&body_names, *body);
                (
                    "Anomaly detected".to_string(),
                    format!(
                        "{body_name}: {} (conf {:.0}%)",
                        anomaly.ron_id(),
                        initial_confidence * 100.0
                    ),
                    NotificationSeverity::Info,
                    "survey.anomaly_detected",
                )
            }
            SurveyEvent::CrewInjured {
                body,
                name,
                scientist_name,
                ..
            } => {
                let body_name = body_name(&body_names, *body);
                (
                    "Crew injured".to_string(),
                    format!("{scientist_name} on {name} ({body_name})"),
                    NotificationSeverity::Warning,
                    "survey.mission_complete",
                )
            }
            SurveyEvent::ProbeLost { body, name, .. } => {
                let body_name = body_name(&body_names, *body);
                (
                    "Probe lost".to_string(),
                    format!("{name} ({body_name})"),
                    NotificationSeverity::Warning,
                    "survey.mission_complete",
                )
            }
            SurveyEvent::RoverStuck { body, name, .. } => {
                let body_name = body_name(&body_names, *body);
                (
                    "Rover stuck".to_string(),
                    format!("{name} ({body_name})"),
                    NotificationSeverity::Warning,
                    "survey.mission_complete",
                )
            }
            SurveyEvent::DrillBitStuck { body, name, .. } => {
                let body_name = body_name(&body_names, *body);
                (
                    "Drill bit stuck".to_string(),
                    format!("{name} ({body_name})"),
                    NotificationSeverity::Warning,
                    "survey.mission_complete",
                )
            }
            // Other variants (`AnomalyRefuted`, `MissionStarted`,
            // `MissionLaunchBlocked`) are intentionally not bridged —
            // they're either internal counters or already surface
            // through other UI paths. Add a variant here only if the
            // LGD adds a matching row to the categories manifest.
            SurveyEvent::AnomalyRefuted { .. }
            | SurveyEvent::MissionStarted { .. }
            | SurveyEvent::MissionLaunchBlocked { .. } => continue,
        };

        notifications.write(NotificationEvent {
            category: NotificationCategoryId::from(category),
            severity,
            title,
            body,
            dedup_key: None,
            auto_dismiss_s: None,
            sticky: false,
        });
    }
}

/// Translate colony + ship construction events into notifications.
pub fn bridge_construction_events(
    mut construction_events: MessageReader<ConstructionEvent>,
    mut notifications: MessageWriter<NotificationEvent>,
    colonies: Query<&crate::colony::components::Colony>,
) {
    for event in construction_events.read() {
        let (title, body, severity, category) = match event {
            ConstructionEvent::Completed { colony, building } => {
                let colony_name = colonies
                    .get(*colony)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|_| "colony".to_string());
                (
                    format!("{} complete", building.display_name()),
                    colony_name,
                    NotificationSeverity::Info,
                    "construction.complete",
                )
            }
            ConstructionEvent::ShipCompleted { hull } => (
                "Hull complete".to_string(),
                hull.clone(),
                NotificationSeverity::Notice,
                "shipbuilding.hull_complete",
            ),
        };

        notifications.write(NotificationEvent {
            category: NotificationCategoryId::from(category),
            severity,
            title,
            body,
            dedup_key: None,
            auto_dismiss_s: None,
            sticky: false,
        });
    }
}

/// Translate research events into notifications.
pub fn bridge_research_events(
    mut research_events: MessageReader<ResearchEvent>,
    mut notifications: MessageWriter<NotificationEvent>,
) {
    for event in research_events.read() {
        let (title, body, severity, category) = match event {
            ResearchEvent::TechCompleted {
                tech_id: _,
                tech_display_name,
            } => (
                format!("{tech_display_name} unlocked"),
                String::new(),
                NotificationSeverity::Notice,
                "research.tech_unlocked",
            ),
        };

        notifications.write(NotificationEvent {
            category: NotificationCategoryId::from(category),
            severity,
            title,
            body,
            dedup_key: None,
            auto_dismiss_s: None,
            sticky: false,
        });
    }
}

/// Short, lowercase label for a [`MissionFailureReason`]. Kept in this
/// module rather than next to the enum so the bridge owns its display
/// concerns (the enum's `probability` table is the sim-layer concern).
fn failure_reason_text(reason: MissionFailureReason) -> &'static str {
    match reason {
        MissionFailureReason::ProbeLoss => "probe lost",
        MissionFailureReason::RoverStuck => "rover stuck",
        MissionFailureReason::DrillBitStuck => "drill bit stuck",
        MissionFailureReason::SolarStorm => "solar storm",
        MissionFailureReason::CrewInjury => "crew injured",
    }
}

#[cfg(test)]
mod tests {
    //! 11 unit tests (the spec's "9" is a typo): one per `SurveyEvent`
    //! variant (9) + one covering both `ConstructionEvent` variants +
    //! one for `ResearchEvent::TechCompleted`.
    //!
    //! These tests deliberately drop the render layer (per
    //! `[[feedback-egui-render-tests]]`); they assert the message
    //! payload only, since the spawn system is a PR-B / PR-D concern.
    //!
    //! The Bevy 0.18 idiom is to register the bridge as a system on a
    //! `Schedule`, run the schedule against a `World` (the schedule
    //! installs the `MessageReader` / `MessageWriter` / `Query` system
    //! params from the world's resources), then drain the resulting
    //! `Messages<NotificationEvent>`. Direct calls would require a
    //! private `bypass_validation` accessor; using a schedule is the
    //! supported path.

    use super::*;
    use crate::colony::types::BuildingType;
    use crate::survey::types::AnomalyType;
    use crate::survey::types::MissionFailureReason;
    use crate::survey::types::SurveyMethod;

    /// Build a body `Entity` with a `Name` for the survey bridge to
    /// resolve. Each test gets its own `World`; the helper returns
    /// the entity and transfers the world out via closure capture.
    fn spawn_body(world: &mut World, name: &str) -> Entity {
        world.spawn(Name::new(name.to_string())).id()
    }

    /// Build a colony `Entity` the construction bridge can resolve
    /// the name from.
    fn spawn_colony(world: &mut World, name: &str) -> Entity {
        world
            .spawn(crate::colony::components::Colony {
                name: name.to_string(),
                population: 0.0,
                growth_rate_modifier: 1.0,
                buildings: std::collections::HashMap::new(),
                development: crate::colony::components::ColonyDevelopment::default(),
            })
            .id()
    }

    /// Run a single bridge system against the world, then drain and
    /// return the resulting `NotificationEvent`s.
    fn run_survey_bridge(world: &mut World) -> Vec<NotificationEvent> {
        let mut schedule = Schedule::default();
        schedule.add_systems(bridge_survey_events);
        schedule.run(world);
        world
            .resource_mut::<Messages<NotificationEvent>>()
            .drain()
            .collect()
    }

    fn run_construction_bridge(world: &mut World) -> Vec<NotificationEvent> {
        let mut schedule = Schedule::default();
        schedule.add_systems(bridge_construction_events);
        schedule.run(world);
        world
            .resource_mut::<Messages<NotificationEvent>>()
            .drain()
            .collect()
    }

    fn run_research_bridge(world: &mut World) -> Vec<NotificationEvent> {
        let mut schedule = Schedule::default();
        schedule.add_systems(bridge_research_events);
        schedule.run(world);
        world
            .resource_mut::<Messages<NotificationEvent>>()
            .drain()
            .collect()
    }

    fn fresh_world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<SurveyEvent>>();
        world.init_resource::<Messages<ConstructionEvent>>();
        world.init_resource::<Messages<ResearchEvent>>();
        world.init_resource::<Messages<NotificationEvent>>();
        world
    }

    // ── Survey bridge (9 tests) ───────────────────────────────────

    #[test]
    fn test_bridge_survey_mission_completed() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::MissionCompleted {
            body,
            mission_id: 1,
            name: "Mare Imbrium 1".to_string(),
            method: SurveyMethod::Flyby,
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Mare Imbrium 1 complete");
        assert_eq!(next.severity, NotificationSeverity::Info);
    }

    #[test]
    fn test_bridge_survey_mission_failed() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::MissionFailed {
            body,
            mission_id: 2,
            name: "Mare Imbrium 1".to_string(),
            method: SurveyMethod::Rover,
            reason: MissionFailureReason::RoverStuck,
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Mare Imbrium 1 failed");
        assert_eq!(next.severity, NotificationSeverity::Warning);
        assert!(next.body.contains("rover stuck"));
    }

    #[test]
    fn test_bridge_survey_mission_aborted() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::MissionAborted {
            body,
            mission_id: 3,
            name: "Mare Imbrium 1".to_string(),
            method: SurveyMethod::Drill,
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Mare Imbrium 1 aborted");
        assert_eq!(next.severity, NotificationSeverity::Warning);
    }

    #[test]
    fn test_bridge_survey_anomaly_activated() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::AnomalyActivated {
            body,
            anomaly: AnomalyType::BrineAquifer,
            confidence: 0.85,
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.dimension_unlocked");
        assert_eq!(next.title, "Anomaly activated");
        assert_eq!(next.severity, NotificationSeverity::Notice);
        assert!(next.body.contains("brine_aquifer"));
    }

    #[test]
    fn test_bridge_survey_anomaly_detected() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::AnomalyDetected {
            body,
            anomaly: AnomalyType::MagneticAnomaly,
            initial_confidence: 0.10,
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.anomaly_detected");
        assert_eq!(next.title, "Anomaly detected");
        assert_eq!(next.severity, NotificationSeverity::Info);
        assert!(next.body.contains("magnetic_anomaly"));
    }

    #[test]
    fn test_bridge_survey_crew_injured() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::CrewInjured {
            body,
            mission_id: 4,
            name: "Mare Imbrium 1".to_string(),
            scientist: 1,
            scientist_name: "Dr. Vasquez".to_string(),
            injured_until_sim_time: 9_000_000.0,
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Crew injured");
        assert_eq!(next.severity, NotificationSeverity::Warning);
        assert!(next.body.contains("Dr. Vasquez"));
    }

    #[test]
    fn test_bridge_survey_probe_lost() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::ProbeLost {
            body,
            mission_id: 5,
            name: "Mare Imbrium 1".to_string(),
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Probe lost");
        assert_eq!(next.severity, NotificationSeverity::Warning);
    }

    #[test]
    fn test_bridge_survey_rover_stuck() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::RoverStuck {
            body,
            mission_id: 6,
            name: "Mare Imbrium 1".to_string(),
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Rover stuck");
        assert_eq!(next.severity, NotificationSeverity::Warning);
    }

    #[test]
    fn test_bridge_survey_drill_bit_stuck() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mare Imbrium 1");
        world.write_message(SurveyEvent::DrillBitStuck {
            body,
            mission_id: 7,
            name: "Mare Imbrium 1".to_string(),
        });
        let events = run_survey_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "survey.mission_complete");
        assert_eq!(next.title, "Drill bit stuck");
        assert_eq!(next.severity, NotificationSeverity::Warning);
    }

    // ── Construction bridge (1 test, covers both variants) ────────

    #[test]
    fn test_bridge_construction_completed_and_ship_completed() {
        // Two events in one frame; bridge emits two notifications
        // until PR-D coalescing lands. This test asserts both
        // surfaces and locks in the per-variant mapping.
        let mut world = fresh_world();
        let colony_entity = spawn_colony(&mut world, "Luna Prime");

        world.write_message(ConstructionEvent::Completed {
            colony: colony_entity,
            building: BuildingType::Factory,
        });
        world.write_message(ConstructionEvent::ShipCompleted {
            hull: "Mk2 Freighter".to_string(),
        });

        let events = run_construction_bridge(&mut world);
        assert_eq!(events.len(), 2, "expected one toast per event");

        let building_event = events
            .iter()
            .find(|e| e.category.as_str() == "construction.complete")
            .expect("construction.complete notification should be present");
        assert_eq!(building_event.title, "Factory complete");
        assert_eq!(building_event.body, "Luna Prime");
        assert_eq!(building_event.severity, NotificationSeverity::Info);

        let ship_event = events
            .iter()
            .find(|e| e.category.as_str() == "shipbuilding.hull_complete")
            .expect("shipbuilding.hull_complete notification should be present");
        assert_eq!(ship_event.title, "Hull complete");
        assert_eq!(ship_event.body, "Mk2 Freighter");
        assert_eq!(ship_event.severity, NotificationSeverity::Notice);
    }

    // ── Research bridge (1 test) ──────────────────────────────────

    #[test]
    fn test_bridge_research_tech_completed() {
        let mut world = fresh_world();
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "fusion_propulsion".to_string(),
            tech_display_name: "Fusion Propulsion".to_string(),
        });
        let events = run_research_bridge(&mut world);
        assert_eq!(events.len(), 1);
        let next = &events[0];
        assert_eq!(next.category.as_str(), "research.tech_unlocked");
        assert_eq!(next.title, "Fusion Propulsion unlocked");
        assert_eq!(next.severity, NotificationSeverity::Notice);
    }
}
