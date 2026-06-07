//! Freighter template registry, RON loader, and cargo capacity query
//! (GRA-40).
//!
//! The RON file at `assets/data/freighter_templates.ron` (sibling of
//! `buildings.ron`, `ship_hulls.ron`, `ship_modules.ron`) defines a list of
//! freighter templates.  Each template binds a base hull to a list of cargo
//! slots; each cargo slot lists its default module plus an ordered upgrade
//! path gated by tech.  The Coder loads the file at startup into a Bevy
//! [`Resource`] and validates it against [`ShipbuildingData`] (hulls and
//! modules must exist, sizes must match, etc.).
//!
//! At query time, cargo capacity for a `(template, slots)` pair is the sum
//! of `cargo_capacity_t` attributes of the modules installed in the
//! template's cargo slots.  See [`freighter_cargo_capacity_t`] and the
//! `Cargo Capacity Matrix` table in
//! `docs/design/SHIP_TEMPLATES.md`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::shipbuilding::{HullSlotDefinition, ShipbuildingData};

use super::components::ShipSlot;
// `FreighterSlots` is re-exported by the parent module if needed; the
// query APIs in this file take `&[ShipSlot]` for testability.

/// Template id used by [`crate::ships::migration::migrate_legacy_freighters`]
/// to map every pre-GRA-40 `ShipClass::Freighter` entity 1:1.  The LGD
/// pinpoints the `light_freighter` template as the migration target (its
/// cargo capacity matches today's `freighter_frame` baseline of 70 t, two
/// `cargo_pod_medium` modules).
pub const LEGACY_MIGRATION_TEMPLATE_ID: &str = "light_freighter";

// ── RON-facing structs (mirror the on-disk shape) ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FreighterTemplatesFile {
    templates: Vec<FreighterTemplateRon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FreighterTemplateRon {
    id: String,
    display_name: String,
    description: String,
    base_hull: String,
    era_tier: u32,
    required_tech: Option<String>,
    cargo_slots: Vec<CargoSlotRon>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CargoSlotRon {
    hull_slot_id: String,
    default_module: String,
    #[serde(default)]
    upgrade_path: Vec<UpgradeStepRon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpgradeStepRon {
    tier: u32,
    module: String,
    required_tech: String,
}

// ── Validated in-memory structs ──────────────────────────────────────────────

/// One cargo slot on a freighter template.
///
/// The slot id is a `HullSlotDefinition.slot_id` on the template's base hull;
/// `default_module` and each `UpgradeStep.module` reference entries in
/// `ShipModuleDefinition.id` (loaded from `ship_modules.ron`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoSlot {
    pub hull_slot_id: String,
    pub default_module: String,
    pub upgrade_path: Vec<UpgradeStep>,
}

/// One tier of a slot's upgrade path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStep {
    pub tier: u32,
    pub module: String,
    pub required_tech: String,
}

/// One validated freighter template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreighterTemplate {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub base_hull: String,
    pub era_tier: u32,
    pub required_tech: Option<String>,
    pub cargo_slots: Vec<CargoSlot>,
    pub tags: Vec<String>,
}

// ── Resource ────────────────────────────────────────────────────────────────

/// Bevy `Resource` holding the validated freighter template set, loaded once
/// at startup from `assets/data/freighter_templates.ron`.
///
/// The registry also exposes a derived `best_buildable` helper used by the
/// GRA-39 auto-construction AI: given the set of researched techs, pick the
/// `(template_id, best_tier)` pair with the highest total cargo capacity
/// (tie-breaks: cheapest `total_build_points`, then template id
/// lexicographic — both fully deterministic and reproducible across runs).
#[derive(Resource, Debug, Clone, Default)]
pub struct FreighterTemplateRegistry {
    by_id: HashMap<String, FreighterTemplate>,
}

impl FreighterTemplateRegistry {
    /// Look up a template by id.  Returns `None` if the id is unknown.
    pub fn get(&self, template_id: &str) -> Option<&FreighterTemplate> {
        self.by_id.get(template_id)
    }

