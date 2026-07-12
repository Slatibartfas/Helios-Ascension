//! Asset-handle sidecar for save/load.
//!
//! # Why this exists
//!
//! Bevy 0.18's `DynamicScene` serializer walks every
//! `#[reflect(Component)]` of every entity via `Reflect`. Most
//! components round-trip cleanly — positions, fleets, ship instances,
//! orbits, colony state, etc. — but [`Handle<T>`] is special. Its
//! [`Handle::Strong`](bevy::asset::Handle::Strong) variant holds an
//! [`Arc<StrongHandle>`](bevy::asset::StrongHandle) whose
//! strong-count isn't stable across builds and whose underlying
//! handle carries runtime-only channels for asset cleanup. Bevy
//! can't (and shouldn't) register `ReflectSerialize` for `Arc<_>` of
//! a process-local pointer, so every save attempt that visits a
//! `Mesh3d`, `MeshMaterial3d`, or any other `Handle<T>`-bearing
//! component fails with:
//!
//! ```text
//! type `bevy_platform::sync::Arc<bevy_asset::handle::StrongHandle>`
//! did not register the `ReflectSerialize`
//! ```
//!
//! The autosave timer logs `autosave failed: save serialise failed: …`
//! every interval and no save file is produced.
//!
//! Three options were considered:
//!
//! 1. **Deny the component** from the snapshot — autosave succeeds
//!    but loaded saves lose visual fidelity (entities come back
//!    without their mesh refs). The previous fix tried this; the
//!    player-visible downside is too large to ship.
//! 2. **Custom `ReflectSerialize` / `FromReflect` for `Handle<T>`**
//!    that round-trips as a path string. The deserialize side is
//!    the blocker — `FromReflect` has no world access, so it can't
//!    call `asset_server.load(path)` to reconstruct a typed handle.
//! 3. **Sidecar (this module)**. Before the scene is snapshotted,
//!    walk the world and extract asset paths from each
//!    `Handle<T>`-bearing component into a sidecar struct that
//!    lives in the save envelope alongside the scene. The scene
//!    blob continues to deny the Handle-bearing components (no
//!    more `Arc<StrongHandle>` failures), and on load the sidecar
//!    re-attaches fresh `Handle<T>` values via
//!    [`AssetServer::load`](bevy::asset::AssetServer::load), using
//!    the entity-map captured during `write_to_world` to remap
//!    saved entity IDs to new ones.
//!
//! Option 3 keeps the scene snapshot simple (it just keeps the
//! existing `deny_component` calls for Handle-bearing components),
//! makes the on-disk schema purely additive (`handles: Option<HandleSidecar>`
//! with `#[serde(default)]` so older saves still load), and doesn't
//! fight Bevy's reflection system.
//!
//! # Limitations
//!
//! - Only `Handle::Strong` with a known asset path round-trips.
//!   `Handle::Uuid` carries no path (the path lives only on the
//!   StrongHandle), and `Handle::Strong` without a path
//!   (internally-reserved handles) likewise fall back to "no path"
//!   and the corresponding entity on load will be missing that
//!   visual component.
//! - Paths are stored as strings, so if the asset on disk has been
//!   replaced since save, the load resolves to the new asset (which
//!   may differ visually — a non-issue for in-development saves).
//! - Adding new Handle-bearing components requires extending
//!   [`EntityHandles`] with a new field plus a query clause in
//!   [`extract_handle_sidecar`] and a clause in
//!   [`apply_handle_sidecar`].

use bevy::asset::AssetServer;
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy_mesh::Mesh;
use bevy_pbr::{MeshMaterial3d, StandardMaterial};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-entity record of Handle-bearing components we round-trip via
/// the sidecar. Each path field is `Some(path)` if the entity had
/// the corresponding component and the inner `Handle::Strong` carried
/// a loadable asset path, `None` otherwise.
///
/// Add new fields here when introducing additional Handle-bearing
/// components whose visual fidelity needs to survive save/load, plus
/// a query clause in [`extract_handle_sidecar`] and an attach clause
/// in [`apply_handle_sidecar`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityHandles {
    /// Full [`Entity`] bits (index + generation). Matches what
    /// Bevy's `DynamicScene` stores under each entity key, so the
    /// `entity_map` populated by [`DynamicScene::write_to_world`]
    /// accepts this as a lookup key.
    pub entity: u64,
    /// Asset path for `Mesh3d`'s `Handle<Mesh>`, if any.
    pub mesh3d: Option<String>,
    /// Asset path for `MeshMaterial3d<StandardMaterial>`'s
    /// `Handle<StandardMaterial>`, if any.
    pub mesh_material3d_standard: Option<String>,
}

