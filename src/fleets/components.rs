//! ECS components for the fleet management and orbital transfer system.

use super::types::{FleetRole, PropulsionType, ShipClass};
use crate::astronomy::KeplerOrbit;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Summary information about a single ship within a fleet.
#[derive(Debug, Clone, Reflect)]
pub struct ShipInfo {
    /// Name of the ship.
    pub name: String,
    /// Ship class (Frigate, Destroyer, etc.).
    pub class: ShipClass,
    /// Optional hull definition id (e.g. `micro_probe_frame`).  When `Some`,
    /// the ship's dry mass and slot layout come from the matching
    /// `ShipHullDefinition` rather than from the class default.  Day-1
    /// spawns (`spawn_initial_fleet`, GRA-128) set this for every ship so
    /// the fleet honours the tier-1 hull spec from `assets/data/ship_hulls.ron`.
    /// `None` for class-default construction paths (auto-freight templates,
    /// pre-save ship records that pre-date the field, etc.).
    pub hull_id: Option<String>,
    /// Dry mass without propellant (tonnes).
    pub dry_mass_t: f32,
    /// Current propellant mass (tonnes).
    pub fuel_mass_t: f32,
    /// Maximum propellant capacity (tonnes).
    pub max_fuel_t: f32,
    /// Engine thrust in kilonewtons.
    pub thrust_kn: f32,
    /// Engine specific impulse (seconds).
    pub isp_s: f32,
    /// Propulsion type.
    pub propulsion: PropulsionType,
    /// Maximum cargo payload (tonnes) this ship can carry in a single
    /// freight trip.  Zero for non-freighter classes.  Populated by
    /// `sync_fleet_cache_from_ship_entities` for entities that carry a
    /// `ShipTemplateRef` + `FreighterSlots` (the legacy freighter
    /// migration in `src/ships/migration.rs` adds those components).
    /// GRA-119.
    pub cargo_capacity_t: f64,
}

impl ShipInfo {
    /// Create a ship with typical parameters for its class and propulsion type.
    ///
    /// Thin wrapper around [`ShipInfo::new_with_dry_mass`] that uses the
    /// class's default dry mass.  `hull_id` defaults to `None` so the
    /// existing class-only construction paths (auto-freight templates,
    /// pre-GRA-128 callers) keep working unchanged.  GRA-128.
    pub fn new(name: String, class: ShipClass, propulsion: PropulsionType) -> Self {
        Self::new_with_dry_mass(name, None, class, propulsion, class.default_dry_mass_t())
    }

    /// Create a ship with a hull-aware dry mass.
    ///
    /// `hull_id` is recorded on the `ShipInfo` (so the constructor round-trip
    /// persists which RON hull spec produced the ship), and the resolved
    /// `dry_mass_t` (typically `ShipHullDefinition::base_dry_mass_t` from the
    /// `ShipbuildingData` registry) overrides the class default.  Fuel load
    /// is recomputed from `class.default_fuel_fraction()` against the new dry
    /// mass; thrust uses `propulsion.thrust_kn(dry_mass)` and Isp uses
    /// `propulsion.isp_s()`.  Pass `class.default_dry_mass_t()` for a
    /// class-only ship.  GRA-128.
    pub fn new_with_dry_mass(
        name: String,
        hull_id: Option<&str>,
        class: ShipClass,
        propulsion: PropulsionType,
        dry_mass_t: f32,
    ) -> Self {
        // fuel_fraction is of total wet mass: fuel = dry * frac/(1-frac)
        let fuel_frac = class.default_fuel_fraction();
        let fuel_mass = dry_mass_t * fuel_frac / (1.0 - fuel_frac);
        Self {
            name,
            hull_id: hull_id.map(|s| s.to_string()),
            class,
            dry_mass_t,
            fuel_mass_t: fuel_mass,
            max_fuel_t: fuel_mass,
            thrust_kn: propulsion.thrust_kn(dry_mass_t),
            isp_s: propulsion.isp_s(),
            propulsion,
            cargo_capacity_t: 0.0,
        }
    }

    /// Total (wet) mass including propellant.
    pub fn wet_mass_t(&self) -> f32 {
        self.dry_mass_t + self.fuel_mass_t
    }

    /// Current propellant fill level as a fraction 0..1.
    pub fn fuel_fraction(&self) -> f32 {
        if self.max_fuel_t <= 0.0 {
            return 0.0;
        }
        self.fuel_mass_t / self.max_fuel_t
    }

    /// Maximum achievable Δv for this ship alone (m/s), using the Tsiolkovsky
    /// rocket equation with this ship's current fuel level and Isp.
    pub fn delta_v_ms(&self) -> f64 {
        use super::orbital_mechanics::G0;
        let dry = self.dry_mass_t as f64;
        let wet = self.wet_mass_t() as f64;
        if dry <= 0.0 || wet <= dry {
            return 0.0;
        }
        self.isp_s as f64 * G0 * (wet / dry).ln()
    }
}

/// Authoritative ship entity used by construction, assignment, and fleet caching.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct ShipInstance {
    /// Full ship performance and mass data.
    pub info: ShipInfo,
    /// Current body the ship is parked around when not in transit.
    pub parked_body: Entity,
    /// Parking orbit radius in AU.
    pub parked_orbit_radius_au: f64,
    /// Whether the ship should remain fixed like a station.
    pub stationary: bool,
    /// Fleet this ship is currently assigned to, if any.
    pub assigned_fleet: Option<Entity>,
    /// Stable ordering inside the assigned fleet cache.
    pub sort_order: i32,
}

impl ShipInstance {
    /// Create a new ship entity from a fleet-facing ship record.
    pub fn new(
        info: ShipInfo,
        parked_body: Entity,
        parked_orbit_radius_au: f64,
        stationary: bool,
        assigned_fleet: Option<Entity>,
        sort_order: i32,
    ) -> Self {
        Self {
            info,
            parked_body,
            parked_orbit_radius_au,
            stationary,
            assigned_fleet,
            sort_order,
        }
    }

    /// Return a fleet-cache copy of this ship.
    pub fn as_ship_info(&self) -> ShipInfo {
        self.info.clone()
    }
}

/// A named collection of ships orbiting (or transferring between) celestial bodies.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct Fleet {
    /// Display name for this fleet.
    pub name: String,
    /// The assigned role of the fleet.
    pub role: FleetRole,
    /// Ships that make up this fleet.
    pub ships: Vec<ShipInfo>,
}

