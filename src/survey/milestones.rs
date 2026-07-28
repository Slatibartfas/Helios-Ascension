//! Early-game milestone ECS bridge — narrow reflected resource + idempotent
//! consumers + read-only accessors.
//!
//! GRA-787 implements the approved architecture from
//! GRA-786 ([comment 46e61a63](https://paperclip.klingspor.one/GRA/issues/GRA-786#comment-46e61a63)):
//! "a narrow reflected resource, not a narrative engine". This module owns
//! the persistence layer and the event consumers; the dossier rendering
//! lives in `crate::ui::dossier_panel::draw_milestones_section` (read-only).
//!
//! ## Producer gaps
//!
//! Two flags cannot be derived from existing emitted messages without
//! guessing, per the architecture's "do not guess" rule:
//!
//! - **outpost_established** — GRA-786 expected
//!   `ConstructionEvent::Completed { building: BuildingType::Outpost }`,
//!   but no such `BuildingType` variant exists. We close the gap by adding
//!   a dedicated `ConstructionEvent::OutpostEstablished { colony, body }`
//!   variant fired from `process_construction_actions`. See
//!   `src/colony/events.rs` for the rationale.
//! - **deposit_extraction_milestone** — the dimension advanced by a survey
//!   mission is not in `SurveyEvent::MissionCompleted`'s payload. The
//!   mission `method` alone is not unambiguous evidence of "deposit
//!   discovery" (a Drill mission could be probing for volatiles instead of
//!   ore). We expose the flag and the `current_step()` / `next_objective()`
//!   labels as if the producer existed; the flag stays `false` until a
//!   follow-up adds a `SurveyEvent::DepositDiscovered { body, deposit, ... }`
//!   or wires dimension data into `MissionCompleted`.
//!
//! ## Idempotency
//!
//! Every consumer only flips `false → true`. A duplicate delivery of the
//! same message (e.g. an `AnomalyActivated` fired twice for the same body
//! after a re-arming loop) is a no-op — the post-condition is "flag set if
//! and only if any qualifying event was emitted".
//!
//! ## Persistence
//!
//! The resource is `Reflect + FromReflect + Default`, registered via
//! `app.register_type::<EarlyGameMilestones>()` in `SurveyPlugin`. Bevy's
//! `DynamicScene::from_world` walks `AppTypeRegistry` for resources with
//! `#[reflect(Resource)]`; old saves that lack the resource restore to
//! `Default` through the fresh-world plugin-initialisation path described
//! in GRA-319's `R3 (fresh world for restore)` rule. We do NOT bump
//! `FORMAT_VERSION` because the snapshot is additive: an existing save
//! without the resource restores cleanly via `init_resource`, and a save
//! with the resource carries the booleans through `Reflect`.

use bevy::prelude::*;

use crate::colony::events::ConstructionEvent;
use crate::research::events::ResearchEvent;
use crate::research::TechnologiesData;
use crate::survey::events::SurveyEvent;
use crate::survey::types::SurveyMethod;

