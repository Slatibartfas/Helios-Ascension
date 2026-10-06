//! Interstellar convoy preset registry and RON loader (GRA-813).
//!
//! Mirrors the `freighter_templates` loader pattern (ships/models.rs):
//! one `Resource` keyed by preset id, populated at startup from
//! `assets/data/interstellar_convoys.ron`, validated against the
//! already-loaded `ShipbuildingData` (hull ids must resolve; required_tech
//! must be a non-empty string).
//!
//! On-disk shape:
//!
//! ```ron
//! (
//!     presets: [
//!         (
//!             id: "ch2_first_convoy",
//!             display_name: "First Convoy (cislunar → Mars)",
//!             description: "...",
//!             era_tier: 1,
//!             required_tech: Some("basic_space_tech"),
//!             min_starbase: false,
//!             hulls: [
//!                 (hull_id: "courier_vessel_frame", count: 1),
//!             ],
//!             tags: ["convoy", "ch2"],
//!         ),
//!     ],
//! )
//! ```
//!
//! A malformed row is logged via `warn!` and the preset is skipped —
//! the app still starts. A missing file is logged at `warn!` and the
//! resource registers as empty (the loader is the runtime binding for
//! `assets/data/interstellar_convoys.ron`, which is LGD scope under
//! GRA-801; until GRA-801 lands on `main` the file may be absent
//! during development).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::shipbuilding::ShipbuildingData;

/// On-disk path for the interstellar convoy preset file. The file is
/// shipped by the LGD under GRA-801; this loader is the CTO-owned
/// runtime binding (`AnomaliesFile`-pattern).
pub const INTERSTELLAR_CONVOYS_RON_PATH: &str = "assets/data/interstellar_convoys.ron";

// ── RON-facing structs (mirror the on-disk shape) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InterstellarConvoysFile {
    presets: Vec<InterstellarConvoyPresetRon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InterstellarConvoyPresetRon {
    id: String,
    display_name: String,
    description: String,
    era_tier: u32,
    #[serde(default)]
    required_tech: Option<String>,
    #[serde(default)]
    min_starbase: bool,
    hulls: Vec<InterstellarConvoyHullSpecRon>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InterstellarConvoyHullSpecRon {
    hull_id: String,
    count: u32,
}

// ── Validated in-memory structs ──────────────────────────────────────────

/// One hull in a convoy preset (e.g. "1 × `mobile_starbase_frame`").
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct InterstellarConvoyHullSpec {
    pub hull_id: String,
    pub count: u32,
}

/// One validated interstellar convoy preset.
///
/// Loaded from `assets/data/interstellar_convoys.ron` via
/// [`load_interstellar_convoys`]. The id is stable and unique within
/// the file; it is the lookup key for [`InterstellarConvoyPresets::get`].
///
/// `min_starbase` is the designer's intent flag (true → preset must
/// include at least one `mobile_starbase_frame`). It is preserved on
/// the in-memory struct for UI surface but is not enforced by the
/// loader — the LGD owns the pairing invariant as a data discipline,
/// not a Rust invariant (same shape as `freighter_templates`'s
/// "template gates ≥ hull gates" relaxation in
/// `ships/models.rs::validate_template`).
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct InterstellarConvoyPreset {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub era_tier: u32,
    pub required_tech: Option<String>,
    pub min_starbase: bool,
    pub hulls: Vec<InterstellarConvoyHullSpec>,
    pub tags: Vec<String>,
}

// ── Resource ─────────────────────────────────────────────────────────────

/// Bevy `Resource` holding the validated interstellar convoy preset
/// set, loaded once at startup from `assets/data/interstellar_convoys.ron`.
///
/// The registry is empty until [`load_interstellar_convoys`] runs.
/// Gameplay consumers (a future "dispatch interstellar convoy" UI,
/// AI scoring, mission-log wiring) look up presets by id via
/// [`InterstellarConvoyPresets::get`].
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct InterstellarConvoyPresets {
    presets: HashMap<String, InterstellarConvoyPreset>,
}

impl InterstellarConvoyPresets {
    /// Look up a preset by id. Returns `None` if the id is unknown.
    pub fn get(&self, preset_id: &str) -> Option<&InterstellarConvoyPreset> {
        self.presets.get(preset_id)
    }

    /// Iterate over every loaded preset in unspecified order. For
    /// deterministic output (UI lists, AI ranking), sort by id externally.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &InterstellarConvoyPreset)> {
        self.presets.iter().map(|(id, p)| (id.as_str(), p))
    }

    /// Number of loaded presets. Tests use this to assert "the
    /// file shipped exactly N rows" without depending on id strings.
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }

    /// Insert a preset, replacing any existing entry with the same
    /// id. Tests use this to build synthetic registries without
    /// going through the file system.
    pub fn insert(&mut self, preset: InterstellarConvoyPreset) {
        self.presets.insert(preset.id.clone(), preset);
    }
}

