# Porkchop UI polish (GRA-style follow-up)

User feedback: the porkchop plot looked "quite uniform, not so much green
to red", the planned trajectory never updated when a cell was selected,
and the hover tooltip only appeared while the left mouse button was held.

Three changes in `src/ui/porkchop_panel.rs` + `src/ui/transfer_planner.rs`:

1. **Relative colormap.** Compute min/max ΔV over the grid's feasible
   cells and remap the configured colour stops onto that range. The
   absolute colormap (0–15 km/s) wastes most of its green band on
   Earth↔Mars transfers which sit in 6–9 km/s. NASA/JPL-style relative
   mapping stretches the gradient across the actual data range. Floor
   at 0.5 km/s so degenerate grids still produce variation.

2. **`Sense::hover()` instead of `click_and_drag()`.** With
   `click_and_drag`, egui's `interact_pointer_pos` only fires while a
   button is held. Switched to `Sense::hover()` so `hover_pos()` is
   populated whenever the pointer is over the canvas. Added a thin
   white outline on the hovered cell (so the player gets visual
   feedback that isn't a click) and a dark rounded-rect backdrop
   behind the tooltip text so it stays readable over red/yellow cells.

3. **Trajectory preview wiring.** The porkchop branch in
   `transfer_planner.rs` previously `return;`-ed before the legacy
   `planned_transfer = build_planned_transfer(...)` block ran, so the
   3D preview arc kept showing whatever `selected_option` last pointed
   at. Added a `planned_transfer` rebuild right after the panel
   renders: turn the selected cell into a synthetic `TransferOption`
   (label `"Porkchop Cell"`, `transfer_orbit_override = cell.transfer_orbit`)
   and feed it through `build_planned_transfer` exactly like the
   body-target branch. Clears to `None` when no cell is selected or the
   selected cell is infeasible / out-of-budget.

**Follow-up: trajectory still didn't update.** Setting
`planned_transfer` from the porkchop branch was necessary but not
sufficient — `stable_preview_travel_time` in
`src/fleets/visuals.rs` reads from `computed_options[selected_option]`
**first** and only falls through to `planned_transfer.duration_s` when
`selected_option` is out of range. For the porkchop path,
`computed_options` still contains the legacy Efficient/Moderate/Fast
list with `selected_option = 0`, so the ghost arc drew the Hohmann
travel time, ignoring the porkchop cell's `tof_s`. Fix: when
`porkchop_grid.is_some()`, prefer `planned_transfer.duration_s`
immediately.

**Lesson: order of guards matters in flight-time helpers.** The
"computed_options first, planned_transfer fallback" pattern is fine
for the legacy 3-option path but breaks as soon as a new selection
source (porkchop, gravity assist) populates `planned_transfer`
without emptying `computed_options`. Always check
`porkchop_grid.is_some()` / `selected_gravity_assist.is_some()` /
similar source-of-truth flags before falling back to the legacy
selection.

Lesson learned: `Option<KeplerOrbit>` is `Copy`. The original Execute
Porkchop Transfer branch had a `cell.transfer_orbit.clone()` which
clippy flagged as `clone_on_copy` — pass by value.

**Sense combination gotcha.** `Sense::hover()` alone ignores clicks.
`Sense::click_and_drag()` only reports `interact_pointer_pos` while a
button is held (so tooltips still required holding the mouse). The
fix is `Sense::click() | Sense::hover()` (BitOr), which gives both
`hover_pos()` always-while-over-rect AND `clicked()` on left-click.
The hint `interact_pointer_pos` only matters with drag; for pure
hover-driven tooltips, `hover_pos()` is what you want.

**Tooltip clamping/flip.** A naive "anchor below cursor" tooltip
disappears when the cursor enters the bottom rows of the plot — the
tooltip rect spills past the panel bottom edge and gets clipped.
Two fixes: (a) flip above the cursor when `below_room < tooltip_height`
and `above_room` has room, (b) horizontally clamp against the
`plot_rect`. Pass the panel's clip rect into the tooltip renderer.

**Porkchop window = "from now to one synodic period".** Originally
the grid centred on the next optimal Hohmann window (`±half`), which
meant Saturn (synodic period 1.09 yr) had no "Depart Now" cell —
the cheapest Hohmann window sat a year out, and reaching t_dep=0
required dragging a 5-year slider.  Now `dep_window_bounds` returns
`(0, max(synodic_period, 2 * half) + half)` so the player always
sees the ΔV cost of launching immediately alongside the next
Hohmann basin.  Co-orbital degenerate cases (|d_phi_dt| ≈ 0, e.g.
Earth-Moon) keep a tight `[0, 2*half]` window centred on t=0.

The wider window changed the test assertions: with coarser grids,
the closest-tof cell may land hundreds of days off the optimal
Hohmann phase, so its ΔV is 6-7 km/s instead of the canonical 5.6.
Bumped `porkchop_earth_mars_has_feasible_cells` to 80×30 and
`porkchop_phase_window_mark` to 8×8 with ±25% tolerance.  These
tolerances are plausibility checks, not precision bounds.

**Porkchop is the source of truth for t_dep.** The Transfer Window
+ Planned Departure boxes (and their slider) are now hidden when
`porkchop_grid.is_some()`.  Clicking a porkchop cell IS the
departure-time selection — `departure_offset_days` is synced to
`cell.t_dep_s / 86_400` so the legacy side-panel "Arrives:"
timestamp and `waiting_orbit_count` stay consistent, but no UI
control surfaces that value when the porkchop is active.  This
removes the dual-source-of-truth bug where the slider and the
porkchop could disagree.

**Porkchop grid staleness.** The grid's `t_dep = 0` column is
anchored to the sim-time epoch the grid was built at (`elapsed`
passed to `build_grid_for_body_target`).  Once `elapsed` advances
past that epoch, the cached ΔV values drift (inner planets move
~1°/day, outer planets slower, but always enough to matter for a
1200-cell Lambert solve).  Closing-and-reopening the planner
without re-building gave the user a "Depart Now" tick that still
represented the *build-time* planetary geometry, not the *current*
one.

Fix: `FleetUiState.porkchop_built_at_s: Option<f64>` records the
build epoch.  The planner's deferred-build path invalidates the
cache when `(elapsed - built_at).abs() >
PORKCHOP_STALENESS_THRESHOLD_S` (default 3 days).  Three days is
short enough to stay accurate for inner-planet transfers and long
enough to amortize the worst-case 40×30 grid solve (~360 ms).
Factor out as `porkchop_grid_is_stale(built_at, elapsed) -> bool`
so the boundary (3-day exact, time-reversal, no-build) is
unit-testable without spinning up a Bevy world.

Tests added:
- `relative_colormap_stretches_onto_grid_range` — asserts the first/last
  finite stop land on the grid's ΔV range and the sampled endpoints
  hit the green/red ends.
- `relative_colormap_floor_on_degenerate_grid` — degenerate grids
  expand the range symmetrically to the 0.5 km/s floor.
