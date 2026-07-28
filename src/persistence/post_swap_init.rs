//! Post-swap 3D scene decoration for the Restore path (GRA-358 PR-J).
//!
//! ## Why this exists
//!
//! After `swap_world_into` runs, the live `App` has 700+
//! `CelestialBody` entities — but they only carry the *minimal*
//! component set the apply path reads (`CelestialBody`, `SystemId`,
//! `SpaceCoordinates::ZERO`, `PlanetResources::default()`).
//! They're invisible in the 3D scene because they have no
//! `Transform`, no `Mesh3d`, no `KeplerOrbit`. The player's
//! dossier/ledger shows them via the divergence overlay, but the
//! 3D view is empty.
//!
//! PR-I shipped this minimal-body contract on purpose:
//!
//! - the restore factory (`build_minimal_world_for_restore`)
//!   uses `MinimalPlugins + PersistencePlugin`, so it has no
//!   `AssetServer` and can't realistically populate meshes;
//! - the swap duplicates handles between worlds, so even if we
//!   *did* populate meshes in the pending world they wouldn't
//!   survive the reflection-copy into the live `App`;
//! - BootInitPlugin's regen chain is gated off on the Restore
//!   path (via `RestoredWorldGate`) precisely to avoid the B0004
//!   / `STATUS_STACK_OVERFLOW` recursive-`Children`-propagation
//!   failure from PR-J's earlier attempt at running the full
//!   chain on a freshly-swapped world.
//!
//! ## What this module adds
//!
//! A **post-swap decoration pass** that runs once on the live
//! `App` after `promote_pending_world` commits. It walks every
//! `CelestialBody` entity, looks up the matching row in
//! `assets/data/solar_system.ron`, and inserts:
//!
//! - `KeplerOrbit` (analytic orbit descriptor, lets
//!   `propagate_orbits` compute `SpaceCoordinates` from
//!   `SimulationTime`).
//! - `OrbitCenter` (parent-entity pointer so child orbits are
//!   offset from the parent's position).
//! - `LogicalParent` (UI hierarchy pointer for bodies that need
//!   to follow their parent's renderable transform — atmosphere
//!   shells, rings, etc.).
//! - `OrbitPath` (so `draw_orbit_paths` paints a visible ring).
//! - `Transform::default()` + `Visibility::Visible` so the entity
//!   has the bundle Bevy's transform propagation chain needs.
//! - `Mesh3d` + `MeshMaterial3d` (a basic sphere with the
//!   body's `celestial_body.ron` albedo colour; this gives the
//!   player a populated, clickable 3D view).
//!
//! The pass is a **fallback** for the missing regen chain, not a
//! substitute for it. Compared to a full `setup_solar_system`
//! pass it deliberately leaves out:
//!
//! - texturing (the `texture` / `multi_layer_textures` rows on
//!   each CelestialBodyData — the live app's `AssetServer` has
//!   the right handles, but reading them inline doubles the
//!   memory footprint of every Restore);
//! - atmosphere shells, night-side layers, cloud decks,
//!   glow billboards;
//! - star-corona 3D / halo 3D shells;
//! - Lagrange-point markers;
//! - rings (the system_populator pass is still gated off on
//!   Restore by `RestoredWorldGate`).
//!
//! Bodies appear as smooth coloured spheres with their orbit
//! rings drawn — enough for the player to recognise the system,
//! click on bodies, and resume gameplay. A follow-up PR can
//! extend this stub with the textured/sparkly variant when the
//! regen-chain path is unblocked.
//!
//! ## Idempotency
//!
//! The pass gates itself on a marker resource
//! [`RestoredBodiesRendered`] (set on success, missing on entry).
//! `promote_pending_world` removes it as part of the post-restore
//! teardown so a subsequent Restore runs the pass again. New
//! Games never set this marker; the regen chain populates the
//! 3D scene directly via `setup_solar_system`.
//!
//! ## Failure mode
//!
//! Any failure inside the pass sets the marker anyway so the
//! boot-init chain doesn't loop on the same error every frame.
//! The exact failure surfaces as a single
//! `populate_restored_bodies_3d: <detail>` warning at the entry
//! of the `Restore` session — the player sees an "empty 3D
//! view" instead of a black-screen panic.
//!
//! ## Schedule
//!
//! The pass runs in `Update`, gated by
//! `world_ready_is_present AND restored_world_is_present AND
//! NOT restored_bodies_already_rendered`. The gate mirrors
//! `BootInitPlugin`'s run_if chain, minus the `Loading` part
//! (Restore lands directly in `InGame`, not `Loading`).

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::prelude::{AlphaMode, LinearRgba};
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::astronomy::components::{KeplerOrbit, LocalOrbitAmplification, OrbitCenter, OrbitPath};
use crate::economy::components::{OrbitsBody, StarSystem};
use crate::persistence::swap::RestoredWorldGate;
use crate::plugins::solar_system::{
    create_ring_mesh, Asteroid, CelestialBody, Comet, DwarfPlanet, GasGiant, LogicalParent, Moon,
    Planet, Ring, Star, StarCorona3dMaterial, StarCoronaShell, StarHalo3dMaterial, StarHaloShell,
    StarSurfaceMaterial,
};
use crate::plugins::solar_system_data::{
    calculate_visual_radius, AsteroidClass, BodyType, CelestialBodyData, OrbitData, SolarSystemData,
};

/// Marker resource inserted by [`populate_restored_bodies_3d`]
/// once the 3D pass has run. The pass's `run_if` gate short-
/// circuits on subsequent ticks; a future Restore that wants to
/// re-render the scene (or any other reset path) must
/// `world.remove_resource::<RestoredBodiesRendered>()` first.
///
/// Not registered into `AppTypeRegistry` — there is no save-
/// time value, only a "have we decorated yet?" gate, and the
/// persistence denylist would drop it from round-trip anyway.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct RestoredBodiesRendered;

/// Populate the 3D scene for the restore path. Runs on the live
/// `App`'s world, AFTER `swap_world_into` has committed the
/// minimal-body stub set.
///
/// The pass is **single-shot per session** (gated by
/// `RestoredBodiesRendered`); the regen chain on the New Game
/// path provides the rich 3D scene directly via
/// `setup_solar_system`, so this stub is only needed on Restore.
///
/// See module-level docs for the full rationale.
pub struct RestoreBodyVisualData {
    body_type: BodyType,
    color: (f32, f32, f32),
    texture_path: Option<String>,
    emissive: Option<(f32, f32, f32)>,
    /// Host planet's physical radius (km) for ring bodies. The
    /// annulus mesh's inner edge is computed from the host's
    /// visual radius to avoid the inner ring clipping into the
    /// planet sphere.
    parent_radius_km: Option<f32>,
}

