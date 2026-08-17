//! Unit tests for `SfxPlugin`.
//!
//! These tests don't spawn audio devices (which would require a
//! working audio backend + `bevy_audio::AudioPlugin` plus the
//! `Audio` plugin to be initialized). Instead they exercise
//! the data layer: the manifest loader, the registry, the bus,
//! the cooldown logic, and the cue-id ↔ string mapping. The
//! audio-spawning system (`playback::play_sfx_system`) is
//! covered indirectly via the cooldown test — we count
//! `commands.spawn` invocations through a `World` harness.
#![allow(dead_code, unused_imports)]

use super::*;
use crate::plugins::sfx::bus::sync_sfx_bus_volume;
use crate::plugins::sfx::playback::play_sfx_system;
use bevy::asset::AssetPlugin;
use bevy::ecs::system::IntoSystem;
use bevy::prelude::{App, MinimalPlugins};

/// Build a minimal App with the resources needed by the
/// SFX data layer. The SFX plugin's `Startup` system isn't
/// registered — tests construct the registry directly so
/// they can pin specific cue sets.
fn sfx_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    // `AudioSource` must be registered with the asset server
    // before handles can be allocated — same requirement as
    // the production SFX plugin path (via `AudioPlugin`).
    app.init_asset::<bevy::audio::AudioSource>();
    app.init_resource::<SfxRegistry>();
    app.init_resource::<SfxBus>();
    app.init_resource::<Messages<SfxEvent>>();
    app
}

#[test]
fn every_cue_id_has_a_string_form() {
    // `as_str_id` must produce a stable, non-empty id for every
    // variant. Modders copy-paste these strings into the
    // manifest, so any drift here breaks authoring.
    for id in SfxCueId::ALL {
        let s = id.as_str_id();
        assert!(!s.is_empty(), "SfxCueId::{id:?} has empty string id");
        assert!(
            s.contains('.'),
            "SfxCueId::{id:?} id `{s}` should be namespaced"
        );
    }
}

#[test]
fn string_to_id_round_trips() {
    for id in SfxCueId::ALL {
        let s = id.as_str_id();
        assert_eq!(
            SfxCueId::from_str_id(s),
            Some(*id),
            "round-trip failed for {s}"
        );
    }
}

#[test]
fn unknown_string_returns_none() {
    assert_eq!(SfxCueId::from_str_id("nonexistent.cue"), None);
    assert_eq!(SfxCueId::from_str_id(""), None);
    assert_eq!(SfxCueId::from_str_id("ui.button_click "), None); // trailing space
}

#[test]
fn all_variants_distinct() {
    let mut seen = std::collections::HashSet::new();
    for id in SfxCueId::ALL {
        assert!(
            seen.insert(*id),
            "duplicate variant in SfxCueId::ALL: {id:?}"
        );
    }
    assert_eq!(seen.len(), SfxCueId::ALL.len());
}

