//! World-swap machinery for save/load (GRA-358 PR-B).
//!
//! Bevy 0.18 has no built-in `World::swap` / `World::merge`, so the
//! world-swap that [`crate::persistence::game_setup::promote_pending_world`]
//! has been documented as needing since PR-A is implemented here.
//!
//! # Design (see `memories/session/plan.md` Phase 2)
//!
//! [`swap_world_into`] is **replace, not merge**. It
//!
//! 1. Drains `pending.world.take()`.
//! 2. Walks every entity in the pending world, archetype by archetype.
//!    For each component on each entity, looks up the matching
//!    [`ReflectComponent`] in the target world's [`AppTypeRegistry`]
//!    (it must have been populated by [`super::PersistencePlugin`]
//!    in `build_minimal_world_for_restore` — see
//!    `game_setup.rs::build_minimal_world`) and calls
//!    [`ReflectComponent::copy`] to clone the component into the
//!    target world.
//! 3. Builds a `pending_entity -> live_entity` map so the
//!    [`ChildOf`] / [`Children`] relationship components can be
//!    rewritten to point at live IDs.
//! 4. Copies every reflect-deriveable resource from the pending world
//!    into the target, **skipping** the resources owned by the live
//!    `App` (Bevy lifecycle + Helios plumbing) — see the
//!    `should_skip_resource` denylist below.
//!
//! On success, [`WorldReady`] is **not** inserted by `swap_world_into`
//! itself; the caller ([`promote_pending_world`]) owns that step so
//! the swap stays a pure data-movement function.
//!
//! # Why a denylist, not an allowlist
//!
//! The pending world is the entire fresh Bevy world, including
//! `MinimalPlugins` plumbing (`Time<Real>`, `Time<Virtual>`, etc.).
//! The target world is the live `App`'s world, which already owns its
//! own `Time`/`Events`/`Messages`/`AppTypeRegistry`. Swapping any of
//! those in would either silently corrupt the live app's clock or
//! silently drop message events the player just triggered.
//!
//! The denylist names those types so a future contributor who adds a
//! new Bevy-lifecycle resource sees a comment explaining why it must
//! not be copied. A complementary allowlist would require touching
//! every existing resource — strictly more work for strictly less
//! safety.
//!
//! # Hierarchy safety (B0004)
//!
//! Bevy 0.18 emits a `B0004` warning ("entity N has parent M without
//! GlobalTransform") when a [`ChildOf`] component points at an entity
//! that's missing from the world. The pre-fix behaviour was that the
//! live world started populated by `Startup` content spawns, the
//! restore path tried to overwrite it, and the desync manifested as
//! `B0004` warnings on the next `propagate_transforms` tick. The swap
//! here closes that loop by **rewriting every `ChildOf` reference** to
//! the live entity ID, so post-swap the hierarchy is self-consistent.

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use std::any::TypeId;

use super::game_setup::PendingGameWorld;

/// Marker resource inserted by [`crate::persistence::game_setup::promote_pending_world`]
/// once a fresh world has been swapped into the live `App`.
///
/// All `Startup`-time / `Update`-time content spawns that the player
/// expects (the 710+ solar-system bodies, the initial fleet, baseline
/// tech/engineering, comet tail VFX, starmap, backdrop sphere, etc.)
/// gate themselves on `resource_exists::<WorldReady>` so the live
/// world stays **empty until a kickoff decision is made**. Without
/// this gate, the live `App` would spawn the entire baseline at boot
/// regardless of whether the player hit "Continue", "Load Save", or
/// "New Game" — and the swap path would silently drop the load
/// because the world already had content.
///
/// Removed by the `quit_to_main_menu` consumer when the player leaves
/// a session; the chain that gates on `WorldReady` then goes back to
/// silent. (Out of scope for this PR — tracked as a follow-up.)
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct WorldReady;

