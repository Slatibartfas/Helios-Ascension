//! ECS components for the fleet management and orbital transfer system.

use super::types::{FleetRole, PropulsionType, ShipClass};
use crate::astronomy::KeplerOrbit;
use bevy::prelude::*;

/// Summary information about a single ship within a fleet.
#[derive(Debug, Clone)]
pub struct ShipInfo {
    /// Name of the ship.
    pub name: String,
    /// Ship class (Frigate, Destroyer, etc.).
    pub class: ShipClass,
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
    pub fn new(name: String, class: ShipClass, propulsion: PropulsionType) -> Self {
        let dry_mass = class.default_dry_mass_t();
        // fuel_fraction is of total wet mass: fuel = dry * frac/(1-frac)
        let fuel_frac = class.default_fuel_fraction();
        let fuel_mass = dry_mass * fuel_frac / (1.0 - fuel_frac);
        Self {
            name,
            class,
            dry_mass_t: dry_mass,
            fuel_mass_t: fuel_mass,
            max_fuel_t: fuel_mass,
            thrust_kn: propulsion.thrust_kn(dry_mass),
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
#[derive(Component, Debug, Clone)]
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
#[derive(Component, Debug, Clone)]
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
}

/// Stable circular parking orbit for a fleet around a celestial body.
///
/// Updated each frame by `update_fleet_orbit_positions`.
#[derive(Component, Debug, Clone, Copy)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Component, Debug, Clone)]
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
}

impl ActiveManeuver {
    /// Whether this transfer uses kinematic (straight-line) interpolation rather
    /// than Keplerian orbit propagation.
    ///
    /// Kinematic transfers include full-thrust, coast phases, max-speed runs,
    /// and direct L1/L2 Lagrange-point transfers.
    pub fn is_kinematic(&self) -> bool {
        self.option_label == "Full Thrust"
            || self.option_label.contains("Coast")
            || self.option_label == "Max Speed"
            || self.option_label.contains("Direct")
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
}

/// Merge two or more fleets: all ships from `source_fleets` are moved into
/// `target_fleet` (which keeps its name), and the source fleet entities are
/// despawned once empty.
#[derive(Debug, Clone)]
pub struct MergeFleetAction {
    /// Fleets whose ships will be moved out and despawned.
    pub source_fleets: Vec<Entity>,
    /// Fleet entity that survives and receives all ships.
    pub target_fleet: Entity,
}

/// Request to transfer ships between fleets.
#[derive(Debug, Clone)]
pub struct TransferShipsAction {
    /// The source fleet entity.
    pub source_fleet: Entity,
    /// The destination fleet entity.
    pub destination_fleet: Entity,
    /// The indices of the ships to transfer from the source fleet.
    pub ship_indices: Vec<usize>,
}

/// Request to assign ship entities to a fleet without going through fleet-local indices.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Copy)]
pub struct AssignLogisticsRequestAction {
    /// The freighter fleet entity (must be in orbit at the request's destination).
    pub fleet: Entity,
    /// The `ResourceRequest` id (`PendingResourceRequests::requests[i].id`).
    pub request_id: u64,
}

/// Request to create a fleet and attach specific ships to it immediately.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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

/// A fully computed transfer plan, ready to be turned into an `ActiveManeuver`.
#[derive(Debug, Clone)]
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