    /// Iterate over all loaded templates in unspecified order.  For
    /// deterministic output, sort by id externally.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &FreighterTemplate)> {
        self.by_id.iter().map(|(id, t)| (id.as_str(), t))
    }

    /// Number of loaded templates.  Used in tests + the migration guard
    /// (`if registry.is_empty() ... bail`).
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Insert a template, replacing any existing entry with the same id.
    /// Used by the loader; tests may also use it to build synthetic
    /// registries without going through the file system.
    pub fn insert(&mut self, template: FreighterTemplate) {
        self.by_id.insert(template.id.clone(), template);
    }

    /// The best `(template_id, tier)` the given researched-tech set can
    /// build.  Returns `None` only if the registry is empty.
    ///
    /// "Best" follows the design doc: highest total cargo capacity across
    /// all buildable `(template, tier)` combinations; ties broken by
    /// cheapest `total_build_points` (sum of `base_build_points` over the
    /// template's slot installed modules at the chosen tier), then by
    /// template id lexicographic.
    ///
    /// The `tech_check` closure abstracts over the research state so the
    /// registry does not have to depend on `ResearchState` directly.  The
    /// closure receives a `&str` tech id and returns whether it has been
    /// researched.
    ///
    /// `shipbuilding_data` is used to look up `cargo_capacity_t` and
    /// `build_points` for each module — the values needed to rank
    /// candidates.  Callers in Bevy systems pass `&Res<ShipbuildingData>`.
    pub fn best_buildable<F>(
        &self,
        shipbuilding_data: &ShipbuildingData,
        tech_check: F,
    ) -> Option<BestBuildableEntry>
    where
        F: Fn(&str) -> bool,
    {
        // Build a parallel list of (entry, template_ref) so we can score
        // each candidate with the template in hand without re-looking-up.
        let mut candidates: Vec<(BestBuildableEntry, &FreighterTemplate)> = Vec::new();

        for template in self.by_id.values() {
            // Template-level gate: required_tech must be researched.
            if let Some(ref tech) = template.required_tech {
                if !tech_check(tech) {
                    continue;
                }
            }

            // Per-slot best tier: highest upgrade_path tier whose
            // required_tech is researched.  Tier 0 is always buildable
            // (no tech gate on default_module).  The template's
            // `best_tier` is the minimum across slots — we don't
            // over-promise: a slot that can't yet upgrade still
            // installs its default.
            let template_best_tier = template
                .cargo_slots
                .iter()
                .map(|slot| {
                    slot.upgrade_path
                        .iter()
                        .filter(|step| tech_check(&step.required_tech))
                        .map(|step| step.tier)
                        .max()
                        .unwrap_or(0)
                })
                .min()
                .unwrap_or(0);

            candidates.push((
                BestBuildableEntry {
                    template_id: template.id.clone(),
                    best_tier: template_best_tier,
                },
                template,
            ));
        }

        if candidates.is_empty() {
            return None;
        }

        // Pick by highest cargo capacity, then cheapest build points, then
        // template id lexicographic.  Cargo and build_points are computed
        // inside the comparator (no extra pass needed).
        candidates.sort_by(|(entry_a, template_a), (entry_b, template_b)| {
            let cargo_a = template_uniform_cargo(shipbuilding_data, template_a, entry_a.best_tier);
            let cargo_b = template_uniform_cargo(shipbuilding_data, template_b, entry_b.best_tier);
            let cost_a = template_uniform_cost(shipbuilding_data, template_a, entry_a.best_tier);
            let cost_b = template_uniform_cost(shipbuilding_data, template_b, entry_b.best_tier);
            cargo_b
                .partial_cmp(&cargo_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    cost_a
                        .partial_cmp(&cost_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| entry_a.template_id.cmp(&entry_b.template_id))
        });

        candidates.into_iter().next().map(|(entry, _)| entry)
    }
}

