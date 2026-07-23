//! Restore a RON save into a fresh Bevy [`World`].
//!
//! PR-A ships [`restore_world`] that:
//!
//! 1. Parses the on-disk RON into a [`SaveFile`] envelope.
//! 2. Validates [`SaveMetadata::format_version`] against
//!    [`FORMAT_VERSION`], routing through [`super::migrate::run_migrations`]
//!    when a future version migrates an older save forward.
//! 3. Decodes the body — a RON-serialised Bevy [`DynamicScene`] — back
//!    into a [`DynamicScene`] using Bevy's [`SceneDeserializer`].
//! 4. Calls [`DynamicScene::write_to_world`] against a freshly-constructed
//!    [`World`] — never the live one. Per CTO design R3, the loader
//!    creates a new [`World`]; the caller swaps it in.
//!
//! The caller is responsible for:
//!
//! - Inserting [`AppTypeRegistry`] (and any required [`ReflectComponent`] /
//!   [`ReflectResource`] registrations) into the target World **before**
//!   calling [`restore_world`]. Bevy's reflection is plug-in driven; the
//!   persistence module does not own plugin registration.
//! - Replacing the live [`World`] with the restored one. PR-A does not
//!   touch the swap path — that lands in PR-B alongside the menu UI.

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy_scene::serde::SceneDeserializer;
use bevy_scene::DynamicScene;
use serde::de::DeserializeSeed;
use std::fmt;

use super::handle_sidecar::apply_handle_sidecar;
use super::migrate::run_migrations;
use super::snapshot::{SaveFile, SaveMetadata};

/// Errors from the restore path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    /// The RON envelope could not be parsed at all (corrupt file,
    /// truncated write, garbage bytes).
    Parse(String),
    /// The save's `format_version` was below the supported floor.
    VersionTooOld { found: u32, min: u32 },
    /// The save's `format_version` is newer than this binary knows.
    /// Likely a save from a future Helios build.
    VersionTooNew { found: u32, current: u32 },
    /// A migrator step failed (malformed field in the save body).
    Migrate(String),
    /// Bevy's [`SceneDeserializer`] returned an error. Almost always a
    /// `Reflect` coverage gap (a component in the save has no
    /// `ReflectComponent` registered).
    Scene(String),
    /// The target [`World`] is missing [`AppTypeRegistry`].
    MissingTypeRegistry,
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreError::Parse(s) => write!(f, "save parse failed: {s}"),
            RestoreError::VersionTooOld { found, min } => write!(
                f,
                "save format version {found} is older than minimum supported {min}"
            ),
            RestoreError::VersionTooNew { found, current } => write!(
                f,
                "save format version {found} is newer than binary's {current}"
            ),
            RestoreError::Migrate(s) => write!(f, "save migration failed: {s}"),
            RestoreError::Scene(s) => write!(f, "scene deserialise failed: {s}"),
            RestoreError::MissingTypeRegistry => write!(
                f,
                "AppTypeRegistry is not present on the World — call \
                 init_resource::<AppTypeRegistry>() before restoring"
            ),
        }
    }
}

impl std::error::Error for RestoreError {}

/// Outcome of a successful restore — the populated [`World`] plus the
/// parsed [`SaveMetadata`] so the caller can update UI / playtime
/// trackers without re-parsing the envelope.
#[derive(Debug)]
pub struct RestoredWorld {
    pub world: World,
    pub metadata: SaveMetadata,
}

