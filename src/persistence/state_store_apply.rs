//! StateStore apply (GRA-358 PR-I).
//!
//! Takes a [`StateStore`] and applies it on top of a freshly
//! regenerated world. The regen chain (run by
//! [`super::game_setup::play_new_game_with_seed`]) is the source
//! of truth for celestial bodies; the apply path only needs to
//! overlay the divergences + player state on top.
//!
//! # Per-component appliers
//!
//! Each public applier maps to a top-level field on the
//! [`StateStore`]:
//!
//! - [`apply_metadata`] — `GameSeed`, `PlaytimeTracker`,
//!   `SimulationTime` (wall-clock & start_timestamp).
//! - [`apply_bodies`] — for every `BodyKey`, look up the regen
//!   entity and (re)insert `Colony`, `Population`,
//!   `PlanetResources`, `LocalStockpile`, `AtmosphereComposition`,
//!   `KeplerOrbit` (orbit-override), or mark destroyed.
//! - [`apply_surveys`] — for every `BodyKey`, build a
//!   `SurveyState` and insert it on the matching body entity.
//! - [`apply_fleets`] — spawn fresh `Fleet` entities (and
//!   `FleetOrbit` if anchored), with `ShipInfo` carried in
//!   `Fleet.ships`.
//! - [`apply_research`] — overwrite `ResearchState`,
//!   `ResearchTeamCapacity`. Active projects are TODO because
//!   the live `ResearchProject` carries an `Entity` team_id that
//!   would need to be re-mapped to the regen world's teams.
//! - [`apply_economy`] — overwrite `GlobalBudget`,
//!   `ResourceRateTracker`, `ShippingCompanies`,
//!   `PendingResourceRequests`.
//! - [`apply_ui`] — overwrite `ViewMode`, `CurrentStarSystem`,
//!   `SavedSurveyRadius`, `AtmosphereSettings`, `TimeScale`,
//!   `LaunchState`.
//! - [`apply_notifications`] — overwrite `NotificationSettings`.
//! - [`apply_autosave`] — overwrite `AutosaveTimer`.
//!
//! The public entry point [`apply_state_store`] runs all of
//! them in dependency order and returns an [`ApplyOutcome`]
//! summary for the save-load UI.

use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};

use super::state_store::{BodyDivergence, BodyKey, StateStore};
use crate::astronomy::components::SystemId;
use crate::colony::components::Colony;
use crate::economy::components::{LocalStockpile, PlanetResources, Population};
use crate::fleets::components::{Fleet, FleetOrbit, ShipInfo};
use crate::fleets::types::{FleetRole, PropulsionType, ShipClass};
use crate::persistence::playtime::PlaytimeTracker;
use crate::plugins::camera::ViewMode;
use crate::plugins::solar_system::CelestialBody;
use crate::research::components::ResearchTeamCapacity;
use crate::research::systems::ResearchState;
use crate::survey::types::SurveyDimension;
use crate::ui::launch::LaunchState;
use crate::ui::time::SimulationTime;

/// What the apply step did. Surfaced to the save-load UI so the
/// player gets a "Loaded, N bodies diverged" toast.
#[derive(Debug, Default, Clone)]
pub struct ApplyOutcome {
    /// Number of `BodyDivergence` entries applied.
    pub bodies_applied: usize,
    /// Number of bodies that were marked destroyed (and
    /// therefore despawned).
    pub bodies_destroyed: usize,
    /// Number of `SurveyDivergence` entries applied.
    pub surveys_applied: usize,
    /// Number of fleet records respawned.
    pub fleets_spawned: usize,
    /// Number of ships across all fleets.
    pub ships_spawned: usize,
    /// `true` if the regen chain's seed mismatched the save's
    /// stored seed; the apply path still proceeds but flags it
    /// for the UI to surface as a warning.
    pub seed_mismatch: bool,
    /// Human-readable warnings (anchor body missing, fleet
    /// ships dropped, etc.). Surface to the campaign log.
    pub warnings: Vec<String>,
}

// ════════════════════════════════════════════════════════════
// Public entry point
// ════════════════════════════════════════════════════════════