impl Fleet {
    /// Create an empty fleet with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            role: FleetRole::default(),
            ships: Vec::new(),
        }
    }

    /// Total dry mass of all ships (tonnes).
    pub fn total_dry_mass_t(&self) -> f32 {
        self.ships.iter().map(|s| s.dry_mass_t).sum()
    }

    /// Total current propellant mass (tonnes).
    pub fn total_fuel_t(&self) -> f32 {
        self.ships.iter().map(|s| s.fuel_mass_t).sum()
    }

    /// Total wet mass of all ships (tonnes).
    pub fn total_wet_mass_t(&self) -> f32 {
        self.ships.iter().map(|s| s.wet_mass_t()).sum()
    }

    /// Total cargo capacity (tonnes) summed over every ship in the fleet
    /// (GRA-119).  Zero for fleets with no freighters or for ships whose
    /// `ShipTemplateRef` / `FreighterSlots` haven't been resolved yet by
    /// `sync_fleet_cache_from_ship_entities`.  The auto-freight loop and
    /// the manual-assign path in `economy::logistics` cap a single
    /// delivery's `in_transit_mt` at this value.
    pub fn total_cargo_capacity_t(&self) -> f64 {
        self.ships.iter().map(|s| s.cargo_capacity_t).sum()
    }

    /// Thrust-weighted average specific impulse of the fleet's engines (seconds).
    pub fn average_isp_s(&self) -> f32 {
        let total_thrust: f32 = self.ships.iter().map(|s| s.thrust_kn).sum();
        if total_thrust <= 0.0 {
            return 450.0;
        }
        self.ships
            .iter()
            .map(|s| s.thrust_kn * s.isp_s)
            .sum::<f32>()
            / total_thrust
    }

    /// Minimum thrust of any ship in the fleet (kilonewtons).
    pub fn min_thrust_kn(&self) -> f32 {
        self.ships
            .iter()
            .map(|s| s.thrust_kn)
            .reduce(f32::min)
            .unwrap_or(0.0)
    }

    /// Maximum achievable Δv for the entire fleet (m/s).
    ///
    /// A fleet's Δv capacity is limited by the ship with the **lowest** individual
    /// Δv — every ship must complete the maneuver, so the weakest ship determines
    /// what the fleet can achieve.
    pub fn max_delta_v_ms(&self) -> f64 {
        if self.ships.is_empty() {
            return 0.0;
        }
        self.ships
            .iter()
            .map(|s| s.delta_v_ms())
            .fold(f64::INFINITY, f64::min)
    }

    /// Total propellant consumed (tonnes) across all ships to perform `delta_v_ms`.
    ///
    /// Each ship's fuel cost is computed individually from its own Isp and wet
    /// mass via the Tsiolkovsky rocket equation, then summed.
    pub fn total_fuel_cost_for_dv(&self, delta_v_ms: f64) -> f32 {
        use super::orbital_mechanics::estimate_fuel_cost_tonnes;
        self.ships
            .iter()
            .map(|s| estimate_fuel_cost_tonnes(s.wet_mass_t(), s.isp_s, delta_v_ms))
            .sum()
    }

    /// Minimum Δv (m/s) available after deducting `abort_fuel_t` evenly across
    /// all ships.
    ///
    /// Used to pre-check whether a course correction is feasible once the abort
    /// burn penalty has been paid.
    pub fn min_delta_v_after_abort(&self, abort_fuel_t: f32) -> f64 {
        if self.ships.is_empty() {
            return 0.0;
        }
        let n = self.ships.len() as f32;
        let per_ship = abort_fuel_t / n;
        self.ships
            .iter()
            .map(|s| {
                use super::orbital_mechanics::G0;
                let dry = s.dry_mass_t as f64;
                let fuel_after = (s.fuel_mass_t - per_ship).max(0.0) as f64;
                let wet_after = dry + fuel_after;
                if wet_after > dry {
                    s.isp_s as f64 * G0 * (wet_after / dry).ln()
                } else {
                    0.0
                }
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Fuel fill level as a fraction 0..1.
    pub fn fuel_fraction(&self) -> f32 {
        let max: f32 = self.ships.iter().map(|s| s.max_fuel_t).sum();
        if max <= 0.0 {
            return 0.0;
        }
        self.total_fuel_t() / max
    }

    /// Minimum fleet acceleration (m/s²) — the weakest ship in the fleet
    /// determines how quickly the entire fleet can change velocity.
    ///
    /// Each ship's specific acceleration is `thrust_kn / wet_mass_t` (kN/t = m/s²).
    /// The fleet can only maneuver as fast as the ship with the lowest value.
    ///
    /// Returns 0.0 for an empty fleet or if all ships have zero mass.
    pub fn min_accel_ms2(&self) -> f64 {
        self.ships
            .iter()
            .map(|s| {
                let wm = s.wet_mass_t();
                if wm <= 0.0 {
                    0.0_f64
                } else {
                    (s.thrust_kn / wm) as f64
                }
            })
            .reduce(f64::min)
            .unwrap_or(0.0)
    }

    /// True if the fleet contains at least one ship of `ShipClass::Freighter`.
    ///
    /// Used to gate logistics actions and the in-transit visibility filter
    /// (GRA-37.d / GRA-41).  Combat / survey / transport-without-freighter
    /// fleets are unaffected by the visibility toggle.
    pub fn has_freighter_ship(&self) -> bool {
        self.ships.iter().any(|s| s.class == ShipClass::Freighter)
    }

    /// Coarse 3-bucket classification for the trajectory overlay filter
    /// (GRA-154 M-7).  Returns the first non-civilian bucket found, or
    /// `Civilian` if every ship is civilian.  An empty fleet defaults to
    /// `Civilian`.
    pub fn fleet_class(&self) -> super::types::FleetClass {
        use super::types::FleetClass;
        let mut freighter = false;
        let mut combat = false;
        for s in &self.ships {
            match s.class.fleet_class() {
                FleetClass::Freighter => freighter = true,
                FleetClass::Combat => combat = true,
                FleetClass::Civilian => {}
            }
            if freighter {
                return FleetClass::Freighter;
            }
            if combat {
                return FleetClass::Combat;
            }
        }
        FleetClass::Civilian
    }
}

/// Stable circular parking orbit for a fleet around a celestial body.
///
/// Updated each frame by `update_fleet_orbit_positions`.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct FleetOrbit {
    /// The celestial body this fleet orbits.
    pub body: Entity,
    /// Orbit radius in AU from the body's centre.
    pub radius_au: f64,
    /// Current visual orbital angle in radians (in the ecliptic plane).
    /// Advanced by `update_fleet_orbit_positions` at a gameplay-friendly rate,
    /// not physics-accurate angular velocity.
    pub angle_rad: f64,
    /// Visual orbit direction: +1.0 = CCW (prograde), -1.0 = CW (retrograde).
    /// Derived from the actual Keplerian velocity direction at insertion so that
    /// the parking-orbit icon continues in the same direction as the arrival arc.
    pub direction: f64,
}

impl FleetOrbit {
    /// Create a prograde (CCW) circular orbit around `body` at `radius_au` astronomical units.
    pub fn new(body: Entity, radius_au: f64) -> Self {
        Self {
            body,
            radius_au,
            angle_rad: 0.0,
            direction: 1.0,
        }
    }
}

/// An active Keplerian transfer arc being executed by a fleet.
///
/// While present on an entity, `update_fleet_maneuver_positions` drives the
/// fleet's `SpaceCoordinates` along the transfer ellipse each frame.
/// `complete_fleet_maneuvers` removes this component when `arrival_time` is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum TransferReferenceFrame {
    /// The transfer is expressed relative to a real body entity.
    Body(Entity),
    /// The transfer is expressed in the current system barycentric frame.
    SystemBarycentric,
}

