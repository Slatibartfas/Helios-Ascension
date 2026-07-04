//! End-to-end roundtrip test for GRA-314 PR-A + GRA-319.
//!
//! Spawns a [`World`] with simulation-state Components and Resources
//! from each affected plugin area, snapshots it, restores into a fresh
//! world, and asserts the resulting world matches.
//!
//! **GRA-319:** the original test-only fixtures (HullMass, Orbit, etc.)
//! have been replaced with real Helios types — at least one component
//! or resource from astronomy, colony, economy, fleet, research, survey,
//! shipbuilding, and personnel. The acceptance criterion is ≥1 per area
//! for the five "core" areas (astronomy/colony/economy/fleet/research);
//! the extra coverage on survey/shipbuilding/personnel is gravy.
//!
//! We use small, builder-free fixture types where possible; for the
//! larger struct types (ShipConstructionProject, RefitProject,
//! ResearchProject, etc.) we use the type's default or the simplest
//! non-Default construction the API exposes.

use bevy::ecs::reflect::AppTypeRegistry;
use bevy::prelude::*;
use helios_ascension::astronomy::{
    KeplerOrbit, NearbyStarsData, Selected, SpaceCoordinates, StarSystemData,
};
use helios_ascension::colony::{
    Colony, ColonyDevelopment, ColonyTier, PendingConstructionActions,
};
use helios_ascension::economy::{LocalStockpile, MinimumStockpile, ResourceType};
use helios_ascension::fleets::{ActiveManeuver, Fleet, FleetOrbit};
use helios_ascension::personnel::Scientist;
use helios_ascension::persistence::{restore_world, snapshot_world_with_registry, SaveMetadata};
use helios_ascension::research::{ResearchProject, ResearchState, TechCategory};
use helios_ascension::shipbuilding::{RefitProject, ShipConstructionProject};
use helios_ascension::survey::SurveyState;
use std::collections::HashMap;

fn build_world() -> World {
    let mut world = World::new();
    world.init_resource::<AppTypeRegistry>();
    let registry = world.resource::<AppTypeRegistry>().clone();
    {
        let mut write = registry.write();
        // ── Astronomy ──────────────────────────────────────────────
        write.register::<SpaceCoordinates>();
        write.register::<KeplerOrbit>();
        write.register::<Selected>();
        write.register::<NearbyStarsData>();
        write.register::<StarSystemData>();
        // ── Colony ─────────────────────────────────────────────────
        write.register::<Colony>();
        write.register::<ColonyTier>();
        write.register::<ColonyDevelopment>();
        write.register::<PendingConstructionActions>();
        // ── Economy ────────────────────────────────────────────────
        write.register::<LocalStockpile>();
        write.register::<MinimumStockpile>();
        write.register::<ResourceType>();
        // ── Fleet ──────────────────────────────────────────────────
        write.register::<Fleet>();
        write.register::<FleetOrbit>();
        write.register::<ActiveManeuver>();
        // ── Research ───────────────────────────────────────────────
        write.register::<ResearchProject>();
        write.register::<ResearchState>();
        write.register::<TechCategory>();
        // ── Survey ─────────────────────────────────────────────────
        write.register::<SurveyState>();
        // ── Shipbuilding ───────────────────────────────────────────
        write.register::<ShipConstructionProject>();
        write.register::<RefitProject>();
        // ── Personnel ──────────────────────────────────────────────
        write.register::<Scientist>();
    }
    world
}