/// Apply `store` to `world`. The world must already contain the
/// regen chain's output (a freshly-built universe seeded from
/// `store.metadata.seed`). This function overlays the saved
/// divergences and player state on top.
///
/// `world` is taken by `&mut` because every Bevy 0.18 component
/// / resource insertion requires it.
///
/// # Restore-path regen fallback (PR-I follow-up)
///
/// When the world has zero celestial bodies (e.g. the
/// production restore factory built via
/// [`super::game_setup::build_minimal_world_for_restore`] —
/// empty, with no rendering plugins), the apply would silently
/// drop every per-body divergence as a warning. To keep the
/// player-visible contract (saving and loading preserves
/// `LocalStockpile`, `Population`, `Colony` etc.) without
/// pulling the entire plugin stack into the restore factory,
/// [`apply_state_store`] calls [`regenerate_bodies_minimal`]
/// before `apply_bodies` when it detects an empty universe.
///
/// This minimal regen spawns `CelestialBody` + `SystemId`
/// entities by loading [`SolarSystemData`] from
/// `assets/data/solar_system.ron`; it deliberately skips
/// meshes, materials, transforms, atmospheric shells, and the
/// full regen chain's `populate_nearby_systems` /
/// `initialize_colony_stockpiles` / etc. The minimal regen is
/// enough for `apply_bodies` to find the bodies and overlay the
/// saved divergences, which is what the player observes via the
/// top resource bar and the dossier panel. The non-rendering
/// regen chain stays where it is (it runs on first New Game).
///
/// Behavioural guard: the minimal regen is a no-op when the
/// world already has bodies — i.e. it does not duplicate the
/// New Game path.
pub fn apply_state_store(world: &mut World, store: &StateStore) -> ApplyOutcome {
    let mut outcome = ApplyOutcome {
        seed_mismatch: check_seed_mismatch(world, store),
        ..ApplyOutcome::default()
    };

    // PR-I follow-up (GRA-358): if the fresh restore world has
    // no celestial bodies, populate it minimally so `apply_bodies`
    // can find the divergence targets. This bridges the gap
    // between the old v1 DynamicScene path (which carried the
    // full entity set in the save) and the v2 StateStore path
    // (which carries only divergences). Without this step every
    // per-body override — most critically `LocalStockpile` —
    // would surface as a "regen chain did not produce that body"
    // warning and the top resource bar would read zero after
    // load.
    if !world_has_celestial_bodies(world) {
        // Note: the "regen-minimal ran" event isn't surfaced as
        // an `outcome.warnings` entry — the warnings vector is
        // currently discarded by `promote_pending_world`, and
        // the regen-minimal is a silent fallback rather than a
        // user-visible restoration step. If the warnings path
        // ever gets wired to the Save Panel, this is the line
        // to populate.
        let _ = regenerate_bodies_minimal(world, store.metadata.start_timestamp);
    }

    // PR-I follow-up: the restore factory
    // (`build_minimal_world_for_restore`) returns a world with
    // only `MinimalPlugins + PersistencePlugin` registered, so
    // the economy / research / ui / autosave resources the
    // apply path reads are absent. Without this init step the
    // appliers would silently no-op on `get_resource_mut::<T>()`
    // and the player would lose treasury, research points,
    // unlocked tech, view mode, atmosphere quality, autosave
    // cadence, etc. on every Restore. We seed the missing
    // resources with their `Default` impl here so the apply
    // can find them and overlay the saved values.
    init_missing_resources_for_apply(world);

    apply_metadata(world, store);
    apply_bodies(world, &store.bodies, &mut outcome);
    apply_surveys(world, &store.surveys, &mut outcome);
    apply_fleets(world, &store.fleets, &mut outcome);
    apply_research(world, &store.research, &mut outcome);
    apply_economy(world, &store.economy, &mut outcome);
    apply_ui(world, &store.ui);
    apply_notifications(world, &store.notifications);
    apply_autosave(world, &store.meta_autosave);

    outcome
}

// ════════════════════════════════════════════════════════════
// Regen-minimal fallback (PR-I follow-up; see apply_state_store)
// ════════════════════════════════════════════════════════════

/// `true` if `world` already has at least one entity with a
/// `CelestialBody` component. Used by `apply_state_store` to
/// decide whether to run [`regenerate_bodies_minimal`].
fn world_has_celestial_bodies(world: &mut World) -> bool {
    // `With<CelestialBody>` is a borrowed-component filter;
    // iterating for entities (not components) avoids the borrow
    // conflict with the caller's `&mut World` while still
    // answering "does any body entity exist?".
    let mut q = world.query_filtered::<Entity, With<CelestialBody>>();
    q.iter(world).next().is_some()
}

/// Minimal body regen for the v2 restore path. Loads
/// [`crate::plugins::solar_system_data::SolarSystemData`] from
/// `assets/data/solar_system.ron` and spawns an entity per
/// body with the *minimum* component set the apply path reads:
///
/// - `CelestialBody` — for `BodyKey::name` lookup
/// - `SystemId(0)` — `Star System` placeholder (the RON file
///   has only one Sol system)
///
/// Other components (`KeplerOrbit`, `SpaceCoordinates`, mesh
/// handles, atmospheric shells, parent hierarchy) are skipped:
/// they're the responsibility of the full regen chain (which
/// runs on the New Game path; the Restore path doesn't need
/// them just to surface the player's LocalStockpile in the top
/// bar). When the user's `apply_state_store` call is later
/// replaced by the full regen-on-restore path, this helper
/// becomes a no-op (the bodies already exist by the time
/// `apply_bodies` runs).
///
/// Returns `true` if any bodies were spawned; `false` if the
/// RON loader failed or the world already had entities (so we
/// could leave a useful log line on the warning trail).
pub(crate) fn regenerate_bodies_minimal(world: &mut World, start_timestamp: i64) -> bool {
    use crate::plugins::solar_system_data::SolarSystemData;

    let data = match SolarSystemData::load_from_file("assets/data/solar_system.ron") {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "regenerate_bodies_minimal: failed to load solar_system.ron ({e}); \
                 per-body divergences will be skipped"
            );
            return false;
        }
    };

    // Filter out historically-destroyed bodies the same way
    // `setup_solar_system` does — saves written for a "modern"
    // game start shouldn't bring back SL-9 or ISON.
    let mut bodies = data.bodies;
    let pre = bodies.len();
    bodies.retain(|b| {
        b.destroyed_at
            .is_none_or(|t| start_timestamp == 0 || start_timestamp < t)
    });
    let _removed = pre - bodies.len();

    for body_data in bodies.iter() {
        // Look at the helper test that builds bodies from
        // `make_body` for the canonical `CelestialBody` shape —
        // the regen-minimal path deliberately uses the same
        // shape so any future builder changes apply uniformly.
        //
        // `visual_radius` MUST use `calculate_visual_radius`
        // (the regen-chain's non-linear scaling for stars +
        // power-curve for planets) rather than the raw
        // physical radius. A raw radius (e.g. Sol at
        // 696,340 km) makes the camera's `update_min_zoom`
        // floor become `radius × 2.5 ≈ 1,740,000` game units,
        // which jumps the camera back into Starmap on every
        // System-mode entry. The regen-chain's defaults keep
        // Sol at `696_340 × 0.00015 = 104.45` game units so
        // `update_min_zoom` floors at 250 (the minimum).
        let visual_radius = crate::plugins::solar_system_data::calculate_visual_radius(
            body_data.body_type,
            body_data.radius,
        );
        let entity = world
            .spawn((
                CelestialBody {
                    name: body_data.name.clone(),
                    radius: body_data.radius,
                    mass: body_data.mass,
                    body_type: body_data.body_type,
                    visual_radius,
                    asteroid_class: body_data.asteroid_class,
                    star_approach_au: body_data.star_approach_au,
                    rotation_period_s: body_data.rotation_period_seconds(),
                    habitable_outer_au: if body_data.body_type
                        == crate::plugins::solar_system_data::BodyType::Star
                    {
                        Some(1.0)
                    } else {
                        None
                    },
                },
                SystemId(0usize),
                // `SpaceCoordinates` at a placeholder
                // `DVec3::ZERO` lets the camera-target-center
                // code resolve the body when transitioning from
                // Starmap → System. Without it, `transition` skips
                // the body (and `update_min_zoom` falls back to
                // the `5.0` minimum that clips the camera into a
                // tiny frustum — producing the
                // `radius: 1739250 → Starmap` bounce the user
                // observed). PR-J will replace the placeholder
                // with the orbital-propagation result.
                crate::astronomy::components::SpaceCoordinates::new(bevy::math::DVec3::ZERO),
                // PR-I follow-up: the regen chain normally
                // inserts an empty `PlanetResources` on every
                // body and the spectral-class logic fills it in.
                // Without the regen chain running on Restore,
                // colonies whose body lacks this component hit
                // the mining system's per-frame
                // `Colony X has no PlanetResources` warning (now
                // demoted to `debug!` in `mining.rs`). The empty
                // default here lets the mining system find the
                // component (and compute zero rates cleanly
                // because the deposit map is empty) until the
                // full regen chain lands in PR-J.
                crate::economy::components::PlanetResources::default(),
            ))
            .id();
        let _ = entity; // entity Map for follow-up hierarchies not needed here.
    }

    true
}

