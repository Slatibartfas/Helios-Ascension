# Shipbuilding

## Overview

Helios Ascension's shipbuilding system is data-driven and currently centered on a single canonical set of ship assets:

- `assets/data/ship_hulls.ron` defines hull frames and slot layouts.
- `assets/data/ship_modules.ron` defines ship modules and their gameplay stats.
- `assets/data/technologies.ron` defines the research unlock path and engineering targets that gate hulls and ship module families.

The repository intentionally uses those files as the source of truth. Temporary or generated `ship_modules*.ron` snapshots should not be checked in alongside the canonical data.

The gameplay data model is paired with a single native Bevy UI frontend:

- `src/ui/shipbuilding_workspace.rs` is the sole shipbuilding UI path
- `src/ui/shipbuilding_state.rs` holds the shared frontend state consumed by that workspace

## Current Data Set

- **18 hull definitions** in `assets/data/ship_hulls.ron` — 17 ship frames (`*_frame`) and 1 station core (`orbital_foundry_core`)
- **84 ship module definitions** in `assets/data/ship_modules.ron`
- **21 `ShipModuleCategory` variants** in `src/shipbuilding/types.rs`: 12 consolidated (the canonical Aurora-style taxonomy) and 9 legacy sub-categories retained for backward compatibility with existing RON data
- Ship progression is organized around five propulsion eras: **Chemical, Fission / NTR, Gas-Core / Early Fusion, Fusion Torch, and Antimatter**

## Module Categories

`ShipModuleCategory` is a 21-variant enum. New RON data should target the **12 consolidated** categories. The 9 **legacy** sub-categories remain in the enum only so existing hull `slot_layout` and module `category` fields keep deserializing — they are not first-class authoring targets.

**Consolidated (12, canonical):**

- `FlightSystems`
- `PowerThermal`
- `FuelStorage`
- `Weapons`
- `FireControl`
- `Sensors`
- `ArmorDefense`
- `CrewSystems`
- `UtilitySupport`
- `ConstructionISRU`
- `ElectronicWarfare`
- `SpecialScience`

**Legacy (9, retained for compatibility):**

- `Bridges`, `Habitats`, `Medical`, `Maintenance`, `CargoStorage`, `Magazines`, `PointDefense`, `Armor`, `Construction`

LGD balance decisions from GRA-7 keep `Medical` and `CrewSystems` as distinct categories rather than collapsing them — the med-bay slot is sized for sickbays, surgical units, and triage systems and is not interchangeable with general crew quarters. The `ConstructionISRU` slot family unifies mining heads, regolith processors, gantries, and habitat modules under a single "industrial / in-situ resource utilization" umbrella.

## Hull Summary

The hull set has 18 entries. They are organized by tier (in `assets/data/ship_hulls.ron`, `tier: 1..3` for ships; stations are tagged with `is_station: true` instead of a tier):

**Tier 1 — Chemical-era probes & small craft (5 hulls, all `required_tech: chemical_spaceframes`):**

- `micro_probe_frame` — Microsat Probe Bus (cubesat-class payloads, 0.08 t dry)
- `small_probe_frame` — Deep Survey Probe (flybys and long-baseline surveys, 0.42 t)
- `courier_frame` — Relay Courier Probe (long-endurance robotic comms bus, 0.8 t)
- `lander_frame` — Planetary Lander Bus (descent / rover / sample-return, 2.2 t)
- `probe_carrier_frame` — Survey Carrier Stage (modern upper-stage carrier, 9.5 t)

**Tier 2 — Orbital-assembly combatants and short-haul logistics (3 hulls):**

- `fighter_frame` — Orbital Interceptor Hull (orbital_construction, 120 t)
- `frigate_frame` — Escort Frigate Hull (orbital_construction, 1 800 t)
- `destroyer_frame` — Combat Destroyer Hull (orbital_assembly_heavy)

**Logistics family (5 hulls, added in GRA-9):**

- `freighter_frame` — General-purpose freighter hull
- `mining_barge_frame` — Mining & Refinery Barge (atmosphere mining, regolith processing)
- `cryogenic_tanker_frame` — Cryogenic Tanker (Earth/Mars/Venus bulk propellant transfer)
- `bulk_cargo_frame` — Industrial Bulk Cargo Hull (CNT-framed interplanetary freighter)
- `outer_system_tanker_frame` — Outer-System Cryogenic Tanker (fusion-era, multi-month trans-Jovian)