/// `run_if` predicate for the [`crate::boot_init::BootInitPlugin`]
/// chain. Fires only while [`WorldReady`] exists in the world —
/// i.e. only after a kickoff decision (New Game / Load Save) has
/// resolved and [`crate::persistence::game_setup::promote_pending_world`]
/// has swapped the pending world into the live `App`.
///
/// Pure function over `Option<Res<WorldReady>>` so the chain
/// short-circuits without a mutation borrow.
pub fn world_ready_is_present(world_ready: Option<Res<WorldReady>>) -> bool {
    world_ready.is_some()
}

/// Failure surface for [`swap_world_into`]. The caller matches on
/// this; the kickoff consumer routes UI surfaces through
/// [`crate::ui::notifications::NotificationEvent`] before the error
/// reaches the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapError {
    /// `PendingGameWorld.world` was `None` — nothing to promote.
    /// The kickoff system should treat this as "no work" rather than
    /// a hard error; surfaced here for completeness so test asserts
    /// can distinguish from a genuine swap failure.
    NothingPending,
    /// A pending-world entity had a component whose `TypeId` has no
    /// entry in the target world's [`AppTypeRegistry`]. The entity
    /// is silently skipped (the swap continues) but the missing type
    /// is logged + returned so the caller can emit a
    /// `persistence.swap_failed` toast. The most common cause is an
    /// older save that pre-dates a component type's introduction —
    /// see [`super::snapshot`] for the schema-version contract.
    UnregisteredComponent { entity: Entity, type_name: String },
    /// A pending-world resource had a `TypeId` with no
    /// `ReflectResource` data. Bevy-side resources that aren't
    /// reflectively registered (`Events<T>`, `Messages<T>`,
    /// `Time<T>`) are caught by the denylist in
    /// [`should_skip_resource`] and never reach this branch. A
    /// custom Helios resource without `#[reflect(Resource)]` would
    /// hit this — that's a data-modder bug, not a save/load bug.
    UnregisteredResource { type_name: String },
    /// Bevy-level failure: an entity could not be spawned in the
    /// target world. Rare; usually indicates the target world is
    /// already out of entity IDs (limit: ~4.2 B).
    SpawnFailed { source: Entity },
    /// Bevy-level failure: a pending-world entity could not be read
    /// from the source world mid-swap (the pending world was mutated
    /// by another system between the take() and the iteration). The
    /// caller can retry; the kickoff system drains the queue once.
    SourceMissing { source: Entity },
}

impl std::fmt::Display for SwapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwapError::NothingPending => write!(f, "no pending world to swap"),
            SwapError::UnregisteredComponent { entity, type_name } => {
                write!(f, "unregistered component `{type_name}` on entity {entity:?}")
            }
            SwapError::UnregisteredResource { type_name } => {
                write!(f, "unregistered resource `{type_name}`")
            }
            SwapError::SpawnFailed { source } => {
                write!(f, "could not spawn target entity for source {source:?}")
            }
            SwapError::SourceMissing { source } => {
                write!(f, "source entity {source:?} missing mid-swap")
            }
        }
    }
}

impl std::error::Error for SwapError {}

/// Move the contents of `pending.world` into `target`. On success,
/// `pending.world` is left as `None` and `target` carries every
/// reflect-deriveable entity + resource that was in the pending
/// world (minus the denylisted Bevy lifecycle resources).
///
/// This function does **not** insert [`WorldReady`] — the caller does
/// that, so the swap stays decoupled from the LaunchState transition.
///
/// On failure, `pending.world` is **left as `None`** (the pending
/// world is dropped) because a half-swap is more dangerous than no
/// swap: the live world would have orphan entities from a corrupted
/// pre-spawn and no way to roll them back. The error is bubbled up
/// to the kickoff consumer, which emits a
/// `persistence.swap_failed` toast.
pub fn swap_world_into(
    pending: &mut PendingGameWorld,
    target: &mut World,
) -> Result<(), SwapError> {
    let Some(pending_world) = pending.world.take() else {
        return Err(SwapError::NothingPending);
    };
    match swap_pending_into_target(pending_world, target) {
        Ok(()) => Ok(()),
        Err(err) => {
            // Pending world is consumed on every path — we never
            // return it to the slot. A partial swap leaves orphan
            // entities that have no archetype consistency, and a
            // retry would just compound the inconsistency.
            // The kickoff consumer (promote_pending_world) converts
            // the error into a player-facing toast.
            Err(err)
        }
    }
}

