//! StateStore extract (GRA-358 PR-I).
//!
//! Walks the live `bevy::ecs::world::World` and produces a
//! [`StateStore`] containing only the divergences from the
//! seed-derived regen — the regen chain is the source of truth
//! for celestial bodies, so we only persist what the player
//! has actually changed.
//!
//! Per-component extractors (one per `StateStore` field) live
//! at the bottom of this file as private `fn`. The public
//!
//! LGD note: the extract helpers default-construct a record and
//! then patch individual fields across multiple resource
//! lookups. The struct-literal alternative would have to repeat
//! every other field on every branch, which is harder to audit
//! than the targeted patches — so we suppress the lint at the
//! module level instead of rewriting every call site.
#![allow(clippy::field_reassign_with_default)]

//!
//! entry point [`extract_state_store`] runs them in order
//! and returns a [`StateStore`].

use bevy::prelude::*;
use serde_json::Value as Json;
use std::collections::BTreeMap;

use super::state_store::{
    AutosaveRecord, BodyDivergence, BodyKey, EconomyRecord, EngineeringProjectRecord, FleetRecord,
    NotificationCategoryRecord, NotificationRecord, ResearchRecord, ResourceRequestRecord,
    ShipRecord, ShippingCompanyRecord, StateStore, StateStoreMetadata, SurveyDivergence, UiRecord,
};

/// Errors during extraction. Only the ones the apply path
/// surfaces to the player; anything else (missing optional
/// resource, default state) silently becomes a `None` in the
/// StateStore.
#[derive(Debug)]
pub enum ExtractError {
    /// The body the player is anchored to is missing the
    /// `CelestialBody` component. Should never happen at
    /// runtime; surfaced as a warning to the campaign log.
    AnchorBodyMissing,
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::AnchorBodyMissing => {
                write!(f, "anchor body is missing CelestialBody")
            }
        }
    }
}

impl std::error::Error for ExtractError {}

/// Public entry point. Walks the world once, populates the
/// `StateStore`, and returns it.
///
/// The `seed` and `start_timestamp` arguments come from the
/// new-game flow (the save panel / autosave timer passes
/// whatever was active when the save was triggered). The
/// `sim_now_seconds` and `playtime_s` are read from the live
/// `SimulationTime` / `PlaytimeTracker` resources.
///
/// Takes `&mut World` because Bevy 0.18's `World::query` /
/// `World::entity_mut` / `World::get_entity_mut` APIs all
/// require mutable access to the world. The extract path
/// doesn't mutate anything; we just need the borrow checker
/// to be satisfied.
pub fn extract_state_store(
    world: &mut World,
    seed: u64,
    start_timestamp: i64,
) -> Result<StateStore, ExtractError> {
    let mut store = StateStore {
        metadata: extract_metadata(world, seed, start_timestamp),
        ..Default::default()
    };
    store.bodies = extract_bodies(world);
    store.surveys = extract_surveys(world);
    store.fleets = extract_fleets(world);
    store.research = extract_research(world);
    store.economy = extract_economy(world);
    store.ui = extract_ui(world);
    store.notifications = extract_notifications(world);
    store.meta_autosave = extract_autosave(world);
    Ok(store)
}

// ════════════════════════════════════════════════════════════
// Metadata
// ════════════════════════════════════════════════════════════

fn extract_metadata(world: &mut World, seed: u64, start_timestamp: i64) -> StateStoreMetadata {
    let saved_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (sim_now_seconds, playtime_s) = {
        // We *try* to read `SimulationTime` and `PlaytimeTracker`
        // if they exist; a save written from a state where
        // simulation hasn't started (e.g. a debug pre-init save)
        // just gets zeros.
        let mut sim = 0.0;
        let mut play = 0.0;
        if let Some(time) = world.get_resource::<super::super::ui::time::SimulationTime>() {
            sim = time.elapsed_seconds();
            play = sim; // playtime is approximated by sim time
        }
        (sim, play)
    };
    StateStoreMetadata {
        format_version: super::format_version::FORMAT_VERSION_V2,
        helios_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at_unix_s: saved_at_unix,
        playtime_s,
        seed,
        start_timestamp,
        sim_now_seconds,
        preview: super::state_store::SavePreview::default(),
    }
}