impl TransferReferenceFrame {
    pub fn body(self) -> Option<Entity> {
        match self {
            Self::Body(entity) => Some(entity),
            Self::SystemBarycentric => None,
        }
    }

    pub fn is_barycentric(self) -> bool {
        matches!(self, Self::SystemBarycentric)
    }
}

#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct ActiveManeuver {
    /// Keplerian orbit describing the transfer arc.
    ///
    /// `mean_anomaly_epoch` is the mean anomaly at `departure_time`.
    /// `mean_motion` is the full orbital mean motion (rad/s).
    pub transfer_orbit: KeplerOrbit,
    /// Reference frame used by planning, preview rendering, and in-flight updates.
    pub reference_frame: TransferReferenceFrame,
    /// Entity of the star (or central body) the transfer orbit is centred on.
    pub orbit_center: Entity,
    /// Entity of the body the fleet departed from (used for visual arc rendering).
    pub origin_body: Entity,
    /// `SimulationTime.elapsed` at the moment of departure.
    pub departure_time: f64,
    /// `SimulationTime.elapsed` when the fleet is expected to arrive.
    pub arrival_time: f64,
    /// Preserve the precomputed orbit exactly at departure instead of refitting
    /// it from the origin body's instantaneous angle.
    pub preserve_orbit_geometry: bool,
    /// Destination body entity.
    pub destination_body: Entity,
    /// Orbital radius the fleet will enter around the destination (AU).
    pub arrival_orbit_radius_au: f64,
    /// Arrival circularisation burn Δv (m/s) — stored for display purposes.
    pub arrival_delta_v_ms: f64,
    /// Estimated propellant consumed by the maneuver (tonnes).
    pub fuel_used_t: f32,
    /// Label of the transfer option chosen by the player.
    pub option_label: &'static str,
    /// Visual orbit angle (radians) of the fleet on its parking ring at the moment
    /// of departure — retained for diagnostics.  Local transfer arcs in System view
    /// derive the departure direction dynamically from the origin→destination geometry,
    /// so this field does not affect the rendered trajectory.
    pub departure_angle: f32,
    /// The physics position (AU) of the origin body at departure time.
    /// Used for kinematic transfers.
    pub start_position_au: Option<bevy::math::DVec3>,
    /// The physics position (AU) of the destination body at arrival time.
    /// Used for kinematic transfers.
    pub end_position_au: Option<bevy::math::DVec3>,
    /// Lambert departure velocity (m/s) for barycentric curved transfers.
    ///
    /// Used only by the visual systems to shape the preview and transit arc.
    pub departure_velocity_ms: Option<bevy::math::DVec3>,
    /// Lambert arrival velocity (m/s) for barycentric curved transfers.
    ///
    /// Used only by the visual systems to shape the preview and transit arc.
    pub arrival_velocity_ms: Option<bevy::math::DVec3>,
    /// The visual position of the fleet at the moment of departure.
    /// Used for course corrections to prevent the visual arc from jumping back to the origin body.
    pub start_visual_pos: Option<bevy::math::Vec3>,
    /// Optional second-leg Keplerian orbit for gravity-assist transfers (flyby → destination).
    ///
    /// When set, `update_fleet_maneuver_positions` follows `transfer_orbit` until
    /// `leg2_start_s` seconds after departure, then switches to this orbit.
    pub leg2_orbit: Option<KeplerOrbit>,
    /// Seconds after departure when the Leg-2 orbit begins (= Leg-1 half-period).
    /// Only meaningful when `leg2_orbit` is `Some`.
    pub leg2_start_s: f64,
    /// For gravity-assist transfers, the body that will be used for the flyby.
    ///
    /// This is set when a `PlannedTransfer` containing a two‑leg assist is built,
    /// and propagated into the corresponding `ActiveManeuver`.  The render
    /// systems use it to reconstruct the two‑leg trajectory after execution.
    pub flyby_body: Option<Entity>,
    /// GRA-153 H-3 (Kilo CRITICAL 2): when `true`, force `is_kinematic()`
    /// to return `true` regardless of `option_label`.  Set by
    /// `process_fleet_actions` for mid-transit course corrections so the
    /// propagation actually uses the refreshed `start_position_au` /
    /// `end_position_au` instead of the stale Keplerian orbit.
    pub kinematic_override: bool,
}

impl ActiveManeuver {
    /// Whether this transfer uses kinematic (straight-line) interpolation rather
    /// than Keplerian orbit propagation.
    ///
    /// Kinematic transfers include full-thrust, coast phases, max-speed runs,
    /// and direct L1/L2 Lagrange-point transfers.
    pub fn is_kinematic(&self) -> bool {
        self.kinematic_override
            || self.option_label == "Full Thrust"
            || self.option_label.contains("Coast")
            || self.option_label == "Max Speed"
            || self.option_label.contains("Direct")
            // GRA-153 M-3: Abort to Origin is propagated as a linear
            // interpolation from the fleet's current heliocentric position
            // (start_position_au) to the origin body's predicted position at
            // arrival (end_position_au).  Treating it as a Keplerian transfer
            // would re-fly the original Hohmann orbit, which is the bug
            // class this maneuver exists to avoid.
            || self.option_label == "Abort to Origin"
    }

    /// Fractional progress of the transfer arc, clamped to \[0, 1\].
    pub fn progress(&self, current_sim_time: f64) -> f64 {
        let elapsed = current_sim_time - self.departure_time;
        let duration = self.arrival_time - self.departure_time;
        if duration <= 0.0 {
            1.0
        } else {
            (elapsed / duration).clamp(0.0, 1.0)
        }
    }

    /// Simulation-time seconds remaining until arrival.
    pub fn time_remaining_s(&self, current_sim_time: f64) -> f64 {
        (self.arrival_time - current_sim_time).max(0.0)
    }

    /// Whether the fleet has reached its destination.
    pub fn is_complete(&self, current_sim_time: f64) -> bool {
        current_sim_time >= self.arrival_time
    }
}

// ── Action queues ─────────────────────────────────────────────────────────────

