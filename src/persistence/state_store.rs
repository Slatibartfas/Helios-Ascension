//! StateStore — regenerate-from-seed save/load (GRA-358 PR-I).
//!
//! # Why
//!
//! The v1 save path ([`super::snapshot`] + [`super::swap`]) was
//! built around Bevy's `DynamicScene` — it serialized the *entire*
//! live world, then on load spawned 9,000+ live counterparts and
//! rewrote every `ChildOf` / `OrbitCenter` / `LogicalParent` /
//! `Children` reference against a pending→live `entity_map`.
//! Every denylist drift, archetype skip, or generation collision
//! silently corrupted the remap, producing the "Mercury orbits
//! Saturn" / "Saturn's rings are missing" symptoms that PR-H was
//! trying to paper over.
//!
//! StateStore replaces that with a **regenerate + overlay** model:
//!
//! 1. **Save**: emit a small RON document that contains only the
//!    *deterministic seed* + the *divergences* from the
//!    seed-derived world (destroyed comets, per-body
//!    mean-anomaly-epoch, colony overrides, fleets, ships,
//!    research, economy, UI state, simulation history, etc.).
//! 2. **Load**: run the same `setup_solar_system` +
//!    `populate_nearby_systems` + `initialize_baseline_technology`
//!    chain the new-game path uses, then *overlay* the saved
//!    divergences on top. No `entity_map`, no remap, no denylist
//!    dance.
//!
//! # Identity model
//!
//! Every celestial body in the live world is regenerated from a
//! RON catalog keyed by `(system_id, name)`. Player entities
//! (fleets, ships, colonies, surveys) reference bodies by the
//! same `(system_id, name)` pair via a [`BodyKey`] — never by
//! `Entity` index. This is the single design rule that makes
//! regenerate + overlay work: a body the regen chain produces
//! at index 47 in the new run will be at index 1,247 in the
//! next run, but `(SystemId(0), "Mercury")` is the same body
//! every time.
//!
//! # Format
//!
//! ```ron
//! (
//!     metadata: (
//!         format_version: 2,
//!         helios_version: "0.4.0",
//!         saved_at_unix_s: 1785100725,
//!         playtime_s: 1234,
//!         seed: 1785100709,
//!         start_timestamp: 1735689600,
//!         sim_now_seconds: 5259487.5,
//!     ),
//!     bodies: {
//!         (system: 0, name: "Earth"): (
//!             colony_override: Some(( /* Colony data */ )),
//!             mean_anomaly_epoch_override: None,
//!             survey_state: Some(( /* SurveyState */ )),
//!             population_override: Some(8200000000.0),
//!         ),
//!         (system: 0, name: "1P/Halley"): (
//!             destroyed: Some(1735689600),
//!         ),
//!     },
//!     fleets: [
//!         (
//!             name: "Day-One Constellation",
//!             at_anchor: (system: 0, name: "Earth", kind: Planet),
//!             ships: [ (class: Frigate, count: 6, modules: [...]) ],
//!             pending_manoeuvre: None,
//!         ),
//!     ],
//!     research: ( unlocked: ["..."], projects: [...], points_balance: ... ),
//!     economy: ( budget: ..., stockpile: ..., shipping_companies: ..., pending_requests: ... ),
//!     ui: ( view_mode: SystemView, current_star_system: 0, ... ),
//!     notifications: ( settings: ..., ... ),
//! )
//! ```
//!
//! # Migration
//!
//! The loader detects `format_version: 1` (legacy DynamicScene)
//! and falls back to [`super::swap`]. New saves always emit
//! `format_version: 2`. A one-shot migration from v1 → v2 happens
//! the first time a v1 save is loaded under the new path (it
//! runs the v1 restore, walks the live world, emits a v2
//! `StateStore` to a sidecar `*.v2.ron`, and continues with v1
//! for that session). Future loads use the v2 sidecar.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::time::{SystemTime, UNIX_EPOCH};

