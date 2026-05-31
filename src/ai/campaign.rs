//! Campaign AI — high-level strategic decision making.
//!
//! Each AI faction runs campaign decisions on a periodic tick:
//! - Evaluate current goals and resource allocation
//! - Decide where to expand (colonise new bodies)
//! - Decide what to build (buildings, ships)
//! - Decide tech research priorities
//! - Update diplomatic stance

use std::collections::HashMap;

use bevy::prelude::{Entity, Local, Query, Res, ResMut};
use bevy::log::info;
use crate::astronomy::components::SpaceCoordinates;
use crate::colony::{Colony, PendingConstructionActions};
use crate::economy::budget::GlobalBudget;
use crate::economy::components::{LocalStockpile, MineralDeposit, SurveyLevel};
use crate::fleets::components::{Fleet, PendingFleetActions, ShipInfo, SpawnFleetAction};
use crate::plugins::solar_system::CelestialBody;
use crate::plugins::solar_system_data::BodyType;
use crate::research::systems::ResearchState;
use crate::research::PendingResearchActions;
use crate::research::data::TechnologiesData;
use crate::research::types::TechCategory;
use crate::ui::SimulationTime;

use super::components::{
    AIDecisionContext, AIFaction, AIPersonality, AIControlledColony,
    AIControlledFleet, AIFactionResearchState,
};
use crate::colony::types::BuildingType;
use crate::fleets::types::{FleetRole, PropulsionType, ShipClass};

/// AI tick interval in simulation seconds (decide every ~1 game-month).
const AI_TICK_INTERVAL_SECS: f64 = 2_592_000.0;

/// Minimum treasury to maintain for emergencies.
const TREASURY_RESERVE_MC: f64 = 50_000.0;

/// Cost to establish an outpost colony (MC).
const OUTPOST_COST_MC: f64 = 100_000.0;

/// Average ship cost in MC (used for fleet build decisions).
const AVG_SHIP_COST_MC: f64 = 50_000.0;