**Survey family (1 hull, added in GRA-9):**

- `long_range_survey_frame` — Long-Range Survey Hull (fusion-era outer-system and near-interstellar survey, plugs into `SpecialScience` slot family)

**Tier 3 — Capital / interstellar (3 hulls):**

- `cycler_frame` — Inner-system cycler, gas-core / early-fusion era
- `torch_cruiser_frame` — Fusion Torch cruiser (`fusion_superstructures`)
- `interstellar_precursor_frame` — Antimatter-era precursor (`antimatter_containment_structures`)

**Stations (1 core):**

- `orbital_foundry_core` — Orbital shipyard / foundry station (constructed in place)

Slot compatibility is enforced through `slot_layout` category and size matching in the shipbuilding data loader and UI. Hulls in the same tier share the same propulsion-era technology gate (`required_tech`) on the hull definition; module families are gated independently on each module entry.

## Propulsion Eras

Five propulsion eras define the progression curve. Each era unlocks a coordinated set of hulls, drives, reactors, and slot families. The technology in `unlocks_engineering` for the era's flagship drive must remain a single shared engineering target so all module variants in that family unlock through one engineering project.

| Era | Hulls | Flagship drive tech | Sample engineering target | Hull-construction tech |
| --- | --- | --- | --- | --- |
| **Chemical** | `micro_probe_frame`, `small_probe_frame`, `courier_frame`, `lander_frame`, `probe_carrier_frame` | `chemical_rockets` / `advanced_chemical_rocket` | `standard_chemical_rocket` | `chemical_spaceframes` |
| **Fission / NTR** | `fighter_frame`, `frigate_frame`, `destroyer_frame`, `freighter_frame` | `fission_power` + `nerva_drive` / `kiwi_drive` | `fission_pile`, `nerva_drive` | `orbital_construction`, `orbital_assembly_heavy` |
| **Gas-Core / Early Fusion** | `cycler_frame`, `mining_barge_frame`, `cryogenic_tanker_frame`, `bulk_cargo_frame` | `gas_core_fission`, `ion_drive` | `gas_core_fission`, `ion_drive` | `carbon_nanotube_frames` |
| **Fusion Torch** | `torch_cruiser_frame`, `outer_system_tanker_frame`, `long_range_survey_frame` | `fusion_torch` | `fusion_torch` | `fusion_superstructures` |
| **Antimatter** | `interstellar_precursor_frame` | `antimatter_propulsion` | `antimatter_drive` | `antimatter_containment_structures` |

Within an era, hull `required_tech` controls whether the hull class is even visible. Module `required_tech` controls module visibility, and module `required_component_design` selects the engineering project that must be completed before any module in that family can be installed. **All ship modules in the current RON set both fields** — the runtime loader is not tolerant of a missing `required_component_design`, and new module entries should follow that rule.

## Logistics & Survey Hulls (added in GRA-9)

Five hulls introduced in GRA-9 are dedicated to industrial and exploration roles rather than direct combat:

- **`mining_barge_frame`** — twin ISRU bays + cargo bay for atmosphere / regolith processing; unifies mining-head, regolith-processor, and habitat-module slots under the `ConstructionISRU` family
- **`cryogenic_tanker_frame`** — three-tank depot-to-depot propellant logistics for the inner system
- **`bulk_cargo_frame`** — CNT-framed interplanetary freighter with three cargo bays; carries prefab modules, machinery, and bulk consumables
- **`outer_system_tanker_frame`** — three large cryo tanks, torch-rated reactor / radiator bay, and a long-endurance habitat with an embedded medical bay for multi-month trans-Jovian cruises
- **`long_range_survey_frame`** — gives the `SpecialScience` slot family a second hull so the category is no longer a single-purpose island on the interstellar precursor

These hulls use the same `slot_layout` discipline as the existing frames; the `position` authoring rule below applies equally to them.

## Current UI Workflow

The native workspace is the production shipbuilding frontend. Its current design centers on:

- A **blueprint canvas** that renders hull slots as cards on a schematic grid
- A **focused-slot library** that filters compatible modules for the selected slot
- A live **engineering analytics** pane driven by `ShipbuildingData::summarize_design()`
- Shared preview/selection state so hovering a module can show deltas before installation
- Native tabs for **Design**, **Archive**, **Construction**, and **Components**
- Direct handoff into the Research panel for engineering project selection

## Slot Placement Notes

The native blueprint can use authored slot positions when they exist:

- `HullSlotDefinition.position` is an optional normalized `(x, y)` coordinate in `assets/data/ship_hulls.ron`

When `position` is absent, the workspace falls back to heuristic placement based on slot ID/category. That keeps all hulls renderable, but it is only an approximation.

For high-quality blueprint layouts, prefer authored `position` data in the hull definitions. The LGD has reserved an `lgd/blueprint-positions-tier1-hulls` branch for tier-1 hulls; new hulls should ship with `position` data whenever possible.

## Data Authoring Rules

When adding or editing ship content:

1. Update only the canonical files in `assets/data/`.
2. Keep module IDs unique. Duplicate IDs silently collapse during loading because modules are indexed by ID.
3. Keep RON tuple structure exact. Missing `),` separators will break deserialization.
4. Use only valid `ResourceType`, `ShipModuleCategory`, `ShipClass`, `PropulsionType`, and `HullSizeTier` enum values.
5. **Use the 12 consolidated `ShipModuleCategory` variants for new entries.** Legacy sub-categories are tolerated for backward compatibility only.
6. If a new module requires research, add or update the matching technology entry in `assets/data/technologies.ron`.
7. **Every module must set both `required_tech` and `required_component_design`.** Visibility and engineering gating are now decoupled: `required_tech` shows the family in the UI; `required_component_design` is the engineering project the player must complete before installing any module in that family.
8. If multiple modules share one engineering target, author `required_component_design` so the family unlocks through a single engineering project.
9. Ensure the owning technology exposes that engineering target through `unlocks_engineering`; tech tooltips and engineering availability depend on that coupling.
10. Update this document and `.github/copilot-instructions.md` when the data model or workflow changes.
11. If a hull should display cleanly in the native blueprint workspace, add `position` data to `slot_layout` entries instead of relying on heuristic placement.

## Technology Coupling

Ship module progression now has two explicit authored links:

1. `required_tech` controls when the module family is even visible.
2. `required_component_design` selects the engineering project that must be completed before any module in that family can be installed.

Hull progression now has its own authored gate:

1. `required_tech` on hulls represents the spaceframe or construction breakthrough needed to build that class of hull.
2. Early hulls use `chemical_spaceframes`, midgame combatants move through `orbital_assembly_heavy` and `carbon_nanotube_frames`, and late ships rely on `fusion_superstructures` or `antimatter_containment_structures`.
3. This prevents late propulsion families from trivially riding on modern baseline hull architecture even when slot sizes would otherwise match.

When a family target is not explicitly authored in the `components` array, the runtime synthesizes the engineering definition from ship module data. That synthesis still depends on `unlocks_engineering` in `assets/data/technologies.ron` so the tech tree and Available Engineering tab expose the project correctly.

For ship-related content, the intended workflow is:

1. Add or update the technology in `assets/data/technologies.ron` and reference the engineering target in its `unlocks_engineering` array.
2. Add or update the module in `assets/data/ship_modules.ron` with **both** `required_tech` and `required_component_design` set.
3. Add or update the hull in `assets/data/ship_hulls.ron` when a new propulsion era needs a new spaceframe.
4. If the module belongs to an existing family, point `required_component_design` at that family target instead of inventing a parallel unlock path.
5. Ensure the module's `required_tech` and the technology's `unlocks_engineering` entry refer to the same progression step.
6. Validate with `cargo build` and a short `cargo run` to catch runtime RON parsing errors.

## Validation

Recommended validation commands after shipbuilding data changes:

```bash
cargo build
cargo run
```

`cargo build` catches Rust-side issues. `cargo run` is still required because malformed RON, invalid enum variants, and duplicate IDs only surface during data loading at runtime.

Recommended validation after shipbuilding UI changes:

```bash
cargo check
cargo run
```

- `cargo check` catches Rust/UI API issues quickly
- `cargo run` is still the final validation step for slot layout, hover/selection behavior, and runtime-only ECS/query conflicts