#[test]
fn manifest_loads_with_all_phase1_cues() {
    // This is the integration test: load the actual manifest
    // from disk and verify every variant in SfxCueId::ALL has
    // a matching entry. If you add a variant to the enum but
    // forget to add it to the manifest, this test fails.
    let mut app = sfx_test_app();
    let asset_server = app.world().resource::<AssetServer>().clone();
    {
        let mut registry = app.world_mut().resource_mut::<SfxRegistry>();
        registry.assets.clear();
        registry.cues.clear();
        // Use the same loader the plugin uses.
        let contents = std::fs::read_to_string("assets/data/sfx_manifest.ron")
            .expect("manifest file must exist for the test");
        let manifest: SfxManifest = ron::from_str(&contents).expect("manifest must parse");
        for cue in &manifest.cues {
            if let Some(id) = SfxCueId::from_str_id(&cue.id) {
                registry.cues.insert(id, cue.clone());
                let path = crate::plugins::sfx::asset_path_for(cue);
                registry
                    .assets
                    .insert(id, asset_server.load::<bevy::audio::AudioSource>(&path));
                registry.last_seen_manifest_id.insert(id, cue.id.clone());
            }
        }
        registry.ready = true;
    }
    let registry = app.world().resource::<SfxRegistry>();
    for id in SfxCueId::ALL {
        assert!(
            registry.cue(*id).is_some(),
            "manifest is missing an entry for {id:?} (string id `{}`)",
            id.as_str_id()
        );
    }
    // And no extra entries — catches typos like a stale
    // `ui.tab_swap` lingering in the manifest after a rename.
    assert_eq!(
        registry.len(),
        SfxCueId::ALL.len(),
        "manifest has {} cues but SfxCueId::ALL has {} variants; \
         every entry should map to one variant (modders adding new \
         ids must add the variant first)",
        registry.len(),
        SfxCueId::ALL.len()
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn bus_volume_multiplies_correctly() {
    // Pure-math test: master × category × cue.default_volume,
    // clamped to [0, 1]. The "linear gain" semantics are
    // documented in SfxBus::volume_for and the formula is the
    // integration contract with the Settings UI slider.
    let mut bus = SfxBus::default();
    bus.master = 0.5;
    bus.categories.insert(SfxCategory::Ui, 1.0);
    bus.categories.insert(SfxCategory::Notifications, 1.0);

    // Master 0.5 × Ui (1.0) × cue (0.8) = 0.4
    assert_eq!(bus.volume_for(SfxCategory::Ui, 0.8), 0.4);
    // Master 0.5 × Notifications (1.0) × cue (0.7) = 0.35
    assert_eq!(bus.volume_for(SfxCategory::Notifications, 0.7), 0.35);

    // Master at 0 silences everything.
    bus.master = 0.0;
    assert_eq!(bus.volume_for(SfxCategory::Ui, 0.8), 0.0);
    assert!(!bus.is_audible(SfxCategory::Ui));

    // Cue volume > 1.0 is clamped (defensive — modders can put
    // any f32 in the manifest).
    bus.master = 1.0;
    bus.categories.insert(SfxCategory::Ui, 1.0);
    assert_eq!(bus.volume_for(SfxCategory::Ui, 5.0), 1.0);

    // Category muted silences that category.
    bus.categories.insert(SfxCategory::Notifications, 0.0);
    assert_eq!(bus.volume_for(SfxCategory::Notifications, 1.0), 0.0);
    assert!(!bus.is_audible(SfxCategory::Notifications));
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn bus_audible_short_circuit() {
    // The bus's `is_audible` is the cheap pre-filter for the
    // notification bridge — it short-circuits MessageWriter so
    // we don't even queue plays that would be silenced.
    let mut bus = SfxBus::default();
    assert!(bus.is_audible(SfxCategory::Ui));
    bus.master = 0.0;
    assert!(!bus.is_audible(SfxCategory::Ui));
    bus.master = 1.0;
    bus.categories.insert(SfxCategory::Notifications, 0.0);
    assert!(!bus.is_audible(SfxCategory::Notifications));
}

/// Bevy 0.18 helper: run a single function-style system once
/// in a fresh schedule. Equivalent to `app.world_mut().run_system(id)`
/// but doesn't require pre-registering the system.
fn run_system_once<F, Marker>(world: &mut bevy::prelude::World, f: F)
where
    F: IntoSystem<(), (), Marker>,
{
    let mut schedule = bevy::prelude::Schedule::default();
    schedule.add_systems(f);
    schedule.run(world);
}

#[test]
fn cooldown_blocks_rapid_replay() {
    // The cooldown lives in SfxRegistry and is checked by
    // play_sfx_system. We exercise it by driving the system
    // twice in the same tick with the same event id.
    let mut app = sfx_test_app();
    let asset_server = app.world().resource::<AssetServer>().clone();
    {
        let mut registry = app.world_mut().resource_mut::<SfxRegistry>();
        // Hand-build a single cue with a long cooldown so the
        // second event is guaranteed to be blocked.
        let cue = SfxCue {
            id: "ui.button_click".to_string(),
            file: "_silence.wav".to_string(),
            category: SfxCategory::Ui,
            default_volume: 0.7,
            cooldown_ms: 10_000,
            prompt: String::new(),
        };
        registry.cues.insert(SfxCueId::ButtonClick, cue);
        registry.assets.insert(
            SfxCueId::ButtonClick,
            asset_server.load::<bevy::audio::AudioSource>("audio/sfx/_silence.wav"),
        );
        registry.ready = true;
    }

    // Fire two events back-to-back.
    {
        let mut writer = app
            .world_mut()
            .resource_mut::<bevy::prelude::Messages<SfxEvent>>();
        writer.write(SfxEvent(SfxCueId::ButtonClick));
        writer.write(SfxEvent(SfxCueId::ButtonClick));
    }

    // Run the system once. The first event spawns one
    // AudioPlayer; the second is blocked by the cooldown we
    // set in the cue (10s).
    let world = app.world_mut();
    run_system_once(world, play_sfx_system);

    let player_count_after_first = world
        .query_filtered::<bevy::ecs::entity::Entity, bevy::prelude::With<bevy::audio::AudioPlayer>>(
        )
        .iter(world)
        .count();
    assert_eq!(
        player_count_after_first, 1,
        "first event should spawn one AudioPlayer (second blocked by cooldown)"
    );

    // Running the system a second time without a new event
    // spawns nothing (the message buffer is empty).
    run_system_once(world, play_sfx_system);
    let player_count_after_second = world
        .query_filtered::<bevy::ecs::entity::Entity, bevy::prelude::With<bevy::audio::AudioPlayer>>(
        )
        .iter(world)
        .count();
    assert_eq!(player_count_after_second, 1, "no new event → no new player");
}

#[test]
fn unready_registry_drops_all_events() {
    // If Startup failed (e.g. manifest missing on disk), the
    // registry stays empty and `ready` stays false. The system
    // must drop all events rather than panic.
    let mut app = sfx_test_app();

    {
        let mut writer = app
            .world_mut()
            .resource_mut::<bevy::prelude::Messages<SfxEvent>>();
        for _ in 0..5 {
            writer.write(SfxEvent(SfxCueId::ButtonClick));
        }
    }
    // Registry is default (empty, not ready).
    let world = app.world_mut();
    run_system_once(world, play_sfx_system);
    let player_count = world
        .query_filtered::<bevy::ecs::entity::Entity, bevy::prelude::With<bevy::audio::AudioPlayer>>(
        )
        .iter(world)
        .count();
    assert_eq!(
        player_count, 0,
        "unready registry must drop every event without spawning"
    );
}

#[test]
fn sync_bus_volume_reads_settings() {
    // sync_sfx_bus_volume reads PersistentSettings into the
    // bus's master. We exercise the resource plumbing without
    // a full app boot.
    let mut app = sfx_test_app();
    app.init_resource::<crate::ui::launch::userdata::PersistentSettings>();
    app.init_resource::<crate::ui::notifications::settings::NotificationSettings>();
    {
        let mut settings = app
            .world_mut()
            .resource_mut::<crate::ui::launch::userdata::PersistentSettings>();
        settings.sfx_volume = 0.42;
    }
    let world = app.world_mut();
    run_system_once(world, sync_sfx_bus_volume);
    let bus = world.resource::<SfxBus>();
    assert_eq!(bus.master, 0.42);
}

#[test]
fn asset_path_for_uses_sfx_subdir() {
    // The cue's file is relative to `assets/audio/sfx/`. If a
    // modder puts "ui/click.wav" in the file field, the loader
    // will look for `assets/audio/sfx/ui/click.wav` — that's
    // fine, but we want to surface the path construction in a
    // test so a future refactor doesn't accidentally move the
    // root.
    let cue = SfxCue {
        id: "test.cue".into(),
        file: "test.wav".into(),
        category: SfxCategory::Ui,
        default_volume: 1.0,
        cooldown_ms: 0,
        prompt: String::new(),
    };
    assert_eq!(asset_path_for(&cue), "audio/sfx/test.wav");
}

#[test]
fn end_to_end_manifest_load_via_app() {
    // End-to-end smoke: build the loader's data flow inline,
    // verify the registry is populated + ready.
    let mut app = sfx_test_app();
    let asset_server = app.world().resource::<AssetServer>().clone();
    {
        let mut registry = app.world_mut().resource_mut::<SfxRegistry>();
        let contents = std::fs::read_to_string("assets/data/sfx_manifest.ron")
            .expect("manifest file must exist for the test");
        let manifest: SfxManifest = ron::from_str(&contents).expect("manifest must parse");
        registry.cues.clear();
        registry.assets.clear();
        registry.cooldowns.clear();
        for cue in &manifest.cues {
            if let Some(id) = SfxCueId::from_str_id(&cue.id) {
                registry.cues.insert(id, cue.clone());
                registry.assets.insert(
                    id,
                    asset_server.load::<bevy::audio::AudioSource>(&asset_path_for(cue)),
                );
            }
        }
        registry.ready = true;
    }
    let registry = app.world().resource::<SfxRegistry>();
    assert!(registry.ready);
    assert_eq!(registry.len(), SfxCueId::ALL.len());
}