/// System to run campaign AI decisions for all AI factions.
pub fn run_campaign_ai(
    sim_time: Res<SimulationTime>,
    mut ai_factions: Query<(Entity, &mut AIFaction, &AIFactionResearchState)>,
    colonies: Query<(Entity, &Colony, &AIControlledColony)>,
    all_bodies: Query<(Entity, &CelestialBody, &SpaceCoordinates)>,
    mut global_budget: ResMut<GlobalBudget>,
    research_state: Res<ResearchState>,
    tech_data: Res<TechnologiesData>,
    fleets: Query<(Entity, &Fleet, Option<&AIControlledFleet>)>,
    stockpile_query: Query<&LocalStockpile>,
    deposits_query: Query<&MineralDeposit>,
    survey_query: Query<&SurveyLevel>,
    mut pending_construction: ResMut<PendingConstructionActions>,
    mut pending_fleet: ResMut<PendingFleetActions>,
    mut pending_research: ResMut<PendingResearchActions>,
    mut last_ai_tick: Local<f64>,
) {
    let now = sim_time.elapsed_seconds();

    // Only run AI tick at configured interval (every game-month).
    if now - *last_ai_tick < AI_TICK_INTERVAL_SECS {
        return;
    }
    *last_ai_tick = now;

    // Collect all bodies that could be colonisation targets.
    let mut available_bodies: Vec<Entity> = Vec::new();
    for (entity, _, _) in all_bodies.iter() {
        available_bodies.push(entity);
    }

    // Collect enemy fleets (fleets not controlled by any AI faction).
    let enemy_fleets: Vec<Entity> = Vec::new();

    // Get all faction entities upfront to avoid borrow conflicts with get_mut below.
    let faction_entities: Vec<Entity> = ai_factions.iter().map(|(e, _, _)| e).collect();

    for faction_entity in faction_entities {
        // SAFETY: Each entity is processed once, no aliasing.
        let (_, mut faction, faction_research) = ai_factions.get_mut(faction_entity).unwrap();
        faction.increment_tick();

        let difficulty = faction.difficulty;
        let _personality = faction.personality;
        let prod_mult = difficulty.production_multiplier();
        let agg_mult = difficulty.aggression_multiplier();

        // Collect faction colonies.
        let faction_colonies: Vec<Entity> = colonies
            .iter()
            .filter(|(_, _, ac)| ac.faction_id == faction.faction_id)
            .map(|(colony_entity, _, _)| colony_entity)
            .collect();

        faction.colonies = faction_colonies.clone();

        // Collect faction fleets.
        let faction_fleets: Vec<Entity> = fleets
            .iter()
            .filter(|(_, _, acf)| acf.is_some_and(|af| af.faction_id == faction.faction_id))
            .map(|(fleet_entity, _, _)| fleet_entity)
            .collect();

        faction.fleets = faction_fleets.clone();

        // Compute faction stockpile.
        let mut stockpile: HashMap<String, f64> = HashMap::new();
        for colony_entity in &faction_colonies {
            if let Ok(ls) = stockpile_query.get(*colony_entity) {
                for (resource, amount) in ls.stockpiles.iter() {
                    let key = format!("{:?}", resource);
                    *stockpile.entry(key).or_insert(0.0) += *amount;
                }
            }
        }

        // Build decision context.
        let ctx = AIDecisionContext {
            faction_entity,
            faction: faction.clone(),
            available_bodies: available_bodies.clone(),
            enemy_fleets: enemy_fleets.clone(),
            treasury_mc: global_budget.treasury,
            stockpile,
        };

        // Make strategic decisions.
        decide_expansion(
            &mut faction,
            &ctx,
            prod_mult,
            &all_bodies,
            &colonies,
            &mut pending_construction,
            &mut global_budget,
            &stockpile_query,
            &deposits_query,
            &survey_query,
        );
        decide_fleet_build(
            &mut faction,
            &ctx,
            agg_mult,
            prod_mult,
            &faction_colonies,
            &faction_fleets,
            &mut pending_fleet,
        );
        decide_research(
            &mut faction,
            &ctx,
            &mut pending_research,
            &tech_data,
            &research_state,
            faction_research,
        );
        update_goals(&mut faction, &ctx, prod_mult);

        // Spend resources on highest priority actions.
        spend_resources(
            &mut faction,
            &ctx,
            prod_mult,
            &mut pending_construction,
            &mut global_budget,
        );
    }
}

/// Rate a celestial body as a colonisation candidate for a given faction.
/// Returns a score; higher is better. Considers resources, habitability, and distance.
fn rate_colonisation_candidate(
    body_entity: Entity,
    body: &CelestialBody,
    coords: &SpaceCoordinates,
    _faction_colonies: &[Entity],
    _stockpile_query: &Query<&LocalStockpile>,
    deposits_query: &Query<&MineralDeposit>,
    survey_query: &Query<&SurveyLevel>,
) -> f64 {
    // Skip if already colonised (has a colony component on same entity).
    // For AI, we check if there's a Colony component on this entity.
    // Since we can't easily check that here, we rely on the expansion guard
    // in decide_expansion which checks faction.colonies.len() >= target.

    let mut score = 0.0;

    // Score body type — planets are better than asteroids.
    match body.body_type {
        BodyType::Planet => score += 20.0,
        BodyType::DwarfPlanet => score += 10.0,
        BodyType::Moon => score += 5.0,
        BodyType::Asteroid => score += 2.0,
        BodyType::Comet => score += 1.0,
        BodyType::GasGiant => score += 15.0,
        _ => {}
    }

    // Score based on survey level (more surveyed = more known resources).
    if let Ok(survey) = survey_query.get(body_entity) {
        match survey {
            SurveyLevel::Unsurveyed => score += 0.0,
            SurveyLevel::OrbitalScan => score += 5.0,
            SurveyLevel::SeismicSurvey => score += 10.0,
            SurveyLevel::CoreSample => score += 15.0,
        }
    }

    // Score mineral deposits.
    if let Ok(deposit) = deposits_query.get(body_entity) {
        score += deposit.reserve.proven_crustal * 2.0;
        score += deposit.reserve.deep_deposits * 3.0;
        score += deposit.reserve.planetary_bulk * 5.0;
    }

    // Score size (larger = more industrial potential).
    if body.mass > 0.0 {
        score += (body.mass.log10() - 20.0).max(0.0) * 2.0;
    }

    // Distance penalty: prefer closer bodies (measured from origin for now).
    let dist_au = coords.position.length();
    score -= dist_au * 2.0;

    score
}