/// Resource holding fleet management actions queued by the UI, to be executed
/// in the `Update` schedule where ECS mutation is safe.
#[derive(Resource, Default)]
pub struct PendingFleetActions {
    /// Requests to spawn new fleets from a launch site.
    pub spawn_fleets: Vec<SpawnFleetAction>,
    /// Requests to begin an orbital transfer.
    pub start_transfers: Vec<StartTransferAction>,
    /// Fleet entities whose active maneuver should be aborted.
    pub cancel_maneuvers: Vec<Entity>,
    /// Fleets to refuel — fills all ships to their maximum propellant capacity.
    /// In the future this will draw propellant from the location's resource stockpile.
    pub refuel_fleets: Vec<Entity>,
    /// Individual ships to refuel to full capacity.
    pub refuel_ships: Vec<(Entity, usize)>,
    /// Requests to rename a fleet.
    pub rename_fleets: Vec<(Entity, String)>,
    /// Requests to change a fleet's role.
    pub change_fleet_roles: Vec<(Entity, FleetRole)>,
    /// Requests to transfer ships between fleets.
    pub transfer_ships: Vec<TransferShipsAction>,
    /// Requests to assign concrete ship entities directly to fleets.
    pub assign_ships: Vec<AssignShipsAction>,
    /// Requests to create a new fleet and immediately attach ships to it.
    pub create_fleets_from_ships: Vec<CreateFleetFromShipsAction>,
    /// Requests to scrap individual ships.
    pub scrap_ships: Vec<(Entity, usize)>,
    /// Requests to disband fleets (confirmed by the player).
    pub disband_fleets: Vec<Entity>,
    /// Requests to merge several fleets into one.
    pub merge_fleets: Vec<MergeFleetAction>,
    /// Requests to manually assign a freighter fleet to a logistics
    /// `ResourceRequest` (GRA-33 / PR-B player-agency layer).  The fleet must be
    /// in orbit at the request's destination body and contain at least one
    /// `ShipClass::Freighter`.  Consumed by `process_fleet_logistics_assignments`
    /// in `economy::logistics`.
    pub assign_logistics_requests: Vec<AssignLogisticsRequestAction>,
    /// Requests to abort a mid-transit maneuver and return the fleet to a
    /// parking orbit at its origin body.  Distinct from `cancel_maneuvers`
    /// (which silently dissolves the fleet) and from `disband_fleets` (which
    /// requires confirmation).  Consumed by `process_fleet_actions` in
    /// `fleets::systems`.  GRA-153 M-3.
    pub abort_to_origin: Vec<AbortToOriginAction>,
}

/// Merge two or more fleets: all ships from `source_fleets` are moved into
/// `target_fleet` (which keeps its name), and the source fleet entities are
/// despawned once empty.
#[derive(Debug, Clone, Reflect)]
pub struct MergeFleetAction {
    /// Fleets whose ships will be moved out and despawned.
    pub source_fleets: Vec<Entity>,
    /// Fleet entity that survives and receives all ships.
    pub target_fleet: Entity,
}

/// Request to transfer ships between fleets.
#[derive(Debug, Clone, Reflect)]
pub struct TransferShipsAction {
    /// The source fleet entity.
    pub source_fleet: Entity,
    /// The destination fleet entity.
    pub destination_fleet: Entity,
    /// The indices of the ships to transfer from the source fleet.
    pub ship_indices: Vec<usize>,
}

/// Request to assign ship entities to a fleet without going through fleet-local indices.
#[derive(Debug, Clone, Reflect)]
pub struct AssignShipsAction {
    /// Ship entities to update.
    pub ship_entities: Vec<Entity>,
    /// Destination fleet, or `None` to leave the ships independent.
    pub destination_fleet: Option<Entity>,
}

/// Request to manually assign a freighter fleet to an open `ResourceRequest`
/// from the fleet panel (GRA-33 / PR-B).
///
/// The action is queued by the UI when the player clicks **Assign** in the
/// fleet-panel Logistics section.  `process_fleet_logistics_assignments`
/// (in `economy::logistics`) consumes the queue, validates the fleet/request
/// pairing, deducts the request amount from the source `LocalStockpile`
/// (first-fit-largest), and flips the request to `InTransit` with the fleet's
/// Hohmann round-trip ETA.
#[derive(Debug, Clone, Copy, Reflect)]
pub struct AssignLogisticsRequestAction {
    /// The freighter fleet entity (must be in orbit at the request's destination).
    pub fleet: Entity,
    /// The `ResourceRequest` id (`PendingResourceRequests::requests[i].id`).
    pub request_id: u64,
}

/// Request to create a fleet and attach specific ships to it immediately.
#[derive(Debug, Clone, Reflect)]
pub struct CreateFleetFromShipsAction {
    /// Display name for the new fleet.
    pub name: String,
    /// Body the fleet will orbit initially.
    pub orbit_body: Entity,
    /// Parking orbit radius in AU.
    pub orbit_radius_au: f64,
    /// Whether the spawned fleet should remain fixed like an orbital station.
    pub stationary: bool,
    /// Ship entities that should be assigned to the new fleet.
    pub ship_entities: Vec<Entity>,
}

/// Request to spawn a new fleet in orbit around a body.
#[derive(Debug, Clone, Reflect)]
pub struct SpawnFleetAction {
    /// Display name for the new fleet.
    pub name: String,
    /// Ships to include.
    pub ships: Vec<ShipInfo>,
    /// Body the fleet will orbit initially.
    pub orbit_body: Entity,
    /// Parking orbit radius in AU.
    pub orbit_radius_au: f64,
    /// True when the spawned fleet should remain fixed like an orbital station.
    pub stationary: bool,
}

/// Request to start a previously computed orbital transfer.
#[derive(Debug, Clone)]
pub struct StartTransferAction {
    /// The fleet entity that should perform the transfer.
    pub fleet: Entity,
    /// Fully computed transfer details.
    pub transfer: PlannedTransfer,
    /// Fuel (tonnes) to deduct immediately as an abort/correction burn penalty.
    /// Zero for transfers from a stable orbit; non-zero for mid-transit course corrections.
    pub abort_cost_t: f32,
    /// How far in the future (seconds) the fleet should depart.  Zero = depart immediately.
    /// The fleet remains in its parking orbit until this offset elapses.
    pub departure_offset_s: f64,
}

/// Request to abort a mid-transit maneuver and return the fleet to a parking
/// orbit at its origin body.  GRA-153 M-3.
///
/// The handler (`process_fleet_actions`) inspects the fleet's current
/// `ActiveManeuver`, deducts the abort fuel cost, and replaces the maneuver
/// with a fresh return-to-origin transfer — preserving the fleet entity, its
/// ships' `assigned_fleet` membership, and the visible render position.
#[derive(Debug, Clone, Copy, Reflect)]
pub struct AbortToOriginAction {
    /// The fleet entity that should abort its current maneuver.
    pub fleet: Entity,
    /// Fuel (tonnes) to deduct as the abort burn (matches the H-4 result).
    pub abort_cost_t: f32,
}