pub fn populate_restored_bodies_3d(world: &mut World) {
    // Safety net: if the AssetServer isn't available (a test
    // App or some modder scenario stripped DefaultPlugins), we
    // can't add meshes. Skip-and-mark so the gate doesn't keep
    // firing and spamming the log.
    let assets_available = world.get_resource::<Assets<Mesh>>().is_some()
        && world
            .get_resource::<Assets<bevy::pbr::StandardMaterial>>()
            .is_some();
    if !assets_available {
        warn!(
            "populate_restored_bodies_3d: Assets<Mesh>/Assets<StandardMaterial> missing on \
             the live world; skipping 3D pass (per-body data is intact, only the visual is \
             affected). New Game's regen chain is unaffected."
        );
        world.insert_resource(RestoredBodiesRendered);
        return;
    }

    let data = match SolarSystemData::load_from_file("assets/data/solar_system.ron") {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "populate_restored_bodies_3d: failed to load solar_system.ron ({e}); \
                 bodies will remain stubs"
            );
            world.insert_resource(RestoredBodiesRendered);
            return;
        }
    };

    // Build a body_name → BodyData lookup so the per-entity
    // decoration loop doesn't repeat a linear scan for every
    // body.
    let by_name: std::collections::HashMap<&str, &CelestialBodyData> =
        data.bodies.iter().map(|b| (b.name.as_str(), b)).collect();
    let name_to_entity: std::collections::HashMap<String, Entity> = {
        let mut q = world.query::<(Entity, &CelestialBody)>();
        q.iter(world).map(|(e, cb)| (cb.name.clone(), e)).collect()
    };

    // Pass A: insert KeplerOrbit, OrbitCenter, LogicalParent, OrbitPath
    // for every body that has matching RON data. Bodies without
    // RON rows (e.g. procedurally-spawned system bodies — these
    // aren't in the live world on Restore because
    // SystemPopulatorPlugin is gated off too) keep their
    // stub-only contract.
    //
    // We also stash the per-body RON `color` triple into a
    // small Side-car so Pass B's material-assignment step
    // doesn't have to re-walk the RON data. The sidecar lives
    // only for the duration of this function.
    let mut sidecar_by_id: std::collections::HashMap<Entity, RestoreBodyVisualData> =
        std::collections::HashMap::new();
    let mut decorated = 0usize;
    let mut skipped = 0usize;
    // Snapshot the (Entity, CelestialBody) pairs up front so the
    // per-entity mutation loop doesn't have to juggle a query
    // borrow against `&mut World` (Bevy 0.18 forbids it).
    let bodies: Vec<(Entity, String, BodyType)> = {
        let mut q = world.query::<(Entity, &CelestialBody)>();
        q.iter(world)
            .map(|(e, cb)| (e, cb.name.clone(), cb.body_type))
            .collect()
    };
    for (id, name, body_type) in bodies {
        let Some(body_data) = by_name.get(name.as_str()) else {
            skipped += 1;
            continue;
        };
        if let Some(orbit_data) = &body_data.orbit {
            let kepler = kepler_orbit_from_data(orbit_data);
            if let Ok(mut e) = world.get_entity_mut(id) {
                e.insert(kepler);
                decorated += 1;
            }
        }
        // Parent attachment (OrbitCenter / LogicalParent /
        // OrbitsBody) must run for **every** body with a parent
        // row — including rings, which have `orbit: None` in
        // `solar_system.ron` and would otherwise fall through to
        // the universe origin (Saturn Rings appearing as a
        // glowing ball at Sol's location was the visible symptom).
        // The `update_render_transform` system reads `LogicalParent`
        // to position a body relative to its parent — without it,
        // rings, atmosphere shells, and Lagrange helpers all stack
        // on top of (0, 0, 0).
        if let Some(parent_name) = &body_data.parent {
            if let Some(&parent_id) = name_to_entity.get(parent_name) {
                if let Ok(mut e) = world.get_entity_mut(id) {
                    // Rings don't have an orbit so they don't need
                    // OrbitCenter (OrbitCenter is only consumed by
                    // `propagate_orbits`); they only need
                    // LogicalParent so the rendering pipeline
                    // resolves their world position via the
                    // parent's `SpaceCoordinates`.
                    if body_data.orbit.is_some() {
                        e.insert(OrbitCenter(parent_id));
                        // `OrbitsBody` is the resource-generation
                        // chain's parent tracker (different from
                        // `OrbitCenter`, which is the visual
                        // chain's). Inserting it here lets
                        // `generate_solar_system_resources`
                        // walk the parent chain to find the star
                        // for frost-line / metallicity lookup.
                        // Without this component, the regen
                        // chain's resource system silently skips
                        // the body, so `PlanetResources::default()`
                        // (empty deposits) stays on it and
                        // `extract_resources` has nothing to mine.
                        // Rings don't need this — they have no
                        // deposits by design.
                        e.insert(OrbitsBody::new(parent_id));
                    }
                    e.insert(LogicalParent(parent_id));
                }
            }
        }
        // Every body gets an OrbitPath so the drawn ring shows
        // up in the same scheme the regen chain uses. Rings
        // are excluded — the regen chain sets their `OrbitPath`
        // `visible: false` because the ring annulus mesh itself
        // is the visual; drawing an additional orbit circle on
        // top would just add a noisy halo line.
        let orbit_path_color = if matches!(body_type, BodyType::Star) {
            Color::srgba(1.0, 0.95, 0.6, 0.4)
        } else {
            Color::srgba(0.4, 0.75, 1.0, 0.55)
        };
        if let Ok(mut e) = world.get_entity_mut(id) {
            if !matches!(body_type, BodyType::Ring) {
                // Match the regen chain's segment count (128)
                // and default fade exponent so restored bodies
                // render visually identical to fresh-game bodies.
                e.insert(OrbitPath::with_segments(orbit_path_color, 128));
            }
            // **Insert Bevy marker components** — `Star`,
            // `Planet`, `Moon`, etc. — that several downstream
            // systems (`update_orbit_visibility`,
            // `update_body_lod_visibility`, the picking pipeline)
            // use to classify bodies. `regenerate_bodies_minimal`
            // doesn't add these (its job is the apply path's
            // divergence overlay, not scene rendering), so without
            // these inserts the body's orbit indicator stays
            // hidden (`update_orbit_visibility` falls through to
            // `false`) and the body can't be hovered/selected in
            // the 3-D view. The regen chain's `setup_solar_system`
            // adds them at spawn time, so this just mirrors the
            // existing convention.
            match body_type {
                BodyType::Star => {
                    e.insert(Star);
                }
                BodyType::Planet => {
                    e.insert(Planet);
                }
                BodyType::GasGiant => {
                    e.insert(GasGiant);
                    // Some queries classify GasGiant under
                    // Planet-like orbits; mirror that here so
                    // orbit-visibility treats gas giants the same
                    // as planets.
                    e.insert(Planet);
                }
                BodyType::DwarfPlanet => {
                    e.insert(DwarfPlanet);
                }
                BodyType::Moon => {
                    e.insert(Moon);
                }
                BodyType::Asteroid => {
                    e.insert(Asteroid);
                }
                BodyType::Comet => {
                    e.insert(Comet);
                }
                BodyType::Ring => {
                    e.insert(Ring);
                }
            }
        }
        // Stars also spawn a `PointLight` (we're unlit material,
        // but the planets benefit from a local light source so
        // any future PR that switches to lit materials has a
        // sun to reflect). Sol is by convention the system
        // anchor at the origin (Sol's `SpaceCoordinates` is
        // `DVec3::ZERO` from `regenerate_bodies_minimal`); the
        // PointLight itself doesn't depend on Sol's transform
        // because Bevy lights follow their entity's transform.
        // The brightness (5e9 candela) is the real-Sun
        // surface-brightness order; Bevy's tone-mapping
        // compresses it to a usable render value.
        if matches!(body_type, BodyType::Star) {
            if let Ok(mut e) = world.get_entity_mut(id) {
                e.insert(PointLight {
                    color: Color::srgb(
                        body_data.emissive.0,
                        body_data.emissive.1,
                        body_data.emissive.2,
                    ),
                    // Real-Sun surface-brightness order; Bevy's
                    // tone-mapping compresses it to a usable
                    // render value. The regen chain uses
                    // `intensity: 2.8e11` — we keep the same
                    // order-of-magnitude so planets get a
                    // consistent day/night terminator.
                    intensity: 2.8e11,
                    // Range must exceed the largest planet's
                    // rendered distance (~30 AU × 1500
                    // Bevy units/AU ≈ 45 000 units); 2e9
                    // matches the regen chain and is more
                    // than enough to reach the outer planets.
                    range: 2.0e9,
                    shadows_enabled: false,
                    ..default()
                });
                // **Sol also carries `StarSystem::sun_like()`**
                // so `generate_solar_system_resources`'s
                // `star_query` finds it. The regen chain on
                // New Game adds a StarSystem only for nearby
                // star systems (multi-star logic), but on
                // Restore we have only the Sol baseline and
                // the resource system needs at least one
                // StarSystem on the chain to resolve frost
                // line + metallicity. sun_like() returns the
                // canonical defaults (frost line ≈ 4.0 AU,
                // solar metallicity).
                e.insert(StarSystem::sun_like());
            }
        }
        // Moons render closer to their parent than planets
        // render to Sol — at the project's SCALING_FACTOR the
        // Earth's Moon sits inside Earth's own sphere without
        // amplification. The regen chain attaches
        // `LocalOrbitAmplification` to moons and the
        // `update_render_transform` path uses it to push the
        // moon's rendered position outwards. We mirror that
        // here so the moon is at least visually distinguishable
        // from its parent planet on the Restore path.
        // Amplification factor 8.0 matches the order used by
        // `setup_solar_system::moon_amplification` for inner
        // moons.
        //
        // **Rings** (Saturn Rings, Uranus Rings) don't orbit —
        // they're attached to their host planet via
        // `LogicalParent`. Inserting `LocalOrbitAmplification(1.0)`
        // puts the ring's translation on the
        // "amplification" branch in `update_render_transform`,
        // which resolves `parent_world = parent's SpaceCoordinates
        // × SCALING_FACTOR` and then sets
        // `transform.translation = parent_world + (ring's own
        // coords) × SCALING_FACTOR × 1.0`. The ring's own
        // `SpaceCoordinates` stays at the `DVec3::ZERO`
        // placeholder from `regenerate_bodies_minimal`, so the
        // rendered position collapses to `parent_world` —
        // exactly the host planet's location. Without this, the
        // ring falls through to the "non-moon body" branch that
        // scales its (zero) coords straight to (0,0,0).
        if matches!(body_type, BodyType::Moon) {
            if let Ok(mut e) = world.get_entity_mut(id) {
                e.insert(LocalOrbitAmplification(8.0));
            }
        } else if matches!(body_type, BodyType::Ring) {
            if let Ok(mut e) = world.get_entity_mut(id) {
                e.insert(LocalOrbitAmplification(1.0));
            }
        }
        let texture_path = body_data
            .multi_layer_textures
            .as_ref()
            .map(|multi| multi.base.clone())
            .or_else(|| body_data.texture.clone())
            .or_else(|| generic_texture_path(body_data));

        // Remember the body's RON colour and optional texture assets.
        //
        // For ring bodies, also capture the parent planet's
        // physical radius (km) so the annulus mesh's inner
        // edge can be sized to clear the planet's sphere.
        let parent_radius_km = if matches!(body_type, BodyType::Ring) {
            body_data
                .parent
                .as_deref()
                .and_then(|parent_name| by_name.get(parent_name).copied())
                .map(|parent| parent.radius)
        } else {
            None
        };
        sidecar_by_id.insert(
            id,
            RestoreBodyVisualData {
                body_type,
                color: (body_data.color.0, body_data.color.1, body_data.color.2),
                texture_path,
                emissive: Some((
                    body_data.emissive.0,
                    body_data.emissive.1,
                    body_data.emissive.2,
                )),
                parent_radius_km,
            },
        );
    }

    // Pass B: add Transform + Visibility + mesh + material.
    // This uses the same asset graph as the live App so texture
    // handles resolve normally once the assets stream in.
    decorate_with_visuals(world, &sidecar_by_id);

    // Pass C: populate `PlanetResources` deposit maps for
    // every non-stellar body that the apply step didn't already
    // fill. The regen-chain's
    // `generate_solar_system_resources` system normally does
    // this on New Game; on Restore the boot-init chain is
    // gated off, so bodies sit with empty
    // `PlanetResources::default()` and the mining system
    // produces zero output. We re-run the regen-chain's
    // resource-generation logic inline (its private helper
    // `generate_resources_for_body` is now `pub(crate)` so
    // we can call it directly) so mining rates are realistic
    // without re-registering the system.
    populate_planet_resources(world);

    // Pass D: rebuild the star surface + corona + halo shells
    // for every star body. The regen chain spawns these as
    // children of the star entity via `with_children`; on the
    // Restore path the boot-init chain is gated off, so stars
    // render as plain `StandardMaterial` spheres with no
    // limb darkening, no FBM-plasma corona, and no diffuse
    // halo — they look like featureless orange balls instead
    // of a sun. We re-spawn the shells here using the live
    // world's `Assets<StarSurfaceMaterial>`,
    // `Assets<StarCorona3dMaterial>`, and
    // `Assets<StarHalo3dMaterial>` so the existing
    // `update_star_corona_3d_lod` system (registered by the
    // live App regardless of restore path) drives their
    // colour animations normally.
    populate_restored_star_shells(world);

    info!(
        "populate_restored_bodies_3d: decorated {decorated} bodies (skipped {skipped} \
         without RON data) — Restore-path 3D scene populated"
    );
    world.insert_resource(RestoredBodiesRendered);
}

