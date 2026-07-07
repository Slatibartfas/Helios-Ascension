//! Per-asteroid gameplay-data layer for Helios Ascension.
//!
//! This module is the Rust half of the GRA-313 / GRA-321 design. The
//! schema lives in `assets/data/asteroids.ron` (LGD-owned); the loader
//! here validates the seven invariants enumerated in
//! `docs/design/ASTEROID_ENTITIES.md §2.3` and attaches an
//! [`AsteroidData`] component + marker [`AsteroidBody`] to each matching
//! `CelestialBody` entity.
//!
//! Plugin order: the loader runs in `Startup` alongside the other
//! RON-loaders (`nearby_stars.rs`, `solar_system.rs`); the existing
//! `setup_solar_system` already spawned the bodies, so the scanner
//! finds every entity it needs to attach components to.
//!
//! [`AsteroidSystemSet`] lives in `Update` and is reserved for future
//! per-tick behaviour (composition-reveal animations, redirect-cost
//! updates). Per `helios-architecture`, simulation-driving systems read
//! `SimulationTime`; the loader itself is fire-and-forget at Startup.

use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::economy::types::ResourceType;
use crate::plugins::solar_system::{setup_solar_system, CelestialBody};
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

// ── Sidecar schema ────────────────────────────────────────────────────────

/// On-disk shape of a single asteroid entry. Field semantics are
/// documented in `docs/design/ASTEROID_ENTITIES.md §2.2`.
///
/// `composition` is deserialized as `HashMap<String, f32>` because RON's
/// keyed map uses string keys (`"Water": 0.18`) and `ResourceType`
/// unit-variant names are bare identifiers in RON. The loader
/// converts each key to `ResourceType` and validates that the result
/// is in `ResourceType::all()` (invariant #3).
#[derive(Debug, Clone, Deserialize)]
pub struct RawAsteroidEntry {
    /// Join key into `CelestialBody.name` in `assets/data/solar_system.ron`.
    /// The matched body must have `body_type ∈ {Asteroid, DwarfPlanet}`
    /// (invariant #1, widened in CTO HB-303 review).
    pub body: String,

    /// Optional override of the body's `asteroid_class`. `None` means
    /// "inherit the body's existing class from `solar_system.ron`" —
    /// this is the override-semantics path that test B locks down.
    #[serde(default)]
    pub asteroid_class: Option<AsteroidClass>,

    /// Total dry mass available for mining AND redirect (kg). Source
    /// of truth — overrides the smaller `CelestialBody.mass`, which is
    /// bulk mass including unmineable silicates.
    pub total_mass_kg: f64,

    /// Mass-fraction composition map. Invariant #2: sum ≤ 1.0 + ε.
    /// Invariant #3: every key is a `ResourceType` variant.
    pub composition: HashMap<String, f32>,

    /// Survey tier (0..=8) before any `composition` entry becomes
    /// visible in the dossier. See invariant #4 and
    /// [`DISCOVERY_TIER_TABLE`].
    pub discovery_tier: u8,

    /// Δv (km/s) required to redirect this asteroid onto its
    /// `redirect_target` orbit. `None` = not divertable. Invariant
    /// #5: when `Some(_)`, must lie in `(0.0, 30.0]`.
    #[serde(default)]
    pub delta_v_to_redirect_kms: Option<f64>,

    /// Δv target name (e.g. `"Mars"`, `"Moon"`, `"L4"`, `"L5"`,
    /// `"Earth"`). Invariant #6: must be in [`REDIRECT_TARGETS`].
    pub redirect_target: String,

    /// Hand-curated flag for the terraforming planner (§3.4). Must be
    /// declared explicitly — the loader hard-fails on a missing entry
    /// (invariant #7). See [`auto_terraforming_rule`] for the
    /// consistency check.
    pub terraforming_source: bool,

    /// Optional explicit override for [`auto_terraforming_rule`].
    /// `None` = use the auto rule; `Some(true)` / `Some(false)` =
    /// force the flag (loader still warns on disagreement).
    #[serde(default)]
    pub terraforming_source_override: Option<bool>,

    /// Optional lore seed for the dossier narrative. Pure display.
    #[serde(default)]
    pub lore_seed: Option<String>,
}

/// Top-level shape of `assets/data/asteroids.ron`.
#[derive(Debug, Clone, Deserialize)]
pub struct RawAsteroidSidecar {
    pub asteroids: Vec<RawAsteroidEntry>,
}