use super::format_version::FORMAT_VERSION_V2;
use crate::astronomy::components::SystemId;

// ════════════════════════════════════════════════════════════════════
// Identity model
// ════════════════════════════════════════════════════════════════════

/// Stable identity for a celestial body — the regen chain
/// produces the same `(system, name)` for the same RON catalog
/// entry on every run, so this is the only key we ever persist.
///
/// We deliberately do *not* use Bevy `Entity` indices. A body
/// that was `Entity(47)` in the save will be `Entity(1247)` in
/// the new run; only `(SystemId(0), "Mercury")` is invariant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BodyKey {
    /// Star-system id (`0` = Sol). Matches `SystemId::0` on the
    /// entity. Stable across regens.
    pub system: u32,
    /// Body name exactly as it appears in the RON catalog and on
    /// the `CelestialBody::name` component. Stable across regens.
    pub name: String,
}

impl BodyKey {
    /// Construct a `BodyKey` from a `SystemId` and a name.
    pub fn new(system: SystemId, name: impl Into<String>) -> Self {
        Self {
            system: system.0 as u32,
            name: name.into(),
        }
    }

    /// Sol-system body shortcut (system 0). The vast majority of
    /// bodies are Sol-system, so this saves a lot of `SystemId(0)`
    /// noise in the save file.
    pub fn sol(name: impl Into<String>) -> Self {
        Self {
            system: 0,
            name: name.into(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Top-level container
// ════════════════════════════════════════════════════════════════════

/// The whole save file. Top-level RON value. Self-describing
/// (`format_version` is in `metadata`) so a future v3 loader
/// can read a v2 save and reject it gracefully if the schema
/// has moved.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateStore {
    pub metadata: StateStoreMetadata,
    /// Per-body divergences from the seed-derived world. Only
    /// bodies whose state actually differs from the regen
    /// appear here; the regen chain handles everything else.
    /// Keyed by `BodyKey` for stable identity across runs.
    pub bodies: BTreeMap<BodyKey, BodyDivergence>,
    /// Player-owned fleets. Each fleet anchors to a body via
    /// `BodyKey`; ships reference each other by stable
    /// `ShipKey` (fleet-local index, not global Entity).
    pub fleets: Vec<FleetRecord>,
    /// Research progress.
    pub research: ResearchRecord,
    /// Economy state (budget, stockpile, mining, shipping).
    pub economy: EconomyRecord,
    /// UI / camera state.
    pub ui: UiRecord,
    /// Notification state (settings, active toasts).
    pub notifications: NotificationRecord,
    /// Mining / survey state per body.
    pub surveys: BTreeMap<BodyKey, SurveyDivergence>,
    /// Auto-save slot bookkeeping (last auto-save time, etc.).
    pub meta_autosave: AutosaveRecord,
    /// GRA-791: persistent mission log (`current` + `past` missions
    /// + long-running `goals`). Saved as a typed record so the
    /// apply path can rebuild the live `MissionLog` resource
    /// without a JSON blob indirection — the underlying
    /// `MissionEntry` / `GoalEntry` types already derive
    /// `Serialize / Deserialize`. Old (pre-GRA-791) saves default
    /// to an empty record; the apply path is a no-op in that
    /// case.
    pub mission_log: MissionLogRecord,
}

impl StateStore {
    /// Magic header: every v2 save starts with this. The loader
    /// uses it to fast-detect v2 before parsing the RON body.
    pub const MAGIC: &'static str = "helios_state_store_v2";

    /// Empty store for a fresh new game (no player data).
    pub fn empty(seed: u64) -> Self {
        Self {
            metadata: StateStoreMetadata::new(seed),
            ..Default::default()
        }
    }

    /// Render as RON. The output starts with the magic header
    /// line so [`Self::from_ron`] can fast-detect v2 without
    /// parsing the entire body.
    pub fn to_ron(&self) -> Result<String, StateStoreError> {
        let body = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(StateStoreError::Serialize)?;
        // Magic header is a single line of the form
        // `helios_state_store_v2\n`. RON allows any identifier
        // as a top-level expression when wrapped in parens,
        // so a v2 file is one identifier + one RON struct.
        // `from_ron` strips this line and parses the rest.
        Ok(format!("{}\n{}\n", Self::MAGIC, body))
    }

    /// Parse from RON. Returns `Err(StateStoreError::NotV2Format)`
    /// if the RON doesn't start with the v2 magic (so the caller
    /// can fall back to v1).
    pub fn from_ron(s: &str) -> Result<Self, StateStoreError> {
        if !s.trim_start().starts_with(Self::MAGIC) {
            return Err(StateStoreError::NotV2Format);
        }
        // Strip the magic line (it's not part of the StateStore
        // struct; the loader injects it for fast-detection).
        let body_start = s.find('\n').ok_or(StateStoreError::NotV2Format)? + 1;
        ron::from_str(&s[body_start..]).map_err(|e| StateStoreError::Parse(e.to_string()))
    }
}

/// Errors during StateStore parse/serialize.
#[derive(Debug)]
pub enum StateStoreError {
    /// The save is not a v2 StateStore (missing magic header).
    NotV2Format,
    /// RON serialisation failed.
    Serialize(ron::Error),
    /// RON deserialisation failed.
    Parse(String),
    /// The save's `format_version` doesn't match what this
    /// binary knows how to load.
    UnsupportedVersion { found: u32, expected: u32 },
    /// The save's `helios_version` doesn't match the running
    /// build's `CARGO_PKG_VERSION`. Surfaced as a warning by
    /// the apply path; not an error by default.
    VersionMismatch { save: String, runtime: String },
}

impl std::fmt::Display for StateStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateStoreError::NotV2Format => {
                write!(f, "save is not a v2 StateStore (missing magic header)")
            }
            StateStoreError::Serialize(e) => write!(f, "RON serialise error: {e}"),
            StateStoreError::Parse(s) => write!(f, "RON parse error: {s}"),
            StateStoreError::UnsupportedVersion { found, expected } => {
                write!(
                    f,
                    "unsupported StateStore format_version: {found} (expected {expected})"
                )
            }
            StateStoreError::VersionMismatch { save, runtime } => {
                write!(
                    f,
                    "Helios version mismatch: save is {save}, runtime is {runtime}"
                )
            }
        }
    }
}

