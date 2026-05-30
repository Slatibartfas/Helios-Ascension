//! ECS systems for the sensor system.

use std::collections::HashMap;

use bevy::math::DVec3;
use bevy::prelude::*;

use super::components::{Contact, ContactState, SensorSuite, Signature, StealthMode};
use super::data::{AU_IN_KM, SensorData};
use crate::astronomy::SpaceCoordinates;
use crate::fleets::components::Fleet;
use crate::fleets::types::ShipClass;
use crate::ui::SimulationTime;

// ── Signature calculation ─────────────────────────────────────────────────────

/// Compute the thermal signature contribution from engines.
///
/// `thermal = base × (thrust / max_thrust)²`
pub fn engine_thermal_signature(base: f32, thrust_ratio: f32) -> f32 {
    base * thrust_ratio * thrust_ratio
}

/// Calculate the current signature for a ship given its class, stealth mode, and engine state.
///
/// `signature = base × (1 + emission_factor) × (1 − reduction_factor)`
///
/// Uses default class signatures when `data` is `None` or the class isn't found.
pub fn calculate_signature(
    ship_class: ShipClass,
    stealth_mode: StealthMode,
    thrust_ratio: f32,
    data: Option<&SensorData>,
) -> Signature {
    let class_map = data.map(|d| d.signature_class_map());
    let class_def = class_map.and_then(|m| m.get(&ship_class.to_string()));

    let (base_thermal, base_em, base_visual, base_neutrino) = match class_def {
        Some(c) => (c.base_thermal as f32, c.base_em as f32, c.base_visual as f32, c.base_neutrino as f32),
        None => (1.0_f32, 0.5_f32, 0.3_f32, 0.0_f32),
    };

    let emission_mult = stealth_mode.emission_multiplier();
    let engine_thermal = engine_thermal_signature(base_thermal, thrust_ratio);

    Signature {
        thermal: base_thermal + engine_thermal * emission_mult,
        em: base_em * emission_mult,
        visual: base_visual * emission_mult,
        neutrino: base_neutrino,
    }
}

/// Detection check: `sensor_strength / (target_signature × distance²) × time_factor`
///
/// Distance is in AU (not squared — the function squares it internally for 1/AU² falloff).
/// Returns the detection delta [0, 100] per tick for a single sensor against a target.
pub fn detection_check(
    sensor_strength: f32,
    target_signature: f32,
    distance_au: f64,
    dt: f64,
) -> f32 {
    if distance_au <= 0.0 || target_signature <= 0.0 {
        return 0.0;
    }
    let distance_sq = (distance_au * distance_au) as f32;
    let factor = sensor_strength / (target_signature * distance_sq);
    let time_factor = dt as f32;
    (factor * time_factor * 100.0).clamp(0.0, 100.0)
}

/// Distance between two entities in km.
fn distance_km(a: &SpaceCoordinates, b: &SpaceCoordinates) -> f64 {
    let diff = a.position - b.position;
    let dist_au = diff.length();
    dist_au * AU_IN_KM
}

// ── Signature update system ───────────────────────────────────────────────────

/// Update signatures for all ships in sensor-equipped fleets.
///
/// Runs before the detection system so updated signatures are available for
/// contact resolution. If `SensorData` isn't loaded, uses default class signatures.
pub fn update_fleet_signatures(
    mut fleet_query: Query<&mut Fleet>,
    sensor_data: Option<ResMut<SensorData>>,
    _time: Res<SimulationTime>,
) {
    let data = sensor_data.as_mut().map(|r| r.as_mut());
    for mut fleet in fleet_query.iter_mut() {
        for ship in &mut fleet.ships {
            let thrust_ratio = if ship.max_fuel_t > 0.0 {
                ship.fuel_fraction()
            } else {
                1.0
            };
            ship.signature = calculate_signature(
                ship.class,
                ship.stealth_mode,
                thrust_ratio,
                data,
            );
        }
    }
}

// ── Detection system ─────────────────────────────────────────────────────────

/// Maximum simulation seconds a contact lingers after target leaves detection range.
const CONTACT_LINGER_S: f64 = 3.0;

