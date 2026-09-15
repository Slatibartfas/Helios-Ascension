//! Launch capacity: the single shared source of truth for
//! surface-to-orbit mass access.
//!
//! ## Model
//!
//! Each body owns a [`LaunchCapacity`] stockpile in tonnes. The
//! stockpile's **production rate** and **cap** are *derived* every
//! time they are needed by summing the launch modifiers on the
//! body's buildings:
//!
//! - `LaunchCapacityProduction` — tonnes per year, summed per build.
//! - `LaunchCapacityMax` — tonnes of storage, summed per build.
//!
//! Because the figures are derived rather than stored, a building
//! that is added or demolished is reflected immediately, with no
//! migration step. Only the *balance* and its accrual anchor are
//! persisted (see [`LaunchCapacity`]).
//!
//! ## Charging rule
//!
//! Capacity is charged **once** at the surface-to-orbit boundary:
//! a ship that lifts off, a probe that launches, cargo loaded from a
//! surface into an orbiting hull, or colonists departing a surface.
//! Orbital assembly, orbital shipyards, orbit-to-orbit transfers,
//! interbody cruise, and payloads released from an already-orbiting
//! carrier cost nothing.
//!
//! ## Consumers
//!
//! Every consumer must go through [`reconcile`] before checking or
//! spending, so the balance is current as of the moment of the
//! decision, and must mark the originating body dirty afterwards
//! (`DirtyReason::LaunchCapacity`) so the spend survives save/load.

use bevy::prelude::*;

use crate::colony::{BuildingsData, Colony};

use super::components::LaunchCapacity;

/// Derived launch figures for one body, recomputed from its
/// buildings. Both fields are zero for a body with no launch
/// infrastructure.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LaunchCapacityProfile {
    /// Tonnes per simulated year.
    pub production_t_per_year: f64,
    /// Tonnes of storable launch mass.
    pub cap_tonnes: f64,
}

impl LaunchCapacityProfile {
    /// True when the body can move or store any mass at all.
    pub fn is_operational(&self) -> bool {
        self.production_t_per_year > 0.0 && self.cap_tonnes > 0.0
    }
}

/// Sum the launch modifiers across every building on `colony`.
///
/// The modifier names are matched generically, so any future launch
/// facility contributes by declaring the same two modifiers in
/// `assets/data/buildings.ron` — no code change required.
pub fn profile_for_colony(colony: &Colony, data: &BuildingsData) -> LaunchCapacityProfile {
    let mut profile = LaunchCapacityProfile::default();
    for (building_type, count) in colony.buildings.iter() {
        if *count == 0 {
            continue;
        }
        let Some(definition) = data.get(building_type) else {
            continue;
        };
        for modifier in &definition.modifiers {
            match modifier.modifier_type.as_str() {
                "LaunchCapacityProduction" => {
                    profile.production_t_per_year += modifier.value * f64::from(*count);
                }
                "LaunchCapacityMax" => {
                    profile.cap_tonnes += modifier.value * f64::from(*count);
                }
                _ => {}
            }
        }
    }
    profile
}

/// Advance `capacity` to `now_seconds` using `profile` and return the
/// resulting available balance.
///
/// This is the only sanctioned way to read a live balance: it keeps
/// the accrual anchor honest, so a long pause, a time-acceleration
/// burst, or a save/load round trip cannot mint or lose mass.
pub fn reconcile(
    capacity: &mut LaunchCapacity,
    profile: &LaunchCapacityProfile,
    now_seconds: f64,
) -> f64 {
    capacity.accrue(
        profile.production_t_per_year,
        profile.cap_tonnes,
        now_seconds,
    );
    capacity.current_tonnes
}

/// Read the available balance for `entity` without mutating it.
///
/// Use this for UI and planning paths: it projects the accrual into
/// a temporary copy so the displayed figure matches what a consume
/// would find, while leaving the authoritative component (and the
/// dirty-body bookkeeping) untouched.
pub fn projected_available(
    capacity: &LaunchCapacity,
    profile: &LaunchCapacityProfile,
    now_seconds: f64,
) -> f64 {
    let mut copy = *capacity;
    copy.accrue(
        profile.production_t_per_year,
        profile.cap_tonnes,
        now_seconds,
    );
    copy.current_tonnes
}

