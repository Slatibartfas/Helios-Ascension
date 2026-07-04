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
    hohmann_round_trip_seconds, PendingResourceRequests, RequestPriority, RequestState,
    ResourceRequest,
};
use crate::economy::types::ResourceType;
use crate::fleets::{Fleet, FleetOrbit};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
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
#[derive(Resource, Default, Debug, Reflect)]
#[reflect(Resource)]
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
    // Bevy 0.18 forbids having two `Query` system params that both touch
    // the same component (B0001).  We need a read pass (compute the source
    // list + total) and a write pass (consume from each chosen body), so
    // fold them into a single `Query<(Entity, &mut LocalStockpile)>` and
    // use sequential `iter()` / `get_mut()` calls within the system.
    mut stockpiles: Query<(Entity, &mut LocalStockpile)>,
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

        // Pick the best idle freighter at the request's destination body
        // (GRA-119).  "Best" is the fleet with the highest total cargo
        // capacity — a fleet of 3 light_freighters (3 × 70 t = 210 t)
        // beats a fleet of 4 light_freighters only if those freighters
        // are at a lower module tier.  The per-ship `cargo_capacity_t`
        // field is populated by `sync_fleet_cache_from_ship_entities`
        // from each ship's `ShipTemplateRef` + `FreighterSlots`.
        let mut best: Option<(Entity, f64)> = None; // (fleet_entity, total_cargo_capacity_t)
        for (fleet_entity, fleet, orbit) in idle_freight_fleets.iter() {
            if orbit.body != dest {
                continue;
            }
            let capacity = fleet.total_cargo_capacity_t();
            if capacity <= 0.0 {
                continue;
            }
            if best.is_none_or(|(_, c)| capacity > c) {
                best = Some((fleet_entity, capacity));
            }
        }

        let Some((fleet_entity, fleet_capacity_t)) = best else {
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

        // GRA-119: cap the dispatched amount at the fleet's cargo capacity.
        // `actual_dispatched` is the amount the fleet can carry in this
        // single trip; any shortfall becomes a new `Maintenance`-priority
        // request below.
        let req_snapshot = requests.requests[req_idx].clone();
        let target = req_snapshot.amount_mt.min(fleet_capacity_t);

        // Deduct `target` from source LocalStockpile (first-fit-largest),
        // mirroring the manual-assign path.  The helper returns the
        // amount actually consumed (which may be < target if the source
        // pool runs short between the snapshot and the consume pass).
        // `requests.requests[req_idx]` borrow dropped before the
        // mutable call.
        let actual_dispatched = deduct_from_source(&req_snapshot.resource, target, &mut stockpiles);
        if actual_dispatched <= 0.0 {
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
        // and stamp the freighter + ETA.  When the cap forces a split,
        // reduce the original request's `amount_mt` to the dispatched
        // amount and enqueue a new `Maintenance` request for the
        // shortfall (GRA-119).  Scope the `&mut requests.requests[]` borrow
        // so it drops before we call `requests.add(...)` below — NLL does
        // not see the inner `req.source_body = ...` mutation as a barrier
        // for the outer borrow.
        let shortfall_mt = req_snapshot.amount_mt - actual_dispatched;
        let (original_request_id, original_resource, original_dest_name) = {
            let req = &mut requests.requests[req_idx];
            req.in_transit_mt = actual_dispatched;
            req.amount_mt = actual_dispatched;
            req.eta_seconds = Some(now + transit_s);
            req.state = RequestState::InTransit;
            req.assignee_fleet_id = Some(fleet_entity);
            if req.source_body.is_none() {
                req.source_body = req_snapshot.source_body;
            }
            (req.id, req.resource, req.destination_name.clone())
        };

        if shortfall_mt > 0.0 {
            // GRA-119: enqueue a new Pending request for the shortfall
            // at the same destination, Maintenance priority, so the
            // remaining mass is serviced by the next available freighter
            // trip (either by the same fleet on its next orbit, another
            // idle fleet, or the abstract company AI).
            requests.add(ResourceRequest {
                id: 0, // overwritten by `add`
                destination_body: req_snapshot.destination_body,
                destination_name: req_snapshot.destination_name.clone(),
                resource: req_snapshot.resource,
                amount_mt: shortfall_mt,
                priority: RequestPriority::Maintenance,
                state: RequestState::Pending,
                in_transit_mt: 0.0,
                eta_seconds: None,
                assigned_company_idx: None,
                created_at_seconds: now,
                source_body: req_snapshot.source_body,
                linked_project: req_snapshot.linked_project,
                payment_made: false,
                completed_at_seconds: None,
                assignee_fleet_id: None,
            });
            info!(
                "AutoFreight: request {} capped at {:.1}/{:.1} Mt — enqueued \
                 {:.1} Mt Maintenance remainder at {}",
                original_request_id,
                actual_dispatched,
                req_snapshot.amount_mt,
                shortfall_mt,
                original_dest_name,
            );
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
            original_request_id,
            original_resource,
            actual_dispatched,
            original_dest_name,
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

/// First-fit-largest deduction across all `LocalStockpile`s.  Returns the
/// amount actually consumed — capped at `amount`, and zero if the source
/// pool is empty.  Mirrors the logic in
/// `logistics::process_fleet_logistics_assignments` and
/// `company::process_company_ai`.  GRA-119 changes the signature from
/// `bool` to `f64` so the auto-freight loop can size the in-transit
/// amount to whatever was actually deducted (rather than assuming a
/// full-or-nothing outcome).
///
/// We use a single `Query<(Entity, &mut LocalStockpile)>` rather than a
/// read+mut pair because Bevy 0.18 rejects that combination as B0001.
/// The `iter()` borrow is dropped at the end of the first `collect()`,
/// which releases the conflict before we call `get_mut()` below.
fn deduct_from_source(
    resource: &ResourceType,
    amount: f64,
    stockpiles: &mut Query<(Entity, &mut LocalStockpile)>,
) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }
    let mut sources: Vec<(Entity, f64)> = stockpiles
        .iter()
        .map(|(e, ls)| (e, ls.get(resource)))
        .filter(|(_, amt)| *amt > 0.0)
        .collect();
    sources.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let total: f64 = sources.iter().map(|(_, a)| a).sum();
    if total <= 0.0 {
        return 0.0;
    }

    let mut remaining = amount;
    for (src_entity, _) in &sources {
        if remaining <= 0.0 {
            break;
        }
        if let Ok((_, mut ls)) = stockpiles.get_mut(*src_entity) {
            let taken = ls.consume(*resource, remaining);
            remaining -= taken;
        }
    }
    amount - remaining
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astronomy::components::SpaceCoordinates;
    use crate::colony::components::Colony;
    use crate::fleets::components::ShipInfo;
    use crate::fleets::types::{PropulsionType, ShipClass};

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
    /// `cargo_capacity_t` is 0.0 by default — most tests don't care, and
    /// `sync_fleet_cache_from_ship_entities` is bypassed (no
    /// `ShipInstance` entity here).  GRA-119 tests pass an explicit
    /// capacity to exercise the cap-and-split logic without spinning up
    /// the full template registry.
    fn spawn_idle_freighter_fleet_with_capacity(
        world: &mut World,
        body: Entity,
        cargo_capacity_t: f64,
    ) -> Entity {
        let mut ship = ShipInfo::new(
            "Test Freighter".into(),
            ShipClass::Freighter,
            PropulsionType::Chemical,
        );
        ship.cargo_capacity_t = cargo_capacity_t;
        let mut fleet = Fleet::new("Test Fleet".into());
        fleet.ships.push(ship);
        world.spawn((fleet, FleetOrbit::new(body, 0.001))).id()
    }

    fn spawn_idle_freighter_fleet(world: &mut World, body: Entity) -> Entity {
        spawn_idle_freighter_fleet_with_capacity(world, body, 0.0)
    }

    /// Init the resources the auto-freight system reads / mutates.
    ///
    /// The plugin normally handles this in `AutoFreightPlugin::build`, but
    /// the unit tests run the system directly against a bare-bones `App`,
    /// so we have to mirror the initialization.
    fn init_econ_resources(world: &mut World) {
        world.init_resource::<PendingResourceRequests>();
        world.init_resource::<ShippingCompanies>();
        world.init_resource::<SimulationTime>();
        world.init_resource::<AutoFreightNotificationState>();
        world.init_resource::<Messages<FreighterNoDesignAvailable>>();
    }

    /// Build a `Pending` `ResourceRequest` at the given body for `Iron`.
    fn push_pending_iron_request(requests: &mut PendingResourceRequests, dest: Entity) -> u64 {
        push_pending_iron_request_amount(requests, dest, 50.0)
    }

    /// GRA-119 variant: a `Pending` `ResourceRequest` of an arbitrary
    /// amount (used by the cap-and-split test, which needs 5,000 Mt).
    fn push_pending_iron_request_amount(
        requests: &mut PendingResourceRequests,
        dest: Entity,
        amount_mt: f64,
    ) -> u64 {
        requests.add(ResourceRequest {
            id: 0,
            destination_body: dest,
            destination_name: "Test Colony".into(),
            resource: ResourceType::Iron,
            amount_mt,
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
        // GRA-119: explicit 100 t cargo capacity so the picker (which
        // now selects by `total_cargo_capacity_t`) can claim the 50 Mt
        // request.  Default-0-capacity freighters are filtered out of
        // the picker.
        let fleet_entity = spawn_idle_freighter_fleet_with_capacity(app.world_mut(), body, 100.0);

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

    /// GRA-119: a 5,000 Mt Iron request at a body whose only idle
    /// freighter fleet has 70 t cargo capacity must dispatch a single
    /// 70 t trip and enqueue a new `Maintenance`-priority `Pending`
    /// request for the remaining 4,930 Mt.  Mirrors the LGD's "in-game
    /// check" test plan in the GRA-118 design comment.
    #[test]
    fn test_caps_per_freighter_capacity_and_splits_remainder() {
        let mut app = App::new();
        let mut schedule = Schedule::default();
        init_econ_resources(app.world_mut());

        // 5,000 Mt Iron on hand (more than the cap can move in one trip).
        let body = spawn_body_with_stockpile(app.world_mut(), 5_000.0);
        // Single light_freighter = 70 t cargo (2× cargo_pod_medium).
        let fleet_entity = spawn_idle_freighter_fleet_with_capacity(app.world_mut(), body, 70.0);

        // AutoFreight company.
        let mut company = ShippingCompany::new("Test Co.", 0, 0.0);
        company.policy = CompanyAIPolicy::AutoFreight;
        app.world_mut()
            .resource_mut::<ShippingCompanies>()
            .companies = vec![company];

        // One Pending 5,000 Mt Construction Iron request at the same body.
        let request_id = {
            let mut requests = app.world_mut().resource_mut::<PendingResourceRequests>();
            push_pending_iron_request_amount(&mut requests, body, 5_000.0)
        };

        schedule.add_systems(auto_freight_loop);
        schedule.run(app.world_mut());

        // Original request: reduced to 70 t, InTransit, freighter as assignee.
        let reqs = app.world().resource::<PendingResourceRequests>();
        let original = reqs
            .find_by_id(request_id)
            .expect("original request must still exist after split");
        assert_eq!(
            original.state,
            RequestState::InTransit,
            "original request must be InTransit after the cap"
        );
        assert_eq!(
            original.assignee_fleet_id,
            Some(fleet_entity),
            "freighter fleet should be the assignee"
        );
        assert!(
            (original.in_transit_mt - 70.0).abs() < 1e-6,
            "in_transit_mt must be capped at fleet capacity: got {:.3}",
            original.in_transit_mt
        );
        assert!(
            (original.amount_mt - 70.0).abs() < 1e-6,
            "original amount_mt must be reduced to the dispatched amount: \
             got {:.3}",
            original.amount_mt
        );
        assert!(
            original.eta_seconds.is_some(),
            "eta_seconds must be stamped after assignment"
        );

        // Remainder request: 4,930 Mt Pending Maintenance at the same destination.
        // The remainder's id is assigned by `PendingResourceRequests::add`;
        // filter by shape.
        let remainders: Vec<&ResourceRequest> = reqs
            .requests
            .iter()
            .filter(|r| r.id != request_id && r.state == RequestState::Pending)
            .collect();
        assert_eq!(
            remainders.len(),
            1,
            "expected exactly one remainder request, got {}",
            remainders.len()
        );
        let remainder = remainders[0];
        assert!(
            (remainder.amount_mt - 4_930.0).abs() < 1e-6,
            "remainder amount must be the shortfall: got {:.3}",
            remainder.amount_mt
        );
        assert_eq!(
            remainder.priority,
            RequestPriority::Maintenance,
            "remainder must be Maintenance priority (not the original's \
             Construction) so the next dispatch cycle is steady-state, not \
             building-priority"
        );
        assert_eq!(
            remainder.destination_body, body,
            "remainder must target the same destination"
        );
        assert_eq!(
            remainder.resource,
            ResourceType::Iron,
            "remainder must carry the same resource"
        );
        assert!(
            remainder.assignee_fleet_id.is_none(),
            "remainder must be unassigned so the next dispatch cycle \
             (manual, auto, or company AI) can claim it"
        );
        assert!(
            remainder.linked_project == original.linked_project,
            "remainder must inherit the linked_project so building queues \
             stay in sync with their demand"
        );

        // Source stockpile: only 70 t should be deducted (4,930 t
        // remains on hand for the next trip).
        let ls = app
            .world()
            .entity(body)
            .get::<LocalStockpile>()
            .expect("body still has LocalStockpile");
        assert!(
            (ls.get(&ResourceType::Iron) - 4_930.0).abs() < 1e-6,
            "source stockpile must be 5,000 - 70 = 4,930 Mt after the cap"
        );
    }
}