/// Decide whether and where to expand (colonise new bodies).
fn decide_expansion(
    faction: &mut AIFaction,
    ctx: &AIDecisionContext,
    prod_mult: f64,
    all_bodies: &Query<(Entity, &CelestialBody, &SpaceCoordinates)>,
    all_colonies: &Query<(Entity, &Colony, &AIControlledColony)>,
    pending_construction: &mut PendingConstructionActions,
    global_budget: &mut GlobalBudget,
    stockpile_query: &Query<&LocalStockpile>,
    deposits_query: &Query<&MineralDeposit>,
    survey_query: &Query<&SurveyLevel>,
) {
    // Check if we have enough colonies for current goals.
    if faction.colonies.len() >= faction.goals.target_colonies as usize {
        return;
    }

    // Check if we can afford an outpost.
    let affordable = ctx.treasury_mc > OUTPOST_COST_MC * (1.0 / prod_mult) + TREASURY_RESERVE_MC;
    if !affordable {
        return;
    }

    // Filter out bodies that are already colonised by this faction.
    let colonised_entities: Vec<Entity> = all_colonies
        .iter()
        .filter(|(_, _, ac)| ac.faction_id == faction.faction_id)
        .map(|(e, _, _)| e)
        .collect();

    // Rate all uncolonised bodies and pick the best candidate.
    let mut best_entity: Option<Entity> = None;
    let mut best_score = f64::MIN;

    for (entity, body, coords) in all_bodies.iter() {
        // Skip already colonised bodies.
        if colonised_entities.contains(&entity) {
            continue;
        }

        let score = rate_colonisation_candidate(
            entity,
            body,
            coords,
            &faction.colonies,
            stockpile_query,
            deposits_query,
            survey_query,
        );

        if score > best_score {
            best_score = score;
            best_entity = Some(entity);
        }
    }

    if let Some(target) = best_entity {
        if best_score > 0.0 {
            // Deduct outpost cost from treasury before queueing.
            let cost = OUTPOST_COST_MC / prod_mult;
            if global_budget.treasury >= cost + TREASURY_RESERVE_MC {
                global_budget.treasury -= cost;
                pending_construction.establish_outpost.push(
                    crate::colony::EstablishOutpostRequest {
                        body_entity: target,
                        colony_name: format!("{} Colony", faction.name),
                        needs_oxygen: true,
                        faction_id: Some(faction.faction_id),
                    },
                );
                info!(
                    "[{}] AI expansion: queuing colony on entity {:?} (score: {:.1}, cost: {:.0} MC)",
                    faction.name,
                    target,
                    best_score,
                    cost
                );
            }
        }
    }
}

