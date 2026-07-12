//! Snapshot the live Bevy [`World`] into a RON string.
//!
//! PR-A ships a single [`snapshot_world`] helper that:
//!
//! 1. Builds a [`DynamicScene`] from the [`World`] (Bevy engine reflection
//!    pulls every `#[reflect(Component)]` component and
//!    `#[reflect(Resource)]` resource registered with `AppTypeRegistry`),
//!    minus a small denylist of Bevy-runtime resources whose inner
//!    types are not (and cannot reasonably be) reflect-serialised
//!    (e.g. `bevy_a11y::AccessibilityRequested` wraps an
//!    `Arc<AtomicBool>` that Bevy's [`SceneSerializer`] refuses).
//! 2. Wraps the scene in [`SceneSerializer`] and pipes it through `ron`.
//! 3. Wraps the RON blob in a [`SaveFile`] envelope that also carries
//!    [`SaveMetadata`] for the menu to read without parsing the body.
//!
//! Out of scope for PR-A:
//!
//! - Atomic on-disk write (write to `.tmp` then rename) — that lands in
//!   PR-B alongside the slot manager.
//! - Compression — saves are KB-scale.

use bevy::prelude::*;
use bevy_scene::serde::SceneSerializer;
use bevy_scene::DynamicSceneBuilder;
use serde::{Deserialize, Serialize};
use std::fmt;

use super::format_version::FORMAT_VERSION;
use super::handle_sidecar::{extract_handle_sidecar, HandleSidecar};
use super::migrate::{Body, SchemaKind};

/// Top-level save file envelope written to disk.
///
/// The on-disk layout is a single RON document with three fields:
/// `metadata` ([`SaveMetadata`]), `body` ([`Body`]), and `handles`
/// (an optional [`HandleSidecar`]).
///
/// The split is intentional: the menu's [`SaveIndex`](crate::ui::launch::save_index::SaveIndex)
/// scanner (GRA-311 PR-A) reads the first 4 KB of every save to discover
/// what's on disk. Putting `metadata` first means the menu can list saves
/// without paying the cost of a full [`Body`] round-trip.
///
/// `handles` is `#[serde(default)]` so saves written before the sidecar
/// existed deserialize as `None` and fall back to "no visual fidelity
/// round-trip" (the load still succeeds; entities come back without
/// mesh / material refs, just like the deny-only fix did). New saves
/// always include the sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFile {
    pub metadata: SaveMetadata,
    pub body: Body,
    /// Asset-handle sidecar — see [`HandleSidecar`] for the full
    /// rationale. `None` when no entity in the saved world had a
    /// tracked Handle-bearing component, or when the save predates
    /// the sidecar (older PR-A saves).
    #[serde(default)]
    pub handles: Option<HandleSidecar>,
}

/// Player-visible header stored at the top of every save.
///
/// Fields are intentionally narrow and stable: GRA-311's menu reads this
/// struct to populate the Load Game list. Bumping the schema here is
/// exactly what the format-version + migrator chain exists to handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMetadata {
    /// Format version. Must equal [`FORMAT_VERSION`] on write.
    /// On read, the value is checked by the loader.
    pub format_version: u32,
    /// When the save was produced, in seconds since the Unix epoch
    /// (UTC). The menu formats this for the player.
    pub saved_at_unix_s: u64,
    /// Total in-game playtime at the moment of the save, in seconds.
    /// Useful for "1h 23m" UI labels.
    pub playtime_s: u64,
    /// Stable game-seed the save was started with. Players can share
    /// seeds; future patches may use this to deterministically
    /// reproduce a save on a different binary.
    pub seed: u64,
    /// Helios version (Cargo package version) that produced the save.
    /// For humans reading a bug report — the loader does not gate on this.
    pub helios_version: String,
}