// ── Loader + validation ───────────────────────────────────────────────────

/// Load `assets/data/interstellar_convoys.ron` into
/// [`InterstellarConvoyPresets`].
///
/// The loader follows the same pattern as
/// `ships/models.rs::load_freighter_templates`:
/// - missing file → log at `warn!` and register an empty resource
///   (the file is a CTO child deliverable; in development the LGD's
///   GRA-801 PR may not yet be on `main`).
/// - parse error → log at `warn!` and register an empty resource.
/// - per-row validation error → log at `warn!`, skip the row,
///   continue with the rest.
///
/// The loader depends on `ShipbuildingData` already being loaded
/// (hull ids are validated against it). The loader is registered on
/// `ShipbuildingPlugin`'s `Startup` tuple-chain (see `src/shipbuilding/mod.rs`),
/// which runs after `load_shipbuilding_data` so the dependency is satisfied.
pub fn load_interstellar_convoys(mut commands: Commands, shipbuilding_data: Res<ShipbuildingData>) {
    let path = INTERSTELLAR_CONVOYS_RON_PATH;
    let registry = match load_and_validate(path, &shipbuilding_data) {
        Ok(registry) => registry,
        Err(error) => {
            warn!(
                "Failed to load {}: {}. The interstellar-convoy registry will start empty; \
                 gameplay consumers that look up a preset will return None until the file is fixed.",
                path, error
            );
            InterstellarConvoyPresets::default()
        }
    };

    info!(
        "Loaded {} interstellar convoy preset(s) from {}",
        registry.len(),
        path
    );
    commands.insert_resource(registry);
}

fn load_and_validate(
    path: &str,
    shipbuilding_data: &ShipbuildingData,
) -> Result<InterstellarConvoyPresets, String> {
    let contents = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path, e))?;
    let parsed: InterstellarConvoysFile =
        ron::from_str(&contents).map_err(|e| format!("parse {}: {}", path, e))?;

    let mut registry = InterstellarConvoyPresets::default();
    let mut seen_ids: HashSet<String> = HashSet::new();
    for preset in parsed.presets {
        // Per-row errors are logged inside `validate_preset` and the
        // preset is skipped; the loader does NOT abort on the first
        // bad row (the issue AC explicitly requires a single
        // malformed row to trip the warning path without blocking
        // every other preset from loading).
        if validate_preset(&preset, shipbuilding_data, path).is_err() {
            continue;
        }
        if !seen_ids.insert(preset.id.clone()) {
            warn!(
                "{}: duplicate preset id {:?}; the second entry will replace the first",
                path, preset.id
            );
        }
        registry.insert(ron_to_preset(preset));
    }
    Ok(registry)
}

