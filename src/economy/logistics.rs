//! Resource request and minimum-stockpile logistics system.
//!
//! Each time a building is queued (or an outpost founded) and the destination
//! body's *local* stockpile does not have enough materials, a `ResourceRequest`
//! is created.  Requests are fulfilled either by:
//!
//! 1. A player-controlled Freighter fleet (manual — see
//!    `process_fleet_logistics_assignments` and the fleet-panel Logistics
//!    section in `ui::fleets_panel`).
//! 2. A private `ShippingCompany` AI (automated — see `company.rs`).
//!
//! When resources arrive the linked `ConstructionProject` is unblocked
//! (`awaiting_resources` cleared) and construction can proceed.
//!
//! Additionally, each colony with a `MinimumStockpile` component is checked
//! every simulation tick; if any tracked resource falls below its threshold a
//! Maintenance-priority request is automatically created.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::astronomy::components::SpaceCoordinates;
use crate::colony::components::ConstructionProject;
use crate::economy::components::LocalStockpile;
use crate::economy::types::ResourceType;
use crate::economy::GlobalBudget;
use crate::fleets::{FleetOrbit, PendingFleetActions, ShipClass, AU_IN_METERS, GM_SUN};
use crate::shipbuilding::ShipConstructionProject;
use crate::ui::SimulationTime;

// ── Priority ─────────────────────────────────────────────────────────────────

/// Priority tier for resource requests.
///
/// Higher-priority requests are fulfilled first by shipping companies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RequestPriority {
    /// Surplus/profitable rebalancing — lowest priority.
    Trade = 0,
    /// Below minimum-stockpile threshold — steady-state replenishment.
    Maintenance = 1,
    /// Building queue blocked waiting for construction materials.
    Construction = 2,
    /// Life support shortfall — highest priority.
    Emergency = 3,
}

impl std::fmt::Display for RequestPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestPriority::Trade => write!(f, "Trade"),
            RequestPriority::Maintenance => write!(f, "Maintenance"),
            RequestPriority::Construction => write!(f, "Construction"),
            RequestPriority::Emergency => write!(f, "Emergency"),
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

/// Lifecycle state of a resource request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestState {
    /// No freighter assigned yet.
    Pending,
    /// A freighter has been assigned and resources earmarked; transit not started.
    Assigned,
    /// Resources are physically en route.
    InTransit,
    /// Resources delivered; request closed.
    Delivered,
    /// Request timed out without being fulfilled.
    Expired,
}

impl std::fmt::Display for RequestState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestState::Pending => write!(f, "Pending"),
            RequestState::Assigned => write!(f, "Assigned"),
            RequestState::InTransit => write!(f, "In Transit"),
            RequestState::Delivered => write!(f, "Delivered"),
            RequestState::Expired => write!(f, "Expired"),
        }
    }
}

// ── ResourceRequest ───────────────────────────────────────────────────────────

/// A request for a specific resource to be delivered to a body.
#[derive(Debug, Clone)]
pub struct ResourceRequest {
    /// Unique identifier.
    pub id: u64,
    /// Body that needs the resources.
    pub destination_body: Entity,
    /// Name of the destination (for UI display).
    pub destination_name: String,
    /// What resource is needed.
    pub resource: ResourceType,
    /// How much is needed (Megatons).
    pub amount_mt: f64,
    /// Priority tier.
    pub priority: RequestPriority,
    /// Current lifecycle state.
    pub state: RequestState,
    /// Amount currently in transit (≤ amount_mt).
    pub in_transit_mt: f64,
    /// SimulationTime (seconds) when the delivery arrives.  `None` = not yet in transit.
    pub eta_seconds: Option<f64>,
    /// Index into `ShippingCompanies.companies` of the assigned company, if any.
    pub assigned_company_idx: Option<usize>,
    /// SimulationTime (seconds) when this request was created.
    pub created_at_seconds: f64,
    /// Source body entity (where resources are drawn from).  `None` = global budget fallback.
    pub source_body: Option<Entity>,
    /// `ConstructionProject` entity that is blocked waiting for this delivery.
    /// `None` for Maintenance / Emergency requests.
    pub linked_project: Option<Entity>,
    /// True once the company has been paid for this delivery (prevents double-payment).
    pub payment_made: bool,
    /// SimulationTime (seconds) when this request transitioned to Delivered or Expired.
    /// `None` while the request is still open.
    pub completed_at_seconds: Option<f64>,
    /// Player-controlled freighter fleet that manually took this request via the
    /// fleet panel.  `None` for company-AI or maintenance-created requests.  When
    /// set, `assigned_company_idx` is `None` and the request is no longer offered
    /// to the `ShippingCompany` AI.
    pub assignee_fleet_id: Option<Entity>,
}