impl std::error::Error for StateStoreError {}

// ════════════════════════════════════════════════════════════════════
// Metadata
// ════════════════════════════════════════════════════════════════════

/// Top-level metadata. The loader refuses to load a v2 save
/// whose `helios_version` doesn't match the running build's
/// `CARGO_PKG_VERSION` — saves are not forwards-compatible
/// across a major version bump.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateStoreMetadata {
    /// Always `2` for this format. Distinct from
    /// [`super::format_version::FORMAT_VERSION`] which is the
    /// envelope version; the StateStore is the body.
    pub format_version: u32,
    /// `CARGO_PKG_VERSION` of the build that wrote this save.
    pub helios_version: String,
    /// Wall-clock unix-seconds the player clicked Save.
    pub saved_at_unix_s: i64,
    /// Total in-game playtime when the save was written.
    pub playtime_s: f64,
    /// Deterministic seed for the regen chain.
    pub seed: u64,
    /// `SimulationTime::start_timestamp` at game start.
    pub start_timestamp: i64,
    /// `SimulationTime::elapsed_seconds()` at save time — the
    /// regen chain uses this to re-derive mean anomalies when
    /// the saved body entry doesn't override them.
    pub sim_now_seconds: f64,
    /// Player-facing summary (current in-game date, colony
    /// count, total population, ship count, power output,
    /// Kardashev scale, resource breakdown, Kardashev
    /// history). Populated by the save-panel preview path so
    /// the Load Game list has something to render without
    /// loading the full world.
    ///
    /// Optional so older v2 saves (which didn't have this
    /// field) still deserialise — the scanner surfaces an
    /// empty preview in that case.
    #[serde(default)]
    pub preview: SavePreview,
}