// ════════════════════════════════════════════════════════════
// Per-body divergences
// ════════════════════════════════════════════════════════════

fn extract_bodies(world: &mut World) -> BTreeMap<BodyKey, BodyDivergence> {
    use crate::astronomy::components::SystemId;
    use crate::colony::components::Colony;
    use crate::economy::components::{
        DirtyBodies, DirtyReason, LocalStockpile, PlanetResources, Population,
    };
    use crate::plugins::solar_system::CelestialBody;

    let mut out: BTreeMap<BodyKey, BodyDivergence> = BTreeMap::new();

    // Snapshot the dirty-bodies map once, up front, so the
    // inner loop doesn't need a `Res<DirtyBodies>` borrow
    // (which would conflict with the `query::<>` mutable
    // borrow on `world`). The map is
    // `HashMap<Entity, DirtyReason>` — lookup is O(1).
    let dirty: std::collections::HashMap<bevy::prelude::Entity, DirtyReason> = world
        .get_resource::<DirtyBodies>()
        .map(|d| d.bodies.clone())
        .unwrap_or_default();

    // Walk every body entity. We only persist divergences
    // — the regen chain sets the rest. Process inline to
    // avoid lifetime gymnastics over the `q.iter()` borrow.
    let mut q = world.query::<(
        Entity,
        &CelestialBody,
        &SystemId,
        Option<&Colony>,
        Option<&Population>,
        Option<&PlanetResources>,
        Option<&LocalStockpile>,
    )>();
    for (entity, body, system, colony, pop, res, stock) in q.iter(world) {
        // Skip bodies whose state is the regen default — we
        // only persist *divergences*. The regen chain seeds
        // every body with a `Population { count: 0.0 }` and
        // every asteroid with spectral-class `PlanetResources`,
        // so the gating has to check the *actual values*, not
        // just the component presence.
        //
        // v0.4.0 (PR-I) plays safe: we persist a body only if
        //   (a) it has a `Colony` (a real player-founded
        //       colony, not the Earth baseline which the
        //       regen chain re-seeds), OR
        //   (b) it has a non-zero population, OR
        //   (c) it has a non-empty `LocalStockpile` (the
        //       player's freighter deliveries, mining
        //       outputs, or build queues live here), OR
        //   (d) it appears in `DirtyBodies` — the player
        //       has mined / delivered / consumed /
        //       terraformed / shifted orbit / mass-Changed
        //       on this body since the last save, even if
        //       the net state has reverted to the regen
        //       default.
        //
        // We deliberately do NOT persist `PlanetResources`:
        // the regen chain re-seeds every body's deposits
        // deterministically from the seed, so there is no
        // "player override" to save. Persisting them anyway
        // bloats the save by 70 MB for a 700-body universe
        // (the asteroid spectral-class tables are large).
        // Per-body resource overrides land in a future PR
        // once the regen chain grows a `ResourceOverride`
        // hook.
        let has_colony = colony.is_some();
        let has_pop = pop.map(|p| p.count > 0.0).unwrap_or(false);
        let has_stockpile = stock.map(|s| !s.stockpiles.is_empty()).unwrap_or(false);
        let dirty_reason = dirty.get(&entity).copied();
        let is_dirty = dirty_reason.is_some();

        if !(has_colony || has_pop || has_stockpile || is_dirty) {
            continue;
        }

        let key = BodyKey::new(*system, body.name.clone());
        let div = out.entry(key).or_default();

        // ── Component-level divergence writes ──────────────────
        //
        // Each `DirtyReason` variant drives which fields
        // we populate. When the body is dirty, we
        // populate every applicable field. The "always
        // extracted" branches (colony, population,
        // stockpile) fire regardless of dirty status —
        // they're triggered by *component presence*
        // (has_colony / has_pop / has_stockpile), not by
        // the dirty tracker.
        //
        // The "dirty-only" branches (atmosphere, orbit,
        // body) only fire when the body is in the dirty
        // set with a matching reason. Without the dirty
        // marker, the regen chain re-derives these
        // components on the next run — that's the
        // correct behaviour (we don't want to persist
        // regen-default orbital elements for every
        // body).
        let reason = dirty_reason; // None if not dirty

        // Colony divergence: any reason that mutates a
        // player-founded colony (or that flags a body
        // as freshly-founded). The colony itself is
        // always extracted when present — regen-chain
        // reasons (Orbit, Body) don't carry colony
        // state.
        if has_colony {
            if let Some(c) = colony {
                if let Ok(v) = serde_json::to_value(c) {
                    div.colony_override = Some(v);
                }
            }
        }

        // Population divergence: population > 0 on any
        // body is a divergence (regen chain seeds
        // everyone with 0.0; Earth gets 8.2B only
        // because the regen chain explicitly sets it).
        // We always extract when has_pop is true.
        if has_pop {
            if let Some(p) = pop.filter(|p| p.count > 0.0) {
                if let Ok(v) = serde_json::to_value(p) {
                    div.population_override = Some(v);
                }
            }
        }

        // Stockpile divergence: any mutation to
        // `LocalStockpile` (mining, freighter delivery,
        // build-queue consumption, maintenance drain,
        // life-support drain). We always extract when
        // has_stockpile is true; the dirty marker
        // additionally lets us persist an *empty*
        // stockpile for a body the player has touched
        // and then drained (see the
        // `state_store_v2_dirty_resource_bodies_roundtrip`
        // test).
        if has_stockpile
            || matches!(
                reason,
                Some(DirtyReason::Stockpile | DirtyReason::Colony | DirtyReason::Multiple)
            )
        {
            if let Some(s) = stock {
                if let Ok(v) = serde_json::to_value(s) {
                    let mut obj = serde_json::Map::new();
                    obj.insert("stockpile".to_string(), v);
                    div.resources_override = Some(Json::Object(obj));
                }
            }
        }

        // Atmosphere divergence: terraforming changes.
        // The extract path doesn't currently serialise
        // `AtmosphereComposition` (it lacks the
        // `Serialize` derive — see PR-I follow-up).
        // When the player has explicitly terraformed a
        // body, we still emit a sentinel flag so the
        // apply path can warn.
        if matches!(
            reason,
            Some(DirtyReason::Atmosphere | DirtyReason::Multiple)
        ) {
            // TODO(pr-terraform): serialise the
            // `AtmosphereComposition` component into
            // `atmosphere_override` once it derives
            // `Serialize`. For now we surface a warning
            // in `ApplyOutcome.warnings` (handled by the
            // apply path).
            div.atmosphere_override = Some(Json::Object(serde_json::Map::new()));
        }

        // Orbit divergence: orbit-shift mechanics
        // (asteroid redirect, tractor tug). The regen
        // chain re-derives `KeplerOrbit` from the
        // save's `sim_now_seconds` for every body, so
        // the override fields are only honoured when
        // the player has nudged the orbit. We populate
        // the override fields from the live
        // `KeplerOrbit` component.
        if matches!(reason, Some(DirtyReason::Orbit | DirtyReason::Multiple)) {
            if let Ok(em) = world.get_entity(entity) {
                if let Some(orbit) = em.get::<crate::astronomy::components::KeplerOrbit>() {
                    div.mean_anomaly_epoch_override = Some(orbit.mean_anomaly_epoch);
                    div.semi_major_axis_override = Some(orbit.semi_major_axis);
                    div.eccentricity_override = Some(orbit.eccentricity);
                }
            }
        }

        // Body divergence: mass / radius / rotation
        // changes (e.g. asteroid mining depleted
        // enough mass to matter). PR-I doesn't
        // serialise the `CelestialBody` component (it
        // lacks the `Serialize` derive), so this branch
        // is a no-op — the apply path's warning system
        // flags skipped mutations.
        if matches!(reason, Some(DirtyReason::Body | DirtyReason::Multiple)) {
            // TODO(pr-body-mass): derive Serialize on
            // `CelestialBody` and emit a JSON blob into
            // `body_override` (a new field on
            // `BodyDivergence`). Until then the extract
            // path leaves the live CelestialBody alone
            // and the regen chain re-derives mass /
            // radius from the seed on the next run.
        }

        // Suppress unused-variable lint for `res` so the
        // query tuple still compiles.
        let _ = res;
    }
    out
}

