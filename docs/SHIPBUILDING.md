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

- 9 hull definitions in `assets/data/ship_hulls.ron`
- 69 ship module definitions in `assets/data/ship_modules.ron`
- 12 consolidated ship module categories in `src/shipbuilding/types.rs`
- Tiered module progression currently implemented through tier 5 in the module data

## Module Categories

The active ship module categories are:

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

## Hull Summary

The hull set currently includes four tier-1 probe and small-craft frames at the low end:

- `micro_probe_frame`
- `small_probe_frame`
- `courier_frame`
- `lander_frame`

Additional hulls scale upward into combat, logistics, and station roles. Slot compatibility is enforced through `slot_layout` category and size matching in the shipbuilding data loader and UI.

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

For high-quality blueprint layouts, prefer authored `position` data in the hull definitions.

## Data Authoring Rules

When adding or editing ship content:

1. Update only the canonical files in `assets/data/`.
2. Keep module IDs unique. Duplicate IDs silently collapse during loading because modules are indexed by ID.
3. Keep RON tuple structure exact. Missing `),` separators will break deserialization.
4. Use only valid `ResourceType`, `ShipModuleCategory`, `ShipClass`, and `PropulsionType` enum values.
5. If a new module requires research, add or update the matching technology entry in `assets/data/technologies.ron`.
6. If multiple modules share one engineering target, author `required_component_design` so the family unlocks through a single engineering project.
7. Ensure the owning technology exposes that engineering target through `unlocks_engineering`; tech tooltips and engineering availability depend on that coupling.
8. Update this document and `.github/copilot-instructions.md` when the data model or workflow changes.
9. If a hull should display cleanly in the native blueprint workspace, add `position` data to `slot_layout` entries instead of relying on heuristic placement.

## Technology Coupling

Ship module progression now has two explicit authored links:

1. `required_tech` controls when the module family is even visible.
2. `required_component_design` selects the engineering project that must be completed before any module in that family can be installed.

When a family target is not explicitly authored in the `components` array, the runtime synthesizes the engineering definition from ship module data. That synthesis still depends on `unlocks_engineering` in `assets/data/technologies.ron` so the tech tree and Available Engineering tab expose the project correctly.

For ship-related content, the intended workflow is:

1. Add or update the technology in `assets/data/technologies.ron`.
2. Add or update the module in `assets/data/ship_modules.ron`.
3. If the module belongs to an existing family, point `required_component_design` at that family target instead of inventing a parallel unlock path.
4. Ensure the module's `required_tech` and the technology's `unlocks_engineering` entry refer to the same progression step.
5. Validate with `cargo build` and a short `cargo run` to catch runtime RON parsing errors.

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