/// Restore `ron_text` into a freshly-constructed [`World`].
///
/// `world_factory` builds the target world with all plugins and the
/// [`AppTypeRegistry`] already initialised. PR-A ships a default factory
/// (see [`default_world_factory`]) that returns `World::new()` + a
/// stub `AppTypeRegistry` — production code must inject a real factory
/// that runs the full plugin set so component types are registered
/// reflectively.
///
/// PR-A uses this factory only for tests; the production load path lands
/// in PR-B (Save Panel UI + slot manager) and will provide the real
/// factory that runs [`super::PersistencePlugin::build`] plus the
/// full app plugin stack.
pub fn restore_world<F>(ron_text: &str, world_factory: F) -> Result<RestoredWorld, RestoreError>
where
    F: FnOnce() -> World,
{
    // 1. Parse envelope.
    let envelope: SaveFile =
        ron::from_str(ron_text).map_err(|e| RestoreError::Parse(e.to_string()))?;

    // 2. Validate format version + run migrations if needed.
    let (migrated_body, _version) = run_migrations(envelope.metadata.format_version, envelope.body)
        .map_err(|e| match e {
            super::migrate::MigrateError::TooOld { found, min } => {
                RestoreError::VersionTooOld { found, min }
            }
            super::migrate::MigrateError::TooNew { found, current } => {
                RestoreError::VersionTooNew { found, current }
            }
            super::migrate::MigrateError::Step { reason, .. } => RestoreError::Migrate(reason),
        })?;

    // 3. Build the target world via the caller-supplied factory. The
    //    factory is responsible for inserting AppTypeRegistry; we read it
    //    back to drive SceneDeserializer.
    let mut world = world_factory();

    let registry = world
        .get_resource::<AppTypeRegistry>()
        .ok_or(RestoreError::MissingTypeRegistry)?
        .clone();
    let registry_locked = registry.read();

    // 4. Decode the body back into a DynamicScene. PR-A's body is a
    //    RON string; feed it through Bevy's SceneDeserializer. The
    //    Bevy 0.18 SceneDeserializer is a `DeserializeSeed`, and the
    //    seed's `deserialize` takes the underlying deserializer by
    //    `&mut D` (ron's `Deserializer` mutates its cursor as it walks
    //    the input), so we hold it in a `mut` binding here.
    let mut ron_deserializer = ron::Deserializer::from_str(&migrated_body.data)
        .map_err(|e| RestoreError::Scene(format!("ron deserializer init: {e}")))?;
    let scene: DynamicScene = SceneDeserializer {
        type_registry: &registry_locked,
    }
    .deserialize(&mut ron_deserializer)
    .map_err(|e| RestoreError::Scene(e.to_string()))?;

    // 5. Write the scene into the world.
    let mut entity_map = EntityHashMap::default();
    scene
        .write_to_world(&mut world, &mut entity_map)
        .map_err(|e| RestoreError::Scene(format!("{e:?}")))?;

    // 6. Re-attach Handle-bearing components that the snapshot
    //    denied because their inner `Arc<StrongHandle>` is
    //    process-local. The sidecar (saved alongside the scene)
    //    records the asset paths; we resolve them via the asset
    //    server and insert fresh handles on the corresponding
    //    entities (using `entity_map` to translate saved IDs to new
    //    ones). `apply_handle_sidecar` is a no-op when the world has
    //    no AssetServer (test factory `default_world_factory`) or
    //    when the sidecar is `None` (older saves without the field).
    if let Some(sidecar) = envelope.handles.as_ref() {
        apply_handle_sidecar(&mut world, sidecar, &entity_map);
    }

    Ok(RestoredWorld {
        world,
        metadata: envelope.metadata,
    })
}

