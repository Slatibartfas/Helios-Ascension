use bevy::prelude::*;

use super::components::{
    LaunchCapacityState, PendingShipbuildingActions, QueueShipConstructionAction,
    ShipConstructionProject, ShipConstructionState, ShipDesignAssignment,
};
use super::data::{ShipDesignLibrary, ShipDesignSummary, ShipHullDefinition, ShipbuildingData};
use super::refit::{determine_refit_type, RefitProject, RefitType};
use super::types::ConstructionMode;
use crate::colony::{BuildingType, Colony};
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::economy::components::LocalStockpile;
use crate::economy::logistics::{
    PendingResourceRequests, RequestPriority, RequestState, ResourceRequest,
};
use crate::economy::{GlobalBudget, ResourceType};
use crate::fleets::{
    Fleet, FleetOrbit, PropulsionType, ShipClass, ShipInfo, ShipInstance, AU_IN_METERS,
};
use crate::plugins::solar_system::CelestialBody;
use crate::research::ResearchState;
use crate::ui::SimulationTime;

const SHIPYARD_BP_PER_YEAR: f64 = 600.0;
const FACTORY_SUPPORT_BP_PER_YEAR: f64 = 75.0;
const FACTORIES_SUPPORTED_PER_SHIPYARD: f64 = 3.0;
const ENGINEERING_BAY_BONUS: f64 = 0.03;

const LAUNCH_SITE_CAPACITY_T_PER_YEAR: f64 = 5.0;
const SPACE_PORT_CAPACITY_T_PER_YEAR: f64 = 40.0;
const ORBITAL_LIFT_CAPACITY_T_PER_YEAR: f64 = 25_000.0;

const BASE_LAUNCH_ALTITUDE_KM: f64 = 400.0;
const STATION_ORBIT_ALTITUDE_KM: f64 = 1_000.0;

const LAUNCH_CREDIT_COST_PER_TON_MC: f64 = 0.45;
const LAUNCH_METHANE_PER_TON_MT: f64 = 0.000_000_12;
const LAUNCH_OXYGEN_PER_TON_MT: f64 = 0.000_000_22;
const LAUNCH_POLYMERS_PER_TON_MT: f64 = 0.000_000_01;

pub fn has_surface_launch_infrastructure(colony: &Colony) -> bool {
    colony.building_count(BuildingType::LaunchSite) > 0
        || colony.building_count(BuildingType::SpacePort) > 0
        || colony.building_count(BuildingType::OrbitalLift) > 0
}

pub fn queue_validation_errors(
    colony: Option<&Colony>,
    hull: Option<&ShipHullDefinition>,
    summary: Option<&ShipDesignSummary>,
    mode: ConstructionMode,
) -> Vec<String> {
    let mut errors = Vec::new();

    let Some(hull) = hull else {
        errors.push("Select an unlocked hull before queueing a design.".to_string());
        return errors;
    };

    if let Some(error) = hull.mode_compatibility_error(mode) {
        errors.push(error.to_string());
    }

    let Some(summary) = summary else {
        errors
            .push("The current design is invalid or locked by research requirements.".to_string());
        return errors;
    };

    if !summary.missing_required_slots.is_empty() {
        errors.push(format!(
            "Missing required slots: {}",
            summary.missing_required_slots.join(", ")
        ));
    }

    let Some(colony) = colony else {
        errors.push("Select a build site before queueing a design.".to_string());
        return errors;
    };

    if colony.building_count(BuildingType::Shipyard) == 0 {
        errors.push("Selected colony needs an operational Shipyard.".to_string());
    }

    if mode == ConstructionMode::SurfaceLaunch && !has_surface_launch_infrastructure(colony) {
        errors.push(
            "Selected colony needs a Launch Site, Space Port, or Orbital Lift for surface launches."
                .to_string(),
        );
    }

    errors
}