fn validate_preset(
    preset: &InterstellarConvoyPresetRon,
    shipbuilding_data: &ShipbuildingData,
    path: &str,
) -> Result<(), String> {
    // Hulls: each entry must resolve against ShipbuildingData, and
    // count must be ≥ 1 (zero-ship presets are a data typo and would
    // produce a convoy with no hulls at dispatch time).
    if preset.hulls.is_empty() {
        warn!(
            "{}: preset {:?} has no hulls; skipping (a convoy must declare at least one hull)",
            path, preset.id
        );
        return Err(format!("preset {:?}: empty hulls list", preset.id));
    }
    for spec in &preset.hulls {
        if spec.count == 0 {
            warn!(
                "{}: preset {:?} hulls[].count must be >= 1 (saw 0 for {:?}); skipping preset",
                path, preset.id, spec.hull_id
            );
            return Err(format!(
                "preset {:?}: hull {:?} has count 0",
                preset.id, spec.hull_id
            ));
        }
        if shipbuilding_data.get_hull(&spec.hull_id).is_none() {
            warn!(
                "{}: preset {:?} hulls[].hull_id {:?} not found in ship_hulls.ron; skipping preset",
                path, preset.id, spec.hull_id
            );
            return Err(format!(
                "preset {:?}: hull {:?} not found in ship_hulls.ron",
                preset.id, spec.hull_id
            ));
        }
    }

    // required_tech: must be present and non-empty if Some. The
    // loader does not validate that the id resolves against
    // `technologies.ron` (same relaxation as
    // `ships/models.rs::validate_template` — the loader does not
    // have a tech-tree dependency; modders can introduce new tech
    // ids without breaking the loader).
    if let Some(tech) = &preset.required_tech {
        if tech.is_empty() {
            warn!(
                "{}: preset {:?} required_tech is an empty string; skipping preset",
                path, preset.id
            );
            return Err(format!(
                "preset {:?}: required_tech is an empty string",
                preset.id
            ));
        }
    }

    // min_starbase: data-only flag, surfaced to UI/AI consumers.
    // When true, the LGD is asserting the preset includes at least
    // one mobile_starbase_frame; we do not enforce that here — the
    // pair is in `design/INTERSTELLAR_CONVOYS.md` as a data
    // discipline, not a Rust invariant.
    if preset.min_starbase
        && !preset
            .hulls
            .iter()
            .any(|h| h.hull_id == "mobile_starbase_frame")
    {
        warn!(
            "{}: preset {:?} sets min_starbase=true but no mobile_starbase_frame entry; \
             the data design contract is broken but the preset is still loaded",
            path, preset.id
        );
    }

    Ok(())
}

