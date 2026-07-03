//! End-to-end roundtrip test for GRA-314 PR-A.
//!
//! Spawns a [`World`] with multiple reflectively-registered components
//! and resources, snapshots it, restores into a fresh world, and asserts
//! the resulting world matches.
//!
//! **Why test types only:** PR-A reveals that Helios has very few
//! `#[reflect(Component)]` / `#[reflect(Resource)]` registrations (only
//! `notifications/components.rs` and a couple of UI state types).
//! Snapshot/restore silently drops unregistered types, so testing on
//! live Helios components would only test the empty-set case. The
//! reflect-coverage follow-up will register the production components;
//! this test will then gain test fixtures that exercise them.

use bevy::ecs::reflect::AppTypeRegistry;
use bevy::prelude::*;
use helios_ascension::persistence::{restore_world, snapshot_world_with_registry, SaveMetadata};

// -- Reflective test types ----------------------------------------------------
//
// Each type mirrors a "real" Helios component shape so when the
// reflect-coverage follow-up lands, swapping in the real types is a
// mechanical replace.

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct HullMass(i32);

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct Orbit {
    semi_major_axis_au: f64,
    eccentricity: f64,
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct ColonistPopulation(u32);

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct ShipName(String);

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct FleetId(u64);

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct ResearchProgress {
    category: String,
    points: u32,
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
struct ConstructionQueue {
    item_count: u32,
}

#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Resource)]
struct SimulationTime {
    elapsed_s: f64,
    tick: u64,
}

#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Resource)]
struct GameSeed(u64);

fn build_world_with_test_types() -> World {
    let mut world = World::new();
    world.init_resource::<AppTypeRegistry>();
    let registry = world.resource::<AppTypeRegistry>().clone();
    {
        let mut write = registry.write();
        write.register::<HullMass>();
        write.register::<Orbit>();
        write.register::<ColonistPopulation>();
        write.register::<ShipName>();
        write.register::<FleetId>();
        write.register::<ResearchProgress>();
        write.register::<ConstructionQueue>();
        write.register::<SimulationTime>();
        write.register::<GameSeed>();
    }
    world
}

#[test]
fn roundtrip_world_with_components_and_resources() {
    // 1. Build a populated world — 10+ reflectively-registered types
    //    across the test fixtures (7 components + 2 resources = 9, plus
    //    any default plugin-registered components like Entity / Name).
    let mut world = build_world_with_test_types();
    world.spawn((
        HullMass(1500),
        Orbit {
            semi_major_axis_au: 1.0,
            eccentricity: 0.0167,
        },
        ShipName("ISS-1".to_string()),
        FleetId(7),
    ));
    world.spawn((
        ColonistPopulation(500),
        ResearchProgress {
            category: "propulsion".to_string(),
            points: 250,
        },
    ));
    world.spawn(ConstructionQueue { item_count: 3 });
    world.insert_resource(SimulationTime {
        elapsed_s: 86400.0, // 1 day
        tick: 86400,
    });
    world.insert_resource(GameSeed(0xDEADBEEFCAFEBABE));

    // 2. Snapshot.
    let metadata = SaveMetadata::new_now(0xDEADBEEFCAFEBABE, 86400, "0.4.0");
    let registry = world.resource::<AppTypeRegistry>().clone();
    let ron_text =
        snapshot_world_with_registry(&world, &registry, metadata).expect("snapshot must succeed");
    assert!(!ron_text.is_empty(), "snapshot RON must not be empty");

    // 3. Restore into a fresh world with the same registrations.
    let restored =
        restore_world(&ron_text, build_world_with_test_types).expect("restore must succeed");

    // 4. Verify each reflectively-registered type survived.
    let mut hull_q = restored.world.query::<&HullMass>();
    let hull_count = hull_q.iter(&restored.world).count();
    assert_eq!(hull_count, 1, "1 HullMass entity expected");
    let hull = hull_q.iter(&restored.world).next().unwrap();
    assert_eq!(hull.0, 1500);

    let mut orbit_q = restored.world.query::<&Orbit>();
    let orbit = orbit_q.iter(&restored.world).next().unwrap();
    assert!((orbit.semi_major_axis_au - 1.0).abs() < 1e-9);
    assert!((orbit.eccentricity - 0.0167).abs() < 1e-6);

    let mut pop_q = restored.world.query::<&ColonistPopulation>();
    assert_eq!(pop_q.iter(&restored.world).next().unwrap().0, 500);

    let mut fleet_q = restored.world.query::<&FleetId>();
    assert_eq!(fleet_q.iter(&restored.world).next().unwrap().0, 7);

    let mut research_q = restored.world.query::<&ResearchProgress>();
    let rp = research_q.iter(&restored.world).next().unwrap();
    assert_eq!(rp.category, "propulsion");
    assert_eq!(rp.points, 250);

    let mut cq_q = restored.world.query::<&ConstructionQueue>();
    assert_eq!(cq_q.iter(&restored.world).next().unwrap().item_count, 3);

    let sim_time = restored
        .world
        .get_resource::<SimulationTime>()
        .expect("SimulationTime resource must survive");
    assert!((sim_time.elapsed_s - 86400.0).abs() < 1e-9);
    assert_eq!(sim_time.tick, 86400);

    let seed = restored
        .world
        .get_resource::<GameSeed>()
        .expect("GameSeed resource must survive");
    assert_eq!(seed.0, 0xDEADBEEFCAFEBABE);

    // 5. Metadata round-trips.
    assert_eq!(restored.metadata.format_version, 1);
    assert_eq!(restored.metadata.seed, 0xDEADBEEFCAFEBABE);
    assert_eq!(restored.metadata.playtime_s, 86400);
}

#[test]
fn roundtrip_preserves_empty_world() {
    let mut world = build_world_with_test_types();
    world.insert_resource(SimulationTime::default());
    world.insert_resource(GameSeed(42));

    let metadata = SaveMetadata::new_now(42, 0, "0.4.0");
    let registry = world.resource::<AppTypeRegistry>().clone();
    let ron_text = snapshot_world_with_registry(&world, &registry, metadata)
        .expect("empty world snapshot must succeed");

    let restored = restore_world(&ron_text, build_world_with_test_types)
        .expect("empty world restore must succeed");

    assert_eq!(
        restored
            .world
            .get_resource::<GameSeed>()
            .expect("GameSeed survives")
            .0,
        42
    );
}

#[test]
fn restore_rejects_garbage() {
    let result = restore_world("not valid ron at all", build_world_with_test_types);
    assert!(result.is_err(), "garbage must not parse");
}

#[test]
fn restore_rejects_too_old_format_version() {
    use helios_ascension::persistence::{Body, SaveFile, SchemaKind, MIN_SUPPORTED_VERSION};

    let envelope = SaveFile {
        metadata: SaveMetadata {
            format_version: 0, // pre-history
            saved_at_unix_s: 0,
            playtime_s: 0,
            seed: 0,
            helios_version: "0.0.0".to_string(),
        },
        body: Body {
            schema: SchemaKind::SceneRon,
            data: String::new(),
        },
    };
    let ron_text = ron::to_string(&envelope).expect("serialize envelope");
    let result = restore_world(&ron_text, build_world_with_test_types);
    assert!(result.is_err(), "v0 must be rejected");
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains(&MIN_SUPPORTED_VERSION.to_string()),
        "error must mention minimum supported version, got: {msg}"
    );
}

#[test]
fn restore_rejects_too_new_format_version() {
    use helios_ascension::persistence::{Body, SaveFile, SchemaKind, FORMAT_VERSION};

    let envelope = SaveFile {
        metadata: SaveMetadata {
            format_version: FORMAT_VERSION + 100,
            saved_at_unix_s: 0,
            playtime_s: 0,
            seed: 0,
            helios_version: "0.0.0".to_string(),
        },
        body: Body {
            schema: SchemaKind::SceneRon,
            data: String::new(),
        },
    };
    let ron_text = ron::to_string(&envelope).expect("serialize envelope");
    let result = restore_world(&ron_text, build_world_with_test_types);
    assert!(result.is_err(), "future version must be rejected");
}