/// A fully computed transfer plan, ready to be turned into an `ActiveManeuver`.
///
/// `option_label` is intentionally `&'static str` — it identifies a static
/// catalog entry (e.g. `"Hohmann"`, `"GA"`).  The other lifetime-free fields
/// all serialise cleanly.  `StartTransferAction::transfer` carries
/// `#[reflect(ignore)]` so a save restores the queue but loses the in-flight
/// plan; the planner rebuilds it on reload.
#[derive(Debug, Clone, Reflect)]
pub struct PlannedTransfer {
    /// Origin body entity.
    pub origin_body: Entity,
    /// Destination body entity.
    pub destination_body: Entity,
    /// Reference frame used to interpret the transfer geometry.
    pub reference_frame: TransferReferenceFrame,
    /// The central star/body the transfer orbit is centred on.
    pub orbit_center: Entity,
    /// Keplerian orbit for the transfer arc.
    pub transfer_orbit: KeplerOrbit,
    /// Transfer duration (seconds) — used to compute `arrival_time` at execution.
    pub duration_s: f64,
    /// Preserve the precomputed transfer orbit exactly when the maneuver launches.
    pub preserve_orbit_geometry: bool,
    /// Arrival circularisation Δv (m/s).
    pub arrival_delta_v_ms: f64,
    /// Parking orbit radius at the destination (AU).
    pub arrival_orbit_radius_au: f64,
    /// Estimated propellant consumed (tonnes).
    pub fuel_cost_t: f32,
    /// Label identifying which transfer option was chosen.
    /// `&'static str` — not Reflect-friendly by itself; the planner
    /// rebuilds the label on reload.
    pub option_label: &'static str,
    /// Pre-computed departure position (AU) for kinematic/direct transfers.
    /// When set, `process_fleet_actions` uses this instead of predicting from `origin_body`.
    pub start_position_au: Option<bevy::math::DVec3>,
    /// Pre-computed arrival position (AU) for kinematic/direct transfers.
    /// When set, `process_fleet_actions` uses this instead of predicting from `destination_body`.
    pub end_position_au: Option<bevy::math::DVec3>,
    /// Lambert departure velocity (m/s) for barycentric curved transfers.
    pub departure_velocity_ms: Option<bevy::math::DVec3>,
    /// Lambert arrival velocity (m/s) for barycentric curved transfers.
    pub arrival_velocity_ms: Option<bevy::math::DVec3>,
    /// For gravity-assist transfers a pre-computed flyby body is stored here.
    ///
    /// This allows the rendering code to know where the two-leg corner should
    /// occur once the maneuver is in progress.  `None` for all non-GA transfers.
    pub flyby_body: Option<Entity>,

    /// Optional second-leg Keplerian orbit for gravity-assist transfers (flyby → destination).
    ///
    /// When set, `update_fleet_maneuver_positions` follows `transfer_orbit` until
    /// `leg2_start_s` seconds after departure, then switches to this orbit.
    pub leg2_orbit: Option<KeplerOrbit>,
    /// Seconds after departure when the Leg-2 orbit begins (= Leg-1 half-period).
    /// Only meaningful when `leg2_orbit` is `Some`.
    pub leg2_start_s: f64,
}

/// Which historical space probe a `HistoricalProbe` entity represents.
///
/// The four values match the LGD brief for GRA-127.B (GRA-131): the four
/// most-referenced deep-space probes at the 2026-01-01 epoch.  Each kind
/// pins a single JPL Horizons state vector used at spawn time and
/// drives the per-probe science bonus (`+0.5 RP` once per save, gated on
/// `SimulationTime` so the bonus never applies to player save-scumming).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[reflect(Component)]
pub enum HistoricalProbeKind {
    /// Voyager 1 — hyperbolic escape trajectory, ~169.5 AU heliocentric at 2026-01-01.
    Voyager1,
    /// Voyager 2 — hyperbolic escape trajectory, ~138.7 AU heliocentric at 2026-01-01.
    Voyager2,
    /// Parker Solar Probe — bound elliptical orbit inside Mercury's perihelion.
    Parker,
    /// New Horizons — hyperbolic escape trajectory, ~64.1 AU heliocentric at 2026-01-01.
    NewHorizons,
}

impl HistoricalProbeKind {
    /// Short identifier used in save data, telemetry, and log lines.
    pub fn slug(self) -> &'static str {
        match self {
            HistoricalProbeKind::Voyager1 => "voyager_1",
            HistoricalProbeKind::Voyager2 => "voyager_2",
            HistoricalProbeKind::Parker => "parker",
            HistoricalProbeKind::NewHorizons => "new_horizons",
        }
    }
}

/// Marker + descriptive metadata for a historical space probe entity.
///
/// `HistoricalProbe` is a *companion* to the regular `KeplerOrbit` (and
/// optional `HyperbolicTrajectory` for the three escape trajectories).
/// It carries the kind discriminator, a display name, the launching
/// agency, and the launch year so the starmap tooltip and science
/// panel can show "Voyager 1 (NASA, 1977)" without an extra lookup.
///
/// Historical probes are spawned exactly once per save (idempotency
/// tracked by the `HistoricalProbesSpawned` resource).  They never
/// appear in the player fleet list, never accept transfer orders, and
/// do not consume construction materials — they are background
/// science assets only.
///
/// `name` and `agency` are `&'static str` catalog references and are
/// intentionally not part of the reflected shape (`#[reflect(ignore)]`);
/// the spawner rebuilds them from `HistoricalProbeKind` after reload.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct HistoricalProbe {
    /// Which historical probe this entity represents.
    pub kind: HistoricalProbeKind,
    /// Display name (e.g. "Voyager 1").
    #[reflect(ignore)]
    pub name: &'static str,
    /// Launching space agency (e.g. "NASA").
    #[reflect(ignore)]
    pub agency: &'static str,
    /// Calendar year the probe launched (UTC).
    pub launch_year: u16,
}

// === Porkchop plot (H-1) — GRA-152 ============================================
//
// RON-driven replacement for the Efficient/Moderate/Fast placeholder options
// in the transfer planner. A porkchop is a contour plot of total ΔV (or
// departure C3) over the (t_dep, t_tof) plane; the player picks a cell, the
// planner converts that cell to a real Lambert conic, and the executed arc
// finally agrees with the displayed ΔV. The LGD-owned RON schema lives in
// `assets/data/porkchop_config.ron`; the Rust types below are its loader
// shape and the resource that `DataLoaderPlugin` registers at Startup.

/// Default grid bounds for a transfer (used when no per-category override
/// matches).  All times are in seconds; distances are in AU; C3 is in
/// (km/s)².  The defaults match the LGD RON file's `defaults` block.
#[derive(Debug, Clone, Serialize, Deserialize, Resource, Reflect)]
#[reflect(Resource)]
pub struct PorkchopConfig {
    pub defaults: PorkchopGridDefaults,
    #[serde(default)]
    pub category_overrides: Vec<PorkchopCategoryOverride>,
    pub colormap: Vec<PorkchopColorStop>,
    #[serde(default)]
    pub contour_levels_km_s: Vec<f64>,
    pub display_max_dv_km_s: f64,
}