/// Process sensor detection for all sensor-equipped fleets.
///
/// Systems:
/// 1. Query sensor-equipped fleets (entities with Fleet + SensorSuite)
/// 2. Query all potential targets (entities with Fleet)
/// 3. For each sensor-target pair within range:
///    - Fetch target's signature from its Fleet.ships
///    - Run detection check against effective signature
///    - Create or update Contact in the fleet's ContactRecords
///    - Contacts whose target leaves detection range for >CONTACT_LINGER_S are removed
/// 4. Two-range: detection_range for contact creation/tracking, id_range for identification
pub fn sensor_detection_system(
    sim_time: Res<SimulationTime>,
    mut sensor_fleet_query: Query<
        (Entity, &mut Fleet, &SpaceCoordinates),
        With<SensorSuite>,
    >,
    target_query: Query<(Entity, &Fleet, &SpaceCoordinates)>,
) {
    let elapsed = sim_time.elapsed_seconds();
    let dt = 1.0 / 60.0; // ~1 frame tick

    // Track which targets each sensor fleet can see this frame: (sensor_entity, target_entity)
    let mut seen: Vec<(Entity, Entity)> = Vec::new();

    for (sensor_entity, mut sensor_fleet, sensor_pos) in sensor_fleet_query.iter_mut() {
        // Aggregate best sensor across all ships in this fleet
        let (best_detection_km, best_id_km, best_strength, is_neutrino, _has_active) =
            best_fleet_sensor(&sensor_fleet);

        let detection_range_km = best_detection_km;
        let id_range_km = best_id_km;

        for (target_entity, target_fleet, target_pos) in target_query.iter() {
            if target_entity == sensor_entity {
                // Can't detect yourself
                continue;
            }

            let diff = sensor_pos.position - target_pos.position;
            let dist_au_sq = diff.length_squared();
            let dist_au = dist_au_sq.sqrt();
            let dist_km = (dist_au * AU_IN_KM) as f32;

            // Primary detection range check
            if dist_km > detection_range_km {
                continue;
            }

            // Get fleet's aggregate signature (sum across ships)
            let target_sig = aggregate_fleet_signature(target_fleet);
            let effective_sig = target_sig.effective_for(is_neutrino);

            if effective_sig <= 0.0 {
                continue;
            }

            // Run detection check with 1/AU² distance falloff
            let detection_delta = detection_check(best_strength, effective_sig, dist_au, dt);

            let in_id_range = dist_km <= id_range_km;

            if detection_delta > 0.0 {
                seen.push((sensor_entity, target_entity));

                let target_name = target_fleet.name.clone();
                let friendly = is_friendly(target_fleet);

                let contact = sensor_fleet
                    .contacts
                    .entry(target_entity)
                    .or_insert_with(|| {
                        Contact::new(
                            target_entity,
                            target_name,
                            target_sig,
                            elapsed,
                            friendly,
                            in_id_range,
                        )
                    });

                // Update existing contact
                contact.last_signature = target_sig;
                contact.last_detection_time = elapsed;
                contact.tracking_pct =
                    (contact.tracking_pct + detection_delta).min(100.0);
                contact.friendly = friendly;
                contact.in_id_range = in_id_range;
                contact.accumulate_tracking(dt);
                contact.update_state();
            } else if in_id_range {
                // Within ID range but didn't get detection delta this tick — still in contact
                seen.push((sensor_entity, target_entity));

                if let Some(contact) = sensor_fleet.contacts.get_mut(&target_entity) {
                    contact.last_detection_time = elapsed;
                    contact.in_id_range = true;
                    contact.update_state();
                }
            }
        }

        // Remove contacts whose targets have left detection range for >CONTACT_LINGER_S
        sensor_fleet.contacts.retain(|target_entity, contact| {
            let is_seen = seen.contains(&(sensor_entity, *target_entity));
            if is_seen {
                true
            } else {
                // Keep if within linger window
                elapsed - contact.last_detection_time < CONTACT_LINGER_S
            }
        });
    }
}

/// Return the best combined sensor values across all ships in a fleet.
fn best_fleet_sensor(fleet: &Fleet) -> (f32, f32, f32, bool, bool) {
    let mut best_detection = 0.0_f32;
    let mut best_id = 0.0_f32;
    let mut best_strength = 0.0_f32;
    let mut neutrino = false;
    let mut has_active = false;

    for ship in &fleet.ships {
        if let Some(ref suite) = ship.sensor_suite {
            if suite.detection_range_km > best_detection {
                best_detection = suite.detection_range_km;
                best_id = suite.id_range_km;
                best_strength = suite.strength;
                neutrino = suite.neutrino;
            }
            if suite.is_active {
                has_active = true;
            }
        }
    }

    (best_detection, best_id, best_strength, neutrino, has_active)
}