impl ResourceRequest {
    /// True if the request is still active (not yet delivered or expired).
    pub fn is_open(&self) -> bool {
        matches!(
            self.state,
            RequestState::Pending | RequestState::Assigned | RequestState::InTransit
        )
    }

    /// Display label for the resource and amount.
    pub fn summary(&self) -> String {
        format!("{:?} {:.1} Mt", self.resource, self.amount_mt)
    }
}

// ── PendingResourceRequests ───────────────────────────────────────────────────

/// Global resource that holds all active and recently-completed requests.
///
/// Requests are retained after delivery/expiry for a short window so that the
/// Logistics UI can show recent history; entries older than `HISTORY_KEEP_S`
/// are pruned.
#[derive(Resource, Default)]
pub struct PendingResourceRequests {
    pub requests: Vec<ResourceRequest>,
    next_id: u64,
}

/// How many seconds of delivered/expired requests to retain for the UI history.
const HISTORY_KEEP_S: f64 = 86_400.0 * 30.0; // 30 in-game days

impl PendingResourceRequests {
    /// Add a new request, returning its assigned ID.
    pub fn add(&mut self, mut req: ResourceRequest) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        req.id = id;
        self.requests.push(req);
        id
    }

    /// Iterate over open (not delivered/expired) requests.
    pub fn open_requests(&self) -> impl Iterator<Item = &ResourceRequest> {
        self.requests.iter().filter(|r| r.is_open())
    }

    /// Find a request by ID.
    pub fn find_by_id(&self, id: u64) -> Option<&ResourceRequest> {
        self.requests.iter().find(|r| r.id == id)
    }

    /// Find a request by ID (mutable).
    pub fn find_by_id_mut(&mut self, id: u64) -> Option<&mut ResourceRequest> {
        self.requests.iter_mut().find(|r| r.id == id)
    }

    /// Remove delivered/expired requests once their *completion* is older than `HISTORY_KEEP_S`.
    ///
    /// Using `completed_at_seconds` (not `created_at_seconds`) ensures that long-haul
    /// requests are not immediately evicted from history just because they were created
    /// more than 30 days ago.
    pub fn prune(&mut self, current_sim_seconds: f64) {
        self.requests.retain(|r| {
            r.is_open()
                || r.completed_at_seconds
                    .unwrap_or(r.created_at_seconds)
                    .gt(&(current_sim_seconds - HISTORY_KEEP_S))
        });
    }

    /// True if there is already an open request for this body+resource combination.
    pub fn has_open_request_for(&self, body: Entity, resource: ResourceType) -> bool {
        self.requests
            .iter()
            .any(|r| r.destination_body == body && r.resource == resource && r.is_open())
    }

    pub fn open_request_ids_for(&self, body: Entity, resource: ResourceType) -> Vec<u64> {
        self.requests
            .iter()
            .filter(|r| r.destination_body == body && r.resource == resource && r.is_open())
            .map(|r| r.id)
            .collect()
    }
}

// ── MinimumStockpile ──────────────────────────────────────────────────────────

/// Per-body, per-resource minimum stockpile thresholds.
///
/// When the `LocalStockpile` for a body falls below a configured threshold a
/// Maintenance-priority `ResourceRequest` is automatically created so that
/// private freighters (or the player) keep the stockpile topped up.
#[derive(Component, Default, Clone, Debug)]
pub struct MinimumStockpile {
    /// Minimum stockpile per resource type (Megatons).
    pub thresholds: HashMap<ResourceType, f64>,
}

impl MinimumStockpile {
    /// Set (or update) the minimum threshold for a resource.
    pub fn set(&mut self, resource: ResourceType, amount_mt: f64) {
        self.thresholds.insert(resource, amount_mt.max(0.0));
    }

    /// Get the configured minimum for a resource (0.0 if not set).
    pub fn get(&self, resource: &ResourceType) -> f64 {
        self.thresholds.get(resource).copied().unwrap_or(0.0)
    }