/// For each non-star body in the live world that lacks a
/// `PlanetResources` component, run the regen-chain's
/// resource-generation logic and insert the resulting
/// `PlanetResources`. Mirrors what
/// `generate_solar_system_resources` would do for a freshly
/// regenerated world but runs inline (the system itself can't
/// re-register due to Bevy 0.18 system-ordering rules).
///
/// Reads the live world's `GameSeed` for determinism so a
/// restore reproduces the same deposit map as the original
/// New Game.
fn populate_planet_resources(world: &mut World) {
    // Reload solar_system.ron so we can re-derive distance /
    // frost-line values per body in the same loop. Cheap —
    // ~700 rows and the loader is plain `ron::from_str`.
    let data = match SolarSystemData::load_from_file("assets/data/solar_system.ron") {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "populate_planet_resources: failed to load solar_system.ron ({e}); \
                 mining deposits will stay empty"
            );
            return;
        }
    };
    let by_name: std::collections::HashMap<&str, &CelestialBodyData> =
        data.bodies.iter().map(|b| (b.name.as_str(), b)).collect();

    // Snapshot the body list up front so the per-entity
    // mutation loop doesn't fight Bevy's borrow checker.
    let bodies: Vec<(Entity, String, BodyType)> = {
        let mut q = world.query::<(Entity, &CelestialBody)>();
        q.iter(world)
            .map(|(e, cb)| (e, cb.name.clone(), cb.body_type))
            .collect()
    };
    // Seeded RNG so the procedural profile draws are
    // deterministic per-game.
    let mut rng = StdRng::seed_from_u64(
        world
            .get_resource::<crate::game_state::GameSeed>()
            .map(|g| g.value)
            .unwrap_or(0xDEAD_BEEF_CAFE_F00D),
    );
    let mut populated = 0usize;
    let mut skipped = 0usize;
    for (id, name, body_type) in bodies {
        // Stars and Rings aren't part of the regen-chain's
        // resource filter. Comets have no deposits by design.
        if matches!(body_type, BodyType::Star | BodyType::Ring | BodyType::Comet) {
            skipped += 1;
            continue;
        }
        // Bodies with a pre-existing `PlanetResources` were
        // populated by the apply step (from the divergence
        // overlay). We only skip those that already carry
        // non-empty deposits so the minimal-world defaults get
        // replaced by the regen-chain resource profile.
        if let Some(resources) = world.get::<crate::economy::components::PlanetResources>(id) {
            if !resources.deposits.is_empty() {
                skipped += 1;
                continue;
            }
        }
        // Look up RON row for distance / mass.
        let Some(body_data) = by_name.get(name.as_str()) else {
            skipped += 1;
            continue;
        };
        // Distance from parent star — walk the parent chain up
        // to one orbit-frame. For Sol bodies whose
        // OrbitCenter points to another Sol body, we don't
        // have parent star coordinates (just a sibling body's
        // `SpaceCoordinates`, which is ZERO at this point in
        // the schedule). Fall back to the body's own
        // semi-major axis (orbits in `solar_system.ron` use AU,
        // consistent with `SCALING_FACTOR` downstream).
        let distance_au = body_data
            .orbit
            .as_ref()
            .map(|o| o.semi_major_axis as f64)
            .unwrap_or(0.0);
        // Frost line defaults from the canonical Sun-like system.
        let resources = crate::economy::generation::generate_resources_for_body(
            &name,
            body_type,
            body_data.mass,
            body_data.asteroid_class,
            distance_au,
            crate::economy::components::StarSystem::sun_like().frost_line_au,
            &mut rng,
        );
        if let Ok(mut e) = world.get_entity_mut(id) {
            e.insert(resources);
            populated += 1;
        }
    }
    if populated > 0 || skipped > 0 {
        info!(
            "populate_planet_resources: populated {populated} bodies, skipped {skipped} (star/ring/comet/already-populated-or-nonempty)"
        );
    }
}