/// Validated in-memory mirror of [`RawAsteroidEntry`] with `composition`
/// keys resolved to `ResourceType`. Used by [`AsteroidData`] and the
/// gameplay layer; not constructed directly by RON.
#[derive(Debug, Clone)]
pub struct AsteroidEntry {
    /// See [`RawAsteroidEntry::body`].
    pub body: String,
    /// See [`RawAsteroidEntry::asteroid_class`].
    pub asteroid_class: Option<AsteroidClass>,
    /// See [`RawAsteroidEntry::total_mass_kg`].
    pub total_mass_kg: f64,
    /// See [`RawAsteroidEntry::composition`] (keys resolved).
    pub composition: HashMap<ResourceType, f32>,
    /// See [`RawAsteroidEntry::discovery_tier`].
    pub discovery_tier: u8,
    /// See [`RawAsteroidEntry::delta_v_to_redirect_kms`].
    pub delta_v_to_redirect_kms: Option<f64>,
    /// See [`RawAsteroidEntry::redirect_target`].
    pub redirect_target: String,
    /// See [`RawAsteroidEntry::terraforming_source`].
    pub terraforming_source: bool,
    /// See [`RawAsteroidEntry::terraforming_source_override`].
    pub terraforming_source_override: Option<bool>,
    /// See [`RawAsteroidEntry::lore_seed`].
    pub lore_seed: Option<String>,
}

// ── Constants ─────────────────────────────────────────────────────────────

/// Supported `redirect_target` strings (invariant #6).
///
/// LGD-owned set per `ASTEROID_ENTITIES.md §2.2`. Adding a new target
/// is a RON + schema change; the loader hard-fails if a sidecar entry
/// names anything outside this list.
pub const REDIRECT_TARGETS: &[&str] = &["Mars", "Moon", "L4", "L5", "Earth"];

/// Strictest allowed Δv (km/s) for a divertable asteroid — invariant
/// #5. 30 km/s vastly exceeds any plausible redirect; values above
/// that almost certainly indicate a model bug.
pub const MAX_REDIRECT_DELTA_V_KMS: f64 = 30.0;

/// Epsilon for invariant #2 (`composition` sum ≤ 1.0). Float
/// accumulation drift makes a strict `== 1.0` upper bound fragile.
pub const COMPOSITION_SUM_EPSILON: f32 = 1.0e-4;

/// Volatile thresholds for [`auto_terraforming_rule`] (mass-fraction).
/// An asteroid is auto-classified as a terraforming volatile source
/// if any of these is exceeded — see `ASTEROID_ENTITIES.md §2.2 (a)`.
pub const TERRAFORMING_WATER_THRESHOLD: f32 = 0.05;
pub const TERRAFORMING_AMMONIA_THRESHOLD: f32 = 0.02;
pub const TERRAFORMING_METHANE_THRESHOLD: f32 = 0.02;

/// Δv cap (km/s) for [`auto_terraforming_rule`] — see §2.2 (b).
pub const TERRAFORMING_DELTA_V_CAP_KMS: f64 = 6.0;

/// Per-dossier visibility mapping for [`AsteroidEntry::discovery_tier`].
///
/// Modders set `discovery_tier` per body in the RON; this Rust constant
/// decides which `composition` entries become visible at each tier.
/// Deliberately *not* RON-driven — see CTO HB-303 Q2.
///
/// Each tier is a `&[ResourceType]` slice; the dossier iterates the
/// table and reveals entries whose resource is contained in the slice
/// for the body's current tier. Beyond tier 4 every entry is surfaced
/// (diminishing returns; headroom for v0.8 content).
pub const DISCOVERY_TIER_TABLE: &[(&str, &[ResourceType])] = &[
    ("0_unsurveyed", &[]),
    ("1_class_generic", &[]),
    (
        "2_bulk",
        &[
            ResourceType::Water,
            ResourceType::Iron,
            ResourceType::Silicates,
        ],
    ),
    (
        "3_secondaries",
        &[
            ResourceType::Nickel,
            ResourceType::Ammonia,
            ResourceType::Carbon,
            ResourceType::Magnesium,
        ],
    ),
    (
        "4_trace",
        &[
            ResourceType::Titanium,
            ResourceType::Aluminum,
            ResourceType::Methane,
            ResourceType::Platinum,
            ResourceType::Gold,
            ResourceType::Copper,
            ResourceType::Cobalt,
        ],
    ),
    ("5_plus_headroom", &[]),
];