/// Aggregate signatures across all ships in a fleet (maximum per band).
fn aggregate_fleet_signature(fleet: &Fleet) -> Signature {
    let mut total = Signature::default();

    for ship in &fleet.ships {
        total.thermal += ship.signature.thermal;
        total.em += ship.signature.em;
        total.visual += ship.signature.visual;
        total.neutrino += ship.signature.neutrino;
    }

    total
}

/// Heuristic: all player fleets are considered friendly.
fn is_friendly(_fleet: &Fleet) -> bool {
    true // TODO: faction system
}

// ── Contact tracking system ───────────────────────────────────────────────────

/// Update contact tracking quality for all contacts in a fleet.
///
/// Runs after sensor_detection_system to apply +10% per second tracking quality
/// accumulation and state transitions.
pub fn update_contact_tracking(
    sim_time: Res<SimulationTime>,
    mut fleet_query: Query<&mut Fleet, With<SensorSuite>>,
) {
    let elapsed = sim_time.elapsed_seconds();

    for mut fleet in fleet_query.iter_mut() {
        for contact in fleet.contacts.values_mut() {
            let dt = (elapsed - contact.last_detection_time).max(0.0);
            contact.accumulate_tracking(dt);
            contact.last_detection_time = elapsed;
            contact.update_state();
        }
    }
}

// ── Active sensor ping system ─────────────────────────────────────────────────

/// Active sensor ping reveal: when a fleet with active sensors pings,
/// enemy fleets within `effective_ping_radius` are immediately revealed to the pinging fleet.
///
/// Design: ping detected within `ping_range × 0.8` by enemy EM receivers (i.e.,
/// the enemy detects the ping on their EM receivers and is therefore revealed).
pub fn active_sensor_ping_system(
    sim_time: Res<SimulationTime>,
    sensor_fleet_query: Query<
        (Entity, &Fleet, &SpaceCoordinates),
        With<SensorSuite>,
    >,
    mut fleet_query: Query<&mut Fleet, With<SensorSuite>>,
) {
    let elapsed = sim_time.elapsed_seconds();
    let dt = 1.0 / 60.0;

    // Collect all fleets with active sensors and their ping ranges
    let pingers: Vec<(Entity, DVec3, f32)> = sensor_fleet_query
        .iter()
        .filter_map(|(entity, fleet, pos)| {
            let ping_range_km = fleet_active_ping_range(fleet);
            if ping_range_km > 0.0 {
                Some((entity, pos.position, ping_range_km))
            } else {
                None
            }
        })
        .collect();

    if pingers.is_empty() {
        return;
    }

    // For each fleet with mutable access, check if any active pinger is in range
    for (sensor_entity, sensor_fleet, sensor_pos) in sensor_fleet_query.iter() {
        let sensor_pos = sensor_pos.position;

        for (pinger_entity, pinger_pos, ping_range_km) in &pingers {
            if *pinger_entity == sensor_entity {
                continue;
            }

            let diff = pinger_pos - sensor_pos;
            let dist_au = diff.length();
            let dist_km = (dist_au * AU_IN_KM) as f32;
            let effective_radius = ping_range_km * 0.8;

            if dist_km <= effective_radius {
                // Enemy ping detected within effective range — reveal the pinger to this fleet
                if let Ok(mut fleet) = fleet_query.get_mut(sensor_entity) {
                    let target_sig = aggregate_fleet_signature(sensor_fleet);
                    let target_name = sensor_fleet.name.clone();
                    let friendly = is_friendly(sensor_fleet);

                    let contact = fleet
                        .contacts
                        .entry(*pinger_entity)
                        .or_insert_with(|| {
                            Contact::new(
                                *pinger_entity,
                                target_name,
                                target_sig,
                                elapsed,
                                friendly,
                                true,
                            )
                        });

                    contact.last_detection_time = elapsed;
                    contact.tracking_pct = (contact.tracking_pct + 50.0).min(100.0);
                    contact.state = ContactState::Identified;
                    contact.in_id_range = true;
                    contact.accumulate_tracking(dt);
                    contact.update_state();
                }
            }
        }
    }
}