// ════════════════════════════════════════════════════════════
// Per-body surveys
// ════════════════════════════════════════════════════════════

fn extract_surveys(world: &mut World) -> BTreeMap<BodyKey, SurveyDivergence> {
    use crate::astronomy::components::SystemId;
    use crate::plugins::solar_system::CelestialBody;
    use crate::survey::components::SurveyState;

    let mut out: BTreeMap<BodyKey, SurveyDivergence> = BTreeMap::new();
    let mut q = world.query::<(&CelestialBody, &SystemId, &SurveyState)>();
    for (body, system, survey) in q.iter(world) {
        // Skip the regen-default state. The regen chain
        // seeds every body with an unsurveyed `SurveyState`;
        // only bodies the player has actually started to
        // survey need to round-trip.
        if survey.dimensions.is_empty()
            && survey.drill_missions_completed == 0
            && survey.detected_anomalies.is_empty()
        {
            continue;
        }
        let key = BodyKey::new(*system, body.name.clone());
        // We persist the whole `SurveyState` as a JSON blob
        // so any future field the team adds round-trips for
        // free. The apply path rebuilds a `SurveyState` from
        // the JSON, then inserts it as a component.
        let state_json = match serde_json::to_value(survey) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let dim_tiers: Vec<(String, u32)> = survey
            .dimensions
            .iter()
            .map(|(dim, fid)| (format!("{:?}", dim), fid.tier as u32))
            .collect();
        let anomalies: Vec<String> = survey
            .detected_anomalies
            .iter()
            .map(|a| format!("{:?}", a))
            .collect();
        out.insert(
            key,
            SurveyDivergence {
                dimension_tiers: dim_tiers,
                drill_missions_completed: survey.drill_missions_completed,
                anomalies,
                last_surveyed_sim_seconds: survey.last_updated_sim_time,
                state_json: Some(state_json),
            },
        );
    }
    out
}