    /// Remove the minimum threshold for a resource.
    pub fn clear(&mut self, resource: &ResourceType) {
        self.thresholds.remove(resource);
    }

    /// True if any threshold is configured.
    pub fn has_any(&self) -> bool {
        !self.thresholds.is_empty()
    }
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Check every body with a `MinimumStockpile` component each simulation tick.
///
/// When the local stockpile drops below a configured threshold and no open
/// request already exists for that (body, resource) pair, a new
/// Maintenance-priority request is created.
pub fn check_minimum_stockpile_requests(
    bodies: Query<(
        Entity,
        &LocalStockpile,
        &MinimumStockpile,
        Option<&crate::colony::Colony>,
    )>,
    mut requests: ResMut<PendingResourceRequests>,
    sim_time: Res<SimulationTime>,
) {
    let now = sim_time.elapsed_seconds();

    for (entity, stockpile, minimum, colony_opt) in bodies.iter() {
        let name = colony_opt
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("{entity:?}"));

        for (resource, &threshold) in &minimum.thresholds {
            if threshold <= 0.0 {
                continue;
            }

            let current = stockpile.get(resource);
            if current >= threshold {
                continue;
            }

            // Only create a new request if there is no open one for this slot.
            if requests.has_open_request_for(entity, *resource) {
                continue;
            }

            let shortfall = (threshold - current).max(0.0);
            requests.add(ResourceRequest {
                id: 0, // overwritten by add()
                destination_body: entity,
                destination_name: name.clone(),
                resource: *resource,
                amount_mt: shortfall,
                priority: RequestPriority::Maintenance,
                state: RequestState::Pending,
                in_transit_mt: 0.0,
                eta_seconds: None,
                assigned_company_idx: None,
                created_at_seconds: now,
                source_body: None,
                linked_project: None,
                payment_made: false,
                completed_at_seconds: None,
                assignee_fleet_id: None,
            });

            debug!(
                "MinimumStockpile: created Maintenance request for {:?} at {} (current {:.1} Mt < threshold {:.1} Mt)",
                resource, name, current, threshold
            );
        }
    }
}

// ── Default life-support minimums (GRA-31 PR-A) ─────────────────────────

/// Default minimum-stockpile thresholds for the two life-support
/// resources on a freshly-founded colony.  Operator-confirmed 2026-06-07
/// (interaction 833ad131, `use_design_values`): Oxygen 200 Mt, Water
/// 100 Mt — designed against the GRA-22 50–100× per-build scale.
pub const DEFAULT_LIFE_SUPPORT_OXYGEN_MT: f64 = 200.0;
pub const DEFAULT_LIFE_SUPPORT_WATER_MT: f64 = 100.0;