fn create_ship_resource_requests(
    resource_requests: &mut PendingResourceRequests,
    destination_body: Entity,
    destination_name: &str,
    created_at_seconds: f64,
    costs: &[(ResourceType, f64)],
) -> Vec<u64> {
    let mut request_ids = Vec::new();

    for (resource, amount_mt) in costs {
        if *amount_mt <= 0.0 {
            continue;
        }

        let request_id = resource_requests.add(ResourceRequest {
            id: 0,
            destination_body,
            destination_name: destination_name.to_string(),
            resource: *resource,
            amount_mt: *amount_mt,
            priority: RequestPriority::Construction,
            state: RequestState::Pending,
            in_transit_mt: 0.0,
            eta_seconds: None,
            assigned_company_idx: None,
            created_at_seconds,
            source_body: None,
            linked_project: None,
            payment_made: false,
            completed_at_seconds: None,
            assignee_fleet_id: None,
        });
        request_ids.push(request_id);
    }

    request_ids
}

fn design_from_template(
    template: &crate::shipbuilding::ShipDesignTemplate,
) -> super::ShipDesignDraft {
    super::ShipDesignDraft {
        name: template.name.clone(),
        hull_id: template.hull_id.clone(),
        modules: template.modules.clone(),
        construction_mode: template.construction_mode,
    }
}

fn template_descends_from(
    design_library: &ShipDesignLibrary,
    candidate_id: uuid::Uuid,
    ancestor_id: uuid::Uuid,
) -> bool {
    let mut cursor = Some(candidate_id);
    while let Some(template_id) = cursor {
        if template_id == ancestor_id {
            return true;
        }

        cursor = design_library
            .get_template(&template_id)
            .and_then(|template| template.parent_template_id);
    }

    false
}

fn module_refit_delta(
    old_modules: &[super::ShipModuleSelection],
    new_modules: &[super::ShipModuleSelection],
) -> (Vec<String>, Vec<String>) {
    let old_by_slot: std::collections::HashMap<_, _> = old_modules
        .iter()
        .map(|selection| (selection.slot_id.as_str(), selection.module_id.as_str()))
        .collect();
    let new_by_slot: std::collections::HashMap<_, _> = new_modules
        .iter()
        .map(|selection| (selection.slot_id.as_str(), selection.module_id.as_str()))
        .collect();

    let mut removed = Vec::new();
    let mut added = Vec::new();

    for (slot_id, old_module_id) in &old_by_slot {
        match new_by_slot.get(slot_id) {
            Some(new_module_id) if *new_module_id == *old_module_id => {}
            _ => removed.push((*old_module_id).to_string()),
        }
    }

    for (slot_id, new_module_id) in &new_by_slot {
        match old_by_slot.get(slot_id) {
            Some(old_module_id) if *old_module_id == *new_module_id => {}
            _ => added.push((*new_module_id).to_string()),
        }
    }

    (removed, added)
}