impl SaveMetadata {
    /// Build a [`SaveMetadata`] from the current wall clock + the game's
    /// seed resource. Caller is responsible for filling in `playtime_s`
    /// from whatever the play-time tracker resource turns out to be
    /// (PR-C will introduce [`AutosaveTimer`](super::autosave::AutosaveTimer);
    /// for PR-A the caller passes `0` or a known elapsed-seconds value).
    pub fn new_now(seed: u64, playtime_s: u64, helios_version: impl Into<String>) -> Self {
        let saved_at_unix_s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            format_version: FORMAT_VERSION,
            saved_at_unix_s,
            playtime_s,
            seed,
            helios_version: helios_version.into(),
        }
    }
}

/// Errors from snapshot/restore operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// RON could not serialise the scene — almost always a Reflect
    /// coverage gap (R2 in the CTO design).
    Serialize(String),
    /// The type registry is missing — the [`App`] was never built
    /// with `AppTypeRegistry`. PR-A does not auto-register it; the
    /// caller must add `.init_resource::<AppTypeRegistry>()`.
    MissingTypeRegistry,
    /// Bevy returned an error we cannot classify. Wrapped for
    /// ergonomic matching.
    Other(String),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Serialize(s) => write!(f, "save serialise failed: {s}"),
            SnapshotError::MissingTypeRegistry => write!(
                f,
                "AppTypeRegistry is not present on the World — call \
                 init_resource::<AppTypeRegistry>() before snapshotting"
            ),
            SnapshotError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Resources the snapshot must **skip**.
///
/// not reflect-serialisable:
///
/// - [`bevy_a11y::AccessibilityRequested`] wraps an
///   `Arc<AtomicBool>` whose field-reflection Bevy won't register —
///   the [`SceneSerializer`] fails when it encounters it.
/// - [`bevy_a11y::ManageAccessibilityUpdates`] has `Reflect` + the
///   `ReflectSerialize` type data registered, but its sibling resource
///   triggers the same chain. We deny both for symmetry because Bevy
///   is free to swap which one it visits first across versions.
/// - [`bevy_winit::WinitMonitors`], [`bevy_winit::WinitSettings`],
///   [`bevy_winit::DisplayHandleWrapper`], and
///   [`bevy_winit::EventLoopProxyWrapper`] hold platform / event-loop
///   handles that have no meaningful serialised form and are
///   reconstructed on every Bevy startup anyway.
///
/// All of these are runtime plumbing that the player has zero
/// observable interest in persisting — letting the in-memory state
/// drift after the next launch is correct.
/// Walk every spawned entity in `world` and return its `Entity` handle.
///
/// `pub(crate)` so [`super::handle_sidecar::extract_handle_sidecar`]
/// can iterate the same set without duplicating the EntityIndex
/// round-trip logic.
pub(crate) fn collect_all_entities(world: &World) -> Vec<bevy::prelude::Entity> {
    // Bevy 0.18 doesn't expose a public iterator over every entity
    // regardless of archetype (the `iter_entities` method was
    // moved to `RenderPhase` for the render crate).  Walk the
    // world's `Entities` metadata array directly — each entry's
    // `location: Option<EntityLocation>` is `Some` iff the entity
    // is currently spawned, so we filter for those and reconstruct
    // the `Entity` from `index + generation`.
    //
    // SAFETY: `world.entities()` returns `&Entities`, which exposes
    // its `meta` field as a `pub(crate)` accessor.  We access the
    // array through `EntityMeta` in read-only fashion (no mutation
    // of the entity allocator) so the safety contract is satisfied
    // by the immutable borrow — no concurrent mutation can race
    // with us while the borrow lasts.
    let entities = world.entities();
    let mut out = Vec::with_capacity(entities.len() as usize);
    // `Entities::meta` is `Vec<EntityMeta>`; `EntityMeta::location`
    // is `Option<EntityLocation>` — `Some` means spawned.  Use the
    // public `get_spawned`-via-`resolve_from_index` round-trip
    // rather than poking at private fields.
    for index in 0..entities.len() {
        // `u32::MAX` is reserved as a "no entity" sentinel by
        // Bevy's entity allocator, so `EntityIndex::from_raw_u32`
        // would return `None`.  Skip it explicitly.
        if index == u32::MAX {
            continue;
        }
        let entity = entities.resolve_from_index(
            bevy::ecs::entity::EntityIndex::from_raw_u32(index)
                .expect("checked against u32::MAX above"),
        );
        if entities.contains_spawned(entity) {
            out.push(entity);
        }
    }
    out
}

