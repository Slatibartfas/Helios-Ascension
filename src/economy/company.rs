//! Private shipping companies — autonomous AI freighter operators.
//!
//! Companies respond to open `ResourceRequest` entries in `PendingResourceRequests`.
//! When a company has available freighters it assigns one to the highest-priority
//! request it can service, calculates a transit time from the request's distance
//! from Earth (the default hub), marks the request as `InTransit`, and releases
//! the freighter when the delivery completes (handled by `complete_deliveries` in
//! `logistics.rs`).
//!
//! # Transit time model (pre-ship-construction placeholder)
//!
//! Until actual ship entities are implemented, companies operate via **abstract
//! phantom freighters**.  Transit time is derived from the destination body's
//! AU distance from Sol:
//!
//! ```text
//! transit_days = distance_au * TRANSIT_DAYS_PER_AU
//! ```
//!
//! with `TRANSIT_DAYS_PER_AU = 90` (chemical propulsion baseline).
//! Mars (~0.52 AU average) → ~47 days; Jupiter (~4.2 AU) → ~380 days.
//!
//! Companies earn credits per delivery and can purchase additional freighters
//! when their treasury exceeds a threshold.

use bevy::prelude::*;

use crate::astronomy::components::SpaceCoordinates;
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::economy::components::LocalStockpile;
use crate::economy::logistics::{PendingResourceRequests, RequestPriority, RequestState};
use crate::economy::GlobalBudget;
use crate::ui::SimulationTime;

pub use crate::economy::auto_build::CompanyBuildPolicy;
pub use crate::economy::auto_freight::CompanyAIPolicy;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Approximate transit days per AU of distance for chemical-propulsion freighters.
/// Earth–Mars average Hohmann ≈ 47 days at 0.52 AU separation → 90 d/AU.
const TRANSIT_DAYS_PER_AU: f64 = 90.0;

/// Minimum transit days regardless of distance (loading/unloading overhead).
const MIN_TRANSIT_DAYS: f64 = 10.0;

/// Payment per megatonne delivered (Mega-Credits).
const BASE_RATE_MC_PER_MT: f64 = 0.1;

/// Priority multipliers applied to the base payment.
fn priority_multiplier(priority: RequestPriority) -> f64 {
    match priority {
        RequestPriority::Trade => 0.5,
        RequestPriority::Maintenance => 1.0,
        RequestPriority::Construction => 2.0,
        RequestPriority::Emergency => 4.0,
    }
}

/// Treasury threshold at which a company buys a new freighter.
const BUY_SHIP_THRESHOLD_MC: f64 = 100_000.0;

/// Cost of one new company freighter (Mega-Credits).
const FREIGHTER_COST_MC: f64 = 80_000.0;

/// Length of the rolling treasury-delta window in seconds (60 in-game days).
/// Powers the per-company "Δ Treasury" column in the Private Shipping overview
/// panel (GRA-37.e).  When the simulation clock advances past
/// `treasury_window_start_seconds + WINDOW_S`, the window is rolled: the
/// starting treasury is re-anchored and the delta resets to zero.
const TREASURY_WINDOW_S: f64 = 60.0 * 86_400.0;

/// Default cap on simultaneous auto-builds per company (GRA-39 AC #3).
/// Players can override per company via a future UI control; the field is
/// public on `ShippingCompany`.
pub const DEFAULT_MAX_ACTIVE_BUILDS: u32 = 2;

// ── Data structures ───────────────────────────────────────────────────────────

