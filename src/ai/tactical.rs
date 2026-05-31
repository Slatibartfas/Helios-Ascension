//! Tactical AI — fleet combat decisions, maneuvering, engagement/retreat logic.
//!
//! When two fleets encounter each other (or an AI fleet is near an enemy),
//! the tactical AI decides:
//! - Whether to engage, hold position, or retreat
//! - How to maneuver during combat
//! - When to withdraw (based on casualties, fleet strength)
//! - Focus fire targets
//!
//! Decisions are wired into `PendingFleetActions`:
//! - `Engage`     → cancels current maneuver if fleet is in transit; combat system takes over
//! - `Hold`       → no action needed
//! - `Retreat`    → cancels maneuver + queues immediate Hohmann transfer to nearest faction colony
//! - `Maneuver`   → logs only (positional maneuvering deferred to Phase 2)

use bevy::prelude::*;
use bevy::math::DVec3;

use crate::astronomy::components::SpaceCoordinates;
use crate::astronomy::KeplerOrbit;
use crate::fleets::components::{ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, PlannedTransfer, StartTransferAction};
use crate::fleets::orbital_mechanics::{hohmann_transfer, GM_SUN};
use crate::fleets::TransferReferenceFrame;
use crate::colony::components::Colony;
use crate::plugins::solar_system::CelestialBody;

use super::components::{AIControlledColony, AIControlledFleet, AIFaction, AIDifficulty, AIPersonality};

/// Distance within which AI declares combat (AU).
const COMBAT_RANGE_AU: f64 = 1.0;

/// Combat strength comparison result.
#[derive(Debug, Clone, Copy)]
pub enum CombatDecision {
    Engage,
    Hold,
    Retreat,
    Maneuver { preferred_distance: f64 },
}

/// Engagement threshold: ratio of our strength to enemy strength below which we retreat.
const RETREAT_RATIO_EASY: f64 = 0.4;
const RETREAT_RATIO_NORMAL: f64 = 0.6;
const RETREAT_RATIO_HARD: f64 = 0.8;

/// Engagement threshold for holding: ratio above which we aggressively engage.
const ENGAGE_RATIO_EASY: f64 = 1.8;
const ENGAGE_RATIO_NORMAL: f64 = 1.5;
const ENGAGE_RATIO_HARD: f64 = 1.2;

pub fn evaluate_combat(
    our_fleet: &Fleet,
    enemy_fleet: &Fleet,
    difficulty: AIDifficulty,
    personality: AIPersonality,
) -> CombatDecision {
    let our_strength = calculate_fleet_strength(our_fleet);
    let enemy_strength = calculate_fleet_strength(enemy_fleet);

    if enemy_strength <= 0.0 {
        return CombatDecision::Hold;
    }

    let ratio = our_strength / enemy_strength;

    let (retreat_thresh, engage_thresh) = match difficulty {
        AIDifficulty::Easy => (RETREAT_RATIO_EASY, ENGAGE_RATIO_EASY),
        AIDifficulty::Normal => (RETREAT_RATIO_NORMAL, ENGAGE_RATIO_NORMAL),
        AIDifficulty::Hard => (RETREAT_RATIO_HARD, ENGAGE_RATIO_HARD),
    };

    if ratio < retreat_thresh {
        info!(
            "Tactical AI: ratio {:.2} < {:.2} retreat threshold — RETREAT",
            ratio, retreat_thresh
        );
        return CombatDecision::Retreat;
    }

    if ratio > engage_thresh
        && (personality == AIPersonality::Militarist || ratio > engage_thresh * 1.5) {
            info!(
                "Tactical AI: ratio {:.2} > {:.2} engage threshold — ENGAGE",
                ratio, engage_thresh
            );
            return CombatDecision::Engage;
        }

    CombatDecision::Hold
}

/// Calculate total combat strength of a fleet.
fn calculate_fleet_strength(fleet: &Fleet) -> f64 {
    fleet
        .ships
        .iter()
        .map(|ship| ship.dry_mass_t as f64 * 0.5)
        .sum::<f64>()
        .max(1.0)
}

/// Decide which enemy fleet to target (returns the entity of the weakest enemy).
pub fn select_priority_target(
    enemy_fleets: &[(Entity, Fleet)],
) -> Option<Entity> {
    enemy_fleets
        .iter()
        .min_by_key(|(_, f)| calculate_fleet_strength(f) as u32)
        .map(|(e, _)| *e)
}

// ── Retreat transfer builder ───────────────────────────────────────────────────

