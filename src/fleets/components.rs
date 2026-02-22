
//! ECS components for the fleet management and orbital transfer system.

use bevy::prelude::*;
use super::types::{PropulsionType, ShipClass};
use crate::astronomy::KeplerOrbit;

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

/// A named collection of ships orbiting (or transferring between) celestial bodies.
#[derive(Component, Debug, Clone)]
pub struct Fleet {
    /// Display name for this fleet.
    pub name: String,
    /// Ships that make up this fleet.
    pub ships: Vec<ShipInfo>,
}

impl Fleet {
    /// Create an empty fleet with the given name.
    pub fn new(name: String) -> Self {
        Self { name, ships: Vec::new() }
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

    /// Thrust-weighted average specific impulse of the fleet's engines (seconds).
    pub fn average_isp_s(&self) -> f32 {
        let total_thrust: f32 = self.ships.iter().map(|s| s.thrust_kn).sum();
        if total_thrust <= 0.0 {
            return 450.0;
        }
        self.ships.iter().map(|s| s.thrust_kn * s.isp_s).sum::<f32>() / total_thrust
    }

    /// Total thrust of all engines in the fleet (kilonewtons).
    pub fn total_thrust_kn(&self) -> f32 {
        self.ships.iter().map(|s| s.thrust_kn).sum()
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
                if wm <= 0.0 { 0.0_f64 } else { (s.thrust_kn / wm) as f64 }
            })
            .reduce(f64::min)
            .unwrap_or(0.0)
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
        Self { body, radius_au, angle_rad: 0.0, direction: 1.0 }
    }
}

/// An active Keplerian transfer arc being executed by a fleet.
///
/// While present on an entity, `update_fleet_maneuver_positions` drives the
/// fleet's `SpaceCoordinates` along the transfer ellipse each frame.
/// `complete_fleet_maneuvers` removes this component when `arrival_time` is reached.
#[derive(Component, Debug, Clone)]
pub struct ActiveManeuver {
    /// Keplerian orbit describing the transfer arc.
    ///
    /// `mean_anomaly_epoch` is the mean anomaly at `departure_time`.
    /// `mean_motion` is the full orbital mean motion (rad/s).
    pub transfer_orbit: KeplerOrbit,
    /// Entity of the star (or central body) the transfer orbit is centred on.
    pub orbit_center: Entity,
    /// Entity of the body the fleet departed from (used for visual arc rendering).
    pub origin_body: Entity,
    /// `SimulationTime.elapsed` at the moment of departure.
    pub departure_time: f64,
    /// `SimulationTime.elapsed` when the fleet is expected to arrive.
    pub arrival_time: f64,
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
}

impl ActiveManeuver {
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
    /// The central star/body the transfer orbit is centred on.
    pub orbit_center: Entity,
    /// Keplerian orbit for the transfer arc.
    pub transfer_orbit: KeplerOrbit,
    /// Transfer duration (seconds) — used to compute `arrival_time` at execution.
    pub duration_s: f64,
    /// Arrival circularisation Δv (m/s).
    pub arrival_delta_v_ms: f64,
    /// Parking orbit radius at the destination (AU).
    pub arrival_orbit_radius_au: f64,
    /// Estimated propellant consumed (tonnes).
    pub fuel_cost_t: f32,
    /// Label identifying which transfer option was chosen.
    pub option_label: &'static str,
}
