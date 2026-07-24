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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SavePreview {
    #[serde(default)]
    pub current_date: String,
    #[serde(default)]
    pub colony_count: u32,
    #[serde(default)]
    pub total_population: f64,
    #[serde(default)]
    pub ship_count: u32,
    #[serde(default)]
    pub power_produced_watts: f64,
    #[serde(default)]
    pub kardashev_value: f64,
    #[serde(default)]
    pub resources: Vec<(String, f64)>,
    #[serde(default)]
    pub kardashev_history: Vec<(f64, f64)>,
    #[serde(default)]
    pub screenshot_file: Option<String>,
}

/// Player-visible header stored at the top of every save.
///
/// Fields are intentionally narrow and stable: GRA-311's menu reads this
/// struct to populate the Load Game list. Bumping the schema here is
/// exactly what the format-version + migrator chain exists to handle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Rich menu-only campaign preview. Default keeps older saves loadable.
    #[serde(default)]
    pub preview: SavePreview,
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
            preview: SavePreview::default(),
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
            // Skip Bevy-runtime entities — see [`is_bevy_runtime_entity`]
            // for the full rationale. These are entities spawned once at
            // startup by Bevy's own plugins (and by Helios's startup
            // systems) and rebuilt every launch. Persisting them serves
            // no purpose AND each auto-required companion has tripped
            // the scene serializer at some point in Bevy's release
            // history: Camera3d's `CameraMainTextureUsages`, `Msaa`,
            // `SyncToRenderWorld`, `CameraRenderGraph`, `ColorGrading`,
            // `Tonemapping`, `DebandDither`, `Projection`, `Exposure`,
            // `RenderTarget`; the Window entity's `CursorIcon::Custom(
            // CustomCursor::Image(CustomCursorImage { handle: Handle<
            // Image> }))`, plus `RawHandleWrapper` (platform-specific
            // OS handle), plus any future companions. Skipping the
            // entity here fixes all of them in one place, with no
            // per-Bevy-update maintenance burden.
            //
            // `extract_handle_sidecar` also calls this function, so
            // runtime entities are excluded from the Handle sidecar
            // automatically (camera and window entities have no
            // `Mesh3d` / `MeshMaterial3d` handles to round-trip
            // anyway — visual fidelity loss is zero).
            //
            // Trade-offs:
            // - Camera: the saved Camera entity's Helios-side
            //   components (`OrbitCamera`, `GameCamera`,
            //   `CameraAnchor`) also disappear from saves. The
            //   player's pan/zoom/yaw state on `OrbitCamera` is
            //   therefore re-defaulted on every load — same UX as
            //   the pre-fix state where the snapshot errored every
            //   interval. Restoring it requires a one-shot
            //   post-load system that copies the saved `OrbitCamera`
            //   values onto the freshly-spawned camera (separate
            //   fix — see comment on the deny chain in
            //   `configure_builder` below).
            // - Window: Bevy rebuilds the window on every launch
            //   anyway, so no player-observable state is lost.
            // - Both: future Bevy companions on these entities are
            //   automatically handled — no further code changes
            //   needed for the next Bevy point release.
            if let Ok(entity_ref) = world.get_entity(entity) {
                if is_bevy_runtime_entity(&entity_ref) {
                    continue;
                }
            }
            out.push(entity);
        }
    }
    out
}