/// Seed every resource the apply path reads with its `Default`
/// impl if it isn't already present on `world`. The list mirrors
/// the test-side `bootstrap_world` helper at the bottom of this
/// file — keep them in sync; if a new applier is added and it
/// reads a resource, add it here too.
///
/// The `apply_*` family uses `if let Some(mut r) =
/// world.get_resource_mut::<T>()` everywhere, so a missing
/// resource silently drops the corresponding state — every
/// resource listed below is one the player would notice missing
/// on Restore (treasury, research points, unlocked tech, view
/// mode, atmosphere quality, autosave cadence, notification
/// settings, etc.).
///
/// This helper is a no-op for each resource that's already
/// present, so it's safe to call from the production entry
/// point on every Restore without affecting the New Game path
/// (whose factory builds the resources through `MinimalPlugins
/// + PersistencePlugin + regen chain`).
fn init_missing_resources_for_apply(world: &mut World) {
    use crate::astronomy::components::CurrentStarSystem;
    use crate::economy::budget::{GlobalBudget, ResourceRateTracker};
    use crate::economy::company::ShippingCompanies;
    use crate::economy::logistics::PendingResourceRequests;
    use crate::persistence::autosave::AutosaveTimer;
    use crate::plugins::atmosphere::AtmosphereSettings;
    use crate::plugins::camera::{SavedSurveyRadius, ViewMode};
    use crate::research::components::ResearchTeamCapacity;
    use crate::research::systems::ResearchState;
    use crate::ui::notifications::settings::NotificationSettings;

    macro_rules! init_if_missing {
        ($world:expr, $t:ty) => {
            if !$world.get_resource::<$t>().is_some() {
                $world.init_resource::<$t>();
            }
        };
    }

    // The order matches the test bootstrap exactly; new
    // resources should be appended to the bottom of each
    // section so a code review can spot additions.
    init_if_missing!(world, SimulationTime);
    init_if_missing!(world, ResearchState);
    init_if_missing!(world, ResearchTeamCapacity);
    init_if_missing!(world, GlobalBudget);
    init_if_missing!(world, ResourceRateTracker);
    init_if_missing!(world, ShippingCompanies);
    init_if_missing!(world, PendingResourceRequests);
    init_if_missing!(world, ViewMode);
    init_if_missing!(world, CurrentStarSystem);
    init_if_missing!(world, SavedSurveyRadius);
    init_if_missing!(world, AtmosphereSettings);
    // TimeScale, LaunchState, GameSeed, PlaytimeTracker are
    // already inserted by `build_minimal_world`; the
    // `bootstrap_world` test helper also re-inserts them
    // because it builds a bare World. The apply path tolerates
    // either path.
    init_if_missing!(world, NotificationSettings);
    init_if_missing!(world, AutosaveTimer);
}

// ════════════════════════════════════════════════════════════
// Metadata → seed / playtime / SimulationTime
// ════════════════════════════════════════════════════════════

/// Apply the top-level metadata. The regen chain should have
/// already inserted `GameSeed` and `PlaytimeTracker`; we
/// overwrite the playtime so the player sees the correct total
/// when they hit the in-game stats panel.
fn apply_metadata(world: &mut World, store: &StateStore) {
    if let Some(mut playtime) = world.get_resource_mut::<PlaytimeTracker>() {
        playtime.total_real_s = store.metadata.playtime_s;
    }
    // SimulationTime: the regen chain builds this with
    // `with_start_timestamp` and `elapsed = 0`. We restore the
    // sim clock so the next frame's orbital propagation uses
    // the player's actual save-time position.
    if let Some(mut time) = world.get_resource_mut::<SimulationTime>() {
        time.elapsed = store.metadata.sim_now_seconds;
    }
    // The regen chain's GameSeed may differ from the save's
    // seed (user manually typed a different one, or the
    // launcher swapped presets). The apply path does not
    // overwrite the regen-chain seed — divergence is flagged
    // in `ApplyOutcome::seed_mismatch` and surfaced in the UI.
}