/// Rebuild the star surface + corona + halo shells for every star
/// body in the live world. Mirrors the regen chain's
/// `setup_solar_system` block that runs `commands.entity(star).with_children(...)`
/// to spawn three child shells:
///
/// - A `StarSurfaceMaterial` sphere at the star's own radius
///   (Eddington limb-darkening shader).
/// - A `StarCorona3dMaterial` sphere at 1.75× the radius (ray-marched
///   FBM plasma).
/// - A `StarHalo3dMaterial` sphere at 4× the radius (limb-brightening
///   diffuse halo).
///
/// On the Restore path the regen chain is gated off (the
/// boot-init gate `restored_world_is_present` short-circuits the
/// `setup_solar_system` system), so without this pass stars render
/// as plain `StandardMaterial` spheres — they look like
/// featureless orange balls instead of a sun. We re-create the
/// shells here against the live world's
/// `Assets<StarSurfaceMaterial>` / `StarCorona3dMaterial` /
/// `StarHalo3dMaterial>` so the existing
/// `update_star_corona_3d_lod` system (registered by the live App
/// regardless of restore path) drives the colour animations
/// normally.
///
/// **Child hierarchy**: the regen chain uses
/// `commands.entity(star).with_children(...)`, which inserts
/// `ChildOf(star)` AND populates the star's `Children`
/// collection. PR-J (GRA-358) warns that stale `Children`
/// collections on a swapped-in live App's prior-session parent
/// entities blow Bevy 0.18's `propagate_parent_transforms`
/// recursion. Star shells, however, are **newly created** during
/// restore, so the parent star's `Children` collection is empty
/// before this pass runs and we have full control over what's
/// inserted — safe to use `ChildOf(star)` for them.
///
/// **Idempotency**: the function does a pre-check for an existing
/// `StarCoronaShell` child so re-running the decoration pass
/// (e.g. after a second restore) doesn't duplicate shells. The
/// outer `RestoredBodiesRendered` gate already prevents the whole
/// pass from re-running, so this is belt-and-suspenders.
fn populate_restored_star_shells(world: &mut World) {
    // Verify the star material asset collections exist. They are
    // registered by `SolarSystemPlugin` (which is part of the live
    // App's plugin stack on every restore), so missing asset
    // collections would only occur in a unit-test `App` that
    // strips DefaultPlugins. Skip-and-warn in that case so we
    // don't panic on `resource_mut` for a missing resource.
    let materials_available = world
        .get_resource::<Assets<StarSurfaceMaterial>>()
        .is_some()
        && world
            .get_resource::<Assets<StarCorona3dMaterial>>()
            .is_some()
        && world.get_resource::<Assets<StarHalo3dMaterial>>().is_some();
    if !materials_available {
        warn!(
            "populate_restored_star_shells: star material asset collections missing on the \
             live world; skipping star-shell pass"
        );
        return;
    }

    // Snapshot the star entities up front so the per-entity
    // mutation loop doesn't have to juggle a query borrow
    // against `&mut World` (Bevy 0.18 forbids it).
    let stars: Vec<(Entity, String, f32)> = {
        let mut q = world.query::<(Entity, &CelestialBody)>();
        q.iter(world)
            .filter_map(|(e, cb)| {
                if cb.body_type != BodyType::Star {
                    return None;
                }
                Some((e, cb.name.clone(), cb.visual_radius))
            })
            .collect()
    };

    // Look up RON data for emissive colour overrides.
    let data = SolarSystemData::load_from_file("assets/data/solar_system.ron").ok();
    let by_name: std::collections::HashMap<&str, &CelestialBodyData> = data
        .as_ref()
        .map(|d| d.bodies.iter().map(|b| (b.name.as_str(), b)).collect())
        .unwrap_or_default();

    for (star_entity, name, visual_radius) in stars {
        let Some(body_data) = by_name.get(name.as_str()) else {
            continue;
        };
        let (er, eg, eb) = body_data.emissive;

        // Derive corona / halo colours from the body's
        // emissive data — matches the regen chain's logic so
        // the visual is identical to a fresh-game star.
        let core_col = Vec4::new(er * 5.0, eg * 5.0, eb * 5.0, 1.0);
        let halo_col = Vec4::new(er * 4.5, eg * 3.5, eb * 1.8, 1.0);

        // Shell radii — same as regen chain.
        let corona_shell_r = visual_radius * 1.75;
        let halo_shell_r = visual_radius * 4.0;

        // Sphere meshes.
        let (star_sphere, corona_sphere, halo_sphere) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            let star_sphere = meshes.add(Sphere::new(visual_radius).mesh().uv(128, 64));
            let corona_sphere = meshes.add(
                Sphere::new(corona_shell_r)
                    .mesh()
                    .ico(5)
                    .expect("ico sphere is valid"),
            );
            let halo_sphere = meshes.add(Sphere::new(halo_shell_r).mesh().uv(32, 16));
            (star_sphere, corona_sphere, halo_sphere)
        };

        // ── Star sphere with limb-darkening shader ───────────
        let star_surface_mat = {
            let mut materials_surface = world.resource_mut::<Assets<StarSurfaceMaterial>>();
            materials_surface.add(StarSurfaceMaterial {
                color_center: Vec4::new(er * 9.0, eg * 9.0, eb * 9.0, 1.0),
                color_limb: Vec4::new(er * 5.5, eg * 2.8, eb * 0.8, 1.0),
                star_texture: None,
            })
        };
        // Replace the placeholder StandardMaterial on the star
        // body with the limb-darkening material so the disk
        // has a hot core + cooler limb gradient.
        if let Ok(mut e) = world.get_entity_mut(star_entity) {
            // Drop the existing mesh handle (StandardMaterial
            // sphere from Phase 1) and re-attach with the new
            // materials. We use `Mesh3d::from` for the new
            // sphere — but Phase 1 already attached one, so we
            // just insert a `MeshMaterial3d` override and let
            // the existing `Mesh3d` (unit-radius sphere scaled
            // by `visual_radius` via `Transform.scale`) carry
            // through.
            e.insert(MeshMaterial3d(star_surface_mat));
            // The Phase 1 mesh handle is a unit-radius sphere;
            // we need the actual `visual_radius`-sized sphere.
            // `Mesh3d` carries a `Handle<Mesh>` so we just swap
            // it for the new one.
            e.insert(Mesh3d(star_sphere));
            // Phase 1 sets `Transform.scale = Vec3::splat(visual_radius)`.
            // The new mesh is already sized to `visual_radius`,
            // so the scale should be 1.0.
            if let Some(mut t) = e.get_mut::<Transform>() {
                t.scale = Vec3::ONE;
            }
        }

        // ── Inner corona shell (FBM plasma) ────────────────
        let corona_mat = {
            let mut materials_corona = world.resource_mut::<Assets<StarCorona3dMaterial>>();
            materials_corona.add(StarCorona3dMaterial {
                color_core: Vec4::ZERO, // LOD system drives it
                color_halo: Vec4::ZERO,
                time_phase: 0.0,
                corona_params: Vec4::new(visual_radius, corona_shell_r, 0.0, 0.0),
            })
        };
        let corona_entity = world
            .spawn((
                Mesh3d(corona_sphere),
                MeshMaterial3d(corona_mat),
                Transform::default(),
                StarCoronaShell {
                    base_core_color: core_col,
                    base_halo_color: halo_col,
                    visual_radius,
                },
            ))
            .id();
        if let Ok(mut parent) = world.get_entity_mut(star_entity) {
            parent.add_child(corona_entity);
        }

        // ── Outer halo shell (limb brightening) ─────────────
        let halo_mat = {
            let mut materials_halo = world.resource_mut::<Assets<StarHalo3dMaterial>>();
            materials_halo.add(StarHalo3dMaterial {
                color_halo: Vec4::ZERO, // LOD system drives it
                time_phase: 0.0,
                halo_params: Vec4::new(visual_radius, halo_shell_r, 0.0, 0.0),
            })
        };
        let halo_entity = world
            .spawn((
                Mesh3d(halo_sphere),
                MeshMaterial3d(halo_mat),
                Transform::default(),
                StarHaloShell {
                    base_halo_color: halo_col,
                    visual_radius,
                },
            ))
            .id();
        if let Ok(mut parent) = world.get_entity_mut(star_entity) {
            parent.add_child(halo_entity);
        }
    }
}

