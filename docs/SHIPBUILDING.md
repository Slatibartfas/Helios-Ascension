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

> **32 hulls · 295 modules · 21 categories (12 consolidated + 9 legacy)**

- **32 hull definitions** in `assets/data/ship_hulls.ron` — 31 ship frames (`*_frame`) and 1 station core (`orbital_foundry_core`). The 32 ids (verbatim from the file) are: `micro_probe_frame`, `small_probe_frame`, `courier_frame`, `courier_vessel_frame`, `lander_frame`, `probe_carrier_frame`, `fighter_frame`, `patrol_frigate_frame`, `frigate_frame`, `destroyer_frame`, `freighter_frame`, `orbital_foundry_core`, `cycler_frame`, `torch_cruiser_frame`, `interstellar_precursor_frame`, `mining_barge_frame`, `cryogenic_tanker_frame`, `bulk_cargo_frame`, `outer_system_tanker_frame`, `long_range_survey_frame`, `interplanetary_hauler_frame`, `long_range_destroyer_frame`, `interdictor_destroyer_frame`, `antimatter_interceptor_frame`, `fleet_escort_frigate_frame`, `mobile_starbase_frame`, `warp_cruiser_frame`, `stellar_engineering_shipyard_frame`, `femtotech_industrial_hull`, `metric_destroyer_frame`, `conversion_dreadnought_frame`, `ringworld_spine_frame`.
- **295 ship module definitions** in `assets/data/ship_modules.ron`. All 295 set both `required_tech` (visibility) and `required_component_design` (engineering project) — the runtime loader is not tolerant of a missing engineering key.
- **21 `ShipModuleCategory` variants** in `src/shipbuilding/types.rs`: 12 consolidated (the canonical Aurora-style taxonomy) and 9 legacy sub-categories retained for backward compatibility with existing RON data. The 12 consolidated variants are the target vocabulary for **new** hull `slot_layout` categories and **new** module `category` fields.
- Ship progression is organized around **six propulsion eras**: **Chemical → Fission / NTR → Gas-Core / Early Fusion → Fusion Torch → Antimatter → Interstellar / Warp**. The Warp era is the v0.6 gateway: it unlocks the closed-timelike-curve engineering targets (`warp_mechanics`, `warp_tactics`, `warp_interdiction`) and the interstellar/warp hull set described below.

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

The hull set has 32 entries across six propulsion eras. Tiers in `assets/data/ship_hulls.ron` run from `tier: 1` (Chemical probes) through `tier: 8` (Ringworld Spine). Stations are still tagged with `is_station: true` instead of a tier where the field is omitted; tier is also set on station entries for sort order. Hull `required_tech` gates the spaceframe; module-family gates are independent on each module entry.

### Chemical era (tier 1)

| Hull id | Display name | Class | Required tech |
|---|---|---|---|
| `micro_probe_frame` | Microsat Probe Bus | ResearchVessel | `chemical_spaceframes` |
| `small_probe_frame` | Deep Survey Probe | ResearchVessel | `chemical_spaceframes` |
| `courier_frame` | Relay Courier Probe | ResearchVessel | `chemical_spaceframes` |
| `courier_vessel_frame` | Orbital Courier Hull | Courier | `basic_space_tech` |
| `lander_frame` | Planetary Lander Bus | ResearchVessel | `chemical_spaceframes` |
| `probe_carrier_frame` | Survey Carrier Stage | ResearchVessel | `chemical_spaceframes` |
| `patrol_frigate_frame` | Patrol Frigate Hull | Frigate | `basic_military` |
| `freighter_frame` | Heavy Orbital Freighter | Freighter | `chemical_spaceframes` |
| `mining_barge_frame` | Mining & Refinery Barge Hull | Freighter | `chemical_spaceframes` |

### Fission / NTR era (tier 1–2)

| Hull id | Display name | Class | Required tech |
|---|---|---|---|
| `fighter_frame` | Orbital Interceptor Hull | Frigate | `orbital_assembly_heavy` |
| `frigate_frame` | Escort Frigate Hull | Frigate | `orbital_construction` |
| `cryogenic_tanker_frame` | Cryogenic Tanker Hull | Freighter | `orbital_construction` |
| `orbital_foundry_core` | Orbital Yard Core | Station | `orbital_construction` |