/// One entry in `FreighterTemplateRegistry::best_buildable`'s candidate set.
/// Returned to the caller so the AI can both build the chosen template and
/// tell the player which tier it was built at.
#[derive(Debug, Clone)]
pub struct BestBuildableEntry {
    pub template_id: String,
    pub best_tier: u32,
}

// ── Loader + validation ─────────────────────────────────────────────────────

/// Load `assets/data/freighter_templates.ron` into the registry resource.
///
/// On a parse or validation error the registry is left empty and the error
/// is logged via Bevy's `error!` macro.  A non-existent file is logged at
/// `warn!` (consistent with the `load_hulls_file` / `load_modules_file`
/// pattern in `shipbuilding::data`).  Both behaviours match the rest of the
/// loader: missing data files do not crash the game.
pub fn load_freighter_templates(mut commands: Commands, shipbuilding_data: Res<ShipbuildingData>) {
    let path = "assets/data/freighter_templates.ron";
    let registry = match load_and_validate(path, &shipbuilding_data) {
        Ok(registry) => registry,
        Err(error) => {
            error!("Failed to load {}: {}", path, error);
            FreighterTemplateRegistry::default()
        }
    };

    info!(
        "Loaded {} freighter templates from {}",
        registry.len(),
        path
    );
    commands.insert_resource(registry);
}

fn load_and_validate(
    path: &str,
    shipbuilding_data: &ShipbuildingData,
) -> Result<FreighterTemplateRegistry, String> {
    let contents =
        fs::read_to_string(Path::new(path)).map_err(|e| format!("read {}: {}", path, e))?;
    let parsed: FreighterTemplatesFile =
        ron::from_str(&contents).map_err(|e| format!("parse {}: {}", path, e))?;

    let mut registry = FreighterTemplateRegistry::default();
    for template in parsed.templates {
        validate_template(&template, shipbuilding_data)?;
        registry.insert(ron_to_template(template));
    }
    Ok(registry)
}

