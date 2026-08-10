---
name: palette-unification
description: CYAN is the canonical primary accent. ACCENT is deprecated. Phase 1 (2026-08-10) commits this decision.
metadata:
  type: project
---

# Palette unification - CYAN wins (2026-08-10)

## Decision

bevy_theme::CYAN is the canonical primary accent for the entire codebase. The egui
theme::Color::ACCENT is deprecated and will be removed in a future release.

## Canonical RGB

- bevy_theme::CYAN (the source of truth): Color::srgba(0.373, 0.784, 0.847, 1.0)
- theme::Color::CYAN (new, 2026-08-10): same RGB. Re-exports or duplicates
  bevy_theme::CYAN. Lives in src/ui/theme.rs.
- theme::Color::ACCENT (legacy, deprecated): Color::srgb(0.0, 0.949, 1.0).
  Visually similar cyan but numerically distinct. Deprecated 2026-08-10.

## Why CYAN wins

1. bevy_theme::CYAN is the only primary accent used by bevy_ui code today. The egui
   theme had a parallel ACCENT because egui code needed its own copy.
2. The visual delta from ACCENT (srgb 0.0, 0.949, 1.0) to CYAN (srgba 0.373, 0.784, 0.847, 1.0)
   is intentional: more blue, less green. The egui theme was leaning toward pure neon cyan;
   the bevy_theme version is a more muted blue-leaning cyan that reads better against the
   dark background.

## Migration timeline

- Phase 1 (2026-08-10, this commit): Migrated 16 callers of theme::Color::ACCENT in
  src/ui/shipbuilding_workspace.rs to theme::Color::CYAN. The visual delta will be visible
  on the next run.
- Phase 9 (planned): Retire the egui-side ACCENT entirely. Declare it removed (not just
  deprecated). Sweep the deprecated attribute warnings.
- Future palette additions: Add new tokens to bevy_theme.rs FIRST. Only mirror to
  theme.rs if egui code actually needs the token. Do not duplicate colors across the two
  systems.

## How to apply

- When adding a new accent-colored element: use bevy_theme::CYAN for bevy_ui code, or
  theme::Color::CYAN for egui code (which now re-exports the same RGB).
- Do NOT introduce new uses of theme::Color::ACCENT. The compiler will warn you.
- If you need a different shade (lighter/darker/with alpha), derive it from bevy_theme::CYAN
  with Color::srgba adjustments - do NOT define a new palette constant.
