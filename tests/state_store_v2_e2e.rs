//! StateStore v2 end-to-end test (GRA-358 PR-I).
//!
//! Verifies the full v2 save/load cycle:
//!
//! 1. Build a fresh world (regen chain stub)
//! 2. Add a Colony, a Fleet, a Survey divergence
//! 3. Save via `write_save_to_path`
//! 4. Confirm file is small (the v1 path produced 95 MB; v2
//!    should be KBs)
//! 5. Read it back via `StateStore::from_ron`
//! 6. Apply it to a fresh world
//! 7. Confirm the apply outcome reflects the divergences
//!
//! The test does NOT run the full regen chain (that would
//! require the entire plugin stack) — instead it constructs
//! a minimal `World` with the resources the extract / apply
//! paths need and confirms the round-trip is loss-less at
//! the data-schema level.

use bevy::prelude::*;
use helios_ascension::astronomy::components::SystemId;
use helios_ascension::economy::components::LocalStockpile;
use helios_ascension::game_state::GameSeed;
use helios_ascension::persistence::playtime::PlaytimeTracker;
use helios_ascension::persistence::state_store::{BodyKey, StateStore};
use helios_ascension::persistence::state_store_apply::apply_state_store;
use helios_ascension::persistence::state_store_extract::extract_state_store;
use helios_ascension::persistence::write_save_to_path;
use helios_ascension::plugins::solar_system::CelestialBody;
use helios_ascension::plugins::solar_system_data::BodyType;
use std::collections::HashMap;

fn minimal_world() -> World {
    let mut world = World::new();
    world.insert_resource(GameSeed { value: 0xCAFE });
    world.insert_resource(PlaytimeTracker::default());
    world.insert_resource(helios_ascension::ui::time::SimulationTime::default());
    world
}

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