/// Decide what fleet units to build.
fn decide_fleet_build(
    faction: &mut AIFaction,
    ctx: &AIDecisionContext,
    agg_mult: f64,
    prod_mult: f64,
    faction_colonies: &[Entity],
    faction_fleets: &[Entity],
    pending_fleet: &mut PendingFleetActions,
) {
    let (_, fleet_priority, _, _) = faction.personality.priorities();

    // Count current ships across all faction fleets (live query result, not stale cache).
    let current_ships = faction_fleets.len() * 3; // rough estimate

    // Target based on personality × difficulty.
    let fleet_base = match faction.personality {
        AIPersonality::Militarist => 15,
        AIPersonality::Economic => 6,
        AIPersonality::Scientific => 4,
        AIPersonality::Balanced => 8,
    };
    let target_ships = ((fleet_base as f64 * fleet_priority * agg_mult) as usize).clamp(3, 50);

    if current_ships >= target_ships {
        return;
    }

    // Check if we can afford ships.
    let build_count = ((target_ships - current_ships) as f64).min(3.0) as usize;
    let total_cost = build_count as f64 * AVG_SHIP_COST_MC;
    if total_cost > ctx.treasury_mc - TREASURY_RESERVE_MC {
        return;
    }

    // Pick a colony to build ships at (first established colony).
    let Some(&home_colony) = faction_colonies.first() else {
        return;
    };

    // Determine ship class based on personality.
    let ship_class = match faction.personality {
        AIPersonality::Militarist => ShipClass::Destroyer,
        AIPersonality::Economic => ShipClass::Frigate,
        AIPersonality::Scientific => ShipClass::Courier,
        AIPersonality::Balanced => ShipClass::Frigate,
    };
    let propulsion = PropulsionType::FusionTorch;

    let ships: Vec<ShipInfo> = (0..build_count)
        .map(|i| {
            ShipInfo::new(
                format!(
                    "{} {}",
                    faction.name.split_whitespace().next().unwrap_or("AI"),
                    faction_fleets.len() * 10 + i + 1
                ),
                ship_class,
                propulsion,
            )
        })
        .collect();

    pending_fleet.spawn_fleets.push(SpawnFleetAction {
        name: format!("{} Fleet {}", faction.name, faction_fleets.len() + 1),
        ships,
        orbit_body: home_colony,
        orbit_radius_au: 0.01,
        faction_id: Some(faction.faction_id),
        initial_role: Some(if faction.personality == AIPersonality::Militarist {
            FleetRole::Attack
        } else {
            FleetRole::Defend
        }),
    });

    // Log the fleet build (cost deducted when fleet actually spawns).
    let fleet_cost = build_count as f64 * AVG_SHIP_COST_MC / prod_mult;
    info!(
        "[{}] AI fleet build: +{} ships (target: {}, current: {}, cost: {:.0} MC)",
        faction.name,
        build_count,
        target_ships,
        current_ships,
        fleet_cost
    );
}

/// Decide research priorities based on personality and game state.
/// Uses per-faction research state so AI and player research are independent.
fn decide_research(
    faction: &mut AIFaction,
    _ctx: &AIDecisionContext,
    pending_research: &mut PendingResearchActions,
    tech_data: &TechnologiesData,
    _research_state: &ResearchState,
    faction_research: &AIFactionResearchState,
) {
    let focus = faction.personality.preferred_tech_categories();

    let research_focus = if faction.goals.at_war {
        let mut shifted = vec!["Military".to_string()];
        shifted.extend(focus.clone());
        shifted
    } else {
        focus.clone()
    };

    let changed = research_focus != faction.research_focus;
    if changed {
        info!("[{}] AI research focus: {:?}", faction.name, research_focus);
        faction.research_focus = research_focus.clone();
    }

    // Map string category names to TechCategory enum values.
    let focus_categories: Vec<TechCategory> = research_focus
        .iter()
        .filter_map(|name| match name.as_str() {
            "Military" => Some(TechCategory::Military),
            "Propulsion" => Some(TechCategory::Propulsion),
            "Weapons" => Some(TechCategory::Weapons),
            "Defense" | "DefensiveSystems" => Some(TechCategory::DefensiveSystems),
            "Mining" | "Industry" => Some(TechCategory::Industry),
            "Colony" | "Construction" => Some(TechCategory::Construction),
            "Trade" | "Sociology" => Some(TechCategory::Sociology),
            "Engineering" | "Electronics" => Some(TechCategory::Electronics),
            "Physics" => Some(TechCategory::Physics),
            "Biology" => Some(TechCategory::Biology),
            "Energy" => Some(TechCategory::Energy),
            "Sensors" => Some(TechCategory::Sensors),
            "Materials" => Some(TechCategory::Materials),
            "SpaceTechnology" | "Space" => Some(TechCategory::SpaceTechnology),
            "LifeSupport" => Some(TechCategory::LifeSupport),
            _ => None,
        })
        .collect();

    // Collect candidates: not yet unlocked by this faction, in preferred categories.
    let mut candidates: Vec<(&String, &crate::research::types::Technology)> = Vec::new();
    for (tech_id, tech) in tech_data.technologies.iter() {
        if !faction_research.is_unlocked(tech_id)
            && focus_categories.contains(&tech.category) {
                // Skip if already active or queued
                if !faction_research.is_queued(tech_id) {
                    candidates.push((tech_id, tech));
                }
            }
    }

    // Sort by tier descending (prefer higher-tier techs).
    candidates.sort_by_key(|b| std::cmp::Reverse(b.1.tier));

    // Enqueue top candidates into the per-faction research queue (up to 2 per tick).
    let max_enqueue = 2_usize.min(candidates.len());
    for (tech_id, _) in candidates.into_iter().take(max_enqueue) {
        if !faction_research.is_queued(tech_id) {
            info!("[{}] AI enqueuing research: {}", faction.name, tech_id);
            pending_research.start_research.push(tech_id.clone());
        }
    }
}