### Gas-Core / Early Fusion era (tier 2–3)

| Hull id | Display name | Class | Required tech |
|---|---|---|---|
| `destroyer_frame` | Line Destroyer Hull | Destroyer | `carbon_nanotube_frames` |
| `cycler_frame` | Cycler Superstructure | Cruiser | `carbon_nanotube_frames` |
| `bulk_cargo_frame` | Industrial Bulk Cargo Hull | Freighter | `carbon_nanotube_frames` |

### Fusion Torch era (tier 3–4)

| Hull id | Display name | Class | Required tech |
|---|---|---|---|
| `torch_cruiser_frame` | Torch Cruiser Hull | Cruiser | `fusion_superstructures` |
| `outer_system_tanker_frame` | Outer-System Cryogenic Tanker | Freighter | `fusion_superstructures` |
| `long_range_survey_frame` | Long-Range Survey Hull | ResearchVessel | `fusion_superstructures` |

### Antimatter era (tier 4–5)

| Hull id | Display name | Class | Required tech |
|---|---|---|---|
| `interstellar_precursor_frame` | Interstellar Precursor Keel | ResearchVessel | `antimatter_containment_structures` |
| `interplanetary_hauler_frame` | Interplanetary Hauler Hull | Freighter | `antimatter_containment_structures` |
| `long_range_destroyer_frame` | Long-Range Destroyer Hull | Destroyer | `antimatter_containment_structures` |
| `interdictor_destroyer_frame` | Interdictor Destroyer Hull | Destroyer | `antimatter_containment_structures` |
| `antimatter_interceptor_frame` | Antimatter Interceptor Hull | Frigate | `antimatter_containment_structures` |
| `fleet_escort_frigate_frame` | Fleet-Escort Frigate Hull | Frigate | `antimatter_containment_structures` |
| `mobile_starbase_frame` | Mobile Starbase Hub | Station | `antimatter_containment_structures` |

### Interstellar / Warp era (tier 5–8)

The Warp era is the v0.6 gateway. It unlocks the closed-timelike-curve engineering targets (`warp_mechanics`, `warp_tactics`, `warp_interdiction`), the stellar-engineering / femtotech / metric-engineering industrial chain, and the megastructure hull class (`ringworld_spine_frame`). The era's phase-angle and ΔV envelope for cross-system Hohmann transfers is parameterised in [`assets/data/interstellar_propulsion.ron`](../assets/data/interstellar_propulsion.ron) (loaded into `InterstellarPropulsionPolicy` in `src/fleets/data.rs`) — the AI planner uses ±15° / 1.20 margin, the human player gets ±45° / 1.05 margin.

| Hull id | Display name | Class | Required tech |
|---|---|---|---|
| `warp_cruiser_frame` | Warp-Era Cruiser Hull | Cruiser | `warp_mechanics` |
| `stellar_engineering_shipyard_frame` | Stellar Engineering Shipyard Hull | Station | `stellar_engineering` |
| `femtotech_industrial_hull` | Femtotech Industrial Frame | Freighter | `femtotechnology` |
| `metric_destroyer_frame` | Metric-Era Capital Destroyer Hull | Destroyer | `metric_engineering` |
| `conversion_dreadnought_frame` | Conversion-Era Dreadnought Hull | Cruiser | `conversion_tech` |
| `ringworld_spine_frame` | Ringworld Spine Frame | Station | `ringworld_engineering` |

**Warp-era hulls of note:**