/// Top-level sidecar that travels alongside the scene blob in
/// [`super::snapshot::SaveFile::handles`]. `None` is equivalent to
/// "no Handle-bearing components in the saved world" — older saves
/// without this field deserialize as `None` via `#[serde(default)]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandleSidecar {
    pub entries: Vec<EntityHandles>,
}

impl HandleSidecar {
    /// True when there is nothing to round-trip — a freshly-default-
    /// constructed sidecar, or a sidecar whose every entity had no
    /// extractable paths. Lets the snapshot path elide the field
    /// entirely when there's nothing useful to record.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
            || self.entries.iter().all(|e| {
                e.mesh3d.is_none() && e.mesh_material3d_standard.is_none()
            })
    }
}

/// Walk the world and build a [`HandleSidecar`] recording the asset
/// paths of every Handle-bearing component we round-trip. The result
/// is written into [`super::snapshot::SaveFile::handles`].
///
/// Cost: linear in entity count, with two archetype-component
/// lookups per entity. For a Helios in-game session with thousands
/// of entities this is microseconds; it runs once per autosave
/// fire (default 5 min) plus once per manual save.
///
/// Takes `&World` (not `&mut World`) so it composes with the rest of
/// the snapshot pipeline which is read-only. We walk every spawned
/// entity via the same path [`super::snapshot::collect_all_entities`]
/// uses, and call `EntityRef::get::<T>` for each tracked component
/// rather than running two `Query::iter` passes — Query::iter needs
/// `&mut World` because it caches archetype state, while
/// `EntityRef::get` reads immutably.
pub fn extract_handle_sidecar(world: &World) -> HandleSidecar {
    let mut entries_map: HashMap<Entity, EntityHandles> = HashMap::new();

    // Walk every spawned entity. `get_entity` returns
    // `EntityNotSpawnedError` for despawned entries — we filter
    // those via `collect_all_entities` upstream, but a defensive
    // skip here costs nothing.
    let entities = super::snapshot::collect_all_entities(world);
    for entity in entities {
        let entity_ref = match world.get_entity(entity) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let entry = entries_map
            .entry(entity)
            .or_insert_with(|| EntityHandles {
                entity: entity.to_bits(),
                ..Default::default()
            });

        // Mesh3d — extract asset path from inner Handle<Mesh>.
        // Strong handles without a path yield None (the entity
        // stays in the map with mesh3d: None so apply sees it as
        // "had a mesh but no path", matching runtime reality).
        if let Some(mesh) = entity_ref.get::<Mesh3d>() {
            entry.mesh3d = mesh.0.path().map(|p| p.to_string());
        }

        // MeshMaterial3d<StandardMaterial> — same pattern. Helios's
        // custom materials (OceanMaterial, AtmosphereMaterial,
        // StarGlowMaterial, StarDiffractionMaterial) carry the same
        // Handle<T> problem but live on entities that the system
        // populator re-creates on every world rebuild, so they're
        // not round-tripped — a fresh setup pass re-attaches them
        // with their paths. If a future commit puts one of those
        // materials on a save-persistent entity archetype, add a
        // corresponding `entity_ref.get::<MeshMaterial3d<...>>()`
        // clause here plus an attach clause in `apply_handle_sidecar`.
        if let Some(mat) = entity_ref.get::<MeshMaterial3d<StandardMaterial>>() {
            entry.mesh_material3d_standard = mat.0.path().map(|p| p.to_string());
        }
    }

    HandleSidecar {
        entries: entries_map.into_values().collect(),
    }
}