/// Player-facing save preview (mirrors
/// [`crate::persistence::snapshot::SavePreview`] — kept
/// inline rather than imported to keep the StateStore
/// crate-agnostic of the v1 snapshot module that PR-I is
/// phasing out).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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

impl Default for StateStoreMetadata {
    fn default() -> Self {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self {
            format_version: FORMAT_VERSION_V2,
            helios_version: env!("CARGO_PKG_VERSION").to_string(),
            saved_at_unix_s: now_unix,
            playtime_s: 0.0,
            seed: 0,
            start_timestamp: now_unix,
            sim_now_seconds: 0.0,
            preview: SavePreview::default(),
        }
    }
}

impl StateStoreMetadata {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            ..Default::default()
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Per-body divergences
// ════════════════════════════════════════════════════════════════════

/// Per-body state that diverges from the seed-derived regen.
///
/// Every component is a free-form `serde_json::Value` blob —
/// the extract path serialises the live Bevy component (which
/// already derives `Serialize / Deserialize / Reflect`) into
/// JSON; the apply path deserialises back into a freshly-built
/// component and inserts it. This avoids hand-rolling a per-
/// component record struct for every type (`Colony`,
/// `PlanetResources`, `AtmosphereComposition`, …) and lets
/// new components join the StateStore by adding one line to
/// the extract/apply helpers — no schema change here.
///
/// Keys that aren't blobs:
/// - `destroyed_at_unix_s` + `destroyed_reason`: structured
///   fields the apply path uses to decide whether the body
///   should be skipped on this run.
/// - `mean_anomaly_epoch_override` / `semi_major_axis_override`
///   / `eccentricity_override`: structured numeric overrides
///   for the orbital elements (the regen chain sets these
///   from the save's `sim_now_seconds`; an override is only
///   needed when the player has manually nudged the orbit,
///   e.g. via terraforming or redirecting an asteroid).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BodyDivergence {
    /// Body was destroyed (e.g. comet disintegration, asteroid
    /// redirected into a star). The regen chain skips the body
    /// when this is `Some(_)`.
    #[serde(default)]
    pub destroyed: Option<DestroyedRecord>,

    /// Override the mean anomaly at epoch (orbital phase at
    /// game start). See struct doc for when to use.
    #[serde(default)]
    pub mean_anomaly_epoch_override: Option<f64>,

    /// Override the semi-major axis.
    #[serde(default)]
    pub semi_major_axis_override: Option<f64>,

    /// Override the eccentricity.
    #[serde(default)]
    pub eccentricity_override: Option<f64>,

    /// Colony on this body — serialised `Colony` component
    /// (which already implements `Serialize`).
    #[serde(default)]
    pub colony_override: Option<Json>,

    /// Population — serialised `Population` component.
    #[serde(default)]
    pub population_override: Option<Json>,

    /// Per-body resources — serialised `PlanetResources` +
    /// `LocalStockpile` (stored as a two-element tuple).
    #[serde(default)]
    pub resources_override: Option<Json>,

    /// Atmosphere — serialised `AtmosphereComposition` component.
    /// Optional so older saves (which didn't have terraforming)
    /// still load; the apply path warns when this is `Some` but
    /// `AtmosphereComposition` doesn't derive `Serialize` yet.
    #[serde(default)]
    pub atmosphere_override: Option<Json>,