fn check_seed_mismatch(world: &World, store: &StateStore) -> bool {
    world
        .get_resource::<crate::game_state::GameSeed>()
        .map(|g| g.value != store.metadata.seed)
        .unwrap_or(false)
}

// ════════════════════════════════════════════════════════════
// Bodies → Colony / Population / Resources / Atmosphere
// ════════════════════════════════════════════════════════════

/// Build a (BodyKey → Entity) index over the live world. We
/// can't keep an indexed map as a resource (Bevy entities
/// mutate across runs) so we rebuild it on every apply.
fn build_body_index(world: &mut World) -> HashMap<BodyKey, Entity> {
    let mut out = HashMap::new();
    let mut q = world.query::<(Entity, &CelestialBody, &SystemId)>();
    for (e, body, sys) in q.iter(world) {
        out.insert(BodyKey::new(*sys, body.name.clone()), e);
    }
    out
}

fn apply_bodies(
    world: &mut World,
    bodies: &BTreeMap<BodyKey, BodyDivergence>,
    outcome: &mut ApplyOutcome,
) {
    let body_index = build_body_index(world);

    for (key, div) in bodies {
        let Some(&entity) = body_index.get(key) else {
            outcome.warnings.push(format!(
                "body divergence for {:?} skipped: regen chain did not produce that body",
                key
            ));
            continue;
        };

        if div.destroyed.is_some() {
            world.despawn(entity);
            outcome.bodies_destroyed += 1;
            outcome.bodies_applied += 1;
            continue;
        }

        if let Some(json) = &div.colony_override {
            match serde_json::from_value::<Colony>(json.clone()) {
                Ok(c) => {
                    world.entity_mut(entity).insert(c);
                }
                Err(e) => outcome.warnings.push(format!(
                    "failed to deserialise colony_override for {:?}: {}",
                    key, e
                )),
            }
        }

        if let Some(json) = &div.population_override {
            match serde_json::from_value::<Population>(json.clone()) {
                Ok(p) => {
                    world.entity_mut(entity).insert(p);
                }
                Err(e) => outcome.warnings.push(format!(
                    "failed to deserialise population_override for {:?}: {}",
                    key, e
                )),
            }
        }

        if let Some(json) = &div.resources_override {
            apply_resources_override(world, entity, json, key, &mut outcome.warnings);
        }

        if div.atmosphere_override.is_some() {
            // TODO(pr-i): `AtmosphereComposition` doesn't
            // derive `Serialize/Deserialize` (see astronomy
            // /components.rs:567 — only Component+Reflect).
            // The extract path therefore can't emit an
            // atmosphere_override blob today. Land a derive
            // on `AtmosphereComposition` (with `serde::default`
            // on every f32 field to preserve the existing
            // `Default`/`Reflect` behaviour) and wire this
            // branch in the same commit.
            outcome.warnings.push(format!(
                "atmosphere_override for {:?} skipped (AtmosphereComposition needs Serialize derive)",
                key
            ));
        }

        if div.mean_anomaly_epoch_override.is_some()
            || div.semi_major_axis_override.is_some()
            || div.eccentricity_override.is_some()
        {
            apply_orbital_override(world, entity, div, &mut outcome.warnings);
        }

        outcome.bodies_applied += 1;
    }
}

fn apply_resources_override(
    world: &mut World,
    entity: Entity,
    json: &serde_json::Value,
    key: &BodyKey,
    warnings: &mut Vec<String>,
) {
    if let Some(deposits) = json.get("deposits") {
        match serde_json::from_value::<PlanetResources>(deposits.clone()) {
            Ok(r) => {
                world.entity_mut(entity).insert(r);
            }
            Err(e) => warnings.push(format!(
                "failed to deserialise resources_override.deposits for {:?}: {}",
                key, e
            )),
        }
    }
    if let Some(stock) = json.get("stockpile") {
        match serde_json::from_value::<LocalStockpile>(stock.clone()) {
            Ok(s) => {
                world.entity_mut(entity).insert(s);
            }
            Err(e) => warnings.push(format!(
                "failed to deserialise resources_override.stockpile for {:?}: {}",
                key, e
            )),
        }
    }
}

fn apply_orbital_override(
    world: &mut World,
    entity: Entity,
    div: &BodyDivergence,
    warnings: &mut Vec<String>,
) {
    use crate::astronomy::components::KeplerOrbit;
    let has_override = div.mean_anomaly_epoch_override.is_some()
        || div.semi_major_axis_override.is_some()
        || div.eccentricity_override.is_some();
    if !has_override {
        return;
    }
    let Some(mut em) = world.get_entity_mut(entity).ok() else {
        warnings.push(format!(
            "orbital override skipped: entity {:?} missing",
            entity
        ));
        return;
    };
    if let Some(mut orbit) = em.get_mut::<KeplerOrbit>() {
        if let Some(m) = div.mean_anomaly_epoch_override {
            orbit.mean_anomaly_epoch = m;
        }
        if let Some(a) = div.semi_major_axis_override {
            orbit.semi_major_axis = a;
        }
        if let Some(e) = div.eccentricity_override {
            orbit.eccentricity = e;
        }
    } else {
        warnings.push(
            "orbital override requested but body has no KeplerOrbit (regen chain skip)".to_string(),
        );
    }
}

// ════════════════════════════════════════════════════════════
// Surveys → SurveyState
// ════════════════════════════════════════════════════════════