// ════════════════════════════════════════════════════════════
// Fleets
// ════════════════════════════════════════════════════════════

fn extract_fleets(world: &mut World) -> Vec<FleetRecord> {
    use crate::astronomy::components::SystemId;
    use crate::fleets::components::{Fleet, FleetOrbit};
    use crate::fleets::RegenChainFleet;
    use crate::plugins::solar_system::CelestialBody;

    let mut out = Vec::new();
    // One query for both `Fleet` and `FleetOrbit` so we can
    // resolve the anchor body in the same iteration. Doing
    // this in two queries (Fleet, then FleetOrbit) would
    // conflict on the mutable world borrow.
    //
    // We filter on `Without<RegenChainFleet>`: the Day-One
    // Constellation, the Mars Flyby Probe, and the debug
    // Earth→Jupiter fleet are all spawned by the regen chain
    // on every fresh world, so persisting them would just
    // duplicate them on the next load. The marker is added
    // by `spawn_regen_chain_fleet` in `fleets/systems.rs`.
    let mut q = world.query::<(&Fleet, Option<&FleetOrbit>, Option<&RegenChainFleet>)>();
    for (fleet, orbit_opt, regen_marker) in q.iter(world) {
        // Skip regen-chain-spawned fleets — the regen
        // chain will re-emit them on the next run. Only
        // player-built / player-modified fleets reach the
        // save.
        if regen_marker.is_some() {
            continue;
        }
        let mut ships = Vec::new();
        for (i, ship) in fleet.ships.iter().enumerate() {
            let class_name = serde_json::to_value(ship.class)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", ship.class));
            ships.push(ShipRecord {
                key: i as u32,
                class: class_name,
                name: Some(ship.name.clone()),
                modules: Vec::new(), // TODO: loadout extraction
                hull: ship.hull_id.clone().unwrap_or_default(),
                built_sim_seconds: 0.0,
                assigned_scientists: Vec::new(),
                fuel_fraction: ship.fuel_fraction(),
                hull_integrity: 1.0,
            });
        }
        // Anchor: try FleetOrbit → body → CelestialBody.
        let at_anchor = orbit_opt.and_then(|orbit| {
            world.get_entity(orbit.body).ok().and_then(|er| {
                let body = er.get::<CelestialBody>()?;
                let sys = *er.get::<SystemId>()?;
                Some(BodyKey::new(sys, body.name.clone()))
            })
        });
        out.push(FleetRecord {
            name: fleet.name.clone(),
            at_anchor,
            pending_manoeuvre: None, // TODO: ActiveManeuver extraction
            ships,
            logistics_policy: None,
        });
    }
    out
}