/// Return the best active ping range across all ships in a fleet (0 if none).
fn fleet_active_ping_range(fleet: &Fleet) -> f32 {
    let mut best = 0.0_f32;
    for ship in &fleet.ships {
        if let Some(ref suite) = ship.sensor_suite {
            if suite.is_active && suite.detection_range_km > best {
                // Use the detection range as a proxy for ping range strength
                // (each tier's active_ping_range_km is proportional)
                best = suite.detection_range_km.max(best);
            }
        }
    }
    best
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::components::ShipInfo;
    use crate::fleets::types::{PropulsionType, ShipClass};
    use crate::sensors::components::{ActiveSensor, ContactState, StealthMode};
    use crate::sensors::data::SensorData;

    // ── Signature ─────────────────────────────────────────────────────────────

    #[test]
    fn signature_effective_for_normal_sensor() {
        let sig = Signature {
            thermal: 1.0,
            em: 0.5,
            visual: 0.3,
            neutrino: 0.0,
        };
        // Normal sensors see thermal+EM+visual
        assert_eq!(sig.effective_for(false), 1.8);
    }

    #[test]
    fn signature_effective_for_neutrino_sensor() {
        let sig = Signature {
            thermal: 1.0,
            em: 0.5,
            visual: 0.3,
            neutrino: 2.0,
        };
        // Neutrino sensors see only neutrino band
        assert_eq!(sig.effective_for(true), 2.0);
    }

    // ── Detection check ───────────────────────────────────────────────────────

    #[test]
    fn detection_check_zero_signature() {
        let result = detection_check(1.0, 0.0, 1.0, 1.0 / 60.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn detection_check_zero_distance() {
        let result = detection_check(1.0, 1.0, 0.0, 1.0 / 60.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn detection_check_positive() {
        // sensor_strength=1.0, sig=1.0, dist=1.0 AU, dt=1 frame
        let result = detection_check(1.0, 1.0, 1.0, 1.0 / 60.0);
        // factor = 1.0 / (1.0 * 1.0²) = 1.0; time_factor = 1/60
        // result = 1.0 * 1/60 * 100 ≈ 1.67
        assert!((result - 1.66667).abs() < 0.01);
    }

    #[test]
    fn detection_check_distance_falloff() {
        // Same parameters but at 2 AU (4× farther)
        let near = detection_check(1.0, 1.0, 1.0, 1.0 / 60.0);
        let far = detection_check(1.0, 1.0, 2.0, 1.0 / 60.0);
        // far should be 1/4 of near due to 1/AU² falloff
        assert!((far * 4.0 - near).abs() < 0.01);
    }

    #[test]
    fn detection_check_clamps_at_100() {
        // Very strong sensor at close range
        let result = detection_check(1000.0, 1.0, 0.01, 1.0);
        assert_eq!(result, 100.0);
    }

    // ── Contact state machine ──────────────────────────────────────────────────

    #[test]
    fn contact_starts_unexplained() {
        let contact = Contact::new(
            Entity::PLACEHOLDER,
            "Test".into(),
            Signature::default(),
            0.0,
            false,
            false,
        );
        assert_eq!(contact.state, ContactState::Unexplained);
        assert_eq!(contact.tracking_pct, 1.0);
    }

    #[test]
    fn contact_update_state_unexplained_to_detected() {
        let mut contact = Contact::new(
            Entity::PLACEHOLDER,
            "Test".into(),
            Signature::default(),
            0.0,
            false,
            false,
        );
        contact.tracking_pct = 5.0;
        contact.update_state();
        assert_eq!(contact.state, ContactState::Detected);
    }

    #[test]
    fn contact_update_state_detected_at_100_is_locked() {
        let mut contact = Contact::new(
            Entity::PLACEHOLDER,
            "Test".into(),
            Signature::default(),
            0.0,
            false,
            false,
        );
        contact.tracking_pct = 100.0;
        contact.state = ContactState::Detected;
        contact.update_state();
        assert_eq!(contact.state, ContactState::Locked);
    }

    #[test]
    fn contact_update_state_stays_detected_below_100() {
        let mut contact = Contact::new(
            Entity::PLACEHOLDER,
            "Test".into(),
            Signature::default(),
            0.0,
            false,
            false,
        );
        contact.state = ContactState::Detected;
        contact.tracking_pct = 50.0;
        contact.update_state();
        // 50% is still Detected, not Identified (Identified = 100%)
        assert_eq!(contact.state, ContactState::Detected);
    }

    #[test]
    fn contact_tracking_quality_accumulates() {
        let mut contact = Contact::new(
            Entity::PLACEHOLDER,
            "Test".into(),
            Signature::default(),
            0.0,
            false,
            false,
        );
        contact.accumulate_tracking(1.0); // 1 second
        assert!((contact.tracking_quality - 10.0).abs() < 0.001);
        contact.accumulate_tracking(2.0); // 2 more seconds
        assert!((contact.tracking_quality - 30.0).abs() < 0.001);
    }

    // ── Signature calculation ─────────────────────────────────────────────────

    #[test]
    fn calculate_signature_full_power() {
        let sig = calculate_signature(
            ShipClass::Frigate,
            StealthMode::FullPower,
            1.0,
            None,
        );
        // Default Frigate: thermal=1.0, em=0.5, visual=0.3, neutrino=0.0
        // Full power: emission_mult=1.0, engine_thermal = 1.0 * 1² = 1.0
        // thermal = 1.0 + 1.0 * 1.0 = 2.0
        assert!((sig.thermal - 2.0).abs() < 0.01);
        assert!((sig.em - 0.5).abs() < 0.01);
        assert!((sig.visual - 0.3).abs() < 0.01);
    }

    #[test]
    fn calculate_signature_running_silent() {
        let sig = calculate_signature(
            ShipClass::Frigate,
            StealthMode::RunningSilent,
            1.0,
            None,
        );
        // Running Silent: emission_mult=0.3
        // engine_thermal = 1.0 * 1² = 1.0
        // thermal = 1.0 + 1.0 * 0.3 = 1.3
        assert!((sig.thermal - 1.3).abs() < 0.01);
        assert!((sig.em - 0.15).abs() < 0.01); // 0.5 * 0.3
        assert!((sig.visual - 0.09).abs() < 0.01); // 0.3 * 0.3
    }

    #[test]
    fn calculate_signature_dark_drive() {
        let sig = calculate_signature(
            ShipClass::Cruiser,
            StealthMode::DarkDrive,
            0.5,
            None,
        );
        // Dark Drive: emission_mult=0.1
        // Default Cruiser: thermal=8.0
        // engine_thermal = 8.0 * 0.5² = 2.0
        // thermal = 8.0 + 2.0 * 0.1 = 8.2
        assert!((sig.thermal - 8.2).abs() < 0.01);
    }

    // ── Stealth mode emission multipliers ─────────────────────────────────────

    #[test]
    fn stealth_mode_multipliers() {
        assert!((StealthMode::FullPower.emission_multiplier() - 1.0).abs() < 0.001);
        assert!((StealthMode::RunningSilent.emission_multiplier() - 0.3).abs() < 0.001);
        assert!((StealthMode::DarkDrive.emission_multiplier() - 0.1).abs() < 0.001);
        assert!((StealthMode::Hidden.emission_multiplier() - 0.05).abs() < 0.001);
    }

    // ── Sensor suite ──────────────────────────────────────────────────────────

    #[test]
    fn sensor_suite_detection_factor() {
        let suite = SensorSuite {
            tier_id: "test".into(),
            detection_range_km: 50_000.0,
            id_range_km: 10_000.0,
            strength: 1.0,
            neutrino: false,
            is_active: true,
        };
        // factor = strength / distance² = 1.0 / (50000²)
        let factor = suite.detection_factor(50_000.0);
        assert!((factor - 1.0 / 2_500_000_000.0).abs() < 0.0001);
    }

    // ── Active sensor ping radius ─────────────────────────────────────────────

    #[test]
    fn active_sensor_effective_ping_radius() {
        let active = ActiveSensor {
            ping_range_km: 10_000.0,
        };
        assert!((active.effective_ping_radius() - 8_000.0).abs() < 0.01);
    }

    // ── Aggregate fleet signature ─────────────────────────────────────────────

    #[test]
    fn aggregate_fleet_signature_sums_all_bands() {
        let mut fleet = Fleet::new("Test Fleet".into());
        let ship1 = ShipInfo::new("Ship1".into(), ShipClass::Frigate, PropulsionType::Chemical);
        let ship2 = ShipInfo::new("Ship2".into(), ShipClass::Cruiser, PropulsionType::FusionTorch);
        fleet.ships.push(ship1);
        fleet.ships.push(ship2);

        let sig = aggregate_fleet_signature(&fleet);
        // Both ships have non-zero signatures
        assert!(sig.thermal > 0.0);
        assert!(sig.em > 0.0);
        assert!(sig.visual > 0.0);
    }
}