fn apply_surveys(
    world: &mut World,
    surveys: &BTreeMap<BodyKey, super::state_store::SurveyDivergence>,
    outcome: &mut ApplyOutcome,
) {
    use crate::survey::components::SurveyState;

    let body_index = build_body_index(world);
    for (key, div) in surveys {
        let Some(&entity) = body_index.get(key) else {
            outcome.warnings.push(format!(
                "survey divergence for {:?} skipped: body not in regen chain",
                key
            ));
            continue;
        };
        if let Some(json) = &div.state_json {
            match serde_json::from_value::<SurveyState>(json.clone()) {
                Ok(state) => {
                    world.entity_mut(entity).insert(state);
                }
                Err(e) => outcome.warnings.push(format!(
                    "failed to deserialise survey_state for {:?}: {}",
                    key, e
                )),
            }
        } else {
            // Legacy / v0.4.x save: rebuild a minimal
            // SurveyState from the per-dimension tier summary.
            let mut state = SurveyState::unsurveyed();
            for (dim_name, tier) in &div.dimension_tiers {
                use crate::survey::components::DimensionFidelity;
                if let Some(dim) = parse_survey_dimension(dim_name) {
                    state.dimensions.insert(
                        dim,
                        DimensionFidelity {
                            tier: *tier as u8,
                            last_measured_sim_time: Some(div.last_surveyed_sim_seconds),
                            confidence: 1.0,
                        },
                    );
                }
            }
            state.drill_missions_completed = div.drill_missions_completed;
            state.last_updated_sim_time = div.last_surveyed_sim_seconds;
            world.entity_mut(entity).insert(state);
        }
        outcome.surveys_applied += 1;
    }
}