/// A private autonomous freight operator.
#[derive(Debug, Clone)]
pub struct ShippingCompany {
    /// Display name shown in the Logistics panel.
    pub name: String,
    /// Company treasury in Mega-Credits (MC).
    pub treasury_mc: f64,
    /// Total number of freighters owned.
    pub freighter_count: u32,
    /// Freighters not currently on a delivery run.
    pub available_freighters: u32,
    /// Cumulative deliveries completed (for reputation scoring).
    pub total_deliveries: u32,
    /// Reliability score 0.0–1.0 (future: affects player preference).
    pub reputation: f32,
    /// AI policy governing automated freight behaviour (GRA-38).
    /// `AutoFreight` is the default for new companies (DW2-style opt-out);
    /// `Manual` companies never auto-assign freighters.
    pub policy: CompanyAIPolicy,
    /// Treasury value (MC) at the start of the current rolling window.
    /// The overview panel reports `treasury_mc - treasury_window_start_mc`
    /// as the per-window net change.  See `TREASURY_WINDOW_S`.
    pub treasury_window_start_mc: f64,
    /// Simulation time (seconds) when the current rolling window started.
    /// Zero until the first roll, which is harmless (every delivery
    /// pre-roll is inside the same window as the spawn point).
    pub treasury_window_start_seconds: f64,
    /// Auto-build policy governing whether the company queues freighter
    /// construction at its home body when freight demand goes unmet (GRA-39).
    /// `Manual` is the default — player has to opt in per company.
    /// Companies without a `home_body` (e.g. seeded placeholder companies)
    /// can never auto-build regardless of this policy.
    pub build_policy: CompanyBuildPolicy,
    /// Colony entity that the company considers its home — used both as the
    /// build site for auto-construction and as the destination-body filter
    /// for the demand heuristic.  `None` means "no fixed home" (placeholder
    /// companies and the default seeded companies).
    pub home_body: Option<Entity>,
    /// Maximum number of `ShipConstructionProject`s this company may have in
    /// flight at once.  Prevents a runaway queue when the demand heuristic
    /// keeps firing.
    pub max_active_builds: u32,
    /// Cached count of active (state == `Building`) freighter builds owned
    /// by this company, recomputed each tick by `auto_build_loop` and read
    /// by the company panel UI.  Players can't set this; the AI system owns
    /// it.
    pub active_builds: u32,
}

impl ShippingCompany {
    /// Create a new company with the given name and starting freighter count.
    ///
    /// The default `policy` is `CompanyAIPolicy::AutoFreight` (DW2-style
    /// opt-out — see GRA-38 / GRA-37).  Use [`ShippingCompany::with_policy`]
    /// to spawn a `Manual` company.
    pub fn new(name: impl Into<String>, freighters: u32, treasury_mc: f64) -> Self {
        Self {
            name: name.into(),
            treasury_mc,
            freighter_count: freighters,
            available_freighters: freighters,
            total_deliveries: 0,
            reputation: 0.5,
            policy: CompanyAIPolicy::default(),
            treasury_window_start_mc: treasury_mc,
            treasury_window_start_seconds: 0.0,
            build_policy: CompanyBuildPolicy::default(),
            home_body: None,
            max_active_builds: DEFAULT_MAX_ACTIVE_BUILDS,
            active_builds: 0,
        }
    }