    /// Body override — serialised `CelestialBody` component
    /// (mass / radius / rotation period / visual radius).
    /// Optional for the same reason as `atmosphere_override`:
    /// PR-I doesn't have a player-facing "change body mass"
    /// system, but the field is reserved so when asteroid-mining
    /// or redirect mechanics land, the extract path has a
    /// ready-made home for the divergence.
    #[serde(default)]
    pub body_override: Option<Json>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestroyedRecord {
    /// Unix-seconds the body was destroyed.
    pub at_unix_s: i64,
    /// Free-form reason ("redirected into Sol", "disintegrated",
    /// "intentionally de-orbited", etc.) for the campaign log.
    pub reason: String,
}

// ════════════════════════════════════════════════════════════════════
// Colony / resources / atmosphere records
// ════════════════════════════════════════════════════════════════════

/// Serialised construction project. Mirrors `ConstructionProject`.
///
/// Kept as a typed record because the live `ConstructionProject`
/// component carries Bevy-specific fields (`colony_entity:
/// Entity`, `blocking_request_id: Option<u64>`) that don't
/// round-trip cleanly through `serde_json::Value` — `Entity`
/// has no `Serialize` impl in Bevy 0.18, and the
/// `#[serde(skip)]` fields would lose data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructionProjectRecord {
    /// Building type (`BuildingType` enum, persisted as string).
    pub building: String,
    /// Fraction complete (0.0–1.0).
    pub progress: f64,
    /// Total work-points assigned at queue time.
    pub total: f64,
    /// `true` if construction has been paused by the player.
    pub paused: bool,
    /// `true` if the project is waiting for a `ResourceRequest`
    /// to be fulfilled.
    pub awaiting_resources: bool,
}

// ════════════════════════════════════════════════════════════════════
// Surveys
// ════════════════════════════════════════════════════════════════════

/// Per-body survey state. The regen chain seeds each body with
/// a baseline `SurveyState` (Earth = tier-4, others from the
/// real-world record); this override replaces it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurveyDivergence {
    /// Per-dimension tier (0–5). Eight dimensions per body.
    pub dimension_tiers: Vec<(String, u32)>,
    /// Drill missions completed (0 = tier-3 gate locked).
    pub drill_missions_completed: u32,
    /// Anomalies discovered.
    pub anomalies: Vec<String>,
    /// Last-surveyed sim-time.
    pub last_surveyed_sim_seconds: f64,
    /// Full `SurveyState` serialised to JSON. Stored alongside
    /// the tier summary so the apply path can rebuild a fresh
    /// `SurveyState` component without hand-rolling every
    /// field. Optional so older saves (PR-A) without this
    /// field still load.
    #[serde(default)]
    pub state_json: Option<Json>,
}

// ════════════════════════════════════════════════════════════════════
// Fleets and ships
// ════════════════════════════════════════════════════════════════════