/// Conservative defaults used when no `PorkchopCategoryOverride` matches.
/// Times are seconds; distances are AU; C3 is (km/s)².
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct PorkchopGridDefaults {
    /// ±`t_dep_window_days / 2` around the optimal Hohmann window.
    pub t_dep_window_days: f64,
    pub tof_min_hohmann_factor: f64,
    pub tof_max_hohmann_factor: f64,
    pub tof_floor_days: f64,
    pub tof_ceiling_years: f64,
    pub resolution_t_dep: usize,
    pub resolution_tof: usize,
    pub c3_ceiling_km2_s2: f64,
}

/// Per-category override.  Matched top-down against
/// `PorkchopMetric` transfer category, first hit wins.  Unknown
/// `match_key`s fall through to `defaults`.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct PorkchopCategoryOverride {
    pub match_key: String,
    pub t_dep_window_days: f64,
    pub tof_min_hohmann_factor: f64,
    pub tof_max_hohmann_factor: f64,
    pub tof_floor_days: f64,
    pub tof_ceiling_years: f64,
    pub resolution_t_dep: usize,
    pub resolution_tof: usize,
    pub c3_ceiling_km2_s2: f64,
}

/// One stop in the porkchop ΔV → RGBA colormap.  Linear interpolation
/// between adjacent stops.  The last stop's `delta_v_km_s` is treated as
/// the +∞ sentinel and is used to colour infeasible cells.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Reflect)]
pub struct PorkchopColorStop {
    pub delta_v_km_s: f64,
    pub rgba: (u8, u8, u8, u8),
}

impl Default for PorkchopConfig {
    fn default() -> Self {
        // Mirrors `assets/data/porkchop_config.ron` defaults.  Keep
        // these in sync — the loader falls back to `Default::default()`
        // if the RON file fails to parse, so any change here also
        // needs the corresponding RON edit.  `tests/data::porkchop_
        // config_ron_loads_cleanly` is the strict CI gate that
        // keeps the two paths aligned.
        Self {
            defaults: PorkchopGridDefaults {
                t_dep_window_days: 60.0,
                tof_min_hohmann_factor: 0.4,
                tof_max_hohmann_factor: 5.0,
                tof_floor_days: 5.0,
                tof_ceiling_years: 10.0,
                resolution_t_dep: 60,
                resolution_tof: 60,
                c3_ceiling_km2_s2: 400.0,
            },
            category_overrides: Vec::new(),
            colormap: vec![
                PorkchopColorStop {
                    delta_v_km_s: 0.0,
                    rgba: (40, 200, 80, 220),
                },
                PorkchopColorStop {
                    delta_v_km_s: 4.0,
                    rgba: (220, 200, 60, 220),
                },
                PorkchopColorStop {
                    delta_v_km_s: 8.0,
                    rgba: (230, 140, 40, 220),
                },
                PorkchopColorStop {
                    delta_v_km_s: 15.0,
                    rgba: (220, 60, 60, 220),
                },
                // Phase C: the previous +∞ sentinel stop has been
                // removed. Infeasible cells are now rendered with
                // the dedicated `INFEASIBLE_COLOR` (dark grey) in
                // `porkchop_color_ramp.rs` and the baked texture in
                // `porkchop_panel.rs`. Modders can still supply a
                // finite extended stop (e.g. ΔV = 30 km/s for the
                // "off the chart" band) but the runtime no longer
                // requires it.
            ],
            contour_levels_km_s: vec![3.0, 5.0, 8.0, 12.0],
            display_max_dv_km_s: 20.0,
        }
    }
}

impl PorkchopConfig {
    /// Resolve the effective grid bounds for a transfer category.
    /// Returns `(t_dep_window_days, tof_{min,max}_hohmann_factor, tof_floor_days,
    /// tof_ceiling_years, resolution_{t_dep,tof}, c3_ceiling_km2_s2)`.
    pub fn resolve(&self, category: &str) -> ResolvedPorkchopParams {
        for ov in &self.category_overrides {
            if ov.match_key == category {
                return ResolvedPorkchopParams {
                    t_dep_window_days: ov.t_dep_window_days,
                    tof_min_hohmann_factor: ov.tof_min_hohmann_factor,
                    tof_max_hohmann_factor: ov.tof_max_hohmann_factor,
                    tof_floor_days: ov.tof_floor_days,
                    tof_ceiling_years: ov.tof_ceiling_years,
                    resolution_t_dep: ov.resolution_t_dep,
                    resolution_tof: ov.resolution_tof,
                    c3_ceiling_km2_s2: ov.c3_ceiling_km2_s2,
                };
            }
        }
        ResolvedPorkchopParams {
            t_dep_window_days: self.defaults.t_dep_window_days,
            tof_min_hohmann_factor: self.defaults.tof_min_hohmann_factor,
            tof_max_hohmann_factor: self.defaults.tof_max_hohmann_factor,
            tof_floor_days: self.defaults.tof_floor_days,
            tof_ceiling_years: self.defaults.tof_ceiling_years,
            resolution_t_dep: self.defaults.resolution_t_dep,
            resolution_tof: self.defaults.resolution_tof,
            c3_ceiling_km2_s2: self.defaults.c3_ceiling_km2_s2,
        }
    }
}

/// Resolved grid bounds returned by `PorkchopConfig::resolve`.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedPorkchopParams {
    pub t_dep_window_days: f64,
    pub tof_min_hohmann_factor: f64,
    pub tof_max_hohmann_factor: f64,
    pub tof_floor_days: f64,
    pub tof_ceiling_years: f64,
    pub resolution_t_dep: usize,
    pub resolution_tof: usize,
    pub c3_ceiling_km2_s2: f64,
}

// === Interstellar propulsion policy — GRA-343 (GRA-328b) =====================
//
// Phase-angle tolerance + ΔV margin for cross-system Hohmann transfers
// (Sol → non-Sol star systems). The GRA-331 LGD contract fixes:
//   * AI planner   — ±15° phase tolerance, 20% ΔV margin.
//   * Human player — ±45° phase tolerance,  5% ΔV margin.
//
// Loaded once at Startup from `assets/data/interstellar_propulsion.ron`
// by `load_interstellar_propulsion_policy` in `src/fleets/data.rs`. The
// RON is the source of truth; this struct is the in-memory shape and the
// resource the planner reads via `Res<InterstellarPropulsionPolicy>`.
// Modders edit the RON — no Rust recompile required (per the GRA-343
// acceptance criteria #2).
//
// All four fields are required (no `#[serde(default)]`); a missing key
// aborts deserialisation and the loader falls back to
// `InterstellarPropulsionPolicy::default()`. The strict
// `tests/data::interstellar_propulsion_ron_loads_cleanly` test is the
// hard CI gate that catches RON typos before they hit runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct InterstellarPropulsionPolicy {
    /// Phase-angle tolerance (degrees) for AI-controlled launch windows.
    ///
    /// The solver compares `|actual_phase - ideal_phase|` against this
    /// tolerance and refuses to commit a transfer that lands outside the
    /// cone. Default `15.0` per the GRA-331 LGD contract; modders who want
    /// a more aggressive AI planner can raise this.
    pub ai_phase_angle_tolerance_deg: f64,
    /// Phase-angle tolerance (degrees) for human-controlled launch windows.
    ///
    /// Wider than the AI default so the player has meaningful agency over
    /// when to launch; the margin check still gates the worst ±90°+
    /// geometries. Default `45.0` per the GRA-331 LGD contract.
    pub human_phase_angle_tolerance_deg: f64,
    /// ΔV margin factor for AI-controlled launches.
    ///
    /// The solver gates the commit on `fleet.max_delta_v_ms() >=
    /// dv_required * ai_deltav_margin`. Default `1.20` (20% reserve per
    /// the GRA-331 LGD contract). Lowering this tightens AI launch
    /// permissions but risks the planner committing a transfer the
    /// fleet cannot complete.
    pub ai_deltav_margin: f64,
    /// ΔV margin factor for human-controlled launches.
    ///
    /// Tighter than the AI default (`1.05`) because the player can
    /// manually accept the warning via the confirmation UI; the AI
    /// planner refuses any transfer it cannot budget for. Default `1.05`
    /// per the GRA-331 LGD contract.
    pub human_deltav_margin: f64,
}