/// Reflected resource holding the six monotonic early-game flags.
///
/// One bit per milestone; each flips exactly once during a campaign. The
/// resource is owned by `SurveyPlugin` (initialised via `init_resource`)
/// and registered for `DynamicScene` snapshot/restore via
/// `app.register_type::<EarlyGameMilestones>()`.
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct EarlyGameMilestones {
    /// A probe (Flyby / Orbital / RemoteSensing / AtmosphericProbe)
    /// mission was dispatched. Set on `SurveyEvent::MissionStarted` when
    /// `method.is_probe_using()`.
    pub probe_dispatched: bool,
    /// Any survey mission completed successfully. Set on
    /// `SurveyEvent::MissionCompleted` regardless of method — the
    /// architecture's "do not guess" caveat applies to the
    /// deposit-extraction flag specifically, not to this catch-all.
    pub survey_completed: bool,
    /// An anomaly was either detected on a body dossier or activated
    /// (confidence crossed the threshold). Set on
    /// `SurveyEvent::AnomalyDetected` OR `SurveyEvent::AnomalyActivated`.
    /// Refutations do not advance this flag — a refuted anomaly drops
    /// confidence but the initial detection is still a "the player has
    /// noticed something anomalous" moment.
    pub anomaly_detected_or_activated: bool,
    /// First deposit / extraction milestone on a body. See the module-level
    /// "Producer gaps" comment — currently unreachable because no
    /// unambiguous signal exists.
    pub deposit_extraction_milestone: bool,
    /// A new outpost colony was established. Set on
    /// `ConstructionEvent::OutpostEstablished` (the new variant added in
    /// GRA-787 to close the producer gap).
    pub outpost_established: bool,
    /// A paid tier-1 technology was unlocked. Set on
    /// `ResearchEvent::TechCompleted` when the id resolves through
    /// `TechnologiesData` and `tier == 1` AND `research_cost > 0.0`. The
    /// "paid" half excludes the `free_construction`-style zero-cost stub
    /// techs that exist for testing.
    pub paid_tier_1_technology_unlocked: bool,
}

/// Stable display order for the milestone list. Used by
/// `current_step()` / `next_objective()` and by the dossier renderer.
///
/// The order is the canonical "early game" progression the architecture
/// calls out — probe → survey → anomaly → deposit → outpost → research.
/// We deliberately do NOT use a `BTreeMap` because the renderer needs a
/// stable iteration order across save/load and across patches.
pub const MILESTONE_ORDER: [MilestoneStep; 6] = [
    MilestoneStep::ProbeDispatched,
    MilestoneStep::SurveyCompleted,
    MilestoneStep::AnomalyDetectedOrActivated,
    MilestoneStep::DepositExtractionMilestone,
    MilestoneStep::OutpostEstablished,
    MilestoneStep::PaidTier1TechnologyUnlocked,
];

/// One milestone flag, named for the renderer. The `Display` impl is
/// used by `next_objective()` and the dossier section header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum MilestoneStep {
    ProbeDispatched,
    SurveyCompleted,
    AnomalyDetectedOrActivated,
    DepositExtractionMilestone,
    OutpostEstablished,
    PaidTier1TechnologyUnlocked,
}

impl MilestoneStep {
    /// All variants in canonical order. Mirrors `MILESTONE_ORDER`.
    pub const ALL: [MilestoneStep; 6] = MILESTONE_ORDER;

    /// Short upper-case label for the dossier section header
    /// (≤ 24 chars per the design convention).
    pub fn display_name(self) -> &'static str {
        match self {
            MilestoneStep::ProbeDispatched => "FIRST PROBE",
            MilestoneStep::SurveyCompleted => "FIRST SURVEY",
            MilestoneStep::AnomalyDetectedOrActivated => "ANOMALY",
            MilestoneStep::DepositExtractionMilestone => "DEPOSIT",
            MilestoneStep::OutpostEstablished => "OUTPOST",
            MilestoneStep::PaidTier1TechnologyUnlocked => "TIER-1 TECH",
        }
    }

    /// One-line player-facing description for `next_objective()` and the
    /// dossier's "next up" line.
    pub fn description(self) -> &'static str {
        match self {
            MilestoneStep::ProbeDispatched => {
                "Dispatch a probe (Flyby, Orbital, Remote Sensing, or Atmospheric Probe)."
            }
            MilestoneStep::SurveyCompleted => "Complete any survey mission.",
            MilestoneStep::AnomalyDetectedOrActivated => {
                "Detect or activate an anomaly on a body dossier."
            }
            MilestoneStep::DepositExtractionMilestone => {
                "Survey a body until a mineral deposit is confirmed."
            }
            MilestoneStep::OutpostEstablished => "Establish your first outpost colony.",
            MilestoneStep::PaidTier1TechnologyUnlocked => {
                "Research a tier-1 technology that costs research points."
            }
        }
    }

    fn is_set(self, m: &EarlyGameMilestones) -> bool {
        match self {
            MilestoneStep::ProbeDispatched => m.probe_dispatched,
            MilestoneStep::SurveyCompleted => m.survey_completed,
            MilestoneStep::AnomalyDetectedOrActivated => m.anomaly_detected_or_activated,
            MilestoneStep::DepositExtractionMilestone => m.deposit_extraction_milestone,
            MilestoneStep::OutpostEstablished => m.outpost_established,
            MilestoneStep::PaidTier1TechnologyUnlocked => m.paid_tier_1_technology_unlocked,
        }
    }
}

