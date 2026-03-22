use bevy::prelude::*;

use super::components::{
    LaunchCapacityState, PendingShipbuildingActions, QueueShipConstructionAction,
    ShipConstructionProject, ShipConstructionState,
};
use super::data::{ShipDesignLibrary, ShipDesignSummary, ShipHullDefinition, ShipbuildingData};
use super::types::ConstructionMode;
use super::ShipDesignTemplate;
use crate::colony::{BuildingType, Colony};
use crate::economy::budget::SECONDS_PER_YEAR;
use crate::economy::components::LocalStockpile;
use crate::economy::logistics::{
    PendingResourceRequests, RequestPriority, RequestState, ResourceRequest,
};
use crate::economy::{GlobalBudget, ResourceType};
use crate::fleets::{PropulsionType, ShipClass, ShipInfo, ShipInstance, AU_IN_METERS};
use crate::plugins::solar_system::CelestialBody;
use crate::research::ResearchState;
use crate::ui::SimulationTime;

const SHIPYARD_BP_PER_YEAR: f64 = 2_500.0;
const FACTORY_SUPPORT_BP_PER_YEAR: f64 = 125.0;
const ENGINEERING_BAY_BONUS: f64 = 0.05;

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
        });
        request_ids.push(request_id);
    }

    request_ids
}

pub fn process_pending_shipbuilding_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingShipbuildingActions>,
    colonies: Query<&Colony>,
    mut stockpiles: Query<&mut LocalStockpile>,
    shipbuilding_data: Res<ShipbuildingData>,
    mut design_library: ResMut<ShipDesignLibrary>,
    research_state: Res<ResearchState>,
    mut resource_requests: ResMut<PendingResourceRequests>,
    sim_time: Res<SimulationTime>,
) {
    let now = sim_time.elapsed_seconds();

    for QueueShipConstructionAction { build_site, design } in actions.queue_projects.drain(..) {
        let Ok(colony) = colonies.get(build_site) else {
            warn!(
                "Ignoring shipbuilding action for non-colony entity {:?}",
                build_site
            );
            continue;
        };

        let hull = shipbuilding_data.get_hull(&design.hull_id);

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

        // Create and save a design template for this construction
        let template_id = {
            let template = ShipDesignTemplate {
                id: uuid::Uuid::new_v4(),
                name: design.name.clone(),
                hull_id: design.hull_id.clone(),
                modules: design.modules.clone(),
                version: design_library.latest_version(&design.name) + 1,
                parent_template_id: None,
                created_at_game_time: now,
                construction_mode: design.construction_mode,
            };
            design_library.save_template(template)
        };

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
            let engineering_bays = colony.building_count(BuildingType::EngineeringBay) as f64;
            let bonus = 1.0 + engineering_bays * ENGINEERING_BAY_BONUS;
            let bp = (shipyards * SHIPYARD_BP_PER_YEAR + factories * FACTORY_SUPPORT_BP_PER_YEAR)
                * bonus
                * years_elapsed;
            colony_bp.push((colony_entity, bp));
        }
    }

    for (colony_entity, mut available_bp) in colony_bp {
        let mut project_entities: Vec<Entity> = projects
            .iter()
            .filter(|(_, project)| {
                project.build_site == colony_entity
                    && project.is_building()
                    && !project.awaiting_resources
            })
            .map(|(entity, _)| entity)
            .collect();
        project_entities.sort();

        for project_entity in project_entities {
            if available_bp <= 0.0 {
                break;
            }

            if let Ok((_, mut project)) = projects.get_mut(project_entity) {
                let needed = project.required_build_points - project.progress;
                let applied = needed.min(available_bp);
                project.progress += applied;
                available_bp -= applied;

                if project.progress >= project.required_build_points {
                    project.state = match project.construction_mode {
                        ConstructionMode::SurfaceLaunch => ShipConstructionState::ReadyForLaunch,
                        ConstructionMode::OrbitalAssembly | ConstructionMode::OrbitalShipyard => {
                            ShipConstructionState::CompletedInOrbit
                        }
                    };
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
    mut stockpiles: Query<&mut LocalStockpile>,
    mut budget: ResMut<GlobalBudget>,
    mut launch_state: ResMut<LaunchCapacityState>,
    mut resource_requests: ResMut<PendingResourceRequests>,
    mut projects: Query<(Entity, &mut ShipConstructionProject)>,
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
                    commands.spawn(ShipInstance::new(
                        build_ship_info(&project),
                        project.build_site,
                        insertion_orbit_radius_au(body, project.is_station),
                        project.is_station,
                        None,
                        0,
                    ));
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
                commands.spawn(ShipInstance::new(
                    build_ship_info(&project),
                    project.build_site,
                    insertion_orbit_radius_au(body, project.is_station),
                    project.is_station,
                    None,
                    0,
                ));
                commands.entity(entity).despawn();
            }
        }
    }
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

fn build_ship_info(project: &ShipConstructionProject) -> ShipInfo {
    let propulsion = project.propulsion.unwrap_or(PropulsionType::Chemical);
    let fuel_mass = project.fuel_capacity_t.max(0.0) as f32;
    ShipInfo {
        name: project.design_name.clone(),
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