/// Update strategic goals based on current state.
fn update_goals(faction: &mut AIFaction, ctx: &AIDecisionContext, prod_mult: f64) {
    let (col_priority, fleet_priority, _, _) = faction.personality.priorities();

    let base_target = match faction.personality {
        AIPersonality::Militarist => 4,
        AIPersonality::Economic => 6,
        AIPersonality::Scientific => 3,
        AIPersonality::Balanced => 5,
    };

    let wealth_factor = (ctx.treasury_mc / 500_000.0).min(2.0);
    faction.goals.target_colonies =
        ((base_target as f64 * col_priority * wealth_factor) as u32).clamp(2, 15);

    let fleet_base = 10;
    faction.goals.target_fleet_size =
        ((fleet_base as f64 * fleet_priority * prod_mult) as u32).clamp(5, 50);
}

/// Spend resources on high-priority actions (buildings, ships).
fn spend_resources(
    faction: &mut AIFaction,
    _ctx: &AIDecisionContext,
    prod_mult: f64,
    pending_construction: &mut PendingConstructionActions,
    global_budget: &mut GlobalBudget,
) {
    // Base building cost — adjust for difficulty.
    let base_cost = 100_000.0 / prod_mult;

    // Build mining facilities when Economic or Balanced personality.
    if faction.personality == AIPersonality::Economic
        || faction.personality == AIPersonality::Balanced
    {
        for &colony_entity in &faction.colonies {
            let cost = base_cost * 1.0; // MiningHub is cheap.
            if global_budget.treasury >= cost + TREASURY_RESERVE_MC {
                pending_construction
                    .start_construction
                    .push((colony_entity, BuildingType::Mine));
                global_budget.treasury -= cost;
                info!(
                    "[{}] AI spending {} MC on MiningHub at colony {:?}",
                    faction.name,
                    cost,
                    colony_entity
                );
            }
        }
    }

    // Build military buildings when Militarist personality.
    if faction.personality == AIPersonality::Militarist {
        for &colony_entity in &faction.colonies {
            let cost = base_cost * 2.5; // GroundDefenseBattery costs more.
            if global_budget.treasury >= cost + TREASURY_RESERVE_MC {
                pending_construction
                    .start_construction
                    .push((colony_entity, BuildingType::GroundDefenseBattery));
                global_budget.treasury -= cost;
                info!(
                    "[{}] AI spending {} MC on GroundDefenseBattery at colony {:?}",
                    faction.name,
                    cost,
                    colony_entity
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_tick_interval_logic() {
        let mut last_tick = 0.0;
        let tick_interval = AI_TICK_INTERVAL_SECS;

        for i in 0..5 {
            let now = i as f64 * tick_interval;
            if now - last_tick >= tick_interval {
                last_tick = now;
            }
        }
        assert!(last_tick >= 4.0 * tick_interval);
    }
}