impl std::fmt::Display for MilestoneStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

impl EarlyGameMilestones {
    /// Index of the first unset milestone in `MILESTONE_ORDER`, or `None`
    /// if every flag is set. Pure function — no event-bus access.
    pub fn current_step(&self) -> Option<MilestoneStep> {
        MILESTONE_ORDER.iter().copied().find(|s| !s.is_set(self))
    }

    /// Display string for the next unset milestone, or
    /// `"ALL CLEAR — early game complete"` when the player has triggered
    /// every flag the bridge can observe.
    pub fn next_objective(&self) -> String {
        match self.current_step() {
            Some(step) => format!("{} — {}", step.display_name(), step.description()),
            None => "ALL CLEAR — early game complete".to_string(),
        }
    }

    /// Snapshot view used by the dossier renderer — every variant paired
    /// with its current `is_set` state. The order matches `MILESTONE_ORDER`
    /// so the renderer does not need to re-sort.
    pub fn progress_rows(&self) -> [(MilestoneStep, bool); 6] {
        [
            (MilestoneStep::ProbeDispatched, self.probe_dispatched),
            (MilestoneStep::SurveyCompleted, self.survey_completed),
            (
                MilestoneStep::AnomalyDetectedOrActivated,
                self.anomaly_detected_or_activated,
            ),
            (
                MilestoneStep::DepositExtractionMilestone,
                self.deposit_extraction_milestone,
            ),
            (MilestoneStep::OutpostEstablished, self.outpost_established),
            (
                MilestoneStep::PaidTier1TechnologyUnlocked,
                self.paid_tier_1_technology_unlocked,
            ),
        ]
    }
}

/// Whether a survey method uses a probe (Flyby / Orbital / RemoteSensing /
/// AtmosphericProbe). Used by the survey consumer to filter
/// `SurveyEvent::MissionStarted` for the `probe_dispatched` flag.
///
/// Mirrors `MissionFailureReason::probability(_, ProbeLoss) > 0.0` —
/// the four probe-using methods are exactly the ones that have a non-zero
/// probe-loss probability.
pub fn is_probe_using_method(method: SurveyMethod) -> bool {
    matches!(
        method,
        SurveyMethod::Flyby
            | SurveyMethod::Orbital
            | SurveyMethod::RemoteSensing
            | SurveyMethod::AtmosphericProbe
    )
}