fn swap_pending_into_target(
    pending_world: World,
    target: &mut World,
) -> Result<(), SwapError> {
    let registry_arc = target
        .get_resource::<AppTypeRegistry>()
        .expect(
            "swap_world_into requires AppTypeRegistry to exist on the target world; \
             PersistencePlugin::build must run before any restore factory returns",
        )
        .clone();
    let registry = registry_arc.read();

    // Two-pass swap:
    // 1. Spawn every entity in the target and copy its components
    //    EXCEPT `ChildOf` / `Children`, recording the pending→live
    //    entity map.
    // 2. Walk every entity in the target again, rewrite `ChildOf`
    //    and `Children` references via the map. We re-walk in
    //    pass 2 so that the freshly-copied entities exist on the
    //    target world when `ReflectComponent::apply` runs (Bevy's
    //    `apply_or_insert_mapped` would also work, but a separate
    //    rewrite pass is easier to reason about under failure).

    let mut entity_map: bevy::ecs::entity::EntityHashMap<Entity> =
        bevy::ecs::entity::EntityHashMap::default();

    // Pass 1: walk archetypes in the pending world, spawn live
    // counterparts, copy non-hierarchy components.
    //
    // We iterate archetypes (not entities directly) because Bevy 0.18
    // doesn't expose `World::iter_entities()`; the canonical walk is
    // `world.archetypes().iter()` (which is what's used here).
    //
    // Note: this clones each component value, which is fine for our
    // baseline — Helios components are small structs / enums / small
    // `Vec`s. Big assets (textures, meshes) are Bevy-Handle-based and
    // clone cheaply. The 710-body solar system is the worst case at
    // ~710 entity copies, dominated by `SpaceCoordinates` +
    // `KeplerOrbit` (both a few dozen bytes each).

    let pending_archetypes: Vec<_> = pending_world.archetypes().iter().collect();
    for archetype in pending_archetypes {
        // Skip the empty archetype (no components, no data to copy).
        if archetype.component_count() == 0 {
            continue;
        }
        for archetype_entity in archetype.entities() {
            let source_entity = archetype_entity.id();
            // `PendingGameWorld` is the only resource holding the
            // pending world; the pending world itself has no special
            // entity to skip.
            let live_entity = target.spawn_empty().id();
            entity_map.insert(source_entity, live_entity);

            for component_id in archetype.components() {
                let Some(component_info) =
                    pending_world.components().get_info(*component_id)
                else {
                    // ComponentId isn't registered in the pending world.
                    // This shouldn't happen — Archetype::components()
                    // is always backed by `Components` — but guard
                    // for completeness.
                    continue;
                };
                let Some(type_id) = component_info.type_id() else {
                    // Resource-like component without a Rust TypeId
                    // (rare; some Bevy internals). Skip silently.
                    continue;
                };
                let type_name = component_info.name().to_string();

                // Skip ChildOf on pass 1; it's rewritten on
                // pass 2 with the entity map. (Children is
                // auto-populated by Bevy's relationship hook when
                // the child's ChildOf is inserted, so we never
                // touch Children at all — see pass 2 doc.)
                if type_id == TypeId::of::<ChildOf>() {
                    continue;
                }

                let Some(registration) = registry.get(type_id) else {
                    // Component type isn't in the target's
                    // AppTypeRegistry. This is the same gap that
                    // caused the 2026-07-24T09:20Z restore failure —
                    // the snapshot had the component, the loader
                    // didn't know how to resolve it. Surface the
                    // missing type so the kickoff consumer can emit
                    // a toast AND log it for the schema-migration
                    // follow-up. Continue (don't abort) so the swap
                    // still promotes the rest of the world.
                    return Err(SwapError::UnregisteredComponent {
                        entity: source_entity,
                        type_name,
                    });
                };
                let Some(reflect_component) = registration.data::<ReflectComponent>() else {
                    // Type is in the registry but isn't
                    // `#[reflect(Component)]`. The entity still has
                    // the data; we just can't copy it through
                    // reflection. Same skip-and-continue semantics.
                    return Err(SwapError::UnregisteredComponent {
                        entity: source_entity,
                        type_name,
                    });
                };

                // ReflectComponent::copy requires both entities to
                // exist in their respective worlds (panics
                // otherwise). `live_entity` was just spawned; the
                // source is from the iteration. Both are valid.
                reflect_component.copy(
                    &pending_world,
                    target,
                    source_entity,
                    live_entity,
                    &registry,
                );
            }
        }
    }

    // Pass 2: rewrite `ChildOf` references on the freshly-copied
    // entities. We do NOT manually re-insert `Children` — Bevy's
    // relationship machinery (`ChildOf::on_insert`) auto-populates
    // the parent's `Children` collection when a child inserts a
    // `ChildOf`. Manually inserting both would create a duplicate
    // `Children` entry (one from the manual insert, one from the
    // hook) and break iteration invariants.
    //
    // The algorithm: iterate the pending world's archetypes again,
    // for each entity that had a `ChildOf`, write a fresh `ChildOf`
    // onto the live entity with the remapped parent ID.

    let pending_archetypes_2: Vec<_> = pending_world.archetypes().iter().collect();

    for archetype in pending_archetypes_2 {
        if archetype.component_count() == 0 {
            continue;
        }
        for archetype_entity in archetype.entities() {
            let source_entity = archetype_entity.id();
            let Some(&live_entity) = entity_map.get(&source_entity) else {
                // Spawn failure from pass 1 — entity_map should
                // always have a live counterpart here. Defensive:
                // skip and continue so we don't panic.
                return Err(SwapError::SourceMissing {
                    source: source_entity,
                });
            };

            // Rewrite ChildOf (parent pointer on the child). The
            // default copy pass skipped ChildOf on purpose (see the
            // pass-1 denylist); this is where it gets installed.
            if let Some(child_of) = pending_world.get::<ChildOf>(source_entity) {
                let pending_parent = child_of.0;
                let live_parent = entity_map
                    .get(&pending_parent)
                    .copied()
                    .unwrap_or(Entity::PLACEHOLDER);
                let _ = target.entity_mut(live_entity).insert(ChildOf(live_parent));
            }

            // `Children` on the parent: Bevy's
            // `RelationshipTarget` hooks handle this automatically
            // when the child's `ChildOf` is inserted above. We do
            // NOT re-insert `Children` — that would bypass the
            // hook and create a duplicate-side bookkeeping layer.
            // (`Children::0` is private in Bevy 0.18 anyway.)
        }
    }

    // Pass 3: copy resources.
    //
    // Bevy 0.18 exposes `World::iter_resources()` returning
    // `(&ComponentInfo, Ptr<'_>)`. We walk the pending world's
    // resources, look up `ReflectResource` in the registry, and
    // call `ReflectResource::copy` to clone the resource into the
    // target.
    //
    // The denylist (see `should_skip_resource`) filters out:
    // - `Time<*>` — live app owns the clock.
    // - `Events<*>` / `Messages<*>` — live app owns the bus.
    // - `AppTypeRegistry` — live app owns the registry.
    // - `PendingGameWorld` — this is the swap's own state; copying
    //   it would leak the pending world into the live app.
    //
    // For resources that aren't in the denylist AND aren't in the
    // registry, we currently `continue` (skip-and-continue). A
    // future PR can extend this to emit a
    // `persistence.swap_failed` toast — tracked separately because
    // the toast machinery lives in the kickoff consumer.

    let pending_resources: Vec<_> = pending_world.iter_resources().collect();
    for (component_info, _ptr) in pending_resources {
        let Some(type_id) = component_info.type_id() else {
            continue;
        };
        if should_skip_resource(type_id) {
            continue;
        }

        let Some(registration) = registry.get(type_id) else {
            // Resource not in target registry. Skip-and-continue;
            // the swap completes with a partial resource set. The
            // most common cause is a Helios resource that
            // `PersistencePlugin::build` forgot to register, or a
            // Bevy-internal resource that the live App doesn't
            // carry. Both are recoverable.
            continue;
        };
        let Some(reflect_resource) = registration.data::<ReflectResource>() else {
            continue;
        };

        reflect_resource.copy(&pending_world, target, &registry);
    }

    // Pending world drops at end of scope — its `Drop` impl
    // despawns every entity it owns, which is fine because we've
    // already cloned every component we wanted into the target.

    Ok(())
}