pub fn process_pending_shipbuilding_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingShipbuildingActions>,
    colonies: Query<&Colony>,
    mut stockpiles: Query<&mut LocalStockpile>,
    shipbuilding_data: Res<ShipbuildingData>,
    design_library: Res<ShipDesignLibrary>,
    research_state: Res<ResearchState>,
    mut resource_requests: ResMut<PendingResourceRequests>,
    sim_time: Res<SimulationTime>,
    ships: Query<(Entity, &ShipInstance, &ShipDesignAssignment)>,
) {
    let now = sim_time.elapsed_seconds();

    for QueueShipConstructionAction {
        build_site,
        template_id,
        integration_target_fleet,
    } in actions.queue_projects.drain(..)
    {
        let Ok(colony) = colonies.get(build_site) else {
            warn!(
                "Ignoring shipbuilding action for non-colony entity {:?}",
                build_site
            );
            continue;
        };

        let Some(template) = design_library.get_template(&template_id) else {
            warn!(
                "Ignoring shipbuilding action for missing template {}",
                template_id
            );
            continue;
        };
        let design = design_from_template(template);
        let hull = shipbuilding_data.get_hull(&template.hull_id);

        let Some(summary) = shipbuilding_data.summarize_design(&design, &research_state) else {
            warn!(
                "Rejected invalid or locked ship design '{}' at {}",
                design.name, colony.name
            );
            continue;
        };

        let queue_errors =
            queue_validation_errors(Some(colony), hull, Some(&summary), design.construction_mode);
        if !queue_errors.is_empty() {
            warn!(
                "Rejected ship design '{}' at {}: {}",
                design.name,
                colony.name,
                queue_errors.join(" ")
            );
            continue;
        }

        let blocking_request_ids = if let Ok(mut stockpile) = stockpiles.get_mut(build_site) {
            if stockpile.can_afford(&summary.resource_costs) {
                stockpile.deduct(&summary.resource_costs);
                Vec::new()
            } else {
                let shortfalls: Vec<_> = summary
                    .resource_costs
                    .iter()
                    .filter_map(|(resource, amount)| {
                        let shortfall = (*amount - stockpile.get(resource)).max(0.0);
                        (shortfall > 0.0).then_some((*resource, shortfall))
                    })
                    .collect();
                create_ship_resource_requests(
                    &mut resource_requests,
                    build_site,
                    &colony.name,
                    now,
                    &shortfalls,
                )
            }
        } else {
            create_ship_resource_requests(
                &mut resource_requests,
                build_site,
                &colony.name,
                now,
                &summary.resource_costs,
            )
        };
        let awaiting_resources = !blocking_request_ids.is_empty();

        let selected_modules = design.modules.clone();
        let module_count = selected_modules.len();
        let project_entity = commands
            .spawn(ShipConstructionProject {
                template_id,
                design_name: design.name,
                hull_id: design.hull_id,
                build_site,
                integration_target_fleet,
                selected_modules,
                ship_class: summary.ship_class,
                propulsion: summary.propulsion,
                progress: 0.0,
                required_build_points: summary.build_points,
                dry_mass_t: summary.dry_mass_t,
                launch_mass_t: summary.launch_mass_t,
                fuel_capacity_t: summary.fuel_capacity_t,
                cargo_capacity_t: summary.cargo_capacity_t,
                ordnance_capacity_t: summary.ordnance_capacity_t,
                magazine_capacity_t: summary.magazine_capacity_t,
                crew: summary.crew,
                power_generation_mw: summary.power_generation_mw,
                power_draw_mw: summary.power_draw_mw,
                thrust_kn: summary.thrust_kn,
                isp_s: summary.isp_s,
                acceleration_ms2: summary.acceleration_ms2,
                delta_v_ms: summary.delta_v_ms,
                sensor_range_au: summary.sensor_range_au,
                docking_ports: summary.docking_ports,
                construction_capacity_bp_per_year: summary.construction_capacity_bp_per_year,
                launch_capacity_t_per_year: summary.launch_capacity_t_per_year,
                is_station: summary.is_station,
                construction_mode: design.construction_mode,
                state: ShipConstructionState::Building,
                awaiting_resources,
                blocking_request_ids: blocking_request_ids.clone(),
                module_count,
                resource_costs: summary.resource_costs,
                launch_resource_costs: launch_resource_costs(summary.launch_mass_t),
                launch_credit_cost_mc: summary.launch_mass_t * LAUNCH_CREDIT_COST_PER_TON_MC,
                // Player-queued builds: no owning company.
                building_company_idx: None,
            })
            .id();

        for request_id in blocking_request_ids {
            if let Some(request) = resource_requests.find_by_id_mut(request_id) {
                request.linked_project = Some(project_entity);
            }
        }
    }

    for action in actions.queue_refits.drain(..) {
        let Ok(colony) = colonies.get(action.build_site) else {
            warn!(
                "Ignoring refit action for non-colony entity {:?}",
                action.build_site
            );
            continue;
        };

        if colony.building_count(BuildingType::Shipyard) == 0 {
            warn!(
                "Rejected refit at {} because no operational shipyard is present",
                colony.name
            );
            continue;
        }

        let Ok((ship_entity, ship, assignment)) = ships.get(action.ship_entity) else {
            warn!(
                "Ignoring refit action for missing ship {:?}",
                action.ship_entity
            );
            continue;
        };

        if ship.parked_body != action.build_site {
            warn!(
                "Rejected refit for ship {:?}: ship is not stationed at build site {:?}",
                ship_entity, action.build_site
            );
            continue;
        }

        if assignment.template_id == action.new_template_id {
            continue;
        }

        let Some(old_template) = design_library.get_template(&assignment.template_id) else {
            warn!(
                "Rejected refit for ship {:?}: missing current template {}",
                ship_entity, assignment.template_id
            );
            continue;
        };
        let Some(new_template) = design_library.get_template(&action.new_template_id) else {
            warn!(
                "Rejected refit for ship {:?}: missing target template {}",
                ship_entity, action.new_template_id
            );
            continue;
        };

        if !template_descends_from(&design_library, new_template.id, old_template.id) {
            warn!(
                "Rejected refit for ship {:?}: template {} is not an upgrade of {}",
                ship_entity, new_template.id, old_template.id
            );
            continue;
        }

        if determine_refit_type(&old_template.hull_id, &new_template.hull_id)
            == RefitType::DifferentHull
        {
            warn!(
                "Rejected refit for ship {:?}: hull changes require reconstruction",
                ship_entity
            );
            continue;
        }

        let (removed_module_ids, added_module_ids) =
            module_refit_delta(&old_template.modules, &new_template.modules);
        let removed_module_refs: Vec<_> = removed_module_ids.iter().map(String::as_str).collect();
        let added_module_refs: Vec<_> = added_module_ids.iter().map(String::as_str).collect();
        let bp_cost = RefitProject::calculate_refit_bp(
            &removed_module_refs,
            &added_module_refs,
            &shipbuilding_data,
        );
        let resource_costs = RefitProject::calculate_refit_resources(
            &removed_module_refs,
            &added_module_refs,
            &shipbuilding_data,
        );

        let blocking_request_ids = if let Ok(mut stockpile) = stockpiles.get_mut(action.build_site)
        {
            if stockpile.can_afford(&resource_costs) {
                stockpile.deduct(&resource_costs);
                Vec::new()
            } else {
                let shortfalls: Vec<_> = resource_costs
                    .iter()
                    .filter_map(|(resource, amount)| {
                        let shortfall = (*amount - stockpile.get(resource)).max(0.0);
                        (shortfall > 0.0).then_some((*resource, shortfall))
                    })
                    .collect();
                create_ship_resource_requests(
                    &mut resource_requests,
                    action.build_site,
                    &colony.name,
                    now,
                    &shortfalls,
                )
            }
        } else {
            create_ship_resource_requests(
                &mut resource_requests,
                action.build_site,
                &colony.name,
                now,
                &resource_costs,
            )
        };

        let project_entity = commands
            .spawn(RefitProject {
                ship_entity,
                old_template_id: old_template.id,
                new_template_id: new_template.id,
                bp_cost,
                resource_costs,
                progress: 0.0,
                build_site: action.build_site,
                slipway_id: 0,
                awaiting_resources: !blocking_request_ids.is_empty(),
                blocking_request_ids: blocking_request_ids.clone(),
            })
            .id();

        for request_id in blocking_request_ids {
            if let Some(request) = resource_requests.find_by_id_mut(request_id) {
                request.linked_project = Some(project_entity);
            }
        }
    }

    for entity in actions.cancel_projects.drain(..) {
        commands.entity(entity).despawn();
    }
}

