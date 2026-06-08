//! Auto-build AI for private `ShippingCompany` operators (GRA-39).
//!
//! When a `ShippingCompany` has `CompanyBuildPolicy::AutoBuild` and a
//! `home_body`, the loop scans `PendingResourceRequests` for unfulfilled
//! demand at the company's home body each tick.  When the demand heuristic
//! fires (≥1 open `ResourceRequest` at the home body older than
//! `DEMAND_AGE_THRESHOLD_S`), the loop picks the cheapest *applicable*
//! freighter template — i.e. the one the company can currently build given
//! its researched tech set — and queues a `ShipConstructionProject` at the
//! home body via the existing colony construction pipeline.
//!
//! The build is gated by:
//! * a per-company active-build cap (`max_active_builds`, default 2) so a
//!   company can't drown its shipyard in queued work;
//! * a treasury check (`treasury_mc >= build_cost_mc`) so a poor company
//!   doesn't enqueue builds it can't pay for;
//! * a shipyard presence check (the home body must have at least one
//!   `BuildingType::Shipyard`) so the company doesn't queue work the colony
//!   can't process.
//!
//! When the registry has no buildable template (no research), the loop
//! emits a throttled `FreighterBuildNoDesignAvailable` event so the UI can
//! surface the situation to the player.  This is the GRA-39 equivalent of
//! the GRA-38 "no-design" event, scoped to the build side.

use bevy::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::colony::components::Colony;
use crate::colony::types::BuildingType;
use crate::economy::company::ShippingCompanies;
use crate::economy::logistics::{PendingResourceRequests, RequestState};
use crate::research::ResearchState;
use crate::shipbuilding::components::{
    ShipConstructionProject, ShipConstructionState, ShipModuleSelection,
};
use crate::shipbuilding::data::ShipbuildingData;
use crate::ships::templates::{
    BestBuildableEntry, CargoSlot, FreighterTemplate, FreighterTemplateRegistry, UpgradeStep,
};
use crate::ui::SimulationTime;

// ── CompanyBuildPolicy ────────────────────────────────────────────────────────

/// Auto-build policy governing whether a `ShippingCompany` queues
/// freighter construction at its home body (GRA-39 / GRA-37).
///
/// `Manual` is the default per the spec (AC #1).  Companies can opt in
/// per-company via the Logistics tab.  Companies without a `home_body`
/// (e.g. the seeded default companies) can never auto-build regardless of
/// this policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompanyBuildPolicy {
    /// No auto-build.  Player must use the existing colony construction UI.
    #[default]
    Manual,
    /// Auto-queue a freighter build when the demand heuristic fires.
    AutoBuild,
}

impl std::fmt::Display for CompanyBuildPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompanyBuildPolicy::Manual => write!(f, "Manual"),
            CompanyBuildPolicy::AutoBuild => write!(f, "Auto-Build"),
        }
    }
}

// ── No-design notification message ───────────────────────────────────────────

/// Emitted (throttled) when the auto-build loop has unmet demand but no
/// buildable freighter template exists.  Companion to GRA-38's
/// `FreighterNoDesignAvailable`; the UI subscribes to both.
#[derive(Message, Debug, Clone)]
pub struct FreighterBuildNoDesignAvailable {
    pub company_idx: usize,
    pub home_body: Entity,
}

// ── Throttle state ───────────────────────────────────────────────────────────

/// Per-company throttling so we don't spam the event log + notification UI
/// every tick for the same unfulfilled build.
#[derive(Resource, Default, Debug)]
pub struct AutoBuildNotificationState {
    /// `(company_idx, last_complained_sim_seconds)` map.
    last_complained: HashMap<usize, f64>,
}

const NO_DESIGN_THROTTLE_S: f64 = 86_400.0;

// ── Tunables ─────────────────────────────────────────────────────────────────