impl Default for InterstellarPropulsionPolicy {
    /// Mirrors `assets/data/interstellar_propulsion.ron` defaults. Keep
    /// these in sync — the loader falls back to `Default::default()` if
    /// the RON file fails to parse, so any change here also needs the
    /// corresponding RON edit. The strict
    /// `tests/data::interstellar_propulsion_ron_loads_cleanly` test is
    /// the CI gate that keeps the two paths aligned.
    fn default() -> Self {
        Self {
            ai_phase_angle_tolerance_deg: 15.0,
            human_phase_angle_tolerance_deg: 45.0,
            ai_deltav_margin: 1.20,
            human_deltav_margin: 1.05,
        }
    }
}

impl InterstellarPropulsionPolicy {
    /// Conservative validator: returns `Err(violations)` if any of the
    /// four policy values are non-finite, non-positive, or implausibly
    /// extreme. The loader runs this and logs warnings on failure but
    /// still inserts the resource (debug builds shouldn't panic on a bad
    /// RON); the strict RON-load test is the hard gate.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if !self.ai_phase_angle_tolerance_deg.is_finite()
            || self.ai_phase_angle_tolerance_deg <= 0.0
            || self.ai_phase_angle_tolerance_deg > 180.0
        {
            violations.push(format!(
                "ai_phase_angle_tolerance_deg must be in (0, 180] (got {})",
                self.ai_phase_angle_tolerance_deg
            ));
        }
        if !self.human_phase_angle_tolerance_deg.is_finite()
            || self.human_phase_angle_tolerance_deg <= 0.0
            || self.human_phase_angle_tolerance_deg > 180.0
        {
            violations.push(format!(
                "human_phase_angle_tolerance_deg must be in (0, 180] (got {})",
                self.human_phase_angle_tolerance_deg
            ));
        }
        if !self.ai_deltav_margin.is_finite() || self.ai_deltav_margin < 1.0 {
            violations.push(format!(
                "ai_deltav_margin must be ≥ 1.0 (got {})",
                self.ai_deltav_margin
            ));
        }
        if !self.human_deltav_margin.is_finite() || self.human_deltav_margin < 1.0 {
            violations.push(format!(
                "human_deltav_margin must be ≥ 1.0 (got {})",
                self.human_deltav_margin
            ));
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

// ── GRA-371: TransferPlan resource + SelectionSource enum ─────────────────────
//
// Phase 1 of the GRA-367 harmonisation plan.  These types define the *future*
// unified shape that will replace `FleetUiState`'s six coexisting transfer
// fields, but Phase 1 ships them with no behaviour change — they are populated
// through a one-way write-through shadow (`sync_transfer_plan_from_ui_state`,
// registered in `FleetPlugin::build`) and the only consumer in this phase is
// the 1-line reference-frame indicator in `src/ui/transfer_planner.rs`.  No
// rendering path reads `SelectionSource` yet; `SelectionSource` exists so the
// data layer can land first and the panel collapse (Phases 2-6) follows
// without an API break.

/// Player's currently-active transfer plan — the unified shape that
/// `FleetUiState`'s six coexisting option surfaces will collapse onto.
///
/// Phase 1 only writes this resource through the shadow-sync system described
/// above.  Consumers of `TransferPlan` for *rendering* are introduced in
/// Phase 2 (selected-option card unification).  Until then the panel keeps
/// reading `FleetUiState` directly — `TransferPlan` exists so the data
/// layer can land first.
///
/// The `source` enum carries exactly the information that determines which
/// per-class branch the planner renders today; collapse the branch onto the
/// enum value in Phases 3-6, then drop the per-class fields from
/// `FleetUiState`.
///
/// GRA-367 Phase 1 (this PR).  See `docs/design/TRANSFER_PLANNER_HARMONISATION.md`
/// r2 for the full phase breakdown.
#[derive(Resource, Default, Debug)]
pub struct TransferPlan {
    /// The data layer for the active transfer (porkchop / GA / kinematic /
    /// cross-star / star-approach / empty).  In Phase 1 the planner still
    /// derives its render branch from `FleetUiState`; `source` is populated
    /// by the shadow-sync system to match what the planner is *currently*
    /// rendering so callers can later migrate branch-by-branch without a
    /// feature flag.
    pub source: SelectionSource,
    /// Currently-selected transfer option (if the player has picked a cell /
    /// preset).  In Phase 1 this mirrors `(col, row)` for porkchop targets
    /// and `selected_option` for legacy rows; nothing reads it.
    pub selected: Option<SelectedTransfer>,
    /// Optional rendered preview block (mirror of `computed_options`).
    pub preview: Option<PlanPreview>,
    /// Fully-assembled commit plan (mirror of `planned_transfer`).
    pub commit: Option<PlannedTransfer>,
    /// Player-overridable reference frame.  Phase 1 stores the resolved
    /// frame but does not consume it; Phase 6 wires the override UI.
    pub frame: PlannerFrame,
}

impl TransferPlan {
    /// Reset the plan to its default state.  Mirrors `FleetUiState::clear_target`
    /// for callers that move to `TransferPlan` as the source of truth.
    pub fn clear(&mut self) {
        self.source = SelectionSource::Empty;
        self.selected = None;
        self.preview = None;
        self.commit = None;
        self.frame = PlannerFrame::Auto;
    }
}

/// Which transfer class / grid is currently driving the planner.
///
/// Phase 1 only writes this from the shadow-sync system; nothing renders off
/// it yet.  The `Empty` variant is also the initial state, so consumers can
/// safely check `is_empty()` to know whether any transfer is in flight.
///
/// GRA-367 Phase 1.  See `docs/design/TRANSFER_PLANNER_HARMONISATION.md` r2.
#[derive(Debug, Clone)]
pub enum SelectionSource {
    /// No transfer is currently selected (player has not picked a target).
    Empty,
    /// A planet-to-planet porkchop grid is active.  `(col, row)` is the
    /// currently-highlighted cell (or `None` if no selection).
    Porkchop {
        grid: PorkchopGridSnapshot,
        selected: Option<(usize, usize)>,
    },
    /// A gravity-assist chain (one or more flyby candidates) is active.
    /// In Phase 1 the planner still treats GAs as a per-candidate row, not a
    /// grid — so we mirror the picked candidate + its dep window here.
    GravityAssist {
        candidate: GravityAssistEntrySnapshot,
        dep_window: DepWindowSnapshot,
    },
    /// A short-hop (moon, ring) "3-speeds"-style row is active.  Phase 3
    /// swaps the 3-card row for an n-row porkchop bar (RON-configurable
    /// n_options, default 5).
    BodyHohmann {
        option: TransferOptionSnapshot,
        dep_window: DepWindowSnapshot,
    },
    /// Interstellar transfer (single kinematic option).
    Interstellar {
        option: KinematicOptionSnapshot,
        distance_ly: f32,
    },
    /// Cross-star hop (interior of binary / trinary system, or a 1×1
    /// degenerate porkchop populated by `try_build_cross_system_hohmann`).
    CrossStar {
        grid: CrossSystemGridSnapshot,
        selected: Option<(usize, usize)>,
    },
}

impl SelectionSource {
    /// `true` when no transfer is currently in flight.
    pub fn is_empty(&self) -> bool {
        matches!(self, SelectionSource::Empty)
    }

    /// Short label used by the per-class branches in the shadow-sync
    /// system to record which source the planner is *currently* rendering
    /// off of `FleetUiState`.  Pure-string discriminator; the variants
    /// themselves carry the data.
    pub fn class_label(&self) -> &'static str {
        match self {
            SelectionSource::Empty => "empty",
            SelectionSource::Porkchop { .. } => "porkchop",
            SelectionSource::GravityAssist { .. } => "gravity_assist",
            SelectionSource::BodyHohmann { .. } => "short_hop",
            SelectionSource::Interstellar { .. } => "interstellar",
            SelectionSource::CrossStar { .. } => "cross_star",
        }
    }
}

/// Resolved reference frame for the transfer (Phase 1 + Phase 6 override).
///
/// `Auto` means "use the planner's auto-resolved frame" (the same logic as
/// the legacy `resolve_planner_transfer_frame`).  Phase 6 will add a UI
/// override; Phase 1 stores only `Auto` and the resolved value gets baked
/// into `preview` for tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlannerFrame {
    /// Defer to the planner's auto-resolved frame (`BodyLocal` /
    /// `StellarLocal` / `SystemBarycentric`).
    #[default]
    Auto,
    /// Force body-local frame around a specific entity.
    BodyLocal(Entity),
    /// Force stellar-local frame around a specific entity.
    StellarLocal(Entity),
    /// Force system barycentric frame (cross-system transfers).
    SystemBarycentric,
}