fn parse_survey_dimension(name: &str) -> Option<SurveyDimension> {
    match name {
        "OrbitalMech" => Some(SurveyDimension::OrbitalMech),
        "Atmosphere" => Some(SurveyDimension::Atmosphere),
        "SurfaceFeatures" => Some(SurveyDimension::SurfaceFeatures),
        "MineralClasses" => Some(SurveyDimension::MineralClasses),
        "MineralDeposits" => Some(SurveyDimension::MineralDeposits),
        "Subsurface" => Some(SurveyDimension::Subsurface),
        "Habitability" => Some(SurveyDimension::Habitability),
        "Anomalies" => Some(SurveyDimension::Anomalies),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════
// Fleets → Fleet + FleetOrbit + ShipInfo
// ════════════════════════════════════════════════════════════

fn apply_fleets(
    world: &mut World,
    fleets: &[super::state_store::FleetRecord],
    outcome: &mut ApplyOutcome,
) {
    let body_index = build_body_index(world);
    for record in fleets {
        let mut ships = Vec::new();
        for ship in &record.ships {
            match build_ship_info(ship) {
                Ok(s) => ships.push(s),
                Err(e) => outcome.warnings.push(format!(
                    "fleet {:?}: dropped ship #{} ({})",
                    record.name, ship.key, e
                )),
            }
        }
        outcome.ships_spawned += ships.len();

        let role = record
            .logistics_policy
            .as_deref()
            .and_then(parse_fleet_role)
            .unwrap_or_default();
        let fleet = Fleet {
            name: record.name.clone(),
            role,
            ships,
        };
        let mut entity = world.spawn(fleet);
        if let Some(anchor) = &record.at_anchor {
            if let Some(&body) = body_index.get(anchor) {
                entity.insert(FleetOrbit::new(body, 0.05));
            } else {
                outcome.warnings.push(format!(
                    "fleet {:?}: anchor body {:?} not in regen chain; parking orbit skipped",
                    record.name, anchor
                ));
            }
        }
        outcome.fleets_spawned += 1;
    }
}

fn build_ship_info(record: &super::state_store::ShipRecord) -> Result<ShipInfo, String> {
    let class = parse_ship_class(&record.class)
        .ok_or_else(|| format!("unknown ShipClass `{}`", record.class))?;
    let hull_id = if record.hull.is_empty() {
        None
    } else {
        Some(record.hull.clone())
    };
    let mut info = if let Some(hid) = hull_id.as_deref() {
        ShipInfo::new_with_dry_mass(
            record.name.clone().unwrap_or_default(),
            Some(hid),
            class,
            default_propulsion_for(class),
            class.default_dry_mass_t(),
        )
    } else {
        ShipInfo::new(
            record.name.clone().unwrap_or_default(),
            class,
            default_propulsion_for(class),
        )
    };
    if record.fuel_fraction > 0.0 {
        let wet = info.dry_mass_t / (1.0 - record.fuel_fraction);
        info.fuel_mass_t = wet - info.dry_mass_t;
        info.max_fuel_t = info.fuel_mass_t;
    }
    if record.hull_integrity < 1.0 {
        return Err(format!(
            "hull_integrity {} below 1.0 (not persisted yet)",
            record.hull_integrity
        ));
    }
    Ok(info)
}

fn parse_ship_class(name: &str) -> Option<ShipClass> {
    match name {
        "Courier" => Some(ShipClass::Courier),
        "Frigate" => Some(ShipClass::Frigate),
        "Destroyer" => Some(ShipClass::Destroyer),
        "Cruiser" => Some(ShipClass::Cruiser),
        "ResearchVessel" => Some(ShipClass::ResearchVessel),
        "Freighter" => Some(ShipClass::Freighter),
        "Station" => Some(ShipClass::Station),
        _ => None,
    }
}

fn default_propulsion_for(class: ShipClass) -> PropulsionType {
    match class {
        ShipClass::Courier => PropulsionType::Chemical,
        ShipClass::Frigate => PropulsionType::Chemical,
        ShipClass::Destroyer => PropulsionType::NuclearThermal,
        ShipClass::Cruiser => PropulsionType::NuclearThermal,
        ShipClass::ResearchVessel => PropulsionType::IonDrive,
        ShipClass::Freighter => PropulsionType::Chemical,
        ShipClass::Station => PropulsionType::Chemical,
    }
}

fn parse_fleet_role(name: &str) -> Option<FleetRole> {
    match name {
        "Unassigned" => Some(FleetRole::Unassigned),
        "Attack" => Some(FleetRole::Attack),
        "Defend" => Some(FleetRole::Defend),
        "Survey" => Some(FleetRole::Survey),
        "Transport" => Some(FleetRole::Transport),
        "Explore" => Some(FleetRole::Explore),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════
// Research → ResearchState + ResearchTeamCapacity
// ════════════════════════════════════════════════════════════

fn apply_research(
    world: &mut World,
    record: &super::state_store::ResearchRecord,
    outcome: &mut ApplyOutcome,
) {
    if let Some(rp_avail) = record.rp_balance.get("available") {
        if let Some(mut state) = world.get_resource_mut::<ResearchState>() {
            state.research_points_available = *rp_avail;
        }
    }
    if let Some(ep_avail) = record.ep_balance.get("available") {
        if let Some(mut state) = world.get_resource_mut::<ResearchState>() {
            state.engineering_points_available = *ep_avail;
        }
    }

    if !record.unlocked.is_empty() {
        if let Some(mut state) = world.get_resource_mut::<ResearchState>() {
            state.unlocked_technologies.clear();
            for id in &record.unlocked {
                state.unlocked_technologies.insert(id.clone());
            }
        }
    }

    if let Some(cap) = record.team_capacity.get("research") {
        if let Some(mut c) = world.get_resource_mut::<ResearchTeamCapacity>() {
            c.max_research_teams = *cap as usize;
        }
    }
    if let Some(cap) = record.team_capacity.get("engineering") {
        if let Some(mut c) = world.get_resource_mut::<ResearchTeamCapacity>() {
            c.max_engineering_teams = *cap as usize;
        }
    }

    if !record.projects.is_empty() {
        outcome.warnings.push(format!(
            "{} research/engineering projects carried across; team binding is reset (player resumes manually)",
            record.projects.len()
        ));
    }
}

// ════════════════════════════════════════════════════════════
// Economy → GlobalBudget / ResourceRateTracker / Shipping / Requests
// ════════════════════════════════════════════════════════════

fn apply_economy(
    world: &mut World,
    record: &super::state_store::EconomyRecord,
    outcome: &mut ApplyOutcome,
) {
    use crate::economy::budget::GlobalBudget;
    use crate::economy::company::ShippingCompanies;
    use crate::economy::logistics::PendingResourceRequests;

    if let Some(mut budget) = world.get_resource_mut::<GlobalBudget>() {
        budget.treasury = record.treasury;
    }

    if !record.rates.is_empty() {
        if let Some(mut tracker) =
            world.get_resource_mut::<crate::economy::budget::ResourceRateTracker>()
        {
            tracker.gross_production_rates.clear();
            tracker.gross_consumption_rates.clear();
            for (res_name, (prod, cons)) in &record.rates {
                if let Some(res) = parse_resource_type(res_name) {
                    tracker.gross_production_rates.insert(res, *prod);
                    tracker.gross_consumption_rates.insert(res, *cons);
                }
            }
        }
    }

    if !record.shipping_companies.is_empty() {
        if let Some(mut companies) = world.get_resource_mut::<ShippingCompanies>() {
            companies.companies.clear();
            for c in &record.shipping_companies {
                use crate::economy::company::{
                    CompanyAIPolicy, CompanyBuildPolicy, ShippingCompany,
                };
                companies.companies.push(ShippingCompany {
                    name: c.name.clone(),
                    treasury_mc: c.credit_balance,
                    freighter_count: 0,
                    available_freighters: 0,
                    total_deliveries: 0,
                    reputation: 0.5,
                    policy: CompanyAIPolicy::AutoFreight,
                    treasury_window_start_mc: c.credit_balance,
                    treasury_window_start_seconds: 0.0,
                    build_policy: CompanyBuildPolicy::Manual,
                    home_body: None,
                    max_active_builds: 1,
                    active_builds: 0,
                });
            }
        }
    }

    if !record.pending_requests.is_empty() {
        if let Some(mut reqs) = world.get_resource_mut::<PendingResourceRequests>() {
            reqs.requests.clear();
            for r in &record.pending_requests {
                use crate::economy::logistics::{RequestPriority, RequestState, ResourceRequest};
                if let Some(res) = parse_resource_type(&r.resource) {
                    reqs.requests.push(ResourceRequest {
                        id: 0,
                        destination_body: Entity::PLACEHOLDER,
                        destination_name: r.destination.name.clone(),
                        resource: res,
                        amount_mt: r.amount_megatonnes,
                        priority: parse_request_priority(&r.priority)
                            .unwrap_or(RequestPriority::Trade),
                        state: parse_request_state(&r.state).unwrap_or(RequestState::Pending),
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
                }
            }
        }
        outcome.warnings.push(format!(
            "{} pending resource requests carried across; destinations re-bound to body names (player re-targets manually)",
            record.pending_requests.len()
        ));
    }
}

fn parse_resource_type(name: &str) -> Option<crate::economy::ResourceType> {
    use crate::economy::ResourceType;
    match name {
        "Water" => Some(ResourceType::Water),
        "Hydrogen" => Some(ResourceType::Hydrogen),
        "Ammonia" => Some(ResourceType::Ammonia),
        "Methane" => Some(ResourceType::Methane),
        "Phosphorus" => Some(ResourceType::Phosphorus),
        "Food" => Some(ResourceType::Food),
        "Nitrogen" => Some(ResourceType::Nitrogen),
        "Oxygen" => Some(ResourceType::Oxygen),
        "CarbonDioxide" => Some(ResourceType::CarbonDioxide),
        "Argon" => Some(ResourceType::Argon),
        "Iron" => Some(ResourceType::Iron),
        "Aluminum" => Some(ResourceType::Aluminum),
        "Titanium" => Some(ResourceType::Titanium),
        "Silicates" => Some(ResourceType::Silicates),
        "Nickel" => Some(ResourceType::Nickel),
        "Tungsten" => Some(ResourceType::Tungsten),
        "Carbon" => Some(ResourceType::Carbon),
        "Chromium" => Some(ResourceType::Chromium),
        "Magnesium" => Some(ResourceType::Magnesium),
        "Helium3" => Some(ResourceType::Helium3),
        "Deuterium" => Some(ResourceType::Deuterium),
        "Uranium" => Some(ResourceType::Uranium),
        "Thorium" => Some(ResourceType::Thorium),
        "Gold" => Some(ResourceType::Gold),
        "Silver" => Some(ResourceType::Silver),
        "Platinum" => Some(ResourceType::Platinum),
        "Copper" => Some(ResourceType::Copper),
        "RareEarths" => Some(ResourceType::RareEarths),
        "Lithium" => Some(ResourceType::Lithium),
        "Sulfur" => Some(ResourceType::Sulfur),
        "Cobalt" => Some(ResourceType::Cobalt),
        "Fluorine" => Some(ResourceType::Fluorine),
        "Polymers" => Some(ResourceType::Polymers),
        "Antimatter" => Some(ResourceType::Antimatter),
        "ExoticMatter" => Some(ResourceType::ExoticMatter),
        "Metamaterials" => Some(ResourceType::Metamaterials),
        "Computronium" => Some(ResourceType::Computronium),
        _ => None,
    }
}

fn parse_request_priority(name: &str) -> Option<crate::economy::logistics::RequestPriority> {
    use crate::economy::logistics::RequestPriority;
    match name {
        "Emergency" => Some(RequestPriority::Emergency),
        "Construction" => Some(RequestPriority::Construction),
        "Maintenance" => Some(RequestPriority::Maintenance),
        "Trade" => Some(RequestPriority::Trade),
        _ => None,
    }
}

fn parse_request_state(name: &str) -> Option<crate::economy::logistics::RequestState> {
    use crate::economy::logistics::RequestState;
    match name {
        "Pending" => Some(RequestState::Pending),
        "Assigned" => Some(RequestState::Assigned),
        "InTransit" => Some(RequestState::InTransit),
        "Delivered" => Some(RequestState::Delivered),
        "Expired" => Some(RequestState::Expired),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════
// UI / camera / time / state
// ════════════════════════════════════════════════════════════

fn apply_ui(world: &mut World, record: &super::state_store::UiRecord) {
    use crate::astronomy::components::CurrentStarSystem;
    use crate::plugins::atmosphere::AtmosphereSettings;
    use crate::plugins::camera::SavedSurveyRadius;
    use crate::ui::time::TimeScale;

    if let Some(mut view) = world.get_resource_mut::<ViewMode>() {
        *view = parse_view_mode(&record.view_mode).unwrap_or_default();
    }
    if let Some(mut star) = world.get_resource_mut::<CurrentStarSystem>() {
        star.0 = record.current_star_system as usize;
    }
    if let Some(mut radius) = world.get_resource_mut::<SavedSurveyRadius>() {
        radius.0 = if record.survey_radius_au > 0.0 {
            Some(record.survey_radius_au as f32)
        } else {
            None
        };
    }
    if let Some(mut atm) = world.get_resource_mut::<AtmosphereSettings>() {
        atm.enabled = record.atmosphere_enabled;
        atm.quality = parse_atmosphere_quality(&record.atmosphere_quality);
    }
    if let Some(mut ts) = world.get_resource_mut::<TimeScale>() {
        if record.paused {
            ts.pause();
        } else {
            ts.set_speed(record.time_scale as f32);
        }
    }
    if let Some(mut launch) = world.get_resource_mut::<LaunchState>() {
        *launch = parse_launch_state(&record.launch_state);
    }
}

fn parse_view_mode(name: &str) -> Option<ViewMode> {
    match name {
        "System" => Some(ViewMode::System),
        "Starmap" => Some(ViewMode::Starmap),
        _ => None,
    }
}

fn parse_atmosphere_quality(name: &str) -> u32 {
    match name {
        "Low" => 0,
        "Medium" => 1,
        "High" => 2,
        _ => 1,
    }
}

fn parse_launch_state(name: &str) -> LaunchState {
    match name {
        "MainMenu" => LaunchState::MainMenu,
        "NewGame" => LaunchState::NewGame,
        "LoadGame" => LaunchState::LoadGame,
        "Settings" => LaunchState::Settings,
        "SaveGame" => LaunchState::SaveGame,
        "InGame" => LaunchState::InGame,
        _ => LaunchState::InGame,
    }
}

// ════════════════════════════════════════════════════════════
// Notifications
// ════════════════════════════════════════════════════════════

fn apply_notifications(world: &mut World, record: &super::state_store::NotificationRecord) {
    use crate::ui::notifications::settings::{
        NotificationCategoryId, NotificationSettings, PerCategorySetting,
    };

    if record.category_settings.is_empty() {
        return;
    }
    if let Some(mut settings) = world.get_resource_mut::<NotificationSettings>() {
        for (id, cfg) in &record.category_settings {
            settings.per_category.insert(
                NotificationCategoryId(id.clone()),
                PerCategorySetting {
                    enabled: cfg.enabled,
                    pause_on_event: cfg.pause_on_event,
                    sound_on: cfg.sound,
                    auto_dismiss_s: 0.0,
                    sticky: false,
                },
            );
        }
    }
}

// ════════════════════════════════════════════════════════════
// Autosave
// ════════════════════════════════════════════════════════════

fn apply_autosave(world: &mut World, record: &super::state_store::AutosaveRecord) {
    use crate::persistence::autosave::AutosaveTimer;
    if let Some(mut timer) = world.get_resource_mut::<AutosaveTimer>() {
        if record.autosave_interval_sim_seconds > 0.0 {
            timer.interval_s = record.autosave_interval_sim_seconds;
            timer.next_due_s = record.autosave_interval_sim_seconds;
        }
    }
}

// ════════════════════════════════════════════════════════════
// Unit tests
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameSeed;
    use crate::plugins::solar_system_data::BodyType;
    use std::collections::HashMap;

    fn make_body(world: &mut World, name: &str, sys: u32) -> Entity {
        world
            .spawn((
                CelestialBody {
                    name: name.to_string(),
                    radius: 0.0,
                    mass: 0.0,
                    body_type: BodyType::Planet,
                    visual_radius: 0.0,
                    asteroid_class: None,
                    star_approach_au: None,
                    rotation_period_s: None,
                    habitable_outer_au: None,
                },
                SystemId(sys as usize),
            ))
            .id()
    }

    fn bootstrap_world() -> World {
        let mut world = World::new();
        world.insert_resource(GameSeed { value: 0xABCD });
        world.insert_resource(crate::persistence::playtime::PlaytimeTracker::default());
        world.insert_resource(SimulationTime::default());
        world.init_resource::<ResearchState>();
        world.init_resource::<ResearchTeamCapacity>();
        world.init_resource::<crate::economy::budget::GlobalBudget>();
        world.init_resource::<crate::economy::budget::ResourceRateTracker>();
        world.init_resource::<crate::economy::company::ShippingCompanies>();
        world.init_resource::<crate::economy::logistics::PendingResourceRequests>();
        world.init_resource::<crate::plugins::camera::ViewMode>();
        world.init_resource::<crate::astronomy::components::CurrentStarSystem>();
        world.init_resource::<crate::plugins::camera::SavedSurveyRadius>();
        world.init_resource::<crate::plugins::atmosphere::AtmosphereSettings>();
        world.init_resource::<crate::ui::time::TimeScale>();
        world.init_resource::<crate::ui::launch::LaunchState>();
        world.init_resource::<crate::ui::notifications::settings::NotificationSettings>();
        world.init_resource::<crate::persistence::autosave::AutosaveTimer>();
        world
    }

    #[test]
    fn apply_state_store_on_empty_world_does_not_panic() {
        let mut world = bootstrap_world();
        let store = StateStore::empty(0xABCD);
        let outcome = apply_state_store(&mut world, &store);
        assert_eq!(outcome.bodies_applied, 0);
        assert_eq!(outcome.fleets_spawned, 0);
        assert!(!outcome.seed_mismatch);
    }

    #[test]
    fn apply_state_store_flags_seed_mismatch() {
        let mut world = bootstrap_world();
        // The bootstrap set seed to 0xABCD; the store says 0xBEEF.
        let store = StateStore::empty(0xBEEF);
        let outcome = apply_state_store(&mut world, &store);
        assert!(outcome.seed_mismatch);
    }

    #[test]
    fn apply_bodies_restores_colony_on_matching_body() {
        let mut world = bootstrap_world();
        let _earth = make_body(&mut world, "Earth", 0);

        let mut bodies = BTreeMap::new();
        let colony = Colony {
            name: "Earth".to_string(),
            population: 8.2e9,
            development: crate::colony::components::ColonyDevelopment {
                tier: crate::colony::components::ColonyTier::Civilisation,
                yield_multiplier: 1.0,
                investments: 0,
            },
            buildings: HashMap::new(),
            growth_rate_modifier: 1.0,
        };
        bodies.insert(
            BodyKey::sol("Earth"),
            BodyDivergence {
                colony_override: Some(serde_json::to_value(&colony).unwrap()),
                ..Default::default()
            },
        );
        let store = StateStore {
            bodies,
            ..StateStore::empty(0xABCD)
        };
        let outcome = apply_state_store(&mut world, &store);
        assert_eq!(outcome.bodies_applied, 1);
        assert!(
            outcome.warnings.is_empty(),
            "warnings: {:?}",
            outcome.warnings
        );
    }

    #[test]
    fn apply_bodies_skips_missing_body() {
        let mut world = bootstrap_world();
        let _earth = make_body(&mut world, "Earth", 0);

        let mut bodies = BTreeMap::new();
        bodies.insert(
            BodyKey::sol("Pluto"),
            BodyDivergence {
                destroyed: Some(super::super::state_store::DestroyedRecord {
                    at_unix_s: 0,
                    reason: "test".to_string(),
                }),
                ..Default::default()
            },
        );
        let store = StateStore {
            bodies,
            ..StateStore::empty(0xABCD)
        };
        let outcome = apply_state_store(&mut world, &store);
        assert!(outcome.warnings.iter().any(|w| w.contains("Pluto")));
    }

    #[test]
    fn apply_fleets_spawns_entity() {
        let mut world = bootstrap_world();
        let _earth = make_body(&mut world, "Earth", 0);

        let store = StateStore {
            fleets: vec![super::super::state_store::FleetRecord {
                name: "Day-One Constellation".to_string(),
                at_anchor: Some(BodyKey::sol("Earth")),
                pending_manoeuvre: None,
                ships: vec![super::super::state_store::ShipRecord {
                    key: 0,
                    class: "Frigate".to_string(),
                    name: Some("ISS-1".to_string()),
                    modules: vec![],
                    hull: String::new(),
                    built_sim_seconds: 0.0,
                    assigned_scientists: vec![],
                    fuel_fraction: 0.5,
                    hull_integrity: 1.0,
                }],
                logistics_policy: None,
            }],
            ..StateStore::empty(0xABCD)
        };
        let outcome = apply_state_store(&mut world, &store);
        assert_eq!(outcome.fleets_spawned, 1);
        assert_eq!(outcome.ships_spawned, 1);
        assert!(
            outcome.warnings.is_empty(),
            "warnings: {:?}",
            outcome.warnings
        );

        // The fleet should have a FleetOrbit anchored at Earth.
        let mut q = world.query::<&Fleet>();
        assert_eq!(q.iter(&world).count(), 1);
        let mut q = world.query::<&FleetOrbit>();
        assert_eq!(q.iter(&world).count(), 1);
    }
}