// ════════════════════════════════════════════════════════════
// Research
// ════════════════════════════════════════════════════════════

fn extract_research(world: &mut World) -> ResearchRecord {
    use crate::research::components::{EngineeringProject, ResearchProject, ResearchTeamCapacity};
    use crate::research::systems::ResearchState;

    let mut out = ResearchRecord::default();
    if let Some(state) = world.get_resource::<ResearchState>() {
        out.unlocked = state.unlocked_technologies.iter().cloned().collect();
    }
    {
        // Active research projects: walk entities. Combine
        // both query types into a single iteration so we
        // don't conflict on the mutable world borrow.
        let mut q = world.query::<(Option<&ResearchProject>, Option<&EngineeringProject>)>();
        for (rp_opt, ep_opt) in q.iter(world) {
            if let Some(proj) = rp_opt {
                out.projects.push(EngineeringProjectRecord {
                    id: proj.tech_id.clone(),
                    progress: proj.progress,
                    total: proj.required_points,
                    paused: !proj.active,
                });
            }
            if let Some(proj) = ep_opt {
                out.projects.push(EngineeringProjectRecord {
                    id: proj.component_id.clone(),
                    progress: proj.progress,
                    total: proj.required_points,
                    paused: false,
                });
            }
        }
    }
    if let Some(cap) = world.get_resource::<ResearchTeamCapacity>() {
        out.team_capacity
            .insert("research".to_string(), cap.max_research_teams as u32);
        out.team_capacity
            .insert("engineering".to_string(), cap.max_engineering_teams as u32);
    }
    // RP/EP per category — the regen chain doesn't compute
    // these from a seed, they're player-derived, so persist.
    if let Some(state) = world.get_resource::<ResearchState>() {
        out.rp_balance
            .insert("available".to_string(), state.research_points_available);
        out.ep_balance
            .insert("available".to_string(), state.engineering_points_available);
    }
    out
}

// ════════════════════════════════════════════════════════════
// Economy
// ════════════════════════════════════════════════════════════

