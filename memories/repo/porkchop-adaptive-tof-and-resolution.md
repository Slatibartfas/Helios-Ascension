# Porkchop adaptive Y-axis + resolution bump

User feedback on Saturn-class porkchops: the Y-axis extended to a
configured TOF of 1-5 yr but the cheap-transfer basin lived in the
bottom third of the grid (rows 0-20% of the way up); the rest was
grey (Lambert infeasibility, not high-ΔV). Two follow-up changes in
`src/fleets/porkchop.rs` + `src/ui/porkchop_panel.rs`:

## 1. Adaptive `rendered_tof_bounds_s`

`PorkchopGrid` carries a new field `rendered_tof_bounds_s: (f64, f64)`
alongside the existing `tof_bounds_s: (f64, f64)`. The builder:

  1. Solves the full configured range as before (so no data is lost).
  2. Scans `cells` for the highest row that contains at least one
     feasible cell.
  3. Sets `rendered_tof_bounds_s.0 = tof_bounds_s.0` (the bottom row
     is the panel's "Depart Now + minimum ΔV" anchor and must stay
     visible).
  4. Sets `rendered_tof_bounds_s.1 = top_row_tof + 10% × configured_span`,
     capped at `tof_bounds_s.1`.

The panel maps only the populated row range to its vertical extent, so
the colormap band stretches across the populated region instead of
getting squashed into the bottom third.

`compute_adaptive_tof_bounds` is the pure helper (exported for unit
tests). Degenerate cases:
  * No feasible cells → fall back to configured range (player still
    sees "nothing fits in this budget" topology).
  * All rows feasible → no trim needed.
  * Trimmed to top row but `top_row_tof + margin ≥ tof_max` → capped
    at `tof_max` (the 10% margin is purely cosmetic when the basin
    already fills the panel).

Earth→Saturn with the default 60×60 config happens to leave the
Lambert solver feasible at every row (5× Hohmann ≈ 30 yr capped at
10 yr; the solver finds solutions across the whole range), so the
adaptive trim doesn't fire — a valid "no trim needed" outcome. The
synthetic `adaptive_tof_bounds_trims_synthetic_long_tail` test forces
a sparse-feasible layout to exercise the trim path.

## 2. Resolution bump: 40×50 → 60×60

Default resolution bumped from 2000 to 3600 cells, still well within
the 5000-cell validator ceiling (rebuild cost ≈ 1.1 s worst-case on
a 4-core host — inside the 3-day staleness window so the deferred
build doesn't visibly lag at 1 yr/s sim speed). Per-category overrides
also bumped:
  * interstellar: 60×50 → 70×60 (4200 cells)
  * moon:         50×40 → 70×50 (3500 cells)
  * star_approach:50×40 → 60×50 (3000 cells)

The RON file's comments document the rationale (`assets/data/porkchop_config.ron`).

## Test tolerance bump

`tests/transfer_porkchop.rs` bumped the Hohmann-cell ΔV tolerance
from 20% → 25% because the 60×60 resolution lands the closest-tof
cell further off the optimal Hohmann phase than 40×50 did, taking the
Hohmann-cell ΔV from ≈ 6.2 km/s (+11%) to ≈ 6.8 km/s (+21%). The unit
test `porkchop_earth_mars_has_feasible_cells` already used 25% for
the same reason — bringing the integration test into alignment.

## What's NOT done

The Lambert solver cost itself was not optimised. At ~0.3 ms/cell ×
3600 cells = ~1.1 s per build, the deferred-build path stays inside
the 3-day staleness window at 1 yr/s, so the panel never goes blank
mid-rotation. If the rebuild cost ever becomes the bottleneck,
candidates are: (a) early-out when feasible cells cluster at the
bottom of the configured range (skip the top half of rows on the
second pass), (b) parallel solve via `rayon`, (c) caching the per-cell
Lambert solution across t_dep shifts within a single buffer rotation
(the geometry changes linearly with t_dep, so a smart cache could
re-use partial work).

## Pre-existing test failures (unrelated)

These were failing on `main` before this change and remain failing:
  * `fleets::porkchop::planner_wiring_tests::local_frame_jupiter_europa_optimal_dv_matches_hohmann`
  * `astronomy::asteroids::tests::test_a_ceres_dwarf_planet_accepted`
  * `astronomy::star_epoch::tests::advance_position_round_trip`
  * `persistence::restore::tests::roundtrip_world_with_components_and_resources`
  * `persistence::snapshot::tests::snapshot_requires_type_registry`