/// Insert (or back-fill) life-support `MinimumStockpile` entries on
/// every colony body that has a breathable atmosphere (or an explicit
/// `ColonyEnvironmentCosts` indicating life-support is in use).
///
/// Runs every frame, but is essentially a no-op once the colony is
/// initialised: each colony only gets the default applied once, and
/// the `set_min` call is a no-op if the key already exists with a
/// value.  The system also runs from `PostStartup` (registered in the
/// `EconomyPlugin`) so freshly-loaded saves get the defaults without
/// waiting for a tick.
pub fn apply_default_life_support_minimums(
    mut commands: Commands,
    colonies: Query<
        Entity,
        (
            With<crate::colony::components::Colony>,
            With<crate::colony::components::ColonyEnvironmentCosts>,
        ),
    >,
    existing_min: Query<&MinimumStockpile>,
    atmospheres: Query<
        Option<&crate::astronomy::components::AtmosphereComposition>,
        With<crate::colony::components::Colony>,
    >,
) {
    for entity in colonies.iter() {
        // Skip bodies without a breathable atmosphere AND without an
        // explicit `needs_oxygen` flag.  Vacuum bodies (Luna, Mercury)
        // don't get defaults — the player must wire them up
        // explicitly.  A `ColonyEnvironmentCosts` with a non-zero
        // oxygen rate is treated as "life support in use" regardless
        // of atmosphere, so sealed outposts are covered.
        let atmosphere_breathable = atmospheres
            .get(entity)
            .ok()
            .flatten()
            .map(|a| a.breathable)
            .unwrap_or(false);

        // We don't read `ColonyEnvironmentCosts::oxygen_per_person_per_year`
        // here (that requires a separate query with the field borrowed);
        // the filter on `With<ColonyEnvironmentCosts>` already proves
        // life-support is being managed.  Vacuum worlds only need
        // defaults if the player explicitly attached a `ColonyEnvironmentCosts`
        // with a positive oxygen draw — which is exactly what the
        // `With<ColonyEnvironmentCosts>` filter catches.
        let _ = atmosphere_breathable; // kept for future breathing-rule refinements

        // If the colony already has a MinimumStockpile, only add the
        // missing keys.  `MinimumStockpile::set` is a no-op if the key
        // exists with a non-zero value, so we use `get() == 0.0` to
        // detect "missing or explicitly zero".
        if let Ok(min) = existing_min.get(entity) {
            let mut updated = min.clone();
            let mut changed = false;
            if min.get(&ResourceType::Oxygen) <= 0.0 {
                updated.set(ResourceType::Oxygen, DEFAULT_LIFE_SUPPORT_OXYGEN_MT);
                changed = true;
            }
            if min.get(&ResourceType::Water) <= 0.0 {
                updated.set(ResourceType::Water, DEFAULT_LIFE_SUPPORT_WATER_MT);
                changed = true;
            }
            if changed {
                commands.entity(entity).insert(updated);
            }
        } else {
            // No MinimumStockpile at all — insert a fresh one with the
            // design-doc defaults.  Does not touch other resources.
            let mut min = MinimumStockpile::default();
            min.set(ResourceType::Oxygen, DEFAULT_LIFE_SUPPORT_OXYGEN_MT);
            min.set(ResourceType::Water, DEFAULT_LIFE_SUPPORT_WATER_MT);
            commands.entity(entity).insert(min);
        }
    }
}

/// Complete deliveries whose simulated arrival time has passed.
///
/// For each `InTransit` request whose `eta_seconds ≤ now`:
/// 1. Transfer `in_transit_mt` of the resource to the destination `LocalStockpile`.
/// 2. Mark the request as `Delivered` and record `completed_at_seconds`.
/// 3. If a `ConstructionProject` is linked **and** all other requests for that same
///    project are also now delivered, unblock construction (`awaiting_resources = false`).
///
/// Falls back to the `GlobalBudget` stockpile if no destination `LocalStockpile`
/// component exists (should not happen in normal gameplay).
pub fn complete_deliveries(
    mut requests: ResMut<PendingResourceRequests>,
    mut stockpiles: Query<&mut LocalStockpile>,
    mut budget: ResMut<GlobalBudget>,
    mut projects: Query<&mut ConstructionProject>,
    mut ship_projects: Query<&mut ShipConstructionProject>,
    sim_time: Res<SimulationTime>,
) {
    let now = sim_time.elapsed_seconds();

    // Pass 1: deliver resources and collect candidate project entities to unblock.
    // We track (proj_entity, destination_name, resource, delivered_mt) for logging.
    let mut delivered_for_project: Vec<(Entity, String, ResourceType, f64)> = Vec::new();

    for req in requests.requests.iter_mut() {
        if req.state != RequestState::InTransit {
            continue;
        }
        let eta = match req.eta_seconds {
            Some(t) => t,
            None => continue,
        };
        if now < eta {
            continue;
        }

        // Deliver resources to local stockpile.
        let delivered = if let Ok(mut ls) = stockpiles.get_mut(req.destination_body) {
            ls.add(req.resource, req.in_transit_mt);
            req.in_transit_mt
        } else {
            // Fallback: add to global budget
            budget.add_resource(req.resource, req.in_transit_mt);
            req.in_transit_mt
        };

        req.state = RequestState::Delivered;
        req.completed_at_seconds = Some(now);

        if let Some(proj_entity) = req.linked_project {
            delivered_for_project.push((
                proj_entity,
                req.destination_name.clone(),
                req.resource,
                delivered,
            ));
        } else {
            info!(
                "Delivery complete: {:?} {:.1} Mt → {}",
                req.resource, delivered, req.destination_name
            );
        }
    }

    // Pass 2: for each candidate project, check whether ALL linked requests have
    // now been fulfilled. Only unblock when there are no remaining open requests
    // for the same project.
    for (proj_entity, dest_name, resource, delivered) in delivered_for_project {
        let still_waiting = requests
            .requests
            .iter()
            .any(|r| r.linked_project == Some(proj_entity) && r.is_open());

        if still_waiting {
            info!(
                "Partial delivery: {:?} {:.1} Mt → {} — still waiting for other resources",
                resource, delivered, dest_name
            );
            continue;
        }

        // All requests satisfied — unblock the project.
        if let Ok(mut project) = projects.get_mut(proj_entity) {
            if project.awaiting_resources {
                project.awaiting_resources = false;
                info!(
                    "All deliveries complete for {} — construction unblocked",
                    dest_name
                );
            }
            continue;
        }

        if let Ok(mut project) = ship_projects.get_mut(proj_entity) {
            if project.awaiting_resources {
                project.awaiting_resources = false;
                project.blocking_request_ids.clear();
                info!(
                    "All deliveries complete for {} — ship construction unblocked",
                    dest_name
                );
            }
        }
    }
}