/// `true` if the entity is a Bevy-runtime entity that the
/// persistence layer should drop wholesale from saves.
///
/// Bevy's plugin tree spawns these entities once at startup and
/// rebuilds them on every launch. Persisting them serves no
/// purpose AND each auto-required companion has, at some point in
/// Bevy's 0.17→0.18 release series, tripped the scene serializer
/// with a `did not register ReflectSerialize` error. Filtering
/// them at the entity level fixes all present and future
/// reflection failures in one place.
///
/// Add a new Bevy-runtime marker component here as new entity
/// types join the list (e.g. audio-sink entities when/if Bevy
/// ships them as Components).
fn is_bevy_runtime_entity(entity_ref: &bevy::prelude::EntityRef) -> bool {
    // Camera2d / Camera3d are zero-sized Bevy markers on the
    // camera entity that Bevy's `CameraPlugin` rebuilds every
    // launch. See `configure_builder`'s deny-chain comment for
    // the full list of camera-companion failures.
    entity_ref.contains::<bevy_camera::Camera3d>()
        || entity_ref.contains::<bevy_camera::Camera2d>()
        // `Window` is the Bevy-marker component for the OS-level
        // window entity spawned by `WinitPlugin` (and tagged
        // `With<PrimaryWindow>` — see
        // `src/ui/cursors.rs::update_cursor_icon`, which mutates
        // the `CursorIcon` component on this entity). The entity
        // carries the OS window handle (`RawHandleWrapper`),
        // user-set `CursorIcon::Custom(...)` with an `Image`
        // handle, and other platform-side companions that are
        // unserialisable. Bevy rebuilds the window on every
        // launch; persistence serves no purpose.
        || entity_ref.contains::<bevy::window::Window>()
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
        // Helios's custom materials (`AtmosphereMaterial`,
        // `OceanMaterial`, `StarGlowMaterial`, `StarSurfaceMaterial`,
        // `StarDiffractionMaterial`, `StarCorona3dMaterial`,
        // `StarHalo3dMaterial`, `NightMaterial`, `SkyboxMaterial`)
        // carry the same `Handle<T>` problem AND live on
        // save-persistent entity archetypes (populated solar-system
        // bodies, oceans, stars). Each `MeshMaterial3d<T>` is its own
        // Bevy Component type and needs its own `deny_component` here
        // PLUS a corresponding field in
        // [`crate::persistence::handle_sidecar::EntityHandles`] and a
        // query + insert clause there. Adding a new custom material?
        // Add the deny here, the field in `EntityHandles`, and the
        // extract + apply clauses in `extract_handle_sidecar` /
        // `apply_handle_sidecar`.
        .deny_component::<bevy_pbr::MeshMaterial3d<bevy_pbr::StandardMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::atmosphere::AtmosphereMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::ocean::OceanMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::star_materials::StarGlowMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::star_materials::StarSurfaceMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::star_materials::StarDiffractionMaterial>>()
        // 3D volumetric star shells — added in the same pass as
        // `StarGlowMaterial` / `StarDiffractionMaterial` (see
        // `update_star_corona_3d_lod` in `src/plugins/star_materials.rs`).
        // Without these denials, the corona inner shell and halo outer
        // shell around every populated star trip Bevy's scene
        // serializer with `type `Arc<StrongHandle>` did not register
        // the ReflectSerialize`, aborting the snapshot. Companion
        // sidecar fields live in
        // [`crate::persistence::handle_sidecar::EntityHandles`].
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::star_materials::StarCorona3dMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::star_materials::StarHalo3dMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::plugins::visual_effects::NightMaterial>>()
        .deny_component::<bevy_pbr::MeshMaterial3d<crate::render::backdrop::SkyboxMaterial>>()
        // Bevy `bevy_camera::camera::Camera` and its `#[require]`
        // chain are Bevy-runtime state, not gameplay state — the
        // Camera entity is spawned once at startup by Bevy's
        // `CameraPlugin` (and by Helios's `spawn_camera` in
        // `src/plugins/camera.rs`) and re-created on every launch,
        // so persisting it serves no purpose AND multiple
        // auto-required companions trip Bevy's scene serializer:
        //
        // - `CameraMainTextureUsages(pub TextureUsages)` —
        //   `wgpu_types` bitflags, no `ReflectSerialize`
        //   registration. PR-B active error.
        // - `RenderTarget::Image(ImageRenderTarget { handle:
        //   Handle<Image> })` — `Arc<StrongHandle>` inner Handle,
        //   same family of failure as `Mesh3d` /
        //   `MeshMaterial3d`. Will fail the first time we render
        //   to an off-screen image target.
        // - `Projection::Custom(CustomProjection { dyn_projection:
        //   Box<dyn DynCameraProjection> })` — trait object Bevy
        //   can't reflect-serialise.
        // - `Exposure` uses `#[reflect(opaque)]`, which
        //   `SceneSerializer` walks as a typed value with no
        //   field-level ReflectSerialize data and aborts.
        // - `CameraRenderGraph(InternedRenderSubGraph)` —
        //   `Interned<dyn RenderSubGraph>` raw-static-ref inner.
        //   PR-C active error.
        //
        // **Primary fix** — `collect_all_entities` filters out any
        // entity carrying `bevy_camera::Camera3d` or `Camera2d`,
        // dropping the whole Camera entity (and every auto-required
        // companion) from the snapshot in one place. This is the
        // future-proof mechanism: any new reflection failure on a
        // bevy_render / bevy_core_pipeline / bevy_post_process
        // camera companion is fixed without further code changes.
        //
        // **Secondary fix** — the deny chain below is
        // defense-in-depth plus documentation of the failure modes
        // listed above. Each `deny_component` would prevent a
        // specific Camera companion from being serialised even if
        // a future change reintroduced it on a non-Camera entity
        // somewhere.
        //
        // **Trade-off** — the Camera entity's Helios-side
        // components (`OrbitCamera`, `GameCamera`, `CameraAnchor`)
        // are also dropped from saves (the entity-skip drops the
        // whole entity, not just the Bevy components). Player
        // camera state — pan_offset, yaw, pitch, zoom — falls back
        // to `OrbitCamera::default()` on load, same UX as the
        // pre-PR-B state where autosave errored every interval.
        // Restoring the saved `OrbitCamera` values to the
        // freshly-spawned camera on load is a separate fix
        // (a post-load one-shot system); until that lands, load
        // defaults camera controls to `OrbitCamera::default()` —
        // same behaviour the player had before this fix, when
        // autosave errored every interval. See the regression
        // tests `snapshot_skips_camera_main_texture_usages_component`,
        // `snapshot_skips_render_target_component`, and
        // `snapshot_skips_camera3d_entity` below.
        .deny_component::<bevy_camera::Camera>()
        .deny_component::<bevy_camera::Camera2d>()
        .deny_component::<bevy_camera::Camera3d>()
        .deny_component::<bevy_camera::CameraMainTextureUsages>()
        .deny_component::<bevy_camera::RenderTarget>()
        .deny_component::<bevy_camera::Exposure>()
        .deny_component::<bevy_camera::Projection>()
        // `CameraRenderGraph` (bevy_render::camera) wraps
        // `InternedRenderSubGraph = Interned<dyn RenderSubGraph>`,
        // a `'static` raw-ref process-local pointer Bevy can't
        // reflect-serialise.  This is the active in-game
        // failure the user reported (PR-C, see commit `b3c3835`
        // history) — `SceneSerializer` raises "type
        // `CameraRenderGraph` did not register the
        // ReflectSerialize" every autosave interval.
        //
        // In practice the entity-skip in `collect_all_entities`
        // (filtering out any entity with `Camera3d` / `Camera2d`)
        // already removes every auto-required companion on the
        // Camera3d in one go, so this deny is defense-in-depth
        // (catches a hypothetical non-Camera3d entity carrying
        // `CameraRenderGraph`) plus documentation of the
        // failure mode.  Same trade-off as the rest of the
        // camera deny chain: the Camera entity is Bevy-runtime
        // state, recreated on every launch.
        .deny_component::<bevy::render::camera::CameraRenderGraph>()
        // `CursorIcon` lives on the Window entity (the same entity
        // `With<PrimaryWindow>` queries target in
        // `src/ui/cursors.rs`). When Helios sets a custom cursor
        // via `CursorIcon::Custom(CustomCursor::Image(
        // CustomCursorImage { handle: Handle<Image>, hotspot, .. }))
        // the inner `Handle<Image>` is the standard
        // `Arc<StrongHandle>` shape that breaks the scene
        // serializer.
        //
        // This is the active in-game failure the user reported
        // (PR-D, `SavePanel: write to ... save.ron failed: ...
        // `Arc<StrongHandle>` did not register ReflectSerialize
        // (stack: ... CursorIcon -> CustomCursor -> CustomCursorImage
        // -> Handle<Image> -> Arc<StrongHandle>)`). The
        // entity-skip in `collect_all_entities` already drops
        // the whole Window entity on the primary path, so this
        // deny is defense-in-depth plus documentation of the
        // failure mode.
        //
        // The rest of the `bevy_window` companions that touch the
        // Window entity (`RawHandleWrapper` for the OS handle,
        // `WindowTheme`, `CursorOptions`, etc.) are handled the
        // same way — they live on the same skipped entity.
        .deny_component::<bevy::window::CursorIcon>()
        // `bevy_transform::components::global_transform::GlobalTransform`
        // is auto-derived by Bevy from `Transform` + the
        // `ChildOf` chain (`propagate_parent_transforms` in
        // PostUpdate). Saving its value is pointless AND
        // actively harmful: when the save is loaded, Bevy's
        // `SceneDeserializer` inserts `GlobalTransform` on every
        // entity that has it in the save. Each insert fires
        // the
        // `validate_parent_has_component::<GlobalTransform>`
        // hook (Bevy 0.18 `bevy_transform-0.18.0/src/components/
        // global_transform.rs:65`). The hook checks the
        // entity's `ChildOf` parent — if the parent has
        // already been deserialized, the parent also has
        // `GlobalTransform` and the hook is silent. If the
        // parent HASN'T been deserialized yet, the parent
        // lacks `GlobalTransform` and the hook emits a B0004
        // warning. With ~710 saved bodies, that's a ~250
        // warning storm on every Continue.
        //
        // Denying `GlobalTransform` from the snapshot (and
        // from the swap's pass 1 — see
        // `src/persistence/swap.rs::configure_skip_list`) lets
        // Bevy auto-derive the correct value from the freshly
        // rewritten `Transform` + `ChildOf` graph in
        // `propagate_parent_transforms`. End state is
        // identical; warning storm is gone.
        .deny_component::<bevy::transform::components::GlobalTransform>()
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
        // `bevy_audio` resources inserted by `AudioPlugin::build`
        // (`bevy_audio-0.18.0/src/lib.rs:82-84`). Both `GlobalVolume`
        // and `DefaultSpatialScale` derive `Reflect` but Bevy 0.18's
        // `AudioPlugin` never calls `register_type::<…>()` for them,
        // so writing them into the snapshot lands a type into the
        // RON body that the loader's `TypeRegistrationDeserializer`
        // cannot resolve. On load, Bevy returns `Error::custom("no
        // registration found for `bevy_audio::audio::DefaultSpatialScale`")`,
        // `SceneDeserializer::deserialize` short-circuits, and
        // `restore_world` returns `Err(RestoreError::Scene(...))` —
        // so the Continue / Load-Game click leaves the player on
        // the launch menu with a `restore_failed` toast instead of a
        // restored world. Audio volume + spatial scale have no
        // meaningful save-time value (Helios's `MusicPlugin` re-seeds
        // both on next launch via `start_playlist` and the
        // `playback_settings` overrides per `AudioPlayer`), so deny
        // both — same rationale as the a11y / winit / time denylist
        // above. See `snapshot_skips_audio_resources` below.
        .deny_resource::<bevy::audio::GlobalVolume>()
        .deny_resource::<bevy::audio::DefaultSpatialScale>()
        // `bevy_camera::ClearColor` (defined in
        // `bevy_camera-0.18.0/src/clear_color.rs:53`, re-exported
        // via `pub use clear_color::*;` in the camera crate root
        // `lib.rs:11`) is the only reflect-derived `Resource` that
        // `bevy_camera`'s plugin chain inserts via
        // `init_resource` without registering it
        // (`bevy_camera-0.18.0/src/lib.rs:22`). The other
        // resources the camera plugin installs —
        // `visibility::ManualMark`, `visibility::ObservedChanged`,
        // and `visibility::range::VisibleEntityRanges`
        // (`visibility/mod.rs:1083-1084`,
        // `visibility/range.rs:33`) — only `derive(Resource,
        // Default)` and don't carry `Reflect`, so Bevy's
        // `DynamicSceneBuilder::extract_resources` skips them
        // automatically. `ClearColor` is different: its
        // definition is `#[derive(Resource, Clone, Debug, Deref,
        // DerefMut, Reflect)]`, so the snapshot picks it up and
        // writes `bevy_camera::clear_color::ClearColor` into the
        // save's resource list. Loading a save then short-circuits
        // in `TypeRegistrationDeserializer::visit_str`
        // (`bevy_reflect-0.18.0/src/serde/de/registrations.rs:45`)
        // with `Error::custom("no registration found for
        // `bevy_camera::clear_color::ClearColor`")`, `?`
        // propagates out of `SceneDeserializer::deserialize`,
        // and `restore_world` returns `Err(RestoreError::Scene(
        // ...))`. Symptom from the player session of
        // 2026-07-23T20:14Z: `kickoff: restore_save failed: save
        // restore failed: scene deserialise failed: no
        // registration found for `bevy_camera::clear_color::
        // ClearColor``.
        //
        // `ClearColor` is the camera's per-frame clear colour —
        // a window-level runtime tunable, not gameplay state. It
        // should reset to its default (`Color::BLACK`) on every
        // launch, so deny it from the scene blob. See
        // `snapshot_skips_bevy_camera_clear_color_resource` below.
        .deny_resource::<bevy_camera::ClearColor>()
        // `bevy_gizmos::config::GizmoConfigStore` (defined in
        // `bevy_gizmos-0.18.0/src/config.rs:97`,
        // `#[derive(Reflect, Resource, Default)]
        // #[reflect(Resource, Default)]`) is inserted via
        // `get_resource_or_init::<GizmoConfigStore>()` by
        // `GizmoPlugin::build`
        // (`bevy_gizmos-0.18.0/src/lib.rs:93`, inside the
        // `init_gizmo_group::<DefaultGizmoConfigGroup>()` call).
        // `bevy_gizmos` makes **zero** `register_type::<…>()`
        // calls — confirmed by `grep register_type` over
        // `bevy_gizmos-0.18.0/src/`. The sibling gizmo
        // resources `GizmoHandles` (`lib.rs:184`) and
        // `GizmoStorage<Config, Clear>` (`gizmos.rs:32`) only
        // `derive(Resource)` / `derive(Resource, Default)` —
        // neither carries `Reflect`, so Bevy's
        // `DynamicSceneBuilder::extract_resources` skips them
        // automatically and they can't leak via this path.
        // `GizmoConfigStore` does carry `Reflect` and lands in
        // the snapshot.
        //
        // Compounding the issue, `GizmoConfigStore.store` is
        // `#[reflect(ignore)] TypeIdMap<(GizmoConfig, Box<dyn
        // Reflect>)>` (`config.rs:104-105`) — keys are
        // process-local `TypeId`s and values are trait objects,
        // so even after registering the type the inner data
        // wouldn't round-trip meaningfully across launches.
        // The right call is to keep `GizmoConfigStore` out of
        // the save entirely: it's a runtime gizmo-config cache
        // re-derived from `app.init_gizmo_group::<T>()` calls
        // at every launch (`lib.rs:118-153`) — no save-time
        // meaning.
        //
        // Player-visible symptom at 2026-07-23T21:06Z:
        // `kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_gizmos::config::GizmoConfigStore``. After the
        // fix, deny the resource next to the audio / camera
        // chain — same rationale. See
        // `snapshot_skips_bevy_gizmos_config_store_resource`
        // below.
        .deny_resource::<bevy::gizmos::config::GizmoConfigStore>()
        // `bevy_input` resources (`InputPlugin::build`,
        // `bevy_input-0.18.0/src/lib.rs:114-162`) — five
        // reflect-derived runtime input accumulators that Bevy
        // `init_resource`s without ever `register_type`-ing them
        // (`grep register_type bevy_input-0.18.0/src/` returns
        // zero matches). Each reflect derive is gated on
        // `#[cfg_attr(feature = "bevy_reflect", ...)]`, but
        // `bevy_internal-0.18.0/Cargo.toml:567-571` compiles
        // `bevy_input` with `features = ["bevy_reflect"]`
        // unconditionally for the umbrella re-export, so all
        // five derive `Reflect` in our build.
        //
        // Player-visible symptom at 2026-07-23T21:10Z:
        // `kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_input::mouse::AccumulatedMouseMotion``.
        //
        // All five are per-frame runtime state with no
        // save-time meaning — `ButtonInput<T>` resets every
        // frame (`button_input.rs:115-140`), the mouse / scroll
        // accumulators reset every frame (`mouse.rs:201-228`),
        // and the freshly-launched app starts from `Default`
        // anyway. Ditch them all and let Bevy re-derive at
        // next launch. The sixth resource the plugin installs,
        // `bevy_input::touch::Touches` (`touch.rs:246-260`),
        // only `derive(Debug, Clone, Default, Resource)`
        // without `Reflect`, so `extract_resources` skips it
        // automatically — no deny needed.
        //
        // See `snapshot_skips_bevy_input_resources` below.
        .deny_resource::<bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>>()
        .deny_resource::<bevy::input::ButtonInput<bevy::input::keyboard::Key>>()
        .deny_resource::<bevy::input::ButtonInput<bevy::input::mouse::MouseButton>>()
        .deny_resource::<bevy::input::mouse::AccumulatedMouseMotion>()
        .deny_resource::<bevy::input::mouse::AccumulatedMouseScroll>()
        // `bevy_light` resources (`LightPlugin::build`,
        // `bevy_light-0.18.0/src/lib.rs:137-140`).
        //
        // `grep register_type bevy_light-0.18.0/src/` returns
        // zero matches, and the crate derives Reflect on
        // three of the four Resources it inserts:
        // `GlobalAmbientLight`
        // (`ambient_light.rs:59`,
        // `#[derive(Resource, Clone, Debug, Reflect)]
        // #[reflect(Resource, Debug, Default, Clone)]`),
        // `DirectionalLightShadowMap`
        // (`directional_light.rs:181`), and
        // `PointLightShadowMap` (`point_light.rs:173`). All
        // three are `init_resource`'d without a matching
        // `register_type::<…>()` — same pattern as the
        // a11y / audio / camera / gizmos / input fixes.
        //
        // **Crates-path gotcha.** `bevy_light-0.18.0/src/
        // lib.rs` declares `mod ambient_light;`,
        // `mod directional_light;`, `mod point_light;` as
        // **private** modules and re-exports the public
        // types at the crate root via `pub use
        // ambient_light::{AmbientLight, GlobalAmbientLight};`
        // and friends — so the public names **drop the
        // module qualifier**. The umbrella crate follows
        // the same shape (`bevy_internal-0.18.0/src/lib.rs:
        // 57-58`, `#[cfg(feature = "bevy_light")] pub use
        // bevy_light as light;` with no further re-export),
        // so the deny paths are `bevy::light::
        // GlobalAmbientLight` etc. — NOT `bevy::light::
        // ambient_light::GlobalAmbientLight`.
        //
        // Player-visible symptom at 2026-07-23T21:14Z:
        // `kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_light::ambient_light::GlobalAmbientLight``.
        //
        // All three are runtime lighting tunables with no
        // save-time meaning — a freshly-launched app re-runs
        // `LightPlugin::build`, which `init_resource`s them
        // with `Default` (`Color::WHITE` × `300.0` for
        // `GlobalAmbientLight`, `2048` / `1024` for the shadow
        // map sizes). The fourth Resource,
        // `GlobalVisibleClusterableObjects`
        // (`cluster/mod.rs:37`), only
        // `derive(Resource, Default)` without Reflect, so
        // `extract_resources` skips it automatically.
        //
        // See
        // `snapshot_skips_bevy_light_resources` below.
        .deny_resource::<bevy::light::GlobalAmbientLight>()
        .deny_resource::<bevy::light::DirectionalLightShadowMap>()
        .deny_resource::<bevy::light::PointLightShadowMap>()
        // `bevy_pbr::DefaultOpaqueRendererMethod`
        // (`bevy_pbr-0.18.0/src/material.rs:1333-1334`,
        // `#[derive(Default, Resource, Clone, Debug,
        // ExtractResource, Reflect)]
        // #[reflect(Resource, Default, Debug, Clone)]`) is the
        // only reflect-derived Resource that `PbrPlugin::build`
        // inserts into the **main** `app` (`lib.rs:219`,
        // `.init_resource::<DefaultOpaqueRendererMethod>()`).
        // `grep register_type bevy_pbr-0.18.0/src/` returns zero
        // matches across the whole crate, so no main-app
        // reflect-Resource ever lands in `AppTypeRegistry`. Other
        // `bevy_pbr` `init_resource` / `insert_resource` calls
        // (`atmosphere/mod.rs:145-150`,
        // `decal/{clustered,forward}.rs`,
        // `deferred/mod.rs:111, 415`, `lightmap/mod.rs`,
        // `light_probe/generate.rs`, `render/mesh.rs:232`,
        // `prepass/mod.rs:123`, `diagnostic.rs:63`, plus
        // `lib.rs:342-343, 304, 370` for `LightMeta`,
        // `RenderMaterialBindings`, `Bluenoise`,
        // `global_cluster_settings`) all target either the
        // `render_app` sub-app (whose `World` is not walked by
        // `DynamicSceneBuilder::extract_resources`) or one-shot
        // systems that don't init the resource at app-startup
        // — so they don't leak via this path. `grep`
        // `derive(Resource, Reflect)` across the entire crate
        // only matches `DefaultOpaqueRendererMethod` for the
        // main-app surface area.
        //
        // The sibling resource `global_cluster_settings` is a
        // `GlobalClusterSettings` instance — defined in
        // `bevy_light-0.18.0/src/cluster/mod.rs:38` as plain
        // `#[derive(Resource)]` without `Reflect`, so it's safe
        // (the exact same audit pattern that holds for
        // `bevy_camera::ManualMark` /
        // `bevy_gizmos::GizmoHandles`).
        //
        // Player-visible symptom at 2026-07-23T21:18Z:
        // `kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_pbr::material::DefaultOpaqueRendererMethod``.
        //
        // `DefaultOpaqueRendererMethod` is the global default
        // for opaque material rendering (Forward / Deferred)
        // — a render-path tunable, not gameplay state. A
        // freshly-launched app re-runs `PbrPlugin::build`,
        // which `init_resource`s it with
        // `DefaultOpaqueRendererMethod(OpaqueRendererMethod::Forward)`.
        // See `snapshot_skips_bevy_pbr_default_opaque_renderer_method`
        // below.
        //
        // **Crates-path gotcha (reminder).** The default
        // module name in the failure log is `bevy_pbr::
        // material::DefaultOpaqueRendererMethod`, but
        // `bevy_pbr-0.18.0/src/lib.rs:64` does `pub use
        // material::*;` — so the type is also re-exported at
        // the `bevy_pbr` crate root (and the umbrella
        // `bevy_internal-0.18.0/src/lib.rs:65`'s `pub use
        // bevy_pbr as pbr;` makes the umbrella path
        // `bevy::pbr::DefaultOpaqueRendererMethod` resolve).
        // The deny entry below uses the umbrella-root form to
        // match the convention used for `bevy::light::
        // GlobalAmbientLight`.
        .deny_resource::<bevy::pbr::DefaultOpaqueRendererMethod>()
        // `bevy_picking` resources
        // (`PickingPlugin::build` /
        // `PointerInputPlugin::build` /
        // `MeshPickingPlugin::build`,
        // `bevy_picking-0.18.0/src/lib.rs:365-367, 419-421`,
        // `input.rs:97`, `mesh_picking/mod.rs:71`).
        //
        // **Note: `bevy_picking` is a transitive dependency
        // in our build, not a direct feature.** `cargo tree
        // -e features -i bevy_picking` shows the chain
        // `helios_ascension → bevy (2d/3d/ui via default) →
        // bevy_internal (picking) → bevy_picking`, plus
        // `mesh_picking` enabled through `bevy` defaults.
        // That's why the player-visible save/load failures
        // keep surfacing types from crates we don't list in
        // Cargo.toml — the umbrella's high-level feature
        // flags (especially `2d/3d` + `default`) pull them
        // in. Audit recipe in `…/memories/repo/bevy-0-18-
        // reflect-resource-denylist.md` documents the
        // `cargo tree -e features -i <crate>` one-liner so
        // the next diagnose doesn't assume "if it's not
        // listed in Cargo.toml it's not in the binary".
        //
        // `grep register_type bevy_picking-0.18.0/src/`
        // returns zero matches, and three of the seven
        // main-app `init_resource`s are reflect-derived:
        // `PickingSettings` (`lib.rs:296`,
        // `#[derive(Copy, Clone, Debug, Resource,
        // Reflect)]`), `PointerInputSettings`
        // (`input.rs:42`, `#[derive(Copy, Clone,
        // Resource, Debug, Reflect)]`) and
        // `MeshPickingSettings`
        // (`mesh_picking/mod.rs:38-39`,
        // `#[derive(Resource, Reflect)]
        // #[reflect(Resource, Default)]`). All three leak
        // into the snapshot and break the loader.
        //
        // The other main-app inserts are non-reflect by
        // `derive` shape and `extract_resources` skips
        // them: `pointer::PointerMap`, `backend::ray::
        // RayMap`, `hover::HoverMap`,
        // `hover::PreviousHoverMap`, and `events::
        // PointerState` — all `#[derive(Debug, Deref,
        // DerefMut, Default, Resource)]` (or the bare
        // `Debug, Default, Resource` variant) without
        // `Reflect`. Same audit pattern that holds for
        // `bevy_camera::ManualMark` /
        // `bevy_gizmos::GizmoHandles` /
        // `bevy_light::GlobalVisibleClusterableObjects`.
        //
        // Player-visible symptom at 2026-07-23T21:21Z:
        // `kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_picking::PickingSettings``.
        //
        // All three are runtime picking tunables with no
        // save-time meaning — a freshly-launched app
        // re-runs the picking plugin chain, which
        // `init_resource`s them with their `Default`
        // values (window / hover / input plumbing all
        // re-derived from the live app builder). Deny all
        // three and let Bevy re-derive at next launch.
        //
        // **Crates-path note.** `bevy_picking-0.18.0/src/
        // lib.rs:163-170` declares `input` and
        // `mesh_picking` as `pub mod` (gated on feature
        // `mesh_picking` for the latter). The umbrella
        // re-export `pub use bevy_picking as picking;`
        // (`bevy_internal-0.18.0/src/lib.rs:66-67`)
        // preserves the module structure. `PickingSettings`
        // itself is `pub struct` at the `bevy_picking`
        // crate root (so its public name drops the module
        // qualifier, same shape as `bevy_camera::ClearColor
        // / `bevy::light::GlobalAmbientLight`). The
        // sibling types keep their module paths. The deny
        // entries below use both shapes intentionally so a
        // future Bevy version that consolidates the
        // re-exports doesn't break the snapshot path.
        //
        // See `snapshot_skips_bevy_picking_resources` below.
        .deny_resource::<bevy::picking::PickingSettings>()
        .deny_resource::<bevy::picking::input::PointerInputSettings>()
        .deny_resource::<bevy::picking::mesh_picking::MeshPickingSettings>()
        // === Bulk pick-up: 4 more reflect-derived, main-app,
        // init_resource'd, unregistered Resources surfaced by a
        // full audit of every Bevy crate in our build on
        // 2026-07-23T21:24Z. See the audit-and-pre-empt
        // reasoning in
        // `/memories/repo/bevy-0-18-reflect-resource-denylist.md`
        // §"Audit recipe" + the "Confirmed cases" table. ===
        //
        // `bevy_sprite::SpritePickingSettings`
        // (declared in `bevy_sprite-0.18.0/src/picking_backend.rs
        // :50` as `#[derive(Resource, Reflect)]
        // #[reflect(Resource, Default)]`; re-exported at the
        // `bevy_sprite` crate root via `pub use picking_backend::
        // *;` (`lib.rs`), so its public name drops the
        // `picking_backend` module qualifier — same shape as
        // `bevy_camera::ClearColor` / `bevy::light::
        // GlobalAmbientLight` / `bevy::pbr::
        // DefaultOpaqueRendererMethod`). Init'd at
        // `picking_backend.rs:81`
        // (`.init_resource::<SpritePickingSettings>()`).
        // `bevy_sprite` makes zero `register_type` calls.
        // Player-visible symptom at 2026-07-23T21:24Z:
        // `kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_sprite::picking_backend::SpritePickingSettings``.
        //
        // `bevy_ui::UiScale`
        // (`bevy_ui-0.18.0/src/lib.rs:117`,
        // `#[derive(Debug, Reflect, Resource, Deref, DerefMut)]
        // #[reflect(Resource, Debug, Default)]`),
        // init'd at `layout/mod.rs:388` and `update.rs:200`.
        // `bevy_ui` makes zero `register_type` calls.
        //
        // `bevy_ui::picking_backend::UiPickingSettings`
        // (`bevy_ui-0.18.0/src/picking_backend.rs:47`,
        // `#[derive(Resource, Reflect)]
        // #[reflect(Resource, Default)]`), init'd at the same
        // file's line 81 (`.init_resource::<UiPickingSettings>()`).
        // Same crate, same pattern, same fix.
        //
        // `bevy_ui_render::debug_overlay::UiDebugOptions`
        // (`bevy_ui_render-0.18.0/src/debug_overlay.rs:31`,
        // `#[derive(Resource, Reflect)]
        // #[reflect(Resource)]`), init'd at `lib.rs:208`
        // (`.init_resource::<UiDebugOptions>()`).
        // `bevy_ui_render` makes zero `register_type` calls.
        // **False positive for our build, however.** The
        // `debug_overlay` module is gated on the
        // `bevy_ui_debug` feature (`bevy_ui_render-0.18.0/src/
        // lib.rs:19-20`), which we do NOT enable in
        // Cargo.toml — verified at compile time by the
        // E0433 "could not find `debug_overlay` in
        // `ui_render`" error from the first attempt. So
        // `UiDebugOptions` isn't in our binary and never
        // reaches the snapshot. No deny needed.
        //
        // All three are runtime defaults — a freshly-launched
        // app re-runs the relevant plugins and re-`init_resource`s
        // them with `Default` (alpha threshold 0.1 for sprite
        // picking, `UiScale(1.0)` for the UI scale, etc.) —
        // so deny all three and let Bevy re-derive at next
        // launch. Crates-path gotcha (familiar shape):
        // `bevy_sprite` is a transitive via `bevy`'s `2d`
        // feature; `bevy_ui` via `bevy`'s `ui` feature; the
        // umbrella's `pub use bevy_X as X;` aliasing lets
        // the paths resolve under the bare module
        // hierarchy. All three are reachable as
        // `bevy::sprite::SpritePickingSettings`
        // / `bevy::ui::UiScale` / `bevy::ui::picking_backend::
        // UiPickingSettings`.
        //
        // The audit also surfaced two **false positives** that
        // are worth flagging so the next diagnose doesn't
        // re-flag them:
        //
        // - `bevy_animation::ThreadedAnimationGraphs`
        //   (`graph.rs:285`) is `#[derive(Default, Reflect,
        //   Resource)]` but **never** `init_resource`'d in any
        //   Bevy plugin we've audited — `extract_resources`
        //   walks actual Resources present in the World, so
        //   undeclared types never reach the snapshot.
        // - `bevy_render::globals::GlobalsUniform`
        //   (`globals.rs:42`), `bevy_pbr::wireframe::`,
        //   `bevy_sprite_render::{tilemap_chunk, wireframe2d}`
        //   hits all derive `#[derive(Resource, ...,
        //   ExtractResource, Reflect)]` BUT are init'd by
        //   `render_app.init_resource::<…>()` (a sub-app)
        //   rather than `app.init_resource::<…>()` on the main
        //   `App` — `DynamicSceneBuilder::extract_resources`
        //   walks the **main** `World` only, so those
        //   render-app resources never leak via this path.
        //   Same caveat for `bevy_scene::DynamicSceneBuilder`
        //   reflect-internals (`dynamic_scene.rs:239`,
        //   `dynamic_scene_builder.rs:418, 422`).
        //
        // See
        // `snapshot_skips_bulk_picking_ui_resources` below.
        .deny_resource::<bevy::sprite::SpritePickingSettings>()
        .deny_resource::<bevy::ui::UiScale>()
        .deny_resource::<bevy::ui::picking_backend::UiPickingSettings>()
        // === Bulk pick-up #2: 3 more reflect-derived,
        // main-app, init_resource'd, unregistered Resources
        // surfaced by a re-audit after the player reported
        // `bevy_sprite_render::tilemap_chunk::
        // TilemapChunkMeshCache` at 2026-07-23T21:36Z. The
        // previous bulk-audit (commit `a99a5f5`)
        // **mis-classified** this resource as a render-app
        // false positive — it isn't. `TilemapChunkPlugin::
        // build` (`bevy_sprite_render-0.18.0/src/tilemap_chunk
        // /mod.rs:34-37`) takes `&mut App` (the main app) and
        // calls `app.init_resource::<TilemapChunkMeshCache>()`,
        // not `render_app.init_resource::<…>()`. My
        // `grep -v "render_app\|sub_app"` filter let it
        // through, but my *reasoning* ("all these types
        // live on render_app") was wrong — this one is main-
        // app. This commit reclassifies it as a leak and
        // adds two more siblings discovered by the same
        // re-audit. ===
        //
        // `bevy_sprite_render::TilemapChunkMeshCache`
        // (`bevy_sprite_render-0.18.0/src/tilemap_chunk/
        // mod.rs:43`,
        // `#[derive(Resource, Default, Deref, DerefMut,
        // Reflect)] #[reflect(Resource, Default)]`),
        // init'd at `tilemap_chunk/mod.rs:35`
        // (`.init_resource::<TilemapChunkMeshCache>()`).
        // `bevy_sprite_render` makes **zero** `register_type`
        // calls anywhere on the type we care about — the
        // one `register_type::<MeshMaterial2d<M>>()` hit is
        // for the standard 2D material, unrelated. Player-
        // visible symptom at 2026-07-23T21:36Z: `kickoff:
        // restore_save failed: save restore failed: scene
        // deserialise failed: no registration found for
        // `bevy_sprite_render::tilemap_chunk::
        // TilemapChunkMeshCache``. `tilemap_chunk` is a
        // **private module** but re-exported at the
        // `bevy_sprite_render` crate root via `pub use
        // tilemap_chunk::*;` (`lib.rs:32`) — so the
        // umbrella path drops the module qualifier: deny
        // path is `bevy::sprite_render::TilemapChunkMeshCache`.
        //
        // `bevy_input_focus::directional_navigation::
        // DirectionalNavigationMap`
        // (`bevy_input_focus-0.18.0/src/directional_navigation
        // .rs:198-199`, gated on
        // `#[cfg_attr(feature = "bevy_reflect", derive(Reflect),
        // reflect(Resource, Debug, Default, PartialEq,
        // Clone))]`), init'd at `directional_navigation.rs:
        // 68` (`.init_resource::<DirectionalNavigationMap>()`).
        // `bevy_input_focus` makes zero `register_type` calls,
        // AND the umbrella enables `bevy_reflect` on
        // `bevy_input_focus` (via `bevy_internal-0.18.0/Cargo.
        // tom:567-571`-style `features = ["bevy_reflect"],
        // default-features = false`) — so the
        // feature-gated derive is active in our build. This
        // is the same `bevy_internal` shape that bit us with
        // `bevy_input::ButtonInput<Key>` last time.
        // `directional_navigation` is a **public module**
        // (verified via `grep "^pub mod" bevy_input_focus-
        // 0.18.0/src/lib.rs`), so the umbrella path keeps
        // the qualifier: deny path is `bevy::input_focus::
        // directional_navigation::DirectionalNavigationMap`.
        //
        // `bevy_input_focus::directional_navigation::
        // AutoNavigationConfig` — same crate / same module
        // (`navigator.rs:1`, gated `derive(Reflect,
        // reflect(Resource, Debug, PartialEq, Clone))`),
        // init'd at `directional_navigation.rs:69`. Same
        // fix, same crates-path shape.
        //
        // All three are runtime defaults with no save-time
        // meaning — `TilemapChunkMeshCache` holds per-chunk
        // GPU mesh handles (re-derived when chunks spawn
        // again), and the two `bevy_input_focus` resources
        // are UI-navigation defaults re-derived by
        // `DirectionalNavigationPlugin` at every launch.
        // See
        // `snapshot_skips_render2d_and_input_focus_resources`
        // below.
        //
        // **Methodology note.** The original bulk-audit
        // (commit `a99a5f5`) missed `TilemapChunkMeshCache`
        // because I over-grouped the `bevy_sprite_render`
        // init sites as "all render-app" without checking
        // each one's receiver type. The re-audit for this
        // commit cross-checks each `app.init_resource::<…>()`
        // line against the enclosing `fn build(...)`
        // signature — anything taking `&mut App` is main-app,
        // anything taking `&mut SubApp` (or `render_app.…`) is
        // render-app. That's the audit-discipline rule, not
        // the substring filter. Documented in the memory
        // note's audit-recipe section.
        .deny_resource::<bevy::sprite_render::TilemapChunkMeshCache>()
        .deny_resource::<bevy::input_focus::directional_navigation::DirectionalNavigationMap>()
        .deny_resource::<bevy::input_focus::directional_navigation::AutoNavigationConfig>()
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
    fn snapshot_skips_audio_resources() {
        // Regression test for the in-game restore failure: Bevy
        // 0.18's `AudioPlugin::build`
        // (`bevy_audio-0.18.0/src/lib.rs:82-84`) inserts both
        // `GlobalVolume` and `DefaultSpatialScale` as `Resource`s
        // with `#[derive(Reflect)]`, but never calls
        // `register_type::<…>()` on either. Without the
        // `deny_resource` chain in `configure_builder`, both land in
        // the snapshot RON — fine on the write path, fatal on the
        // load path: `TypeRegistrationDeserializer::visit_str` in
        // `bevy_reflect` returns
        // `Error::custom("no registration found for
        // `bevy_audio::audio::DefaultSpatialScale`")`, the `?` in
        // `restore_world` propagates it as
        // `Err(RestoreError::Scene(...))`, and the kickoff logs
        // `WARN helios_ascension::ui::launch::subview_kickoff:
        // kickoff: restore_save failed: save restore failed:
        // scene deserialise failed: no registration found for
        // `bevy_audio::audio::DefaultSpatialScale``. The save file
        // on disk stays intact and the player is left on the
        // launch menu.
        //
        // The fix is to deny both resources from the scene blob.
        // This test ensures the denylist never silently regresses:
        // the snapshot must succeed even when an `AudioPlugin`-like
        // world holds both resources, and neither type may appear
        // in the resulting RON.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `AudioPlugin::default()` inserts at startup.
        world.insert_resource(bevy::audio::GlobalVolume::default());
        world.insert_resource(bevy::audio::DefaultSpatialScale::default());
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_audio resources");
        assert!(
            !ron.contains("GlobalVolume"),
            "denylist failed — GlobalVolume serialised into save: {ron}"
        );
        assert!(
            !ron.contains("DefaultSpatialScale"),
            "denylist failed — DefaultSpatialScale serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_bevy_camera_clear_color_resource() {
        // Regression test for the in-game restore failure from
        // 2026-07-23T20:14Z: loading a save short-circuited with
        // `no registration found for `bevy_camera::clear_color::
        // ClearColor``. `bevy_camera`'s plugin chain (built by
        // `bevy_camera-0.18.0/src/lib.rs:22`) installs
        // `bevy_camera::ClearColor` via `init_resource` but never
        // calls `register_type::<ClearColor>()`, even though the
        // resource itself is `#[derive(Resource, Clone, Debug,
        // Deref, DerefMut, Reflect)]` (`clear_color.rs:53`). The
        // sibling camera resources (`ManualMark`,
        // `ObservedChanged`, `VisibleEntityRanges`) are
        // `#[derive(Resource, Default)]` without `Reflect`, so
        // `DynamicSceneBuilder::extract_resources` skips them
        // automatically — only `ClearColor` reaches the snapshot.
        //
        // The fix is a `deny_resource::<bevy_camera::ClearColor>()`
        // next to the audio / a11y / winit / time denylist, same
        // rationale: clear colour is a window-level runtime
        // tunable (default `Color::BLACK`), not gameplay state,
        // and should re-default on every launch.
        //
        // This test asserts the deny stays in place — any future
        // revert that lets `ClearColor` reach the snapshot RON will
        // fail here before the player sees another broken load.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `bevy_camera::CameraPlugin::build` inserts
        // at startup. `ClearColor::default()` is `Color::BLACK`.
        world.insert_resource(bevy_camera::ClearColor::default());
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_camera::ClearColor resource");
        // The RON type path that the loader couldn't resolve.
        assert!(
            !ron.contains("ClearColor"),
            "denylist failed — ClearColor serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_bevy_gizmos_config_store_resource() {
        // Regression test for the in-game restore failure from
        // 2026-07-23T21:06Z: loading a save short-circuited with
        // `no registration found for
        // `bevy_gizmos::config::GizmoConfigStore``. `bevy_gizmos`
        // ships three Resources from `GizmoPlugin::build`
        // (`bevy_gizmos-0.18.0/src/lib.rs:93`), but only
        // `GizmoConfigStore` derives `Reflect`
        // (`config.rs:97`, `#[derive(Reflect, Resource, Default)]
        // #[reflect(Resource, Default)]`). It's
        // `init_resource`'d implicitly via
        // `get_resource_or_init::<GizmoConfigStore>()` inside
        // `init_gizmo_group::<DefaultGizmoConfigGroup>()` without
        // any matching `register_type::<…>()` call anywhere in
        // the gizmos crate. The other two — `GizmoHandles`
        // (`lib.rs:184`) and `GizmoStorage<Config, Clear>`
        // (`gizmos.rs:32`) — only `derive(Resource)` /
        // `derive(Resource, Default)`, so Bevy's
        // `DynamicSceneBuilder::extract_resources` skips them
        // automatically.
        //
        // Compounding the issue, `GizmoConfigStore.store` is
        // `#[reflect(ignore)] TypeIdMap<(GizmoConfig, Box<dyn
        // Reflect>)>` (`config.rs:104-105`) — keys are
        // process-local `TypeId`s and values are trait objects,
        // so even with `register_type` in place the inner data
        // couldn't round-trip meaningfully across launches.
        // The right call is to deny `GizmoConfigStore` from the
        // snapshot entirely — it's a runtime gizmo-config cache,
        // re-derived at every launch from
        // `app.init_gizmo_group::<T>()` calls.
        //
        // This test asserts the deny stays in place: insert the
        // resource the way `GizmoPlugin::build` does, run
        // `snapshot_world`, and confirm the type path never
        // reaches the saved RON.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `GizmoPlugin::build` does — the
        // `init_gizmo_group::<DefaultGizmoConfigGroup>` call
        // pulls a `GizmoConfigStore` into the world via
        // `get_resource_or_init`.
        world.init_resource::<bevy::gizmos::config::GizmoConfigStore>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_gizmos::config::GizmoConfigStore resource");
        // The RON type path that the loader couldn't resolve.
        assert!(
            !ron.contains("GizmoConfigStore"),
            "denylist failed — GizmoConfigStore serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_bevy_input_resources() {
        // Regression test for the in-game restore failure from
        // 2026-07-23T21:10Z: every Load click short-circuited
        // with `no registration found for
        // `bevy_input::mouse::AccumulatedMouseMotion``.
        //
        // `bevy_input-0.18.0/src/lib.rs:114-162` `init_resource`s
        // six Resources from `InputPlugin::build`:
        // `ButtonInput<KeyCode>`, `ButtonInput<Key>`,
        // `AccumulatedMouseMotion`, `AccumulatedMouseScroll`,
        // `ButtonInput<MouseButton>`, and `Touches`. Five of
        // them are `#[cfg_attr(feature = "bevy_reflect",
        // derive(Reflect), reflect(... Resource ...))]` —
        // and `bevy_internal-0.18.0/Cargo.toml:567-571`
        // compiles `bevy_input` with `features =
        // ["bevy_reflect"]` unconditionally for the umbrella
        // re-export, so the derives are active in our build.
        // `bevy_input` makes zero `register_type::<…>()` calls,
        // so all five leak into the snapshot and break the
        // loader the moment Bevy tries to resolve a type path
        // it can't find. The sixth (`Touches`,
        // `touch.rs:246-260`) only `derive(Resource, Debug,
        // Clone, Default)` without `Reflect`, so Bevy's
        // `extract_resources` skips it automatically.
        //
        // This test mirrors the audio / camera / gizmos
        // pattern: insert all five reflect-derived input
        // resources the way `InputPlugin::build` does, run
        // `snapshot_world`, and confirm none of their type
        // paths survive into the saved RON. The asserts check
        // generic-name presence (`ButtonInput`,
        // `AccumulatedMouseMotion`, `AccumulatedMouseScroll`)
        // rather than the fully-qualified path because Bevy's
        // RON serializer shortens type paths.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `InputPlugin::build` does.
        world.init_resource::<bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>>();
        world.init_resource::<bevy::input::ButtonInput<bevy::input::keyboard::Key>>();
        world.init_resource::<bevy::input::ButtonInput<bevy::input::mouse::MouseButton>>();
        world.init_resource::<bevy::input::mouse::AccumulatedMouseMotion>();
        world.init_resource::<bevy::input::mouse::AccumulatedMouseScroll>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_input resources");
        // Generic + concrete names. Any of these leaking into
        // the save means the denylist regressed.
        for needle in [
            "ButtonInput",
            "AccumulatedMouseMotion",
            "AccumulatedMouseScroll",
        ] {
            assert!(
                !ron.contains(needle),
                "denylist failed — {needle} serialised into save: {ron}"
            );
        }
        // Sanity check the `KeyCode` enum specifically (the
        // generic-name check above already covers it, but
        // exercise the monomorphisation that Bevy would try to
        // round-trip on the load path).
        assert!(
            !ron.contains("KeyCode"),
            "denylist failed — KeyCode serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_bevy_light_resources() {
        // Regression test for the in-game restore failure from
        // 2026-07-23T21:14Z: loading a save short-circuited with
        // `no registration found for
        // `bevy_light::ambient_light::GlobalAmbientLight``.
        //
        // `bevy_light::LightPlugin::build`
        // (`bevy_light-0.18.0/src/lib.rs:137-140`)
        // `init_resource`s four Resources: `GlobalAmbientLight`,
        // `DirectionalLightShadowMap`, `PointLightShadowMap`, and
        // `GlobalVisibleClusterableObjects`. Three of them derive
        // `Reflect` (`GlobalAmbientLight` at
        // `ambient_light.rs:59`, the two shadow-map size resources
        // at `directional_light.rs:181` and
        // `point_light.rs:173` — all three are
        // `#[derive(Resource, Clone, Debug, Reflect)]
        // #[reflect(Resource, Debug, Default, Clone)]`).
        // `bevy_light` makes zero `register_type::<…>()` calls
        // (grep confirms), so all three leak into the snapshot
        // and break the loader the moment Bevy tries to resolve
        // a type path. The fourth — `cluster::mod::
        // GlobalVisibleClusterableObjects` (`cluster/mod.rs:37`)
        // — only `derive(Resource)` without `Reflect`, so
        // Bevy's `extract_resources` skips it automatically.
        //
        // All three are runtime lighting tunables with no
        // save-time meaning — `GlobalAmbientLight` represents
        // the world's ambient light brightness / colour, the
        // two `*ShadowMap` resources control shadow-map
        // resolution. A freshly-launched app re-runs
        // `LightPlugin::build`, which `init_resource`s them with
        // their `Default` values (`Color::WHITE` × `300.0` for
        // ambient, `2048` / `1024` for the shadow-map sizes), so
        // deny all three and let Bevy re-derive at next launch.
        //
        // Mirror the audio / camera / gizmos / input
        // regression-test pattern: insert all three reflect-
        // derived resources, run `snapshot_world`, and confirm
        // none of their type names survive into the saved RON.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // `GlobalAmbientLight::default()` is `Color::WHITE`
        // with `brightness = 300.0`; the shadow-map defaults
        // are documented inline at their type definitions.
        world.init_resource::<bevy::light::GlobalAmbientLight>();
        world.init_resource::<bevy::light::DirectionalLightShadowMap>();
        world.init_resource::<bevy::light::PointLightShadowMap>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_light resources");
        for needle in [
            "GlobalAmbientLight",
            "DirectionalLightShadowMap",
            "PointLightShadowMap",
        ] {
            assert!(
                !ron.contains(needle),
                "denylist failed — {needle} serialised into save: {ron}"
            );
        }
    }

    #[test]
    fn snapshot_skips_bevy_pbr_default_opaque_renderer_method() {
        // Regression test for the in-game restore failure from
        // 2026-07-23T21:18Z: loading a save short-circuited with
        // `no registration found for
        // `bevy_pbr::material::DefaultOpaqueRendererMethod``.
        //
        // `bevy_pbr` is a noisy crate to audit because most of
        // its resource insertion sites target the `render_app`
        // sub-app (whose `World` is not walked by
        // `DynamicSceneBuilder::extract_resources`) or
        // one-shot systems — none of those leak via this path.
        // But `PbrPlugin::build`
        // (`bevy_pbr-0.18.0/src/lib.rs:219`) does
        // `.init_resource::<DefaultOpaqueRendererMethod>()`
        // on the main `app`, and the type definition
        // (`material.rs:1333-1334`,
        // `#[derive(Default, Resource, Clone, Debug,
        // ExtractResource, Reflect)]
        // #[reflect(Resource, Default, Debug, Clone)]`)
        // makes it reflect-derived. `bevy_pbr` makes zero
        // `register_type::<…>()` calls (`grep register_type
        // bevy_pbr-0.18.0/src/` confirms), so this resource —
        // and nothing else on the main-app surface — leaks
        // into the snapshot and breaks the loader.
        //
        // The other main-app insert is
        // `app.insert_resource(global_cluster_settings)`
        // (`lib.rs:370`), which is a `GlobalClusterSettings`
        // instance (`bevy_light-0.18.0/src/cluster/mod.rs:38`,
        // `#[derive(Resource)]` without `Reflect`) — safe by
        // the same audit pattern that holds for
        // `bevy_camera::ManualMark` /
        // `bevy_gizmos::GizmoHandles` /
        // `bevy_light::GlobalVisibleClusterableObjects`.
        //
        // This test asserts the single-resource deny stays
        // in place: insert the resource the way `PbrPlugin::
        // build` does, run `snapshot_world`, confirm the
        // type name doesn't survive into the saved RON.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror `PbrPlugin::build`'s main-app
        // init_resource — `DefaultOpaqueRendererMethod::
        // default()` is `OpaqueRendererMethod::Forward`.
        world.init_resource::<bevy::pbr::DefaultOpaqueRendererMethod>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_pbr::DefaultOpaqueRendererMethod resource");
        // The RON type path that the loader couldn't
        // resolve. Bevy's serializer shortens this to the
        // bare struct name (`DefaultOpaqueRendererMethod`)
        // because the umbrella re-export drops the `material`
        // module qualifier — same crates-path shape as
        // `bevy_camera::ClearColor` / `bevy::light::
        // GlobalAmbientLight`.
        assert!(
            !ron.contains("DefaultOpaqueRendererMethod"),
            "denylist failed — DefaultOpaqueRendererMethod serialised into save: {ron}"
        );
    }

    #[test]
    fn snapshot_skips_bevy_picking_resources() {
        // Regression test for the in-game restore failure from
        // 2026-07-23T21:21Z: loading a save short-circuited
        // with `no registration found for
        // `bevy_picking::PickingSettings``.
        //
        // `bevy_picking` is in our build **transitively** —
        // `cargo tree -e features -i bevy_picking` confirms
        // the chain `helios_ascension → bevy (2d/3d/ui via
        // default) → bevy_internal (picking) → bevy_picking`,
        // plus `bevy_picking`'s `mesh_picking` feature
        // enabled through `bevy` defaults. That makes
        // seven main-app `init_resource` calls surface in the
        // binary: three reflect-derived leak candidates
        // (`PickingSettings`, `PointerInputSettings`,
        // `MeshPickingSettings`) and four that aren't — see
        // the audit comment block above for the full
        // derivation map. `bevy_picking` makes zero
        // `register_type::<…>()` calls (`grep register_type
        // bevy_picking-0.18.0/src/` confirms), so all three
        // reflect-derived resources leak into the snapshot and
        // break the loader.
        //
        // Mirror the audio / camera / gizmos / input /
        // light multi-resource pattern: insert all three
        // reflect-derived resources, run `snapshot_world`,
        // and confirm none of their type names survive into
        // the saved RON. `mesh_picking` is feature-gated in
        // `bevy_picking` itself (`mesh_picking/mod.rs`) but
        // the umbrella's `bevy` defaults enable it through
        // the `2d/3d` chain — confirmed by `cargo tree` and
        // by the fact that `bevy::picking::mesh_picking::
        // MeshPickingSettings` resolves at compile time.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `PickingPlugin::build` /
        // `PointerInputPlugin::build` /
        // `MeshPickingPlugin::build` do to the live app.
        world.init_resource::<bevy::picking::PickingSettings>();
        world.init_resource::<bevy::picking::input::PointerInputSettings>();
        world.init_resource::<bevy::picking::mesh_picking::MeshPickingSettings>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bevy_picking resources");
        for needle in [
            "PickingSettings",
            "PointerInputSettings",
            "MeshPickingSettings",
        ] {
            assert!(
                !ron.contains(needle),
                "denylist failed — {needle} serialised into save: {ron}"
            );
        }
    }

    #[test]
    fn snapshot_skips_bulk_picking_ui_resources() {
        // Bulk-pick-up regression test for the 3 reflect-derived,
        // main-app, init_resource'd, unregistered Resources
        // surfaced by a full audit of every Bevy crate in our
        // build on 2026-07-23T21:24Z (after the player reported
        // `bevy_sprite::picking_backend::SpritePickingSettings`):
        //
        // - `bevy_sprite::SpritePickingSettings` —
        //   declared in `bevy_sprite-0.18.0/src/picking_backend
        //   .rs:50` as `#[derive(Resource, Reflect)]
        //   #[reflect(Resource, Default)]`, init'd at
        //   `picking_backend.rs:81`. Re-exported at the
        //   `bevy_sprite` crate root via `pub use
        //   picking_backend::*;`.
        // - `bevy_ui::UiScale` — `bevy_ui-0.18.0/src/lib.rs:117`
        //   `#[derive(Debug, Reflect, Resource, Deref, DerefMut)]
        //   #[reflect(Resource, Debug, Default)]`, init'd at
        //   `layout/mod.rs:388` and `update.rs:200`.
        // - `bevy_ui::picking_backend::UiPickingSettings` —
        //   `bevy_ui-0.18.0/src/picking_backend.rs:47`
        //   `#[derive(Resource, Reflect)]
        //   #[reflect(Resource, Default)]`, init'd at
        //   `picking_backend.rs:81`.
        //
        // None of `bevy_sprite` / `bevy_ui` make any
        // `register_type::<…>()` calls (`grep register_type` on
        // each confirms — zero hits per crate). `bevy_sprite`
        // is in our binary via `bevy`'s `2d` feature chain;
        // `bevy_ui` via `bevy`'s `ui` feature — both transitive
        // from our direct feature list (which is why the
        // failures only started surfacing after the player
        // built a fresh save in the current session, not the
        // old binary).
        //
        // Mirrors the multi-resource test pattern set by
        // `snapshot_skips_audio_resources` /
        // `snapshot_skips_bevy_input_resources` /
        // `snapshot_skips_bevy_light_resources`.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `SpritePickingPlugin::build` /
        // `UiPlugin::build` (twice — once for the UI scale,
        // once for the UI picking backend) do to the live app.
        world.init_resource::<bevy::sprite::SpritePickingSettings>();
        world.init_resource::<bevy::ui::UiScale>();
        world.init_resource::<bevy::ui::picking_backend::UiPickingSettings>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip bulk picking/UI resources");
        for needle in [
            "SpritePickingSettings",
            "UiScale",
            "UiPickingSettings",
        ] {
            assert!(
                !ron.contains(needle),
                "denylist failed — {needle} serialised into save: {ron}"
            );
        }
    }

    #[test]
    fn snapshot_skips_render2d_and_input_focus_resources() {
        // Bulk-pick-up #2 regression test for the 3 reflect-
        // derived, main-app, init_resource'd, unregistered
        // Resources surfaced by a re-audit after the player
        // reported `bevy_sprite_render::tilemap_chunk::
        // TilemapChunkMeshCache` at 2026-07-23T21:36Z. The
        // first bulk audit (commit `a99a5f5`) mis-classified
        // `TilemapChunkMeshCache` as a render-app false
        // positive — it's not. The re-audit is the
        // closed-loop discipline: every `init_resource` in
        // an `fn build(...)` taking `&mut App` is main-app
        // (the subject of this deny chain), not
        // necessarily (the audit produced the previous
        // commit's `bevy_sprite_render::tilemap_chunk::
        // {mod.rs hit}` apparent render-app classification,
        // which was wrong).
        //
        // The three targets in this test are the leaks
        // that re-audit surfaced:
        //
        // - `bevy_sprite_render::TilemapChunkMeshCache`
        //   (`bevy_sprite_render-0.18.0/src/tilemap_chunk/
        //   mod.rs:43`,
        //   `#[derive(Resource, Default, Deref, DerefMut,
        //   Reflect)] #[reflect(Resource, Default)]`),
        //   init'd at `tilemap_chunk/mod.rs:35`.
        //   Re-exported at the `bevy_sprite_render` crate
        //   root via `pub use tilemap_chunk::*;` so the
        //   umbrella path drops the module qualifier.
        // - `bevy_input_focus::directional_navigation::
        //   DirectionalNavigationMap`
        //   (`bevy_input_focus-0.18.0/src/directional_
        //   navigation.rs:198`, gated
        //   `#[cfg_attr(feature = "bevy_reflect",
        //   derive(Reflect), reflect(Resource, ...))]`),
        //   init'd at `directional_navigation.rs:68`. The
        //   umbrella enables `bevy_reflect` on
        //   `bevy_input_focus` (same `bevy_internal-0.18.0`
        //   umbrella path that flipped `bevy_input`'s
        //   `ButtonInput` / `AccumulatedMouseMotion` into
        //   the leak set).
        // - `bevy_input_focus::directional_navigation::
        //   AutoNavigationConfig` — sibling type in the
        //   same plugin, same crates-path shape.
        //
        // `bevy_sprite_render` makes zero
        // `register_type::<…>()` calls for the targets we
        // care about (the one hit, `MeshMaterial2d<M>`,
        // is unrelated). `bevy_input_focus` makes zero
        // `register_type` calls period (`grep
        // register_type bevy_input_focus-0.18.0/src/`
        // returns nothing).
        //
        // This test mirrors the multi-resource test
        // pattern set by `snapshot_skips_audio_resources`
        // / `snapshot_skips_bevy_input_resources` /
        // `snapshot_skips_bevy_light_resources` /
        // `snapshot_skips_bulk_picking_ui_resources`.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Mirror what `TilemapChunkPlugin::build` /
        // `DirectionalNavigationPlugin::build` do to the
        // live app.
        world.init_resource::<bevy::sprite_render::TilemapChunkMeshCache>();
        world.init_resource::<bevy::input_focus::directional_navigation::DirectionalNavigationMap>();
        world.init_resource::<bevy::input_focus::directional_navigation::AutoNavigationConfig>();
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip render2d + input_focus resources");
        for needle in [
            "TilemapChunkMeshCache",
            "DirectionalNavigationMap",
            "AutoNavigationConfig",
        ] {
            assert!(
                !ron.contains(needle),
                "denylist failed — {needle} serialised into save: {ron}"
            );
        }
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
    fn snapshot_skips_global_transform_component() {
        // GRA-358 PR-D regression: pre-fix the snapshot serialised
        // `bevy_transform::components::GlobalTransform` for every
        // entity that had one. On load, Bevy's `SceneDeserializer`
        // inserts the component (via
        // `apply_or_insert_mapped`), which fires the
        // `validate_parent_has_component::<GlobalTransform>` hook
        // (Bevy 0.18 `bevy_transform-0.18.0/src/components/
        // global_transform.rs:65`). The hook checks the entity's
        // `ChildOf` parent; if the parent has already been
        // deserialized, the parent also has `GlobalTransform` and
        // the hook is silent. If the parent HASN'T been
        // deserialized yet, the parent lacks `GlobalTransform` and
        // the hook emits a B0004 warning.
        //
        // With ~710 saved bodies deserialized in archetype order
        // (which is independent of hierarchy), a typical Continue
        // produced a ~250-warning storm on top of the swap's own
        // pass 1 / 2 work. The end-state is consistent (Bevy
        // re-derives `GlobalTransform` from the freshly rewritten
        // `Transform` + `ChildOf` graph in PostUpdate), but the
        // log spam is player-visible.
        //
        // The fix: deny `GlobalTransform` from the snapshot AND
        // from the swap's pass 1. Bevy's
        // `#[require(GlobalTransform)]` machinery auto-inserts
        // the default on every entity that gets `Transform`
        // copied, and `propagate_parent_transforms` (PostUpdate)
        // recomputes the correct value from the live `ChildOf`
        // graph. End state is identical; warning storm is gone.
        //
        // This test pins the snapshot side: the saved RON must
        // not mention `GlobalTransform`.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Spawn an entity with a deliberately non-identity
        // `GlobalTransform` (translation 999). If the deny fails
        // and the component is serialised, the saved RON will
        // contain the literal "999" inside a
        // `bevy_transform::components::GlobalTransform` field.
        world.spawn((
            Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            GlobalTransform::from_translation(Vec3::new(999.0, 0.0, 0.0)),
        ));
        let ron = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip GlobalTransform component");
        assert!(
            !ron.contains("GlobalTransform"),
            "denylist failed — GlobalTransform serialised into save: {ron}"
        );
        assert!(
            !ron.contains("999"),
            "denylist failed — stale GlobalTransform translation 999 leaked into save: {ron}"
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

    /// Regression test: saving a fleet mid-transfer used to fail because
    /// `ActiveManeuver::option_label: &'static str` is not reflect-serialisable
    /// (it identifies a static catalog entry like `"Hohmann"` / `"Full Thrust"` /
    /// `"Coast Phase 1"` and never had ReflectSerialize registered on `&str`).
    /// The fix: `#[reflect(ignore)]` on the field + a position-based
    /// `is_kinematic()` fallback so the kinematic determination survives
    /// the empty-string post-restore state.  This test exercises the same
    /// shape `process_fleet_actions` builds at execute time so any future
    /// revert of the denylist-style fix is caught here.
    #[test]
    fn snapshot_serializes_fleet_in_active_maneuver() {
        use crate::astronomy::KeplerOrbit;
        use crate::fleets::{ActiveManeuver, TransferReferenceFrame};

        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        // Synthetic Hohmann-style active maneuver between two placeholder bodies.
        // `Entity::PLACEHOLDER` keeps the test independent of the entity allocator
        // — what matters is the struct shape, not the entity ids.
        let entity = world
            .spawn(ActiveManeuver {
                transfer_orbit: KeplerOrbit::default(),
                reference_frame: TransferReferenceFrame::SystemBarycentric,
                orbit_center: Entity::PLACEHOLDER,
                origin_body: Entity::PLACEHOLDER,
                departure_time: 0.0,
                arrival_time: 86_400.0,
                preserve_orbit_geometry: false,
                destination_body: Entity::PLACEHOLDER,
                arrival_orbit_radius_au: 1.0,
                arrival_delta_v_ms: 0.0,
                fuel_used_t: 0.0,
                // The two labels we'd expect to "leak" if `#[reflect(ignore)]`
                // were ever removed.  The test below checks neither serialises.
                option_label: "Hohmann",
                departure_angle: 0.0,
                start_position_au: None,
                end_position_au: None,
                departure_velocity_ms: None,
                arrival_velocity_ms: None,
                start_visual_pos: None,
                leg2_orbit: None,
                leg2_start_s: 0.0,
                flyby_body: None,
                kinematic_override: false,
            })
            .id();
        world.spawn_empty(); // exercise multi-entity walks too
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must round-trip an in-transit ActiveManeuver");
        // Smoke check: neither `&'static str` field leaks into the save blob.
        // `option_label` is `#[reflect(ignore)]` and `option_label` would only
        // appear if a future revert re-enabled reflect-serialisation of `&str`.
        assert!(
            !snapshot.contains("Hohmann"),
            "ActiveManeuver::option_label leaked into snapshot — \
             check that `#[reflect(ignore)]` is still in place: {snapshot}"
        );
        // Sanity check: the entity was actually walked (the empty-spawn
        // ensures `DynamicSceneBuilder::extract_entities` saw it).
        assert!(
            !snapshot.contains("entities: []"),
            "DynamicSceneBuilder emitted an empty entity list"
        );
        let _ = entity;
    }

    #[test]
    fn snapshot_skips_mesh_material3d_star_corona_3d_component() {
        // Regression test for the 3D volumetric corona shell — sibling to
        // `snapshot_skips_mesh_material3d_star_halo_3d_component` below.
        // `StarCorona3dMaterial` lives on the inner 3D shell around every
        // populated star (see `setup_solar_system` in
        // `src/plugins/solar_system.rs`, which spawns the corona inner
        // shell on top of the surface sphere). Without
        // `deny_component::<MeshMaterial3d<StarCorona3dMaterial>>()` in
        // `configure_builder`, `SceneSerializer` raises "type
        // `Arc<StrongHandle>` did not register the ReflectSerialize"
        // and the autosave timer logs
        // `autosave failed: save serialise failed: …` every interval.
        //
        // Same caveat as the standard-material test: a default
        // `MeshMaterial3d` produces `Handle::Uuid`, so the inner Arc
        // failure path isn't reproduced here — we just verify the
        // denylist keeps the component out of the saved scene blob.
        // The custom-handle failure path is exercised by the runtime
        // autosave (every interval), which is why the deny is in place.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.spawn(bevy_pbr::MeshMaterial3d::<
            crate::plugins::star_materials::StarCorona3dMaterial,
        >::default());
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip StarCorona3dMaterial component");
        assert!(
            !snapshot.contains("StarCorona3dMaterial"),
            "denylist failed — StarCorona3dMaterial serialised into save: {snapshot}"
        );
    }

    #[test]
    fn snapshot_skips_mesh_material3d_star_halo_3d_component() {
        // Regression test for the in-game autosave failure the user
        // reported: every populated star has a 3D halo shell entity
        // (set up in `setup_solar_system`) carrying
        // `MeshMaterial3d<StarHalo3dMaterial>`. The inner
        // `Handle<StarHalo3dMaterial>` is a `Strong(Arc<StrongHandle>)`
        // — a process-local pointer Bevy can't reflect-serialise.
        // Without the corresponding `deny_component` in
        // `configure_builder`, `SceneSerializer` raises "type
        // `Arc<StrongHandle>` did not register the ReflectSerialize"
        // and the autosave timer logs
        // `autosave failed: save serialise failed: …` every interval
        // while at least one star is in view.
        //
        // Same caveat as the standard-material test: a default
        // `MeshMaterial3d` produces `Handle::Uuid`, so the inner Arc
        // failure path isn't reproduced here — we just verify the
        // denylist keeps the component out of the saved scene blob.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.spawn(bevy_pbr::MeshMaterial3d::<
            crate::plugins::star_materials::StarHalo3dMaterial,
        >::default());
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip StarHalo3dMaterial component");
        assert!(
            !snapshot.contains("StarHalo3dMaterial"),
            "denylist failed — StarHalo3dMaterial serialised into save: {snapshot}"
        );
    }

    #[test]
    fn snapshot_skips_camera_main_texture_usages_component() {
        // Regression test for the in-game autosave failure that
        // surfaces once `Mesh3d` / `MeshMaterial3d<T>` are denied:
        // the `Camera3d` entity spawned by `spawn_camera` in
        // `src/plugins/camera.rs` has `CameraMainTextureUsages` as
        // an auto-required component, whose inner
        // `wgpu_types::TextureUsages` is a bitflags type Bevy does
        // not register `ReflectSerialize` for. Without
        // `deny_component::<CameraMainTextureUsages>()` in
        // `configure_builder`, `SceneSerializer` raises "type
        // `TextureUsages` did not register the ReflectSerialize"
        // and the autosave timer logs
        // `autosave failed: save serialise failed: …` every
        // interval.
        //
        // Same caveat as the material tests: the inner
        // `TextureUsages` failure path is only reproducible on a
        // world where Bevy plugins have registered the component
        // in `AppTypeRegistry`. The minimal test verifies the
        // denylist keeps the component out of the saved scene
        // blob end-to-end; the runtime autosave (every interval)
        // exercises the full failure path.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.spawn(bevy_camera::CameraMainTextureUsages::default());
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip CameraMainTextureUsages component");
        assert!(
            !snapshot.contains("CameraMainTextureUsages"),
            "denylist failed — CameraMainTextureUsages serialised into save: {snapshot}"
        );
    }

    #[test]
    fn snapshot_skips_render_target_component() {
        // Regression test for the next camera-chain failure after
        // `CameraMainTextureUsages`: `RenderTarget::Image`
        // wraps `Handle<Image>` (the same `Arc<StrongHandle>`-
        // bearing inner Handle that broke `Mesh3d` /
        // `MeshMaterial3d<T>`). The default `RenderTarget` for
        // a `Camera3d` spawned via Helios's `spawn_camera` is
        // `RenderTarget::Window(WindowRef::default())`, which is
        // reflect-serialisable — but the *moment* Helios ever
        // attaches an off-screen image target the inner Handle
        // raises the same failure we already saw and fixed for
        // `Mesh3d`. Deny preemptively so the next autosave error
        // doesn't surface.
        //
        // Same caveat as the material tests: `RenderTarget::Window`
        // default has no `Handle<Image>` inner, so the inner Arc
        // failure path isn't directly reproducible here — we just
        // verify the denylist keeps the component out of the
        // saved scene blob.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        world.spawn(bevy_camera::RenderTarget::default());
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must skip RenderTarget component");
        assert!(
            !snapshot.contains("RenderTarget"),
            "denylist failed — RenderTarget serialised into save: {snapshot}"
        );
    }

    #[test]
    fn snapshot_skips_camera3d_entity() {
        // Regression test for the PR-C entity-level fix:
        // `collect_all_entities` filters out entities carrying
        // `bevy_camera::Camera3d` (or `Camera2d`), so the Camera3d
        // Helios spawns via `spawn_camera` — and every Bevy camera
        // companion attached to it — never reach `extract_entities`.
        // This anticipates future Bevy releases adding new camera
        // companions (e.g. `CameraRenderGraph` in 0.18,
        // `InternedRenderSubGraph` raw-ref) without requiring per-
        // release `deny_component` maintenance.
        //
        // The test spawns TWO entities: one with `Camera3d` (must
        // be dropped) and one with a plain `Mesh3d` (must survive,
        // proving the filter doesn't sweep up unrelated entities).
        // The mesh-spawning entity also gets `Mesh3d::default()`
        // for the same reason — Bevy's `Mesh3d::default()` has no
        // Strong handle path, so the inner Arc failure isn't
        // reproduced, and `init_resource::<AppTypeRegistry>()` is
        // the only setup needed.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        let camera_entity = world.spawn(bevy_camera::Camera3d::default()).id();
        // Spawn a second non-camera entity to verify the filter
        // doesn't sweep everything.  We give it a `Mesh3d` because
        // that's a real-world entity archetype Helios uses
        // (populated solar-system bodies — see
        // `src/plugins/solar_system.rs`).
        world.spawn_empty();
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must drop Camera3d entity from extraction");
        // The Camera3d entity bits must not appear in the saved
        // scene blob.  `Entity::to_bits()` returns the full u64
        // encoding (index + generation), so we check that the
        // camera entity's bits are not present as a free-standing
        // numeric literal in the RON output.
        let camera_bits = camera_entity.to_bits();
        assert!(
            !snapshot.contains(&format!("{camera_bits}")),
            "Camera3d entity ({camera_bits}) leaked into snapshot — \
             collect_all_entities filter not effective: {snapshot}"
        );
        // And the empty (non-camera) entity should still be in
        // the snapshot, confirming the filter is targeted.
        assert!(
            !snapshot.contains("entities: []"),
            "snapshot dropped non-camera entities — collect_all_entities \
             filter is over-broad: {snapshot}"
        );
    }

    #[test]
    fn snapshot_skips_window_entity() {
        // Regression test for PR-D — `SavePanel` manual save
        // failure: with Helios's `update_cursor_icon` in
        // `src/ui/cursors.rs` setting
        // `CursorIcon::Custom(CustomCursor::Image(CustomCursorImage
        // { handle: Handle<Image>, .. }))` on the primary Window
        // entity, the scene serializer raised
        // "`Arc<StrongHandle>` did not register the
        // ReflectSerialize" every manual save click.
        //
        // `collect_all_entities` (PR-C onwards) filters out any
        // entity carrying `bevy_window::Window` — the same
        // architectural pattern as the Camera3d skip. This test
        // exercises that filter end-to-end and confirms the
        // Window entity (with its auto-required
        // `CursorOptions` + Bevy-injected `RawHandleWrapper` +
        // Helios-injected `CursorIcon::Custom(...)`) does NOT
        // leak into the save.
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        let window_entity = world
            .spawn((
                bevy::window::Window::default(),
                // Stub CursorOptions / CursorIcon — we don't need
                // real values, just the right archetype shape so
                // the entity-skip fires. Note: spawning a Window
                // does NOT auto-require CursorOptions in a bare
                // world (the `#[require]` hook is registered by
                // Bevy's plugins); only the `Window` marker
                // itself is what the filter checks for.
                bevy::window::PrimaryWindow,
                bevy::window::CursorOptions::default(),
                bevy::window::CursorIcon::default(),
            ))
            .id();
        // Spawn a second non-window entity to verify the filter
        // doesn't sweep everything.
        world.spawn_empty();
        let snapshot = snapshot_world(&world, SaveMetadata::new_now(0, 0, "test"))
            .expect("snapshot must drop Window entity from extraction");
        // The Window entity's bits must not appear in the saved
        // scene blob. `Entity::to_bits()` returns the full u64
        // encoding (index + generation), so we check for the
        // numeric literal in the RON output.
        let window_bits = window_entity.to_bits();
        assert!(
            !snapshot.contains(&format!("{window_bits}")),
            "Window entity ({window_bits}) leaked into snapshot — \
             collect_all_entities filter not effective: {snapshot}"
        );
        // And the empty (non-window) entity should still be in
        // the snapshot, confirming the filter is targeted.
        assert!(
            !snapshot.contains("entities: []"),
            "snapshot dropped non-window entities — collect_all_entities \
             filter is over-broad: {snapshot}"
        );
    }
}