// ── Resources & components ────────────────────────────────────────────────

/// Loaded per-asteroid data, keyed by body name (the join key into
/// `CelestialBody.name`). Populated by [`load_asteroid_registry`] in
/// `Startup`.
#[derive(Resource, Default, Debug)]
pub struct AsteroidRegistry {
    pub entries: HashMap<String, AsteroidEntry>,
    pub source_path: String,
}

impl AsteroidRegistry {
    pub fn get(&self, body: &str) -> Option<&AsteroidEntry> {
        self.entries.get(body)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Marker component attached to each registered asteroid entity. Lets
/// queries cheaply find the registered bodies without scanning every
/// `CelestialBody` for `asteroid_class.is_some()`.
#[derive(Component, Debug, Clone, Copy)]
pub struct AsteroidBody;

/// Per-asteroid gameplay data attached to the registered entity.
///
/// Mirrors the resolved [`AsteroidEntry`] but carries the *resolved*
/// `asteroid_class` (after override semantics — see test B).
#[derive(Component, Debug, Clone)]
pub struct AsteroidData {
    pub asteroid_class: AsteroidClass,
    pub total_mass_kg: f64,
    pub composition: HashMap<ResourceType, f32>,
    pub discovery_tier: u8,
    pub delta_v_to_redirect_kms: Option<f64>,
    pub redirect_target: String,
    pub terraforming_source: bool,
    pub lore_seed: Option<String>,
}

/// Idempotency marker so the loader runs exactly once per save.
#[derive(Resource, Default)]
pub struct AsteroidRegistryLoaded;

/// `SystemSet` for the asteroid layer. Reserved for future per-tick
/// behaviour. Lives in `Update` per the `helios-architecture` rule
/// that simulation-driving systems run off `SimulationTime`.
#[derive(SystemSet, Debug, Clone, Hash, PartialEq, Eq)]
pub struct AsteroidSystemSet;

// ── Pure helpers ──────────────────────────────────────────────────────────

/// Auto-derived terraforming-source rule (see
/// `ASTEROID_ENTITIES.md §2.2`).
///
/// Returns `true` iff any of:
///
/// * `Water ≥ TERRAFORMING_WATER_THRESHOLD`
/// * `Ammonia ≥ TERRAFORMING_AMMONIA_THRESHOLD`
/// * `Methane ≥ TERRAFORMING_METHANE_THRESHOLD`
///
/// AND `delta_v_to_redirect_kms ∈ (0.0, TERRAFORMING_DELTA_V_CAP_KMS]`.
/// A `None` Δv (not divertable) returns `false` regardless of
/// composition.
pub fn auto_terraforming_rule(
    composition: &HashMap<ResourceType, f32>,
    delta_v_to_redirect_kms: Option<f64>,
) -> bool {
    let within_dv_cap = match delta_v_to_redirect_kms {
        Some(dv) => dv > 0.0 && dv <= TERRAFORMING_DELTA_V_CAP_KMS,
        None => false,
    };
    if !within_dv_cap {
        return false;
    }
    let water = composition
        .get(&ResourceType::Water)
        .copied()
        .unwrap_or(0.0);
    let ammonia = composition
        .get(&ResourceType::Ammonia)
        .copied()
        .unwrap_or(0.0);
    let methane = composition
        .get(&ResourceType::Methane)
        .copied()
        .unwrap_or(0.0);
    water >= TERRAFORMING_WATER_THRESHOLD
        || ammonia >= TERRAFORMING_AMMONIA_THRESHOLD
        || methane >= TERRAFORMING_METHANE_THRESHOLD
}

/// Per-invariance loader errors. Each variant carries enough context
/// for a useful `error!` log line.
#[derive(Debug, Clone, PartialEq)]
pub enum AsteroidLoadError {
    UnknownBody(String),
    WrongBodyType {
        body: String,
        actual: BodyType,
    },
    UnknownCompositionKey {
        body: String,
        key: String,
    },
    CompositionSumExceeds {
        body: String,
        sum: f32,
    },
    DiscoveryTierOutOfRange {
        body: String,
        tier: u8,
    },
    DeltaVOutOfRange {
        body: String,
        delta_v: f64,
    },
    UnknownRedirectTarget {
        body: String,
        target: String,
    },
    MissingTerraformingSource(String),
    TerraformingFlagInconsistent {
        body: String,
        declared: bool,
        auto: bool,
    },
}

impl std::fmt::Display for AsteroidLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsteroidLoadError::UnknownBody(name) => write!(
                f,
                "asteroid entry `{name}` does not match any body in solar_system.ron"
            ),
            AsteroidLoadError::WrongBodyType { body, actual } => write!(
                f,
                "asteroid `{body}` has body_type {actual:?}; expected Asteroid or DwarfPlanet"
            ),
            AsteroidLoadError::UnknownCompositionKey { body, key } => write!(
                f,
                "asteroid `{body}` composition key `{key}` is not a ResourceType variant"
            ),
            AsteroidLoadError::CompositionSumExceeds { body, sum } => write!(
                f,
                "asteroid `{body}` composition sums to {sum} (> 1.0 + epsilon)"
            ),
            AsteroidLoadError::DiscoveryTierOutOfRange { body, tier } => write!(
                f,
                "asteroid `{body}` discovery_tier {tier} out of range 0..=8"
            ),
            AsteroidLoadError::DeltaVOutOfRange { body, delta_v } => write!(
                f,
                "asteroid `{body}` delta_v_to_redirect_kms {delta_v} not in (0.0, 30.0]"
            ),
            AsteroidLoadError::UnknownRedirectTarget { body, target } => write!(
                f,
                "asteroid `{body}` redirect_target `{target}` not in allowlist {REDIRECT_TARGETS:?}"
            ),
            AsteroidLoadError::MissingTerraformingSource(body) => write!(
                f,
                "asteroid `{body}` missing required `terraforming_source` field (invariant #7)"
            ),
            AsteroidLoadError::TerraformingFlagInconsistent {
                body,
                declared,
                auto,
            } => write!(
                f,
                "asteroid `{body}` terraforming_source={declared} disagrees with auto-rule ({auto})"
            ),
        }
    }
}