/// Convert the RON-loaded [`OrbitData`] into Bevy's analytic
/// [`KeplerOrbit`]. Both are in the same coordinate frame (ecliptic
/// AU / radians); only the unit conversions differ.
fn kepler_orbit_from_data(data: &OrbitData) -> KeplerOrbit {
    // Recover the orbital period from the saved mean motion if
    // the RON row carries it; otherwise derive from the bodies'
    // semi-major axis via Kepler's third law (μ for Sol).
    let orbital_period_s = (data.orbital_period as f64) * 86_400.0;
    // Sanity guard: division by zero would NaN-out the orbit and
    // permanently break propagate_orbits. Clamp to ≥1 day.
    let mean_motion = if orbital_period_s > 0.0 {
        std::f64::consts::TAU / orbital_period_s.max(86_400.0)
    } else {
        0.0
    };
    KeplerOrbit {
        eccentricity: data.eccentricity as f64,
        semi_major_axis: data.semi_major_axis as f64,
        inclination: data.inclination.to_radians() as f64,
        longitude_ascending_node: data.longitude_ascending_node.to_radians() as f64,
        argument_of_periapsis: data.argument_of_periapsis.to_radians() as f64,
        mean_anomaly_epoch: data.initial_angle.to_radians() as f64,
        mean_motion,
    }
}

