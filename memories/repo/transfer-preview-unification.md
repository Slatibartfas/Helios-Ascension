# Transfer preview unification (GRA-178 follow-up)

User feedback after the porkchop rebase fix: the sampled orbit
polyline (the "solid line" preview) was snaking around or
sometimes invisible. The dashed Bezier (from `compute_barycentric_
visual_arc`) was always clean. User wants ONE unified preview arc.

## Why the two paths existed

`draw_fleet_transfer_preview` had three branches:

1. **Barycentric Bezier** — `reference_frame.is_barycentric() && !is_kinematic`.
   Dashed curve from `compute_barycentric_visual_arc` with Lambert
   velocities. Smooth, no snap, no snake.

2. **`preview_center_is_star` orbit polyline** — same-star heliocentric
   transfers where the reference frame is `Body(star)` (e.g. the
   GRA-326 dispatcher routes Earth→Jupiter porkchop selections here).
   Tries the exact-endpoint Bezier first, falls back to
   `build_visual_sampled_transfer_polyline_moving_center` drawing the
   actual orbit's geometry.

3. **Fallback shared geometry** — local-frame transfers.

Branch 2's orbit polyline couldn't track the planet's current mean
anomaly across buffer rebuilds because:
- The transfer orbit's `mean_anomaly_epoch` is set by the Lambert
  solver at the buffer's build time, not at the planet's current
  position.
- Any rebase formula either drifted smoothly (used the planet's
  mean_motion) or snapped on rebuild (used the orbit's mean_motion).
- A planet-anchored rebase (planet_ma(launch)) produced a polyline
  that traced the orbit's geometry but slid around it at the
  planet's rate — the "snaking" symptom.

## Unification

Branch 1's Bezier is extended to ALSO accept `Body(star)` reference
frames (same-star heliocentric with orbit center = star). Branch 2
is removed entirely.

The unified gate is:

```rust
if !is_kinematic
    && !is_course_correction
    && matches!(
        reference_frame,
        TransferReferenceFrame::SystemBarycentric | TransferReferenceFrame::Body(_)
    )
    && same_star_orbit_preview
{ ... compute_barycentric_visual_arc ... }
```

The `same_star_orbit_preview` check still requires the orbit center
to be a star, so moon / planet transfers fall through to the
fallback shared geometry (Branch 3) — those use the regular Bezier
that already works for local-frame transfers.

The Bezier interpolates between predicted `op` and `dp_absolute`
with Lambert-derived tangents when available, so it's both smooth
and accurate (uses the actual orbit's velocity vectors). One
unified calculation, one style (dashed), no snap, no snake.

## Scope

This change touches only the **preview** arc (when the planner is
open). The in-transit Bezier from `draw_fleet_trajectories` is a
separate code path and still uses its own geometry; it has its own
`(sim_elapsed + maneuver.departure_time)` sample-time bug that
deserves a separate fix later.