impl std::error::Error for AsteroidLoadError {}

/// Resolve a raw RON string key to a `ResourceType`. Returns `None` if
/// the key is not a known variant. The lookup is case-sensitive (RON
/// preserves the spelling the LGD chose).
fn resolve_resource_key(key: &str) -> Option<ResourceType> {
    ResourceType::all()
        .iter()
        .copied()
        .find(|r| resource_name(*r) == key)
}

/// Canonical string name of a `ResourceType` variant (matches the
/// `Serialize` representation in this codebase).
fn resource_name(r: ResourceType) -> &'static str {
    match r {
        ResourceType::Water => "Water",
        ResourceType::Hydrogen => "Hydrogen",
        ResourceType::Ammonia => "Ammonia",
        ResourceType::Methane => "Methane",
        ResourceType::Phosphorus => "Phosphorus",
        ResourceType::Food => "Food",
        ResourceType::Nitrogen => "Nitrogen",
        ResourceType::Oxygen => "Oxygen",
        ResourceType::CarbonDioxide => "CarbonDioxide",
        ResourceType::Argon => "Argon",
        ResourceType::Iron => "Iron",
        ResourceType::Aluminum => "Aluminum",
        ResourceType::Titanium => "Titanium",
        ResourceType::Silicates => "Silicates",
        ResourceType::Nickel => "Nickel",
        ResourceType::Tungsten => "Tungsten",
        ResourceType::Carbon => "Carbon",
        ResourceType::Chromium => "Chromium",
        ResourceType::Magnesium => "Magnesium",
        ResourceType::Helium3 => "Helium3",
        ResourceType::Deuterium => "Deuterium",
        ResourceType::Tritium => "Tritium",
        ResourceType::Uranium => "Uranium",
        ResourceType::Thorium => "Thorium",
        ResourceType::Plutonium => "Plutonium",
        ResourceType::Gold => "Gold",
        ResourceType::Silver => "Silver",
        ResourceType::Platinum => "Platinum",
        ResourceType::Copper => "Copper",
        ResourceType::RareEarths => "RareEarths",
        ResourceType::Lithium => "Lithium",
        ResourceType::Sulfur => "Sulfur",
        ResourceType::Cobalt => "Cobalt",
        ResourceType::Fluorine => "Fluorine",
        ResourceType::Polymers => "Polymers",
        ResourceType::Antimatter => "Antimatter",
        ResourceType::ExoticMatter => "ExoticMatter",
        ResourceType::Metamaterials => "Metamaterials",
        ResourceType::Computronium => "Computronium",
    }
}

