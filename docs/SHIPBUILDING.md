# Shipbuilding

## Overview

Helios Ascension's shipbuilding system is data-driven and currently centered on a single canonical set of ship assets:

- `assets/data/ship_hulls.ron` defines hull frames and slot layouts.
- `assets/data/ship_modules.ron` defines ship modules and their gameplay stats.
- `assets/data/technologies.ron` defines the research unlock path that gates advanced modules.

The repository intentionally uses those files as the source of truth. Temporary or generated `ship_modules*.ron` snapshots should not be checked in alongside the canonical data.

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

## Data Authoring Rules

When adding or editing ship content:

1. Update only the canonical files in `assets/data/`.
2. Keep module IDs unique. Duplicate IDs silently collapse during loading because modules are indexed by ID.
3. Keep RON tuple structure exact. Missing `),` separators will break deserialization.
4. Use only valid `ResourceType`, `ShipModuleCategory`, `ShipClass`, and `PropulsionType` enum values.
5. If a new module requires research, add or update the matching technology entry in `assets/data/technologies.ron`.
6. Update this document and `.github/copilot-instructions.md` when the data model or workflow changes.

## Technology Coupling

Module unlocks currently rely on `required_tech` in `assets/data/ship_modules.ron` and matching technology IDs in `assets/data/technologies.ron`.

For ship-related content, the intended workflow is:

1. Add or update the technology in `assets/data/technologies.ron`.
2. Add or update the module in `assets/data/ship_modules.ron`.
3. Ensure the module's `required_tech` matches the technology ID.
4. Validate with `cargo build` and a short `cargo run` to catch runtime RON parsing errors.

## Validation

Recommended validation commands after shipbuilding data changes:

```bash
cargo build
cargo run
```

`cargo build` catches Rust-side issues. `cargo run` is still required because malformed RON, invalid enum variants, and duplicate IDs only surface during data loading at runtime.