/// Stable key for a ship within a fleet. Local to the fleet
/// (not global Entity) — survives regen because the fleet
/// record carries the ship list in order, and each entry's
/// `key` is its position in the list.
pub type ShipKey = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetRecord {
    /// Fleet display name.
    pub name: String,
    /// Body the fleet is currently anchored to (or orbiting).
    pub at_anchor: Option<BodyKey>,
    /// Active transfer (if any).
    pub pending_manoeuvre: Option<TransferRecord>,
    /// Ships in the fleet, in stable order.
    pub ships: Vec<ShipRecord>,
    /// Auto-resolve logistics preference (if any).
    pub logistics_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub destination: BodyKey,
    /// Eta in sim-seconds (matches `ActiveManeuver::arrival_time`).
    pub eta_sim_seconds: f64,
    /// Transfer Δv in m/s.
    pub delta_v_mps: f64,
    /// Burn arcs (`(start_sim_seconds, end_sim_seconds, …)`).
    pub burns: Vec<(f64, f64, f64)>, // (start, end, dv_fraction)
    /// Intermediate gravity-assist bodies.
    pub assists: Vec<BodyKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipRecord {
    /// Stable key (index in the fleet's ship list).
    pub key: ShipKey,
    /// Ship class (`ShipClass` enum, persisted as string).
    pub class: String,
    /// Ship name (if any).
    pub name: Option<String>,
    /// Module loadout. Each entry is a module id (matches the
    /// RON `ship_modules.ron` key) and the slot it occupies.
    pub modules: Vec<(String, String)>, // (module_id, slot_id)
    /// Hull id (matches `ship_hulls.ron` key).
    pub hull: String,
    /// Build date (sim-time) for fleet history.
    pub built_sim_seconds: f64,
    /// Crew / scientists assigned.
    pub assigned_scientists: Vec<String>,
    /// Current fuel fraction (0.0–1.0).
    pub fuel_fraction: f32,
    /// Hull integrity (0.0–1.0).
    pub hull_integrity: f32,
}

// ════════════════════════════════════════════════════════════════════
// Research, economy, UI, notifications, autosave
// ════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResearchRecord {
    /// Unlocked tech ids.
    pub unlocked: Vec<String>,
    /// Active engineering projects.
    pub projects: Vec<EngineeringProjectRecord>,
    /// Per-category RP balance.
    pub rp_balance: BTreeMap<String, f64>,
    /// Per-category EP balance.
    pub ep_balance: BTreeMap<String, f64>,
    /// Research-team capacity (per specialty).
    pub team_capacity: BTreeMap<String, u32>,
    /// Persistent UI state for the tech tree (collapsed
    /// categories, edit dialog open, etc.).
    pub ui_state: Option<ResearchUiRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringProjectRecord {
    pub id: String,
    /// Progress fraction (0.0–1.0).
    pub progress: f64,
    /// Total work-points assigned.
    pub total: f64,
    pub paused: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResearchUiRecord {
    pub selected_category: Option<String>,
    pub edit_dialog_open: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EconomyRecord {
    /// Global budget (treasury, income, expenses).
    pub treasury: f64,
    /// Per-resource rate tracker (production / consumption
    /// in tonnes per second).
    pub rates: BTreeMap<String, (f64, f64)>, // (production, consumption)
    /// Active shipping companies (AI).
    pub shipping_companies: Vec<ShippingCompanyRecord>,
    /// Pending resource requests awaiting delivery.
    pub pending_requests: Vec<ResourceRequestRecord>,
    /// Simulation history snapshots (per body time series).
    pub history: Vec<HistorySnapshotRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingCompanyRecord {
    pub id: u32,
    pub name: String,
    pub fleet_anchor: Option<BodyKey>,
    pub credit_balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequestRecord {
    pub resource: String,
    pub destination: BodyKey,
    pub amount_megatonnes: f64,
    pub priority: String, // Emergency / Construction / Maintenance / Trade
    pub state: String,    // Pending / Assigned / InTransit / Delivered / Expired
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySnapshotRecord {
    pub body: BodyKey,
    pub sim_seconds: f64,
    pub population: f64,
    pub power_output_w: f64,
    pub power_consumption_w: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiRecord {
    /// Current view mode (`SystemView` / `StarmapView`).
    pub view_mode: String,
    /// Currently-selected star system.
    pub current_star_system: u32,
    /// Camera egui panel bounds (for window restoration).
    pub panel_bounds: BTreeMap<String, (f32, f32, f32, f32)>,
    /// Saved survey radius (last camera zoom).
    pub survey_radius_au: f64,
    /// Atmosphere-shells enabled.
    pub atmosphere_enabled: bool,
    /// Atmosphere quality preset.
    pub atmosphere_quality: String,
    /// Time scale.
    pub time_scale: f64,
    /// Paused.
    pub paused: bool,
    /// In-game / main menu / splash state.
    pub launch_state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationRecord {
    /// Per-category settings (enabled, sound, pause-on-event).
    pub category_settings: BTreeMap<String, NotificationCategoryRecord>,
    /// Active toasts (key → toast body).
    pub active_toasts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCategoryRecord {
    pub enabled: bool,
    pub pause_on_event: bool,
    pub sound: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutosaveRecord {
    pub last_autosave_sim_seconds: f64,
    pub last_autosave_wall_unix: i64,
    pub autosave_enabled: bool,
    pub autosave_interval_sim_seconds: f64,
    /// Player-chosen save slot name (the in-game panel writes
    /// this so the autosave can drop a sidecar preview alongside
    /// the actual save).
    pub current_slot_name: String,
}

// ════════════════════════════════════════════════════════════════════
// Mission log (GRA-791)
// ════════════════════════════════════════════════════════════════════

/// Mirror of [`crate::mission_log::MissionLog`]. The live
/// `MissionLog` uses a `VecDeque` for `past` (FIFO-evict at
/// cap), which doesn't serialise cleanly through `ron` without
/// an adapter; the record stores `past` as a plain `Vec` and
/// the apply path rehydrates it into a `VecDeque` in insertion
/// order. Order matters — the consumer's `push_back` /
/// `pop_front` invariant relies on it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionLogRecord {
    /// Active missions (insertion order matches dispatch order).
    pub current: Vec<crate::mission_log::MissionEntry>,
    /// Resolved missions, oldest at the front, newest at the
    /// back. Rehydrated into a `VecDeque` on apply.
    pub past: Vec<crate::mission_log::MissionEntry>,
    /// Declared long-running objectives. Append-only on first
    /// reference; status flips monotonically (Pending →
    /// InProgress → Achieved).
    pub goals: Vec<crate::mission_log::GoalEntry>,
}

// ════════════════════════════════════════════════════════════════════
// Unit tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_key_sol_shortcut() {
        let k = BodyKey::sol("Mercury");
        assert_eq!(k.system, 0);
        assert_eq!(k.name, "Mercury");
    }

    #[test]
    fn body_key_ordering_is_system_then_name() {
        // Sol-bodies come before non-Sol; among Sol, alpha-
        // betical order.
        let a = BodyKey::sol("Earth");
        let b = BodyKey::sol("Mars");
        let c = BodyKey::new(SystemId(5), "X");
        assert!(a < b, "Sol-Earth should come before Sol-Mars");
        assert!(b < c, "Sol-Mars should come before system 5");
    }

    #[test]
    fn empty_store_ron_roundtrip() {
        let store = StateStore::empty(0xC0FFEE);
        let ron = store.to_ron().expect("to_ron");
        // Magic header on the first line.
        assert!(
            ron.starts_with(StateStore::MAGIC),
            "v2 save must start with magic header; got: {ron}"
        );
        let back = StateStore::from_ron(&ron).expect("from_ron");
        assert_eq!(back.metadata.seed, 0xC0FFEE);
        assert_eq!(back.bodies.len(), 0);
        assert_eq!(back.fleets.len(), 0);
    }

    #[test]
    fn from_ron_rejects_v1_format() {
        // A v1 save starts with `(\n    metadata: (\n        format_version: 1`.
        // The loader should refuse to parse it as a StateStore.
        let v1 = "(\n    metadata: (\n        format_version: 1,\n    ),\n)\n";
        let err = StateStore::from_ron(v1).expect_err("v1 must be rejected");
        assert!(matches!(err, StateStoreError::NotV2Format));
    }

    #[test]
    fn metadata_default_uses_current_unix_time() {
        let md = StateStoreMetadata::default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        assert!(
            (now - md.saved_at_unix_s).abs() < 5,
            "saved_at_unix_s should be ~now; got {} vs now {}",
            md.saved_at_unix_s,
            now
        );
    }

    #[test]
    fn body_divergence_with_colony_roundtrips() {
        // The colony_override is a free-form JSON blob (the
        // live `Colony` component serialises into this). The
        // round-trip should preserve the JSON value.
        let mut bodies = BTreeMap::new();
        let colony_json: Json = serde_json::json!({
            "name": "Earth",
            "tier": "Civilisation",
            "population": 8.2e9,
            "buildings": { "Housing": 400 },
        });
        bodies.insert(
            BodyKey::sol("Earth"),
            BodyDivergence {
                colony_override: Some(colony_json),
                ..Default::default()
            },
        );
        let store = StateStore {
            bodies,
            ..StateStore::empty(0)
        };
        let ron = store.to_ron().expect("to_ron");
        let back = StateStore::from_ron(&ron).expect("from_ron");
        let div = back
            .bodies
            .get(&BodyKey::sol("Earth"))
            .expect("Earth divergence must survive roundtrip");
        let colony = div
            .colony_override
            .as_ref()
            .expect("colony_override must survive");
        assert_eq!(colony["name"], "Earth");
        assert_eq!(colony["tier"], "Civilisation");
        assert_eq!(colony["population"], serde_json::json!(8.2e9));
        assert_eq!(colony["buildings"]["Housing"], serde_json::json!(400));
    }

    #[test]
    fn body_divergence_serialize_deserialize_typed() {
        // `Colony` has `#[derive(Serialize, Deserialize)]` so
        // we can round-trip it through `serde_json::Value`
        // without hand-rolling a record struct. This is the
        // whole point of the JSON-blob design.
        use crate::colony::components::{Colony, ColonyDevelopment, ColonyTier};
        use std::collections::HashMap;
        let colony = Colony {
            name: "Earth".to_string(),
            population: 8.2e9,
            development: ColonyDevelopment {
                tier: ColonyTier::Civilisation,
                yield_multiplier: 1.0,
                investments: 0,
            },
            buildings: HashMap::new(),
            growth_rate_modifier: 1.0,
        };
        let json: Json = serde_json::to_value(&colony).expect("Colony must serialise");
        let back: Colony = serde_json::from_value(json.clone()).expect("Colony must deserialise");
        assert_eq!(back.name, "Earth");
        assert_eq!(back.population, 8.2e9);
    }

    #[test]
    fn fleet_record_roundtrip_preserves_ship_loadout() {
        let store = StateStore {
            fleets: vec![FleetRecord {
                name: "Day-One Constellation".to_string(),
                at_anchor: Some(BodyKey::sol("Earth")),
                pending_manoeuvre: None,
                ships: vec![ShipRecord {
                    key: 0,
                    class: "Frigate".to_string(),
                    name: Some("ISS-1".to_string()),
                    modules: vec![
                        ("command".to_string(), "command_module_v1".to_string()),
                        ("engine".to_string(), "ion_drive_v1".to_string()),
                    ],
                    hull: "frigate_v1".to_string(),
                    built_sim_seconds: 0.0,
                    assigned_scientists: vec!["Einstein".to_string()],
                    fuel_fraction: 0.95,
                    hull_integrity: 1.0,
                }],
                logistics_policy: None,
            }],
            ..StateStore::empty(42)
        };
        let ron = store.to_ron().expect("to_ron");
        let back = StateStore::from_ron(&ron).expect("from_ron");
        assert_eq!(back.fleets.len(), 1);
        let fleet = &back.fleets[0];
        assert_eq!(fleet.name, "Day-One Constellation");
        assert_eq!(fleet.at_anchor, Some(BodyKey::sol("Earth")));
        assert_eq!(fleet.ships.len(), 1);
        let ship = &fleet.ships[0];
        assert_eq!(ship.key, 0);
        assert_eq!(ship.class, "Frigate");
        assert_eq!(ship.modules.len(), 2);
        assert_eq!(ship.fuel_fraction, 0.95);
    }

    #[test]
    fn format_version_constant_is_v2() {
        // PR-I pins the current StateStore version. If this
        // changes, also update MIN_SUPPORTED_VERSION and the
        // migration tests.
        assert_eq!(FORMAT_VERSION_V2, 2);
    }
}