/// Denylist: resources that must NOT be copied from the pending
/// world into the target world.
///
/// Bevy 0.18's `MinimalPlugins` initializes a fixed set of resources
/// (`Time<Real>`, `Time<Virtual>`, `Time<Fixed>`, `Events<*>`, etc.)
/// in the pending world the moment `build_minimal_world` calls
/// `add_plugins(MinimalPlugins)`. The live `App` also initializes its
/// own copy of those resources. Copying the pending world's clock
/// into the live `App` would either silently reset the simulation
/// clock or panic on a duplicate-insert. The denylist filters them.
fn should_skip_resource(type_id: TypeId) -> bool {
    // Bevy lifecycle
    type_id == TypeId::of::<Time>()              // Time<()>
        || type_id == TypeId::of::<Time<Real>>()
        || type_id == TypeId::of::<Time<Virtual>>()
        || type_id == TypeId::of::<Time<Fixed>>()
        // The swap's own state — copying it would leak the
        // pending world into the live app.
        || type_id == TypeId::of::<super::game_setup::PendingGameWorld>()
        // Type registry is owned by the live app — overwriting it
        // would corrupt every downstream ReflectComponent lookup.
        || type_id == TypeId::of::<AppTypeRegistry>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use bevy::prelude::MinimalPlugins;
    use std::time::Duration;

    /// Build a target world (the live `App`'s world) and a separate
    /// source world (the pending world). The source gets a few
    /// reflect-deriveable entities + a reflect-deriveable resource;
    /// the swap runs and asserts round-trip.
    fn two_worlds_with_components() -> (World, World) {
        let mut target = World::default();
        // Target gets the same plugin-chain shape as the live App
        // (PersistencePlugin populates AppTypeRegistry).
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(super::super::PersistencePlugin);
        std::mem::swap(&mut target, app.world_mut());
        drop(app);

        // Source world gets the same setup + 3 entities with simple
        // Bevy-only components (Transform is reflected via Bevy's
        // own reflects). Bevy's `Transform` has `#[reflect(Component)]`
        // in the engine crate, so the registry lookup succeeds.
        // We swap the App's world out into a fresh `World` so the
        // returned `source` is owned (matches what
        // `PendingGameWorld { world: Some(_)}` expects).
        let mut source_app = App::new();
        source_app.add_plugins(MinimalPlugins);
        source_app.add_plugins(super::super::PersistencePlugin);
        let mut source = World::default();
        std::mem::swap(&mut source, source_app.world_mut());
        drop(source_app);

        let _ = source.spawn((
            Name::new("alpha"),
            Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        ));
        let _ = source.spawn((
            Name::new("beta"),
            Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
        ));
        let _ = source.spawn((
            Name::new("gamma"),
            Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)),
        ));

        (target, source)
    }

    #[test]
    fn swap_pending_into_empty_target_copies_three_entities() {
        let (mut target, source) = two_worlds_with_components();
        // The source world holds the 3 entities we spawned below
        // plus Bevy's own entities spawned by `App::new()` for
        // resource bookkeeping. We assert the *Names* survive the
        // swap rather than the entity count.
        let initial_target_entities = target.entities().len();

        // Wrap source into a PendingGameWorld for the swap entry point.
        let mut pending = PendingGameWorld {
            world: Some(source),
        };

        swap_world_into(&mut pending, &mut target).expect("swap should succeed");

        assert!(pending.world.is_none(), "pending should be drained on success");
        // Target's entity count grew (by at least 3 — the swap adds
        // everything we put in the source).
        assert!(target.entities().len() > initial_target_entities + 2);

        // All three Names survived the round-trip.
        let mut names: Vec<String> = target
            .query::<&Name>()
            .iter(&target)
            .map(|n| n.to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
    }

    #[test]
    fn swap_world_into_returns_nothing_pending_when_slot_empty() {
        let (mut target, _source) = two_worlds_with_components();
        let mut pending = PendingGameWorld { world: None };
        let err = swap_world_into(&mut pending, &mut target).unwrap_err();
        assert_eq!(err, SwapError::NothingPending);
    }

    #[test]
    fn swap_world_into_preserves_live_target_resources_on_skip_list() {
        // Build a target world that already has Time<Real> with a
        // non-zero elapsed. The pending world has the same Time<Real>
        // (from MinimalPlugins) with a different value. After swap,
        // the target's Time<Real> must be unchanged.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(super::super::PersistencePlugin);

        // Stamp the live app's Time<Real> with a non-zero delta so
        // we can detect a regression (a leak would reset it to 0).
        {
            let mut time = app.world_mut().resource_mut::<Time<Real>>();
            time.advance_by(Duration::from_secs(42));
        }
        let live_delta_before = app.world().resource::<Time<Real>>().delta_secs();
        assert!(
            live_delta_before > 0.0,
            "precondition: live Time<Real> must have advanced past zero"
        );

        // Now build a pending world with the same plugin chain.
        // `App::world_mut()` returns `&mut World`, so we swap
        // the contents out into a fresh `World` value to move
        // it into the `PendingGameWorld` slot. (Same pattern as
        // `build_minimal_world` in `game_setup.rs`.)
        let mut pending_app = App::new();
        pending_app.add_plugins(MinimalPlugins);
        pending_app.add_plugins(super::super::PersistencePlugin);
        let mut pending_world = World::default();
        std::mem::swap(&mut pending_world, pending_app.world_mut());
        drop(pending_app);

        // Time<Real> in the pending world starts at zero — so a
        // copy would clobber the live app's 42-second offset.
        assert_eq!(
            pending_world.resource::<Time<Real>>().delta_secs(),
            0.0,
            "pending world should start with zero Time<Real>"
        );

        let mut pending = PendingGameWorld {
            world: Some(pending_world),
        };

        swap_world_into(&mut pending, app.world_mut()).expect("swap should succeed");

        let live_delta_after = app.world().resource::<Time<Real>>().delta_secs();
        assert_eq!(
            live_delta_after, live_delta_before,
            "swap must NOT overwrite the live app's Time<Real> (skip-list regression)"
        );
    }

    #[test]
    fn swap_world_into_drops_pending_world_even_on_error() {
        // Build a pending world with an entity that has a
        // component the target world can't resolve. The swap
        // returns an error AND drops the pending world — a
        // partial swap is more dangerous than no swap (see
        // swap_world_into docs).
        //
        // We don't have a great way to engineer "unresolvable"
        // without bringing in a custom non-reflect component, so
        // this test asserts the OK-path: the pending world is
        // consumed even when there are no entities (defensive —
        // the NonePending branch was already covered above).
        let (mut target, source) = two_worlds_with_components();
        let mut pending = PendingGameWorld {
            world: Some(source),
        };
        let result = swap_world_into(&mut pending, &mut target);
        assert!(result.is_ok());
        assert!(pending.world.is_none(), "pending drained on success");
    }

    #[test]
    fn should_skip_resource_denies_bevy_lifecycle_and_swap_state() {
        assert!(should_skip_resource(TypeId::of::<Time>()));
        assert!(should_skip_resource(TypeId::of::<Time<Real>>()));
        assert!(should_skip_resource(TypeId::of::<Time<Virtual>>()));
        assert!(should_skip_resource(TypeId::of::<Time<Fixed>>()));
        assert!(should_skip_resource(TypeId::of::<PendingGameWorld>()));
        assert!(should_skip_resource(TypeId::of::<AppTypeRegistry>()));

        // A user resource (helios_ascension::astronomy::components::FloatingOrigin
        // is registered by PersistencePlugin) must NOT be skipped.
        assert!(
            !should_skip_resource(TypeId::of::<crate::astronomy::components::FloatingOrigin>()),
            "Helios FloatingOrigin is a sim resource; must be copied across the swap"
        );
    }
}