fn ron_to_preset(ron: InterstellarConvoyPresetRon) -> InterstellarConvoyPreset {
    InterstellarConvoyPreset {
        id: ron.id,
        display_name: ron.display_name,
        description: ron.description,
        era_tier: ron.era_tier,
        required_tech: ron.required_tech,
        min_starbase: ron.min_starbase,
        hulls: ron
            .hulls
            .into_iter()
            .map(|h| InterstellarConvoyHullSpec {
                hull_id: h.hull_id,
                count: h.count,
            })
            .collect(),
        tags: ron.tags,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_shipbuilding_data() -> ShipbuildingData {
        ShipbuildingData::default()
    }

    fn preset_with_hulls(hull_ids: &[&str], min_starbase: bool) -> InterstellarConvoyPresetRon {
        InterstellarConvoyPresetRon {
            id: "test_preset".to_string(),
            display_name: "Test".to_string(),
            description: String::new(),
            era_tier: 1,
            required_tech: Some("basic_space_tech".to_string()),
            min_starbase,
            hulls: hull_ids
                .iter()
                .map(|h| InterstellarConvoyHullSpecRon {
                    hull_id: (*h).to_string(),
                    count: 1,
                })
                .collect(),
            tags: vec![],
        }
    }

    #[test]
    fn validate_accepts_preset_with_known_hull_id() {
        // Build a ShipbuildingData containing a free hulldown frame.
        let mut shipbuilding_data = empty_shipbuilding_data();
        shipbuilding_data.hulls.insert(
            "freighter_frame".to_string(),
            crate::shipbuilding::ShipHullDefinition {
                id: "freighter_frame".to_string(),
                display_name: "Freighter Frame".to_string(),
                description: String::new(),
                class: crate::fleets::ShipClass::Freighter,
                tier: 1,
                base_build_points: 100.0,
                base_dry_mass_t: 100.0,
                default_construction_mode: crate::shipbuilding::ConstructionMode::OrbitalAssembly,
                surface_launchable: false,
                orbital_only: false,
                is_station: false,
                size_tier: None,
                required_tech: None,
                resource_costs: vec![],
                slot_layout: vec![],
                tags: vec![],
                interstellar_capability: None,
            },
        );
        let preset = preset_with_hulls(&["freighter_frame"], false);
        validate_preset(&preset, &shipbuilding_data, "test.ron")
            .expect("freighter_frame is a known hull");
    }

    #[test]
    fn validate_rejects_unknown_hull_id() {
        let preset = preset_with_hulls(&["not_a_real_hull"], false);
        let err = validate_preset(&preset, &empty_shipbuilding_data(), "test.ron")
            .expect_err("unknown hull_id must fail");
        assert!(err.contains("not_a_real_hull"), "got: {err}");
    }

    #[test]
    fn validate_rejects_empty_hulls_list() {
        let preset = InterstellarConvoyPresetRon {
            id: "empty".to_string(),
            display_name: "Empty".to_string(),
            description: String::new(),
            era_tier: 1,
            required_tech: None,
            min_starbase: false,
            hulls: vec![],
            tags: vec![],
        };
        let err = validate_preset(&preset, &empty_shipbuilding_data(), "test.ron")
            .expect_err("empty hulls list must fail");
        assert!(err.contains("empty hulls"), "got: {err}");
    }

    #[test]
    fn validate_rejects_zero_count_hull() {
        let mut preset = preset_with_hulls(&["freighter_frame"], false);
        preset.hulls[0].count = 0;
        let err = validate_preset(&preset, &empty_shipbuilding_data(), "test.ron")
            .expect_err("count=0 must fail");
        assert!(err.contains("count 0"), "got: {err}");
    }

    #[test]
    fn validate_rejects_empty_required_tech_string() {
        let mut preset = preset_with_hulls(&["freighter_frame"], false);
        preset.required_tech = Some(String::new());
        let err = validate_preset(&preset, &empty_shipbuilding_data(), "test.ron")
            .expect_err("empty required_tech must fail");
        assert!(err.contains("empty string"), "got: {err}");
    }

    #[test]
    fn ron_round_trip_preserves_all_fields() {
        // Serialize a preset to RON, parse it back, and check that
        // every field round-trips.
        let preset = InterstellarConvoyPresetRon {
            id: "round_trip".to_string(),
            display_name: "Round Trip".to_string(),
            description: "desc".to_string(),
            era_tier: 5,
            required_tech: Some("antimatter_propulsion".to_string()),
            min_starbase: true,
            hulls: vec![
                InterstellarConvoyHullSpecRon {
                    hull_id: "freighter_frame".to_string(),
                    count: 2,
                },
                InterstellarConvoyHullSpecRon {
                    hull_id: "courier_vessel_frame".to_string(),
                    count: 1,
                },
            ],
            tags: vec!["ch6".to_string(), "antimatter".to_string()],
        };
        let ron_str = ron::to_string(&preset).expect("serialize");
        let parsed: InterstellarConvoyPresetRon = ron::from_str(&ron_str).expect("parse");
        assert_eq!(parsed.id, preset.id);
        assert_eq!(parsed.display_name, preset.display_name);
        assert_eq!(parsed.description, preset.description);
        assert_eq!(parsed.era_tier, preset.era_tier);
        assert_eq!(parsed.required_tech, preset.required_tech);
        assert_eq!(parsed.min_starbase, preset.min_starbase);
        assert_eq!(parsed.hulls.len(), preset.hulls.len());
        assert_eq!(parsed.hulls[0].hull_id, "freighter_frame");
        assert_eq!(parsed.hulls[0].count, 2);
        assert_eq!(parsed.hulls[1].count, 1);
        assert_eq!(parsed.tags, preset.tags);
    }

    #[test]
    fn registry_insert_get_iter() {
        let mut registry = InterstellarConvoyPresets::default();
        assert!(registry.is_empty());
        registry.insert(InterstellarConvoyPreset {
            id: "alpha".to_string(),
            display_name: "Alpha".to_string(),
            description: String::new(),
            era_tier: 1,
            required_tech: None,
            min_starbase: false,
            hulls: vec![],
            tags: vec![],
        });
        registry.insert(InterstellarConvoyPreset {
            id: "beta".to_string(),
            display_name: "Beta".to_string(),
            description: String::new(),
            era_tier: 2,
            required_tech: None,
            min_starbase: false,
            hulls: vec![],
            tags: vec![],
        });
        assert_eq!(registry.len(), 2);
        assert!(registry.get("alpha").is_some());
        assert!(registry.get("beta").is_some());
        assert!(registry.get("gamma").is_none());
        let ids: Vec<&str> = registry.iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&"alpha"));
        assert!(ids.contains(&"beta"));
    }
}
