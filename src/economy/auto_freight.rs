//! Automated freight AI for private `ShippingCompany` operators (GRA-38).
//!
//! When a `ShippingCompany` has `CompanyAIPolicy::AutoFreight`, the loop scans
//! `PendingResourceRequests` each tick and assigns an **idle player freighter
//! fleet** (any `Fleet` containing a `ShipClass::Freighter`, currently in
//! `FleetOrbit` at the request's destination body, not in transit) to the
//! highest-priority open request.  The same first-fit-largest heuristic is
//! used as the manual-assign path in
//! `logistics::process_fleet_logistics_assignments`; on assignment the
//! request flips to `InTransit`, the freighter's fleet is recorded, and the
//! delivery ETA is computed from a Hohmann round-trip transfer plan.
//!
//! `Manual` companies never participate in this loop — the player must take
//! deliveries via the fleet panel's Logistics section.
//!
//! When the loop has open requests that no AutoFreight company can service
//! (no idle player freighter at the destination AND no abstract company
//! freighter available), a throttled `FreighterNoDesignAvailable` event is
//! emitted so the UI can surface the situation to the player.  This is a
//! placeholder for the future ship-template model (GRA-40); for now it
//! indicates that the player has too few freighters to satisfy current
//! logistics demand.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::astronomy::components::SpaceCoordinates;
use crate::economy::company::{ShippingCompanies, ShippingCompany};
use crate::economy::components::LocalStockpile;
use crate::economy::logistics::{
    hohmann_round_trip_seconds, PendingResourceRequests, RequestState, ResourceRequest,
};
use crate::economy::types::ResourceType;
use crate::fleets::{Fleet, FleetOrbit, ShipClass};
use crate::ui::SimulationTime;

// ── CompanyAIPolicy ───────────────────────────────────────────────────────────

/// AI policy governing automated freight behaviour for a `ShippingCompany`
/// (GRA-38 / GRA-37).
///
/// `AutoFreight` is the default for new companies (DW2-style opt-out per the
/// operator resolution of ask_user_questions `17513eac-…` 2026-06-07):
/// companies automatically claim open `ResourceRequest`s and dispatch their
/// freighters (abstract counter + idle player fleets at the body).
///
/// `Manual` companies do nothing on their own; the player must take
/// deliveries via the fleet panel's manual-assign path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompanyAIPolicy {
    /// No automated freight.  The player (or another AI company) must
    /// assign freighters manually.
    Manual,
    /// Auto-assign idle player freighters to open `ResourceRequest`s each
    /// tick.  Default for new companies.
    #[default]
    AutoFreight,
}

impl std::fmt::Display for CompanyAIPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompanyAIPolicy::Manual => write!(f, "Manual"),
            CompanyAIPolicy::AutoFreight => write!(f, "Auto-Freight"),
        }
    }
}

// ── No-design notification message ───────────────────────────────────────────

/// Emitted (throttled) when the auto-freight loop has open requests that no
/// AutoFreight company can currently service.
///
/// GRA-38 acceptance criterion #6: when the assign loop can't match an open
/// `ResourceRequest` to any matching freighter, surface it to the UI so the
/// player knows logistics demand is unmet.  This is a structural message —
/// the future ship-template gate from GRA-40 will be added on top.
///
/// Uses Bevy 0.18's `Message` trait (the buffered successor to `Event`); the
/// UI consumes these via `MessageReader<FreighterNoDesignAvailable>`.
#[derive(Message, Debug, Clone)]
pub struct FreighterNoDesignAvailable {
    pub request_id: u64,
    pub destination_body: Entity,
    pub resource: ResourceType,
    pub amount_mt: f64,
}

// ── Throttle state ───────────────────────────────────────────────────────────

/// Per-`ResourceRequest` throttling so we don't spam the event log +
/// notification UI every tick for the same unfulfilled request.
#[derive(Resource, Default, Debug)]
pub struct AutoFreightNotificationState {
    /// `(request_id, last_complained_sim_seconds)` map.
    last_complained: HashMap<u64, f64>,
}

/// Throttle window for `FreighterNoDesignAvailable` events (sim seconds).
/// One in-game day — long enough that the UI doesn't flicker, short enough
/// that newly-arriving freighters produce a fresh signal.
const NO_DESIGN_THROTTLE_S: f64 = 86_400.0;

// ── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin wiring the auto-freight loop into the schedule.
///
/// Registers the `FreighterNoDesignAvailable` event, the throttle resource,
/// and the `auto_freight_loop` system in `Update`, ordered after the
/// abstract `process_company_ai` so it sees a stable view of `Pending`
/// requests and `available_freighters` counters.
pub struct AutoFreightPlugin;

impl Plugin for AutoFreightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AutoFreightNotificationState>()
            .add_message::<FreighterNoDesignAvailable>()
            .add_systems(
                Update,
                auto_freight_loop
                    .after(crate::economy::company::process_company_ai)
                    .after(crate::economy::logistics::process_fleet_logistics_assignments),
            );
    }
}

// ── System ───────────────────────────────────────────────────────────────────

/// Main auto-freight system.  Runs each tick.
///
/// For each `AutoFreight` company, in order, claim the highest-priority
/// `Pending` `ResourceRequest` and recruit the best idle player freighter
/// at the request's destination body (FleetOrbit, no ActiveManeuver,
/// contains at least one `ShipClass::Freighter`).  Same first-fit source
/// deduction and Hohmann ETA as the manual-assign path; on success the
/// request flips to `InTransit` and the freighter's fleet is recorded.
///
/// Requests that can't be serviced (no idle freighter at the body, or no
/// source body has the resource) emit a throttled
/// `FreighterNoDesignAvailable` event.
#[allow(clippy::too_many_arguments)]
pub fn auto_freight_loop(
    mut companies: ResMut<ShippingCompanies>,
    mut requests: ResMut<PendingResourceRequests>,
    stockpiles_read: Query<(Entity, &LocalStockpile)>,
    mut stockpiles_mut: Query<&mut LocalStockpile>,
    idle_freight_fleets: Query<
        (Entity, &Fleet, &FleetOrbit),
        Without<crate::fleets::ActiveManeuver>,
    >,
    coords_query: Query<&SpaceCoordinates>,
    sim_time: Res<SimulationTime>,
    mut notif_state: ResMut<AutoFreightNotificationState>,
    mut no_design_events: MessageWriter<FreighterNoDesignAvailable>,
) {
    // Indexes of AutoFreight companies — these are the ones we'll service.
    let auto_freight_indices: Vec<usize> = companies
        .companies
        .iter()
        .enumerate()
        .filter(|(_, c)| c.policy == CompanyAIPolicy::AutoFreight)
        .map(|(i, _)| i)
        .collect();
    if auto_freight_indices.is_empty() {
        return;
    }

    // Collect indices of currently-Pending requests (per the spec: "filtered
    // to state == Open").  Open = Pending in our state machine; Assigned is
    // reserved for the future two-stage pickup transit (see
    // `logistics::RequestState`).
    let mut pending_indices: Vec<usize> = requests
        .requests
        .iter()
        .enumerate()
        .filter(|(_, r)| r.state == RequestState::Pending)
        .map(|(i, _)| i)
        .collect();
    if pending_indices.is_empty() {
        return;
    }
    pending_indices.sort_by(|&a, &b| {
        let ra = &requests.requests[a];
        let rb = &requests.requests[b];
        rb.priority.cmp(&ra.priority).then(
            ra.created_at_seconds
                .partial_cmp(&rb.created_at_seconds)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let now = sim_time.elapsed_seconds();

    for &company_idx in &auto_freight_indices {
        if pending_indices.is_empty() {
            break;
        }

        // Highest-priority open request, considering the *current* snapshot
        // (the loop below drains `pending_indices` as it succeeds).
        let req_idx = pending_indices[0];
        let dest = requests.requests[req_idx].destination_body;

        // Pick the best idle freighter at the request's destination body.
        // "Best" = the fleet with the most freighter-class ships (proxy for
        // cargo capacity until `ShipInfo::cargo_capacity_t` lands in a
        // future PR — see GRA-37.b / GRA-40 for the full template model).
        let mut best: Option<(Entity, usize)> = None; // (fleet_entity, freighter_count)
        for (fleet_entity, fleet, orbit) in idle_freight_fleets.iter() {
            if orbit.body != dest {
                continue;
            }
            let freighter_count = fleet
                .ships
                .iter()
                .filter(|s| s.class == ShipClass::Freighter)
                .count();
            if freighter_count == 0 {
                continue;
            }
            if best.is_none_or(|(_, c)| freighter_count > c) {
                best = Some((fleet_entity, freighter_count));
            }
        }

        let Some((fleet_entity, _freighter_count)) = best else {
            // No idle player freighter at this body.  Throttled no-design
            // notification so the player sees the unmet demand, then drop
            // the request from this tick's queue.
            maybe_emit_no_design(
                &requests.requests[req_idx],
                &mut notif_state,
                now,
                &mut no_design_events,
            );
            pending_indices.remove(0);
            continue;
        };

        // Deduct from source LocalStockpile (first-fit-largest), mirroring
        // the manual-assign path.  `requests.requests[req_idx]` borrow
        // dropped before the mutable call.
        let req_snapshot = requests.requests[req_idx].clone();
        if !deduct_from_source(
            &req_snapshot.resource,
            req_snapshot.amount_mt,
            &stockpiles_read,
            &mut stockpiles_mut,
        ) {
            // No body has the resource.  Don't emit a no-design event here
            // — that's a *production* problem, not a *freight* problem.
            pending_indices.remove(0);
            continue;
        }

        // Compute Hohmann round-trip ETA from the request's destination to
        // its source body (if any), or to itself as a fallback.
        let eta_source = req_snapshot
            .source_body
            .unwrap_or(req_snapshot.destination_body);
        let transit_s =
            hohmann_round_trip_seconds(req_snapshot.destination_body, eta_source, &coords_query);

        // Mutate the request: deduct already applied above, now flip state
        // and stamp the freighter + ETA.
        let req = &mut requests.requests[req_idx];
        req.in_transit_mt = req.amount_mt;
        req.eta_seconds = Some(now + transit_s);
        req.state = RequestState::InTransit;
        req.assignee_fleet_id = Some(fleet_entity);
        if req.source_body.is_none() {
            req.source_body = req_snapshot.source_body;
        }

        // Charge the company for using a freighter slot.  This treats the
        // player fleet as a virtual asset of the company for accounting —
        // the `available_freighters` counter still drives
        // `process_company_ai`, so the two paths don't double-spend.
        let company: &mut ShippingCompany = &mut companies.companies[company_idx];
        company.assign_freighter();

        let transit_days = transit_s / 86_400.0;
        info!(
            "AutoFreight: company {} assigned fleet {:?} → request {} ({:?} {:.1} Mt → {}, ETA {:.0} d)",
            company.name,
            fleet_entity,
            req.id,
            req.resource,
            req.amount_mt,
            req.destination_name,
            transit_days,
        );

        // This request is no longer open; remove it from the per-tick queue.
        pending_indices.remove(0);
    }
}

fn maybe_emit_no_design(
    req: &ResourceRequest,
    state: &mut AutoFreightNotificationState,
    now: f64,
    events: &mut MessageWriter<FreighterNoDesignAvailable>,
) {
    let last = state
        .last_complained
        .get(&req.id)
        .copied()
        .unwrap_or(f64::NEG_INFINITY);
    if (now - last) < NO_DESIGN_THROTTLE_S {
        return;
    }
    state.last_complained.insert(req.id, now);
    events.write(FreighterNoDesignAvailable {
        request_id: req.id,
        destination_body: req.destination_body,
        resource: req.resource,
        amount_mt: req.amount_mt,
    });
}

/// First-fit-largest deduction across all `LocalStockpile`s.  Returns true
/// only if the full amount was satisfied.  Mirrors the logic in
/// `logistics::process_fleet_logistics_assignments` and
/// `company::process_company_ai`.
fn deduct_from_source(
    resource: &ResourceType,
    amount: f64,
    stockpiles_read: &Query<(Entity, &LocalStockpile)>,
    stockpiles_mut: &mut Query<&mut LocalStockpile>,
) -> bool {
    let mut sources: Vec<(Entity, f64)> = stockpiles_read
        .iter()
        .map(|(e, ls)| (e, ls.get(resource)))
        .filter(|(_, amt)| *amt > 0.0)
        .collect();
    sources.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let total: f64 = sources.iter().map(|(_, a)| a).sum();
    if total < amount {
        return false;
    }

    let mut remaining = amount;
    for (src_entity, _) in &sources {
        if remaining <= 0.0 {
            break;
        }
        if let Ok(mut ls) = stockpiles_mut.get_mut(*src_entity) {
            let taken = ls.consume(*resource, remaining);
            remaining -= taken;
        }
    }
    remaining <= 0.0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astronomy::components::SpaceCoordinates;
    use crate::colony::components::Colony;
    use crate::fleets::components::ShipInfo;
    use crate::fleets::types::PropulsionType;

    /// Build a body entity with a `LocalStockpile` containing a given amount
    /// of `ResourceType::Iron`.  Returns the body entity.
    fn spawn_body_with_stockpile(world: &mut World, amount_mt: f64) -> Entity {
        let entity = world
            .spawn((
                Colony::new("Test Colony".into(), 1_000.0),
                LocalStockpile::default(),
                SpaceCoordinates::default(),
            ))
            .id();
        if amount_mt > 0.0 {
            let mut ls = world.get_mut::<LocalStockpile>(entity).unwrap();
            ls.add(ResourceType::Iron, amount_mt);
        }
        entity
    }

    /// Spawn a fleet with a single `Freighter` ship, in orbit at `body`.
    fn spawn_idle_freighter_fleet(world: &mut World, body: Entity) -> Entity {
        let ship = ShipInfo::new(
            "Test Freighter".into(),
            ShipClass::Freighter,
            PropulsionType::Chemical,
        );
        let mut fleet = Fleet::new("Test Fleet".into());
        fleet.ships.push(ship);
        world.spawn((fleet, FleetOrbit::new(body, 0.001))).id()
    }

    /// Init the resources the auto-freight system reads / mutates.
    fn init_econ_resources(world: &mut World) {
        world.init_resource::<PendingResourceRequests>();
        world.init_resource::<ShippingCompanies>();
        world.init_resource::<SimulationTime>();
    }

    /// Build a `Pending` `ResourceRequest` at the given body for `Iron`.
    fn push_pending_iron_request(requests: &mut PendingResourceRequests, dest: Entity) -> u64 {
        requests.add(ResourceRequest {
            id: 0,
            destination_body: dest,
            destination_name: "Test Colony".into(),
            resource: ResourceType::Iron,
            amount_mt: 50.0,
            priority: crate::economy::logistics::RequestPriority::Construction,
            state: RequestState::Pending,
            in_transit_mt: 0.0,
            eta_seconds: None,
            assigned_company_idx: None,
            created_at_seconds: 0.0,
            source_body: Some(dest),
            linked_project: None,
            payment_made: false,
            completed_at_seconds: None,
            assignee_fleet_id: None,
        })
    }

    /// GRA-38 acceptance criterion #6: with one AutoFreight company, one
    /// idle freighter fleet at the same body, and one Open (Pending)
    /// ResourceRequest, the auto-freight loop claims the request, sets
    /// `assignee_fleet_id = fleet.id`, deducts from the source
    /// `LocalStockpile`, sets `eta_seconds`, and the request transitions
    /// to `InTransit`.
    #[test]
    fn test_assigns_open_request() {
        // Bare-bones Bevy app: no plugins, just the resources we need.
        // We run the system manually against a fresh `Schedule` to avoid
        // dragging in the full `EconomyPlugin` (which spawns the real
        // solar system on `PostStartup`).
        let mut app = App::new();
        let mut schedule = Schedule::default();
        init_econ_resources(app.world_mut());

        // World setup.
        let body = spawn_body_with_stockpile(app.world_mut(), 500.0);
        let fleet_entity = spawn_idle_freighter_fleet(app.world_mut(), body);

        // AutoFreight company (DW2 default; explicit here for clarity).
        let mut company = ShippingCompany::new("Test Co.", 0, 0.0);
        company.policy = CompanyAIPolicy::AutoFreight;
        app.world_mut()
            .resource_mut::<ShippingCompanies>()
            .companies = vec![company];

        // One Pending Iron request at the same body.
        let request_id = {
            let mut requests = app.world_mut().resource_mut::<PendingResourceRequests>();
            push_pending_iron_request(&mut requests, body)
        };

        // Run the system once.
        schedule.add_systems(auto_freight_loop);
        schedule.run(app.world_mut());

        // The request should now be InTransit, with the freighter as
        // assignee and an ETA stamped.  Source stockpile should have been
        // drawn down by the request amount.
        let req = app
            .world()
            .resource::<PendingResourceRequests>()
            .find_by_id(request_id)
            .expect("request must still exist after system run");
        assert_eq!(
            req.state,
            RequestState::InTransit,
            "request should have transitioned to InTransit"
        );
        assert_eq!(
            req.assignee_fleet_id,
            Some(fleet_entity),
            "freighter fleet should be the assignee"
        );
        assert!(
            req.eta_seconds.is_some(),
            "eta_seconds should be set after assignment"
        );
        assert!(
            (req.in_transit_mt - 50.0).abs() < 1e-6,
            "in_transit_mt should equal amount_mt"
        );

        let ls = app
            .world()
            .entity(body)
            .get::<LocalStockpile>()
            .expect("body still has LocalStockpile");
        assert!(
            (ls.get(&ResourceType::Iron) - 450.0).abs() < 1e-6,
            "source stockpile should be 500 - 50 = 450 Mt after deduction"
        );
    }

    /// Manual companies must not participate in the auto-freight loop.
    /// The same setup as `test_assigns_open_request` but with policy set
    /// to `Manual` — the request stays Pending and no ETA is stamped.
    #[test]
    fn manual_company_does_not_assign() {
        let mut app = App::new();
        let mut schedule = Schedule::default();
        init_econ_resources(app.world_mut());

        let body = spawn_body_with_stockpile(app.world_mut(), 500.0);
        let _fleet = spawn_idle_freighter_fleet(app.world_mut(), body);

        let mut company = ShippingCompany::new("Manual Co.", 0, 0.0);
        company.policy = CompanyAIPolicy::Manual;
        app.world_mut()
            .resource_mut::<ShippingCompanies>()
            .companies = vec![company];

        let request_id = {
            let mut requests = app.world_mut().resource_mut::<PendingResourceRequests>();
            push_pending_iron_request(&mut requests, body)
        };

        schedule.add_systems(auto_freight_loop);
        schedule.run(app.world_mut());

        let req = app
            .world()
            .resource::<PendingResourceRequests>()
            .find_by_id(request_id)
            .expect("request still present");
        assert_eq!(
            req.state,
            RequestState::Pending,
            "Manual company must not auto-assign"
        );
        assert!(
            req.assignee_fleet_id.is_none(),
            "no fleet should be the assignee"
        );
    }

    /// GRA-38 acceptance criterion #6 (no-design event): when no idle
    /// freighter is at the destination body, the loop emits a
    /// throttled `FreighterNoDesignAvailable` event.
    #[test]
    fn no_design_event_when_no_idle_freighter() {
        let mut app = App::new();
        let mut schedule = Schedule::default();
        init_econ_resources(app.world_mut());
        app.world_mut()
            .init_resource::<Messages<FreighterNoDesignAvailable>>();

        // Body with stockpile, but no freighter at the body.
        let body = spawn_body_with_stockpile(app.world_mut(), 500.0);

        let mut company = ShippingCompany::new("Test Co.", 0, 0.0);
        company.policy = CompanyAIPolicy::AutoFreight;
        app.world_mut()
            .resource_mut::<ShippingCompanies>()
            .companies = vec![company];

        let _request_id = {
            let mut requests = app.world_mut().resource_mut::<PendingResourceRequests>();
            push_pending_iron_request(&mut requests, body)
        };

        schedule.add_systems(auto_freight_loop);
        schedule.run(app.world_mut());

        let events = app
            .world()
            .resource::<Messages<FreighterNoDesignAvailable>>();
        let mut cursor = events.get_cursor();
        let drained: Vec<_> = cursor.read(events).collect();
        assert_eq!(
            drained.len(),
            1,
            "expected exactly one no-design event, got {}",
            drained.len()
        );
    }

    /// The no-design event is throttled — a second consecutive run with
    /// the same unfulfilled request must NOT emit a duplicate event.
    #[test]
    fn no_design_event_is_throttled() {
        let mut app = App::new();
        let mut schedule = Schedule::default();
        init_econ_resources(app.world_mut());
        app.world_mut()
            .init_resource::<Messages<FreighterNoDesignAvailable>>();

        let body = spawn_body_with_stockpile(app.world_mut(), 500.0);
        let mut company = ShippingCompany::new("Test Co.", 0, 0.0);
        company.policy = CompanyAIPolicy::AutoFreight;
        app.world_mut()
            .resource_mut::<ShippingCompanies>()
            .companies = vec![company];

        let _request_id = {
            let mut requests = app.world_mut().resource_mut::<PendingResourceRequests>();
            push_pending_iron_request(&mut requests, body)
        };

        schedule.add_systems(auto_freight_loop);
        // First run: should emit one event.
        schedule.run(app.world_mut());
        // Second run back-to-back: throttled, no new event.
        schedule.run(app.world_mut());

        let events = app
            .world()
            .resource::<Messages<FreighterNoDesignAvailable>>();
        let mut cursor = events.get_cursor();
        let drained: Vec<_> = cursor.read(events).collect();
        assert_eq!(
            drained.len(),
            1,
            "throttle must suppress duplicate events: got {}",
            drained.len()
        );
    }
}