fn validate_template(
    template: &FreighterTemplateRon,
    shipbuilding_data: &ShipbuildingData,
) -> Result<(), String> {
    // Rule 1: base_hull exists.
    let hull = shipbuilding_data
        .get_hull(&template.base_hull)
        .ok_or_else(|| {
            format!(
                "template '{}': base_hull '{}' not found in ship_hulls.ron",
                template.id, template.base_hull
            )
        })?;

    // Rule 6 (relaxed): a freighter template can unlock before its hull
    // can be built as long as both `required_tech` strings are non-empty
    // (i.e. the LGD hasn't accidentally left a field blank).  The
    // intended "hull gates ⊆ template gates" check requires a tech-tree
    // tier map that this loader doesn't have; the original strict
    // string-equality check (see git history) rejected the LGD's
    // standard_freighter design — `orbital_construction` gates the
    // template, `chemical_spaceframes` gates the hull — by mistake.
    // The LGD keeps the "template gates ≥ hull gates" invariant as a
    // data design discipline; the loader only enforces non-empty
    // strings here.
    if let Some(tech) = &hull.required_tech {
        if tech.is_empty() {
            return Err(format!(
                "template '{}': base_hull '{}' has an empty required_tech",
                template.id, template.base_hull
            ));
        }
    }
    if let Some(tech) = &template.required_tech {
        if tech.is_empty() {
            return Err(format!(
                "template '{}': required_tech is an empty string",
                template.id
            ));
        }
    }

    // Rules 2-3 + 5: each cargo slot points at a real hull slot, each
    // module exists in ship_modules.ron and matches the slot's size.
    for slot in &template.cargo_slots {
        let hull_slot = hull
            .slot_layout
            .iter()
            .find(|s| s.slot_id == slot.hull_slot_id);
        let hull_slot: &HullSlotDefinition = hull_slot.ok_or_else(|| {
            format!(
                "template '{}': cargo_slots[].hull_slot_id '{}' not found in \
                 base_hull '{}'",
                template.id, slot.hull_slot_id, template.base_hull
            )
        })?;

        if hull_slot.category != crate::shipbuilding::ShipModuleCategory::CargoStorage {
            return Err(format!(
                "template '{}': cargo_slots[].hull_slot_id '{}' is on a hull \
                 slot of category {:?}, not CargoStorage",
                template.id, slot.hull_slot_id, hull_slot.category
            ));
        }

        validate_module_fits_slot(
            &template.id,
            "default_module",
            &slot.hull_slot_id,
            &slot.default_module,
            hull_slot,
            shipbuilding_data,
        )?;

        // Rule 5: upgrade_path tier numbers are unique and ordered
        // ascending.
        let mut last_tier: u32 = 0;
        let mut first = true;
        for step in &slot.upgrade_path {
            if !first && step.tier <= last_tier {
                return Err(format!(
                    "template '{}': cargo_slots[].upgrade_path[].tier must be \
                     strictly ascending (saw {} after {})",
                    template.id, step.tier, last_tier
                ));
            }
            if first && step.tier <= 1 {
                // tier 1 is the default; we only allow tier >= 2 in the
                // upgrade path (per the design doc — tier 0 is default,
                // tier 1 is reserved/unused in the upgrade path).
                return Err(format!(
                    "template '{}': cargo_slots[].upgrade_path[].tier must be \
                     >= 2 (saw {})",
                    template.id, step.tier
                ));
            }
            last_tier = step.tier;
            first = false;

            validate_module_fits_slot(
                &template.id,
                "upgrade_path[].module",
                &slot.hull_slot_id,
                &step.module,
                hull_slot,
                shipbuilding_data,
            )?;
        }
    }

    // Rule 4: required_tech is a known id.  We only check the template-level
    // and upgrade-path-level techs; the loader does not know about research
    // prerequisites, only that the ids are non-empty strings.  An empty
    // `required_tech` string would be a RON typo.
    if let Some(tech) = &template.required_tech {
        if tech.is_empty() {
            return Err(format!(
                "template '{}': required_tech is an empty string",
                template.id
            ));
        }
    }
    for slot in &template.cargo_slots {
        for step in &slot.upgrade_path {
            if step.required_tech.is_empty() {
                return Err(format!(
                    "template '{}': cargo_slots[].upgrade_path[].required_tech \
                     is an empty string",
                    template.id
                ));
            }
        }
    }

    Ok(())
}

fn validate_module_fits_slot(
    template_id: &str,
    field_name: &str,
    slot_id: &str,
    module_id: &str,
    hull_slot: &HullSlotDefinition,
    shipbuilding_data: &ShipbuildingData,
) -> Result<(), String> {
    let module = shipbuilding_data.get_module(module_id).ok_or_else(|| {
        format!(
            "template '{}': {} '{}' not found in ship_modules.ron",
            template_id, field_name, module_id
        )
    })?;

    if module.category != crate::shipbuilding::ShipModuleCategory::CargoStorage {
        return Err(format!(
            "template '{}': {} '{}' has category {:?}, not CargoStorage",
            template_id, field_name, module_id, module.category
        ));
    }

    if !(module.size == hull_slot.size || hull_slot.size == "Any" || module.size == "Any") {
        return Err(format!(
            "template '{}': {} module '{}' has size '{}' but hull slot '{}' \
             has size '{}'",
            template_id, field_name, module_id, module.size, slot_id, hull_slot.size
        ));
    }

    Ok(())
}

fn ron_to_template(ron: FreighterTemplateRon) -> FreighterTemplate {
    FreighterTemplate {
        id: ron.id,
        display_name: ron.display_name,
        description: ron.description,
        base_hull: ron.base_hull,
        era_tier: ron.era_tier,
        required_tech: ron.required_tech,
        cargo_slots: ron
            .cargo_slots
            .into_iter()
            .map(|s| CargoSlot {
                hull_slot_id: s.hull_slot_id,
                default_module: s.default_module,
                upgrade_path: s
                    .upgrade_path
                    .into_iter()
                    .map(|u| UpgradeStep {
                        tier: u.tier,
                        module: u.module,
                        required_tech: u.required_tech,
                    })
                    .collect(),
            })
            .collect(),
        tags: ron.tags,
    }
}