- **`warp_cruiser_frame`** — the era's signature capital combatant; `warp_mechanics` is the hull-construction gate.
- **`interstellar_precursor_frame`** — listed in the Antimatter era table above; the antimatter drive is the propulsion that *gets the hull to the warp threshold*. The Hull-construction tech on the warp side (`warp_mechanics`) and the propulsion tech on the antimatter side (`antimatter_propulsion`) are intentionally decoupled.
- **`antimatter_interceptor_frame`** — also antimatter era; the fast escort that screen warp-era fleets during the warp-transition window.
- **`mobile_starbase_frame`** — the deployable mobile forward base that the warp fleets operate from before they reach the target system. Antimatter-era `required_tech` because it is built on the antimatter reactor / drive family, but it is conceptually the warp fleet's forward logistics hub.
- **`stellar_engineering_shipyard_frame`** — Station class; the in-system shipyard required to assemble femtotech / metric-engineering / conversion hulls because those structures cannot be launched from a planetary surface.
- **`femtotech_industrial_hull`** — the freight / industrial backbone of the warp economy; built around femtometer-scale manufacturing.
- **`ringworld_spine_frame`** — the era's megastructure-class Station hull; gates with `ringworld_engineering`.

Slot compatibility is enforced through `slot_layout` category and size matching in the shipbuilding data loader and UI. Hulls in the same era share the same propulsion-era hull-construction gate (`required_tech`); module families are gated independently on each module entry.

## Propulsion Eras

**Six propulsion eras** define the progression curve. Each era unlocks a coordinated set of hulls, drives, reactors, and slot families. The technology in `unlocks_engineering` for the era's flagship drive must remain a single shared engineering target so all module variants in that family unlock through one engineering project.

| Era | Hulls | Flagship drive tech | Sample engineering target | Hull-construction tech |
| --- | --- | --- | --- | --- |
| **Chemical** | `micro_probe_frame`, `small_probe_frame`, `courier_frame`, `courier_vessel_frame`, `lander_frame`, `probe_carrier_frame`, `patrol_frigate_frame`, `freighter_frame`, `mining_barge_frame` | `chemical_rockets` / `advanced_chemical_rocket` | `standard_chemical_rocket` | `chemical_spaceframes`, `basic_space_tech`, `basic_military` |
| **Fission / NTR** | `fighter_frame`, `frigate_frame`, `cryogenic_tanker_frame`, `orbital_foundry_core` | `fission_power` + `nerva_drive` / `kiwi_drive` | `fission_pile`, `nerva_drive` | `orbital_construction`, `orbital_assembly_heavy` |
| **Gas-Core / Early Fusion** | `destroyer_frame`, `cycler_frame`, `bulk_cargo_frame` | `gas_core_fission`, `ion_drive` | `gas_core_fission`, `ion_drive` | `carbon_nanotube_frames` |
| **Fusion Torch** | `torch_cruiser_frame`, `outer_system_tanker_frame`, `long_range_survey_frame` | `fusion_torch` | `fusion_torch` | `fusion_superstructures` |
| **Antimatter** | `interstellar_precursor_frame`, `interplanetary_hauler_frame`, `long_range_destroyer_frame`, `interdictor_destroyer_frame`, `antimatter_interceptor_frame`, `fleet_escort_frigate_frame`, `mobile_starbase_frame` | `antimatter_propulsion` | `antimatter_drive` | `antimatter_containment_structures` |
| **Interstellar / Warp** | `warp_cruiser_frame`, `stellar_engineering_shipyard_frame`, `femtotech_industrial_hull`, `metric_destroyer_frame`, `conversion_dreadnought_frame`, `ringworld_spine_frame` | `warp_mechanics` (closed-timelike-curve drive) | `warp_drive`, `femtotech_core`, `metric_drive`, `conversion_torch` | `warp_mechanics`, `stellar_engineering`, `femtotechnology`, `metric_engineering`, `conversion_tech`, `ringworld_engineering` |

The Warp era is the v0.6 gateway: it unlocks the closed-timelike-curve engineering targets and the interstellar/warp hull set. The era's cross-system Hohmann policy — phase-angle tolerances and ΔV margins for transfers to non-Sol star systems — lives in [`assets/data/interstellar_propulsion.ron`](../assets/data/interstellar_propulsion.ron) and is loaded by `src/fleets/data.rs` into the `InterstellarPropulsionPolicy` resource. Modders can widen tolerances to make AI launches easier, tighten margins to make player launches stricter, or alter the AI defaults to give the planning system more slack — see the header comment in that file.