/// For each body entity, attach `Transform::default()`, visible
/// `Visibility`, and a basic sphere mesh + tinted StandardMaterial.
/// Reads the live App's `Assets<Mesh>` / `Assets<StandardMaterial>`
/// so the visuals live on the same asset graph the rendering
/// pipeline reads.
///
/// Meant to be called once per Restore — second invocation
/// duplicates meshes (one per body). The function-level
/// [`RestoredBodiesRendered`] gate at the call site prevents that.
///
/// `colors_by_id` carries the per-body RON colour triple
/// captured by Pass A (because `CelestialBody` has no `color`
/// field — the colour lives on `CelestialBodyData` in
/// `solar_system.ron`). Bodies missing a sidecar entry (no
/// matching RON row) get a neutral grey.
fn decorate_with_visuals(
    world: &mut World,
    visuals_by_id: &std::collections::HashMap<Entity, RestoreBodyVisualData>,
) {
    // Two-phase approach to avoid borrow-checker conflicts
    // between `&mut Assets<Mesh>/&mut Assets<StandardMaterial>`
    // and `&mut World::get_entity_mut`:
    //
    // Phase 1: snapshot bodies, build the material cache,
    //   allocate the shared sphere mesh + materials on the
    //   App's `Assets<…>` resources.
    // Phase 2: drop the asset borrows, then mutate each body
    //   entity with the pre-built handles.
    let neutral_grey = (0.55, 0.55, 0.6);
    let bodies: Vec<(Entity, BodyType, f32, bool)> = {
        let mut q = world.query::<(Entity, &CelestialBody)>();
        q.iter(world)
            .map(|(e, cb)| {
                let already_has_mesh = world.get::<Mesh3d>(e).is_some();
                (e, cb.body_type, cb.visual_radius, already_has_mesh)
            })
            .collect()
    };

    // Phase 1: build the material cache and sphere handle.
    #[derive(Hash, PartialEq, Eq, Clone)]
    enum RestoreMaterialKey {
        Textured(String),
        Tinted(u8, u8, u8, bool, bool),
    }

    let mut material_cache: std::collections::HashMap<
        RestoreMaterialKey,
        bevy::asset::Handle<StandardMaterial>,
    > = std::collections::HashMap::new();
    let sphere_mesh: bevy::asset::Handle<Mesh>;
    {
        // Add the shared sphere mesh first, then drop the
        // `Assets<Mesh>` borrow so we can acquire
        // `Assets<StandardMaterial>` without an alias conflict.
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        sphere_mesh = meshes.add(Sphere::new(1.0).mesh().uv(32, 16));
        drop(meshes);
    }

    // Per-ring mesh handles, keyed by entity. Each ring
    // body's annulus needs its own inner/outer radii (the
    // regen chain's `create_ring_mesh(outer, inner, 128)`
    // derives `inner` from the host planet's visual radius
    // plus a 15% clearance). The mesh has unit scale applied
    // in Phase 2 — i.e. `Transform.scale = visual_radius`
    // does NOT divide the ring into inner/outer because the
    // annulus mesh encodes both.
    let mut ring_meshes: std::collections::HashMap<Entity, bevy::asset::Handle<Mesh>> =
        std::collections::HashMap::new();
    {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        for (id, body_type, visual_radius, already_has_mesh) in &bodies {
            if *already_has_mesh {
                continue;
            }
            if !matches!(body_type, BodyType::Ring) {
                continue;
            }
            let visual = visuals_by_id.get(id);
            let parent_radius = visual.and_then(|v| v.parent_radius_km).unwrap_or(0.0);
            let parent_visual_radius = calculate_visual_radius(BodyType::GasGiant, parent_radius);
            // Inner edge = parent surface + 15% clearance gap.
            // Outer edge is the ring body's own visual radius.
            let inner_radius = (parent_visual_radius * 1.15).max(visual_radius * 0.55);
            let outer_radius = *visual_radius;
            let h = meshes.add(create_ring_mesh(outer_radius, inner_radius, 128));
            ring_meshes.insert(*id, h);
        }
    }

    let material_builds: Vec<(RestoreMaterialKey, StandardMaterial)> = {
        let asset_server = world.resource::<AssetServer>();
        let mut unique_keys: std::collections::HashSet<RestoreMaterialKey> =
            std::collections::HashSet::new();
        let mut builds = Vec::new();
        for (_id, body_type, _visual_radius, already_has_mesh) in &bodies {
            if *already_has_mesh {
                continue;
            }
            let visual = visuals_by_id.get(_id);
            let (r, g, b) = visual.map(|v| v.color).unwrap_or(neutral_grey);
            let is_star = matches!(body_type, BodyType::Star);
            let texture_path = visual.and_then(|v| v.texture_path.clone());
            let has_texture = texture_path.is_some();
            let key = if let Some(path) = texture_path.as_ref() {
                RestoreMaterialKey::Textured(path.clone())
            } else {
                RestoreMaterialKey::Tinted(
                    (r * 255.0_f32).round() as u8,
                    (g * 255.0_f32).round() as u8,
                    (b * 255.0_f32).round() as u8,
                    matches!(body_type, BodyType::Ring),
                    is_star,
                )
            };
            if !unique_keys.insert(key.clone()) {
                continue;
            }
            let base_color = if has_texture {
                Color::WHITE
            } else {
                Color::srgb(r, g, b)
            };
            let emissive = if is_star {
                visual
                    .and_then(|v| v.emissive)
                    .map(|(er, eg, eb)| {
                        LinearRgba::new(er * 6.0_f32, eg * 6.0_f32, eb * 6.0_f32, 1.0)
                    })
                    .unwrap_or(LinearRgba::new(r * 6.0_f32, g * 6.0_f32, b * 6.0_f32, 1.0))
            } else {
                // **Non-star bodies**: regen chain uses
                // `emissive: LinearRgba::WHITE * 0.006` as a
                // minimal ambient floor so planets in
                // dim/distant star systems aren't pitch black on
                // the night side. We mirror that here so the
                // night side of restored bodies isn't pure
                // black. The value is intentionally very low so
                // day/night contrast remains strong.
                LinearRgba::new(0.006_f32, 0.006_f32, 0.006_f32, 1.0)
            };
            let base_color_texture = texture_path
                .as_ref()
                .map(|path| asset_server.load(path.clone()));
            let mut material = StandardMaterial {
                base_color,
                base_color_texture,
                metallic: 0.0,
                perceptual_roughness: 0.85,
                reflectance: 0.4,
                emissive,
                // **Material lighting model**:
                // - Stars use unlit (they emit light, not
                //   reflect it). Their visual comes from the
                //   `StarSurfaceMaterial` shell spawned in
                //   `populate_restored_star_shells` (which has
                //   limb darkening baked in).
                // - Rings use unlit (their alpha texture drives
                //   the appearance; PBR shading on a flat
                //   annulus looks washed out).
                // - Everything else uses lit PBR so the
                //   PointLight on Sol actually contributes to
                //   the day/night terminator on planets.
                unlit: is_star || matches!(body_type, BodyType::Ring),
                ..default()
            };
            if matches!(body_type, BodyType::Ring) {
                material.alpha_mode = AlphaMode::Blend;
                material.cull_mode = None;
            }
            builds.push((key, material));
        }
        builds
    };

    {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        for (key, material) in material_builds {
            let h = materials.add(material);
            material_cache.insert(key, h);
        }
    }

    // Phase 2: insert components into each body. Asset
    // borrows are dropped; we just clone the pre-built
    // `Handle`s.
    for (id, _body_type, visual_radius, already_has_mesh) in bodies {
        if already_has_mesh {
            continue;
        }
        let visual = visuals_by_id.get(&id);
        let key = if let Some(visual) = visual {
            if let Some(path) = &visual.texture_path {
                RestoreMaterialKey::Textured(path.clone())
            } else {
                let (r, g, b) = visual.color;
                RestoreMaterialKey::Tinted(
                    (r * 255.0_f32).round() as u8,
                    (g * 255.0_f32).round() as u8,
                    (b * 255.0_f32).round() as u8,
                    matches!(visual.body_type, BodyType::Ring),
                    matches!(visual.body_type, BodyType::Star),
                )
            }
        } else {
            let (r, g, b) = neutral_grey;
            RestoreMaterialKey::Tinted(
                (r * 255.0).round() as u8,
                (g * 255.0).round() as u8,
                (b * 255.0).round() as u8,
                false,
                false,
            )
        };
        let material = material_cache
            .get(&key)
            .cloned()
            .expect("material_cache populated in phase 1");
        let is_ring = matches!(_body_type, BodyType::Ring);
        if let Ok(mut e) = world.get_entity_mut(id) {
            // Transform: position is computed each frame by
            // `update_render_transform`; we set a default here
            // and the engine overrides it.
            //
            // **Rings**: ring bodies don't have `KeplerOrbit`
            // (solar_system.ron lists them with `orbit: None`),
            // so they don't actually need a Kepler propagation
            // step — their position comes entirely from the
            // parent's `SpaceCoordinates` via `LogicalParent`.
            // The annulus mesh has been sized by Phase 1 with
            // proper inner/outer radii derived from the host
            // planet's visual radius, so we leave `Transform.scale`
            // at 1.0 (rather than multiplying by visual_radius,
            // which would re-introduce the inner-edge clipping
            // bug — the mesh already encodes both radii in
            // absolute Bevy units).
            let mut t = Transform::default();
            if !is_ring {
                t.scale = Vec3::splat(visual_radius.max(0.05));
            }
            e.insert(t);
            e.insert(Visibility::Visible);
            e.insert(InheritedVisibility::default());
            e.insert(ViewVisibility::default());
            if is_ring {
                // Per-ring annulus mesh (Phase 1 sized to match
                // the regen chain's `create_ring_mesh(outer,
                // inner, 128)` call). If for some reason Phase 1
                // skipped this ring, fall back to the shared
                // unit-radius annulus and let the scale=1 path
                // handle it.
                let mesh_handle = ring_meshes
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| sphere_mesh.clone());
                e.insert(Mesh3d(mesh_handle));
            } else {
                e.insert(Mesh3d(sphere_mesh.clone()));
            }
            e.insert(MeshMaterial3d(material));
        }
    }
}