pub fn advance_ship_construction(
    colonies: Query<&Colony>,
    mut projects: Query<(Entity, &mut ShipConstructionProject)>,
    mut refits: Query<(Entity, &mut RefitProject)>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
) {
    let current_elapsed = sim_time.elapsed_seconds();
    let dt = current_elapsed - *last_elapsed;
    *last_elapsed = current_elapsed;

    if dt <= 0.0 {
        return;
    }

    let years_elapsed = dt / SECONDS_PER_YEAR;
    if years_elapsed <= 0.0 {
        return;
    }

    let mut colony_bp: Vec<(Entity, f64)> = Vec::new();
    for (_, project) in projects.iter() {
        if !project.is_building() || project.awaiting_resources {
            continue;
        }

        let colony_entity = project.build_site;
        if colony_bp.iter().any(|(entity, _)| *entity == colony_entity) {
            continue;
        }

        if let Ok(colony) = colonies.get(colony_entity) {
            let shipyards = colony.building_count(BuildingType::Shipyard) as f64;
            if shipyards <= 0.0 {
                continue;
            }

            let factories = colony.building_count(BuildingType::Factory) as f64;
            let effective_factories = factories.min(shipyards * FACTORIES_SUPPORTED_PER_SHIPYARD);
            let engineering_bays = colony.building_count(BuildingType::EngineeringBay) as f64;
            let bonus = 1.0 + engineering_bays * ENGINEERING_BAY_BONUS;
            let bp = (shipyards * SHIPYARD_BP_PER_YEAR
                + effective_factories * FACTORY_SUPPORT_BP_PER_YEAR)
                * bonus
                * years_elapsed;
            colony_bp.push((colony_entity, bp));
        }
    }

    for (_, refit) in refits.iter() {
        if refit.awaiting_resources {
            continue;
        }

        let colony_entity = refit.build_site;
        if colony_bp.iter().any(|(entity, _)| *entity == colony_entity) {
            continue;
        }

        if let Ok(colony) = colonies.get(colony_entity) {
            let shipyards = colony.building_count(BuildingType::Shipyard) as f64;
            if shipyards <= 0.0 {
                continue;
            }

            let factories = colony.building_count(BuildingType::Factory) as f64;
            let effective_factories = factories.min(shipyards * FACTORIES_SUPPORTED_PER_SHIPYARD);
            let engineering_bays = colony.building_count(BuildingType::EngineeringBay) as f64;
            let bonus = 1.0 + engineering_bays * ENGINEERING_BAY_BONUS;
            let bp = (shipyards * SHIPYARD_BP_PER_YEAR
                + effective_factories * FACTORY_SUPPORT_BP_PER_YEAR)
                * bonus
                * years_elapsed;
            colony_bp.push((colony_entity, bp));
        }
    }

    #[derive(Clone, Copy)]
    enum YardWorkItem {
        Construction(Entity),
        Refit(Entity),
    }

    for (colony_entity, mut available_bp) in colony_bp {
        let mut work_items: Vec<YardWorkItem> = projects
            .iter()
            .filter(|(_, project)| {
                project.build_site == colony_entity
                    && project.is_building()
                    && !project.awaiting_resources
            })
            .map(|(entity, _)| YardWorkItem::Construction(entity))
            .collect();
        work_items.extend(
            refits
                .iter()
                .filter(|(_, refit)| refit.build_site == colony_entity && !refit.awaiting_resources)
                .map(|(entity, _)| YardWorkItem::Refit(entity)),
        );
        work_items.sort_by_key(|item| match item {
            YardWorkItem::Construction(entity) | YardWorkItem::Refit(entity) => *entity,
        });

        for work_item in work_items {
            if available_bp <= 0.0 {
                break;
            }

            match work_item {
                YardWorkItem::Construction(project_entity) => {
                    if let Ok((_, mut project)) = projects.get_mut(project_entity) {
                        let needed = project.required_build_points - project.progress;
                        let applied = needed.min(available_bp);
                        project.progress += applied;
                        available_bp -= applied;

                        if project.progress >= project.required_build_points {
                            project.state = match project.construction_mode {
                                ConstructionMode::SurfaceLaunch => {
                                    ShipConstructionState::ReadyForLaunch
                                }
                                ConstructionMode::OrbitalAssembly
                                | ConstructionMode::OrbitalShipyard => {
                                    ShipConstructionState::CompletedInOrbit
                                }
                            };
                        }
                    }
                }
                YardWorkItem::Refit(refit_entity) => {
                    if let Ok((_, mut refit)) = refits.get_mut(refit_entity) {
                        let needed = refit.bp_cost - refit.progress;
                        let applied = needed.min(available_bp);
                        refit.progress += applied;
                        available_bp -= applied;
                    }
                }
            }
        }
    }
}