// ── Cargo capacity query ────────────────────────────────────────────────────

/// Total cargo capacity (tonnes) for a `(template, slots)` pair.
///
/// `slots` should be one `ShipSlot` per cargo slot on the template, with
/// `installed_module` set to a valid module id.  Slots not present in
/// `slots` fall back to the template's `default_module` at tier 0, which
/// matches the design doc's "missing slot uses default" rule.
///
/// Returns `0.0` if the template id is unknown.  Returns the sum of
/// `cargo_capacity_t` attribute values for each installed module.
pub fn freighter_cargo_capacity_t(
    registry: &FreighterTemplateRegistry,
    shipbuilding_data: &ShipbuildingData,
    template_id: &str,
    slots: &[ShipSlot],
) -> f64 {
    let Some(template) = registry.get(template_id) else {
        return 0.0;
    };

    let mut total = 0.0;
    for cargo_slot in &template.cargo_slots {
        let module_id = slots
            .iter()
            .find(|s| s.slot_id == cargo_slot.hull_slot_id)
            .map(|s| s.installed_module.as_str())
            .unwrap_or(cargo_slot.default_module.as_str());
        if let Some(module) = shipbuilding_data.get_module(module_id) {
            for (key, value) in &module.attribute_values {
                if key == "cargo_capacity_t" {
                    total += *value;
                }
            }
        }
    }
    total
}

/// Convenience: total cargo capacity of a freighter entity given its
/// template + per-slot components.  Returns `0.0` if any of the three
/// are missing (unknown template, missing `ShipTemplateRef`, or no
/// `ShipSlot` components).
pub fn freighter_cargo_capacity_t_for_entity(
    registry: &FreighterTemplateRegistry,
    shipbuilding_data: &ShipbuildingData,
    template_ref: &super::components::ShipTemplateRef,
    slots: &[&ShipSlot],
) -> f64 {
    let owned_slots: Vec<ShipSlot> = slots.iter().map(|s| (*s).clone()).collect();
    freighter_cargo_capacity_t(
        registry,
        shipbuilding_data,
        &template_ref.template_id,
        &owned_slots,
    )
}

/// Convenience: total cargo capacity for a freighter that has a
/// `ShipTemplateRef` + `FreighterSlots` component.  Most in-game callers
/// (logistics, fleet panel, AI tie-breaks) hold both at once and want the
/// cargo tonnage without juggling the slot list manually.
pub fn freighter_cargo_capacity_t_for_components(
    registry: &FreighterTemplateRegistry,
    shipbuilding_data: &ShipbuildingData,
    template_ref: &super::components::ShipTemplateRef,
    slots: &super::components::FreighterSlots,
) -> f64 {
    freighter_cargo_capacity_t(
        registry,
        shipbuilding_data,
        &template_ref.template_id,
        &slots.0,
    )
}

// ── Internals used by best_buildable ─────────────────────────────────────────

/// Sum of `cargo_capacity_t` over the template's slots, with each slot
/// upgraded to the same `tier` (0 = default).  This is a coarse
/// "best-current-tier" approximation that the auto-construction AI uses
/// for ranking; the precise per-entity cargo capacity goes through
/// `freighter_cargo_capacity_t` with the entity's `FreighterSlots`.
fn template_uniform_cargo(
    shipbuilding_data: &ShipbuildingData,
    template: &FreighterTemplate,
    tier: u32,
) -> f64 {
    let mut total = 0.0;
    for slot in &template.cargo_slots {
        let module_id = module_id_at_tier(slot, tier);
        if let Some(module) = shipbuilding_data.get_module(module_id) {
            for (key, value) in &module.attribute_values {
                if key == "cargo_capacity_t" {
                    total += value;
                }
            }
        }
    }
    total
}