fn extract_economy(world: &mut World) -> EconomyRecord {
    use crate::astronomy::components::SystemId;
    use crate::economy::budget::{GlobalBudget, ResourceRateTracker};
    use crate::economy::company::ShippingCompanies;
    use crate::economy::logistics::PendingResourceRequests;

    let mut out = EconomyRecord::default();
    if let Some(budget) = world.get_resource::<GlobalBudget>() {
        out.treasury = budget.treasury;
    }
    if let Some(rates) = world.get_resource::<ResourceRateTracker>() {
        // ResourceRateTracker tracks per-resource production and
        // consumption separately; we collapse them into a single
        // (production, consumption) tuple per resource for the
        // save.
        for (res, prod) in rates.gross_production_rates.iter() {
            let key = serde_json::to_value(res)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", res));
            let cons = rates
                .gross_consumption_rates
                .get(res)
                .copied()
                .unwrap_or(0.0);
            out.rates.insert(key, (*prod, cons));
        }
    }
    if let Some(companies) = world.get_resource::<ShippingCompanies>() {
        for (i, c) in companies.companies.iter().enumerate() {
            out.shipping_companies.push(ShippingCompanyRecord {
                id: i as u32,
                name: c.name.clone(),
                fleet_anchor: None, // TODO: anchor extraction
                credit_balance: c.treasury_mc,
            });
        }
    }
    if let Some(reqs) = world.get_resource::<PendingResourceRequests>() {
        for r in &reqs.requests {
            // Look up the destination body key from the live
            // entity. If it's gone, fall back to a stub Earth key.
            let destination = world
                .get_entity(r.destination_body)
                .ok()
                .and_then(|er| {
                    let body = er.get::<crate::plugins::solar_system::CelestialBody>()?;
                    let sys = *er.get::<SystemId>()?;
                    Some(BodyKey::new(sys, body.name.clone()))
                })
                .unwrap_or_else(|| BodyKey::sol("Earth"));
            out.pending_requests.push(ResourceRequestRecord {
                resource: serde_json::to_value(r.resource)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                destination,
                amount_megatonnes: r.amount_mt,
                priority: format!("{:?}", r.priority),
                state: format!("{:?}", r.state),
            });
        }
    }
    out
}

// ════════════════════════════════════════════════════════════
// UI / camera / time / state
// ════════════════════════════════════════════════════════════

fn extract_ui(world: &mut World) -> UiRecord {
    use crate::plugins::atmosphere::AtmosphereSettings;
    use crate::plugins::camera::ViewMode;
    use crate::ui::launch::LaunchState;
    use crate::ui::time::TimeScale;

    let mut out = UiRecord::default();
    if let Some(view) = world.get_resource::<ViewMode>() {
        out.view_mode = format!("{:?}", view);
    }
    if let Some(star) = world.get_resource::<crate::astronomy::components::CurrentStarSystem>() {
        out.current_star_system = star.0 as u32;
    }
    if let Some(r) = world.get_resource::<crate::plugins::camera::SavedSurveyRadius>() {
        out.survey_radius_au = r.0.map(|x| x as f64).unwrap_or(0.0);
    }
    if let Some(atm) = world.get_resource::<AtmosphereSettings>() {
        out.atmosphere_enabled = atm.enabled;
        out.atmosphere_quality = match atm.quality {
            0 => "Low".to_string(),
            1 => "Medium".to_string(),
            2 => "High".to_string(),
            _ => format!("Quality{}", atm.quality),
        };
    }
    if let Some(ts) = world.get_resource::<TimeScale>() {
        out.time_scale = ts.scale as f64;
        out.paused = ts.scale == 0.0;
    }
    if let Some(state) = world.get_resource::<LaunchState>() {
        out.launch_state = format!("{:?}", state);
    } else {
        out.launch_state = "InGame".to_string();
    }
    out
}

// ════════════════════════════════════════════════════════════
// Notifications
// ════════════════════════════════════════════════════════════