/// Demand age threshold (sim-seconds) for the GRA-39 demand heuristic.
/// An open `ResourceRequest` older than this at the company's home body
/// counts as "unmet demand".  90 sim-days; matches the GRA-31 PR-A
/// backfill-window flavour.
const DEMAND_AGE_THRESHOLD_S: f64 = 7_776_000.0;

/// Conversion rate from build points to Mega-Credits.  Used for the
/// treasury gate + the treasury deduction at queue time.  At 1_000 MC/BP
/// a light_freighter (~120 BP) costs ~120_000 MC, in line with
/// `try_buy_freighter`'s 80–100k range.
const MC_PER_BP: f64 = 1_000.0;

// ── Plugin ───────────────────────────────────────────────────────────────────

pub struct AutoBuildPlugin;

impl Plugin for AutoBuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AutoBuildNotificationState>()
            .add_message::<FreighterBuildNoDesignAvailable>()
            .add_systems(
                Update,
                auto_build_loop
                    .after(crate::economy::auto_freight::auto_freight_loop)
                    .after(crate::economy::company::update_company_fleets),
            );
    }
}

// ── System ───────────────────────────────────────────────────────────────────

/// Main auto-build system.  Runs each tick.
#[allow(clippy::too_many_arguments)]
pub fn auto_build_loop(
    mut commands: Commands,
    mut companies: ResMut<ShippingCompanies>,
    requests: Res<PendingResourceRequests>,
    registry: Option<Res<FreighterTemplateRegistry>>,
    shipbuilding_data: Option<Res<ShipbuildingData>>,
    research_state: Res<ResearchState>,
    colonies: Query<&Colony>,
    projects: Query<&ShipConstructionProject>,
    sim_time: Res<SimulationTime>,
    mut notif_state: ResMut<AutoBuildNotificationState>,
    mut no_design_events: MessageWriter<FreighterBuildNoDesignAvailable>,
) {
    let now = sim_time.elapsed_seconds();

    // Refresh the cached `active_builds` counter for every company.  This
    // both feeds the cap check below and keeps the UI's "Queued Builds"
    // column accurate.
    let mut active_per_company: HashMap<usize, u32> = HashMap::new();
    for project in projects.iter() {
        if !project.is_building() {
            continue;
        }
        if let Some(idx) = project.building_company_idx {
            *active_per_company.entry(idx).or_insert(0) += 1;
        }
    }
    for (idx, company) in companies.companies.iter_mut().enumerate() {
        company.active_builds = active_per_company.get(&idx).copied().unwrap_or(0);
    }

    // Bail if the freighter template system isn't loaded yet (GRA-40 ships
    // it at Startup; this is just a startup-ordering safety net).
    let Some(registry) = registry else {
        return;
    };
    let Some(shipbuilding_data) = shipbuilding_data else {
        return;
    };
    if registry.is_empty() {
        return;
    }

    // Companies eligible to consider auto-building this tick.
    let eligible_indices: Vec<usize> = companies
        .companies
        .iter()
        .enumerate()
        .filter(|(_, c)| c.build_policy == CompanyBuildPolicy::AutoBuild && c.home_body.is_some())
        .map(|(i, _)| i)
        .collect();
    if eligible_indices.is_empty() {
        return;
    }

    // Pre-compute per-home-body unmet-demand counts so the inner loop is
    // cheap.
    let mut unmet_at_body: HashMap<Entity, u32> = HashMap::new();
    for req in requests.requests.iter() {
        if req.state != RequestState::Pending {
            continue;
        }
        if (now - req.created_at_seconds) < DEMAND_AGE_THRESHOLD_S {
            continue;
        }
        *unmet_at_body.entry(req.destination_body).or_insert(0) += 1;
    }

    for company_idx in eligible_indices {
        let (home_body, active, cap) = {
            let company = &companies.companies[company_idx];
            let home = company.home_body.expect("filtered above");
            (home, company.active_builds, company.max_active_builds)
        };

        // Cap gate.
        if active >= cap {
            continue;
        }

        // Demand gate.
        if unmet_at_body.get(&home_body).copied().unwrap_or(0) == 0 {
            continue;
        }

        // Shipyard gate.
        match colonies.get(home_body) {
            Ok(colony) => {
                if colony.building_count(BuildingType::Shipyard) == 0 {
                    info!(
                        "AutoBuild: company {} skipped — home body has no shipyard",
                        companies.companies[company_idx].name
                    );
                    continue;
                }
            }
            Err(_) => {
                // Home body entity doesn't have a Colony component.  Treat
                // as "no shipyard" and skip.
                continue;
            }
        }

        // Template pick: best buildable (best_buildable ties cheapest BP,
        // then template id lex).  We do NOT pick by cheapest cargo; the
        // design intent is to grow capacity, and the BP tie-break
        // naturally constrains cost.
        let best = registry.best_buildable(&shipbuilding_data, |t| {
            research_state.is_unlocked(t)
        });
        let Some(entry) = best else {
            maybe_emit_no_design(
                company_idx,
                home_body,
                &mut notif_state,
                now,
                &mut no_design_events,
            );
            continue;
        };

        // Treasury gate + deduction.
        let build_cost_mc = match build_cost_mc_for_entry(
            &registry,
            &shipbuilding_data,
            &entry,
        ) {
            Some(c) => c,
            None => continue,
        };
        {
            let company = &mut companies.companies[company_idx];
            if company.treasury_mc < build_cost_mc {
                continue;
            }
            company.treasury_mc -= build_cost_mc;
        }

        // Spawn the ShipConstructionProject directly.  We don't route
        // through `PendingShipbuildingActions` because the action queue
        // is the UI→simulation boundary; AI builds already know the
        // build_site and template, so the indirection buys nothing.
        spawn_company_ship_project(
            &mut commands,
            &registry,
            &shipbuilding_data,
            &entry,
            home_body,
            company_idx,
        );

        // Increment the cached counter for this company so the cap check
        // sees it.
        companies.companies[company_idx].active_builds += 1;

        info!(
            "AutoBuild: company {} queued freighter {:?} at {:?} (cost {:.0} MC, active {}/{})",
            companies.companies[company_idx].name,
            entry.template_id,
            home_body,
            build_cost_mc,
            companies.companies[company_idx].active_builds,
            cap,
        );
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn maybe_emit_no_design(
    company_idx: usize,
    home_body: Entity,
    state: &mut AutoBuildNotificationState,
    now: f64,
    events: &mut MessageWriter<FreighterBuildNoDesignAvailable>,
) {
    let last = state
        .last_complained
        .get(&company_idx)
        .copied()
        .unwrap_or(f64::NEG_INFINITY);
    if (now - last) < NO_DESIGN_THROTTLE_S {
        return;
    }
    state.last_complained.insert(company_idx, now);
    events.write(FreighterBuildNoDesignAvailable {
        company_idx,
        home_body,
    });
}

/// MC build cost for the given `(template_id, best_tier)`.  Uses
/// `template_uniform_cost` (locally re-implemented; the templates.rs
/// private one isn't visible across the same crate without duplication)
/// to mirror the tie-break in `best_buildable` so the treasury gate is
/// internally consistent with the AI's template ranking.
fn build_cost_mc_for_entry(
    registry: &FreighterTemplateRegistry,
    shipbuilding_data: &ShipbuildingData,
    entry: &BestBuildableEntry,
) -> Option<f64> {
    let template = registry.get(&entry.template_id)?;
    let bp = template_uniform_cost(shipbuilding_data, template, entry.best_tier);
    Some(bp * MC_PER_BP)
}

/// Sum of hull `base_build_points` + per-module `effective_module_build_points`
/// over the template's slots at the given tier.  Used as the cost signal
/// for the treasury gate.
fn template_uniform_cost(
    shipbuilding_data: &ShipbuildingData,
    template: &FreighterTemplate,
    tier: u32,
) -> f64 {
    let mut total = 0.0;
    for slot in &template.cargo_slots {
        let module_id = module_id_at_tier(slot, tier);
        if let Some(module) = shipbuilding_data.get_module(module_id) {
            total += shipbuilding_data.effective_module_build_points(module);
        }
    }
    if let Some(hull) = shipbuilding_data.get_hull(&template.base_hull) {
        total += hull.base_build_points;
    }
    total
}

/// Build a `ShipDesignDraft` for the freighter template and spawn a
/// `ShipConstructionProject` at the given home body.  Returns the new
/// project entity.
fn spawn_company_ship_project(
    commands: &mut Commands,
    registry: &FreighterTemplateRegistry,
    shipbuilding_data: &ShipbuildingData,
    entry: &BestBuildableEntry,
    build_site: Entity,
    company_idx: usize,
) -> Entity {
    let template = registry.get(&entry.template_id).expect("entry resolves");
    let hull_def = shipbuilding_data
        .get_hull(&template.base_hull)
        .expect("template hull exists");

    let selected_modules: Vec<ShipModuleSelection> = template
        .cargo_slots
        .iter()
        .map(|slot| {
            let module_id = module_id_at_tier(slot, entry.best_tier).to_string();
            ShipModuleSelection {
                slot_id: slot.hull_slot_id.clone(),
                module_id,
            }
        })
        .collect();
    let module_count = selected_modules.len();
    let required_build_points =
        template_uniform_cost(shipbuilding_data, template, entry.best_tier);

    // Cargo capacity sum.
    let mut cargo_capacity_t = 0.0;
    for slot in &template.cargo_slots {
        let module_id = module_ref_for_slot(&selected_modules, &slot.hull_slot_id)
            .unwrap_or(slot.default_module.as_str());
        if let Some(module) = shipbuilding_data.get_module(module_id) {
            for (key, value) in &module.attribute_values {
                if key == "cargo_capacity_t" {
                    cargo_capacity_t += value;
                }
            }
        }
    }

    // Resource cost = hull.resource_costs + per-module.resource_costs.
    let mut resource_costs: Vec<(crate::economy::ResourceType, f64)> = Vec::new();
    for sel in &selected_modules {
        if let Some(module) = shipbuilding_data.get_module(&sel.module_id) {
            for (rt, amt) in &module.resource_costs {
                merge_resource_cost(&mut resource_costs, *rt, *amt);
            }
        }
    }
    for (rt, amt) in &hull_def.resource_costs {
        merge_resource_cost(&mut resource_costs, *rt, *amt);
    }

    // Mass: hull base + per-module dry mass.  Coarse; freighters don't
    // carry fuel so launch_mass == dry_mass.
    let mut dry_mass_t = hull_def.base_dry_mass_t;
    for sel in &selected_modules {
        if let Some(module) = shipbuilding_data.get_module(&sel.module_id) {
            dry_mass_t += module.dry_mass_t;
        }
    }

    let project = ShipConstructionProject {
        template_id: Uuid::new_v4(),
        design_name: format!("{} (auto)", template.display_name),
        hull_id: template.base_hull.clone(),
        build_site,
        integration_target_fleet: None,
        selected_modules,
        ship_class: hull_def.class.clone(),
        propulsion: None,
        progress: 0.0,
        required_build_points,
        dry_mass_t,
        launch_mass_t: dry_mass_t,
        fuel_capacity_t: 0.0,
        cargo_capacity_t,
        ordnance_capacity_t: 0.0,
        magazine_capacity_t: 0.0,
        crew: 0.0,
        power_generation_mw: 0.0,
        power_draw_mw: 0.0,
        thrust_kn: 0.0,
        isp_s: 0.0,
        acceleration_ms2: 0.0,
        delta_v_ms: 0.0,
        sensor_range_au: 0.0,
        docking_ports: 0.0,
        construction_capacity_bp_per_year: 0.0,
        launch_capacity_t_per_year: 0.0,
        is_station: hull_def.is_station,
        construction_mode: hull_def.default_construction_mode.clone(),
        state: ShipConstructionState::Building,
        awaiting_resources: false,
        blocking_request_ids: Vec::new(),
        module_count,
        resource_costs,
        launch_resource_costs: Vec::new(),
        launch_credit_cost_mc: 0.0,
        building_company_idx: Some(company_idx),
    };
    commands.spawn(project).id()
}

fn module_ref_for_slot<'a>(
    selected_modules: &'a [ShipModuleSelection],
    hull_slot_id: &str,
) -> Option<&'a str> {
    selected_modules
        .iter()
        .find(|s| s.slot_id == hull_slot_id)
        .map(|s| s.module_id.as_str())
}