/// Sum of `effective_module_build_points` over the template's slots, with
/// each slot upgraded to the same `tier`.  Used as the cheapest-build
/// tie-break in `best_buildable`.
fn template_uniform_cost(
    shipbuilding_data: &ShipbuildingData,
    template: &FreighterTemplate,
    tier: u32,
) -> f64 {
    let mut total = 0.0;
    for slot in &template.cargo_slots {
        let module_id = module_id_at_tier(slot, tier);
        if let Some(module) = shipbuilding_data.get_module(module_id) {
            total += shipbuilding_data.effective_module_build_points(module);
        }
    }
    total
}

fn module_id_at_tier(slot: &CargoSlot, tier: u32) -> &str {
    if tier == 0 {
        return slot.default_module.as_str();
    }
    // The RON schema enforces `tier` ascending in upgrade_path; the
    // tier-to-step mapping is step.tier -> step.module.  We pick the step
    // whose tier is the highest <= requested tier (clamp to the last
    // available step if `tier` exceeds the cap).
    let mut chosen: Option<&UpgradeStep> = None;
    for step in &slot.upgrade_path {
        if step.tier <= tier {
            chosen = Some(step);
        } else {
            break;
        }
    }
    chosen
        .map(|s| s.module.as_str())
        .unwrap_or(slot.default_module.as_str())
}