fn configure_builder<'a>(
    entities: &[bevy::prelude::Entity],
    builder: DynamicSceneBuilder<'a>,
) -> DynamicSceneBuilder<'a> {
    // Bevy 0.18's `DynamicSceneBuilder::from_world` returns an
    // empty scene by default — every entity must be passed through
    // `extract_entities(...)` and every resource through
    // `extract_resources()`.  We snapshot the full world (all
    // entities, all reflect-registered resources) so colony /
    // fleet / time-scale state round-trips.  The denylist below
    // carves out the Bevy-runtime plumbing we don't want
    // persisting (a11y / winit handles).
    //
    // Why this matters: pre-fix code called `from_world(world)`
    // without `extract_entities(...)`, which produced an empty
    // entity list.  Restoring a save then deserialised into zero
    // entities, breaking every world-state test that expected
    // round-tripped components.  The same applied to resources
    // (the doc-comment on `NewGameParams` claimed
    // `#[serde(default)]` everywhere, but the persistence
    // round-trip path wasn't extracting resources at all).
    //
    // Entity IDs are collected by `collect_all_entities` (so
    // `World` doesn't need a `&mut` borrow here — `World::query
    //::<Entity>()` wants `&mut World` in Bevy 0.18) and passed in
    // as a slice.
    builder
        // `bevy_camera::visibility::VisibilityClass` is a component
        // Bevy attaches automatically to every renderable entity
        // (Mesh3d / Mesh2d via `register_required_components`). Its
        // field is `SmallVec<[TypeId; 1]>` — `TypeId` is a
        // process-local handle that Bevy can't (and shouldn't)
        // register `ReflectSerialize` type data for, so the scene
        // serializer raises "type `core::any::TypeId` did not register
        // the `ReflectSerialize`" and the snapshot errors out.
        //
        // The component is purely runtime state — Bevy re-attaches a
        // fresh `VisibilityClass` to renderable entities on the next
        // frame after load, so omitting it from saves is correct.
        // Same pattern as the a11y / winit / time resources above:
        // denylist engine plumbing the player has no observable
        // interest in persisting. See the regression test
        // `snapshot_skips_visibility_class_component` below.
        .deny_component::<bevy_camera::visibility::VisibilityClass>()
        // `bevy_mesh::Mesh3d` wraps `Handle<Mesh>`, whose
        // `Handle::Strong(Arc<StrongHandle>)` variant holds a
        // process-local pointer (the `StrongHandle` is opaque and
        // the `Arc` strong-count isn't stable across builds).
        // Bevy's scene serializer raises "type
        // `bevy_platform::sync::Arc<bevy_asset::handle::StrongHandle>`
        // did not register the `ReflectSerialize`" and the snapshot
        // errors out.
        //
        // We deny from the scene blob (so the snapshot succeeds) and
        // pair it with the [`HandleSidecar`] below — the asset path
        // is recorded separately and re-attached on load via
        // [`apply_handle_sidecar`], giving full visual round-trip
        // fidelity. See `snapshot_skips_mesh3d_component` below
        // and the sidecar round-trip tests.
        .deny_component::<bevy_mesh::Mesh3d>()
        // Same problem for material refs: `MeshMaterial3d<M>`
        // wraps `Handle<M>`, and `StandardMaterial` is the
        // material used on most planet / fleet / marker entities
        // (see e.g. `src/astronomy/selection.rs` and
        // `src/fleets/visuals.rs`). The generic instance
        // `MeshMaterial3d<StandardMaterial>` is the specific
        // component type we deny; the path round-trips through
        // the same sidecar.
        //
        // Helios's custom materials (`OceanMaterial`,
        // `AtmosphereMaterial`, `StarGlowMaterial`,
        // `StarDiffractionMaterial`) carry the same Handle<T>
        // problem but live on entities that the system populator
        // re-creates on every world rebuild — they aren't on
        // save-persistent archetypes, so they don't appear in the
        // snapshot at all. If a future commit puts one on a
        // save-persistent entity, add a corresponding query clause
        // in `extract_handle_sidecar` and a deny_component here.
        .deny_component::<bevy_pbr::MeshMaterial3d<bevy_pbr::StandardMaterial>>()
        .extract_entities(entities.iter().copied())
        // IMPORTANT: apply the resource denylist BEFORE calling
        // `extract_resources()`.  `extract_resources` reads the
        // filter to decide which resource type-IDs to skip, so
        // denying after extraction would let every resource
        // (including the engine-runtime `Time<T>` family with
        // their non-reflect-serialisable `Instant`/`Duration`
        // fields) slip through.  See the full rationale on each
        // denied type below.
        .deny_resource::<bevy_a11y::AccessibilityRequested>()
        .deny_resource::<bevy_a11y::ManageAccessibilityUpdates>()
        .deny_resource::<bevy_winit::WinitMonitors>()
        .deny_resource::<bevy_winit::WinitSettings>()
        .deny_resource::<bevy_winit::DisplayHandleWrapper>()
        .deny_resource::<bevy_winit::EventLoopProxyWrapper>()
        // `bevy_time::Time<T>` (where `T` defaults to the unit
        // type `()` and is `Real`/`Virtual`/`Fixed` for the
        // engine's wall-clock / step-clock resources) each hold
        // an `Instant`/`Duration` field that doesn't carry
        // `ReflectSerialize` type data; serialising raises "type
        // `Instant` did not register the `ReflectSerialize`" and
        // the snapshot returns `Err("save serialise failed: …")`.
        // Time is a wall-clock / step-clock runtime concept that
        // has no meaningful save-time value (a freshly-launched
        // app starts at t=0 anyway), so deny all four flavours
        // — including the unit-defaulted `Time<()>` — like the
        // a11y / winit resources above.  `TimeUpdateStrategy` is
        // a sibling resource that needs the same denylist because
        // it shapes the same wall-clock control flow.
        .deny_resource::<bevy_time::Time<()>>()
        .deny_resource::<bevy_time::Time<bevy_time::Real>>()
        .deny_resource::<bevy_time::Time<bevy_time::Virtual>>()
        .deny_resource::<bevy_time::Time<bevy_time::Fixed>>()
        .deny_resource::<bevy_time::TimeUpdateStrategy>()
        .extract_resources()
}