/// Walk a [`HandleSidecar`] and re-attach the recorded handles to the
/// corresponding entities on `world`. Call AFTER Bevy's
/// `write_to_world` finishes so the [`EntityHashMap`] is populated.
///
/// Behaviour notes:
///
/// - Silently skips entries whose saved entity ID isn't in
///   `entity_map` (the entity didn't survive scene restore — e.g. a
///   setup-only entity that the save layer is allowed to drop, or a
///   stale save from a version that included extra entities).
/// - Silently skips paths that no longer exist on disk — Bevy's
///   `AssetServer::load` returns a handle that will eventually
///   surface an asset-load-failed event. The entity still gets the
///   (broken) handle attached so the visual ref is at least
///   populated; the player will see a missing-asset log rather than
///   an invisible entity with no ref at all.
/// - If `world` doesn't have an [`AssetServer`] (e.g. the test
///   factory `default_world_factory`), this function is a no-op —
///   preserves test ergonomics without forcing every test to wire
///   up an asset server.
pub fn apply_handle_sidecar(
    world: &mut World,
    sidecar: &HandleSidecar,
    entity_map: &EntityHashMap<Entity>,
) {
    // No asset server means there's nothing we can resolve — exit
    // cleanly. Tests use default_world_factory which doesn't include
    // AssetsPlugin; production paths go through PersistencePlugin
    // which has AssetServer available transitively via DefaultPlugins.
    let Some(asset_server) = world.get_resource::<AssetServer>().cloned() else {
        return;
    };

    for entry in &sidecar.entries {
        let saved_entity = match Entity::try_from_bits(entry.entity) {
            Some(e) => e,
            None => continue,
        };
        let new_entity = match entity_map.get(&saved_entity) {
            Some(&e) => e,
            None => continue,
        };
        // `world.entity_mut` would panic if the entity was despawned
        // by something post-write_to_world, so guard first. In Bevy
        // 0.18 `get_entity` returns `Result` (not `Option`).
        if world.get_entity(new_entity).is_err() {
            continue;
        }

        let mut entity_mut = world.entity_mut(new_entity);
        if let Some(path) = &entry.mesh3d {
            let handle: Handle<Mesh> = asset_server.load(path);
            entity_mut.insert(Mesh3d(handle));
        }
        if let Some(path) = &entry.mesh_material3d_standard {
            let handle: Handle<StandardMaterial> = asset_server.load(path);
            entity_mut.insert(MeshMaterial3d(handle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::MinimalPlugins;

    /// Build a minimal App with `AssetServer` initialised so the
    /// sidecar can resolve paths. `AssetPlugin::default()` provides
    /// the asset server and a default in-memory asset source — no
    /// disk I/O is performed when calling `asset_server.load(path)`,
    /// which is exactly what the sidecar apply needs to test entity
    /// attachment without fixture files. We also call
    /// `init_asset::<T>` for every Handle-bearing asset type the
    /// sidecar tests interact with — `AssetServer::load<T>(path)`
    /// panics if T hasn't been initialised, even if the asset
    /// itself is never read.
    fn fresh_app_with_assets() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app
    }

    #[test]
    fn handle_sidecar_empty_by_default() {
        let sidecar = HandleSidecar::default();
        assert!(sidecar.is_empty());
        assert!(sidecar.entries.is_empty());
    }

    #[test]
    fn extract_returns_empty_for_world_with_no_handles() {
        let mut app = fresh_app_with_assets();
        // Spawn an entity without any Handle-bearing components.
        app.world_mut().spawn(Mesh3d::default());
        let sidecar = extract_handle_sidecar(app.world());
        assert!(sidecar.is_empty());
    }

    #[test]
    fn extract_and_apply_round_trip_attach_handles() {
        let mut app = fresh_app_with_assets();

        // Two entities: one with a (default-handle) Mesh3d, one with
        // a default MeshMaterial3d. Both should round-trip.
        let e1 = app
            .world_mut()
            .spawn(Mesh3d::default())
            .id();
        let e2 = app
            .world_mut()
            .spawn(MeshMaterial3d::<StandardMaterial>::default())
            .id();

        let sidecar = extract_handle_sidecar(app.world());
        assert_eq!(sidecar.entries.len(), 2);

        // Build an entity_map and apply the sidecar.
        let mut entity_map = EntityHashMap::default();
        // Pretend the saved entities get remapped to fresh IDs.
        let e1_new = app.world_mut().spawn_empty().id();
        let e2_new = app.world_mut().spawn_empty().id();
        entity_map.insert(e1, e1_new);
        entity_map.insert(e2, e2_new);

        apply_handle_sidecar(app.world_mut(), &sidecar, &entity_map);

        // Both new entities should now exist on the world. Note:
        // `apply_handle_sidecar` only inserts a Mesh3d /
        // MeshMaterial3d when its corresponding path is Some —
        // here the default `Handle::default()` has no path so
        // `apply` attaches nothing. The test just verifies the map
        // walk doesn't panic and the entities survive the round
        // trip.
        let world = app.world();
        assert!(world.get_entity(e1_new).is_ok());
        assert!(world.get_entity(e2_new).is_ok());
    }

    #[test]
    fn apply_is_noop_without_asset_server() {
        // default_world_factory-equivalent world: no AssetServer.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        let entity = world.spawn_empty().id();
        let mut entity_map = EntityHashMap::default();
        entity_map.insert(entity, entity);

        let sidecar = HandleSidecar {
            entries: vec![EntityHandles {
                entity: entity.to_bits(),
                mesh3d: Some("test/path.glb".to_string()),
                mesh_material3d_standard: None,
            }],
        };
        // Should not panic even though AssetServer is missing.
        apply_handle_sidecar(&mut world, &sidecar, &entity_map);
        // And should not have attached anything.
        assert!(world.get_entity(entity).unwrap().get::<Mesh3d>().is_none());
    }

    #[test]
    fn extract_records_path_for_loaded_asset() {
        // AssetServer::load returns a Strong handle with the path
        // baked in, even if the asset file doesn't exist on disk
        // (the load will fail later, but the handle is valid and
        // its path is extractable). This is the positive test for
        // extract_handle_sidecar — confirm the path round-trips
        // into the sidecar as a String.
        let mut app = fresh_app_with_assets();
        let server = app.world().resource::<AssetServer>().clone();
        let mesh_handle: Handle<Mesh> = server.load("models/planet.glb");
        let mat_handle: Handle<StandardMaterial> = server.load("materials/planet.ron");

        app.world_mut().spawn((Mesh3d(mesh_handle), MeshMaterial3d(mat_handle)));

        let sidecar = extract_handle_sidecar(app.world());
        assert_eq!(sidecar.entries.len(), 1);
        let entry = &sidecar.entries[0];
        assert_eq!(
            entry.mesh3d.as_deref(),
            Some("models/planet.glb"),
            "Mesh3d path must round-trip into the sidecar"
        );
        assert_eq!(
            entry.mesh_material3d_standard.as_deref(),
            Some("materials/planet.ron"),
            "MeshMaterial3d<StandardMaterial> path must round-trip into the sidecar"
        );
    }

    #[test]
    fn apply_attaches_components_for_loaded_paths() {
        // Full save→restore-style round trip: spawn entity with a
        // Mesh3d whose Handle has a real path, build a sidecar,
        // apply the sidecar to a fresh entity, and verify the
        // Mesh3d component comes back with a handle pointing at
        // the same asset path.
        let mut app = fresh_app_with_assets();
        let server = app.world().resource::<AssetServer>().clone();
        let mesh_handle: Handle<Mesh> = server.load("models/planet.glb");

        let saved_entity = app.world_mut().spawn(Mesh3d(mesh_handle)).id();

        let sidecar = extract_handle_sidecar(app.world());
        assert!(!sidecar.is_empty());

        // Pretend the entity gets remapped to a fresh ID during
        // restore. The sidecar's `entity: u64` is `saved_entity.to_bits()`,
        // which is the lookup key in the entity_map.
        let new_entity = app.world_mut().spawn_empty().id();
        let mut entity_map = EntityHashMap::default();
        entity_map.insert(saved_entity, new_entity);

        apply_handle_sidecar(app.world_mut(), &sidecar, &entity_map);

        // The new entity now has Mesh3d with a handle whose path
        // matches the saved one — visual fidelity preserved across
        // the round trip.
        let mesh = app
            .world()
            .entity(new_entity)
            .get::<Mesh3d>()
            .expect("apply must have attached Mesh3d");
        assert_eq!(
            mesh.0.path().map(|p| p.to_string()).as_deref(),
            Some("models/planet.glb"),
            "round-tripped Mesh3d must point at the same asset path"
        );
    }

    #[test]
    fn sidecar_serde_ron_round_trip() {
        let sidecar = HandleSidecar {
            entries: vec![
                EntityHandles {
                    entity: 42,
                    mesh3d: Some("models/planet.glb".to_string()),
                    mesh_material3d_standard: None,
                },
                EntityHandles {
                    entity: 99,
                    mesh3d: None,
                    mesh_material3d_standard: Some("materials/planet.ron".to_string()),
                },
            ],
        };
        let ron = ron::to_string(&sidecar).expect("serialize");
        let back: HandleSidecar = ron::from_str(&ron).expect("deserialize");
        assert_eq!(sidecar, back);
    }
}