/// Build a `PlannedTransfer` for the quickest retreat from current position
/// directly toward the destination body using a Hohmann transfer arc.
///
/// `fleet_pos` and `dest_pos` are heliocentric position vectors in AU.
fn build_retreat_transfer(
    fleet_pos: DVec3,
    dest_pos: DVec3,
    origin_body: Entity,
    destination_body: Entity,
    duration_s: f64,
    dv2_ms: f64,
) -> PlannedTransfer {
    let r1 = fleet_pos.length();
    let r2 = dest_pos.length();

    let ecc = (r2 - r1).abs() / (r2 + r1);
    let sma = (r1 + r2) / 2.0;

    let transfer_orbit = KeplerOrbit {
        eccentricity: ecc,
        semi_major_axis: sma,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        mean_anomaly_epoch: 0.0,
        mean_motion: std::f64::consts::TAU / duration_s,
    };

    PlannedTransfer {
        origin_body,
        destination_body,
        reference_frame: TransferReferenceFrame::SystemBarycentric,
        orbit_center: Entity::from_bits(0), // Sun.
        transfer_orbit,
        duration_s,
        preserve_orbit_geometry: false,
        arrival_delta_v_ms: dv2_ms,
        arrival_orbit_radius_au: 0.05,
        fuel_cost_t: 0.0,
        option_label: "Hohmann Retreat",
        start_position_au: Some(fleet_pos),
        end_position_au: Some(dest_pos),
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    }
}

// ── Issue retreat ───────────────────────────────────────────────────────────────

fn issue_retreat(
    fleet_entity: Entity,
    fleet_pos: DVec3,
    faction: &AIFaction,
    colonies: &Query<(Entity, &Colony, &AIControlledColony, &SpaceCoordinates, Option<&CelestialBody>)>,
    pending_fleet: &mut PendingFleetActions,
) {
    // Find nearest faction colony by heliocentric distance.
    let mut best_dist = f64::MAX;
    let mut best_colony_sc = DVec3::ZERO;
    let mut best_colony_entity: Option<Entity> = None;

    for &colony_entity in &faction.colonies {
        if let Ok((_, _, _, sc, _)) = colonies.get(colony_entity) {
            let d = (sc.position - fleet_pos).length();
            if d < best_dist {
                best_dist = d;
                best_colony_sc = sc.position;
                best_colony_entity = Some(colony_entity);
            }
        }
    }

    let Some(colony_entity) = best_colony_entity else {
        info!(
            "Tactical AI: fleet entity {:?} ordered to retreat but has no faction colony",
            fleet_entity
        );
        return;
    };

    let r1 = fleet_pos.length();
    let r2 = best_colony_sc.length();

    if r1 < 1e-9 || r2 < 1e-9 {
        return;
    }

    // Same orbit — no retreat transfer needed.
    if (r1 - r2).abs() < 1e-5 {
        return;
    }

    let (_dv1_ms, dv2_ms, duration_s, _sma_au, _ecc) =
        hohmann_transfer(r1, r2, GM_SUN);

    if duration_s <= 0.0 {
        return;
    }

    // Cancel any in-transit maneuver first.
    pending_fleet.cancel_maneuvers.push(fleet_entity);

    let planned = build_retreat_transfer(
        fleet_pos,
        best_colony_sc,
        fleet_entity,
        colony_entity,
        duration_s,
        dv2_ms,
    );

    pending_fleet.start_transfers.push(StartTransferAction {
        fleet: fleet_entity,
        transfer: planned,
        abort_cost_t: 0.0,
        departure_offset_s: 0.0,
    });
}

// ── Main tactical system ───────────────────────────────────────────────────────