fn merge_resource_cost(
    acc: &mut Vec<(crate::economy::ResourceType, f64)>,
    rt: crate::economy::ResourceType,
    amt: f64,
) {
    for entry in acc.iter_mut() {
        if entry.0 == rt {
            entry.1 += amt;
            return;
        }
    }
    acc.push((rt, amt));
}

/// Local copy of `templates::module_id_at_tier` (which is private to
/// `templates.rs`).  Returns the module id installed in `slot` at the
/// given `tier` (0 = default; tiers ≥ 1 look up the highest
/// `upgrade_path` step with `step.tier <= tier`).
fn module_id_at_tier(slot: &CargoSlot, tier: u32) -> &str {
    if tier == 0 {
        return slot.default_module.as_str();
    }
    let mut chosen: Option<&UpgradeStep> = None;
    for step in &slot.upgrade_path {
        if step.tier <= tier {
            chosen = Some(step);
        } else {
            break;
        }
    }
    chosen
        .map(|s| s.module.as_str())
        .unwrap_or(slot.default_module.as_str())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astronomy::components::SpaceCoordinates;
    use crate::colony::components::Colony;
    use crate::colony::types::BuildingType;
    use crate::economy::components::LocalStockpile;
    use crate::economy::logistics::{RequestPriority, ResourceRequest};
    use crate::economy::ResourceType;
    use crate::fleets::ShipClass;
    use crate::shipbuilding::data::{
        HullSlotDefinition, ShipHullDefinition, ShipModuleCategory, ShipModuleDefinition,
    };
    use crate::shipbuilding::types::ConstructionMode;

    /// Build a colony with a Shipyard at the given `body_entity`.
    fn spawn_colony_with_shipyard(world: &mut World, name: &str) -> Entity {
        let mut colony = Colony::new(name.to_string(), 1_000.0);
        colony.add_building(BuildingType::Shipyard);
        world
            .spawn((
                colony,
                LocalStockpile::default(),
                SpaceCoordinates::default(),
            ))
            .id()
    }

    /// Build a minimal `ShipbuildingData` with a `freighter_frame` hull
    /// + a `cargo_pod_medium` module.  The `light_freighter` template
    /// used by the test below binds these.
    fn build_minimal_shipbuilding_data() -> ShipbuildingData {
        let mut data = ShipbuildingData::default();
        data.hulls.insert(
            "freighter_frame".to_string(),
            ShipHullDefinition {
                id: "freighter_frame".to_string(),
                display_name: "Freighter Frame".to_string(),
                description: String::new(),
                class: ShipClass::Freighter,
                tier: 1,
                base_build_points: 100.0,
                base_dry_mass_t: 100.0,
                default_construction_mode: ConstructionMode::OrbitalAssembly,
                surface_launchable: false,
                orbital_only: false,
                is_station: false,
                size_tier: None,
                required_tech: Some("chemical_spaceframes".to_string()),
                resource_costs: vec![(ResourceType::Iron, 50.0)],
                slot_layout: vec![
                    HullSlotDefinition {
                        slot_id: "cargo_a".to_string(),
                        category: ShipModuleCategory::CargoStorage,
                        size: "Medium".to_string(),
                        required: true,
                        position: None,
                        rotation_deg: None,
                    },
                    HullSlotDefinition {
                        slot_id: "cargo_b".to_string(),
                        category: ShipModuleCategory::CargoStorage,
                        size: "Medium".to_string(),
                        required: true,
                        position: None,
                        rotation_deg: None,
                    },
                ],
                tags: vec![],
            },
        );
        data.modules.insert(
            "cargo_pod_medium".to_string(),
            ShipModuleDefinition {
                id: "cargo_pod_medium".to_string(),
                display_name: "Cargo Pod (M)".to_string(),
                description: String::new(),
                category: ShipModuleCategory::CargoStorage,
                size: "Medium".to_string(),
                tier: 1,
                propulsion: None,
                required_tech: None,
                required_component_design: None,
                power_generation_mw: 0.0,
                power_draw_mw: 0.0,
                thrust_kn: 0.0,
                isp_s: 0.0,
                dry_mass_t: 5.0,
                build_points: 10.0,
                construction_capacity_bp_per_year: 0.0,
                launch_capacity_t_per_year: 0.0,
                resource_costs: vec![(ResourceType::Iron, 10.0)],
                attribute_values: vec![("cargo_capacity_t".to_string(), 35.0)],
                tags: vec![],
            },
        );
        data
    }

    fn build_light_freighter_template() -> FreighterTemplate {
        FreighterTemplate {
            id: "light_freighter".to_string(),
            display_name: "Light Freighter".to_string(),
            description: String::new(),
            base_hull: "freighter_frame".to_string(),
            era_tier: 1,
            required_tech: Some("chemical_spaceframes".to_string()),
            cargo_slots: vec![
                CargoSlot {
                    hull_slot_id: "cargo_a".to_string(),
                    default_module: "cargo_pod_medium".to_string(),
                    upgrade_path: vec![],
                },
                CargoSlot {
                    hull_slot_id: "cargo_b".to_string(),
                    default_module: "cargo_pod_medium".to_string(),
                    upgrade_path: vec![],
                },
            ],
            tags: vec![],
        }
    }

    fn push_old_pending_request(requests: &mut PendingResourceRequests, dest: Entity) -> u64 {
        let now = 0.0; // 0 keeps the math simple; the test will set sim_time past the threshold
        requests.add(ResourceRequest {
            id: 0,
            destination_body: dest,
            destination_name: "Test Colony".into(),
            resource: ResourceType::Iron,
            amount_mt: 50.0,
            priority: RequestPriority::Construction,
            state: RequestState::Pending,
            in_transit_mt: 0.0,
            eta_seconds: None,
            assigned_company_idx: None,
            created_at_seconds: now,
            source_body: Some(dest),
            linked_project: None,
            payment_made: false,
            completed_at_seconds: None,
            assignee_fleet_id: None,
        })
    }

    /// Test helper: init the resources the auto-build system reads.
    fn init_econ_resources(world: &mut World) {
        world.init_resource::<PendingResourceRequests>();
        world.init_resource::<ShippingCompanies>();
        world.init_resource::<ResearchState>();
        world.init_resource::<SimulationTime>();
        world.init_resource::<AutoBuildNotificationState>();
        world.init_resource::<Messages<FreighterBuildNoDesignAvailable>>();
    }

    /// GRA-39 AC #5: when a company has `AutoBuild`, a `home_body` with
    /// a shipyard, ≥1 unfulfilled demand at the home body, sufficient
    /// treasury, and a buildable freighter template, the auto-build loop
    /// spawns a `ShipConstructionProject` at the home body with
    /// `building_company_idx = Some(company_idx)` and decrements the
    /// treasury.
    #[test]
    fn test_queues_build_on_demand_spike() {
        let mut app = App::new();
        let mut schedule = Schedule::default();
        init_econ_resources(app.world_mut());

        // World: a colony (home body) with a Shipyard, plus a freighter
        // template registry + shipbuilding data the registry can validate
        // against.
        let home_body = spawn_colony_with_shipyard(app.world_mut(), "Home");
        let mut data = build_minimal_shipbuilding_data();
        // Resources to actually build the freighter locally.
        data.modules.get_mut("cargo_pod_medium").unwrap().build_points = 10.0;
        app.world_mut().insert_resource(data);

        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(build_light_freighter_template());
        app.world_mut().insert_resource(registry);

        // Top up the home body's local stockpile with enough Iron to
        // build the freighter locally.
        {
            let mut ls = app
                .world_mut()
                .entity_mut(home_body)
                .get_mut::<LocalStockpile>()
                .unwrap();
            ls.add(ResourceType::Iron, 1_000.0);
        }

        // Company: AutoBuild, home_body, plenty of treasury.
        let mut company = crate::economy::company::ShippingCompany::new("Test Co.", 0, 0.0)
            .with_build_policy(CompanyBuildPolicy::AutoBuild)
            .with_home_body(home_body);
        company.treasury_mc = 1_000_000.0;
        app.world_mut()
            .resource_mut::<ShippingCompanies>()
            .companies = vec![company];

        // One Pending request at the home body, *older* than the
        // demand-age threshold (we set sim_time to 2× the threshold so
        // created_at = 0 is well past it).
        let request_id = {
            let mut requests = app.world_mut().resource_mut::<PendingResourceRequests>();
            push_old_pending_request(&mut requests, home_body)
        };

        // Sim time: 2× the demand threshold so created_at = 0 is older.
        {
            let mut sim = app.world_mut().resource_mut::<SimulationTime>();
            sim.elapsed = DEMAND_AGE_THRESHOLD_S * 2.0;
        }

        // Research: chemical_spaceframes must be researched for the
        // light_freighter template's `required_tech` to pass.
        {
            let mut research = app.world_mut().resource_mut::<ResearchState>();
            research.unlock_tech("chemical_spaceframes".to_string());
        }

        let treasury_before = app
            .world()
            .resource::<ShippingCompanies>()
            .companies[0]
            .treasury_mc;

        // Run the system once.
        schedule.add_systems(auto_build_loop);
        schedule.run(app.world_mut());

        // A ShipConstructionProject must now exist at the home body,
        // owned by company 0.
        let mut found = None;
        {
            let mut q = app
                .world_mut()
                .query::<(Entity, &ShipConstructionProject)>();
            for (e, p) in q.iter(app.world()) {
                if p.building_company_idx == Some(0) {
                    found = Some((e, p.build_site, p.hull_id.clone()));
                    break;
                }
            }
        }
        let (proj_entity, build_site, hull_id) =
            found.expect("a ShipConstructionProject should be spawned");

        assert_eq!(
            build_site, home_body,
            "project should be at the company's home body"
        );
        assert_eq!(
            hull_id, "freighter_frame",
            "project hull_id should come from the light_freighter template"
        );

        // Treasury must have been debited.
        let treasury_after = app
            .world()
            .resource::<ShippingCompanies>()
            .companies[0]
            .treasury_mc;
        assert!(
            treasury_after < treasury_before,
            "treasury must decrease after the build is queued"
        );

        // `active_builds` cache must be 1.
        let active = app
            .world()
            .resource::<ShippingCompanies>()
            .companies[0]
            .active_builds;
        assert_eq!(active, 1, "company.active_builds should be 1");

        // The original request must still be Pending (the build doesn't
        // touch request state).
        let req = app
            .world()
            .resource::<PendingResourceRequests>()
            .find_by_id(request_id)
            .expect("request still present");
        assert_eq!(req.state, RequestState::Pending);

        // Suppress unused-binding warning for proj_entity in case a
        // future change needs the handle.
        let _ = proj_entity;
    }
}