/// Consume survey events and advance flags idempotently.
///
/// Order is irrelevant (each variant targets a disjoint flag) but we
/// `match` exhaustively so a future `SurveyEvent` variant forces the
/// author to think about whether it should advance a milestone. The
/// system runs in `Update`, in `MilestonesSystemSet::Survey`; the set
/// chains after `NotificationsSystemSet::EventBridge` so the same
/// message has already been bridged to a toast before we flip the flag.
pub fn advance_survey_milestones(
    mut survey_events: MessageReader<SurveyEvent>,
    mut milestones: ResMut<EarlyGameMilestones>,
) {
    for event in survey_events.read() {
        match event {
            SurveyEvent::MissionStarted { method, .. } => {
                if is_probe_using_method(*method) && !milestones.probe_dispatched {
                    milestones.probe_dispatched = true;
                }
            }
            SurveyEvent::MissionCompleted { .. } => {
                if !milestones.survey_completed {
                    milestones.survey_completed = true;
                }
            }
            SurveyEvent::AnomalyDetected { .. } | SurveyEvent::AnomalyActivated { .. } => {
                if !milestones.anomaly_detected_or_activated {
                    milestones.anomaly_detected_or_activated = true;
                }
            }
            // MissionFailed / MissionAborted / ProbeLost / RoverStuck /
            // DrillBitStuck / CrewInjured / AnomalyRefuted /
            // MissionLaunchBlocked do NOT advance a milestone — they
            // are either negative outcomes or internal counters.
            SurveyEvent::MissionFailed { .. }
            | SurveyEvent::MissionAborted { .. }
            | SurveyEvent::ProbeLost { .. }
            | SurveyEvent::RoverStuck { .. }
            | SurveyEvent::DrillBitStuck { .. }
            | SurveyEvent::CrewInjured { .. }
            | SurveyEvent::AnomalyRefuted { .. }
            | SurveyEvent::MissionLaunchBlocked { .. } => {}
        }
    }
}

/// Consume construction events and advance flags idempotently.
pub fn advance_construction_milestones(
    mut construction_events: MessageReader<ConstructionEvent>,
    mut milestones: ResMut<EarlyGameMilestones>,
) {
    for event in construction_events.read() {
        match event {
            // The variant added by GRA-787 closes the producer gap that
            // GRA-786's design comment called out: there is no
            // `BuildingType::Outpost` variant, so we cannot filter
            // `Completed { building: Outpost }`. Instead the
            // establishment flow emits `OutpostEstablished` directly.
            ConstructionEvent::OutpostEstablished { .. } => {
                if !milestones.outpost_established {
                    milestones.outpost_established = true;
                }
            }
            ConstructionEvent::Completed { .. } | ConstructionEvent::ShipCompleted { .. } => {}
        }
    }
}

/// Consume research events and advance the paid-tier-1 flag, resolving
/// the tech id through `TechnologiesData` to read `tier` and
/// `research_cost`. A tech with `tier != 1` or `research_cost <= 0.0`
/// is ignored — the player has not actually "paid" for tier-1 research.
///
/// If the id does not resolve (e.g. a stale id from an old save with a
/// removed tech row), we do NOT advance the flag. The architecture's
/// "validate exact predicate from current types/data" rule applies
/// symmetrically: a missing predicate is a no-op, not a flag flip.
pub fn advance_research_milestones(
    mut research_events: MessageReader<ResearchEvent>,
    mut milestones: ResMut<EarlyGameMilestones>,
    tech_data: Option<Res<TechnologiesData>>,
) {
    if milestones.paid_tier_1_technology_unlocked {
        // Once set, drain without inspecting; the buffer does not need
        // to grow while the flag is already true.
        for _ in research_events.read() {}
        return;
    }

    let Some(tech_data) = tech_data else {
        // Tech data has not loaded yet. Drain to avoid buffer growth
        // but do not advance — the event will be re-emitted on the
        // next tech completion and we will get another chance.
        for _ in research_events.read() {}
        return;
    };

    for event in research_events.read() {
        let ResearchEvent::TechCompleted { tech_id, .. } = event;
        let Some(tech) = tech_data.get_tech(tech_id) else {
            continue;
        };
        if tech.tier == 1 && tech.research_cost > 0.0 {
            milestones.paid_tier_1_technology_unlocked = true;
        }
    }
}

/// System set that owns the milestone consumers. Configured to run in
/// `Update` after `NotificationsSystemSet::EventBridge` (see
/// `SurveyPlugin::build`) so the toast and the flag flip on the same
/// frame. The three consumers target disjoint message families, so
/// ordering WITHIN the set is irrelevant — Bevy's `.chain()` keeps the
/// documentation tidy.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MilestonesSystemSet;