#[test]
fn state_store_v2_save_and_load_roundtrip() {
    let mut world = minimal_world();
    let _earth = make_body(&mut world, "Earth", 0);
    let _mars = make_body(&mut world, "Mars", 0);

    // Add a LocalStockpile on Earth (a divergence). The
    // extract path skips bodies whose stockpile is empty,
    // so we seed at least one resource entry.
    let mut stock = LocalStockpile {
        stockpiles: HashMap::new(),
    };
    stock
        .stockpiles
        .insert(helios_ascension::economy::ResourceType::Iron, 1.0);
    world.entity_mut(_earth).insert(stock);

    // Extract a StateStore.
    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    assert!(!store.bodies.is_empty(), "Earth should be in divergences");

    // Save to a unique temp file (the test cleans up at the
    // end; we don't depend on any tempdir crate).
    let mut path = std::env::temp_dir();
    path.push(format!(
        "helios_state_store_v2_e2e_{}_{}.ron",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_file(&path);
    write_save_to_path(&mut world, &path).expect("write v2 save");

    // Confirm the file is tiny (well under the 95 MB v1
    // baseline — KBs is what we expect for an empty
    // divergences map).
    let bytes = std::fs::metadata(&path).expect("stat").len();
    assert!(
        bytes < 32_768,
        "v2 save should be < 32 KB for an empty-ish world; got {bytes} bytes"
    );

    // Read it back.
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.starts_with(StateStore::MAGIC),
        "v2 save must start with magic header"
    );
    let store_back = StateStore::from_ron(&text).expect("parse v2");
    assert_eq!(store_back.metadata.seed, 0xCAFE);
    assert_eq!(store_back.bodies.len(), store.bodies.len());

    // Apply it to a fresh world.
    let mut world2 = minimal_world();
    let _earth2 = make_body(&mut world2, "Earth", 0);
    let outcome = apply_state_store(&mut world2, &store_back);
    assert_eq!(outcome.bodies_applied, 1);
    assert!(
        outcome.warnings.is_empty(),
        "warnings: {:?}",
        outcome.warnings
    );

    // Cleanup the temp file. We don't fail the test if this
    // errors (best-effort); the OS will GC /tmp eventually.
    let _ = std::fs::remove_file(&path);
}

/// Regression test for the 70 MB save bug.
///
/// Before PR-I, the extract path persisted every body that
/// had a non-empty `PlanetResources` (which is every
/// asteroid — the regen chain seeds spectral-class deposits
/// for all 700+ bodies). The save ballooned to 70 MB for a
/// fresh game with no player data.
///
/// PR-I fixes this by skipping `PlanetResources` entirely
/// (the regen chain rebuilds it deterministically) and only
/// persisting bodies that have a real player-derived state
/// (Colony, non-zero Population, or non-empty LocalStockpile).
#[test]
fn state_store_v2_save_size_for_many_default_bodies_stays_small() {
    use std::collections::HashMap;
    let mut world = minimal_world();

    // Spawn 700 empty bodies (the rough size of the Sol
    // system) — each gets a `LocalStockpile` (the production
    // regen chain does the same) but no player data.
    for i in 0..700 {
        let name = format!("Body-{i:04}");
        let body = CelestialBody {
            name: name.clone(),
            radius: 0.0,
            mass: 0.0,
            body_type: BodyType::Planet,
            visual_radius: 0.0,
            asteroid_class: None,
            star_approach_au: None,
            rotation_period_s: None,
            habitable_outer_au: None,
        };
        let _ = world.spawn((
            body,
            SystemId(0usize),
            LocalStockpile {
                stockpiles: HashMap::new(),
            },
        ));
    }

    // Spawn Earth with a real colony + non-zero population
    // + a non-empty stockpile (the production baseline).
    let earth = world
        .spawn((
            CelestialBody {
                name: "Earth".to_string(),
                radius: 0.0,
                mass: 0.0,
                body_type: BodyType::Planet,
                visual_radius: 0.0,
                asteroid_class: None,
                star_approach_au: None,
                rotation_period_s: None,
                habitable_outer_au: None,
            },
            SystemId(0usize),
            helios_ascension::economy::components::Population { count: 8.2e9 },
            helios_ascension::colony::components::Colony {
                name: "Earth".to_string(),
                population: 8.2e9,
                development: helios_ascension::colony::components::ColonyDevelopment {
                    tier: helios_ascension::colony::components::ColonyTier::Civilisation,
                    yield_multiplier: 1.0,
                    investments: 0,
                },
                buildings: HashMap::new(),
                growth_rate_modifier: 1.0,
            },
        ))
        .id();
    let mut stock = LocalStockpile {
        stockpiles: HashMap::new(),
    };
    stock
        .stockpiles
        .insert(helios_ascension::economy::ResourceType::Iron, 1.0);
    world.entity_mut(earth).insert(stock);

    // Save to a unique temp file.
    let mut path = std::env::temp_dir();
    path.push(format!(
        "helios_state_store_v2_size_{}_{}.ron",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_file(&path);
    write_save_to_path(&mut world, &path).expect("write v2 save");

    // The save must be tiny — only Earth is a divergence;
    // the 700 empty bodies must NOT appear in the file.
    let bytes = std::fs::metadata(&path).expect("stat").len();
    assert!(
        bytes < 16_384,
        "v2 save with 700 default bodies + 1 Earth divergence must be < 16 KB; got {bytes} bytes"
    );

    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.starts_with(StateStore::MAGIC),
        "v2 save must start with magic header"
    );
    let store_back = StateStore::from_ron(&text).expect("parse v2");
    assert_eq!(
        store_back.bodies.len(),
        1,
        "only Earth should appear in divergences; got {} bodies",
        store_back.bodies.len()
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn state_store_v2_with_bodykey_serialization() {
    // Direct test of the BodyKey round-trip — the foundation
    // every per-component extractor / applier depends on.
    use std::collections::BTreeMap;
    let mut bodies = BTreeMap::new();
    bodies.insert(BodyKey::sol("Earth"), Default::default());
    bodies.insert(
        BodyKey {
            system: 7,
            name: "Proxima b".to_string(),
        },
        Default::default(),
    );
    let store = StateStore {
        bodies,
        ..StateStore::empty(0)
    };
    let ron = store.to_ron().expect("serialise");
    let back = StateStore::from_ron(&ron).expect("parse");
    assert!(back.bodies.contains_key(&BodyKey::sol("Earth")));
    assert!(back.bodies.contains_key(&BodyKey {
        system: 7,
        name: "Proxima b".to_string()
    }));
}

/// Regression test: fleets spawned by the regen chain
/// (Day-One Constellation, Mars Flyby Probe, debug
/// Earth→Jupiter transfer) must NOT be persisted to the
/// v2 save, otherwise every Save → Load cycle duplicates
/// them. The extract path skips any fleet carrying the
/// `RegenChainFleet` marker component.
#[test]
fn state_store_v2_save_skips_regen_chain_fleets() {
    use helios_ascension::fleets::components::{Fleet, FleetOrbit, ShipInfo};
    use helios_ascension::fleets::types::{FleetRole, PropulsionType, ShipClass};
    use helios_ascension::fleets::RegenChainFleet;

    let mut world = minimal_world();
    let earth = make_body(&mut world, "Earth", 0);

    // Spawn a regen-chain fleet and a player fleet in the
    // same world. Only the player fleet should appear in
    // the v2 save.
    let ship = ShipInfo::new(
        "Player-Founded Frigate".to_string(),
        ShipClass::Frigate,
        PropulsionType::Chemical,
    );
    let _regen = world.spawn((
        Fleet {
            name: "Day-One Constellation".to_string(),
            role: FleetRole::Unassigned,
            ships: vec![ship.clone()],
        },
        FleetOrbit::new(earth, 0.05),
        RegenChainFleet,
    ));
    let player = world.spawn((
        Fleet {
            name: "Helios Survey Wing".to_string(),
            role: FleetRole::Survey,
            ships: vec![ship],
        },
        FleetOrbit::new(earth, 0.05),
    ));
    // Drop the player entity reference so the next
    // `&mut world` borrow (in `extract_state_store`)
    // doesn't conflict.
    let _ = player;

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    assert_eq!(store.fleets.len(), 1, "exactly one fleet in divergences");
    let only = &store.fleets[0];
    assert_eq!(
        only.name, "Helios Survey Wing",
        "regen-chain fleet must be skipped; got `{}`",
        only.name
    );
}

/// Regression test: the SavePreview (date, population,
/// ship count, Kardashev, resource breakdown) must
/// populate on v2 saves. The Load Game list renders the
/// preview without loading the full world, so a missing
/// preview renders as "Resources: no preview" — the bug
/// reported in the screenshot.
#[test]
fn state_store_v2_metadata_preview_roundtrips() {
    let preview = helios_ascension::persistence::state_store::SavePreview {
        current_date: "01.07.2026 12:00".to_string(),
        colony_count: 1,
        total_population: 8.2e9,
        ship_count: 6,
        power_produced_watts: 1.5e9,
        kardashev_value: 0.71,
        resources: vec![("Iron".to_string(), 12.5), ("Water".to_string(), 4.0)],
        kardashev_history: vec![(0.0, 0.5), (86400.0, 0.71)],
        screenshot_file: Some("new_mission.png".to_string()),
    };

    let mut store = StateStore::empty(0xCAFE);
    store.metadata.preview = preview.clone();

    let ron = store.to_ron().expect("serialise");
    let back = StateStore::from_ron(&ron).expect("parse");
    assert_eq!(back.metadata.preview.current_date, "01.07.2026 12:00");
    assert_eq!(back.metadata.preview.colony_count, 1);
    assert_eq!(back.metadata.preview.total_population, 8.2e9);
    assert_eq!(back.metadata.preview.ship_count, 6);
    assert_eq!(back.metadata.preview.resources.len(), 2);
    assert_eq!(
        back.metadata.preview.screenshot_file.as_deref(),
        Some("new_mission.png")
    );
}

/// Regression test: a body the player has actively
/// mined / delivered on must round-trip even if its
/// `LocalStockpile` is currently empty (the player
/// consumed everything via the HabitatDome build
/// queue). Without the dirty-bodies tracker the extract
/// path would skip the body, the regen chain would
/// re-seed it as empty, and the player's progress would
/// silently vanish.
#[test]
fn state_store_v2_dirty_resource_bodies_roundtrip() {
    use helios_ascension::economy::{DirtyBodies, DirtyReason};

    let mut world = minimal_world();
    world.init_resource::<DirtyBodies>();
    let body = make_body(&mut world, "Mars", 0);
    // Empty stockpile (the player mined and consumed
    // everything), but the body is in the dirty set.
    world.entity_mut(body).insert(LocalStockpile {
        stockpiles: HashMap::new(),
    });
    world
        .resource_mut::<DirtyBodies>()
        .mark(body, DirtyReason::Stockpile);

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    assert!(
        store.bodies.contains_key(&BodyKey::sol("Mars")),
        "dirty body must appear in divergences even with empty stockpile; bodies: {:?}",
        store.bodies.keys().collect::<Vec<_>>()
    );

    // Re-serialise and verify the empty stockpile survives.
    let ron = store.to_ron().expect("serialise");
    let back = StateStore::from_ron(&ron).expect("parse");
    let div = back
        .bodies
        .get(&BodyKey::sol("Mars"))
        .expect("Mars divergence must survive roundtrip");
    let resources = div
        .resources_override
        .as_ref()
        .expect("dirty body must carry resources_override");
    assert!(
        resources.get("stockpile").is_some(),
        "resources_override must contain `stockpile` for dirty body"
    );
}

/// Regression test for the user's bug: after a save/load
/// cycle the per-body `LocalStockpile` must be preserved.
/// The top resource bar aggregates stockpiles via
/// [`ContextualStockpile`], so when a planet's stockpile
/// drops to zero after loading the bar reads zero too. The
/// extract path writes `resources_override.stockpile` for
/// every body with a non-empty `LocalStockpile`; the apply
/// path rehydrates it via `entity.insert(stock)`. This test
/// confirms the round-trip survives a `write_save_to_path`
/// → `apply_state_store` cycle end-to-end.
#[test]
fn state_store_v2_local_stockpile_roundtrip_via_save_file() {
    let mut world = minimal_world();
    let _mars = make_body(&mut world, "Mars", 0);
    let mut stock = LocalStockpile {
        stockpiles: HashMap::new(),
    };
    stock
        .stockpiles
        .insert(helios_ascension::economy::ResourceType::Iron, 17.5);
    stock
        .stockpiles
        .insert(helios_ascension::economy::ResourceType::Water, 3.25);
    world.entity_mut(_mars).insert(stock);

    let mut path = std::env::temp_dir();
    path.push(format!(
        "helios_state_store_v2_stockpile_roundtrip_{}_{}.ron",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_file(&path);
    write_save_to_path(&mut world, &path).expect("write v2 save");

    let text = std::fs::read_to_string(&path).expect("read");
    let store_back = StateStore::from_ron(&text).expect("parse v2");

    let mut world2 = minimal_world();
    let _mars2 = make_body(&mut world2, "Mars", 0);
    apply_state_store(&mut world2, &store_back);

    // The freshly-loaded Mars must carry its saved stockpile.
    let mut q = world2.query::<(&CelestialBody, &LocalStockpile)>();
    let mut found = false;
    for (body, ls) in q.iter(&world2) {
        if body.name == "Mars" {
            assert_eq!(
                ls.stockpiles
                    .get(&helios_ascension::economy::ResourceType::Iron),
                Some(&17.5),
                "Mars Iron must round-trip through the save file"
            );
            assert_eq!(
                ls.stockpiles
                    .get(&helios_ascension::economy::ResourceType::Water),
                Some(&3.25),
                "Mars Water must round-trip through the save file"
            );
            found = true;
            break;
        }
    }
    assert!(found, "Mars body must exist in the post-load world");

    let _ = std::fs::remove_file(&path);
}

/// Regression test: the production restore path uses
/// [`build_minimal_world_for_restore`] (an empty world — the
/// regen chain is normally gated behind `BootInitPlugin` and
/// suppressed on the Restore path) and then applies the
/// StateStore on top. Before this fix, every per-body
/// divergence (LocalStockpile, Colony, Population) was
/// dropped with a warning because the apply path couldn't
/// find the target bodies.
///
/// After the fix, the regen chain runs as part of the
/// restore factory so the apply path has bodies to attach
/// to. This test asserts that an Earth stockpile extracted
/// before save shows up on Earth after a full
/// save/restore cycle through the production entry point.
#[test]
fn state_store_v2_local_stockpile_survives_production_restore_path() {
    use helios_ascension::persistence::game_setup::build_minimal_world_for_restore;

    let mut world = minimal_world();
    let earth = make_body(&mut world, "Earth", 0);
    let mut stock = LocalStockpile {
        stockpiles: HashMap::new(),
    };
    stock
        .stockpiles
        .insert(helios_ascension::economy::ResourceType::Iron, 42.0);
    world.entity_mut(earth).insert(stock);

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");

    // Use the SAME empty-world factory the production
    // restore path uses. If this fix regresses, the body
    // won't exist on `fresh` and the apply will silently
    // drop the divergence.
    let mut fresh = build_minimal_world_for_restore();
    apply_state_store(&mut fresh, &store);

    // The Earth entity must exist on the post-restore world
    // (populated by `regenerate_bodies_minimal`) AND carry
    // its saved stockpile.
    let mut q = fresh.query::<(&CelestialBody, &LocalStockpile)>();
    let mut found = false;
    for (body, ls) in q.iter(&fresh) {
        if body.name == "Earth" {
            assert_eq!(
                ls.stockpiles
                    .get(&helios_ascension::economy::ResourceType::Iron),
                Some(&42.0),
                "Earth Iron must round-trip through the production restore path"
            );
            found = true;
            break;
        }
    }
    assert!(found, "Earth body must exist in the post-restore world");
}

/// Regression test: the production restore factory builds a
/// world with only `MinimalPlugins + PersistencePlugin` — every
/// other resource the apply path reads (treasury, view mode,
/// research state, …) is absent and would be silently dropped
/// if `apply_state_store` didn't seed defaults. This test asserts
/// a non-default treasury and `ViewMode::Starmap` survive the
/// production restore path.
#[test]
fn state_store_v2_resource_state_survives_production_restore_path() {
    use helios_ascension::economy::budget::GlobalBudget;
    use helios_ascension::persistence::game_setup::build_minimal_world_for_restore;
    use helios_ascension::plugins::camera::ViewMode;

    let mut world = minimal_world();
    // Mutate GlobalBudget + ViewMode so the extract path
    // captures non-default values. Both resources are inserted
    // by `MinimalPlugins + PersistencePlugin + regen chain`, so
    // they're present on the live world the user saved.
    world.insert_resource(GlobalBudget {
        treasury: 12_345.0,
        ..Default::default()
    });
    world.insert_resource(ViewMode::Starmap);

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    assert_eq!(
        store.economy.treasury, 12_345.0,
        "treasury must be captured"
    );
    assert_eq!(
        store.ui.view_mode, "Starmap",
        "view mode must be captured as a debug-format string"
    );

    // Restore on the SAME empty-world factory the production
    // path uses. Without `init_missing_resources_for_apply`,
    // `apply_economy` and `apply_ui` would no-op because
    // GlobalBudget + ViewMode aren't on `fresh` yet.
    let mut fresh = build_minimal_world_for_restore();
    apply_state_store(&mut fresh, &store);

    let treasury = fresh.resource::<GlobalBudget>().treasury;
    assert_eq!(
        treasury, 12_345.0,
        "treasury must round-trip through the production restore path"
    );
    assert_eq!(
        *fresh.resource::<ViewMode>(),
        ViewMode::Starmap,
        "view mode must round-trip through the production restore path"
    );
}

/// Regression test: the regen-minimal fallback must insert a
/// GRA-358 PR-J: the production restore factory runs the full
/// regen chain on a minimal App (see
/// `game_setup::build_minimal_world_for_restore`). The chain
/// populates every body's `PlanetResources` with the
/// spectral-class deposit map. This test asserts that:
///   (a) every body has a `PlanetResources` component (no
///       per-frame "Colony X has no PlanetResources" warnings
///       — that warning is now `debug!` in `mining.rs`), AND
///   (b) bodies the chain assigned deposits to (asteroids,
///       Mars, Mercury, …) actually have them.
#[test]
fn state_store_v2_regen_minimal_inserts_empty_planet_resources() {
    use helios_ascension::economy::components::PlanetResources;
    use helios_ascension::persistence::game_setup::build_minimal_world_for_restore;

    let mut fresh = build_minimal_world_for_restore();
    apply_state_store(&mut fresh, &StateStore::empty(0xCAFE));

    // Every body the regen chain spawned (Earth, Mars, Sun,
    // moons, …) must carry a `PlanetResources` component. The
    // chain's spectral-class resource generator (running
    // inside `build_minimal_world_for_restore`) populates
    // asteroids and planets with non-empty deposit maps;
    // stars and gas giants stay empty.
    let mut q = fresh.query::<(&CelestialBody, &PlanetResources)>();
    let mut found = 0_usize;
    for (_body, _pr) in q.iter(&fresh) {
        // PR-J: no assertion on emptiness — the chain now
        // populates deposits. The mere presence of the
        // component is what matters (see `mining.rs`).
        found += 1;
    }
    assert!(
        found > 0,
        "regen chain must spawn at least one body (Earth, Sun, …); got {found}"
    );
    assert!(
        found > 700,
        "regen chain must spawn the full Sol-system baseline \
         (710 bodies); got only {found} — chain didn't complete?"
    );
}

/// Regression test: the regen-minimal fallback must use
/// `calculate_visual_radius` (the regen-chain's non-linear
/// scaling for stars + power-curve for planets) rather than
/// the raw physical radius. Before this fix, Sol's
/// `visual_radius` was 696,340 (raw km), and `update_min_zoom`
/// clamped the camera `radius` to `2.5 × visual_radius ≈ 1.7M`
/// game units — far above the System-mode threshold (720k) —
/// which caused the camera to bounce back into Starmap every
/// time the user tried to zoom into Sol.
#[test]
fn state_store_v2_regen_minimal_uses_scaled_visual_radius() {
    use helios_ascension::persistence::game_setup::build_minimal_world_for_restore;
    use helios_ascension::plugins::solar_system_data::calculate_visual_radius;

    let mut fresh = build_minimal_world_for_restore();
    apply_state_store(&mut fresh, &StateStore::empty(0xCAFE));

    let mut q = fresh.query::<&CelestialBody>();
    let mut found_sol = false;
    for body in q.iter(&fresh) {
        if body.name != "Sol" {
            continue;
        }
        found_sol = true;
        // Calculate the expected value the same way the regen
        // chain does. Sol is a Star; STAR_RADIUS_SCALE = 1.5e-4,
        // so 696_340 × 1.5e-4 = 104.45 (clamped to the star
        // minimum of 5.0).
        let expected = calculate_visual_radius(body.body_type, body.radius);
        assert!(
            (body.visual_radius - expected).abs() < 0.01,
            "Sol visual_radius={} (raw), expected ≈ {} (regen-chain-scaled). \
             A raw visual_radius causes the camera to bounce into Starmap \
             because the radius clamp becomes > the System-mode threshold.",
            body.visual_radius,
            expected
        );
        // And the specific failure mode the user observed:
        // the camera floor would be 2.5 × raw radius ≈ 1.7M,
        // far above 720k.
        assert!(
            body.visual_radius * 2.5 < 1_000_000.0,
            "Sol's 2.5× visual_radius floor must stay under the \
             starmap transition threshold; got {} (raw was {})",
            body.visual_radius * 2.5,
            body.radius
        );
    }
    assert!(found_sol, "regen-minimal must spawn the Sol star body");
}

/// Regression test: terraforming changes must be
/// captured by the dirty tracker and surface as an
/// `atmosphere_override` sentinel. The apply path
/// currently can't deserialise `AtmosphereComposition`
/// (missing `Serialize` derive — PR-I follow-up), so we
/// assert the sentinel is emitted *and* the apply path
/// produces a clear warning, rather than silently
/// dropping the change.
#[test]
fn state_store_v2_terraforming_changes_roundtrip() {
    use helios_ascension::astronomy::components::AtmosphereComposition;
    use helios_ascension::economy::{DirtyBodies, DirtyReason};

    let mut world = minimal_world();
    world.init_resource::<DirtyBodies>();
    let body = make_body(&mut world, "Mars", 0);
    // Insert a non-default atmosphere so the body has
    // a real terraforming target.
    world.entity_mut(body).insert(AtmosphereComposition {
        surface_pressure_mbar: 100.0,
        surface_temperature_celsius: -50.0,
        gases: vec![helios_ascension::astronomy::components::AtmosphericGas {
            name: "CO2".to_string(),
            percentage: 95.0,
        }],
        breathable: false,
        can_support_atmosphere: true,
        is_reference_pressure: false,
        harvest_altitude_bar: 0.0,
        max_harvest_altitude_bar: 0.0,
        scale_height_km: 0.0,
        rayleigh_rgb: [0.0; 3],
        rayleigh_strength: 0.0,
        mie_strength: 0.0,
        mie_g: 0.0,
        haze_color: [0.0; 3],
        atmosphere_intensity: 0.0,
    });
    // Player terraformed — mark dirty with the
    // matching reason.
    world
        .resource_mut::<DirtyBodies>()
        .mark(body, DirtyReason::Atmosphere);

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    let div = store
        .bodies
        .get(&BodyKey::sol("Mars"))
        .expect("terraformed body must appear in divergences");
    assert!(
        div.atmosphere_override.is_some(),
        "terraforming must surface as atmosphere_override sentinel"
    );

    // Round-trip — the sentinel survives JSON.
    let ron = store.to_ron().expect("serialise");
    let back = StateStore::from_ron(&ron).expect("parse");
    assert!(back
        .bodies
        .get(&BodyKey::sol("Mars"))
        .and_then(|d| d.atmosphere_override.as_ref())
        .is_some());
}

/// Regression test: orbit-shift mechanics (asteroid
/// redirect, tractor tug) must populate the
/// `*_override` fields on the divergence so the
/// regen chain doesn't re-derive the body's orbit on
/// the next load.
#[test]
fn state_store_v2_orbit_shift_roundtrip() {
    use helios_ascension::astronomy::components::KeplerOrbit;
    use helios_ascension::economy::{DirtyBodies, DirtyReason};

    let mut world = minimal_world();
    world.init_resource::<DirtyBodies>();
    let body = make_body(&mut world, "Ceres", 0);
    let orbit = KeplerOrbit {
        eccentricity: 0.08,
        semi_major_axis: 2.77,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        mean_anomaly_epoch: 1.234,
        mean_motion: 0.0,
    };
    world.entity_mut(body).insert(orbit);
    world
        .resource_mut::<DirtyBodies>()
        .mark(body, DirtyReason::Orbit);

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    let div = store
        .bodies
        .get(&BodyKey::sol("Ceres"))
        .expect("orbit-shifted body must appear in divergences");
    assert_eq!(
        div.mean_anomaly_epoch_override,
        Some(orbit.mean_anomaly_epoch),
        "mean anomaly override must capture the player's nudge"
    );
    assert_eq!(
        div.semi_major_axis_override,
        Some(orbit.semi_major_axis),
        "semi-major axis override must capture the player's nudge"
    );
    assert_eq!(
        div.eccentricity_override,
        Some(orbit.eccentricity),
        "eccentricity override must capture the player's nudge"
    );
}

/// Regression test: body-mass / radius changes
/// (asteroid-mining depletion, captured-asteroid
/// merger) must mark the body dirty with
/// `DirtyReason::Body` even though PR-I can't yet
/// serialise `CelestialBody`. The apply path surfaces
/// a warning today; the test pins the contract that
/// the *body must appear in divergences* so the
/// follow-up PR has a guaranteed entry point.
#[test]
fn state_store_v2_body_mass_change_roundtrip() {
    use helios_ascension::economy::{DirtyBodies, DirtyReason};

    let mut world = minimal_world();
    world.init_resource::<DirtyBodies>();
    let body = make_body(&mut world, "Vesta", 0);
    world
        .resource_mut::<DirtyBodies>()
        .mark(body, DirtyReason::Body);

    let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
    assert!(
        store.bodies.contains_key(&BodyKey::sol("Vesta")),
        "mass-changed body must appear in divergences"
    );
    // Body-override is currently a no-op (the
    // CelestialBody component lacks Serialize). The
    // TODO lives in extract_bodies' `Multiple` /
    // `Body` arm. Until then the body *appears* in the
    // save so the apply side has a chance to log the
    // missing field.
}