// ── Tests (in-crate; the heavier integration suite lives in
//    tests/freighter_templates_data_tests.rs) ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::ResourceType;
    use crate::shipbuilding::{
        ConstructionMode, HullSlotDefinition, ShipHullDefinition, ShipModuleCategory,
        ShipModuleDefinition, ShipbuildingData,
    };

    fn build_minimal_shipbuilding_data() -> ShipbuildingData {
        let mut data = ShipbuildingData::default();
        data.hulls.insert(
            "freighter_frame".to_string(),
            ShipHullDefinition {
                id: "freighter_frame".to_string(),
                display_name: "Freighter Frame".to_string(),
                description: String::new(),
                class: crate::fleets::ShipClass::Freighter,
                tier: 1,
                base_build_points: 100.0,
                base_dry_mass_t: 100.0,
                default_construction_mode: ConstructionMode::OrbitalAssembly,
                surface_launchable: false,
                orbital_only: false,
                is_station: false,
                size_tier: None,
                required_tech: Some("chemical_spaceframes".to_string()),
                resource_costs: vec![],
                slot_layout: vec![
                    HullSlotDefinition {
                        slot_id: "cargo_a".to_string(),
                        category: ShipModuleCategory::CargoStorage,
                        size: "Medium".to_string(),
                        required: true,
                        position: None,
                        rotation_deg: None,
                    },
                    HullSlotDefinition {
                        slot_id: "cargo_b".to_string(),
                        category: ShipModuleCategory::CargoStorage,
                        size: "Medium".to_string(),
                        required: true,
                        position: None,
                        rotation_deg: None,
                    },
                ],
                tags: vec![],
            },
        );
        let mk_module = |id: &str, size: &str, cargo: f64, cost: f64| ShipModuleDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: String::new(),
            category: ShipModuleCategory::CargoStorage,
            size: size.to_string(),
            tier: 1,
            propulsion: None,
            required_tech: None,
            required_component_design: None,
            power_generation_mw: 0.0,
            power_draw_mw: 0.0,
            thrust_kn: 0.0,
            isp_s: 0.0,
            dry_mass_t: 0.0,
            build_points: cost,
            construction_capacity_bp_per_year: 0.0,
            launch_capacity_t_per_year: 0.0,
            resource_costs: vec![(ResourceType::Iron, 0.0)],
            attribute_values: vec![("cargo_capacity_t".to_string(), cargo)],
            tags: vec![],
        };
        data.modules.insert(
            "cargo_pod_medium".to_string(),
            mk_module("cargo_pod_medium", "Medium", 35.0, 10.0),
        );
        data.modules.insert(
            "cargo_pod_mk2_medium".to_string(),
            mk_module("cargo_pod_mk2_medium", "Medium", 80.0, 20.0),
        );
        data
    }

    fn light_freighter_template() -> FreighterTemplate {
        FreighterTemplate {
            id: "light_freighter".to_string(),
            display_name: "Light Freighter".to_string(),
            description: String::new(),
            base_hull: "freighter_frame".to_string(),
            era_tier: 1,
            required_tech: Some("chemical_spaceframes".to_string()),
            cargo_slots: vec![
                CargoSlot {
                    hull_slot_id: "cargo_a".to_string(),
                    default_module: "cargo_pod_medium".to_string(),
                    upgrade_path: vec![],
                },
                CargoSlot {
                    hull_slot_id: "cargo_b".to_string(),
                    default_module: "cargo_pod_medium".to_string(),
                    upgrade_path: vec![],
                },
            ],
            tags: vec![],
        }
    }

    #[test]
    fn cargo_capacity_for_light_freighter_is_70t_at_default() {
        let data = build_minimal_shipbuilding_data();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(light_freighter_template());
        let slots = vec![
            ShipSlot::new("cargo_a", "cargo_pod_medium", 0),
            ShipSlot::new("cargo_b", "cargo_pod_medium", 0),
        ];
        assert_eq!(
            freighter_cargo_capacity_t(&registry, &data, "light_freighter", &slots),
            70.0
        );
    }

    #[test]
    fn cargo_capacity_uses_default_when_slot_missing() {
        let data = build_minimal_shipbuilding_data();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(light_freighter_template());
        let slots = vec![ShipSlot::new("cargo_a", "cargo_pod_medium", 0)];
        assert_eq!(
            freighter_cargo_capacity_t(&registry, &data, "light_freighter", &slots),
            70.0
        );
    }

    #[test]
    fn best_buildable_with_no_research_returns_none() {
        let data = build_minimal_shipbuilding_data();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(light_freighter_template());
        let entry = registry.best_buildable(&data, |_| false);
        assert!(entry.is_none(), "expected None with no research");
    }

    #[test]
    fn best_buildable_with_chem_frames_research_returns_light() {
        let data = build_minimal_shipbuilding_data();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(light_freighter_template());
        let entry = registry
            .best_buildable(&data, |tech| tech == "chemical_spaceframes")
            .expect("registry has a template");
        assert_eq!(entry.template_id, "light_freighter");
        assert_eq!(entry.best_tier, 0);
    }

    #[test]
    fn best_buildable_with_mk2_and_chem_frames_research_returns_standard_at_tier_0() {
        let data = build_minimal_shipbuilding_data();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(FreighterTemplate {
            id: "standard_freighter".to_string(),
            display_name: "Standard".to_string(),
            description: String::new(),
            base_hull: "freighter_frame".to_string(),
            era_tier: 2,
            required_tech: Some("chemical_spaceframes".to_string()),
            cargo_slots: vec![
                CargoSlot {
                    hull_slot_id: "cargo_a".to_string(),
                    default_module: "cargo_pod_medium".to_string(),
                    upgrade_path: vec![],
                },
                CargoSlot {
                    hull_slot_id: "cargo_b".to_string(),
                    default_module: "cargo_pod_medium".to_string(),
                    upgrade_path: vec![UpgradeStep {
                        tier: 2,
                        module: "cargo_pod_mk2_medium".to_string(),
                        required_tech: "cargo_hold_mk2".to_string(),
                    }],
                },
            ],
            tags: vec![],
        });
        let entry = registry
            .best_buildable(&data, |tech| {
                tech == "cargo_hold_mk2" || tech == "chemical_spaceframes"
            })
            .expect("registry has a template");
        assert_eq!(entry.template_id, "standard_freighter");
        // Slot A caps at tier 0; slot B caps at tier 2 (mk2).  Template's
        // best tier is the minimum across slots, so 0.
        assert_eq!(entry.best_tier, 0);
    }
}