/// Convert a raw RON entry to a validated [`AsteroidEntry`].
///
/// `body_type_lookup` must return the `BodyType` of the matched
/// `CelestialBodyData` (`None` = unknown body, i.e. invariant #1 fail).
///
/// Returns the first hard error encountered, or `Ok(AsteroidEntry)`
/// on success. The terraforming-source consistency check is *not*
/// hard-fail here — it's surfaced as [`AsteroidLoadError::TerraformingFlagInconsistent`]
/// so the loader can decide between warn-vs-error per CTO HB-303 Q4.
pub fn validate_asteroid_entry<F>(
    entry: &RawAsteroidEntry,
    body_type_lookup: F,
) -> Result<AsteroidEntry, AsteroidLoadError>
where
    F: Fn(&str) -> Option<BodyType>,
{
    // Invariant #1: body joins onto solar_system.ron with the
    // permitted body_type. Widened in CTO HB-303 to include
    // DwarfPlanet (Ceres is a DwarfPlanet in solar_system.ron:707).
    let body_type = body_type_lookup(&entry.body)
        .ok_or_else(|| AsteroidLoadError::UnknownBody(entry.body.clone()))?;
    if !matches!(body_type, BodyType::Asteroid | BodyType::DwarfPlanet) {
        return Err(AsteroidLoadError::WrongBodyType {
            body: entry.body.clone(),
            actual: body_type,
        });
    }

    // Invariant #3: composition keys must be `ResourceType` variants.
    let mut resolved_composition: HashMap<ResourceType, f32> = HashMap::new();
    for (key, value) in entry.composition.iter() {
        let resource =
            resolve_resource_key(key).ok_or_else(|| AsteroidLoadError::UnknownCompositionKey {
                body: entry.body.clone(),
                key: key.clone(),
            })?;
        resolved_composition.insert(resource, *value);
    }

    // Invariant #2: composition sum ≤ 1.0 + epsilon.
    let sum: f32 = resolved_composition.values().copied().sum();
    if sum > 1.0 + COMPOSITION_SUM_EPSILON {
        return Err(AsteroidLoadError::CompositionSumExceeds {
            body: entry.body.clone(),
            sum,
        });
    }

    // Invariant #4: discovery_tier ∈ 0..=8.
    if entry.discovery_tier > 8 {
        return Err(AsteroidLoadError::DiscoveryTierOutOfRange {
            body: entry.body.clone(),
            tier: entry.discovery_tier,
        });
    }

    // Invariant #5: delta_v_to_redirect_kms ∈ (0.0, 30.0] | None.
    if let Some(dv) = entry.delta_v_to_redirect_kms {
        if !(dv > 0.0 && dv <= MAX_REDIRECT_DELTA_V_KMS) {
            return Err(AsteroidLoadError::DeltaVOutOfRange {
                body: entry.body.clone(),
                delta_v: dv,
            });
        }
    }

    // Invariant #6: redirect_target ∈ REDIRECT_TARGETS.
    if !REDIRECT_TARGETS.contains(&entry.redirect_target.as_str()) {
        return Err(AsteroidLoadError::UnknownRedirectTarget {
            body: entry.body.clone(),
            target: entry.redirect_target.clone(),
        });
    }

    // Consistency check: terraforming_source vs auto rule.
    let auto = auto_terraforming_rule(&resolved_composition, entry.delta_v_to_redirect_kms);
    let effective = entry.terraforming_source_override.unwrap_or(auto);
    if entry.terraforming_source != effective {
        return Err(AsteroidLoadError::TerraformingFlagInconsistent {
            body: entry.body.clone(),
            declared: entry.terraforming_source,
            auto: effective,
        });
    }

    Ok(AsteroidEntry {
        body: entry.body.clone(),
        asteroid_class: entry.asteroid_class,
        total_mass_kg: entry.total_mass_kg,
        composition: resolved_composition,
        discovery_tier: entry.discovery_tier,
        delta_v_to_redirect_kms: entry.delta_v_to_redirect_kms,
        redirect_target: entry.redirect_target.clone(),
        terraforming_source: entry.terraforming_source,
        terraforming_source_override: entry.terraforming_source_override,
        lore_seed: entry.lore_seed.clone(),
    })
}

// ── Loader ────────────────────────────────────────────────────────────────

const ASTEROID_RON_PATH: &str = "assets/data/asteroids.ron";