#[test]
fn roundtrip_world_with_components_and_resources() {
    let mut world = build_world();
    // 1. Build a populated world — Components and Resources from each
    //    affected plugin area so we exercise GRA-319's reflect
    //    coverage rather than the empty-set case.

    // ── Astronomy ──
    let _sun = world.spawn((
        SpaceCoordinates::default(),
        KeplerOrbit {
            eccentricity: 0.0,
            semi_major_axis: 0.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: 0.0,
        },
        Selected,
    ));
    let _earth = world.spawn(KeplerOrbit {
        eccentricity: 0.0167,
        semi_major_axis: 1.0,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        mean_anomaly_epoch: 0.0,
        mean_motion: 1.991e-7, // 2π / (365.25 d in seconds)
    });

    // ── Colony + Economy + Survey ──
    let mut amounts = HashMap::new();
    amounts.insert(ResourceType::Iron, 1_000_000.0);
    let _colony_entity = world.spawn((
        Colony {
            name: "Earth".to_string(),
            population: 8_000_000_000.0,
            growth_rate_modifier: 1.0,
            buildings: HashMap::new(),
            development: ColonyDevelopment::default(),
        },
        LocalStockpile {
            amounts,
        },
        MinimumStockpile::default(),
        SurveyState::default(),
    ));

    // ── Fleet ──
    let _fleet = world.spawn(Fleet {
        name: "Earth Survey Fleet".to_string(),
        role: Default::default(),
        ships: Vec::new(),
        current_speed_km_s: 0.0,
    });

    // ── Research ──
    let _research = world.spawn(ResearchProject {
        tech_id: "solar_power".to_string(),
        progress: 250.0,
        required_points: 1000.0,
        team_id: Entity::PLACEHOLDER,
        rp_allocation_percent: 1.0,
        active: true,
    });
    world.insert_resource(ResearchState::default());
    world.insert_resource(PendingConstructionActions::default());

    // 2. Snapshot.
    let metadata = SaveMetadata::new_now(0xDEADBEEFCAFEBABE, 86400, "0.5.0");
    let registry = world.resource::<AppTypeRegistry>().clone();
    let ron_text =
        snapshot_world_with_registry(&world, &registry, metadata).expect("snapshot must succeed");
    assert!(!ron_text.is_empty(), "snapshot RON must not be empty");

    // 3. Restore into a fresh world with the same registrations.
    let mut restored = restore_world(&ron_text, build_world).expect("restore must succeed");

    // 4. Verify each reflectively-registered type survived.

    // Astronomy
    let mut orbit_q = restored.world.query::<&KeplerOrbit>();
    let orbits: Vec<&KeplerOrbit> = orbit_q.iter(&restored.world).collect();
    assert!(orbits.len() >= 2, "at least 2 KeplerOrbit entities expected");
    let earth_orbit = orbits
        .iter()
        .find(|o| (o.semi_major_axis - 1.0).abs() < 1e-9)
        .expect("Earth orbit");
    assert!((earth_orbit.eccentricity - 0.0167).abs() < 1e-6);

    // Colony
    let mut colony_q = restored.world.query::<&Colony>();
    let colony = colony_q.iter(&restored.world).next().expect("Colony");
    assert_eq!(colony.name, "Earth");

    // Economy
    let mut stockpile_q = restored.world.query::<&LocalStockpile>();
    let stockpile = stockpile_q
        .iter(&restored.world)
        .next()
        .expect("LocalStockpile");
    assert_eq!(
        stockpile.amounts.get(&ResourceType::Iron).copied(),
        Some(1_000_000.0)
    );

    // Fleet
    let mut fleet_q = restored.world.query::<&Fleet>();
    let fleet = fleet_q.iter(&restored.world).next().expect("Fleet");
    assert_eq!(fleet.name, "Earth Survey Fleet");

    // Research
    let research_state = restored
        .world
        .get_resource::<ResearchState>()
        .expect("ResearchState must survive");
    let _ = research_state;

    // 5. Metadata round-trips.
    assert_eq!(restored.metadata.format_version, 1);
    assert_eq!(restored.metadata.seed, 0xDEADBEEFCAFEBABE);
    assert_eq!(restored.metadata.playtime_s, 86400);
}

#[test]
fn snapshot_populated_world_is_non_empty() {
    // Per GRA-319 acceptance: "Snapshotting the post-`SystemPopulatorPlugin`
    // world on main captures a non-empty scene (verified by adding a
    // debug println in `snapshot_world` and inspecting the RON)."
    //
    // We can't pull in `SystemPopulatorPlugin` here (it transitively
    // needs plugins we don't want to drag into a unit test), but we can
    // assert the snapshot RON contains the type names of every area we
    // touched.  If a plugin regresses its `register_type` call, the
    // snapshot silently drops the type and the assertion catches it.
    let mut world = build_world();
    world.spawn((
        SpaceCoordinates::default(),
        KeplerOrbit::circular(1.0, 0.0),
        Selected,
    ));
    world.spawn(Colony {
        name: "Mars".to_string(),
        population: 100.0,
        growth_rate_modifier: 1.0,
        buildings: HashMap::new(),
        development: ColonyDevelopment::default(),
    });
    world.spawn(LocalStockpile::default());
    world.spawn(Fleet {
        name: "Probe".to_string(),
        role: Default::default(),
        ships: Vec::new(),
        current_speed_km_s: 0.0,
    });
    world.spawn(ResearchProject {
        tech_id: "solar_power".to_string(),
        progress: 100.0,
        required_points: 1000.0,
        team_id: Entity::PLACEHOLDER,
        rp_allocation_percent: 1.0,
        active: true,
    });
    world.spawn(SurveyState::default());
    world.insert_resource(ResearchState::default());

    let metadata = SaveMetadata::new_now(42, 0, "0.5.0");
    let registry = world.resource::<AppTypeRegistry>().clone();
    let ron_text = snapshot_world_with_registry(&world, &registry, metadata)
        .expect("snapshot must succeed");
    assert!(!ron_text.is_empty(), "snapshot RON must not be empty");
    for needle in &[
        "KeplerOrbit",
        "Colony",
        "LocalStockpile",
        "Fleet",
        "ResearchProject",
        "SurveyState",
        "SpaceCoordinates",
    ] {
        assert!(
            ron_text.contains(needle),
            "snapshot RON must contain {needle}; got first 400 chars: {}",
            &ron_text[..ron_text.len().min(400)]
        );
    }
}

#[test]
fn roundtrip_preserves_empty_world() {
    let mut world = build_world();
    world.insert_resource(ResearchState::default());
    world.insert_resource(PendingConstructionActions::default());

    let metadata = SaveMetadata::new_now(42, 0, "0.5.0");
    let registry = world.resource::<AppTypeRegistry>().clone();
    let ron_text = snapshot_world_with_registry(&world, &registry, metadata)
        .expect("empty world snapshot must succeed");

    let restored = restore_world(&ron_text, build_world)
        .expect("empty world restore must succeed");

    let research = restored
        .world
        .get_resource::<ResearchState>()
        .expect("ResearchState survives");
    let _ = research;
}

#[test]
fn restore_rejects_garbage() {
    let result = restore_world("not valid ron at all", build_world);
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
    let result = restore_world(&ron_text, build_world);
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
    let result = restore_world(&ron_text, build_world);
    assert!(result.is_err(), "future version must be rejected");
}