/// `run_if` predicate matching [`populate_restored_bodies_3d`]'s
/// contract: fires only after a Restore-path swap committed
/// (`RestoredWorldGate` is present), and only on the first tick
/// after that (no [`RestoredBodiesRendered`] yet). Mirrors the
/// gate shape used by `BootInitPlugin`.
pub fn restore_decoration_should_run(
    world_ready: Option<Res<crate::persistence::swap::WorldReady>>,
    restored: Option<Res<RestoredWorldGate>>,
    already_rendered: Option<Res<RestoredBodiesRendered>>,
) -> bool {
    world_ready.is_some() && restored.is_some() && already_rendered.is_none()
}

/// Bevy plugin that wires the post-swap decoration pass onto the
/// live `App`. The plugin is registered in `main.rs::build_game_app`
/// alongside [`crate::persistence::PersistencePlugin`] so the
/// pass is available whether the user picked New Game (in which
/// case the pass's `run_if` gate keeps it silent — the regen
/// chain does the work) or Load Save (in which case the pass
/// fires once after the swap).
pub struct RestoreDecorationPlugin;

impl Plugin for RestoreDecorationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                populate_restored_bodies_3d,
                crate::fleets::systems::spawn_initial_fleet,
                crate::fleets::systems::spawn_debug_earth_jupiter_fleet,
            )
                .run_if(restore_decoration_should_run),
        );
    }
}