fn extract_notifications(world: &mut World) -> NotificationRecord {
    use crate::ui::notifications::settings::NotificationSettings;

    let mut out = NotificationRecord::default();
    if let Some(settings) = world.get_resource::<NotificationSettings>() {
        for (cat, cfg) in &settings.per_category {
            out.category_settings.insert(
                cat.0.clone(),
                NotificationCategoryRecord {
                    enabled: cfg.enabled,
                    pause_on_event: cfg.pause_on_event,
                    sound: cfg.sound_on,
                },
            );
        }
    }
    out
}

// ════════════════════════════════════════════════════════════
// Autosave timer / current slot
// ════════════════════════════════════════════════════════════

fn extract_autosave(world: &mut World) -> AutosaveRecord {
    use crate::persistence::autosave::AutosaveTimer;

    let mut out = AutosaveRecord::default();
    if let Some(t) = world.get_resource::<AutosaveTimer>() {
        out.autosave_interval_sim_seconds = t.interval_s;
        // The runtime `AutosaveTimer` doesn't track the last-
        // save sim-time, the last-save wall-clock, or a player
        // slot name. Those will land in PR-J (autosave UX
        // expansion). The extract path leaves them at their
        // defaults.
    }
    out
}

// Suppress unused-warning for types only referenced through
// StateStore JSON round-trips.
#[allow(dead_code)]
const _TOUCH: () = {
    let _ = std::mem::size_of::<EngineeringProjectRecord>();
    let _ = std::mem::size_of::<ResourceRequestRecord>();
};