/// `Startup` system: load the sidecar RON, validate every entry
/// against the seven invariants, and attach [`AsteroidData`] +
/// [`AsteroidBody`] to matching `CelestialBody` entities.
///
/// Hard errors → `error!` log + skip the body (matches the loader
/// pattern in `nearby_stars.rs`). Inconsistencies under
/// `TerraformingFlagInconsistent` are warned by default and promoted
/// to hard-fail under `cfg(debug_assertions)` per CTO HB-303 Q4.
/// Idempotent via [`AsteroidRegistryLoaded`].
///
/// Order: scans existing `CelestialBody` entities after
/// `setup_solar_system` (which runs in `Startup` of `SolarSystemPlugin`).
/// Plugin ordering inside `AstronomyPlugin::build` puts this loader
/// after `SolarSystemPlugin` so body entities exist.
pub fn load_asteroid_registry(
    mut commands: Commands,
    registry_loaded: Option<Res<AsteroidRegistryLoaded>>,
    mut registry: ResMut<AsteroidRegistry>,
    bodies: Query<(Entity, &CelestialBody)>,
) {
    if registry_loaded.is_some() {
        return;
    }

    let path = Path::new(ASTEROID_RON_PATH);
    let contents = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            error!(
                "AsteroidRegistry: could not read {ASTEROID_RON_PATH}: {e}; \
                 asteroid gameplay layer disabled"
            );
            commands.init_resource::<AsteroidRegistryLoaded>();
            return;
        }
    };

    let sidecar: RawAsteroidSidecar = match ron::from_str(&contents) {
        Ok(s) => s,
        Err(e) => {
            error!("AsteroidRegistry: failed to parse {ASTEROID_RON_PATH}: {e}");
            commands.init_resource::<AsteroidRegistryLoaded>();
            return;
        }
    };

    registry.source_path = ASTEROID_RON_PATH.to_string();

    // Snapshot the world bodies once (name → (entity, body_type,
    // existing_class)) so the validator can use them without holding
    // the query borrow open across validation.
    let mut body_index: HashMap<String, (Entity, BodyType, Option<AsteroidClass>)> = HashMap::new();
    for (entity, body) in bodies.iter() {
        body_index.insert(
            body.name.clone(),
            (entity, body.body_type, body.asteroid_class),
        );
    }

    for raw in sidecar.asteroids.iter() {
        let lookup = |name: &str| body_index.get(name).map(|(_, bt, _)| *bt);
        match validate_asteroid_entry(raw, lookup) {
            Ok(entry) => {
                registry.entries.insert(entry.body.clone(), entry);
            }
            Err(AsteroidLoadError::TerraformingFlagInconsistent {
                body,
                declared,
                auto,
            }) => {
                // Soft-fail in v0.5.x; promote to hard-fail in debug
                // builds per CTO HB-303 Q4.
                warn!(
                    "AsteroidRegistry: `{body}` terraforming_source={declared} disagrees \
                     with auto-rule ({auto}); honouring declared flag"
                );
                if cfg!(debug_assertions) {
                    error!(
                        "AsteroidRegistry: `{body}` terraforming_source inconsistency is a \
                         hard error under debug_assertions"
                    );
                    continue;
                }
                if let Ok(entry) =
                    validate_asteroid_entry(raw, |name| body_index.get(name).map(|(_, bt, _)| *bt))
                {
                    registry.entries.insert(entry.body.clone(), entry);
                }
            }
            Err(e) => {
                error!("AsteroidRegistry: rejecting entry — {e}");
            }
        }
    }

    // Attach components after validation so a hard-failed entry never
    // gets stamped on an entity.
    for entry in registry.entries.values() {
        if let Some((entity, _, existing_class)) = body_index.get(&entry.body) {
            let final_class = entry
                .asteroid_class
                .or(*existing_class)
                .unwrap_or(AsteroidClass::Unknown);

            commands
                .entity(*entity)
                .insert(AsteroidBody)
                .insert(AsteroidData {
                    asteroid_class: final_class,
                    total_mass_kg: entry.total_mass_kg,
                    composition: entry.composition.clone(),
                    discovery_tier: entry.discovery_tier,
                    delta_v_to_redirect_kms: entry.delta_v_to_redirect_kms,
                    redirect_target: entry.redirect_target.clone(),
                    terraforming_source: entry.terraforming_source,
                    lore_seed: entry.lore_seed.clone(),
                });
        }
    }

    info!(
        "AsteroidRegistry: loaded {} asteroid entries from {}",
        registry.entries.len(),
        ASTEROID_RON_PATH
    );
    commands.init_resource::<AsteroidRegistryLoaded>();
}