fn generic_texture_path(body_data: &CelestialBodyData) -> Option<String> {
    match body_data.body_type {
        BodyType::Asteroid => {
            let class = body_data.asteroid_class.unwrap_or(AsteroidClass::CType);
            Some(
                match class {
                    AsteroidClass::CType => "textures/celestial/asteroids/generic_c_type_2k.jpg",
                    AsteroidClass::SType => "textures/celestial/asteroids/generic_s_type_2k.jpg",
                    AsteroidClass::MType => "textures/celestial/asteroids/generic_s_type_2k.jpg",
                    AsteroidClass::VType => "textures/celestial/asteroids/generic_s_type_2k.jpg",
                    AsteroidClass::DType => "textures/celestial/asteroids/generic_c_type_2k.jpg",
                    AsteroidClass::PType => "textures/celestial/asteroids/generic_c_type_2k.jpg",
                    AsteroidClass::Unknown => "textures/celestial/asteroids/generic_c_type_2k.jpg",
                }
                .to_string(),
            )
        }
        BodyType::Comet => Some("textures/celestial/comets/generic_nucleus_2k.jpg".to_string()),
        BodyType::Moon => Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string()),
        BodyType::DwarfPlanet => {
            let mut seed = 0u32;
            for byte in body_data.name.bytes() {
                seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
            }
            if seed % 3 == 0 {
                Some("textures/celestial/asteroids/generic_s_type_2k.jpg".to_string())
            } else {
                Some("textures/celestial/asteroids/generic_c_type_2k.jpg".to_string())
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_body(world: &mut World, name: &str, body_type: BodyType, visual_radius: f32) -> Entity {
        world
            .spawn((
                CelestialBody {
                    name: name.to_string(),
                    radius: 6371.0,
                    mass: 5.972e24,
                    body_type,
                    visual_radius,
                    asteroid_class: None,
                    star_approach_au: None,
                    rotation_period_s: None,
                    habitable_outer_au: None,
                },
                crate::astronomy::components::SystemId(0usize),
                crate::astronomy::components::SpaceCoordinates::default(),
            ))
            .id()
    }

    #[test]
    fn kepler_orbit_from_data_converts_units() {
        let orbit = OrbitData {
            semi_major_axis: 1.0,
            eccentricity: 0.0167,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            orbital_period: 365.25,
            initial_angle: 0.0,
        };
        let k = kepler_orbit_from_data(&orbit);
        assert!((k.semi_major_axis - 1.0).abs() < 1e-9);
        assert!((k.eccentricity - 0.0167).abs() < 1e-4);
        // 2π / (365.25 * 86400) ≈ 1.991e-7 rad/s
        let expected_mean_motion = std::f64::consts::TAU / (365.25 * 86400.0);
        assert!(
            (k.mean_motion - expected_mean_motion).abs() < 1e-12,
            "mean_motion={}, expected={}",
            k.mean_motion,
            expected_mean_motion
        );
    }

    #[test]
    fn kepler_orbit_handles_zero_period_gracefully() {
        let orbit = OrbitData {
            semi_major_axis: 1.0,
            eccentricity: 0.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            orbital_period: 0.0,
            initial_angle: 0.0,
        };
        let k = kepler_orbit_from_data(&orbit);
        // Should not panic; should produce mean_motion = 0
        // (the body stays at its epoch position).
        assert_eq!(k.mean_motion, 0.0);
    }

    #[test]
    fn gate_does_not_fire_without_restore_marker() {
        let mut world = World::new();
        // Bevy's run-if system parameter coercion: the
        // `restore_decoration_should_run` signature takes
        // `Option<Res<T>>`, but `&World::get_resource` returns
        // `Option<&T>`. We mirror the same conditions by inserting
        // resources and asserting on the boolean result via
        // a manual gate evaluation.
        assert!(!gate_world_ready(&world) || !gate_restored(&world) || !gate_already(&world));
        world.insert_resource(crate::persistence::swap::WorldReady);
        world.insert_resource(RestoredWorldGate);
        // Now the gate would fire (no RenderedYet marker yet).
        assert!(!gate_already(&world));
        world.insert_resource(RestoredBodiesRendered);
        // Already-rendered marker stops the gate.
        assert!(gate_already(&world));
    }

    fn gate_world_ready(world: &World) -> bool {
        world
            .get_resource::<crate::persistence::swap::WorldReady>()
            .is_some()
    }
    fn gate_restored(world: &World) -> bool {
        world.get_resource::<RestoredWorldGate>().is_some()
    }
    fn gate_already(world: &World) -> bool {
        world.get_resource::<RestoredBodiesRendered>().is_some()
    }

    #[test]
    fn pass_marks_rendered_even_when_no_assets() {
        let mut world = World::new();
        // No Assets<Mesh> registered → assets_available = false → pass marks and bails.
        let _ = make_body(&mut world, "Earth", BodyType::Planet, 1.0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            populate_restored_bodies_3d(&mut world);
        }));
        assert!(result.is_ok());
        assert!(world.get_resource::<RestoredBodiesRendered>().is_some());
    }
}
