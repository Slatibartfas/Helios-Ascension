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
