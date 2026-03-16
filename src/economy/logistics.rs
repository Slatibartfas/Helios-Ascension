//! Resource request and minimum-stockpile logistics system.
//!
//! Each time a building is queued (or an outpost founded) and the destination
//! body's *local* stockpile does not have enough materials, a `ResourceRequest`
//! is created.  Requests are fulfilled either by:
//!
//! 1. A player-controlled Freighter fleet (manual — future fleet-panel integration).
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

use crate::colony::components::ConstructionProject;
use crate::economy::components::LocalStockpile;
use crate::economy::types::ResourceType;
use crate::economy::GlobalBudget;
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
                    .map(|t| (current_sim_seconds - t) < HISTORY_KEEP_S)
                    .unwrap_or(true) // keep if completed_at not set yet (shouldn't happen)
        });
    }

    /// True if there is already an open request for this body+resource combination.
    pub fn has_open_request_for(&self, body: Entity, resource: ResourceType) -> bool {
        self.requests
            .iter()
            .any(|r| r.destination_body == body && r.resource == resource && r.is_open())
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
    bodies: Query<(Entity, &LocalStockpile, &MinimumStockpile, Option<&crate::colony::Colony>)>,
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
            });

            debug!(
                "MinimumStockpile: created Maintenance request for {:?} at {} (current {:.1} Mt < threshold {:.1} Mt)",
                resource, name, current, threshold
            );
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
        });

        // current_sim_seconds much larger than HISTORY_KEEP_S → should be pruned
        pool.prune(HISTORY_KEEP_S * 2.0);
        assert_eq!(pool.requests.len(), 0);
    }
}