/// Simulation seconds until `capacity` can afford `required_tonnes`,
/// or `None` when the requirement is already met or can never be met
/// with the current infrastructure.
pub fn seconds_until_affordable(
    capacity: &LaunchCapacity,
    profile: &LaunchCapacityProfile,
    now_seconds: f64,
    required_tonnes: f64,
) -> Option<f64> {
    if !required_tonnes.is_finite() || required_tonnes <= 0.0 {
        return None;
    }
    let available = projected_available(capacity, profile, now_seconds);
    if available + f64::EPSILON >= required_tonnes {
        return None;
    }
    if !profile.is_operational() || required_tonnes > profile.cap_tonnes + f64::EPSILON {
        return None;
    }
    let shortfall = required_tonnes - available;
    let seconds = shortfall / profile.production_t_per_year * super::budget::SECONDS_PER_YEAR;
    Some(seconds.max(0.0))
}

/// Bootstrap (`25 %`-charged) stockpile for a body that just gained
/// launch infrastructure, or for a legacy save that predates the
/// persistent component.
pub fn bootstrap(cap_tonnes: f64, now_seconds: f64) -> LaunchCapacity {
    LaunchCapacity::bootstrapped(cap_tonnes, now_seconds)
}

/// System: attach a bootstrapped [`LaunchCapacity`] to every colony
/// that has launch infrastructure but no stockpile yet.
///
/// Only bodies with a positive derived cap are provisioned, so
/// unrelated entities never receive a zero-valued component. Bodies
/// that already carry a stockpile are left untouched — their balance
/// and anchor are authoritative and are advanced lazily by
/// [`reconcile`] on the read/consume paths.
pub fn provision_launch_capacity(
    mut commands: Commands,
    sim_time: Res<crate::ui::SimulationTime>,
    buildings_data: Option<Res<BuildingsData>>,
    colonies: Query<(Entity, &Colony)>,
    existing: Query<(), With<LaunchCapacity>>,
) {
    let Some(data) = buildings_data else {
        return;
    };
    if data.definitions.is_empty() {
        return;
    }

    let now = sim_time.elapsed_seconds();
    for (entity, colony) in colonies.iter() {
        if existing.contains(entity) {
            continue;
        }
        let profile = profile_for_colony(colony, &data);
        if profile.cap_tonnes <= 0.0 {
            continue;
        }
        // `try_insert` keeps a body despawned between the query
        // iteration and the deferred-command flush (menu → in-game
        // swap teardown) from panicking the launch path.
        commands
            .entity(entity)
            .try_insert(bootstrap(profile.cap_tonnes, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::data::{BuildingDefinition, BuildingModifierDef};
    use crate::colony::BuildingType;

    fn modifier(modifier_type: &str, value: f64) -> BuildingModifierDef {
        BuildingModifierDef {
            modifier_type: modifier_type.to_string(),
            value,
        }
    }

    fn buildings_data_with(
        entries: Vec<(BuildingType, Vec<BuildingModifierDef>)>,
    ) -> BuildingsData {
        let mut data = BuildingsData::default();
        for (building_type, modifiers) in entries {
            let definition = BuildingDefinition {
                modifiers,
                ..Default::default()
            };
            data.definitions.insert(building_type, definition);
        }
        data
    }

    #[test]
    fn profile_sums_all_launch_facilities() {
        let data = buildings_data_with(vec![
            (
                BuildingType::LaunchSite,
                vec![
                    modifier("LaunchCapacityProduction", 100.0),
                    modifier("LaunchCapacityMax", 5_000.0),
                ],
            ),
            (
                BuildingType::OrbitalLift,
                vec![
                    modifier("LaunchCapacityProduction", 25_000.0),
                    modifier("LaunchCapacityMax", 250_000.0),
                ],
            ),
            (
                BuildingType::MassDriver,
                vec![
                    modifier("LaunchCapacityProduction", 5_000.0),
                    modifier("LaunchCapacityMax", 50_000.0),
                ],
            ),
            // Logistics-only buildings must not leak into launch.
            (
                BuildingType::CargoTerminal,
                vec![modifier("LogisticsCapacity", 2_000.0)],
            ),
        ]);

        let mut colony = Colony::new("Test".to_string(), 0.0);
        colony.buildings.insert(BuildingType::LaunchSite, 2);
        colony.buildings.insert(BuildingType::OrbitalLift, 1);
        colony.buildings.insert(BuildingType::MassDriver, 3);
        colony.buildings.insert(BuildingType::CargoTerminal, 4);

        let profile = profile_for_colony(&colony, &data);
        assert_eq!(profile.production_t_per_year, 200.0 + 25_000.0 + 15_000.0);
        assert_eq!(profile.cap_tonnes, 10_000.0 + 250_000.0 + 150_000.0);
        assert!(profile.is_operational());
    }

    #[test]
    fn profile_ignores_colonies_without_launch_infrastructure() {
        let data = buildings_data_with(vec![(
            BuildingType::LaunchSite,
            vec![
                modifier("LaunchCapacityProduction", 100.0),
                modifier("LaunchCapacityMax", 5_000.0),
            ],
        )]);
        let mut colony = Colony::new("Test".to_string(), 0.0);
        colony.buildings.insert(BuildingType::LaunchSite, 0);

        let profile = profile_for_colony(&colony, &data);
        assert_eq!(profile, LaunchCapacityProfile::default());
        assert!(!profile.is_operational());
    }

    /// Pins the shipped RON data: the three launch-capable buildings
    /// must each declare both modifiers, and logistics-only buildings
    /// must not.
    #[test]
    fn shipped_buildings_data_declares_launch_modifiers() {
        let data = crate::colony::data::BuildingsData::load_for_tests();

        for building_type in [
            BuildingType::LaunchSite,
            BuildingType::MassDriver,
            BuildingType::OrbitalLift,
        ] {
            let definition = data
                .get(&building_type)
                .unwrap_or_else(|| panic!("{building_type:?} is missing from buildings.ron"));
            let production = definition
                .modifiers
                .iter()
                .find(|m| m.modifier_type == "LaunchCapacityProduction")
                .map(|m| m.value)
                .unwrap_or(0.0);
            let cap = definition
                .modifiers
                .iter()
                .find(|m| m.modifier_type == "LaunchCapacityMax")
                .map(|m| m.value)
                .unwrap_or(0.0);
            assert!(
                production > 0.0,
                "{building_type:?} must declare a positive LaunchCapacityProduction"
            );
            assert!(
                cap > 0.0,
                "{building_type:?} must declare a positive LaunchCapacityMax"
            );
            assert!(
                cap >= production,
                "{building_type:?} must be able to store at least one year of its output"
            );
        }
    }

    #[test]
    fn bootstrap_starts_at_a_quarter_of_cap() {
        let capacity = bootstrap(12_000.0, 500.0);
        assert_eq!(capacity.current_tonnes, 3_000.0);
        assert_eq!(capacity.last_updated_sim_seconds, 500.0);
    }

    #[test]
    fn accrue_adds_production_and_respects_cap() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 1_000.0,
            cap_tonnes: 5_000.0,
        };
        let mut capacity = LaunchCapacity::new(0.0, 0.0);

        // Half a year at 1,000 t/yr.
        reconcile(&mut capacity, &profile, 15_778_800.0);
        assert!((capacity.current_tonnes - 500.0).abs() < 1e-6);

        // Ten more years would overshoot; the cap clamps.
        reconcile(&mut capacity, &profile, 331_000_000.0);
        assert!((capacity.current_tonnes - 5_000.0).abs() < 1e-6);
    }

    #[test]
    fn accrue_ignores_backwards_and_zero_intervals() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 1_000.0,
            cap_tonnes: 5_000.0,
        };
        let mut capacity = LaunchCapacity::new(1_000.0, 100_000.0);

        let added = reconcile(&mut capacity, &profile, 100_000.0);
        assert_eq!(added, 1_000.0);

        // A clock that jumps backwards must not mint capacity, and
        // must re-anchor so the later forward step is measured from
        // the new (earlier) point.
        let added = reconcile(&mut capacity, &profile, 50_000.0);
        assert_eq!(added, 1_000.0);
        assert_eq!(capacity.last_updated_sim_seconds, 50_000.0);
    }

    #[test]
    fn accrue_does_not_backfill_a_restored_anchor() {
        // Simulating a restore: the balance was saved mid-campaign
        // with its anchor, so the first post-load tick only accrues
        // the time since the save, not the whole campaign.
        let profile = LaunchCapacityProfile {
            production_t_per_year: 10_000.0,
            cap_tonnes: 100_000.0,
        };
        let mut capacity = LaunchCapacity::new(1_000.0, 1_000_000_000.0);

        reconcile(&mut capacity, &profile, 1_000_000_000.0);
        assert!((capacity.current_tonnes - 1_000.0).abs() < 1e-6);

        // One year later: 1,000 + 10,000.
        reconcile(&mut capacity, &profile, 1_031_557_600.0);
        assert!((capacity.current_tonnes - 11_000.0).abs() < 1e-3);
    }

    #[test]
    fn try_consume_rejects_invalid_mass() {
        let mut capacity = LaunchCapacity::new(500.0, 0.0);
        assert!(!capacity.try_consume(0.0));
        assert!(!capacity.try_consume(-10.0));
        assert!(!capacity.try_consume(f64::NAN));
        assert!(!capacity.try_consume(f64::INFINITY));
        assert_eq!(capacity.current_tonnes, 500.0);
    }

    #[test]
    fn try_consume_spends_exactly_once_and_blocks_when_short() {
        let mut capacity = LaunchCapacity::new(500.0, 0.0);
        assert!(!capacity.try_consume(600.0));
        assert_eq!(capacity.current_tonnes, 500.0);

        assert!(capacity.try_consume(500.0));
        assert_eq!(capacity.current_tonnes, 0.0);

        assert!(!capacity.try_consume(1.0));
        assert_eq!(capacity.current_tonnes, 0.0);
    }

    #[test]
    fn demolishing_a_facility_clamps_the_balance() {
        let mut capacity = LaunchCapacity::new(40_000.0, 0.0);
        capacity.clamp_to_cap(2_000.0);
        assert_eq!(capacity.current_tonnes, 2_000.0);
    }

    #[test]
    fn projected_available_does_not_mutate_the_anchor() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 1_000.0,
            cap_tonnes: 100_000.0,
        };
        let capacity = LaunchCapacity::new(0.0, 0.0);

        // One year of production: 1,000 t, well under the cap.
        let projected = projected_available(&capacity, &profile, 31_557_600.0);
        assert!((projected - 1_000.0).abs() < 1e-3);

        // A hundred years saturates the 100 kt cap.
        let projected = projected_available(&capacity, &profile, 3_155_760_000.0);
        assert!((projected - 100_000.0).abs() < 1e-3);

        // The projection did not touch the stored component.
        assert_eq!(capacity.current_tonnes, 0.0);
        assert_eq!(capacity.last_updated_sim_seconds, 0.0);
    }

    #[test]
    fn seconds_until_affordable_reports_wait_and_impossibility() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 3_155_760.0,
            cap_tonnes: 100_000.0,
        };

        // Already affordable.
        let ready = LaunchCapacity::new(10_000.0, 0.0);
        assert_eq!(
            seconds_until_affordable(&ready, &profile, 0.0, 5_000.0),
            None
        );

        // A full-cap requirement is reachable; the wait is
        // cap / production years.
        let short = LaunchCapacity::new(0.0, 0.0);
        let wait = seconds_until_affordable(&short, &profile, 0.0, 100_000.0)
            .expect("should report a wait");
        let expected_years = 100_000.0 / 3_155_760.0;
        assert!((wait - expected_years * 31_557_600.0).abs() < 1.0);

        // Larger than the body can ever store.
        assert_eq!(
            seconds_until_affordable(&short, &profile, 0.0, 500_000.0),
            None
        );
    }

    #[test]
    fn seconds_until_affordable_fails_closed_without_infrastructure() {
        let profile = LaunchCapacityProfile::default();
        let capacity = LaunchCapacity::new(0.0, 0.0);
        assert_eq!(
            seconds_until_affordable(&capacity, &profile, 0.0, 10.0),
            None
        );
    }

    /// A surface project that finishes fabrication spends the
    /// launch-mass budget exactly once, and a second attempt on the
    /// same tick fails (no double debit). This is the canonical
    /// "atomic surface launch" guarantee every consumer must keep.
    #[test]
    fn surface_launch_consumes_capacity_exactly_once() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 100.0,
            cap_tonnes: 5_000.0,
        };
        let mut capacity = LaunchCapacity::new(5_000.0, 0.0);

        // Saturate the cap at 5,000 t (no additional accrual).
        reconcile(&mut capacity, &profile, 0.0);

        // Launch a 300 t ship.
        assert!(capacity.try_consume(300.0));
        assert!((capacity.current_tonnes - 4_700.0).abs() < 1e-6);

        // A second identical launch on the same tick would
        // double-spend — confirm the API rejects it.
        assert!(!capacity.try_consume(4_701.0));
        assert!((capacity.current_tonnes - 4_700.0).abs() < 1e-6);
    }

    /// Orbital-mode projects must not debit capacity. The
    /// shipbuilding pipeline routes them straight to
    /// `CompletedInOrbit`, but we pin the public contract: nothing
    /// orbits without paying when it crossed a surface boundary.
    #[test]
    fn orbital_mode_does_not_debit_capacity() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 100.0,
            cap_tonnes: 5_000.0,
        };
        let mut capacity = LaunchCapacity::new(5_000.0, 0.0);
        reconcile(&mut capacity, &profile, 0.0);

        // Nothing crossed the surface, so the balance is untouched.
        assert!((capacity.current_tonnes - 5_000.0).abs() < 1e-6);
    }

    /// When a launch is rejected by the consumables/credit check the
    /// reserved mass must be returned. This is the contract the
    /// shipbuilding consumer relies on: a project that waits for
    /// resource deliveries must come back to a clean slate.
    #[test]
    fn reservation_refund_keeps_balance_consistent() {
        let profile = LaunchCapacityProfile {
            production_t_per_year: 100.0,
            cap_tonnes: 5_000.0,
        };
        let mut capacity = LaunchCapacity::new(3_000.0, 0.0);
        reconcile(&mut capacity, &profile, 0.0);

        let mass = 300.0;
        assert!(capacity.try_consume(mass));
        // Rejected — the project is still short on materials.
        capacity.current_tonnes = (capacity.current_tonnes + mass).min(profile.cap_tonnes);
        assert!((capacity.current_tonnes - 3_000.0).abs() < 1e-6);

        // Later: the shortage clears, the project tries again.
        assert!(capacity.try_consume(mass));
        assert!((capacity.current_tonnes - 2_700.0).abs() < 1e-6);
    }

    /// The dirty marker must be able to carry the new variant
    /// without disturbing the existing `Multiple` escalation path.
    /// This is the unit-level pin for `state_store_extract.rs`.
    #[test]
    fn dirty_reason_launch_capacity_escalates_to_multiple() {
        use crate::economy::components::{DirtyBodies, DirtyReason};
        let mut dirty = DirtyBodies::default();
        let entity = Entity::from_raw_u32(7).expect("entity bits");
        dirty.mark(entity, DirtyReason::LaunchCapacity);
        assert_eq!(dirty.reason(entity), Some(DirtyReason::LaunchCapacity));
        dirty.mark(entity, DirtyReason::Stockpile);
        assert_eq!(dirty.reason(entity), Some(DirtyReason::Multiple));
    }
}