// ════════════════════════════════════════════════════════════
// Unit tests
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astronomy::components::SystemId;
    use crate::colony::components::{Colony, ColonyDevelopment, ColonyTier};
    use crate::economy::components::{LocalStockpile, Population};
    use crate::plugins::atmosphere::AtmosphereSettings;
    use crate::plugins::camera::ViewMode;
    use crate::plugins::solar_system::CelestialBody;
    use crate::plugins::solar_system_data::BodyType;
    use crate::research::systems::ResearchState;
    use crate::ui::launch::LaunchState;
    use crate::ui::time::TimeScale;
    use std::collections::HashMap;

    fn make_body(world: &mut World, name: &str, sys: u32, with_colony: bool) -> Entity {
        // `CelestialBody` has a long list of fields we don't
        // need for the extract tests; just give it the
        // minimum that the body/identity queries require.
        let body = CelestialBody {
            name: name.to_string(),
            radius: 0.0,
            mass: 0.0,
            body_type: BodyType::Planet,
            visual_radius: 0.0,
            asteroid_class: None,
            star_approach_au: None,
            rotation_period_s: None,
            habitable_outer_au: None,
        };
        let mut e = world.spawn((body, SystemId(sys as usize)));
        if with_colony {
            e.insert(Colony {
                name: name.to_string(),
                population: 8.2e9,
                development: ColonyDevelopment {
                    tier: ColonyTier::Civilisation,
                    yield_multiplier: 1.0,
                    investments: 0,
                },
                buildings: HashMap::new(),
                growth_rate_modifier: 1.0,
            });
            e.insert(Population { count: 8.2e9 });
            e.insert(LocalStockpile {
                stockpiles: HashMap::new(),
            });
        }
        e.id()
    }

    #[test]
    fn extract_empty_world_round_trip() {
        let mut world = World::new();
        world.insert_resource(LaunchState::InGame);
        world.insert_resource(AtmosphereSettings::default());
        world.insert_resource(ViewMode::System);
        world.insert_resource(TimeScale::default());
        let store = extract_state_store(&mut world, 0xABCD, 0).expect("extract");
        assert_eq!(store.metadata.seed, 0xABCD);
        assert_eq!(store.bodies.len(), 0);
        assert_eq!(store.fleets.len(), 0);
        assert_eq!(store.ui.launch_state, "InGame");
    }

    #[test]
    fn extract_picks_up_colony_overrides() {
        let mut world = World::new();
        let _earth = make_body(&mut world, "Earth", 0, true);
        let _mars = make_body(&mut world, "Mars", 0, false);
        let store = extract_state_store(&mut world, 42, 0).expect("extract");
        assert_eq!(store.bodies.len(), 1);
        let earth_div = store
            .bodies
            .get(&BodyKey::sol("Earth"))
            .expect("Earth must be in divergences");
        let colony = earth_div
            .colony_override
            .as_ref()
            .expect("colony_override must be set");
        assert_eq!(colony["name"], "Earth");
        assert_eq!(colony["population"], 8.2e9);
        let pop = earth_div
            .population_override
            .as_ref()
            .expect("population_override must be set");
        assert_eq!(pop["count"], 8.2e9);
    }

    #[test]
    fn extract_omits_default_bodies() {
        // A body with no Colony / no Population / no Resources
        // / no Stockpile should NOT appear in the divergences.
        let mut world = World::new();
        let _earth = make_body(&mut world, "Earth", 0, false);
        let store = extract_state_store(&mut world, 0, 0).expect("extract");
        assert!(
            store.bodies.is_empty(),
            "no divergences expected; got: {:?}",
            store.bodies.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn extract_metadata_uses_current_unix_time() {
        let mut world = World::new();
        world.insert_resource(LaunchState::InGame);
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let store = extract_state_store(&mut world, 0, 0).expect("extract");
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        assert!(store.metadata.saved_at_unix_s >= before);
        assert!(store.metadata.saved_at_unix_s <= after);
        assert_eq!(store.metadata.seed, 0);
    }

    #[test]
    fn extract_research_picks_up_unlocked_set() {
        let mut world = World::new();
        let mut state = ResearchState::default();
        state
            .unlocked_technologies
            .insert("solar_power".to_string());
        state
            .unlocked_technologies
            .insert("chemical_spaceframes".to_string());
        state.research_points_available = 1234.5;
        world.insert_resource(state);
        let store = extract_state_store(&mut world, 0, 0).expect("extract");
        assert_eq!(store.research.unlocked.len(), 2);
        assert!(store.research.unlocked.contains(&"solar_power".to_string()));
        assert!(store
            .research
            .unlocked
            .contains(&"chemical_spaceframes".to_string()));
        assert_eq!(store.research.rp_balance.get("available"), Some(&1234.5));
    }

    #[test]
    fn extract_resources_picks_up_stockpile() {
        let mut world = World::new();
        let body = CelestialBody {
            name: "Luna".to_string(),
            radius: 0.0,
            mass: 0.0,
            body_type: BodyType::Moon,
            visual_radius: 0.0,
            asteroid_class: None,
            star_approach_au: None,
            rotation_period_s: None,
            habitable_outer_au: None,
        };
        let mut e = world.spawn((body, SystemId(0usize)));
        let mut stock = LocalStockpile {
            stockpiles: HashMap::new(),
        };
        stock
            .stockpiles
            .insert(crate::economy::types::ResourceType::Iron, 12.5);
        e.insert(stock);
        let store = extract_state_store(&mut world, 0, 0).expect("extract");
        assert!(store.bodies.contains_key(&BodyKey::sol("Luna")));
    }

    #[test]
    fn extract_atmosphere_settings_preserves_quality_preset() {
        let mut world = World::new();
        let atm = AtmosphereSettings {
            quality: 2,
            enabled: false,
            ..AtmosphereSettings::default()
        };
        world.insert_resource(atm);
        let store = extract_state_store(&mut world, 0, 0).expect("extract");
        assert!(!store.ui.atmosphere_enabled);
        assert_eq!(store.ui.atmosphere_quality, "High");
    }

    #[test]
    fn extract_paused_time_scale_yields_paused_flag() {
        let mut world = World::new();
        let mut ts = TimeScale::default();
        ts.pause();
        world.insert_resource(ts);
        let store = extract_state_store(&mut world, 0, 0).expect("extract");
        assert!(store.ui.paused);
        assert_eq!(store.ui.time_scale, 0.0);
    }
}