/// AI combat system: process combat decisions for all AI fleets.
///
/// Each AI-controlled fleet in a `FleetOrbit` evaluates nearby enemy fleets.
/// When a decision is made it is wired into `PendingFleetActions`:
///   Engage  → cancels any mid-transit maneuver (the fleet parks and fights)
///   Retreat → cancels maneuver and starts immediate transfer to nearest faction colony
///   Hold    → no action (fleet maintains orbit)
///   Maneuver→ logs only (positional maneuvering deferred to Phase 2)
pub fn run_tactical_ai(
    sim_time: Res<crate::ui::SimulationTime>,
    mut pending_fleet: ResMut<crate::fleets::PendingFleetActions>,
    ai_factions: Query<&AIFaction>,
    ai_fleets: Query<(Entity, &Fleet, &AIControlledFleet, &FleetOrbit)>,
    all_fleets: Query<(Entity, &Fleet, &SpaceCoordinates, Option<&AIControlledFleet>)>,
    fleet_coords: Query<&SpaceCoordinates, With<Fleet>>,
    maneuver_query: Query<&ActiveManeuver, With<Fleet>>,
    colonies: Query<(Entity, &Colony, &AIControlledColony, &SpaceCoordinates, Option<&CelestialBody>)>,
) {
    let elapsed = sim_time.elapsed_seconds();
    let _ = elapsed; // For future timestamped logging.

    for faction in ai_factions.iter() {
        let faction_id = faction.faction_id;

        // Build list of enemy fleet positions.
        let enemy_fleets: Vec<(Entity, DVec3)> = all_fleets
            .iter()
            .filter(|(_, _, _, acf)| acf.is_none_or(|af| af.faction_id != faction_id))
            .map(|(e, _, sc, _)| (e, sc.position))
            .collect();

        if enemy_fleets.is_empty() {
            continue;
        }

        for (fleet_entity, fleet, _ai_control, _orbit) in ai_fleets.iter() {
            if fleet.ships.is_empty() {
                continue;
            }

            let Ok(fleet_sc) = fleet_coords.get(fleet_entity) else { continue };
            let fleet_pos = fleet_sc.position;

            // Find nearest enemy within combat range.
            let mut nearest_enemy: Option<(Entity, DVec3)> = None;
            let mut nearest_dist = f64::MAX;

            for (enemy_entity, enemy_pos) in &enemy_fleets {
                let dist = (fleet_pos - *enemy_pos).length();
                if dist < nearest_dist {
                    nearest_dist = dist;
                    nearest_enemy = Some((*enemy_entity, *enemy_pos));
                }
            }

            let Some((enemy_entity, _)) = nearest_enemy else { continue };
            if nearest_dist > COMBAT_RANGE_AU {
                continue;
            }

            // Get enemy fleet data for strength evaluation.
            let Some((_, enemy_fleet, _, _)) = all_fleets
                .iter()
                .find(|(e, _, _, _)| *e == enemy_entity)
            else { continue };

            let decision = evaluate_combat(
                fleet,
                enemy_fleet,
                faction.difficulty,
                faction.personality,
            );

            match decision {
                CombatDecision::Engage => {
                    info!(
                        "[{}] Tactical AI: ENGAGE — {} vs {} at {:.4} AU",
                        faction.name, fleet.name, enemy_fleet.name, nearest_dist
                    );
                    // If fleet is in transit, cancel so it parks and lets the combat
                    // system handle the engagement.
                    if maneuver_query.get(fleet_entity).is_ok() {
                        pending_fleet.cancel_maneuvers.push(fleet_entity);
                    }
                }
                CombatDecision::Hold => {
                    // Fleet maintains position; nothing to enqueue.
                }
                CombatDecision::Retreat => {
                    info!(
                        "[{}] Tactical AI: RETREAT — {} fleeing {} at {:.4} AU",
                        faction.name, fleet.name, enemy_fleet.name, nearest_dist
                    );
                    if !faction.colonies.is_empty() {
                        issue_retreat(fleet_entity, fleet_pos, faction, &colonies, &mut pending_fleet);
                    }
                }
                CombatDecision::Maneuver { preferred_distance: _ } => {
                    // Positional maneuvering (preferred distance) deferred to Phase 2.
                    info!(
                        "[{}] Tactical AI: MANEUVER — {} at {:.4} AU (deferred to Phase 2)",
                        faction.name, fleet.name, nearest_dist
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_fleet_strength() {
        let fleet = Fleet::new("Test Fleet".to_string());
        assert!(calculate_fleet_strength(&fleet) >= 1.0);
    }

    #[test]
    fn test_combat_decision_easy_retreat() {
        let mut our = Fleet::new("Weak".to_string());
        let mut enemy = Fleet::new("Strong".to_string());
        // Weak=100t vs Strong=1000t → ratio=0.1, Easy retreat thresh=0.4 → retreat
        let weak = ShipInfo::new("W".into(), ShipClass::Courier, PropulsionType::Chemical);
        let strong = ShipInfo::new("S".into(), ShipClass::Cruiser, PropulsionType::FusionTorch);
        // Override dry_mass_t to get exact strength values:
        // calculate_fleet_strength = sum(dry_mass_t * 0.5)
        // Weak: 100 * 0.5 = 50, Strong: 1000 * 0.5 = 500 → ratio=0.1
        let mut w = weak;
        w.dry_mass_t = 100.0;
        w.fuel_mass_t = 0.0;
        w.max_fuel_t = 0.0;
        let mut s = strong;
        s.dry_mass_t = 1000.0;
        s.fuel_mass_t = 0.0;
        s.max_fuel_t = 0.0;
        our.ships.push(w);
        enemy.ships.push(s);
        let decision = evaluate_combat(&our, &enemy, AIDifficulty::Easy, AIPersonality::Balanced);
        // Very weak fleet vs strong enemy → retreat threshold on Easy is lowest.
        assert!(matches!(decision, CombatDecision::Retreat));
    }

    #[test]
    fn test_combat_decision_overwhelming_advantage() {
        let mut our = Fleet::new("Strong".to_string());
        let mut enemy = Fleet::new("Weak".to_string());
        // Strong=1000t vs Weak=100t → ratio=10.0, Hard engage thresh=1.2
        // Militarist also requires ratio > 1.2 * 1.5 = 1.8 → satisfied
        let strong = ShipInfo::new("S".into(), ShipClass::Cruiser, PropulsionType::FusionTorch);
        let weak = ShipInfo::new("W".into(), ShipClass::Courier, PropulsionType::Chemical);
        let mut s = strong;
        s.dry_mass_t = 1000.0;
        s.fuel_mass_t = 0.0;
        s.max_fuel_t = 0.0;
        let mut w = weak;
        w.dry_mass_t = 100.0;
        w.fuel_mass_t = 0.0;
        w.max_fuel_t = 0.0;
        our.ships.push(s);
        enemy.ships.push(w);
        let decision = evaluate_combat(&our, &enemy, AIDifficulty::Hard, AIPersonality::Militarist);
        // Strong fleet vs weak + militarist → definitely engage.
        assert!(matches!(decision, CombatDecision::Engage));
    }
}