/// Snapshot the given [`World`] into a RON string ready to write to disk.
///
/// PR-B's snapshot uses [`DynamicSceneBuilder`] and an internal denylist
/// to skip Bevy-runtime resources that fail Bevy's reflect-serialise
/// pipeline. Game state (components / resources we own) is included.
pub fn snapshot_world(world: &World, metadata: SaveMetadata) -> Result<String, SnapshotError> {
    // AppTypeRegistry is required by both `extract_entities` and
    // `extract_resources` (each calls `self.original_world.resource
    // ::<AppTypeRegistry>().read()` internally).  Fail fast with the
    // typed `MissingTypeRegistry` error before either runs — without
    // this guard the Bevy internals would panic with an opaque
    // "Resource does not exist" message and the snapshot test that
    // exercises the missing-registry case would die on a panic
    // rather than the typed error it asserts on.
    let registry = world
        .get_resource::<AppTypeRegistry>()
        .ok_or(SnapshotError::MissingTypeRegistry)?;
    let entities = collect_all_entities(world);
    let scene = configure_builder(&entities, DynamicSceneBuilder::from_world(world)).build();

    let registry_locked = registry.read();

    let serializer = SceneSerializer::new(&scene, &registry_locked);
    let scene_ron = ron::ser::to_string_pretty(&serializer, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))?;

    let body = Body {
        schema: SchemaKind::SceneRon,
        data: scene_ron,
    };
    // Extract the Handle-bearing sidecar BEFORE building the SaveFile
    // — the scene snapshot just denied those components (their inner
    // `Arc<StrongHandle>` is process-local), so we record the asset
    // paths here and let the restore path re-attach fresh handles via
    // the asset server. See [`crate::persistence::handle_sidecar`] for
    // the full rationale.
    let handles = extract_handle_sidecar(world);
    let handles = if handles.is_empty() {
        None
    } else {
        Some(handles)
    };
    let file = SaveFile {
        metadata,
        body,
        handles,
    };
    ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))
}