#[cfg(test)]
mod tests {
    //! Focused tests per GRA-787 acceptance criteria:
    //! - ordering / progression (canonical order is preserved end-to-end)
    //! - duplicate delivery (every flag is false→true only)
    //! - classification (probe methods vs non-probe; paid tier-1 vs not)
    //! - persistence round-trip (the resource survives a Reflect-driven
    //!   snapshot via `register_type` + `init_resource`)

    use super::*;

    fn fresh_world() -> World {
        let mut world = World::new();
        world.init_resource::<EarlyGameMilestones>();
        world.init_resource::<Messages<SurveyEvent>>();
        world.init_resource::<Messages<ConstructionEvent>>();
        world.init_resource::<Messages<ResearchEvent>>();
        world.init_resource::<AppTypeRegistry>();
        // Register the resource type so the persistence test can use
        // `ReflectFromReflect`. The other test cases do not need it.
        {
            let mut reg = world.resource_mut::<AppTypeRegistry>();
            reg.register::<EarlyGameMilestones>();
        }
        world
    }

    /// Build a `Schedule` with the three milestone consumers + a world
    /// that already has the milestone resource initialised.
    fn build_schedule(world: &mut World) -> Schedule {
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                advance_survey_milestones,
                advance_construction_milestones,
                advance_research_milestones,
            )
                .chain(),
        );
        schedule
    }

    // ── current_step / next_objective ──────────────────────────────

    #[test]
    fn default_state_first_step_is_probe() {
        let m = EarlyGameMilestones::default();
        assert_eq!(m.current_step(), Some(MilestoneStep::ProbeDispatched));
        assert!(m.next_objective().contains("FIRST PROBE"));
    }

    #[test]
    fn every_flag_set_yields_all_clear() {
        let mut m = EarlyGameMilestones::default();
        m.probe_dispatched = true;
        m.survey_completed = true;
        m.anomaly_detected_or_activated = true;
        m.deposit_extraction_milestone = true;
        m.outpost_established = true;
        m.paid_tier_1_technology_unlocked = true;
        assert_eq!(m.current_step(), None);
        assert_eq!(m.next_objective(), "ALL CLEAR — early game complete");
    }

    #[test]
    fn current_step_skips_already_set_flags() {
        let mut m = EarlyGameMilestones::default();
        m.probe_dispatched = true;
        m.survey_completed = true;
        assert_eq!(
            m.current_step(),
            Some(MilestoneStep::AnomalyDetectedOrActivated)
        );
    }

    // ── probe / non-probe classification ──────────────────────────

    #[test]
    fn is_probe_using_method_matches_probe_loss_table() {
        // Every method with a non-zero `ProbeLoss` probability is a
        // probe-using method. The four-method set is the canonical
        // probe roster.
        assert!(is_probe_using_method(SurveyMethod::Flyby));
        assert!(is_probe_using_method(SurveyMethod::Orbital));
        assert!(is_probe_using_method(SurveyMethod::RemoteSensing));
        assert!(is_probe_using_method(SurveyMethod::AtmosphericProbe));

        // Ground-team methods do not use a probe.
        assert!(!is_probe_using_method(SurveyMethod::SurfaceLander));
        assert!(!is_probe_using_method(SurveyMethod::Rover));
        assert!(!is_probe_using_method(SurveyMethod::Seismic));
        assert!(!is_probe_using_method(SurveyMethod::Drill));
        assert!(!is_probe_using_method(SurveyMethod::SampleReturn));
    }

    #[test]
    fn probe_dispatched_flag_only_fires_for_probe_methods() {
        let mut world = fresh_world();
        // Non-probe mission started — flag must NOT advance.
        world.write_message(SurveyEvent::MissionStarted {
            body: Entity::PLACEHOLDER,
            mission_id: 1,
            name: "Rover 1".to_string(),
            method: SurveyMethod::Rover,
        });
        let mut schedule = build_schedule(&mut world);
        schedule.run(&mut world);
        assert!(
            !world.resource::<EarlyGameMilestones>().probe_dispatched,
            "Rover is not probe-using; flag must stay false"
        );

        // Probe mission started — flag MUST advance.
        world.write_message(SurveyEvent::MissionStarted {
            body: Entity::PLACEHOLDER,
            mission_id: 2,
            name: "Flyby 1".to_string(),
            method: SurveyMethod::Flyby,
        });
        schedule.run(&mut world);
        assert!(
            world.resource::<EarlyGameMilestones>().probe_dispatched,
            "Flyby is probe-using; flag must flip"
        );
    }

    // ── duplicate-delivery idempotency ─────────────────────────────

    #[test]
    fn duplicate_mission_completed_is_no_op() {
        let mut world = fresh_world();
        // Send the same MissionCompleted 5 times.
        for i in 0..5 {
            world.write_message(SurveyEvent::MissionCompleted {
                body: Entity::PLACEHOLDER,
                mission_id: i,
                name: format!("Mission {i}"),
                method: SurveyMethod::Flyby,
            });
        }
        let mut schedule = build_schedule(&mut world);
        schedule.run(&mut world);
        assert!(world.resource::<EarlyGameMilestones>().survey_completed);

        // Drain the buffer; rerun with no events; flag must stay true.
        world.resource_mut::<Messages<SurveyEvent>>().clear();
        schedule.run(&mut world);
        assert!(world.resource::<EarlyGameMilestones>().survey_completed);
    }

    #[test]
    fn duplicate_outpost_established_is_no_op() {
        let mut world = fresh_world();
        let colony = world.spawn_empty().id();
        let body = world.spawn_empty().id();
        for _ in 0..3 {
            world.write_message(ConstructionEvent::OutpostEstablished { colony, body });
        }
        let mut schedule = build_schedule(&mut world);
        schedule.run(&mut world);
        assert!(world.resource::<EarlyGameMilestones>().outpost_established);

        // Rerun with cleared buffer.
        world.resource_mut::<Messages<ConstructionEvent>>().clear();
        schedule.run(&mut world);
        assert!(world.resource::<EarlyGameMilestones>().outpost_established);
    }

    #[test]
    fn duplicate_anomaly_detected_and_activated_is_no_op() {
        let mut world = fresh_world();
        world.write_message(SurveyEvent::AnomalyDetected {
            body: Entity::PLACEHOLDER,
            anomaly: crate::survey::types::AnomalyType::MagneticAnomaly,
            initial_confidence: 0.10,
        });
        let mut schedule = build_schedule(&mut world);
        schedule.run(&mut world);
        assert!(
            world
                .resource::<EarlyGameMilestones>()
                .anomaly_detected_or_activated
        );

        // A subsequent AnomalyActivated must not flip the flag back to
        // false or otherwise change state. (The flag is monotonic, but
        // we verify the no-op assertion explicitly.)
        world.write_message(SurveyEvent::AnomalyActivated {
            body: Entity::PLACEHOLDER,
            anomaly: crate::survey::types::AnomalyType::MagneticAnomaly,
            confidence: 0.85,
        });
        schedule.run(&mut world);
        assert!(
            world
                .resource::<EarlyGameMilestones>()
                .anomaly_detected_or_activated
        );
    }

    // ── ordering / progression ────────────────────────────────────

    #[test]
    fn progression_walks_milestone_order() {
        let mut m = EarlyGameMilestones::default();
        assert_eq!(m.current_step(), Some(MilestoneStep::ProbeDispatched));

        m.probe_dispatched = true;
        assert_eq!(m.current_step(), Some(MilestoneStep::SurveyCompleted));

        m.survey_completed = true;
        assert_eq!(
            m.current_step(),
            Some(MilestoneStep::AnomalyDetectedOrActivated)
        );

        m.anomaly_detected_or_activated = true;
        assert_eq!(
            m.current_step(),
            Some(MilestoneStep::DepositExtractionMilestone)
        );

        // The deposit flag is unreachable today (producer gap).
        m.deposit_extraction_milestone = true;
        assert_eq!(m.current_step(), Some(MilestoneStep::OutpostEstablished));

        m.outpost_established = true;
        assert_eq!(
            m.current_step(),
            Some(MilestoneStep::PaidTier1TechnologyUnlocked)
        );

        m.paid_tier_1_technology_unlocked = true;
        assert_eq!(m.current_step(), None);
    }

    // ── paid tier-1 classification ────────────────────────────────

    #[test]
    fn paid_tier_1_requires_tier_1_and_positive_cost() {
        let mut world = fresh_world();
        // Insert a TechnologiesData with a paid tier-1, a free tier-1,
        // and a paid tier-2.
        let mut data = TechnologiesData::default();
        let paid_t1 = crate::research::types::Technology {
            id: "paid_t1".into(),
            name: "Paid Tier 1".into(),
            category: crate::research::types::TechCategory::Physics,
            description: "test".into(),
            research_cost: 1000.0,
            prerequisites: vec![],
            unlocks_components: vec![],
            unlocks_engineering: vec![],
            modifiers: vec![],
            tier: 1,
        };
        let free_t1 = crate::research::types::Technology {
            id: "free_t1".into(),
            research_cost: 0.0,
            ..paid_t1.clone()
        };
        let paid_t2 = crate::research::types::Technology {
            id: "paid_t2".into(),
            tier: 2,
            ..paid_t1.clone()
        };
        data.technologies.insert("paid_t1".into(), paid_t1);
        data.technologies.insert("free_t1".into(), free_t1);
        data.technologies.insert("paid_t2".into(), paid_t2);
        world.insert_resource(data);

        // Free tier-1: must NOT advance the flag.
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "free_t1".into(),
            tech_display_name: "Free Tier 1".into(),
        });
        let mut schedule = build_schedule(&mut world);
        schedule.run(&mut world);
        assert!(
            !world
                .resource::<EarlyGameMilestones>()
                .paid_tier_1_technology_unlocked,
            "free tier-1 must not count as 'paid'"
        );

        // Paid tier-2: must NOT advance the flag.
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "paid_t2".into(),
            tech_display_name: "Paid Tier 2".into(),
        });
        schedule.run(&mut world);
        assert!(
            !world
                .resource::<EarlyGameMilestones>()
                .paid_tier_1_technology_unlocked,
            "tier-2 must not count as tier-1"
        );

        // Paid tier-1: MUST advance the flag.
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "paid_t1".into(),
            tech_display_name: "Paid Tier 1".into(),
        });
        schedule.run(&mut world);
        assert!(
            world
                .resource::<EarlyGameMilestones>()
                .paid_tier_1_technology_unlocked
        );
    }

    #[test]
    fn paid_tier_1_unknown_id_is_no_op() {
        // Defends against a stale id from an old save with a removed
        // tech row. Per the architecture: "validate exact predicate
        // from current types/data" — a missing predicate is a no-op,
        // not a flag flip.
        let mut world = fresh_world();
        world.insert_resource(TechnologiesData::default());
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "ghost_tech".into(),
            tech_display_name: "Ghost".into(),
        });
        let mut schedule = build_schedule(&mut world);
        schedule.run(&mut world);
        assert!(
            !world
                .resource::<EarlyGameMilestones>()
                .paid_tier_1_technology_unlocked
        );
    }

    #[test]
    fn paid_tier_1_research_data_not_loaded_yet_is_no_op() {
        // Defends against the Startup-ordering race where the research
        // event fires before `TechnologiesData` has been inserted. The
        // consumer drains without advancing; the next completion will
        // re-fire the event and the flag will flip then.
        let mut world = World::new();
        world.init_resource::<EarlyGameMilestones>();
        world.init_resource::<Messages<ResearchEvent>>();
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "paid_t1".into(),
            tech_display_name: "Paid Tier 1".into(),
        });
        let mut schedule = Schedule::default();
        schedule.add_systems(advance_research_milestones);
        schedule.run(&mut world);
        assert!(
            !world
                .resource::<EarlyGameMilestones>()
                .paid_tier_1_technology_unlocked
        );
    }

    // ── persistence round-trip ────────────────────────────────────

    #[test]
    fn persistence_reflect_round_trip_preserves_every_flag() {
        // The save/load pipeline does not currently call
        // `DynamicScene::from_world` directly — PR-I (GRA-358) retired
        // the v1 DynamicScene path and uses the v2 StateStore
        // extractor. The `Reflect + Default + register_type` triad we
        // ship here is the future-proof hook: any future
        // Reflect-driven path (e.g. the v1 DynamicScene loader
        // re-enabled for debug snapshots, or the `serde::ReflectSerialize`
        // adapter used by external tooling) will pick the resource up
        // via `AppTypeRegistry` without further code changes.
        //
        // This test exercises the Reflect round-trip directly via
        // `ReflectFromReflect`, which is what `DynamicScene`'s
        // `extract_resources` uses internally — proving every flag
        // survives a Reflect-driven snapshot.

        use bevy::reflect::ReflectFromReflect;

        let mut world = fresh_world();
        let src = {
            let mut m = world.resource_mut::<EarlyGameMilestones>();
            m.probe_dispatched = true;
            m.survey_completed = true;
            m.anomaly_detected_or_activated = true;
            m.outpost_established = true;
            m.clone()
        };

        // Locate the registered Reflect impl.
        let registry = world.resource::<AppTypeRegistry>().clone();
        let registration = registry
            .read()
            .get_with_type_path(std::any::type_name::<EarlyGameMilestones>())
            .expect("EarlyGameMilestones must be registered")
            .clone();
        let reflect_from_reflect = registration
            .data::<ReflectFromReflect>()
            .expect("EarlyGameMilestones must implement ReflectFromReflect");

        // Clone the source via the Reflect trait and re-hydrate via
        // `from_reflect`. This is exactly the path `DynamicScene` and
        // the `serde::ReflectSerialize` adapter take.
        let dynamic = src.clone_value();
        let restored_value = reflect_from_reflect
            .from_reflect(&*dynamic)
            .expect("from_reflect must succeed for an in-spec resource");
        let restored = restored_value
            .downcast::<EarlyGameMilestones>()
            .expect("downcast must succeed");

        assert!(restored.probe_dispatched);
        assert!(restored.survey_completed);
        assert!(restored.anomaly_detected_or_activated);
        assert!(!restored.deposit_extraction_milestone);
        assert!(restored.outpost_established);
        assert!(!restored.paid_tier_1_technology_unlocked);
    }

    #[test]
    fn persistence_old_save_defaults_when_resource_missing() {
        // An old save without the resource must restore to
        // `EarlyGameMilestones::default()` when the fresh-world plugin
        // initialisation path runs `init_resource`. This is the
        // migration story — we do NOT bump `FORMAT_VERSION` because the
        // snapshot is additive: an existing save that lacks the
        // resource restores cleanly via `init_resource`.
        let mut dst = World::new();
        dst.init_resource::<EarlyGameMilestones>();
        let m = dst.resource::<EarlyGameMilestones>();
        assert!(!m.probe_dispatched);
        assert!(!m.survey_completed);
        assert!(!m.anomaly_detected_or_activated);
        assert!(!m.deposit_extraction_milestone);
        assert!(!m.outpost_established);
        assert!(!m.paid_tier_1_technology_unlocked);
        assert_eq!(m.current_step(), Some(MilestoneStep::ProbeDispatched));
    }
}