pub fn process_ship_launches_and_completions(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    colonies: Query<(Entity, &Colony, &CelestialBody)>,
    fleet_orbits: Query<&FleetOrbit, With<Fleet>>,
    mut stockpiles: Query<&mut LocalStockpile>,
    mut budget: ResMut<GlobalBudget>,
    mut launch_state: ResMut<LaunchCapacityState>,
    mut resource_requests: ResMut<PendingResourceRequests>,
    mut projects: Query<(Entity, &mut ShipConstructionProject)>,
    mut refits: Query<(Entity, &mut RefitProject)>,
    mut ship_queries: ParamSet<(
        Query<&ShipInstance>,
        Query<(&mut ShipInstance, &mut ShipDesignAssignment)>,
    )>,
    design_library: Res<ShipDesignLibrary>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    freighter_registry: Res<crate::ships::templates::FreighterTemplateRegistry>,
) {
    let current_elapsed = sim_time.elapsed_seconds();
    let dt = current_elapsed - *last_elapsed;
    *last_elapsed = current_elapsed;
    let years_elapsed = (dt / SECONDS_PER_YEAR).max(0.0);

    for (site_entity, colony, _) in colonies.iter() {
        let annual_capacity = annual_launch_capacity_t(colony);
        let available = launch_state
            .available_mass_t
            .entry(site_entity)
            .or_insert(annual_capacity.max(0.0));
        *available = (*available + annual_capacity * years_elapsed).min(annual_capacity.max(0.0));
    }

    let mut project_entities: Vec<Entity> = projects.iter().map(|(entity, _)| entity).collect();
    project_entities.sort();

    for project_entity in project_entities {
        let Ok((entity, mut project)) = projects.get_mut(project_entity) else {
            continue;
        };

        if project.awaiting_resources {
            let still_waiting = resource_requests
                .requests
                .iter()
                .any(|request| request.linked_project == Some(entity) && request.is_open());
            if still_waiting {
                continue;
            }
            project.awaiting_resources = false;
            project.blocking_request_ids.clear();
        }

        match project.state {
            ShipConstructionState::Building => {}
            ShipConstructionState::CompletedInOrbit => {
                if let Ok((_, _, body)) = colonies.get(project.build_site) {
                    let integration_target = integration_target_state(
                        project.integration_target_fleet,
                        project.build_site,
                        &fleet_orbits,
                        &ship_queries.p0(),
                    );
                    let (assigned_fleet, orbit_radius_au, stationary, sort_order) =
                        integration_target.unwrap_or((
                            None,
                            insertion_orbit_radius_au(body, project.is_station),
                            project.is_station,
                            0,
                        ));
                    let ship_entity = commands
                        .spawn((
                            ShipInstance::new(
                                build_ship_info(&project),
                                project.build_site,
                                orbit_radius_au,
                                stationary,
                                assigned_fleet,
                                sort_order,
                            ),
                            ShipDesignAssignment {
                                template_id: project.template_id,
                            },
                        ))
                        .id();
                    if let Some((template_ref, marker, slots)) = freighter_template_components(
                        &freighter_registry,
                        &research_state,
                        &project,
                    ) {
                        commands
                            .entity(ship_entity)
                            .insert(template_ref)
                            .insert(marker)
                            .insert(slots);
                    }
                    commands.entity(entity).despawn();
                }
            }
            ShipConstructionState::ReadyForLaunch => {
                let Ok((_, colony, body)) = colonies.get(project.build_site) else {
                    continue;
                };

                let available_capacity = launch_state
                    .available_mass_t
                    .entry(project.build_site)
                    .or_insert_with(|| annual_launch_capacity_t(colony));

                if *available_capacity + f64::EPSILON < project.launch_mass_t {
                    continue;
                }

                let mut can_launch = false;
                if let Ok(mut stockpile) = stockpiles.get_mut(project.build_site) {
                    if stockpile.can_afford(&project.launch_resource_costs)
                        && budget.treasury >= project.launch_credit_cost_mc
                    {
                        stockpile.deduct(&project.launch_resource_costs);
                        budget.treasury -= project.launch_credit_cost_mc;
                        can_launch = true;
                    } else {
                        let existing_requests = resource_requests.requests.iter().any(|request| {
                            request.linked_project == Some(entity) && request.is_open()
                        });
                        if !existing_requests {
                            let launch_costs = project.launch_resource_costs.clone();
                            for (resource, amount) in launch_costs {
                                let shortfall = (amount - stockpile.get(&resource)).max(0.0);
                                if shortfall <= 0.0 {
                                    continue;
                                }
                                let request_id = resource_requests.add(ResourceRequest {
                                    id: 0,
                                    destination_body: project.build_site,
                                    destination_name: colony.name.clone(),
                                    resource,
                                    amount_mt: shortfall,
                                    priority: RequestPriority::Construction,
                                    state: RequestState::Pending,
                                    in_transit_mt: 0.0,
                                    eta_seconds: None,
                                    assigned_company_idx: None,
                                    created_at_seconds: current_elapsed,
                                    source_body: None,
                                    linked_project: Some(entity),
                                    payment_made: false,
                                    completed_at_seconds: None,
                                    assignee_fleet_id: None,
                                });
                                project.blocking_request_ids.push(request_id);
                            }
                        }
                        project.awaiting_resources = true;
                    }
                }

                if !can_launch {
                    continue;
                }

                *available_capacity -= project.launch_mass_t;
                let integration_target = integration_target_state(
                    project.integration_target_fleet,
                    project.build_site,
                    &fleet_orbits,
                    &ship_queries.p0(),
                );
                let (assigned_fleet, orbit_radius_au, stationary, sort_order) = integration_target
                    .unwrap_or((
                        None,
                        insertion_orbit_radius_au(body, project.is_station),
                        project.is_station,
                        0,
                    ));
                let ship_entity = commands
                    .spawn((
                        ShipInstance::new(
                            build_ship_info(&project),
                            project.build_site,
                            orbit_radius_au,
                            stationary,
                            assigned_fleet,
                            sort_order,
                        ),
                        ShipDesignAssignment {
                            template_id: project.template_id,
                        },
                    ))
                    .id();
                if let Some((template_ref, marker, slots)) =
                    freighter_template_components(&freighter_registry, &research_state, &project)
                {
                    commands
                        .entity(ship_entity)
                        .insert(template_ref)
                        .insert(marker)
                        .insert(slots);
                }
                commands.entity(entity).despawn();
            }
        }
    }

    let mut refit_entities: Vec<Entity> = refits.iter().map(|(entity, _)| entity).collect();
    refit_entities.sort();

    for refit_entity in refit_entities {
        let Ok((entity, mut refit)) = refits.get_mut(refit_entity) else {
            continue;
        };

        if refit.awaiting_resources {
            let still_waiting = resource_requests
                .requests
                .iter()
                .any(|request| request.linked_project == Some(entity) && request.is_open());
            if still_waiting {
                continue;
            }
            refit.awaiting_resources = false;
            refit.blocking_request_ids.clear();
        }

        if refit.progress + f64::EPSILON < refit.bp_cost {
            continue;
        }

        let Some(template) = design_library.get_template(&refit.new_template_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        let draft = design_from_template(template);
        let Some(summary) = shipbuilding_data.summarize_design(&draft, &research_state) else {
            commands.entity(entity).despawn();
            continue;
        };

        let mut ship_assignments = ship_queries.p1();
        let Ok((mut ship, mut assignment)) = ship_assignments.get_mut(refit.ship_entity) else {
            commands.entity(entity).despawn();
            continue;
        };

        let fuel_fraction = if ship.info.max_fuel_t > 0.0 {
            ship.info.fuel_mass_t / ship.info.max_fuel_t
        } else {
            0.0
        };
        let mut updated_info = build_ship_info_from_summary(&template.name, &summary);
        updated_info.fuel_mass_t *= fuel_fraction.clamp(0.0, 1.0);
        ship.info = updated_info;
        ship.stationary = summary.is_station;
        assignment.template_id = template.id;

        commands.entity(entity).despawn();
    }
}

fn integration_target_state(
    target_fleet: Option<Entity>,
    build_site: Entity,
    fleet_orbits: &Query<&FleetOrbit, With<Fleet>>,
    ship_instances: &Query<&ShipInstance>,
) -> Option<(Option<Entity>, f64, bool, i32)> {
    let fleet_entity = target_fleet?;
    let orbit = fleet_orbits.get(fleet_entity).ok()?;
    if orbit.body != build_site {
        return None;
    }

    Some((
        Some(fleet_entity),
        orbit.radius_au,
        orbit.direction == 0.0,
        next_sort_order_for_fleet(ship_instances, fleet_entity),
    ))
}

fn next_sort_order_for_fleet(ship_instances: &Query<&ShipInstance>, fleet_entity: Entity) -> i32 {
    ship_instances
        .iter()
        .filter(|ship| ship.assigned_fleet == Some(fleet_entity))
        .map(|ship| ship.sort_order)
        .max()
        .unwrap_or(-1)
        + 1
}

pub fn annual_launch_capacity_t(colony: &Colony) -> f64 {
    colony.building_count(BuildingType::LaunchSite) as f64 * LAUNCH_SITE_CAPACITY_T_PER_YEAR
        + colony.building_count(BuildingType::SpacePort) as f64 * SPACE_PORT_CAPACITY_T_PER_YEAR
        + colony.building_count(BuildingType::OrbitalLift) as f64 * ORBITAL_LIFT_CAPACITY_T_PER_YEAR
}

pub fn launch_resource_costs(launch_mass_t: f64) -> Vec<(ResourceType, f64)> {
    vec![
        (
            ResourceType::Methane,
            launch_mass_t * LAUNCH_METHANE_PER_TON_MT,
        ),
        (
            ResourceType::Oxygen,
            launch_mass_t * LAUNCH_OXYGEN_PER_TON_MT,
        ),
        (
            ResourceType::Polymers,
            launch_mass_t * LAUNCH_POLYMERS_PER_TON_MT,
        ),
    ]
}

/// Build the freighter-template components to bundle with a newly-spawned
/// `ShipInstance`.  Returns a tuple `(ShipTemplateRef, Vec<ShipSlot>)` if
/// the project matches a registered freighter template; returns `None`
/// for non-freighter classes or when no template matches the hull — the
/// migration shim
/// (`crate::ships::migration::migrate_legacy_freighters`) is the
/// catch-all for the legacy case and only runs at startup, so most
/// non-freighter classes simply won't get template components here and
/// that's fine.
fn freighter_template_components(
    registry: &crate::ships::templates::FreighterTemplateRegistry,
    research_state: &ResearchState,
    project: &ShipConstructionProject,
) -> Option<(
    crate::ships::components::ShipTemplateRef,
    crate::ships::components::FreighterTemplateMarker,
    crate::ships::components::FreighterSlots,
)> {
    use crate::ships::components::{
        FreighterSlots, FreighterTemplateMarker, ShipSlot, ShipTemplateRef,
    };

    let hull_id = &project.hull_id;
    let template_id = crate::ships::migration::default_template_for_hull(
        registry,
        project.ship_class,
        hull_id,
        research_state,
    )?;

    let template = registry.get(&template_id)?;

    let slots: Vec<ShipSlot> = template
        .cargo_slots
        .iter()
        .map(|slot| ShipSlot::new(&slot.hull_slot_id, &slot.default_module, 0))
        .collect();

    Some((
        ShipTemplateRef::new(template_id),
        FreighterTemplateMarker,
        FreighterSlots::new(slots),
    ))
}

fn build_ship_info(project: &ShipConstructionProject) -> ShipInfo {
    let propulsion = project.propulsion.unwrap_or(PropulsionType::Chemical);
    let fuel_mass = project.fuel_capacity_t.max(0.0) as f32;
    ShipInfo {
        name: project.design_name.clone(),
        hull_id: Some(project.hull_id.clone()),
        class: if project.is_station {
            ShipClass::Station
        } else {
            project.ship_class
        },
        dry_mass_t: project.dry_mass_t as f32,
        fuel_mass_t: fuel_mass,
        max_fuel_t: fuel_mass,
        thrust_kn: project.thrust_kn as f32,
        isp_s: project.isp_s as f32,
        propulsion,
        cargo_capacity_t: 0.0,
    }
}

fn build_ship_info_from_summary(name: &str, summary: &ShipDesignSummary) -> ShipInfo {
    let propulsion = summary.propulsion.unwrap_or(PropulsionType::Chemical);
    let fuel_mass = summary.fuel_capacity_t.max(0.0) as f32;
    ShipInfo {
        name: name.to_string(),
        hull_id: None,
        class: if summary.is_station {
            ShipClass::Station
        } else {
            summary.ship_class
        },
        dry_mass_t: summary.dry_mass_t as f32,
        fuel_mass_t: fuel_mass,
        max_fuel_t: fuel_mass,
        thrust_kn: summary.thrust_kn as f32,
        isp_s: summary.isp_s as f32,
        propulsion,
        cargo_capacity_t: 0.0,
    }
}

fn insertion_orbit_radius_au(body: &CelestialBody, station: bool) -> f64 {
    let altitude_km = if station {
        STATION_ORBIT_ALTITUDE_KM
    } else {
        BASE_LAUNCH_ALTITUDE_KM
    };

    ((body.radius as f64 + altitude_km) * 1_000.0) / AU_IN_METERS
}
