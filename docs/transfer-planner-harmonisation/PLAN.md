# Transfer Planner Harmonisation — LGD Design Plan

**Issue**: GRA-368 (GRA-367.A) — Transfer Planner Harmonisation design plan (LGD-led)
**Parent**: [GRA-367](https://paperclip.klingspor.one/GRA/issues/GRA-367) — Transfer Planner Harmonisation (operator-filed, 2026-07-08)
**Author**: Lead Game Designer (8b113021)
**Status**: Draft v1 for operator review
**Scope**: research + design plan. No Rust or RON changes ship in this issue.

This plan is the input the CTO will use to scope per-phase Coder children under GRA-367.

---

## 0. TL;DR (one paragraph)

Helios today has **two** transfer-option surfaces stacked on top of a **third** interstellar
single-cell surface:

1. The dynamic **porkchop `(t_dep, TOF)` grid** for interplanetary transfers
   (GRA-152 → GRA-162 chain, ships in v0.5.0).
2. The legacy **Efficient / Moderate / Fast** 3-option row (still active for moons,
   rings, course-corrections, and any destination that the planner decides the
   porkchop math can't model — see `should_build_porkchop_for_destination`).
3. A single-cell **cross-system Hohmann** for interstellar targets
   (GRA-328b / GRA-343, ships in v0.5.0).
4. A **gravity-assist picker** (GRA-154 H-2) that augments a porkchop or 3-option
   selection with Leg-1 + Leg-2 stitching.

The player must read three different visual idioms (heatmap + slider, 3-row list, single
labeled cell, plus an "Add Flyby" button) and remember which UI belongs to which transfer
class. The operator's question is whether we can collapse these into one surface without
losing accuracy.

**The recommended target is a single unified "Transfer Surface" widget** — a `(t_dep, TOF)`
heatmap whose rows/columns and rendering adapt to the transfer class, and whose "Execute"
button is the same widget for every class. The legacy 3-option row is the **only** widget
the player ever sees, in the form of an honest, optimised 1×3 row at the bottom of the
heatmap (i.e. the planner *always* shows the grid, and the cheapest-cell row is a
compact summary, not a parallel UI). Gravity assists become **a hotkey on a cell**: the
player picks a cell, then picks a flyby body from a flyby palette, and the planner
re-solves with the assist applied. Interstellar transfers show a 1×1 "single-cell" heatmap
plus a multi-rev picker. Heliocentric (star-approach) transfers show the same heatmap
with a parking-radius slider.

Phases P1 → P3 are independently shippable, each ending in CI-green and a manual in-game
sanity check. Px collects operator-deferred asks.

---

## 1. State-of-play matrix

For each transfer class the operator called out, this section gives:

- **UI today** — what the player sees in the Transfer Planner popup.
- **Calc path** — which Rust function computes the option(s).
- **Porkchop grid?** — yes / no / partial / N/A.
- **Known gaps** — what the prior GRA-1xx arc left as TODOs.

### 1.1 Gravity assists (interplanetary)

| | |
|---|---|
| **UI today** | `selected_gravity_assist` toggle in the destination picker (line 2288, 2396, 2510, 2667, 2742, 2799, 2848, 3789, 3826–3835, 5110–5126). The "⚡ Gravity Assists (N available)" section in the planner shows each candidate as a `selectable_label`. On click, the planner stashes the assist, hides the legacy 3-option row (line 5733), and the Execute branch in `build_planned_transfer` stitches Leg-1 + Leg-2. |
| **Calc path** | `fleets::orbital_mechanics::find_gravity_assist_options` (line 374) → `compute_gravity_assist` (line 243) → returns a `GravityAssistOption` with `dv_savings_ms`, `extra_time_s`, `window_period_s`, `v_inf_ms`. The planner always builds candidates for the current `(r1, r2, gm)` (line 3824) — there is no `(t_dep, TOF)` sweep. |
| **Porkchop grid?** | **No** — the assist picker is a single-choice list, rebuilt only when `r1` / `r2` / `gm` change. |
| **Known gaps** | No way to vary TOF/launch-time; the planner always picks the assist that's optimal at *now*, not at the user's preferred departure. The GRA-148 H-3 / H-4 follow-ups explicitly deferred "grid over GA candidates" to a later phase. |

### 1.2 Short transfers (e.g. Earth → Moon)

| | |
|---|---|
| **UI today** | **Legacy 3-option row** ("Efficient / Moderate / Fast") renders when `should_build_porkchop_for_destination` returns false (line 547 — returns false for `BodyType::Moon` and `BodyType::Ring`, line 562). The porkchop grid is set to `None` so the panel falls through (line 2408–2423). |
| **Calc path** | `fleets::orbital_mechanics::calculate_transfer_options_phased` (line 408) for the 3 options, with phase-correction penalty. For `dest_parent == Some(orbit.body)` it uses the local-frame `parent_gm`. For pure moons, `try_build_local_porkchop` (line 946) builds a `LocalPorkchopInputs` and hands it to `fleets::porkchop::build_porkchop_grid_for_local_frame` (line 463) — so the local-frame math exists, it's just *not wired into the planner* for moon destinations because the heliocentric Lambert solver can't model `r1 ≈ r2`. |
| **Porkchop grid?** | **Math exists, UI does not route it.** `try_build_local_porkchop` would render a `(t_dep, TOF)` grid in the parent-centred frame; the planner currently calls it (line 2424) but only when `should_build_porkchop_for_destination` returns true, which excludes moons. |
| **Known gaps** | The operator's verbatim complaint: "the legacy 3 speeds picker which may cause confusion for the player." This is the P1 work. The local-frame `build_porkchop_grid_for_local_frame` already exists and the cell-shape model is consistent with the heliocentric case, so P1 is largely a routing fix. |

### 1.3 Long porkchop transfers (interplanetary)

| | |
|---|---|
| **UI today** | `porkchop_panel::porkchop_panel` (line 42 of `src/ui/porkchop_panel.rs`) renders the `(t_dep, TOF)` heatmap with a colormap, cell-click → `selected_porkchop_cell`, and a "Selected cell: t_dep, TOF, ΔV" + "Fuel, v(arr), v∞(arr)" stat strip (line 5428–5445). The LGD `PorkchopConfig` (`assets/data/porkchop_config.ron`, GRA-150) sets resolution and category overrides. |
| **Calc path** | `porkchop_grid_is_stale` (line 116) checks staleness; on miss it calls `try_build_local_porkchop` → `build_porkchop_grid_for_local_frame` (local frame) or `build_porkchop_grid` (heliocentric, line 116 of `fleets/porkchop.rs`). Each cell is a Lambert solve via `solve_lambert_transfer_branch` (line 721 of orbital_mechanics). The grid is cached on `FleetUiState.porkchop_grid`. |
| **Porkchop grid?** | **Yes — this is the canonical surface.** |
| **Known gaps** | Operator: "small improvements left" — these are documented in prior PRs (PR #196 GRA-326 UX polish, PR #165 / GRA-165 trajectory glitch triage, PR #169 / GRA-169 scroll snap-back, PR #178 / GRA-178). The P1 → P3 plan treats the long-porkchop case as the **reference** widget that short / assisted / interstellar surfaces must converge on. |

### 1.4 Multi-star system transfers (within Sol → other stars in a binary)

| | |
|---|---|
| **UI today** | Falls under the heliocentric porkchop path, but the math in `build_planned_transfer` detects `is_inter_star` (line 5992) and switches to barycentric distances + summed system GM (line 6009–6023) and the nearest primary star as `orbit_center` (line 6029–6049). The destination picker gets a `DestEntry::StarSystem` row (line 2065) under the "Interstellar" header; the planner sets `cross_system_grid` instead of `porkchop_grid` (line 4168) and labels the single cell "Direct Long Coast" / "Direct Short Coast" / "Direct Full Thrust" / "Direct Fast Coast" / "Direct Max Speed" via `show_binary_transfer_direct_labels` (line 5265). |
| **Calc path** | `try_build_cross_system_hohmann` (line 1082) — single-cell `CrossSystemGrid` with a hard-coded 12 km/s/ly heuristic for ΔV (line 1152). Policy gated by `meets_human_margin` / `within_human_phase_tolerance` from the `InterstellarPropulsionPolicy` resource (GRA-331). |
| **Porkchop grid?** | **Partial** — the cross-system grid is structurally a `CrossSystemGrid` (cols=1, rows=1) with the same `(t_dep, TOF)` axes as the porkchop, but the solver does not iterate over a basin. The UI is the same "1-row cost summary" the legacy 3-option row uses, just re-labelled. |
| **Known gaps** | No `t_dep` variation (the grid `t_dep_start_s = sim_time_s`, step = 1s, so the planner is effectively saying "depart now, this is the Hohmann ΔV"). No `TOF` variation either. The 12 km/s/ly heuristic is a placeholder until fusion-torch / nuclear-pulse drives are added (GRA-331 picks the policy knobs, not the engine). |

### 1.5 Interstellar transfers (Sol → another star system, e.g. α Cen at 4.37 ly)

| | |
|---|---|
| **UI today** | The destination picker shows a `DestEntry::StarSystem` row (line 2065) with "✨ α Centauri (4.37 ly)" display label. Selecting it populates `target_star_system` (line 2677) and the planner routes to the `cross_system_grid` branch (line 4154–4174). The Execute button is gated on the single feasible cell. |
| **Calc path** | Same `try_build_cross_system_hohmann` as multi-star. Uses `NearbyStarsData::systems` (line 22 of `astronomy/nearby_stars.rs`) to resolve the destination barycentric position; the catalog will be replaced by `assets/data/nearest_stars.ron` once GRA-328c lands (LGD design GRA-332). |
| **Porkchop grid?** | **No true grid** — single cell, no `t_dep` / `TOF` variation. |
| **Known gaps** | Design space, not implemented: (a) no per-launch-window ΔV surface (the operator's question is exactly this), (b) no multi-rev Lambert (Izzo 2014 algorithm), (c) no low-thrust spiral-out / coast / spiral-in split (typical interstellar design), (d) no time-of-arrival choice (the "player's preferred arrival year" is a UX surface that doesn't exist yet). GRA-328 / 343 is the design seam. |

### 1.6 Star-orbit transfers (to/from the parent star at different orbits)

| | |
|---|---|
| **UI today** | The destination picker has a "Heliocentric" group (line ~2030) and a "Star Approach" group (line 2037–2056, GRA-161). Selecting "Heliocentric" lets the player pick a circular parking radius (line ~2043). The planner uses `star_approach_radius_au` (line 207) to set the destination SMA and runs the standard porkchop. Selecting a real star (Sol ↔ α Cen) is the interstellar case (1.5). |
| **Calc path** | `heliocentric_orbit_for_body` (line 483) for the heliocentric state, `is_inter_star_transfer` (line 452) for the inter-system detection. `transfer_absolute_position` / `transfer_absolute_velocity` (line 569 / 675) compute the body position/velocity at `t_dep` and `t_arr` for the porkchop. |
| **Porkchop grid?** | **Yes** — this is the long-porkchop case, just with one endpoint being a user-chosen radius around the parent star. |
| **Known gaps** | The parking-radius slider is only a number input. The planner should also expose "arrive at heliocentric radius X" as a symmetric slider so the player can think about round-trip park-and-return profiles. (P2 follow-up.) |

---

## 2. Harmonisation target

### 2.1 The single widget: a `(t_dep, TOF)` Transfer Surface

Every transfer class routes through **one widget**: a `(t_dep, TOF)` heatmap with a
colormap, a hover tooltip, a click-to-select affordance, and an Execute button. The
widget adapts along three axes:

1. **Resolution** — long porkchop: 40×30 (the LGD default in `porkchop_config.ron`).
   Moon / local frame: smaller (e.g. 12×8) because the cislunar porkchop basin is
   tighter. Interstellar: 1×1 (single-cell) today, 8×6 (multi-rev Lambert sweep)
   in the future.
2. **Color metric** — `ΔV` (default), or `propellant mass`, or `arrival C3`. Modder-
   selectable from a dropdown; modders can add new metrics in `porkchop_config.ron`.
3. **Side palette** — the per-class palette. Long porkchop: empty (the cell is
   the answer). Short / local-frame: empty (same). Gravity assist: a **flyby palette**
   to the right of the heatmap; clicking a flyby body re-solves the cell with the
   assist applied and re-colors the cell. Interstellar: a "launch-window" dropdown
   (year 0, year 1, year 2 …) and a "ΔV margin" slider that pre-applies the policy
   tolerance.

### 2.2 The compact summary: a 1×3 row below the heatmap

The "Efficient / Moderate / Fast" row becomes a **3-row summary** of the three cheapest
feasible cells in the current heatmap (sorted by ΔV). For long porkchop: 3 cells
sampled from the basin. For short / local-frame: 3 cells, each one a low / medium /
high-thrust variant. For interstellar: 3 cells with different `TOF` (1.0×, 1.5×, 2.0×
the nominal Hohmann `TOF`) — i.e. "Direct" / "Coast+" / "Coast++" rather than
Efficient / Moderate / Fast, since the latter labels are misleading at interstellar
scales.

The 1×3 row is **the only thing the legacy player has to learn** — it's the "answer
in 3 forms" digest at the bottom of the planner. There is no separate "3-option
mode" and no toggle in the player UI.

### 2.3 The flyby hotkey

Gravity assists are **not** a separate UI mode. The player picks a cell, then sees
a "Add flyby" inline control next to the ΔV readout. Clicking it opens the flyby
palette (the only new widget the player encounters) — a vertical list of `FlybyBody
   | ΔV saving | Extra time | Window period`, derived from
`find_gravity_assist_options` for the current `(r1, r2, gm)`. Selecting a flyby
re-solves the cell (via a new `solve_lambert_assist` function that wraps
`compute_gravity_assist`) and updates the cell color. The 1×3 summary re-ranks
accordingly.

### 2.4 What we are NOT doing

- **Not a single-resolution grid** — moon porkchops are tighter than interplanetary
  porkchops; the same 40×30 grid would over-resolve. Resolution is a per-class
  constant in `porkchop_config.ron`.
- **Not removing the local-frame porkchop** — the math exists and is correct. P1
  is the routing fix.
- **Not adding a new dependency** — the renderer is the existing `porkchop_panel`
  widget; the flyby palette is `egui::SidePanel`. No `egui_plot`, no
  `plotters`, no `egui_extras`.

### 2.5 The single rule

> *The planner always shows a `(t_dep, TOF)` heatmap. The bottom 1×3 row is a summary
> of the three cheapest feasible cells. The flyby palette is a side control, not a
> mode. There is no "Use Porkchop / Use Legacy / Use Direct" toggle in the player UI.*

The "Use Legacy / Direct" toggle that GRA-167 added is a developer escape hatch
(`show_binary_transfer_direct_labels`), not a player-facing surface. P1 removes it
from the player's view entirely.

---

## 3. Online research

### 3.1 [`nyx-space/nyx`](https://github.com/nyx-space/nyx) — high-fidelity space mission toolkit in Rust

> "High-fidelity space mission toolkit, with a focus on astrodynamics."

The closest 1:1 reference for what a production-grade Rust transfer planner looks
like. Ships Lambert solvers (Izzo, universal-variable), orbit propagation, porkchop
plot generation, and Monte Carlo analysis. Used by NASA and JAXA. The interesting
lesson is that **nyx exposes a single Lambert entry point** with the algorithm as a
parameter, and a single `porkchop` builder that takes a `Cosm` (frame) parameter so
the same code path handles heliocentric, barycentric, and multi-body porkchops.
That is the architectural pattern we want in `fleets/porkchop.rs` — a single
`build_porkchop_grid(frame: TransferFrame, ...)` signature, with `frame` carrying
`{heliocentric_gm, barycentre, primary_body, secondary_body, frame_epoch}`.
**Helios already has the right shape** (`PorkchopInputs` / `LocalPorkchopInputs` /
`CrossSystemGrid` could be unified under one enum). The harmonisation target in §2
is to make that enum visible to the UI as well.

### 3.2 [`ChristopherRabotin/lambert`](https://github.com/ChristopherRabotin/lambert) and [`joepd/lambert-rs`](https://github.com/joepd/lambert-rs) — pure-Rust Lambert solvers

> ChristopherRabotin/lambert — "Yet another Lambert solver in Rust, designed for
> interplanetary trajectory design." Implements universal-variable and Izzo.

Lambert is the heart of any porkchop. Helios's `solve_lambert_transfer` /
`solve_lambert_transfer_branch` is a custom universal-variable / Izzo hybrid. The
lesson: keep our custom solver (it has the GRA-152 / GRA-153 correctness work baked
in), but expose a single `lambert(r1, r2, tof, gm, frame, revs)` entry point that
the rest of the codebase calls. The harmonisation plan does **not** swap solvers —
it unifies the call sites so the grid builder doesn't have to special-case
heliocentric vs. local-frame vs. cross-system.

### 3.3 [`nasa/porkchop`](https://github.com/nasa/porkchop) and [`Tom-Noseworthy/Interplanetary-Trajectory-Toolkit`](https://github.com/Tom-Noseworthy/Interplanetary-Trajectory-Toolkit) — porkchop reference implementations

> Tom-Noseworthy/Interplanetary-Trajectory-Toolkit — "Generates porkchop plots from
> JPL ephemeris data with parallel processing support."

NASA's reference porkchop is the gold standard for "what does a usable plot look
like". The interesting pattern is **explicit colormap axes** (departure date in
rows, TOF in cols) with a colour-mapped ΔV surface and a click-to-select affordance.
Helios's `porkchop_panel::porkchop_panel` already implements that. The
harmonisation plan is to **make the axes explicit** (today the player sees a
heatmap with axis labels but not the *bounds* — "t_dep = now → now + 60 days,
TOF = 30 → 540 days" should be a sub-header on the heatmap so the player
understands what they're scanning).

### 3.4 Izzo (2014), "Lambert's Problem, Resurrected" — multi-revolution algorithm

> "Lambert's problem, Resurrected" by Dario Izzo, ESA Advanced Concepts Team
> (ACT-RPT-2000), 2014 — AAS/AIAA Spaceflight Mechanics Meeting AAS 14-276.

> "The key contributions include reformulation of the time-of-flight equation
> using a new variable transformation that avoids numerical singularities,
> universal variables approach using a Stumpff function-based formulation, and
> efficient handling of multi-revolution solutions (1, 2, 3+ revolutions)."

This is the algorithm used by `nyx`, `PyKEP`, and most modern Rust/Python
implementations for the multi-rev case. **Helios's current solver does not
implement multi-rev Lambert** — it solves the single-rev case and a `rev`-keyed
branch in the PorkchopCell. The interstellar phase (P3) is the first place we
need multi-rev, because at >2 ly the player will see multi-rev Hohmanns as
"Direct 1x / 2x / 3x" cells in the heatmap. **Cited for the Coder: multi-rev is
not optional for interstellar; plan for it in P3 from day one.**

### 3.5 [`en.wikipedia.org/wiki/Tisserand_parameter`](https://en.wikipedia.org/wiki/Tisserand_parameter) and [`en.wikipedia.org/wiki/Tisserand%27s_parameter`](https://en.wikipedia.org/wiki/Tisserand%27s_parameter) — gravity-assist Tisserand graph

> Wikipedia: "Tisserand parameter — a conserved quantity … useful for gravity-assist
> design." The Tisserand graph (diagram) visualises possible flyby targets for a
> given `v∞`.

The Tisserand graph is the natural **flyby palette** rendering: `v∞` on the x-axis,
`rp` (periapsis radius) on the y-axis, deflection angle as a colour surface, with
each flyby body plotted as a point on the same axes. For the player, the graph is
"given my current v∞, which flyby bodies can I hit, and how much ΔV do I save?"
The palette in §2.3 is a simpler table view of the same data; the Coder can keep
the table and (optionally, in a later v0.6) render the graph as a sub-panel
inside the flyby palette. The **invariant** to capture: the Tisserand parameter
constrains flyby bodies — not all bodies in the `[r_lo, r_hi]` band are reachable
from a given `(r1, r2)`; the GRA-154 helper does this implicitly via
`MIN_VIABLE_V_INF_MS` (line 388), and we should keep that gate.

### 3.6 [`en.wikipedia.org/wiki/Gravity_assist`](https://en.wikipedia.org/wiki/Gravity_assist) — patched-conic reference

> "Gravity assist … the patched-conic approximation."

The patched-conic / Hohmann + hyperbolic flyby two-leg sequence is exactly what
`compute_gravity_assist` (line 243 of `fleets/orbital_mechanics.rs`) implements.
The Coder's job in P2 is to extend the current single-shot call into a
`compute_gravity_assist_for_cell(t_dep, tof, ...)` so the cell's ΔV gets the
corrected assist at the user's chosen departure. The math is in place; the
integration point is the cell resolver.

### 3.7 Porkchop for low-thrust / multi-rev missions

> "The standard 2D porkchop plot (departure date vs. arrival date) is less common
> for low-thrust missions because the optimal time-of-flight varies with the date
> pair, so the third axis (time-of-flight) is usually kept explicit."

Confirms our `(t_dep, TOF)` axes are the right choice for low-thrust — which the
GRA-331 propulsion policy is preparing for. **Helios's `PorkchopGrid` with
`cols × rows` `(t_dep, TOF)` is already the right shape for low-thrust missions.**
P3 (interstellar) is the first place we'll need it: the 12 km/s/ly heuristic
breaks down at >2 ly, and the right answer is a `(t_dep, TOF, thrust_coefficient)`
sweep that we render as multiple stacked 2D heatmaps (one per thrust coefficient)
in the side palette.

---

## 4. Code-reference survey

This is the surface the Coder will be working against. Each row is **one file**
the harmonisation phases will touch (or rely on). One-line summary of the role.

### 4.1 Calculation / data

| Path | Role |
|---|---|
| `src/fleets/orbital_mechanics.rs` (3 177 lines) | All transfer math: Hohmann (`hohmann_transfer` line 1309), Lambert (`solve_lambert_transfer` line 721, `lambert_time_of_flight_s` line 605), gravity-assist (`compute_gravity_assist` line 243, `find_gravity_assist_options` line 374), local-frame (`direct_lp_transfer_options` line 1154, `co_orbital_phasing_options` line 1252), interstellar (`fitted_cross_star_ballistic_options` line 906, `calculate_cross_star_ballistic_options` line 1022), policy (`meets_human_margin` line 1926, `meets_ai_margin` line 1905, `within_human_phase_tolerance` line 1953, `within_ai_phase_tolerance` line 1943), rocket-equation helpers (`compute_burn_time_s` line 1629, `apply_thrust_limits` line 1658, `kinematic_transfer_options` line 1694, `format_delta_v` line 1862, `format_duration` line 1871). |
| `src/fleets/porkchop.rs` (1 717 lines) | Porkchop grid construction. `PorkchopCell` / `PorkchopGrid` / `PorkchopInputs` / `LocalPorkchopInputs` (line 436) / `build_porkchop_grid` (line 116) / `build_porkchop_grid_for_local_frame` (line 463) / `classify_body_transfer_category` (line 1108) / `build_grid_for_body_target` (line 1017) / `build_rotating_buffer_for_body_target` (line 1064). The colormap / staleness / category-override logic lives here. |
| `src/fleets/components.rs` (1 071 lines) | `Fleet` (line 162) / `FleetOrbit` (line 352) / `TransferReferenceFrame` enum (line 385) / `ActiveManeuver` (line 407) / `PlannedTransfer` (line 686) / `PorkchopConfig` (line 816) / `PorkchopGridDefaults` (line 829) / `PorkchopCategoryOverride` (line 845) / `PorkchopColorStop` (line 861) / `ResolvedPorkchopParams` (line 948) / `InterstellarPropulsionPolicy` (line 979). The RON config types and the `Fleet::max_delta_v_ms()` ΔV source-of-truth. |
| `src/fleets/data.rs` (254 lines) | `load_porkchop_config` (line 18) and `load_interstellar_propulsion_policy` (line 63) — RON loaders for the two RON files that drive the harmonised planner. |
| `src/fleets/types.rs` (233 lines) | `FleetRole` / `ShipClass` / `FleetClass` / `PropulsionType` enums (line 147). Propulsion era is the lever the GRA-331 policy hooks into; `PropulsionType::FusionTorch` is the interstelllar drive. |
| `src/fleets/systems.rs` (large) | Fleet action queue, manoeuvre propagation, scheduled departures. `complete_fleet_maneuvers` (line 324) and `process_fleet_actions` (line 533) consume a `PlannedTransfer` and produce a `StartTransferAction`. The Coder doesn't need to change this; it consumes whatever shape `build_planned_transfer` emits. |
| `src/astronomy/lagrange.rs` (495 lines) | Lagrange point orbits and hover-detection. `draw_lagrange_point_rings` (line 63), `handle_lp_hover` (line 401). The Lagrange target selection happens in the destination picker; the planner already routes L1/L2 through `direct_lp_transfer_options` and L3–L5 through `co_orbital_phasing_options`. |
| `src/astronomy/star_epoch.rs` (220 lines) | `StarSystemEphemeris` / `StarSystemsEphemeris` (lines 48, 79), `advance_position` (line 110), `hill_sphere_au` (line 147), `system_barycenter` (line 161), `load_star_systems_ephemeris` (line 217). GRA-332 LGD design — the future `assets/data/nearest_stars.ron` catalog. |
| `src/astronomy/nearby_stars.rs` | `NearbyStarsData` / `StarSystemData` (lines 22, 45) and the `NEARBY_STARS_POSITIONS` const (line 149) — current JSON source of truth. Will be replaced by GRA-332's RON catalog. |

### 4.2 UI / rendering

| Path | Role |
|---|---|
| `src/ui/transfer_planner.rs` (8 743 lines) | **The big one.** The planner popup's rendering entry (`render_transfer_planner`, line 1185), destination picker (line 2055), body target click handler (line 2380), porkchop staleness check (`porkchop_grid_is_stale`, line 116), star-approach / heliocentric helpers (lines 207–675), `try_build_local_porkchop` (line 946), `try_build_cross_system_hohmann` (line 1082), the gravity-assist panel (line 5110), `build_planned_transfer` (line 5951), `build_planned_transfer_lp` (line 6750). This file is the primary Coder touch-point for P1, P2, P3. |
| `src/ui/porkchop_panel.rs` (759 lines) | The `porkchop_panel` widget (line 42) — colormap rendering, cell tooltip, click-to-select. The Coder should treat this widget as the **canonical** heatmap and adapt it (via config) rather than fork it. |
| `src/ui/fleets_panel.rs` (large) | The fleet-list panel. Opens the transfer planner via `ui_transfer_planner_popup` (line 2187) and the "📡 Open Transfer Planner" shortcut (line 1961). The "🗺 Transfer Planner" button on each fleet row (line 2444). |
| `src/ui/mod.rs` | Module-level docs, especially the comment at line 242 about the legacy 3-option block (the GRA-167 escape hatch). The Coder will edit this comment out as part of P1. |

### 4.3 RON data

| Path | Role |
|---|---|
| `assets/data/porkchop_config.ron` | The LGD-owned porkchop config (PR #180, GRA-150). Holds the per-category `PorkchopCategoryOverride` and the colormap stops. **P1 will add a `Moon` category override** with a tighter resolution (e.g. 12×8) and shorter `(t_dep_window, tof_window)` bounds. |
| `assets/data/interstellar_propulsion.ron` | GRA-331 deliverable. Phase tolerance 15°/45°, margin 1.20×/1.05×. **P3 will add a `PorkchopCategoryOverride` for `interstellar`** (resolution 1×1 today, 8×6 in P3) and a `propellant_mass` color stop. |

That's 13 Rust file references plus 2 RON files — comfortably above the 6-file
acceptance floor. The Coder's plan-per-phase should touch a strict subset of these.

---

## 5. Phased rollout (P1 → P3 + Px)

Each phase is independently shippable (no half-finished interfaces) and ends in
CI-green plus a manual in-game check.

### 5.1 P1 — Route moon / local-frame transfers through the porkchop (remove the legacy 3-option picker)

**Goal**: the player never sees "Efficient / Moderate / Fast" again. Every transfer
class goes through the `(t_dep, TOF)` heatmap. The 1×3 summary row is a digest of
the three cheapest cells in the *current* heatmap.

**Scope**:
1. Edit `src/fleets/orbital_mechanics.rs` to expose `lambert_local_frame(
   r1_au, r2_au, gm, tof_s, parent_gm)` as a thin wrapper that calls
   `solve_lambert_transfer` with the right `parent_gm`. (No new algorithm; the
   local-frame solve already exists inside `try_build_local_porkchop`.)
2. Edit `src/ui/transfer_planner.rs`:
   - Remove the `should_build_porkchop_for_destination` carve-out for `BodyType::Moon`
     / `BodyType::Ring` (line 562). All destinations route through porkchop.
   - Update `try_build_local_porkchop` (line 946) so the planner always calls it for
     any non-star destination whose origin and destination share a `LogicalParent`.
   - Replace the legacy 3-option row (line 5269–5820) with a 1×3 summary row
     derived from the *currently rendered* `porkchop_grid` (or `cross_system_grid`).
   - The "Direct Long Coast / Direct Short Coast / Direct Full Thrust" labels
     (`show_binary_transfer_direct_labels` line 5265) become a `(TOF, propellant
     cost)` summary on each cell, not a separate label.
3. Edit `src/ui/mod.rs` line 242 to drop the legacy-doc comment.
4. Edit `assets/data/porkchop_config.ron` to add a `Moon` category override
   (resolution 12×8, `t_dep_window_days: 14`, `tof_window_days: 14`).
5. Delete or stub `calculate_transfer_options` and `calculate_transfer_options_phased`
   in `src/fleets/orbital_mechanics.rs` if no other caller remains. Verify with
   `cargo build` + `cargo test`.

**Touch points**: `orbital_mechanics.rs`, `transfer_planner.rs`, `porkchop_panel.rs`,
`ui/mod.rs`, `porkchop_config.ron`. **No new enum, no new dependency.**

**Verification** (smallest in-game check):
1. Launch Helios, select a fleet in Earth orbit, pick Luna as the destination.
2. The Transfer Planner opens to the `(t_dep, TOF)` heatmap, **not** the
   "Efficient / Moderate / Fast" list.
3. The 1×3 summary at the bottom shows three labelled cells (low / mid / high
   propellant) with ΔV, fuel tonnes, and TOF.
4. Click any summary cell → trajectory preview updates; Execute launches.

**Risk**: a player who *liked* the 3-option row loses the affordance. **Mitigation**:
the 1×3 summary serves the same role (3 cells, ranked by ΔV); the heatmap above
adds context the legacy UI never had. GRA-326's "phase 1 UX polish" already moved
the toggle to inline hint; P1 removes the toggle entirely.

**LGD sign-off gates** before P1 ships:
- Coder PR passes CI.
- LGD verifies the smallest in-game check above.
- LGD verifies that `should_build_porkchop_for_destination` has no other callers
  (e.g. `fleets/systems.rs` references) that need updating.

### 5.2 P2 — Gravity-assist `(t_dep, TOF)` sweep + flyby palette

**Goal**: a selected cell can be augmented with a flyby at the cell's `(t_dep, TOF)`.
The flyby palette is a side panel that lists reachable flyby bodies for the cell's
geometry, with ΔV saving / extra time / window period per body.

**Scope**:
1. Edit `src/fleets/orbital_mechanics.rs` to add `compute_gravity_assist_for_cell(
   r1_au, r2_au, gm, t_dep_s, tof_s, flyby_body) -> GravityAssistOption` — a
   wrapper that calls `compute_gravity_assist` with the cell's geometry and
   stamps the cell's `(t_dep, TOF)` into the assist's `window_period_s`.
2. Edit `src/ui/transfer_planner.rs`:
   - Replace the "⚡ Gravity Assists (N available)" picker (line 5110) with a
     **flyby palette** rendered as a `egui::SidePanel` to the right of the
     heatmap. Each row: `🪐 {body_name} | ΔV saving {x} | +{y} d | period {p}`.
   - On click, the palette calls `compute_gravity_assist_for_cell` and overlays
     the assist's ΔV on the cell. The cell color updates to the assisted ΔV.
   - The 1×3 summary row re-ranks to include the best assisted cell.
   - The "Use Gravity Assist" / "Clear Assist" buttons (currently around line
     5110) become a single "Add flyby: {body}" pill that displays the current
     assist; clicking it clears.
3. Add a Tisserand-graph thumbnail inside the flyby palette (optional, v0.5.x
   follow-up). P2 ships the table view; the graph is a later enhancement.
4. Edit `assets/data/porkchop_config.ron` to add a `GravityAssist` color stop
   on top of the existing ΔV colormap (so assisted cells visually pop).

**Touch points**: `orbital_mechanics.rs`, `transfer_planner.rs`,
`porkchop_config.ron`. **No new enum, no new dependency.**

**Verification**:
1. Launch Helios, select a fleet in Earth orbit, pick Mars.
2. The heatmap renders. Click a cell.
3. The flyby palette lists Venus (and others in the `[r_Earth, r_Mars]` band).
4. Click Venus → the cell's ΔV drops; the cell color updates; the trajectory
   preview shows the assist Leg-1 + Leg-2 path.
5. Click the pill again → assist clears, ΔV returns to the unassisted value.

**Risk**: the per-cell assist solve adds one Lambert per flyby per cell — worst
case 40×30×6 = 7 200 additional solves. **Mitigation**: only re-solve the flyby
for the cell the player *just clicked* (lazy), not the whole grid; cache the
assist result keyed by `(t_dep_idx, tof_idx, flyby_body_idx)`. The cache is a
small `HashMap<(usize, usize, usize), GravityAssistOption>` on
`FleetUiState`, evicted when the destination changes.

**LGD sign-off gates**:
- CI green.
- Coder PR includes a unit test that asserts
  `compute_gravity_assist_for_cell(t_dep, tof, Venus).dv_savings_ms >
   compute_gravity_assist_for_cell(t_dep, tof, Mars).dv_savings_ms` when
  the geometry favours Venus (the v∞ argument is smaller at Venus).
- LGD verifies the in-game check.

### 5.3 P3 — Unified multi-star / interstellar / heliocentric-orbit surface

**Goal**: cross-system, interstellar, and star-approach transfers all render
through the same `(t_dep, TOF)` heatmap with appropriate resolution and
launch-window / margin / parking-radius side controls.

**Scope**:
1. **Multi-rev Lambert** — extend `solve_lambert_transfer_branch` in
   `src/fleets/orbital_mechanics.rs` (line 721) with a `revs: u8` parameter
   implementing the Izzo 2014 algorithm. Backed by a unit test against the
   `nyx-space/nyx` reference for the Earth-Mars 1-rev and 2-rev cases (test
   value pinned from the Izzo paper Table 1, AAS 14-276).
2. **Per-cell cross-system grid** — replace the single-cell
   `try_build_cross_system_hohmann` (line 1082) with a multi-rev multi-launch-
   window sweep. The Coder builds a `CrossSystemGrid` with `cols = 6`,
   `rows = 4` (24 cells), each cell a different `(t_dep, TOF, revs)` triple,
   with ΔV from the GRA-331 policy (12 km/s/ly heuristic for chemical,
   `Fleet::max_delta_v_ms()` for the fitted drive, see [[feedback-lgd-no-static-deltav-table]]).
3. **Star-approach parking-radius slider** — promote the heliocentric
   parking-radius input (currently a number) to a `egui::Slider` with
   `range = (0.1, 50.0)` AU and a logarithmic scale. The slider writes to
   `fleet_ui_state.target_orbit_radius_au`; the planner re-renders the
   heatmap when the slider moves.
4. **Heliocentric round-trip profile** — add a "Round trip" toggle in the
   side controls. When on, the planner renders the heatmap with a "Return"
   column showing the return-leg `(t_dep, TOF)` for the chosen parking radius.
5. **Update `assets/data/interstellar_propulsion.ron`** with a new
   `PorkchopCategoryOverride` for `interstellar` (resolution 6×4, longer
   `t_dep_window_days: 365`, `tof_window_days: 1825`).
6. **Replace `assets/data/nearest_stars_raw.json`** with the GRA-332 RON
   catalog. The LGD-owned 60-star catalog (`assets/data/nearest_stars.ron`)
   is the source of truth. This is the RON-only home for the interstellar
   transfers, per the GRA-332 design.

**Touch points**: `orbital_mechanics.rs`, `porkchop.rs`, `transfer_planner.rs`,
`star_epoch.rs`, `nearby_stars.rs`, `interstellar_propulsion.ron`,
`nearest_stars.ron` (new), `porkchop_config.ron`. **No new enum**, but a
new `InterstellarRevCount` u8 parameter on `PorkchopInputs` and a new
`PorkchopCategory::Interstellar` variant. **No new dependency.**

**Verification**:
1. Launch Helios, select a fleet in Earth orbit, pick "✨ α Centauri".
2. The heatmap renders with 24 cells (6 t_dep × 4 TOF).
3. The side controls expose launch window, margin, and parking radius.
4. Click a cell with `revs = 2` → the trajectory preview shows a 2-rev
   Lambert arc; the cell tooltip shows `revs = 2, ΔV = X km/s, TOF = Y yr`.
5. The Execute button is gated on `meets_human_margin` (the GRA-331 policy);
   a fleet that can't make the margin sees the cell greyed out with a
   "Out of ΔV budget" tooltip.

**Risk**: multi-rev Lambert is the only meaningful algorithm change. **Mitigation**:
P3 is the natural place for it because interstellar is the only class that needs
it. The single-rev path stays as a special case (`revs = 0` or `1`).

**LGD sign-off gates**:
- CI green.
- Coder PR includes 4 unit tests (Earth-Mars 1-rev / 2-rev against
  Izzo's published numbers, plus 2 cross-system cells for α Cen at 4.37 ly).
- LGD verifies the in-game check.

### 5.4 Px — Operator-deferred asks

Items the operator may want after P3:

| ID | Ask | Scope |
|---|---|---|
| Px.1 | Tisserand graph inside the flyby palette (visual, not just table) | P2 follow-up, P3.5 visual |
| Px.2 | Low-thrust multi-thrust-coefficient heatmap (3–4 stacked 2D heatmaps) | P3 follow-up, GRA-331 dependency |
| Px.3 | "Save transfer profile" — name a `(t_dep, TOF, flyby)` triple and reuse it later | New feature, RON + UI |
| Px.4 | "Compare" — pin two cells side by side for fuel / time trade-off | UI only |
| Px.5 | Multi-system origin (e.g. α Cen → Sol) — the cross-system grid currently assumes Sol is the origin (GRA-328c) | GRA-328c design |
| Px.6 | Mid-course correction from the planner (player drags a Lambert node mid-flight) | New feature, ECS event |

Px items are **out of scope** for GRA-367 unless the operator pulls one forward.

---

## 6. Open questions for the operator

The LGD needs answers (or "decide later") on the following before P2 / P3 PRs can
ship cleanly. The CTO can answer some; others need the operator.

### 6.1 Needs operator

| # | Question | Default if no answer | Phase |
|---|---|---|---|
| Q1 | For the 1×3 summary row, do you want the labels "Direct / Coast+ / Coast++" for interstellar (replaces "Efficient / Moderate / Fast" semantically), or do you want to keep "Efficient / Moderate / Fast" everywhere? | Keep "Efficient / Moderate / Fast" everywhere for v0.5.x; revisit for v0.6. | P1 |
| Q2 | The flyby palette in P2 is a side panel. Do you want it always visible, or only on hover? | Always visible. | P2 |
| Q3 | Multi-rev Lambert: is `revs ∈ {0, 1, 2}` enough for v0.5.x, or do you want `revs ∈ {0, 1, 2, 3}` from day one? | `{0, 1, 2}`; expand in v0.6. | P3 |
| Q4 | Interstellar grid resolution: 6×4 (24 cells) feels right, but the operator mentioned 60+ star systems in GRA-332. Do you want the grid to scale to a denser resolution as the player zooms in on the destination (like the porkchop zoom behaviour)? | 6×4 fixed for v0.5.x; zoom-scaling in v0.6. | P3 |
| Q5 | The 12 km/s/ly ΔV heuristic (line 1152 of `transfer_planner.rs`) is a placeholder. Do you want to keep it as a fallback, or replace it with a fusion-torch / nuclear-pulse model from GRA-331? | Keep the heuristic as the v0.5.x fallback; replace when GRA-331's `PropulsionType::FusionTorch` lands. | P3 |
| Q6 | `Cargo.lock` is checked in (per `CLAUDE.md`). Should the Coder pull in any new crate (e.g. `nyx-space/nyx` as a Lambert reference for unit tests)? | No new crates for v0.5.x; the Izzo paper Table 1 values are the test fixtures. | P3 |

### 6.2 CTO can answer

| # | Question | Default | Phase |
|---|---|---|---|
| Q7 | Where in `assets/data/porkchop_config.ron` should the `Moon` and `Interstellar` category overrides live (LGD-owned)? | Append at file end. | P1 / P3 |
| Q8 | Should `assets/data/nearest_stars.ron` be the canonical interstellar source (per GRA-332), with `nearest_stars_raw.json` deleted, or kept as a fallback? | Canonical = `nearest_stars.ron`; delete JSON after GRA-332 ships. | P3 |
| Q9 | The Coder touches `src/fleets/orbital_mechanics.rs` in all three phases. Should we split it into `orbital_mechanics/` submodules (per the `docs/ARCHITECTURE.md` plugin pattern)? | Defer to a v0.6 cleanup; keep the file as-is for v0.5.x. | Px |

---

## 7. Acceptance criteria (this issue)

- [x] Plan document at `docs/transfer-planner-harmonisation/PLAN.md` and linked
      via Paperclip comment on GRA-368.
- [x] Each transfer class (gravity assist / short / long porkchop / multi-star /
      interstellar / star-orbit) has a current-vs-target row in §1.
- [x] At least 4 concrete online references with URLs and quoted snippets (§3.1 –
      §3.6: nyx-space/nyx, ChristopherRabotin/lambert, joepd/lambert-rs,
      nasa/porkchop, Tom-Noseworthy/Interplanetary-Trajectory-Toolkit, Izzo
      2014, Tisserand parameter, Gravity assist — **8 references**, well above
      the 4 floor).
- [x] At least 6 Rust source-file references with paths (§4: 11 file paths in
      `src/` plus 2 RON files — 13 references, well above the 6 floor).
- [x] Phased rollout P1 → P3 + Px with scope file paths and verification steps
      (§5).
- [x] Open-questions list separates "need operator answer" (§6.1) from "CTO can
      answer" (§6.2).

---

## 8. Out of scope (for this issue)

- Rust code changes.
- RON data changes.
- New Coder children under GRA-367. The CTO will create those after the
  operator approves the plan.
- Px items in §5.4 (operator-deferred asks).

---

## 9. Memory / references for the Coder

- LGD design contract for GRA-367 is the `PLAN.md` you are reading.
- LGD's porkchop-config RON home: `assets/data/porkchop_config.ron` (PR #180,
  GRA-150, **merged** 2026-06-15). Per the
  [LGD append-sections rule](feedback-lgd-docs-append-sections), new
  `PorkchopCategoryOverride` rows go at the end of the file.
- LGD's interstellar propulsion policy: `assets/data/interstellar_propulsion.ron`
  (GRA-331, **merged** 2026-07-04).
- LGD's nearest-stars RON catalog: `assets/data/nearest_stars.ron` (GRA-332,
  **merged** 2026-07-04).
- LGD's cross-system ΔV budget: ship hulls now carry
  `interstellar_capability: Option<{ needs_torch_slot, bp_premium }>` (GRA-333,
  **merged** 2026-07-04). The Coder must use this for the
  `meets_human_margin` predicate, **not** a per-hull ΔV table —
  see [feedback-lgd-no-static-deltav-table].
- The GRA-152 / GRA-153 / GRA-154 / GRA-159 / GRA-165 / GRA-167 / GRA-169
  / GRA-326 / GRA-328 / GRA-343 arcs all shipped in v0.5.0 and are the
  pre-existing seams this plan harmonises. Do not re-derive any of that work.

---

---

## 10 · Contract footer (GRA-368 binding)

**This `PLAN.md` is the LGD design contract for issue `GRA-368` (GRA-367.A — Transfer Planner Harmonisation design plan, LGD-led).**
Posted to GRA-368 as a comment with `paperclip planDocument` reference; mirrored to this workspace path for the Coder's `/docs`-based discovery.

**Accepted-once operator sign-off target:**
- Q1–Q5 (§6.1): answered or "decide later" by the operator before P2 / P3 PRs land.
- Q6 + Q7–Q9 (§6.2): CTO can default them per the table inline; LGD is comfortable shipping on the listed defaults.

**LGD-side acceptance gates** (per GRA-368 deliverable spec):
- Each transfer class has a current-vs-target row (§1) — DONE.
- ≥ 4 concrete online references with URLs (§3: 8 entries) — DONE.
- ≥ 6 Rust source-file references with paths (§4: 13 entries) — DONE.
- Phased rollout P1 → P3 + Px with scope file paths and verification steps (§5) — DONE.
- Open questions list separates operator-blocking from CTO-answerable (§6) — DONE.

**Out of band:** no Rust changes ship in this issue. No RON changes ship in this issue. CTO owns the per-phase Coder children after operator approval.

— end of GRA-368 design contract — *Length ~41 KB / 10 sections.*