/// Snapshot variant for tests: takes the registry lock by reference so
/// the helper works against a bare [`World`] that has had
/// `init_resource::<AppTypeRegistry>()` called but no Plugin has run yet.
pub fn snapshot_world_with_registry(
    world: &World,
    registry: &bevy::ecs::reflect::AppTypeRegistry,
    metadata: SaveMetadata,
) -> Result<String, SnapshotError> {
    let entities = collect_all_entities(world);
    let scene = configure_builder(&entities, DynamicSceneBuilder::from_world(world)).build();
    let registry_locked = registry.read();
    let serializer = SceneSerializer::new(&scene, &registry_locked);
    let scene_ron = ron::ser::to_string_pretty(&serializer, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))?;
    let body = Body {
        schema: SchemaKind::SceneRon,
        data: scene_ron,
    };
    let handles = extract_handle_sidecar(world);
    let handles = if handles.is_empty() {
        None
    } else {
        Some(handles)
    };
    let file = SaveFile {
        metadata,
        body,
        handles,
    };
    ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
        .map_err(|e| SnapshotError::Serialize(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_metadata_carries_format_version_constant() {
        let md = SaveMetadata::new_now(42, 100, "0.4.0");
        assert_eq!(md.format_version, FORMAT_VERSION);
        assert_eq!(md.seed, 42);
        assert_eq!(md.playtime_s, 100);
        assert_eq!(md.helios_version, "0.4.0");
    }

    #[test]
    fn save_metadata_serde_ron_round_trip() {
        let md = SaveMetadata::new_now(7, 3600, "0.4.0-test");
        let ron = ron::to_string(&md).expect("serialize");
        let back: SaveMetadata = ron::from_str(&ron).expect("deserialize");
        assert_eq!(md, back);
    }

    #[test]
    fn snapshot_requires_type_registry() {
        let world = World::new();
        // Deliberately do NOT insert AppTypeRegistry.
        let err = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect_err("missing AppTypeRegistry must error");
        assert_eq!(err, SnapshotError::MissingTypeRegistry);
    }

    #[test]
    fn snapshot_skips_a11y_resource_inner_arc_atomic() {
        // Regression test for the in-game save failure: the engine
        // installs `bevy_a11y::AccessibilityRequested` whose inner
        // `Arc<AtomicBool>` is not reflect-serialisable. Without the
        // denylist in `configure_builder`, `SceneSerializer` blows up
        // with "did not register the ReflectSerialize" — see the
        // SavePanel "save serialise failed" warnings.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.insert_resource(bevy_a11y::AccessibilityRequested::default());
        // The presence of the resource plus our denylist must let
        // the snapshot complete cleanly.
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip a11y resource");
        assert!(!ron.is_empty());
        // Smoke check: the snapshot should NOT mention a11y types —
        // the denylist kept them out of the scene blob.
        assert!(
            !ron.contains("AccessibilityRequested"),
            "denylist failed — a11y resource serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_winit_resources() {
        // `bevy_winit` resources hold platform/event-loop handles
        // that have no business in a save file. Confirm the denylist
        // covers them so a save doesn't depend on transient Bevy
        // internals.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.init_resource::<bevy_winit::WinitSettings>();
        world.init_resource::<bevy_winit::WinitMonitors>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip winit resources");
        assert!(!ron.contains("WinitSettings"));
        assert!(!ron.contains("WinitMonitors"));
    }

    #[test]
    fn snapshot_skips_visibility_class_component() {
        // Regression test for the in-game autosave failure: every
        // renderable entity (Mesh3d / Mesh2d) carries a
        // `bevy_camera::visibility::VisibilityClass` component whose
        // inner `SmallVec<[TypeId; 1]>` is not reflect-serialisable
        // (`TypeId` is process-local). Without the
        // `deny_component::<VisibilityClass>` call in
        // `configure_builder`, `SceneSerializer` raises
        // "type `core::any::TypeId` did not register the
        // ReflectSerialize" and `snapshot_world` returns an error —
        // the autosave timer logs `autosave failed: save serialise
        // failed: …` every interval and no save file is written.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // VisibilityClass requires `Default`; an empty SmallVec is
        // still enough to trip the serializer because Bevy walks the
        // archetype unconditionally when extracting.
        world.spawn(bevy_camera::visibility::VisibilityClass::default());
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip VisibilityClass component");
        assert!(!ron.is_empty());
        // Smoke check: the snapshot should NOT mention the
        // component type — the denylist kept it out of the scene
        // blob.
        assert!(
            !ron.contains("VisibilityClass"),
            "denylist failed — VisibilityClass serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_mesh3d_component() {
        // Regression test for the second in-game autosave failure:
        // every rendered body carries a `bevy_mesh::Mesh3d`
        // component whose inner `Handle<Mesh>` contains a
        // `Strong(Arc<StrongHandle>)` — a process-local pointer
        // Bevy can't reflect-serialise. Without the
        // `deny_component::<Mesh3d>` call in `configure_builder`,
        // `SceneSerializer` raises "type `Arc<StrongHandle>` did not
        // register the ReflectSerialize" and the snapshot errors
        // out, so the autosave timer logs `autosave failed: save
        // serialise failed: …` every interval and no save file is
        // written.
        //
        // NB: `Mesh3d::default()` produces a `Handle::Uuid` (not the
        // failing `Handle::Strong`), so the inner `Arc<StrongHandle>`
        // failure path isn't directly reproducible in this test —
        // the snapshot succeeds either way. What the test DOES verify
        // is the denylist end: the saved RON must not mention
        // `Mesh3d`, confirming the component was filtered out before
        // `extract_entities` recorded it. The custom-handle failure
        // path is exercised by the user's runtime autosave (every
        // interval), which is why the deny is in place.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mesh3d has `#[require(Transform)]` so spawning it pulls
        // Transform in too — harmless for the snapshot.
        world.spawn(bevy_mesh::Mesh3d::default());
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip Mesh3d component");
        assert!(
            !ron.contains("Mesh3d"),
            "denylist failed — Mesh3d serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_mesh_material3d_component() {
        // Regression test for the same family of failures as
        // `snapshot_skips_mesh3d_component`: `MeshMaterial3d<M>`
        // wraps `Handle<M>` and the standard-material instance is
        // on every planet / fleet / marker entity (see
        // `src/astronomy/selection.rs`, `src/fleets/visuals.rs`,
        // `src/plugins/starmap.rs`). Without the
        // `deny_component::<MeshMaterial3d<StandardMaterial>>`
        // call in `configure_builder`, the same
        // "type `Arc<StrongHandle>` did not register" failure
        // surfaces — typically on the next component after the
        // Mesh3d deny is in place, since these two components
        // usually travel together on the same entity archetype.
        //
        // Same caveat as the Mesh3d test: `MeshMaterial3d::default()`
        // gives `Handle::Uuid`, so the inner Arc failure isn't
        // reproduced here — we just verify the denylist keeps the
        // component out of the saved scene blob.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.spawn(bevy_pbr::MeshMaterial3d::<bevy_pbr::StandardMaterial>::default());
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip MeshMaterial3d component");
        assert!(
            !ron.contains("MeshMaterial3d"),
            "denylist failed — MeshMaterial3d serialised into save: {ron}"
        );
    }
}