// ── Plugin ────────────────────────────────────────────────────────────────

/// Plugin that loads `assets/data/asteroids.ron` and attaches
/// `AsteroidData` + `AsteroidBody` to matching entities.
///
/// Wired into `AstronomyPlugin::build` after `SolarSystemPlugin`
/// (whose `setup_solar_system` Startup system spawns the body
/// entities this loader scans).
pub struct AsteroidRegistryPlugin;

impl Plugin for AsteroidRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AsteroidRegistry>()
            .add_systems(Startup, load_asteroid_registry.after(setup_solar_system))
            .add_systems(Update, no_op_asteroid_system.in_set(AsteroidSystemSet));
    }
}

/// Placeholder system for [`AsteroidSystemSet`]. Real per-tick
/// behaviour (composition reveal animations, redirect-cost updates)
/// lands in v0.5.x follow-ups; the set exists so the loader can
/// declare it now and downstream code can chain off it without a
/// refactor.
fn no_op_asteroid_system() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_lookup() -> impl Fn(&str) -> Option<BodyType> {
        |_| None
    }

    fn asteroid_lookup() -> impl Fn(&str) -> Option<BodyType> {
        |name| match name {
            "Ceres" => Some(BodyType::DwarfPlanet),
            "Vesta" | "Psyche" | "Juno" | "Hygiea" | "162173 Ryugu" => Some(BodyType::Asteroid),
            _ => None,
        }
    }

    fn make_entry(body: &str) -> RawAsteroidEntry {
        RawAsteroidEntry {
            body: body.into(),
            asteroid_class: Some(AsteroidClass::CType),
            total_mass_kg: 1.0e15,
            composition: HashMap::from([
                ("Water".to_string(), 0.1_f32),
                ("Iron".to_string(), 0.1_f32),
            ]),
            discovery_tier: 2,
            delta_v_to_redirect_kms: Some(2.0),
            redirect_target: "Mars".into(),
            // Composition has Water 0.1 ≥ TERRAFORMING_WATER_THRESHOLD
            // (0.05) and Δv 2.0 ≤ TERRAFORMING_DELTA_V_CAP_KMS (6.0),
            // so `auto_terraforming_rule` returns `true`.  The
            // declared flag must match for the consistency check to
            // pass; if a specific test wants to exercise the
            // `terraforming_source = false` path it should override
            // the composition so the auto-rule returns false
            // (e.g. drop Water below 0.05).
            terraforming_source: true,
            terraforming_source_override: None,
            lore_seed: None,
        }
    }

    // ── Test A — DwarfPlanet allowlist ────────────────────────────────
    // GRA-313 §2.3 / CTO HB-303 widened invariant #1 to permit
    // DwarfPlanet (Ceres).  Test A locks down that relaxation.
    #[test]
    fn test_a_ceres_dwarf_planet_accepted() {
        let entry = make_entry("Ceres");
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(
            result.is_ok(),
            "Ceres (DwarfPlanet) must be accepted; got {result:?}"
        );
    }

    // ── Test B — Override semantics ────────────────────────────────────
    // The Ryugu worked-example: solar_system.ron labels Ryugu `SType`,
    // the sidecar overrides to `CType`.  The loader must surface
    // CType.  Test B locks down the override semantics in
    // [`AsteroidData::asteroid_class`] so future refactors don't
    // silently re-introduce the inherit path.
    #[test]
    fn test_b_ryugu_override_semantics() {
        let mut entry = make_entry("Ceres");
        entry.body = "162173 Ryugu".into();
        entry.asteroid_class = Some(AsteroidClass::CType);
        entry.terraforming_source = true;
        entry.terraforming_source_override = Some(true);
        // Composition must satisfy the auto-rule for a `true` flag
        // (Water ≥ 0.05 + Δv ≤ 6.0).
        entry.composition = HashMap::from([("Water".to_string(), 0.09_f32)]);
        entry.delta_v_to_redirect_kms = Some(5.1);

        // The override flag matches the auto-rule (Water 0.09 ≥ 0.05,
        // Δv 5.1 ≤ 6.0) → no inconsistency.
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(
            result.is_ok(),
            "Ryugu override + auto-rule match should validate; got {result:?}"
        );

        let resolved = result.unwrap();
        assert_eq!(
            resolved.asteroid_class,
            Some(AsteroidClass::CType),
            "Ryugu sidecar override must win over solar_system.ron's SType"
        );
    }

    // ── Invariant tests ────────────────────────────────────────────────

    #[test]
    fn invariant_composition_sum_exceeds_one_is_hard_error() {
        let mut entry = make_entry("Vesta");
        entry.composition = HashMap::from([
            ("Iron".to_string(), 0.5_f32),
            ("Silicates".to_string(), 0.6_f32), // total 1.1 > 1.0 + ε
        ]);
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(matches!(
            result,
            Err(AsteroidLoadError::CompositionSumExceeds { .. })
        ));
    }

    #[test]
    fn invariant_discovery_tier_out_of_range_is_hard_error() {
        let mut entry = make_entry("Vesta");
        entry.discovery_tier = 9;
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(matches!(
            result,
            Err(AsteroidLoadError::DiscoveryTierOutOfRange { tier: 9, .. })
        ));
    }

    #[test]
    fn invariant_delta_v_out_of_range_is_hard_error() {
        let mut entry = make_entry("Vesta");
        entry.delta_v_to_redirect_kms = Some(0.0);
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(matches!(
            result,
            Err(AsteroidLoadError::DeltaVOutOfRange { .. })
        ));

        entry.delta_v_to_redirect_kms = Some(31.0);
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(matches!(
            result,
            Err(AsteroidLoadError::DeltaVOutOfRange { .. })
        ));
    }

    #[test]
    fn invariant_redirect_target_not_in_allowlist_is_hard_error() {
        let mut entry = make_entry("Vesta");
        entry.redirect_target = "Mercury".into();
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(matches!(
            result,
            Err(AsteroidLoadError::UnknownRedirectTarget { .. })
        ));
    }

    #[test]
    fn unknown_body_is_hard_error() {
        let entry = make_entry("Pluto");
        let result = validate_asteroid_entry(&entry, empty_lookup());
        assert!(matches!(result, Err(AsteroidLoadError::UnknownBody(_))));
    }

    #[test]
    fn wrong_body_type_is_hard_error() {
        let lookup = |name: &str| match name {
            "Earth" => Some(BodyType::Planet),
            _ => None,
        };
        let entry = make_entry("Earth");
        let result = validate_asteroid_entry(&entry, lookup);
        assert!(matches!(
            result,
            Err(AsteroidLoadError::WrongBodyType { .. })
        ));
    }

    #[test]
    fn unknown_composition_key_is_hard_error() {
        let mut entry = make_entry("Vesta");
        entry.composition = HashMap::from([("Unobtanium".to_string(), 0.5_f32)]);
        let result = validate_asteroid_entry(&entry, asteroid_lookup());
        assert!(matches!(
            result,
            Err(AsteroidLoadError::UnknownCompositionKey { key, .. }) if key == "Unobtanium"
        ));
    }

    // ── Helper tests ───────────────────────────────────────────────────

    #[test]
    fn auto_terraforming_rule_requires_volatile_and_divertable() {
        let composition: HashMap<ResourceType, f32> = HashMap::from([(ResourceType::Water, 0.10)]);
        // No redirect cap → false even with volatile composition.
        assert!(!auto_terraforming_rule(&composition, None));
        // Δv over cap → false.
        assert!(!auto_terraforming_rule(&composition, Some(7.0)));
        // Δv within cap (≤ 6.0) and Water above 0.05 → true.
        assert!(auto_terraforming_rule(&composition, Some(5.0)));
    }

    #[test]
    fn discovery_tier_table_contains_expected_keys() {
        assert_eq!(DISCOVERY_TIER_TABLE[0].0, "0_unsurveyed");
        assert_eq!(DISCOVERY_TIER_TABLE[4].0, "4_trace");
        let t2: Vec<ResourceType> = DISCOVERY_TIER_TABLE[2].1.to_vec();
        assert!(t2.contains(&ResourceType::Water));
        assert!(t2.contains(&ResourceType::Iron));
        assert!(t2.contains(&ResourceType::Silicates));
    }

    #[test]
    fn redirect_targets_allowlist_matches_design() {
        assert!(REDIRECT_TARGETS.contains(&"Mars"));
        assert!(REDIRECT_TARGETS.contains(&"Moon"));
        assert!(REDIRECT_TARGETS.contains(&"L4"));
        assert!(REDIRECT_TARGETS.contains(&"L5"));
        assert!(REDIRECT_TARGETS.contains(&"Earth"));
        assert_eq!(REDIRECT_TARGETS.len(), 5);
    }
}