/// Default world factory for tests: returns an empty [`World`] with an
/// initialised [`AppTypeRegistry`]. The caller is expected to register
/// any `#[reflect(Component)]`/`#[reflect(Resource)]` types via
/// `app.register_type::<T>()` **before** handing the world to
/// [`restore_world`].
///
/// Production code must NOT use this factory — it knows nothing about
/// the game's plugins. PR-B will provide the real factory.
#[cfg(test)]
pub fn default_world_factory() -> World {
    let mut world = World::new();
    world.init_resource::<AppTypeRegistry>();
    world
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::snapshot::{snapshot_world_with_registry, SaveMetadata};

    /// Minimal reflect-aware component for roundtrip tests.
    ///
    /// Lives in this module because Bevy's derive macros require the
    /// type to be `Reflect`-derived and registered via
    /// `app.register_type::<T>()` before snapshot/restore. We don't need
    /// this in production code; it exists to exercise the roundtrip
    /// path end-to-end.
    #[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
    #[reflect(Component)]
    struct TestComponentA {
        x: i32,
        y: i32,
    }

    #[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
    #[reflect(Component)]
    struct TestComponentB {
        name: String,
    }

    #[derive(Resource, Reflect, Default, Clone, Debug, PartialEq)]
    #[reflect(Resource)]
    struct TestResourceA {
        counter: u64,
    }

    fn build_test_world() -> World {
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        {
            let registry = world.resource::<AppTypeRegistry>();
            registry.write().register::<TestComponentA>();
            registry.write().register::<TestComponentB>();
            registry.write().register::<TestResourceA>();
        }
        world
    }

    #[test]
    fn roundtrip_world_with_components_and_resources() {
        let mut world = build_test_world();
        let entity = world.spawn(TestComponentA { x: 7, y: 11 }).id();
        let _ = world.spawn(TestComponentB {
            name: "alpha".to_string(),
        });
        world.insert_resource(TestResourceA { counter: 42 });

        let metadata = SaveMetadata::new_now(123, 100, "0.4.0");
        let registry = world.resource::<AppTypeRegistry>().clone();
        let ron_text = snapshot_world_with_registry(&world, &registry, metadata)
            .expect("snapshot must succeed");

        // Restore into a fresh world with the same registrations.
        let mut restored =
            restore_world(&ron_text, build_test_world).expect("restore must succeed");

        // Verify the resource came back.
        let res = restored
            .world
            .get_resource::<TestResourceA>()
            .expect("TestResourceA must be present after restore");
        assert_eq!(res.counter, 42);

        // Verify entities came back. The snapshot/restore round-trip
        // uses fresh Entity IDs, so we count rather than compare.
        // The test spawns TWO separate entities (one with
        // `TestComponentA` only, one with `TestComponentB` only)
        // so each component lives on its own entity — query them
        // separately, not via a join.
        let mut count_a = 0;
        let mut count_b = 0;
        for _ in restored
            .world
            .query::<&TestComponentA>()
            .iter(&restored.world)
        {
            count_a += 1;
        }
        for _ in restored
            .world
            .query::<&TestComponentB>()
            .iter(&restored.world)
        {
            count_b += 1;
        }
        assert!(
            count_a >= 1,
            "restored world must contain at least one entity with TestComponentA"
        );
        assert!(
            count_b >= 1,
            "restored world must contain at least one entity with TestComponentB"
        );

        // Sanity: original world entity is still there (the snapshot is
        // a copy, not a move).
        assert!(world.get_entity(entity).is_ok(), "source world untouched");
    }

    #[test]
    fn parse_error_on_garbage() {
        let result = restore_world("this is not ron", build_test_world);
        assert!(matches!(result, Err(RestoreError::Parse(_))));
    }

    #[test]
    fn version_too_old_is_rejected() {
        // Hand-craft a SaveFile with a too-old format_version.
        let body = crate::persistence::migrate::Body {
            schema: crate::persistence::migrate::SchemaKind::SceneRon,
            data: String::new(),
        };
        let envelope = SaveFile {
            metadata: SaveMetadata {
                format_version: 0,
                saved_at_unix_s: 0,
                playtime_s: 0,
                seed: 0,
                helios_version: "0.0.0".to_string(),
                preview: Default::default(),
            },
            body,
            // No handle sidecar on this hand-crafted test envelope.
            handles: None,
        };
        let ron_text = ron::to_string(&envelope).expect("serialize envelope");
        let result = restore_world(&ron_text, build_test_world);
        assert!(matches!(result, Err(RestoreError::VersionTooOld { .. })));
    }
}