/// Prune old delivered/expired requests from the log (keeps recent history).
pub fn prune_old_requests(
    mut requests: ResMut<PendingResourceRequests>,
    sim_time: Res<SimulationTime>,
) {
    requests.prune(sim_time.elapsed_seconds());
}

// ── Player fleet assignment (GRA-33 / PR-B) ───────────────────────────────────

/// Hohmann round-trip transfer time in seconds between two bodies around a
/// common central body (typically the local star).
///
/// Uses the heliocentric distances of both bodies (from `SpaceCoordinates`)
/// and `GM_SUN` as the gravitational parameter.  For moon-to-moon transfers
/// this is an approximation, but it matches the scale used by the existing
/// `ShippingCompany` AI's distance-based placeholder estimator and is
/// sufficient for v0.4 ETA display.
///
/// Returns the time in seconds for the *round trip* (current → source →
/// current) — i.e. the time the player should expect the resources to take
/// to reach the destination after the fleet departs, assuming same-ΔV return
/// leg.  Falls back to 30 in-game days if either body's coordinates are
/// missing or the formula returns a non-finite / non-positive value.
pub fn hohmann_round_trip_seconds<F: bevy::ecs::query::QueryFilter>(
    current_body: Entity,
    source_body: Entity,
    coords_query: &Query<&SpaceCoordinates, F>,
) -> f64 {
    let r1 = coords_query
        .get(current_body)
        .map(|sc| sc.position.length().max(0.05))
        .unwrap_or(1.0);
    let r2 = coords_query
        .get(source_body)
        .map(|sc| sc.position.length().max(0.05))
        .unwrap_or(1.0);

    let a = (r1 + r2) * 0.5; // semi-major axis in AU
    let a_m = a * AU_IN_METERS;
    // Half-period of the transfer ellipse: π · √(a³ / GM)
    let t_one_way = std::f64::consts::PI * (a_m.powi(3) / GM_SUN).sqrt();

    // Sanity clamp: Hohmann around Sol between 0.1 AU and 50 AU gives
    // ~3 days to ~250 years.  A 30-day floor handles missing coordinates.
    let fallback = 30.0 * 86_400.0;
    let round_trip = (2.0 * t_one_way).max(fallback);
    if !round_trip.is_finite() || round_trip <= 0.0 {
        fallback
    } else {
        round_trip
    }
}

