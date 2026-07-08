# Transfer Planner Harmonisation

**Issue:** [GRA-367](https://github.com/Slatibartfas/Helios-Ascension/issues/367)
**Status:** Approved by operator 2026-07-08 with Phase 3 amendment (short-hop option count must be configurable, not hard-coded to 3)
**Scope:** UX + algorithm unification across all transfer types
**Revision:** r2 — Phase 3 amended per operator feedback (`short_hop_options` RON key, default 5, range 3–9)

## Motivation

Helios ships six coexisting transfer-selection UIs, all of which feed the same `process_fleet_actions` consumer but disagree on what the player is choosing between:

| Transfer class | Current UI | t_dep sweep? | TOF sweep? | GA option? | Δv tool? |
|---|---|---|---|---|---|
| Interplanetary (planet → planet) | Porkchop grid (`porkchop_panel.rs`) | ✓ | ✓ | separate collapsible | cell colour |
| Moon (e.g. Earth → Moon) | 3-option row (`orbital_mechanics.rs:498`) | ✗ | ✗ | ✗ | Δv row |
| Gravity assist | Collapsible candidates (`transfer_planner.rs:5110-5263`) | ✗ | ✗ | ✓ | savings row |
| Star approach (planet → parent star) | StarApproach picker + slider (`transfer_planner.rs:2766-2854`) | partial | partial | ✗ | Δv row |
| Inter-star (binary system hop) | Single-cell cross-system grid (`try_build_cross_system_hohmann`) | ✗ | ✗ | ✗ | Δv cell |
| Interstellar | Single-cell kinematic (`kinematic_transfer_options`) | ✗ | partial | ✗ | Δv row |

The "3 speeds" picker (Earth-Moon and other short hops) is explicitly labelled in code as a fallback waiting for the porkchop to subsume it (`orbital_mechanics.rs:525-528`). Gravity assists offer Δv savings but no time-of-flight search. Interstellar and inter-star paths already use porkchop-shaped data but present only one cell. The result is six UIs, four option-card formats, three commit flows, and two source-of-truth bugs documented in `memories/repo/porkchop-ui-polish.md`.

**Goal:** one planner, one option model, one commit flow — with each transfer class surfaced through the same panel via per-class data, not per-class UI.

## Research notes (training data — flagged for operator verification)

> **Citation-hygiene note.** WebSearch and WebFetch were unreliable during the research pass (placeholder domains, no-response, ECONNREFUSED). The references below are from prior training data and must be verified before being cited in any external doc, PR, or comment. The architectural recommendations stand on their own.

- **`lambert` crate (crates.io)** — Rust Lambert-problem solver. License and repo URL to be confirmed; commonly cited Izzo (2015) "Revisiting Lambert's Problem" implementation. [`training-data, unverified`]
- **`nyx-space` crate (crates.io)** — Rust astrodynamics framework: Lambert, porkchop, orbit propagation, TLE import, OD. License: Apache-2.0 (to verify). Maintainer: brheart. [`training-data, unverified`]
- **`hifitime` crate (crates.io)** — Rust time/epochs library used by `nyx-space`. License: Apache-2.0 (to verify). [`training-data, unverified`]
- **`poliastro` (Python, MIT)** — established astrodynamics library with Lambert, porkchop, porkchop-plot CLI, low-thrust transfers. Author: Juan Luis Cano Rodríguez et al. [`training-data, unverified`]
- **`porkchop` by ChristopherRabotin (GitHub)** — Rust CLI for porkchop generation, referenced in `nyx-space` ecosystem. License: to verify. [`training-data, unverified`]
- **NASA Trajectory Browser (trajbrowser.nasa.gov)** — interactive porkchop plotter across all solar-system bodies; controls: departure date slider, arrival date slider, C3 colour axis, time-of-flight contour overlay, "minimum-Δv" auto-snap.
- **GMAT (NASA, open source)** — mission analysis toolkit. Porkchop generator + Hohmann + bi-elliptic + lunar gravity assist + low-thrust. License: NOSA.
- **KSP (Squad, proprietary)** — maneuver-node UI: prograde/retrograde/normal/antinormal handles + planned-burn timeline. No Δv vs time-of-flight porkchop; everything is impulse-plan with an encounter planner overlay.
- **Stellaris (Paradox, proprietary)** — fleet-movement is abstracted to a single cost number + pathfinding. No real planner.

**Architectural takeaway:** every mature mission-planner (NASA Trajectory Browser, GMAT, poliastro) treats the porkchop as the canonical selection surface and renders "shortcut" transfers (Hohmann-only, single-point interstellar) as degenerate porkchops with one visible cell. The shortcut UIs in Helios are an unintentional divergence, not a deliberate alternative.

## Target architecture

### 1. One `TransferPlan` data shape

Replace the implicit "which UI is currently active" state machine with a single value:

```rust
#[derive(Resource, Default)]
pub struct TransferPlan {
    pub source: SelectionSource,
    pub selected: Option<SelectedTransfer>,
    pub preview: Option<PlanPreview>,
    pub commit: Option<PlannedTransfer>,
}

pub enum SelectionSource {
    Empty,
    Porkchop { grid: PorkchopGrid, selected: (usize, usize) },
    GravityAssist { candidate: GravityAssistEntry, dep_window: DepWindow },
    BodyHohmann { option: TransferOption, dep_window: DepWindow },
    Interstellar { option: KinematicOption, distance_ly: f32 },
    CrossStar { grid: CrossSystemGrid, selected: (usize, usize) },
}

pub struct SelectedTransfer {
    pub t_dep_s: f64,
    pub tof_s: f64,
    pub total_dv_ms: f64,
    pub legs: Vec<Leg>,         // 1 leg for direct, 2 for GA, 1 kinematic for interstellar
    pub transfer_orbit: Option<KeplerOrbit>,
}
```

**Why one struct.** Today the panel branches on `if porkchop_grid.is_some() / if selected_gravity_assist.is_some() / if target_star_system.is_some() ...`. Each new transfer class adds a new branch. One `TransferPlan` makes the panel render against a single shape and lets the data layer drive the view.

**Migration cost.** `FleetUiState` (`src/ui/mod.rs:209-400`) holds ~25 fields across all six sources. A `TransferPlan` reduces this to 4 fields plus a 5-variant `SelectionSource`. The `clear_target()` / `select_lagrange_target()` invalidation helpers stay; they just patch `TransferPlan.source` instead of zeroing one of six option slots.

### 2. Algorithm unification

All transfer classes reduce to one of three primitives:

| Primitive | Used by | Status today |
|---|---|---|
| `solve_lambert_transfer(r1, r2, t_dep, tof, gm)` | porkchop, cross-star, inter-star | already shared (`orbital_mechanics.rs`) |
| `compute_gravity_assist(r1, r_fly, r2, gm, gm_planet)` | GA | already pure (`orbital_mechanics.rs:243`) |
| `kinematic_transfer_options(distance, accel, max_dv)` | interstellar | already pure (`orbital_mechanics.rs`) |

The three are already pure functions. The unification is the **driver**, not the math: one function per transfer class that sweeps the relevant dimensions and emits `Vec<TransferOption>` (or `PorkchopGrid` for interplanetary).

#### GA: extend to a 2-D grid

Today a GA shows a single `(body, periapsis, savings)` tuple per flyby candidate. Extend `find_gravity_assist_options` to sweep `(t_dep, tof)` and emit a `(t_dep, tof, total_dv, v_inf_arrival)` grid — same shape as `PorkchopGrid`, just rendered with GA-aware axes (Δv savings vs synodic phase). The math is the same patched-conic two-leg Lambert solve, parameterised over `t_dep` and `tof` per leg.

Implementation outline (consistent with `porkchop.rs:131`):
1. Extend `GravityAssistOption` with `t_dep_s`, `tof_s` (already partial today — `:197-229`).
2. `sweep_gravity_assist_grid(r1, r_fly, r2, gm, gm_planet, dep_window, tof_bounds) -> Vec<PorkchopCell>`.
3. Reuse the `porkchop_panel.rs` renderer. Add a `grid_kind` discriminator so the colormap can label Δv_savings instead of total Δv for GA grids.

**Effort:** ~150 LOC + 1 RON entry in `porkchop_config.ron` for GA window/TOF defaults (e.g. 60-day window, 0.4×–2.5× Hohmann, 40×30 resolution).

#### 3-speeds → configurable short-hop porkchop (Phase 3, amended r2)

`calculate_transfer_options_phased` (legacy 3-option row) becomes a call into a new `build_short_hop_grid(r1, r2, gm, delta_i_rad, n_options) -> PorkchopGrid`. **Per operator feedback (2026-07-08), the option count is not hard-coded to 3** — it is driven by a new RON key `short_hop_options` in `porkchop_config.ron`'s `category_overrides` block, default **5** (range 3–9, soft-clamped at 11 to protect the per-frame budget). Each row is a labelled preset (`Fastest`, `Cheapest`, `Balanced`, then named variants like `Fast / Balanced / Slow / Cheap / Conservative` depending on `n_options`); cols = single `t_dep` value because the player has not picked a window. The bar height scales with `n_options`; the renderer caps visual height at 9 rows and scrolls beyond that.

The preset generator is a small helper that produces Δv / TOF pairs spread across the bi-elliptic / Hohmann / direct spectrum — not just a `{x1, x1.5, x2}` multiplier on the Hohmann baseline. Sample `n=5`: `{0.6×T Hohmann, 0.85×T, 1.0×T, 1.4×T, 2.0×T}` paired with the matching Δv. So the player gets **variety**, not just three copies of the same transfer at different speeds.

**Effort:** ~110 LOC + 1 RON entry + delete the legacy `selectable_label` row at `transfer_planner.rs:5737-5941` (~200 LOC savings).

#### Interstellar + cross-star → degenerate 1×1 porkchop

Already implemented as `try_build_cross_system_hohmann` (single cell). Refactor to return a `PorkchopGrid` with 1×1 cells so the renderer doesn't need a special branch.

**Effort:** ~30 LOC + remove the `if is_interstellar { ... }` branch in the render block.

### 3. UX unification

#### Single panel layout (always)

```
┌─ Transfer Planner ──────────────────────────────────────────────────┐
│  Target: [dropdown: body / star / star-system / LP / fleet]         │
│  Reference frame: auto | body-local | heliocentric | barycentric    │
├─ Departure window ──────────────────────────────────────────────────┤
│  [   t_dep slider    ]   [   tof slider   ]   "now + 14 d"          │
├─ Option surface (one of: porkchop | GA grid | short-hop bar | ...)  │
│  [eg. Porkchop grid, colormap, click-to-pick cell]                  │
├─ Selected option card ──────────────────────────────────────────────┤
│  Δv total │ fuel │ v(arr) │ v∞(arr) │ legs: [Leg 1 → Leg 2]         │
│  Warn: ⚠ exceeds fleet Δv budget (gap = X km/s)                    │
├─ Gravity assists (collapsible) ─────────────────────────────────────┤
│  Per-candidate: Δv saved │ extra time │ window period │ grid link   │
├─ Confirm ───────────────────────────────────────────────────────────│
│  [ 🚀 Execute Transfer ]  [ ✕ Cancel ]                              │
└──────────────────────────────────────────────────────────────────────┘
```

**Rules that apply to every class:**

1. The "Option surface" always renders a porkchop-shaped grid (rows × cols, possibly degenerate). For moon/short-hop transfers it shows a horizontal bar with `short_hop_options` rows (default 5, RON-tunable 3–9 — **not stuck at 3** per operator feedback). For interstellar it shows a 1×1 cell. The player learns one UI.
2. The "Selected option card" is identical across classes. Δv / fuel / arrival velocity / legs / budget gap.
3. Gravity assists become a sub-grid of the option surface (or a collapsible above it that filters the option surface to "Δv after GA"). Today it's a parallel picker; after unification it's an input to the same picker.
4. The "Execute" button always builds the same `PlannedTransfer` and pushes the same `StartTransferAction`. No per-class commit path.

#### Reference-frame indicator

A new 1-line widget above the option surface shows the auto-resolved frame (`BodyLocal` / `StellarLocal` / `SystemBarycentric`) and lets the player override it. This is the lever for the rare case where the planner picks the "wrong" frame (e.g. cross-system transfer that the player wants treated as barycentric instead of per-star). Today there's no UI; the frame is implicit.

### 4. Dispatcher consolidation

Replace the per-class branch in `render_transfer_planner` (`transfer_planner.rs:2931-4175`) with one function:

```rust
fn plan_for_target(target: &ResolvedTarget, t_dep: DepWindow, tof: TofBounds, frame: FrameOverride)
    -> Result<PorkchopGrid, PlanError>;
```

Internally it dispatches via match on target class:
- `Body(moon) | Body(planet, parent=planet)` → `build_short_hop_grid` (3-row) or full porkchop
- `Body(planet, parent=star)` → `build_porkchop_grid_for_heliocentric` (existing rotating buffer)
- `Body(star)` → `build_star_approach_grid` (extend `try_build_local_porkchop` to planet-orbit-to-star-orbit; today only one orbit is supported)
- `Lagrange(lp)` → porkchop grid in `BodyLocal` frame (existing `try_build_local_porkchop`)
- `Fleet` → intercept grid (GRA-149 C-3, new)
- `StarSystem { id, dist_ly }` → degenerate `PorkchopGrid` (1×1) populated by `try_build_cross_system_hohmann`
- `Star(star_in_other_system)` → barycentric cross-star grid (extend `fitted_cross_star_ballistic_options` into a grid)

**Total reduction:** ~600 LOC of per-class branch code collapses to ~80 LOC of dispatch + 6 small pure fns.

## Phased delivery

Six children, ordered by risk × payoff. Each ships behind the same data shape so the panel can land first and the algorithms follow.

### Phase 1 — Skeleton + frame indicator (no behavior change)

1. Add `TransferPlan` resource + `SelectionSource` enum to `src/fleets/components.rs`.
2. Mirror all `FleetUiState` transfer fields into `TransferPlan` (read-write both ways, no behaviour change).
3. Render the new 1-line reference-frame indicator above the existing picker.
4. Test: existing `porkchop_grid_pending_rebuild`, `cross_system_grid_built_for`, `fleet_ui_state_clear_target_drops_porkchop_grid` tests stay green.
5. **LOC delta:** +180, -0.

### Phase 2 — Selected-option card unification

1. Extract `build_selected_card(ui, &TransferPlan) -> CardWidget` shared across all branches.
2. Wire it into porkchop, legacy-3-option, GA, interstellar, cross-star branches.
3. Test: snapshot-test one card per class to lock the layout.
4. **LOC delta:** +50, -200.

### Phase 3 — Short-hop grid (3-speeds → configurable porkchop) (amended r2)

1. `build_short_hop_grid(r1, r2, gm, delta_i_rad, n_options: usize) -> PorkchopGrid` (`n_options` rows × 1 col).
2. RON entry `category_overrides.short_hop` in `porkchop_config.ron` with new field `short_hop_options: 5` (clamped 3–9).
3. Delete `selectable_label` row at `transfer_planner.rs:5737-5941` (~200 LOC).
4. Test: regression test that Earth → Moon still completes the planner's "cheapest within 10%" predicate; plus unit test that `build_short_hop_grid` with `n_options = 3, 5, 7, 9` returns rows of strictly increasing TOF and decreasing peak Δv.
5. **LOC delta:** +110, -210.

### Phase 4 — GA grid sweep

1. Extend `GravityAssistOption` with `t_dep_s`, `tof_s`.
2. `sweep_gravity_assist_grid(...) -> Vec<PorkchopCell>` reusing `solve_cell` pattern.
3. Render GA candidates as collapsible sub-grids of the option surface.
4. RON entry `category_overrides.gravity_assist` in `porkchop_config.ron`.
5. Test: extend `gravity_assist_earth_mars_via_venus` to verify grid has ≥1 feasible cell.
6. **LOC delta:** +220, -80.

### Phase 5 — Cross-star + interstellar degenerate grid

1. Refactor `try_build_cross_system_hohmann` to return `PorkchopGrid` (1×1).
2. Same for `fitted_cross_star_ballistic_options`.
3. Drop the `is_interstellar` and `is_inter_star_body_transfer` render branches.
4. Test: regression for `binary_system_cross_star_transfer` + `interstellar_proxima_to_sol`.
5. **LOC delta:** +60, -120.

### Phase 6 — Star-approach grid + frame override

1. `build_star_approach_grid(r_planet, r_star, gm_star, parking_options) -> PorkchopGrid` (rows = parking orbit, cols = t_dep).
2. Wire `FrameOverride` resource into the dispatcher.
3. Test: snapshot-test one star-approach grid for Sol + parking 0.3 AU.
4. **LOC delta:** +180, -60.

**Total across phases:** +810, -670 = +140 net. Phase 2 alone pays back the cost; phases 3-6 are user-visible wins.

## Decisions baked in (please flag any you want to revisit)

1. **One `TransferPlan` resource replaces `FleetUiState`'s 25 transfer fields.** `FleetUiState` keeps the fleet-list / non-transfer fields. `TransferPlan` owns the active selection only.
2. **The reference-frame indicator becomes a real UI.** Today it's implicit (`resolve_planner_transfer_frame` is internal). Surfacing it is the lever for the rare "wrong frame" complaint.
3. **GA is a grid, not a single option.** Costs ~150 LOC; pays back in "player can pick a better launch window for the assist."
4. **Short-hop transfers render as a configurable horizontal bar** instead of a 3-card row. Default 5 options (RON-tunable, 3–9), varied presets across the Hohmann/bi-elliptic spectrum — not stuck at the legacy 3 speeds. Same commit path, more variety per operator feedback 2026-07-08.
5. **No new dependency.** All unification uses existing Lambert + GA + kinematic helpers. The crate survey (`lambert`, `nyx-space`) is a research artifact — adopting them is out of scope until someone benchmarks a >10× speed-up win.
6. **`porkchop_config.ron` gains 2-3 new `category_overrides`** (`short_hop`, `gravity_assist`, `star_approach`). No schema change. `short_hop` adds a `short_hop_options: 5` field (clamped 3–9) so LGD can tune variety per game phase without touching code.
7. **The `porkchop_grid_pending_rebuild` swap (GRA-169) stays untouched.** It's the right primitive for every grid, including the new ones.
8. **Visualisation continues to be `egui::Painter`, not `egui_plot`.** The LGD contract locked that decision in GRA-152.
9. **No gameplay change.** Every transfer a player could pick today they can still pick, just from one panel.

## Loader / validator invariants (all `#[test]`-able)

1. `TransferPlan` round-trip via Bevy reflection (no `&'static str option_label` regressions).
2. Every per-category override in `porkchop_config.ron` has `resolution_x * resolution_y ≤ 5000` (existing rule).
3. `build_short_hop_grid(n)` returns exactly `n` rows × 1 col (`n` clamped 3–9), feasible iff source solver is feasible; rows have strictly increasing TOF and strictly decreasing peak Δv (monotone spread).
4. `sweep_gravity_assist_grid` returns ≥1 cell iff `find_gravity_assist_options` returns ≥1 candidate.
5. `try_build_cross_system_hohmann` returns a 1×1 grid; same `is_feasible` predicate as today.
6. `clear_target()` resets `TransferPlan` to `Default` (verifies the `porkchop.rs:1815` regression test still passes).
7. Snapshot test: one card per class (porkchop / short-hop / GA / interstellar / cross-star / star-approach) renders the same `CardWidget` schema.

## Open questions for the team

1. **GA grid resolution.** 40×30 = 1200 cells per assist candidate × N candidates per transfer = expensive for the per-frame budget. Acceptable to render at lower resolution (e.g. 20×15) and re-resolve on click? `nyx-space` defaults to 100×100; we can go lower because the GA surface is smoother than the porkchop surface.
2. **Short-hop bar height.** 3 rows is fine for moon transfers; does it scale to "any 2-body transfer where the planner chose 3 preset options"? If the LGD wants the 3 options to remain visually distinct (icon + label), the bar can be 3 fixed-width cards instead of 3 rows.
3. **Frame override UI.** Slider / dropdown / icon-buttons? Each has different panel-space cost.
4. **GA collapsed default.** Today it's collapsed. Keep collapsed (player has to opt in) or expand by default (surface the savings immediately)?
5. **"Δv after best GA" surface.** Should the main option surface show Δv before-or-after the best available GA? Today it's before. Surfacing "after GA" without a click is a 1-line change but it changes the player's mental model.
6. **Cargo.toml.** Any new dep on a Lambert / porkchop crate is out of scope (no autonomous merges per helios-merge-policy). Keep the math we have.

## Risk + rollback

- **Risk: behaviour change.** Phase 1 is a pure refactor; phase 2 unifies card layout (cosmetic). Phase 3 swaps the 3-speeds row for a 3-row porkchop bar — same data, same commit. Players picking Efficient/Moderate/Fast still get the same Δv.
- **Risk: perf.** GA grid is the only path that adds cells per frame. 5 candidates × 600 cells × 0.3 ms = 0.9 s worst case. Phases 1-2 don't add cells; phase 4 does. Mitigation: 20×15 default resolution (vs 40×30).
- **Rollback.** Each phase is its own PR. Bisect to the green one. No phase touches `process_fleet_actions` (the consumer), so commit-flow regressions are isolated to the planner UI.
- **Test plan.** Existing `tests/planner_integration.rs` (10 cases, 907 lines) covers every transfer class. Every phase keeps that suite green. New unit tests pin the per-class card snapshots.

## Proposed children

| ID | Title | Phase | Owner | Blocks |
|---|---|---|---|---|
| GRA-367-A | `TransferPlan` skeleton + frame indicator | 1 | CTO (this PR) | B, C, D, E, F |
| GRA-367-B | Selected-option card unification | 2 | Coder | C, D, E, F |
| GRA-367-C | Short-hop grid (3-speeds → porkchop) | 3 | Coder | – |
| GRA-367-D | GA grid sweep + collapsible sub-grids | 4 | Coder | – |
| GRA-367-E | Cross-star + interstellar degenerate grid | 5 | Coder | – |
| GRA-367-F | Star-approach grid + frame override UI | 6 | Coder + LGD | – |
| GRA-367-RON | RON config additions (`category_overrides.short_hop`, `gravity_assist`, `star_approach`) | spans 3-6 | LGD | C, D, F |

CTO owns GRA-367-A + this design doc + the dispatch refactor. Coder owns B-F. LGD owns RON.

## References

- Helios docs: `docs/design/ASTEROID_ENTITIES.md` (style reference), `docs/design/MULTI_STAR_SYSTEMS.md` (cross-system precedent).
- Helios memories: `memories/repo/porkchop-ui-polish.md` (source-of-truth bug), `memories/repo/porkchop-adaptive-tof-and-resolution.md` (Lambert cost data), `memories/repo/transfer-preview-unification.md` (3-D arc predecessor).
- GitHub issues: GRA-152 (initial porkchop), GRA-153 (planner fixes), GRA-154 (porkchop wire-in + L-4 fallback), GRA-159 (body-path wire-in), GRA-165 (GA↔porkchop guard), GRA-167 (local-frame Lambert), GRA-169 (rotating-buffer), GRA-326 (Phase-2 auto-dispatch), GRA-328a/b/c (interstellar), GRA-343 (interstellar propulsion policy RON).
- Code anchors: `src/ui/transfer_planner.rs:1185` (entry), `src/ui/porkchop_panel.rs:42` (renderer), `src/fleets/porkchop.rs:131` (grid builders), `src/fleets/orbital_mechanics.rs:498` (3-speeds fallback), `src/ui/mod.rs:209` (FleetUiState).
- External (training data, unverified — verify before citing in PR): `lambert` crate, `nyx-space` crate, `hifitime` crate, `poliastro` (Python), `porkchop` by ChristopherRabotin, NASA Trajectory Browser, GMAT.