impl PlannerFrame {
    /// `true` if the player has overridden the auto-resolved frame.
    pub fn is_overridden(&self) -> bool {
        !matches!(self, PlannerFrame::Auto)
    }
}

/// Lightweight snapshot of a `PorkchopGrid` for `SelectionSource::Porkchop`.
///
/// We can't put the full `PorkchopGrid` here because that would force
/// `TransferPlan` (in `components.rs`) to depend on `porkchop.rs`, which is
/// already a tight dependency cycle in the project.  The snapshot records
/// the `(col, row)` selection plus the cell count for now; later phases
/// replace this with a `Weak<PorkchopGrid>` handle.
#[derive(Debug, Clone, Copy)]
pub struct PorkchopGridSnapshot {
    /// Total cells (`cols × rows`).
    pub cell_count: usize,
    /// Number of columns (t_dep samples).
    pub cols: usize,
    /// Number of rows (tof samples).
    pub rows: usize,
}

/// Lightweight snapshot of a `GravityAssistEntry` (Phase 1 carry-only).
#[derive(Debug, Clone, Copy)]
pub struct GravityAssistEntrySnapshot {
    /// Index of the assist candidate in `FleetUiState.gravity_assist_candidates`,
    /// or `None` for "no GA selected".
    pub index: Option<usize>,
}

/// Lightweight snapshot of the departure window the planner is using.
#[derive(Debug, Clone, Copy)]
pub struct DepWindowSnapshot {
    /// Sim-time epoch (seconds since world start) at the window's centre.
    pub center_s: f64,
    /// Window half-width (seconds).
    pub half_width_s: f64,
}

/// Lightweight snapshot of a `TransferOption` (Phase 1 carry-only).
#[derive(Debug, Clone, Copy)]
pub struct TransferOptionSnapshot {
    /// Index into `FleetUiState.computed_options`.
    pub index: usize,
    /// Total ΔV for the option (m/s) — captured for the per-class invariant
    /// in Phase 3 ("strictly decreasing peak Δv").
    pub delta_v_ms: f64,
}

/// Lightweight snapshot of a single kinematic (interstellar) option.
#[derive(Debug, Clone, Copy)]
pub struct KinematicOptionSnapshot {
    /// ΔV (m/s) of the kinematic option.
    pub delta_v_ms: f64,
    /// Brachistochrone duration (s).
    pub duration_s: f64,
}

/// Lightweight snapshot of a `CrossSystemGrid` (GRA-343 follow-up).
#[derive(Debug, Clone, Copy)]
pub struct CrossSystemGridSnapshot {
    /// Total cell count (`cols × rows`).
    pub cell_count: usize,
}

/// Mirror of `FleetUiState.selected_porkchop_cell` plus the parsed transfer
/// data.  Phase 2 wires the unified selected-card to this struct; Phase 1
/// just keeps it populated so future consumers can adopt it incrementally.
#[derive(Debug, Clone, Copy)]
pub struct SelectedTransfer {
    /// Absolute departure epoch (sim seconds since world start).
    pub t_dep_s: f64,
    /// Time-of-flight (seconds).
    pub tof_s: f64,
    /// Total ΔV (m/s) for the selected option.
    pub total_dv_ms: f64,
    /// Number of legs (1 for direct / kinematic; 2 for gravity assist).
    pub leg_count: u8,
    /// Optional transfer orbit (helps `process_fleet_actions` keep the same
    /// orbit it had when only `selected_porkchop_cell` was the canonical
    /// state).
    pub has_transfer_orbit: bool,
}

/// Mirror of `FleetUiState.computed_options`-derived preview block.
#[derive(Debug, Clone)]
pub struct PlanPreview {
    /// How many options the planner surfaced (1 for kinematic / interstellar
    /// / cross-star; n for short-hop row; cols × rows for a porkchop).
    pub option_count: usize,
    /// `(col, row)` for the auto-picked cheapest cell — Phase 1 stub.
    pub recommended: Option<(usize, usize)>,
}
        }
    }
}