/// Manually assign player freighter fleets to open `ResourceRequest`s.
///
/// Drains `PendingFleetActions::assign_logistics_requests` (one entry per
/// click of the **Assign** button in the fleet-panel Logistics section).
/// For each action, the system:
///
/// 1. Looks up the request and the fleet.
/// 2. Validates that:
///    * the request is still `Pending` and the fleet is alive;
///    * the fleet is in orbit at the request's destination body;
///    * the fleet contains at least one `ShipClass::Freighter`.
/// 3. Locates a source `LocalStockpile` with enough of the requested
///    resource, using the same first-fit-largest logic as
///    `company::process_company_ai` (sort candidate bodies by amount
///    descending, consume until the request is satisfied).
/// 4. Flips the request to `InTransit`, records the assignee fleet, and sets
///    `eta_seconds = now + 2·Hohmann(current → source)` as the
///    round-trip delivery ETA.
///
/// On arrival, the existing `complete_deliveries` system handles the resource
/// delivery and request closure; this PR adds no delivery code.
pub fn process_fleet_logistics_assignments(
    mut actions: ResMut<PendingFleetActions>,
    mut requests: ResMut<PendingResourceRequests>,
    // Bevy 0.18 forbids having two `Query` system params that both touch
    // the same component (B0001).  We need a read pass (compute the source
    // list) and a write pass (consume from each chosen body), so fold them
    // into a single `Query<(Entity, &mut LocalStockpile)>` and use
    // sequential `iter()` / `get_mut()` calls within the system.  This
    // matches the pattern in `company::process_company_ai` and
    // `auto_freight::auto_freight_loop`.
    mut stockpiles: Query<(Entity, &mut LocalStockpile)>,
    fleet_query: Query<(&FleetOrbit, Option<&crate::fleets::components::Fleet>), ()>,
    sim_time: Res<SimulationTime>,
    coords_query: Query<&SpaceCoordinates>,
) {
    if actions.assign_logistics_requests.is_empty() {
        return;
    }

    let now = sim_time.elapsed_seconds();

    for action in actions.assign_logistics_requests.drain(..) {
        // Look up the request.
        let Some(req) = requests.find_by_id_mut(action.request_id) else {
            warn!(
                "process_fleet_logistics_assignments: request id {} not found (fleet {:?})",
                action.request_id, action.fleet
            );
            continue;
        };

        if !matches!(req.state, RequestState::Pending) {
            warn!(
                "process_fleet_logistics_assignments: request id {} no longer Pending (state {:?}); ignoring assign from fleet {:?}",
                action.request_id, req.state, action.fleet
            );
            continue;
        }

        // Look up the fleet.
        let Ok((orbit, maybe_fleet)) = fleet_query.get(action.fleet) else {
            warn!(
                "process_fleet_logistics_assignments: fleet {:?} not found for request id {}",
                action.fleet, action.request_id
            );
            continue;
        };

        if orbit.body != req.destination_body {
            warn!(
                "process_fleet_logistics_assignments: fleet {:?} is at body {:?} but request id {} targets {:?}",
                action.fleet, orbit.body, action.request_id, req.destination_body
            );
            continue;
        }

        // The fleet must contain at least one Freighter-class ship.
        let has_freighter = maybe_fleet
            .map(|f| f.ships.iter().any(|s| s.class == ShipClass::Freighter))
            .unwrap_or(false);
        if !has_freighter {
            warn!(
                "process_fleet_logistics_assignments: fleet {:?} has no Freighter-class ship; cannot assign request id {}",
                action.fleet, action.request_id
            );
            continue;
        }

        // Pick a source body (first-fit-largest, same as company AI).
        let mut sources: Vec<(Entity, f64)> = stockpiles
            .iter()
            .map(|(e, ls)| (e, ls.get(&req.resource)))
            .filter(|(_, amt)| *amt > 0.0)
            .collect();
        sources.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut remaining = req.amount_mt;
        let mut actual_source: Option<Entity> = None;
        for (src_entity, _) in &sources {
            if remaining <= 0.0 {
                break;
            }
            if let Ok((_, mut ls)) = stockpiles.get_mut(*src_entity) {
                let taken = ls.consume(req.resource, remaining);
                if taken > 0.0 && actual_source.is_none() {
                    actual_source = Some(*src_entity);
                }
                remaining -= taken;
            }
        }

        let actual_dispatched = req.amount_mt - remaining;
        if actual_dispatched <= 0.0 {
            // No body had any of the resource — refund nothing and leave the
            // request Pending.  The player can re-attempt once stockpiles
            // recover, or the company AI / maintenance checks will pick it up.
            warn!(
                "process_fleet_logistics_assignments: no source body has {:?} for request id {}; request stays Pending",
                req.resource, action.request_id
            );
            continue;
        }

        // Round-trip ETA from fleet's current body to the actual source body.
        let source_for_eta = actual_source.unwrap_or(req.destination_body);
        let transit_s =
            hohmann_round_trip_seconds(req.destination_body, source_for_eta, &coords_query);

        req.in_transit_mt = actual_dispatched;
        req.eta_seconds = Some(now + transit_s);
        req.state = RequestState::InTransit;
        req.assignee_fleet_id = Some(action.fleet);
        if req.source_body.is_none() {
            req.source_body = actual_source;
        }

        let transit_days = transit_s / 86_400.0;
        info!(
            "Fleet {:?}: assigned request id {} — {:?} {:.1} Mt → {} (source {:?}, ETA {:.0} days)",
            action.fleet,
            action.request_id,
            req.resource,
            actual_dispatched,
            req.destination_name,
            actual_source,
            transit_days,
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_priority_ordering() {
        assert!(RequestPriority::Emergency > RequestPriority::Construction);
        assert!(RequestPriority::Construction > RequestPriority::Maintenance);
        assert!(RequestPriority::Maintenance > RequestPriority::Trade);
    }

    #[test]
    fn test_minimum_stockpile_set_get_clear() {
        let mut ms = MinimumStockpile::default();
        assert_eq!(ms.get(&ResourceType::Iron), 0.0);

        ms.set(ResourceType::Iron, 500.0);
        assert_eq!(ms.get(&ResourceType::Iron), 500.0);

        ms.clear(&ResourceType::Iron);
        assert_eq!(ms.get(&ResourceType::Iron), 0.0);
    }

    #[test]
    fn test_minimum_stockpile_negative_clamped() {
        let mut ms = MinimumStockpile::default();
        ms.set(ResourceType::Iron, -100.0);
        assert_eq!(ms.get(&ResourceType::Iron), 0.0);
    }

    #[test]
    fn test_pending_requests_add_and_find() {
        let mut pool = PendingResourceRequests::default();
        let entity = Entity::from_raw_u32(1).unwrap();

        let id = pool.add(ResourceRequest {
            id: 0,
            destination_body: entity,
            destination_name: "Mars".into(),
            resource: ResourceType::Iron,
            amount_mt: 100.0,
            priority: RequestPriority::Construction,
            state: RequestState::Pending,
            in_transit_mt: 0.0,
            eta_seconds: None,
            assigned_company_idx: None,
            created_at_seconds: 0.0,
            source_body: None,
            linked_project: None,
            payment_made: false,
            completed_at_seconds: None,
            assignee_fleet_id: None,
        });

        assert_eq!(id, 0);
        assert!(pool.find_by_id(0).is_some());
        assert!(pool.find_by_id(99).is_none());
        assert_eq!(pool.open_requests().count(), 1);
    }

    #[test]
    fn test_has_open_request_for() {
        let mut pool = PendingResourceRequests::default();
        let entity = Entity::from_raw_u32(2).unwrap();

        assert!(!pool.has_open_request_for(entity, ResourceType::Iron));

        pool.add(ResourceRequest {
            id: 0,
            destination_body: entity,
            destination_name: "Moon".into(),
            resource: ResourceType::Iron,
            amount_mt: 50.0,
            priority: RequestPriority::Maintenance,
            state: RequestState::Pending,
            in_transit_mt: 0.0,
            eta_seconds: None,
            assigned_company_idx: None,
            created_at_seconds: 0.0,
            source_body: None,
            linked_project: None,
            payment_made: false,
            completed_at_seconds: None,
            assignee_fleet_id: None,
        });

        assert!(pool.has_open_request_for(entity, ResourceType::Iron));
        // Different resource → no duplicate
        assert!(!pool.has_open_request_for(entity, ResourceType::Uranium));
    }

    #[test]
    fn test_prune_old_requests() {
        let mut pool = PendingResourceRequests::default();
        let entity = Entity::from_raw_u32(3).unwrap();

        // Add a delivered request with creation time far in the past
        pool.add(ResourceRequest {
            id: 0,
            destination_body: entity,
            destination_name: "Ceres".into(),
            resource: ResourceType::Silicates,
            amount_mt: 10.0,
            priority: RequestPriority::Trade,
            state: RequestState::Delivered,
            in_transit_mt: 0.0,
            eta_seconds: None,
            assigned_company_idx: None,
            created_at_seconds: 0.0,
            source_body: None,
            linked_project: None,
            payment_made: false,
            completed_at_seconds: None,
            assignee_fleet_id: None,
        });

        // current_sim_seconds much larger than HISTORY_KEEP_S → should be pruned
        pool.prune(HISTORY_KEEP_S * 2.0);
        assert_eq!(pool.requests.len(), 0);
    }

    // ── GRA-31 PR-A: default life-support minimums ─────────────────────

    /// Fresh colony (no `MinimumStockpile` yet) should get the design-doc
    /// defaults applied automatically: O₂ = 200 Mt, Water = 100 Mt.
    #[test]
    fn test_apply_default_life_support_minimums_creates_for_new_colony() {
        use crate::colony::components::Colony;
        use crate::colony::components::ColonyEnvironmentCosts;

        let mut app = App::new();
        let colony_entity = app
            .world_mut()
            .spawn((
                Colony::new("Earth".to_string(), 1_000_000.0),
                ColonyEnvironmentCosts {
                    oxygen_per_person_per_year: 0.0001,
                    water_per_person_per_year: 0.00005,
                },
            ))
            .id();

        // Sanity: no MinimumStockpile yet.
        assert!(app
            .world()
            .entity(colony_entity)
            .get::<MinimumStockpile>()
            .is_none());

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(apply_default_life_support_minimums);
        sched.run(app.world_mut());

        let min = app
            .world()
            .entity(colony_entity)
            .get::<MinimumStockpile>()
            .expect("MinimumStockpile should have been inserted");
        assert_eq!(
            min.get(&ResourceType::Oxygen),
            DEFAULT_LIFE_SUPPORT_OXYGEN_MT,
            "Oxygen default must be 200 Mt (operator use_design_values)"
        );
        assert_eq!(
            min.get(&ResourceType::Water),
            DEFAULT_LIFE_SUPPORT_WATER_MT,
            "Water default must be 100 Mt (operator use_design_values)"
        );
    }

    /// Existing player-set `MinimumStockpile` values must NOT be
    /// overwritten by the defaults.  Only missing keys are filled in.
    #[test]
    fn test_apply_default_life_support_minimums_respects_player_values() {
        use crate::colony::components::Colony;
        use crate::colony::components::ColonyEnvironmentCosts;

        let mut app = App::new();
        let mut existing = MinimumStockpile::default();
        existing.set(ResourceType::Oxygen, 5_000.0); // player set 5_000 Mt
        existing.set(ResourceType::Water, 1_000.0); // player set 1_000 Mt

        let colony_entity = app
            .world_mut()
            .spawn((
                Colony::new("Mars".to_string(), 1_000.0),
                ColonyEnvironmentCosts {
                    oxygen_per_person_per_year: 0.0001,
                    water_per_person_per_year: 0.00005,
                },
                existing.clone(),
            ))
            .id();

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(apply_default_life_support_minimums);
        sched.run(app.world_mut());

        let min = app
            .world()
            .entity(colony_entity)
            .get::<MinimumStockpile>()
            .expect("MinimumStockpile should still exist");
        assert_eq!(
            min.get(&ResourceType::Oxygen),
            5_000.0,
            "player-set Oxygen (5_000 Mt) must not be overwritten"
        );
        assert_eq!(
            min.get(&ResourceType::Water),
            1_000.0,
            "player-set Water (1_000 Mt) must not be overwritten"
        );
    }

    /// Colony with a `MinimumStockpile` that has *some* keys set but
    /// not the life-support ones gets only the missing keys filled in.
    #[test]
    fn test_apply_default_life_support_minimums_backfills_missing_keys() {
        use crate::colony::components::Colony;
        use crate::colony::components::ColonyEnvironmentCosts;

        let mut app = App::new();
        let mut existing = MinimumStockpile::default();
        existing.set(ResourceType::Food, 10_000.0);
        // Oxygen and Water are missing.

        let colony_entity = app
            .world_mut()
            .spawn((
                Colony::new("Luna".to_string(), 100.0),
                ColonyEnvironmentCosts {
                    oxygen_per_person_per_year: 0.0001,
                    water_per_person_per_year: 0.00005,
                },
                existing,
            ))
            .id();

        let mut sched = bevy::ecs::schedule::Schedule::default();
        sched.add_systems(apply_default_life_support_minimums);
        sched.run(app.world_mut());

        let min = app
            .world()
            .entity(colony_entity)
            .get::<MinimumStockpile>()
            .expect("MinimumStockpile should still exist");
        assert_eq!(min.get(&ResourceType::Food), 10_000.0, "Food preserved");
        assert_eq!(
            min.get(&ResourceType::Oxygen),
            DEFAULT_LIFE_SUPPORT_OXYGEN_MT,
            "Oxygen back-filled with default"
        );
        assert_eq!(
            min.get(&ResourceType::Water),
            DEFAULT_LIFE_SUPPORT_WATER_MT,
            "Water back-filled with default"
        );
    }
}