Within an era, hull `required_tech` controls whether the hull class is even visible. Module `required_tech` controls module visibility, and module `required_component_design` selects the engineering project that must be completed before any module in that family can be installed. **All ship modules in the current RON set both fields** — the runtime loader is not tolerant of a missing `required_component_design`, and new module entries should follow that rule.

## Logistics & Survey Hulls (added in GRA-9)

Five hulls introduced in GRA-9 are dedicated to industrial and exploration roles rather than direct combat:

- **`mining_barge_frame`** — twin ISRU bays + cargo bay for atmosphere / regolith processing; unifies mining-head, regolith-processor, and habitat-module slots under the `ConstructionISRU` family
- **`cryogenic_tanker_frame`** — three-tank depot-to-depot propellant logistics for the inner system
- **`bulk_cargo_frame`** — CNT-framed interplanetary freighter with three cargo bays; carries prefab modules, machinery, and bulk consumables
- **`outer_system_tanker_frame`** — three large cryo tanks, torch-rated reactor / radiator bay, and a long-endurance habitat with an embedded medical bay for multi-month trans-Jovian cruises
- **`long_range_survey_frame`** — gives the `SpecialScience` slot family a second hull so the category is no longer a single-purpose island on the interstellar precursor

These hulls use the same `slot_layout` discipline as the existing frames; the `position` authoring rule below applies equally to them.

## Interstellar / Warp Hulls (v0.6)

Seven hulls make up the warp-era set. They are listed in the Hull Summary → "Interstellar / Warp era" table above; the descriptions below explain the gameplay role and how they hand off to other systems.

- **`warp_cruiser_frame`** — the era's signature capital combatant; gates on `warp_mechanics`. Crewed warp-capable cruiser that transitions between Sol and the nearest 60 star systems using the policy in [`assets/data/interstellar_propulsion.ron`](../assets/data/interstellar_propulsion.ron). The hull *requires* an antimatter reactor in the engineering slot — see `interstellar_precursor_frame` below for the precursor role.
- **`interstellar_precursor_frame`** — antimatter-era precursor keel; `antimatter_containment_structures`. The first interstellar-capable research platform, and the engineering path through which the antimatter drive family matures. It does the interstellar survey / first-arrival work that opens up the warp era's colonisation targets.
- **`antimatter_interceptor_frame`** — antimatter-era frigate; `antimatter_containment_structures`. Fast escort that screens warp-era fleets during the warp-transition window (the high-DV cruise phase before the warp field can be established).
- **`mobile_starbase_frame`** — antimatter-era Station class; `antimatter_containment_structures`. Deployable forward logistics hub that warp fleets operate from before they reach the target system. Conceptually bridges the antimatter era (which built it) and the warp era (which uses it).
- **`stellar_engineering_shipyard_frame`** — Station class; `stellar_engineering`. The in-system shipyard required to assemble femtotech / metric-engineering / conversion hulls because those structures cannot be launched from a planetary surface. Must be deployed in-system before any femtotech / metric / conversion hull can begin construction.
- **`femtotech_industrial_hull`** — Freighter class; `femtotechnology`. The freight / industrial backbone of the warp economy; built around femtometer-scale manufacturing. Required to keep metric- and conversion-era hulls supplied with raw stock.
- **`metric_destroyer_frame`** — Destroyer class; `metric_engineering`. Metric-era capital combatant.
- **`conversion_dreadnought_frame`** — Cruiser class; `conversion_tech`. Conversion-era super-capital combatant.
- **`ringworld_spine_frame`** — Station class; `ringworld_engineering`. The era's megastructure hull.

> Authoring rule: the era's hull-construction techs (`warp_mechanics`, `stellar_engineering`, `femtotechnology`, `metric_engineering`, `conversion_tech`, `ringworld_engineering`) all gate *spaceframes*; the propulsion-side gates (`antimatter_propulsion`, `warp_mechanics` again as the drive family, `metric_drive`, `conversion_torch`) are separate `unlocks_engineering` entries on the propulsion techs. See the Propulsion Eras table above for the era's flagship drive techs and sample engineering targets.

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
