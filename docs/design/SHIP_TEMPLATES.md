# Ship Templates & Tech Upgrades

Design specification for the freighter template system (v0.4.1).

Inspired by **Aurora 4X** (same base ship, better cargo per tech level) and **Distant Worlds 2** (AI picks the most efficient design the home shipyard can build).

Closes GRA-40 (`GRA-37.c — Ship templates + tech upgrades`).

---

## Table of Contents

1. [Design Goals](#design-goals)
2. [Current State](#current-state)
3. [Template Model](#template-model)
4. [Tech Upgrade Matrix](#tech-upgrade-matrix)
5. [RON Schema](#ron-schema)
6. [Default Templates](#default-templates)
7. [Cargo Capacity Matrix](#cargo-capacity-matrix)
8. [AI Build Policy](#ai-build-policy)
9. [Migration Plan](#migration-plan)
10. [Modder Surface](#modder-surface)
11. [Rust Deltas (Hand-off to CTO)](#rust-deltas-hand-off-to-cto)
12. [Test Plan](#test-plan)
13. [Out of Scope](#out-of-scope)

---

## Design Goals

| Goal | Description |
|------|-------------|
| **Realism** | Cargo capacity is physical volume bounded by hull geometry. Tech unlocks better use of that volume, not a magic +50% button. |
| **Player arc** | The freighter research path is a real investment: light → standard → heavy, with Mk2/Mk3 slot upgrades layered on top. |
| **Modder-first** | Adding a new freighter template is a one-line RON entry. No Rust recompile. |
| **AI determinism** | When the GRA-39 auto-construction loop picks what to build, the rule is `cargo_per_build_cost` of the *highest-tier currently buildable* template. No hidden state, no emergent surprises. |
| **Backward compat** | Existing saves load without error. `ShipClass::Freighter` entities get a 1:1 migration to the light freighter template at load. |

---

## Current State

`ShipClass::Freighter` is a flat enum variant on `src/fleets/types.rs:63`. All freighter entities in the world carry this enum tag; the value is identical for every freighter the player owns, and the AI in `src/economy/company.rs` builds the same freighter every time.

There is no per-ship cargo capacity beyond the hull's own slot_layout. The hull's `slot_layout` defines the *shape* of cargo slots but not the *content* — content comes from whatever ship module the player fits in each slot at build time, and the current UI ships a single `cargo_pod_medium` / `cargo_bay_medium` / `cargo_armored_medium` triad with `cargo_capacity_t = 35 / 48 / 28` respectively.

No tech in `assets/data/technologies.ron` gates cargo slot upgrades. The freighter research arc does not exist.

---

## Template Model

A **freighter template** is a named, RON-defined bundle that ties together:

1. **Base hull** — an existing `ship_hulls.ron` entry, referenced by `id` (e.g. `freighter_frame`, `bulk_cargo_frame`).
2. **Cargo slot map** — which hull slot_ids the template uses for cargo, and what module is installed in each at each upgrade tier.
3. **Upgrade path** — for each cargo slot, the ordered list of `(tier, module_id, required_tech)` tuples that mark which module may be installed at which tier.

A **slot upgrade** is a transition from one installed module to the next in the slot's upgrade path. The transition is gated by the `required_tech` of the destination tier.

Cargo capacity for a `(template, installed_modules)` configuration is computed at query time as the sum of `cargo_capacity_t` over all installed modules in the template's cargo slots, plus any cargo capacity from non-cargo hull slots that the player has chosen to fill with cargo modules (out of scope for GRA-40; the existing hull slots handle their own modules).

```
cargo_capacity_t(template, installed) =
    Σ over cargo_slots(template) of cargo_capacity_t(installed[slot_id])
```

This is a one-pass O(slots) sum. Cache invalidation is simple: the cache is per `(Entity, SimVersion)`. The Coder is free to choose the cache shape; the LGD rule is "sum installed module attributes, no magic numbers".

---

## Tech Upgrade Matrix

This design **reuses the existing `cargo_hold_mk2` and `cargo_hold_mk3` techs from `assets/data/technologies.ron`**, both already in `main` from GRA-10 (PR #80). No new tech entries are added by GRA-40; no `technologies.ron` RON pass is required.

| Tech id | Tier | Prereqs | `unlocks_engineering` (per `technologies.ron`) | Slot upgrade unlocked |
|---------|------|---------|------------------------------------------------|----------------------|
| `cargo_hold_mk2` | 2 | `in_situ_resource`, `orbital_construction` | `cargo_pod_mk2_medium`, `cargo_bay_mk2_large`, `cargo_armored_mk2_medium` | Tier 2 on standard and heavy templates. |
| `cargo_hold_mk3` | 3 | `cargo_hold_mk2`, `carbon_nanotube_frames` | `cargo_pod_mk3_medium`, `cargo_bay_mk3_large`, `cargo_armored_mk3_medium` | Tier 3 on heavy template only. |

The tier 2 → tier 3 progression mirrors the existing tier 1 → tier 2 hull progression (e.g. `freighter_frame` requires `chemical_spaceframes`; `cryogenic_tanker_frame` requires `orbital_construction`). Tech that already gates hull scale is the natural prerequisite for slot scale.

### Engineering projects referenced
| Project id | Display name | Size | Tier | Required tech | `cargo_capacity_t` |
|------------|--------------|------|------|---------------|--------------------|
| `cargo_bay_large` | Standard Bulk Cargo Bay | Large | 1 | `basic_space_tech` | 50 |
| `cargo_pod_mk2_medium` | Reinforced Cargo Pod Mk2 | Medium | 2 | `cargo_hold_mk2` | 80 |
| `cargo_pod_mk3_medium` | Heavy-Lift Cargo Pod Mk3 | Medium | 3 | `cargo_hold_mk3` | 160 |
| `cargo_bay_mk2_large` | Bulk Cargo Bay Mk2 | Large | 2 | `cargo_hold_mk2` | 200 |
| `cargo_bay_mk3_large` | Heavy Bulk Bay Mk3 | Large | 3 | `cargo_hold_mk3` | 400 |
| `cargo_armored_mk2_medium` | Armored Logistics Pod Mk2 | Medium | 2 | `cargo_hold_mk2` | 65 |
| `cargo_armored_mk3_medium` | Heavy Armored Logistics Pod Mk3 | Medium | 3 | `cargo_hold_mk3` | 130 |

These modules were added to `assets/data/ship_modules.ron` in GRA-10 alongside the two techs. They follow the existing `cargo_pod_medium` / `cargo_bay_medium` / `cargo_armored_medium` shape (same fields, same `category: CargoStorage`, same `propulsion: None`). Slot size must match module size (Medium slot ↔ Medium module, Large slot ↔ Large module) — the Coder's loader rejects mismatches at startup.
---

## RON Schema

New file: `assets/data/freighter_templates.ron` (flat under `assets/data/`, sibling to `buildings.ron` / `ship_hulls.ron` / `ship_modules.ron`).

> **Path deviation from the original GRA-40 issue scope.** The original issue description said `assets/data/ships/freighter_templates.ron`. This design moves it to `assets/data/freighter_templates.ron` (no `ships/` subdirectory) for consistency with the existing flat layout. LGD reasoning: the `assets/data/` directory has no `ships/` subfolder today — `ship_hulls.ron` and `ship_modules.ron` both live at the `assets/data/` root. Adding a subdirectory just for freighter templates would split related files across two locations and is an unnecessary deviation from the established pattern. The Coder's loader path is `assets/data/freighter_templates.ron`.
```ron
(
    templates: [
        (
            id: "light_freighter",
            display_name: "Light Orbital Freighter",
            description: "Single-slot bulk hauler for short cislunar logistics. Cargo capacity is constrained by hull volume; tech upgrades do not change this template.",
            base_hull: "freighter_frame",
            era_tier: 1,
            required_tech: Some("chemical_spaceframes"),
            cargo_slots: [
                (
                    hull_slot_id: "cargo_a",
                    default_module: "cargo_pod_medium",
                    upgrade_path: [],
                ),
                (
                    hull_slot_id: "cargo_b",
                    default_module: "cargo_pod_medium",
                    upgrade_path: [],
                ),
            ],
            tags: ["logistics", "freighter", "early"],
        ),
        (
            id: "standard_freighter",
            display_name: "Standard Orbital Freighter",
            description: "Two-slot medium freighter whose second cargo slot accepts Mk2 reinforcement once the CargoHold Mk2 research line completes.",
            base_hull: "freighter_frame",
            era_tier: 2,
            required_tech: Some("orbital_construction"),
            cargo_slots: [
                (
                    hull_slot_id: "cargo_a",
                    default_module: "cargo_pod_medium",
                    upgrade_path: [],
                ),
                (
                    hull_slot_id: "cargo_b",
                    default_module: "cargo_pod_medium",
                    upgrade_path: [
                        (tier: 2, module: "cargo_pod_mk2_medium", required_tech: "cargo_hold_mk2"),
                    ],
                ),
            ],
            tags: ["logistics", "freighter", "mid-game"],
        ),
        (
            id: "heavy_freighter",
            display_name: "Heavy Industrial Freighter",
            description: "Three-slot bulk freighter built on a CNT-framed hull: two large bays and one medium bay. Mk2 and Mk3 slot upgrades compound on top of the larger footprint; full upgrade requires the CargoHold Mk3 research line.",
            base_hull: "bulk_cargo_frame",
            era_tier: 3,
            required_tech: Some("carbon_nanotube_frames"),
            cargo_slots: [
                (
                    hull_slot_id: "cargo_a",
                    default_module: "cargo_bay_large",
                    upgrade_path: [
                        (tier: 2, module: "cargo_bay_mk2_large", required_tech: "cargo_hold_mk2"),
                        (tier: 3, module: "cargo_bay_mk3_large", required_tech: "cargo_hold_mk3"),
                    ],
                ),
                (
                    hull_slot_id: "cargo_b",
                    default_module: "cargo_bay_large",
                    upgrade_path: [
                        (tier: 2, module: "cargo_bay_mk2_large", required_tech: "cargo_hold_mk2"),
                        (tier: 3, module: "cargo_bay_mk3_large", required_tech: "cargo_hold_mk3"),
                    ],
                ),
                (
                    hull_slot_id: "cargo_c",
                    default_module: "cargo_pod_medium",
                    upgrade_path: [
                        (tier: 2, module: "cargo_pod_mk2_medium", required_tech: "cargo_hold_mk2"),
                        (tier: 3, module: "cargo_pod_mk3_medium", required_tech: "cargo_hold_mk3"),
                    ],
                ),
            ],
            tags: ["logistics", "freighter", "bulk", "late-game", "cnt"],
        ),
    ],
)
```

### Field reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique. Lowercase_snake_case. Used as `FreighterTemplateId` enum variant or as a `String` lookup key — Coder's call. |
| `display_name` | string | yes | Human-readable, used in UI. |
| `description` | string | yes | One-paragraph description in the existing tech/hull voice. |
| `base_hull` | string | yes | `id` of an entry in `ship_hulls.ron`. Must exist at load (validated at startup). |
| `era_tier` | u32 | yes | 1–3 (current scope). Higher = better cargo scaling, requires higher tech. |
| `required_tech` | `Option<String>` | yes | Tech id (e.g. `"chemical_spaceframes"`) that must be researched before this template can be built. Matches the pattern used by hulls. |
| `cargo_slots` | list of cargo_slot | yes | The cargo slots this template exposes. Empty list = pure tanker, no cargo capacity (e.g. `cryogenic_tanker_frame` in a future expansion). |
| `cargo_slots[].hull_slot_id` | string | yes | Matches a `slot_id` in the base hull's `slot_layout`. Slot must have `category: CargoStorage`. |
| `cargo_slots[].default_module` | string | yes | Module id from `ship_modules.ron` installed at slot.build_tier (initial build). |
| `cargo_slots[].upgrade_path` | list of upgrade_step | yes | Ordered low-to-high tier. Empty list = no upgrades possible. |
| `upgrade_path[].tier` | u32 | yes | **1-indexed to match the existing hull/tech tier system.** The tier at which this upgrade is applied. `2` = Mk2, `3` = Mk3. The baseline (tier 1) is implicit in `default_module` and does not appear in `upgrade_path`. Must be unique per slot. || `upgrade_path[].module` | string | yes | Module id installed at this tier. Must be a `CargoStorage` module. |
| `upgrade_path[].required_tech` | string | yes | Tech id that gates this upgrade. Same string the technology system uses. |
| `tags` | list of string | yes | Free-form tags. Mirror hull tags. |

### Tier-index mapping (RON 1-indexed → component 0-indexed)

The Coder's per-entity component stores the **current upgrade state** as `slot.upgrade_tier: u32`, **0-indexed**:

| `slot.upgrade_tier` | Meaning | Module installed |
|---------------------|---------|------------------|
| `0` | Baseline (no upgrade applied) | `cargo_slots[].default_module` |
| `1` | Mk2 applied | `cargo_slots[].upgrade_path[0].module` (i.e. the entry whose `tier` field equals `2`) |
| `2` | Mk3 applied | `cargo_slots[].upgrade_path[1].module` (i.e. the entry whose `tier` field equals `3`) |
| `N ≥ 1` | Nth upgrade applied | `cargo_slots[].upgrade_path[N - 1].module` |

Lookup rule, spelled out for the Coder:

```
fn installed_module(slot: &CargoSlot, upgrade_tier: u32) -> &str {
    if upgrade_tier == 0 {
        &slot.default_module
    } else {
        &slot.upgrade_path[(upgrade_tier - 1) as usize].module
    }
}
```

The RON's 1-indexed `upgrade_path[].tier` field stays aligned with `hull.tier` / `tech.tier` (which are also 1-indexed throughout `ship_hulls.ron` and `technologies.ron`). The component's 0-indexed `upgrade_tier` makes the array-indexing arithmetic clean: `upgrade_path[upgrade_tier - 1]`. Resolution (a) per the editorial note in the LGD sign-off comment.
### Validation rules (load-time)

The Coder's loader MUST enforce:

1. Every `base_hull` exists in `ship_hulls.ron`.
2. Every `cargo_slots[].hull_slot_id` exists in the base hull's `slot_layout` and has `category: CargoStorage`.
3. Every `default_module` and `upgrade_path[].module` exists in `ship_modules.ron` and has `category: CargoStorage`.
4. Every `required_tech` (in template and in upgrade path) is a valid tech id (not necessarily researched at load — just a known id).
5. Within one `cargo_slots[].upgrade_path`, tier numbers are unique and ordered ascending.
6. The base hull's `required_tech` ⊆ this template's `required_tech` (a freighter template cannot unlock before its hull can be built).

A failure on any of these is a hard loader error. The Coder writes a `tests/freighter_templates_data_tests.rs` test that loads the file and asserts the rules.

### Schema delta

- **No new enum variants** required in Rust. `FreighterTemplateId` is a new enum the Coder adds in `src/ships/templates.rs`. The Coder is free to choose `enum FreighterTemplateId { LightFreighter, StandardFreighter, HeavyFreighter }` or `String`-keyed lookup. Either works; LGD has no preference.
- **No new `ResourceType`** required.
- **No new `TechCategory`** required — both new techs use `SpaceTechnology`.
- **No new `PropulsionType`** required.
- **No new dependency** required.

---

## Default Templates

Three templates ship in the initial `freighter_templates.ron`:

1. **`light_freighter`** — early game (tier 1, `chemical_spaceframes`). Two cargo slots, no upgrades. Matches today's `freighter_frame` behavior one-to-one.
2. **`standard_freighter`** — mid game (tier 2, `orbital_construction`). Two cargo slots; the second slot upgrades Mk1→Mk2 with `cargo_hold_mk2`. Total cargo 70t → 115t.
3. **`heavy_freighter`** — late game (tier 3, `carbon_nanotube_frames`). Three cargo slots (two Large, one Medium), all upgradeable Mk1→Mk2→Mk3. Total cargo 135t (tier 0) → 480t (Mk2) → 960t (Mk3).

The "tier" of the template is **the era tier it belongs to**, not the upgrade tier it caps at. `light_freighter` is era tier 1 and caps at upgrade tier 1. `heavy_freighter` is era tier 3 and caps at upgrade tier 3. A future `ultra_heavy_freighter` would be era tier 4 (fusion era, `fusion_superstructures`) and could cap at upgrade tier 4 once that research line is added in a later roadmap.

### What is *not* a default template (yet)

- `mining_barge` — the existing `mining_barge_frame` is a freighter-class hull but with ISRU bays, not cargo. A `mining_barge_template` could be added in a future issue once the design has a clear "freighter variant for mining logistics" use case. Out of scope for GRA-40.
- `cryogenic_tanker` — the existing `cryogenic_tanker_frame` and `outer_system_tanker_frame` are propellant carriers, not cargo. Adding a tanker template with no cargo slots is technically trivial (empty `cargo_slots: []`) but provides no gameplay value until a propellant-routing AI exists (a future roadmap item, related to GRA-31 in_situ_resource chaining). Out of scope for GRA-40.
- The interstellar precursor and the long-range survey are ResearchVessel, not Freighter — out of scope.

---

## Cargo Capacity Matrix

Cargo capacity (in tonnes) for each `(template, upgrade_state)` combination. Computed by summing `cargo_capacity_t` over all installed cargo modules in the template's slots.

| Template | Tier 0 (default) | Tier 1 (Mk2) | Tier 2 (Mk3) |
|----------|------------------|--------------|--------------|
| `light_freighter` | 2 × 35 = **70 t** | n/a | n/a |
| `standard_freighter` | 1 × 35 + 1 × 35 = **70 t** | 1 × 35 + 1 × 80 = **115 t** | n/a |
| `heavy_freighter` | 2 × 50 + 1 × 35 = **135 t** | 2 × 200 + 1 × 80 = **480 t** | 2 × 400 + 1 × 160 = **960 t** |

> **Why the asymmetric upgrade pattern in `heavy_freighter`.** The Large slots (cargo_a, cargo_b) carry `cargo_bay_large` (50t) at tier 0; the Medium slot (cargo_c) carries `cargo_pod_medium` (35t). The Mk2 upgrade path swaps the Large slots to `cargo_bay_mk2_large` (200t, 4×) and the Medium slot to `cargo_pod_mk2_medium` (80t, 2.3×). The Mk3 upgrade path lands at 400t and 160t respectively. The asymmetry rewards the player who researches the full chain: a Mk3 heavy freighter carries ~14× the tonnage of an un-upgraded heavy and ~8× a fully-upgraded standard. The intermediate "Mk2-only heavy" at 480t is a coherent stopping point for a player who can't justify the Mk3 research cost.

### Sanity check against existing hulls

- `freighter_frame`: 2 medium cargo slots, today holds `cargo_pod_medium` (35t) per slot → 70t. Matches the `light_freighter` default. ✅
- `bulk_cargo_frame`: 2 large + 1 medium cargo slots. Today no `cargo_bay_large` exists, so the Large slots in `bulk_cargo_frame` cannot be filled with cargo — they are dead weight in v0.4.0. Adding `cargo_bay_large` (the new tier-1 module) plus the Mk2 and Mk3 variants gives the hull its first real cargo loadout. ✅

---

## AI Build Policy

The auto-construction AI in GRA-39 (Coder-owned, blocked on this issue) picks **the best template the home shipyard can currently build, tech-gated, deterministic**.

### Policy in one paragraph

When a `ShippingCompany` decides to build a new freighter, it queries its home shipyard for `(template, current_best_upgrade_tier)` pairs. A pair is *buildable* if:

1. The template's `required_tech` is researched.
2. The base hull's `required_tech` is researched (this is implied by 1 once the hull is referenced in the template — the validation rule §[Validation rules](#validation-rules-load-time) makes this transitive).
3. For each cargo slot in the template, the installed module at the chosen `current_best_upgrade_tier` has its `required_tech` researched. The "best" tier for a slot is the highest tier in the slot's `upgrade_path` whose `required_tech` is researched.

Among the buildable `(template, current_best_upgrade_tier)` pairs, the AI picks the one with the highest `total_cargo_capacity_t`. Ties are broken by lowest `total_build_points` (cheapest), then by template `id` lexicographic (deterministic, reproducible across runs).

### Worked example

A `ShippingCompany` at home shipyard Earth with `cargo_hold_mk2` researched but not `cargo_hold_mk3` evaluates:

| Template | Buildable? | Best tier | Cargo |
|----------|-----------|-----------|-------|
| `light_freighter` | yes (`chemical_spaceframes` researched) | 0 | 70 t |
| `standard_freighter` | yes (`orbital_construction` researched) | 1 | 115 t |
| `heavy_freighter` | yes (`carbon_nanotube_frames` researched) | 1 (Mk2 on all 3 slots) | 2 × 200 + 1 × 80 = 480 t |

The AI picks `heavy_freighter` at its best current tier, total 480 t.

After the player researches `cargo_hold_mk3`, the heavy freighter's three slots all become upgradeable to Mk3, total 960 t. Existing freighters stay at their current state; the AI now builds *new* heavies at the new best tier.

### Determinism and AI-faction interaction

The policy is fully deterministic and RON-driven. A player who edits `assets/data/freighter_templates.ron` to add a `mega_freighter` template sees the AI start building that template as soon as its tech requirements are met. No code change, no restart-required config push, no hidden in-memory state. This is the DW2 "AI uses player-made designs" pattern, expressed as deterministic RON lookup.

For v0.4.1 the AI does not *upgrade* existing freighters in place when new tech completes; it only builds new freighters at the new best tier. The player can manually refit existing freighters via the shipbuilding UI (already in place for slot modules). This keeps the AI loop small and is consistent with how Aurora handles tech-driven component upgrades.

---

## Migration Plan

### Source-of-truth shift

Before GRA-40: cargo capacity is `(hull, installed_module)` lookup. The "freighter type" is `ShipClass::Freighter` plus the hull id.

After GRA-40: cargo capacity is `(template, installed_modules)` lookup. The `ShipClass::Freighter` enum is retained as a *role tag* (used for fleet categorization, UI filters, AI fleet-role assignment) but is no longer the source of truth for cargo capacity. A `ShipTemplateRef<FreighterTemplate>` component on each freighter entity carries the template id; a per-slot `ShipSlot` component carries the installed module id and current upgrade tier.

### Existing entities

On load, the migration shim runs once:

```
for entity in entities with ShipClass::Freighter:
    if not entity has ShipTemplateRef:
        add ShipTemplateRef(template_id = "light_freighter")
        for slot in template.cargo_slots:
            add ShipSlot {
                slot_id: slot.hull_slot_id,
                installed_module: slot.default_module,
                upgrade_tier: 0,
            }
```

The shim is a no-op for entities that already have `ShipTemplateRef` (e.g. new ships created post-merge). The Coder is free to fold the shim into a `MigrationPhase` system that runs at startup before the first `FixedUpdate` tick.

### Save format

If a save has a freighter entity without `ShipTemplateRef`, the shim fills it in on load. If a save has a freighter entity with an unknown `template_id`, the load fails with a `LoaderError::UnknownFreighterTemplate(id)` — better to fail loudly than to silently downgrade. Saves created after the merge always have `ShipTemplateRef` populated by the shipyard construction path.

### Hull-only path is preserved

The existing `src/shipbuilding/` pipeline (player builds a freighter by picking a hull and fitting modules) continues to work — it just produces an entity that also has a `ShipTemplateRef` derived from the hull (e.g. building a `freighter_frame` defaults to `ShipTemplateRef<LightFreighter>`). The Coder is the one to choose where in `src/shipbuilding/` the template is assigned; the LGD rule is "the default is the cheapest era tier whose hull matches".

---

## Modder Surface

A modder who wants to:

- **Add a new freighter variant.** Add an entry to `freighter_templates.ron` and reference an existing or new hull in `ship_hulls.ron`. Done.
- **Change a slot's upgrade chain.** Edit the `upgrade_path` list. The AI immediately uses the new chain.
- **Add a new cargo module tier (e.g. Mk4).** Add a new engineering project to `ship_modules.ron`, add a new `cargo_hold_mk4` tech, and add `(tier: 4, module: "...", required_tech: "cargo_hold_mk4")` to the relevant `upgrade_path` entries.
- **Replace the default module for a slot.** Edit `default_module`. New ships use the new default; existing ships keep their installed module (no silent in-place re-equip).
- **Make a freighter unbuildable by the AI.** Set `required_tech` to a tech that will never be researched. The template still appears in the UI for the player to build manually; the AI just never auto-constructs it. This is the "designer's freighter" pattern.

No schema migration is required for any of the above. The loader reads `freighter_templates.ron` at startup; changes are picked up on the next game launch.

---

## Rust Deltas (Hand-off to CTO)

Coder owns the implementation. The LGD-level delta is:

1. **New file** `assets/data/freighter_templates.ron` — schema as in §[RON Schema](#ron-schema).
2. **New file** `assets/data/freighter_templates_default.ron.bak` — none. RON file replaces a slot in the existing loader.
3. **New ship modules** in `assets/data/ship_modules.ron` — the six entries in §[Tech Upgrade Matrix](#tech-upgrade-matrix).
4. **New tech entries** in `assets/data/technologies.ron` — `cargo_hold_mk2`, `cargo_hold_mk3`.
5. **New Rust struct + module** `src/ships/templates.rs`:
   - `pub struct FreighterTemplateRon { templates: Vec<FreighterTemplate> }` (loader-facing, mirrors RON).
   - `pub struct FreighterTemplate { id, display_name, description, base_hull, era_tier, required_tech, cargo_slots, tags }`.
   - `pub struct CargoSlot { hull_slot_id, default_module, upgrade_path }`.
   - `pub struct UpgradeStep { tier, module, required_tech }`.
   - `pub struct FreighterTemplateRegistry` Bevy `Resource` populated by the loader. Methods: `get(id)`, `cargo_capacity(template_id, installed_modules)`, `best_buildable(available_techs)`.
6. **New components** in `src/ships/components.rs`:
   - `pub struct ShipTemplateRef { pub template_id: FreighterTemplateId }` (or `String`; Coder's call).
   - `pub struct ShipSlot { pub slot_id: String, pub installed_module: String, pub upgrade_tier: u32 }`.
7. **Cargo capacity query** in `src/ships/cargo.rs` (or wherever existing cargo queries live): a function `freighter_cargo_capacity_t(entity, registry) -> f32` that sums the installed modules' `cargo_capacity_t`. The existing `cargo_capacity_t` attribute system on ship modules is reused — no new attribute keys.
8. **Migration shim** in `src/ships/migration.rs`: runs at startup, populates `ShipTemplateRef` and `ShipSlot` for legacy `ShipClass::Freighter` entities.
9. **AI hook for GRA-39** in `src/economy/company.rs` (GRA-39 Coder's territory, but LGD exposes the helper): `FreighterTemplateRegistry::best_buildable(available_techs) -> Option<(FreighterTemplateId, u32 /* best tier */)>`. GRA-39 imports this and uses it as the default build target.
10. **New tests** in `tests/freighter_templates_data_tests.rs`:
    - All `base_hull` references exist.
    - All `hull_slot_id` references exist in the base hull's `slot_layout` and have `category: CargoStorage`.
    - All `default_module` and `upgrade_path[].module` references exist in `ship_modules.ron` and have `category: CargoStorage`.
    - Cargo capacity matrix matches §[Cargo Capacity Matrix](#cargo-capacity-matrix) for all 5 buildable (template, upgrade_state) combinations.
    - `best_buildable` with no techs returns the cheapest era-tier-1 template; with `cargo_hold_mk3` researched returns the heavy template at tier 2.

### Bevy 0.18 / architecture notes

- `FreighterTemplateRegistry` is a Bevy `Resource`, loaded once at startup. Hot-reload of `freighter_templates.ron` is **out of scope** for v0.4.1; the file is read once and the resource is read-only after.
- `ShipTemplateRef` and `ShipSlot` are components on the freighter entity. They are *not* inside `FreighterTemplate` (which is a config struct, not an entity component).
- All systems consume `SimulationTime` and run in `FixedUpdate` (per `helios-architecture` rule 1).
- No new plugins; integrates with the existing `ShipsPlugin` (already owns hulls, modules, construction).
- No new `TechCategory`, no new `ResourceType`, no new `PropulsionType`. No Cargo dependency changes.

### Out of scope for GRA-40 (Rust side)

- The auto-construction AI loop itself (GRA-39, Coder-owned).
- The Private Shipping overview panel (GRA-43, Coder-owned).
- Refit UI for upgrading an existing freighter's slot tier in place (the data model supports it; the UI is a follow-up issue. For v0.4.1 the player builds new freighters at the new best tier; refit can ship in v0.4.2).
- Hot-reload of `freighter_templates.ron`.
- Non-freighter ship classes (cruisers, scouts) using the same template pattern. The pattern is reusable; doing it for all classes is a separate, larger issue.

---

## Test Plan

The smallest in-game check that proves the work:

1. Start a new game on a default Sol setup.
2. Open the Shipbuilding panel → Freighter tab. Verify three templates appear: Light, Standard, Heavy.
3. Build a Light Freighter. Verify cargo capacity reads **70 t** (the §[Cargo Capacity Matrix](#cargo-capacity-matrix) default).
4. Research `cargo_hold_mk2` (tier 2, ~3.5k research_cost; lands after the standard research chain reaches `orbital_construction`).
5. Open the shipbuilding panel for an existing Standard Freighter. Verify the "Upgrade slot 2" button is enabled (Mk2 tech met, tier 1 upgrade available). Click it. Verify cargo capacity updates to **115 t**.
6. Research `cargo_hold_mk3` (tier 3, requires `carbon_nanotube_frames` first).
7. Open the shipbuilding panel for an existing Heavy Freighter. Verify slot 3 can be upgraded to Mk3. Click. Verify cargo capacity updates to **780 t**.
8. Spawn a new game with a `ShippingCompany` that has `cargo_hold_mk2` researched. Watch the company's auto-construction loop. Verify the next freighter it builds is `heavy_freighter` (era tier 3, best tier 1), not `standard_freighter` — confirming the DW2-style "most efficient buildable design" policy.
9. Load an old save (pre-GRA-40) with one or more `ShipClass::Freighter` entities. Verify the migration shim runs, all freighters load, and the system has no errors. Verify the freighter fleet shows in the new UI with the correct cargo capacities (all at 70 t, since old saves were always era tier 1).

### Cargo capacity test (Rust)

`tests/freighter_templates_data_tests.rs::test_cargo_capacity_matrix` loads `freighter_templates.ron` and asserts:

```rust
let registry = FreighterTemplateRegistry::from_ron("assets/data/freighter_templates.ron")?;

assert_eq!(registry.cargo_capacity("light_freighter", &[(0, "cargo_pod_medium"), (0, "cargo_pod_medium")]), 70.0);

let standard_default = registry.cargo_capacity("standard_freighter", &[(0, "cargo_pod_medium"), (0, "cargo_pod_medium")]);
let standard_mk2    = registry.cargo_capacity("standard_freighter", &[(0, "cargo_pod_medium"), (1, "cargo_pod_mk2_medium")]);
assert_eq!(standard_default, 70.0);
assert_eq!(standard_mk2, 115.0);

let heavy_default = registry.cargo_capacity("heavy_freighter", &[(0, "cargo_bay_large"), (0, "cargo_bay_large"), (0, "cargo_pod_medium")]);
let heavy_mk2     = registry.cargo_capacity("heavy_freighter", &[(1, "cargo_bay_mk2_large"), (1, "cargo_bay_mk2_large"), (1, "cargo_pod_mk2_medium")]);
let heavy_mk3     = registry.cargo_capacity("heavy_freighter", &[(2, "cargo_bay_mk3_large"), (2, "cargo_bay_mk3_large"), (2, "cargo_pod_mk3_medium")]);
assert_eq!(heavy_default, 135.0);
assert_eq!(heavy_mk2, 480.0);
assert_eq!(heavy_mk3, 960.0);
```

### Best-buildable test (Rust)

`tests/freighter_templates_data_tests.rs::test_ai_best_buildable` asserts:

```rust
let registry = FreighterTemplateRegistry::from_ron("assets/data/freighter_templates.ron")?;
let nothing: HashSet<&str> = HashSet::new();
let (t, tier) = registry.best_buildable(&nothing).unwrap();
assert_eq!(t, "light_freighter");
assert_eq!(tier, 0);

let mid = ["orbital_construction", "carbon_nanotube_frames", "cargo_hold_mk2"].into_iter().collect();
let (t, tier) = registry.best_buildable(&mid).unwrap();
assert_eq!(t, "heavy_freighter");
assert_eq!(tier, 1); // Mk2 on all 3 slots (Mk3 not researched)

let all = ["cargo_hold_mk3"].into_iter().collect();
let (t, tier) = registry.best_buildable(&all).unwrap();
assert_eq!(t, "heavy_freighter");
assert_eq!(tier, 2); // Mk3 on all 3 slots
```

### Acceptance criteria mapping

| Issue criterion | Where it is tested |
|-----------------|---------------------|
| 1. LGD design doc accepted | This file, posted on the issue. |
| 2. RON schema validates with the existing RON loader | `tests/freighter_templates_data_tests.rs::test_loader_validates`. |
| 3. Cargo capacity queries return correct values for each (template, upgrade) combination | §[Cargo capacity test (Rust)](#cargo-capacity-test-rust). |
| 4. Tech-gated slot upgrades can't be installed before the relevant research is complete | UI grays out (Coder's UI work) + Rust `if !has_tech` guard in the refit handler. Test: `test_tech_gated_upgrade_rejected_without_research`. |
| 5. Existing saves load without error after the migration shim runs | §[Test plan](#test-plan) step 9. |
| 6. `cargo test -p helios_ascension ships::templates::tests::test_cargo_capacity_matrix` (new — full template × upgrade matrix) | §[Cargo capacity test (Rust)](#cargo-capacity-test-rust). |
| 7. `cargo clippy --all-targets --all-features -- -D warnings` clean | Coder runs before opening the PR. |
| 8. `cargo fmt --check` clean | Coder runs before opening the PR. |
| 9. CI green | Actions runs `cargo test`; see `helios-ci-pipeline`. |
| 10. Docs in place | This file + `CLAUDE.md` shipbuilding section update. |

---

## Out of Scope

- Other ship classes (cruisers, scouts, tankers) using the same template pattern. The pattern is reusable; the LGD note is "open a follow-up issue once the freighter pattern is proven".
- Player-driven freighter building (already works; `ShipClass::Freighter` selection in the shipyard UI is unchanged; the `ShipTemplateRef` is set automatically by the Coder's load-time assignment).
- Refit UI for upgrading an existing freighter's slot tier in place (the data model supports it; the UI is GRA-40-followup).
- The interstellar precursor and the long-range survey (ResearchVessel, not Freighter).
- An in-game ship designer UI for the player to author freighter templates. RON modding is the player-influence path the operator's "without too much complexity" constraint invites.
- Hot-reload of `freighter_templates.ron`. The file is read once at startup.
- The auto-construction AI loop itself (GRA-39, Coder-owned; uses this doc's `best_buildable` helper).
- Propellant tanker templates (the existing `cryogenic_tanker_frame` and `outer_system_tanker_frame`). Empty `cargo_slots: []` would technically work but provides no gameplay value until propellant routing exists.

---

## Related Work

- `docs/design/LOGISTICS_NETWORK.md` — the GRA-31 system this template model feeds into. The AI policy in §[AI Build Policy](#ai-build-policy) is the GRA-39 default build target.
- `assets/data/ship_hulls.ron` — source of the base hulls the templates reference.
- `assets/data/ship_modules.ron` — source of the cargo modules the templates install.
- `assets/data/technologies.ron` — receives the two new techs (`cargo_hold_mk2`, `cargo_hold_mk3`).
- Issue `GRA-37.c` (this design's parent) — Roadmap 4.4 logistics follow-up.
- Issue `GRA-38` — auto-freight AI (Coder-owned, downstream of this issue).
- Issue `GRA-39` — company freighter auto-construction (Coder-owned, uses this doc's `best_buildable` helper).