    /// Set the AI policy for this company.  Builder-style helper.
    pub fn with_policy(mut self, policy: CompanyAIPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Set the build policy for this company.  Builder-style helper.
    pub fn with_build_policy(mut self, policy: CompanyBuildPolicy) -> Self {
        self.build_policy = policy;
        self
    }

    /// Set the company's home body.  Builder-style helper.
    pub fn with_home_body(mut self, body: Entity) -> Self {
        self.home_body = Some(body);
        self
    }

    /// True if the company can take on another delivery run.
    pub fn has_freighter_available(&self) -> bool {
        self.available_freighters > 0
    }

    /// Assign one freighter to a delivery.
    pub fn assign_freighter(&mut self) {
        if self.available_freighters > 0 {
            self.available_freighters -= 1;
        }
    }

    /// Roll the rolling treasury-delta window if `now` is past its end.
    /// Called from the two mutation paths (`complete_delivery`,
    /// `try_buy_freighter`) so the rolling anchor stays in lockstep with
    /// the actual treasury changes.
    fn maybe_roll_treasury_window(&mut self, now: f64) {
        if now - self.treasury_window_start_seconds > TREASURY_WINDOW_S {
            self.treasury_window_start_mc = self.treasury_mc;
            self.treasury_window_start_seconds = now;
        }
    }

    /// Return a freighter after delivery completes and credit the payment.
    pub fn complete_delivery(&mut self, payment_mc: f64, now: f64) {
        self.maybe_roll_treasury_window(now);
        self.available_freighters = self.available_freighters.saturating_add(1);
        self.freighter_count = self.freighter_count.max(self.available_freighters);
        self.treasury_mc += payment_mc;
        self.total_deliveries += 1;
        self.reputation = (self.reputation + 0.01).min(1.0);
    }

    /// Purchase an additional freighter if treasury allows.
    pub fn try_buy_freighter(&mut self, now: f64) -> bool {
        self.maybe_roll_treasury_window(now);
        if self.treasury_mc >= BUY_SHIP_THRESHOLD_MC {
            self.treasury_mc -= FREIGHTER_COST_MC;
            self.freighter_count += 1;
            self.available_freighters += 1;
            info!(
                "{} purchased a new freighter (total: {})",
                self.name, self.freighter_count
            );
            true
        } else {
            false
        }
    }
}

/// Global resource holding all private shipping companies.
#[derive(Resource)]
pub struct ShippingCompanies {
    pub companies: Vec<ShippingCompany>,
}

impl Default for ShippingCompanies {
    fn default() -> Self {
        Self {
            companies: vec![
                ShippingCompany::new("Helios Freight Co.", 3, 50_000.0),
                ShippingCompany::new("Solar Carriers Ltd.", 1, 20_000.0),
            ],
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Estimate transit time in seconds based on body position (AU from origin).
///
/// Uses the body's `SpaceCoordinates` when available.  Falls back to a
/// 1.0 AU default (Earth-level) if the component is absent.
fn estimate_transit_seconds(dest_entity: Entity, coords_query: &Query<&SpaceCoordinates>) -> f64 {
    // SpaceCoordinates are in AU (the rendering scale).
    let distance_au = coords_query
        .get(dest_entity)
        .map(|sc| sc.position.length())
        .unwrap_or(1.0)
        .max(0.1); // minimum 0.1 AU to avoid zero transit

    let days = (distance_au * TRANSIT_DAYS_PER_AU).max(MIN_TRANSIT_DAYS);
    let seconds_per_day = SECONDS_PER_YEAR / 365.25;
    days * seconds_per_day
}

/// Compute payment for a delivery.
fn compute_payment(amount_mt: f64, distance_au: f64, priority: RequestPriority) -> f64 {
    BASE_RATE_MC_PER_MT * amount_mt * distance_au.max(0.1) * priority_multiplier(priority)
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// AI tick: assign available company freighters to the highest-priority open requests.
///
/// Iterates companies in order; each company grabs the best available request
/// (highest priority, then oldest creation time) and assigns its freighter.
/// The request is moved to `InTransit` with a calculated `eta_seconds`.
///
/// Resource sourcing: for now the source is the system-wide `LocalStockpile`
/// pool (any body with sufficient resources).  When the per-body source
/// constraint is fully enforced the sourcing will be narrowed to a single body.
pub fn process_company_ai(
    mut companies: ResMut<ShippingCompanies>,
    mut requests: ResMut<PendingResourceRequests>,
    mut source_stockpiles: Query<(Entity, &mut LocalStockpile)>,
    coords_query: Query<&SpaceCoordinates>,
    sim_time: Res<SimulationTime>,
) {
    let now = sim_time.elapsed_seconds();

    // Collect indices of Pending requests sorted by priority (desc) then age (asc).
    let mut pending_indices: Vec<usize> = requests
        .requests
        .iter()
        .enumerate()
        .filter(|(_, r)| r.state == RequestState::Pending)
        .map(|(i, _)| i)
        .collect();

    pending_indices.sort_by(|&a, &b| {
        let ra = &requests.requests[a];
        let rb = &requests.requests[b];
        rb.priority.cmp(&ra.priority).then(
            ra.created_at_seconds
                .partial_cmp(&rb.created_at_seconds)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    if pending_indices.is_empty() {
        return;
    }

    for (company_idx, company) in companies.companies.iter_mut().enumerate() {
        if company.policy != CompanyAIPolicy::AutoFreight {
            // GRA-38: Manual companies do not auto-assign their abstract
            // freighters.  The player must take delivery via the fleet
            // panel's manual-assign path.  The companion auto-freight
            // system (in `auto_freight.rs`) respects the same policy
            // when recruiting idle player fleets.
            continue;
        }
        if !company.has_freighter_available() {
            continue;
        }

        // Find the best request this company can service.
        let mut assigned_req_idx: Option<usize> = None;

        'req_loop: for &req_vec_idx in &pending_indices {
            let req = &requests.requests[req_vec_idx];

            // Try to find a source body with sufficient resources.
            let mut found_source = false;
            for (_src_entity, ls) in source_stockpiles.iter() {
                if ls.get(&req.resource) >= req.amount_mt {
                    found_source = true;
                    break;
                }
            }

            if !found_source {
                // Check if multiple bodies combined have enough (system-pool logic).
                let total_available: f64 = source_stockpiles
                    .iter()
                    .map(|(_, ls)| ls.get(&req.resource))
                    .sum();
                if total_available < req.amount_mt {
                    continue 'req_loop; // Can't service — not enough anywhere
                }
            }

            assigned_req_idx = Some(req_vec_idx);
            break 'req_loop;
        }

        let req_vec_idx = match assigned_req_idx {
            Some(i) => i,
            None => continue,
        };

        let req = &mut requests.requests[req_vec_idx];

        // Remove this request index from future consideration in this tick.
        pending_indices.retain(|&i| i != req_vec_idx);

        // Source the resources: deduct from stockpiles (first-fit, largest first).
        let mut remaining = req.amount_mt;

        // Sort bodies by stockpile descending to minimize number of sources.
        let mut sources: Vec<(Entity, f64)> = source_stockpiles
            .iter()
            .map(|(e, ls)| (e, ls.get(&req.resource)))
            .filter(|(_, amt)| *amt > 0.0)
            .collect();
        sources.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (src_entity, _) in &sources {
            if remaining <= 0.0 {
                break;
            }
            if let Ok((_, mut ls)) = source_stockpiles.get_mut(*src_entity) {
                let taken = ls.consume(req.resource, remaining);
                remaining -= taken;
            }
        }

        let actual_dispatched = req.amount_mt - remaining;
        req.in_transit_mt = actual_dispatched;

        // Calculate transit time.
        let transit_s = estimate_transit_seconds(req.destination_body, &coords_query);
        req.eta_seconds = Some(now + transit_s);
        req.state = RequestState::InTransit;
        req.assigned_company_idx = Some(company_idx);

        company.assign_freighter();

        let eta_days = transit_s / (SECONDS_PER_YEAR / 365.25);
        info!(
            "{}: dispatched freighter — {:?} {:.1} Mt → {} (ETA {:.0} days)",
            company.name, req.resource, actual_dispatched, req.destination_name, eta_days,
        );
    }
}

/// Return freighters that completed their delivery and let companies buy ships.
///
/// Must run *after* `complete_deliveries` in `logistics.rs` so that
/// `Delivered` state is already set.
pub fn update_company_fleets(
    mut companies: ResMut<ShippingCompanies>,
    mut requests: ResMut<PendingResourceRequests>,
    coords_query: Query<&SpaceCoordinates>,
    mut budget: ResMut<GlobalBudget>,
    sim_time: Res<SimulationTime>,
) {
    let now = sim_time.elapsed_seconds();

    // For each newly-delivered request: return the freighter and pay the company.
    for req in requests.requests.iter_mut() {
        if req.state != RequestState::Delivered {
            continue;
        }
        if req.payment_made {
            continue; // Already processed.
        }
        let company_idx = match req.assigned_company_idx {
            Some(i) => i,
            None => continue,
        };
        let company = match companies.companies.get_mut(company_idx) {
            Some(c) => c,
            None => continue,
        };

        let distance_au = coords_query
            .get(req.destination_body)
            .map(|sc| sc.position.length())
            .unwrap_or(1.0);
        let payment = compute_payment(req.in_transit_mt, distance_au, req.priority);

        // Deduct payment from player treasury (clamped to available funds so
        // treasury never goes negative) and credit the company with what was paid.
        let actual_payment = payment.min(budget.treasury.max(0.0));
        budget.treasury -= actual_payment;
        company.complete_delivery(actual_payment, now);
        req.payment_made = true;
    }

    // Let well-funded companies expand their fleets.
    for company in companies.companies.iter_mut() {
        company.try_buy_freighter(now);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shipping_company_creation() {
        let c = ShippingCompany::new("Test Co.", 2, 10_000.0);
        assert_eq!(c.freighter_count, 2);
        assert_eq!(c.available_freighters, 2);
        assert_eq!(c.total_deliveries, 0);
        assert!(c.has_freighter_available());
    }

    #[test]
    fn test_shipping_company_assign_and_complete() {
        let mut c = ShippingCompany::new("Test Co.", 1, 0.0);
        c.assign_freighter();
        assert!(!c.has_freighter_available());
        c.complete_delivery(1_000.0, 0.0);
        assert!(c.has_freighter_available());
        assert_eq!(c.total_deliveries, 1);
        assert_eq!(c.treasury_mc, 1_000.0);
    }

    #[test]
    fn test_buy_freighter_sufficient_funds() {
        let mut c = ShippingCompany::new("Rich Co.", 1, BUY_SHIP_THRESHOLD_MC + 1.0);
        let bought = c.try_buy_freighter(0.0);
        assert!(bought);
        assert_eq!(c.freighter_count, 2);
        assert!((c.treasury_mc - (BUY_SHIP_THRESHOLD_MC + 1.0 - FREIGHTER_COST_MC)).abs() < 0.01);
    }

    #[test]
    fn test_buy_freighter_insufficient_funds() {
        let mut c = ShippingCompany::new("Poor Co.", 1, 1_000.0);
        let bought = c.try_buy_freighter(0.0);
        assert!(!bought);
        assert_eq!(c.freighter_count, 1);
    }

    #[test]
    fn test_treasury_window_rolls_after_window_s() {
        let mut c = ShippingCompany::new("Rolling Co.", 1, 5_000.0);
        assert_eq!(c.treasury_window_start_mc, 5_000.0);
        // First delivery at t=0: no roll (now - 0 = 0, not > WINDOW_S).
        c.complete_delivery(1_000.0, 0.0);
        assert_eq!(c.treasury_mc, 6_000.0);
        assert_eq!(c.treasury_window_start_mc, 5_000.0);
        // Another delivery inside the window: anchor unchanged.
        c.complete_delivery(2_000.0, TREASURY_WINDOW_S - 1.0);
        assert_eq!(c.treasury_window_start_mc, 5_000.0);
        assert_eq!(c.treasury_mc, 8_000.0);
        // Delivery after the window end: anchor rolls to the pre-payment
        // treasury (8_000), then the +500 is added on top.  Subsequent
        // callers (e.g. the panel) will see the rolled anchor vs. the
        // current treasury and report the in-window delta.
        c.complete_delivery(500.0, TREASURY_WINDOW_S + 1.0);
        assert_eq!(c.treasury_window_start_mc, 8_000.0);
        assert_eq!(c.treasury_mc, 8_500.0);
        assert_eq!(c.treasury_window_start_seconds, TREASURY_WINDOW_S + 1.0);
    }

    #[test]
    fn test_priority_multiplier_ordering() {
        assert!(
            priority_multiplier(RequestPriority::Emergency)
                > priority_multiplier(RequestPriority::Construction)
        );
        assert!(
            priority_multiplier(RequestPriority::Construction)
                > priority_multiplier(RequestPriority::Maintenance)
        );
        assert!(
            priority_multiplier(RequestPriority::Maintenance)
                > priority_multiplier(RequestPriority::Trade)
        );
    }

    #[test]
    fn test_compute_payment() {
        // 100 Mt, 1.0 AU, Construction priority (2.0×)
        let p = compute_payment(100.0, 1.0, RequestPriority::Construction);
        assert!((p - BASE_RATE_MC_PER_MT * 100.0 * 1.0 * 2.0).abs() < 0.001);
    }
}
