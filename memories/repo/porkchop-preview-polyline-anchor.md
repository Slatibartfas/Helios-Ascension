# Porkchop preview polyline anchor (GRA-178 follow-up)

User-reported "two transfer preview curves" symptom: the dashed Bezier
preview follows the fleet continuously, but the solid sampled-polyline
preview (drawn by `draw_visual_sampled_transfer_polyline_moving_center`
in the `preview_center_is_star` branch of `draw_fleet_transfer_preview`)
snaps every porkchop buffer rotation.

## Root cause

The previous rebase formula was

```rust
let orbit_epoch_sim_s = grid.t_dep_bounds_s.0;  // = sim_time at buffer build
let preview_start_mean_anomaly = orbit.mean_anomaly_epoch
    + orbit.mean_motion * (current_sim_s - orbit_epoch_sim_s);
```

It used the **transfer orbit's** `mean_motion` to advance the polyline's
start mean anomaly each frame. The transfer orbit moves at `n_orbit`
(Hohmann Earth→Mars: ~0.7 rad/yr) while the origin planet moves at
`n_planet` (Earth: ~1.0 rad/yr). Each porkchop buffer rotation (~240
sim days for an 8× Earth→Mars buffer, ≈0.66 real seconds at 1 yr/s)
the orbit's `mean_anomaly_epoch` re-anchors to the new build-time
planet position. The polyline's start mean anomaly jumps by
`(n_planet - n_orbit) × Δt_build` radians — ~6.6° for Earth→Mars,
repeating ~1.5 times per real second. That's the visible "snap".

## Fix

`src/fleets/visuals.rs::sampled_preview_start_mean_anomaly` now anchors
the polyline's start mean anomaly to the **origin planet's** mean
anomaly at the launch time (`current_sim_s + departure_s`). The planet's
orbital elements don't change between grid rebuilds, so the anchor is
continuous across rotations. Falls back to the orbit-only formula when
the origin body has no `KeplerOrbit` (Lagrange targets).

## Scope

This only affects the `preview_center_is_star` branch of
`draw_fleet_transfer_preview`, which is taken when:
- `planned_transfer.reference_frame` is **local-frame** (Body, not
  barycentric)
- `planned_transfer.orbit_center` is a **star**
- option is not kinematic / direct / coast
- no flyby

Interplanetary transfers (Earth→Mars etc.) use `Barycentric(Sun)` and
fall through to the Bezier path, so this fix doesn't touch the most
common case. It does affect the "same-star heliocentric" / star-approach
preview path.

## In-transit code (`draw_fleet_trajectories`) has the same bug

The in-transit renderer at line ~3129 uses
`start_mean_anomaly = maneuver.transfer_orbit.mean_motion * elapsed` —
it ignores `mean_anomaly_epoch` entirely. This is also wrong (drifts
from the planet at `n_orbit` rate), but is a separate issue from the
user's "before departure" complaint and was left untouched in this
fix.

## Test

`sampled_preview_anchor_is_continuous_across_buffer_rebuild` asserts
the helper advances at `planet.mean_motion` between two snapshots
240 sim days apart, and that the fallback path (no planet) gives a
measurably different value (catching any future regression that
switches back to orbit-rate anchoring).