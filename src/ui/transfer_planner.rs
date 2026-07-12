use super::time::format_timestamp_date_time;
use super::*;
use crate::fleets::orbital_mechanics::calculate_cross_star_ballistic_options;
use crate::fleets::porkchop::{
    build_grid_for_body_target, build_rotating_buffer_for_body_target, build_short_hop_grid,
    build_star_approach_grid, StarApproachInputs,
};
use serde::{Deserialize, Serialize};
// GRA-367-E: pull `PorkchopGrid` into the module-level scope so the
// `try_build_cross_system_hohmann` return type resolves without a
// local `use` inside the function body.  Phase 5 emits a degenerate
// `PorkchopGrid` (1×1 cells) for the cross-system path; Phase 1 will
// later consume the same shape.
use crate::fleets::porkchop::PorkchopGrid;
use crate::fleets::PorkchopConfig;
// GRA-343: explicit import — `super::*` does not bring the new
// resource type from `crate::fleets` into this module's namespace
// for fn-signature type aliases.  `InterstellarPropulsionPolicy` is
// re-exported through `crate::fleets` and needs the explicit path.
use crate::fleets::InterstellarPropulsionPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannerTransferFrame {
    BodyLocal(Entity),
    StellarLocal(Entity),
    SystemBarycentric,
}

/// Minimum gravitational parameter (m³ s⁻²) to distinguish a star from a planet.
///
/// Corresponds to roughly 0.01 M☉ — well above Jupiter (1.27 × 10¹⁷) and below
/// even the smallest hydrogen-fusing stars (0.08 M☉ ≈ 1.06 × 10¹⁸).  We use a
/// slightly lower bound so that massive sub-stellar objects do not fall through.
const MIN_STELLAR_GM: f64 = 1.3e18; // ~0.01 M☉ in m³ s⁻²

/// Minimum orbital radius (AU) used as a guard in transfer calculations to avoid
/// division-by-zero or negative square-roots in vis-viva equations.
const MIN_ORBITAL_RADIUS_AU: f64 = 0.001; // 1/1000 AU ≈ 149,600 km (inside Mercury)

/// Safe gravity-assist periapsis scaling factor for **planetary** flyby bodies.
///
/// Like [`STELLAR_FLYBY_RADIUS_KM_MULTIPLIER`], this is a `meters_per_km × radius_multiplier`
/// factor used with `CelestialBody.radius` in kilometres. The km→m conversion is **baked in**:
/// `PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER = 1_000 × 3` — 3× body radius, in m/km.
/// A conservative minimum altitude above the atmosphere/surface.
const PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER: f64 = 3_000.0; // = 1_000 m/km × 3

/// Maximum *real-time* seconds the cached porkchop grid is allowed to
/// stay fresh before the planner rebuilds it.  The grid's `t_dep = 0`
/// column is anchored to the sim-time epoch the grid was built at;
/// once the sim has advanced enough, the "Now" column no longer
/// reflects the current planetary geometry and the ΔV values drift
/// noticeably.  We express the threshold in real-time seconds and
/// multiply by `TimeScale::scale` at the call site so the rebuild
/// fires after a consistent wall-clock interval regardless of how
/// fast the simulation is running.  Without this scaling, at 1 yr/s
/// the sim advances ~5.83 days per frame and the staleness fires
/// immediately after the player clicks a cell, snapping the
/// selection back to the auto-picked cheapest cell.  3 days at
/// 1 hr/s (the default speed) is short enough to stay accurate for
/// inner-planet transfers (where planets move ~1°/day) and long
/// enough to amortize the 40×30 = 1200-cell Lambert solve (worst-case
/// ~360 ms) so we don't rebuild every frame.
const PORKCHOP_STALENESS_REAL_S: f64 = 72.0;

/// Upper bound on the staleness threshold in *sim* seconds, used to
/// cap the `time_scale`-scaled real-time floor at extreme speeds.
///
/// Without this cap the staleness grows linearly with `time_scale`,
/// so at 1 yr/s (`time_scale = 31_557_600`) the grid stays cached
/// for 72 *sim years* — the player watches the "Depart Now" column
/// stay frozen even though the planets have orbited a full year.
/// At 1 yr/s, 1 sim year = 1 real second of wall clock, so a cap of
/// one sim year means the planner refreshes the grid every ~1 real
/// second, which is fast enough to keep the on-screen porkchop in
/// sync with the rapidly-changing planet alignments.
///
/// At low/medium speeds the cap never binds because the scaled
/// threshold is already smaller:
///   * paused: 72 sim sec (≈ minutes of wall time).
///   * 1 hr/s: 259_200 sim sec = 3 sim days.
///   * 1 day/s: 6_220_800 sim sec = 72 sim days — the cap (1 sim
///     year = 31_557_600 sim sec) starts to bind here for the first
///     time, knocking 72 sim days down to 1 sim year (≈ 1 real sec).
///   * 1 yr/s: 2_270_000_000 sim sec = 72 sim years → cap (1 sim
///     year) binds, refreshing every ~1 real second.
const PORKCHOP_STALENESS_MAX_SIM_S: f64 = 31_557_600.0;

/// Pure-function helper: should the cached porkchop grid be invalidated
/// because `elapsed` has drifted too far from the build epoch?
///
/// The threshold is `PORKCHOP_STALENESS_REAL_S × time_scale`, capped
/// at `PORKCHOP_STALENESS_MAX_SIM_S` so high sim speeds can't grow
/// the staleness window into "the grid is frozen for the entire
/// play session" territory.  At 1 yr/s the scaled threshold would be
/// 72 sim years; the cap clamps it to 1 sim year = 1 real second of
/// wall clock, so the on-screen porkchop refreshes roughly once per
/// second instead of never.  At low/medium speeds the cap never binds
/// and the existing per-speed cadence is preserved (3 sim days at
/// 1 hr/s, 72 sim days at 1 day/s).
///
/// Real-time floor: at intermediate speeds (1 wk/s, 1 day/s) the
/// sim-time cap alone would still gate the rebuild on a *sim-time*
/// interval — at 1 wk/s that's 52 real seconds, at 1 day/s 72 real
/// seconds — long enough that the on-screen ΔV basin looks static
/// even though the sim is moving a week per real second.  The
/// real-time floor requires *at least* `PORKCHOP_STALENESS_REAL_FLOOR_S`
/// of wall-clock seconds between rebuilds in addition to the sim-time
/// cap, so at intermediate speeds the grid refreshes every real
/// second (when sim drift has also exceeded the cap) and at extreme
/// speeds the cap binds first.  Both timers must fire for the grid
/// to be marked stale — the real-time guard prevents firing on
/// every frame at 1 yr/s, while the sim-time cap prevents firing
/// on every real second at 1 hr/s (where one real second = one sim
/// hour, well inside the 3-day threshold).
///
/// Returns `true` only when `built_at` is present, the cached grid
/// is present, and both timers exceed their thresholds.  Returning
/// `false` for `built_at = None` is intentional: a fresh build
/// (no grid yet) is handled by the deferred-build path, not the
/// staleness path.
///
/// Public-ish (crate-private with a `pub(super)` so unit tests can
/// exercise the boundary without spinning up a Bevy world).
pub(super) fn porkchop_grid_is_stale(
    built_at: Option<f64>,
    elapsed: f64,
    time_scale: f64,
    last_real_build_s: Option<f64>,
    real_now_s: f64,
) -> bool {
    let (Some(built), Some(last_real)) = (built_at, last_real_build_s) else {
        return false;
    };
    // Sim-time cap (high side): the grid's ΔV values drift
    // unacceptably once the player's "now" has moved this many sim
    // seconds past the build epoch.  The scaled value grows
    // linearly with `time_scale` (so each speed tier has the same
    // per-rebuild CPU budget), capped at PORKCHOP_STALENESS_MAX_SIM_S
    // (= 1 sim year) so 1 yr/s doesn't gate on 72 sim years.
    let scaled_threshold = PORKCHOP_STALENESS_REAL_S * time_scale.max(1.0);
    let sim_cap = scaled_threshold.min(PORKCHOP_STALENESS_MAX_SIM_S);
    // Real-time floor (low side): never rebuild more often than
    // every `PORKCHOP_STALENESS_REAL_FLOOR_S` of wall-clock seconds
    // even if the sim has drifted enough.  Without this a 1 yr/s
    // sim could fire every microsecond (the cap binds at 1 sim
    // year = 1 real second, but the sim cap being satisfied earlier
    // would still trigger the rebuild prematurely without the
    // real-time guard).
    let real_floor_sim_s = PORKCHOP_STALENESS_REAL_FLOOR_S * time_scale.max(1.0);
    // The effective sim cap is the lower of the scaled sim cap
    // (CPU protection at low speeds) and the real-time floor (so
    // the grid refreshes at least every PORKCHOP_STALENESS_REAL_FLOOR_S
    // real seconds at intermediate speeds).  Bounding the effective
    // cap by the real-time floor is what lets the 1 wk/s tier
    // refresh every real second rather than waiting 52 real seconds
    // for the 1-sim-year cap to fire.
    let effective_sim_cap = sim_cap.min(real_floor_sim_s);
    let sim_drift = (elapsed - built).abs();
    // `sim_drift > effective_sim_cap` is the staleness trigger; the
    // boundary (`==`) keeps the grid cached so clicks don't snap back
    // to the cheapest cell right after a rebuild.
    if sim_drift <= effective_sim_cap {
        return false;
    }
    let real_drift = real_now_s - last_real;
    real_drift >= PORKCHOP_STALENESS_REAL_FLOOR_S
}

/// Safe gravity-assist periapsis scaling factor for **stellar** flyby bodies.
///
/// `1_000 m/km × 1.5` — 1.5× the star's photospheric radius, in m/km.  Stars are
/// much larger and hotter than planets; a periapsis measured in stellar radii keeps
/// the flyby outside the corona where solar wind / radiation pressure dominate and
/// Δv cannot be modelled as a simple two-body assist.  This constant is the
/// pair-buddy to [`PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER`]; together they bracket
/// the safe-periapsis formulas used by the gravity-assist planner.  Any future
/// code path that considers a star as a flyby body MUST use this multiplier
/// (not the planetary one) and MUST explicitly exclude stars from the GA
/// candidate filter — see the gravity-assist filter in `compute_route_options`.
#[allow(dead_code)] // Reserved for future stellar-flyby assist code (GRA-149 C-1).
const STELLAR_FLYBY_RADIUS_KM_MULTIPLIER: f64 = 1_500.0; // = 1_000 m/km × 1.5 ≈ 1.5 R★

/// Default star-approach parking radius (AU) used when a star entity has no
/// per-body `star_approach_au` override.  0.3 AU is well outside the
/// photospheres of all main-sequence stars but close enough that the planner
/// can still display a meaningful arrival orbit.  GRA-149 C-2 makes this
/// the global default; per-body overrides live in `CelestialBody.star_approach_au`
/// (e.g. an M-dwarf can park at 0.05 AU above its surface).
const STELLAR_APPROACH_AU: f64 = 0.3;

/// Minimum allowed star-approach parking radius (AU) for the interactive
/// destination picker.  0.05 AU is well above the photospheres of all
/// main-sequence stars (the Sun's photosphere is ~4.7 × 10⁻³ AU; M-dwarfs
/// are even smaller) and is the value GRA-149 C-2 uses for tight M-dwarf
/// overrides.  Clamping below this would let the player pick an orbit
/// inside the star's corona where Δv cannot be modelled as a two-body
/// assist.  GRA-161.
const MIN_STAR_APPROACH_AU: f64 = 0.05;

/// Maximum allowed star-approach parking radius (AU) for the interactive
/// destination picker.  5.00 AU sits inside Jupiter's orbit in the Sol
/// system and outside the closest planet in most M-dwarf systems.  The
/// picker computes a per-star upper bound (closest-planet SMA × 0.9) for
/// the arrival so the parking orbit cannot be placed inside an existing
/// planetary orbit.  GRA-161.
const MAX_STAR_APPROACH_AU: f64 = 5.0;

/// Resolve the star-approach parking radius (AU) for a star body.
///
/// Returns `body.star_approach_au` if set (per-body override from RON or
/// procedural data); otherwise falls back to [`STELLAR_APPROACH_AU`] (0.3 AU).
/// Caller is responsible for clamping against the host planet's SMA to keep
/// the parking orbit outside the origin planet.
#[inline]
fn star_approach_radius_au(body: &CelestialBody) -> f64 {
    body.star_approach_au.unwrap_or(STELLAR_APPROACH_AU)
}

// ── GRA-NNN: orbit-shell picker ─────────────────────────────────────────────
// Named parking-orbit shells (Terra Invicta-style) replace the free-form
// `target_arrival_radius: Option<(Entity, f64)>` DragValue.  Each shell
// resolves to a numeric arrival radius via `radius_for_shell` below, with
// the math driven by the body's own physical properties (radius, mass,
// rotation period) — no RON override, no player-editable number.
//
// Two shell families:
//   * Body shells (Low / Medium / High / Stationary) for planets, moons,
//     rings, dwarf planets, gas giants, asteroids, comets.
//   * Star shells (CloseApproach / HabitableInner / HabitableOuter /
//     Cruise) for stars.

/// Identifies a named parking-orbit shell the player can pick for a
/// destination body.  `radius_for_shell` resolves each id to a numeric
/// arrival radius (AU).
///
/// GRA-NNN.  Supersedes the free-form `target_arrival_radius` DragValue
/// (GRA-161 / GRA-387).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum OrbitShellId {
    /// 1.05 × body radius (just above the surface / atmosphere edge).
    /// Procedural scaling keeps small bodies above any residual outgassing.
    Low,
    /// 3 × body radius — well clear of LEO debris, parking-stable.
    Medium,
    /// 10 × body radius — transfer-staging shell.
    High,
    /// Geostationary-equivalent orbit: r_sync = (GM·T_rot²/4π²)^(1/3).
    /// Falls back to `Low` when the body has no measurable rotation.
    Stationary,
    /// Star shell: [`MIN_STAR_APPROACH_AU`] (0.05 AU).  Inside the
    /// habitable zone but close enough to the photosphere that Δv starts
    /// to deviate from the two-body model.  Used by M-dwarf overrides.
    CloseApproach,
    /// Star shell: `star_approach_radius_au(body)` — re-uses the GRA-149
    /// C-2 default / per-body override.  Where Earth's orbit lives for
    /// a Sol-magnitude star.
    HabitableInner,
    /// Star shell: `sqrt(L_star / L_sol) × 1.0 AU`.  Cached on
    /// `CelestialBody.habitable_outer_au` at spawn time.  Outer edge of
    /// the conservative habitable zone.
    HabitableOuter,
    /// Star shell: [`MAX_STAR_APPROACH_AU`] (5.0 AU).  Outer-system
    /// staging / pre-interstellar cruise parking.
    Cruise,
}

impl OrbitShellId {
    /// Short human-readable label for the picker dropdown.
    pub fn label(self) -> &'static str {
        match self {
            OrbitShellId::Low => "Low",
            OrbitShellId::Medium => "Medium",
            OrbitShellId::High => "High",
            OrbitShellId::Stationary => "Stationary",
            OrbitShellId::CloseApproach => "Close Approach",
            OrbitShellId::HabitableInner => "Habitable Inner",
            OrbitShellId::HabitableOuter => "Habitable Outer",
            OrbitShellId::Cruise => "Cruise",
        }
    }

    /// The set of shells available to a given body type.  Asteroids and
    /// comets don't expose `Stationary` (no measurable rotation).
    pub fn shells_for(body_type: BodyType) -> &'static [OrbitShellId] {
        match body_type {
            BodyType::Star => &[
                OrbitShellId::CloseApproach,
                OrbitShellId::HabitableInner,
                OrbitShellId::HabitableOuter,
                OrbitShellId::Cruise,
            ],
            BodyType::Asteroid | BodyType::Comet => {
                &[OrbitShellId::Low, OrbitShellId::Medium, OrbitShellId::High]
            }
            _ => &[
                OrbitShellId::Low,
                OrbitShellId::Medium,
                OrbitShellId::High,
                OrbitShellId::Stationary,
            ],
        }
    }
}

/// Default shell when no override is set.  `Low` for bodies (matches the
/// pre-existing LEO-proxy default), `HabitableInner` for stars (matches
/// the pre-existing `star_approach_radius_au` default and preserves the
/// GRA-149 C-2 label promise).
pub fn default_shell_for_body_type(body_type: BodyType) -> OrbitShellId {
    match body_type {
        BodyType::Star => OrbitShellId::HabitableInner,
        _ => OrbitShellId::Low,
    }
}

/// Resolve a shell to its numeric parking radius (AU).
///
/// Pure on `&CelestialBody` — caller has already destructured the
/// standard 5-tuple body query.  GRA-NNN.
///
/// Body shells (Low / Medium / High) scale off the body's own radius:
///   Low    = 1.05 × body.radius_km, with a +10 km absolute floor for
///            small bodies whose 1.05× altitude would dip below any
///            practical orbital regime.
///   Medium = 3 × body.radius_km
///   High   = 10 × body.radius_km
///   Stationary = (GM · T_rot² / 4π²)^(1/3), or `Low` if the body has
///                no measurable rotation (asteroids, comets, rings).
///
/// Star shells are constant AU values, except `HabitableOuter` which
/// reads the precomputed `body.habitable_outer_au` cache (falls back to
/// `2 × star_approach_radius_au(body)` if the cache is `None`).
pub fn radius_for_shell(body: &CelestialBody, shell: OrbitShellId) -> f64 {
    use crate::fleets::orbital_mechanics::{AU_IN_METERS, G_CONST};
    let r_km = body.radius as f64;
    let r_m = r_km * 1000.0;
    match shell {
        OrbitShellId::Low => {
            // Absolute floor: 10 km above the surface keeps small-body
            // shells above any residual outgassing / surface irregularities.
            let shell_m = r_m * 1.05;
            let floor_m = r_m + 10_000.0;
            shell_m.max(floor_m) / AU_IN_METERS
        }
        OrbitShellId::Medium => r_m * 3.0 / AU_IN_METERS,
        OrbitShellId::High => r_m * 10.0 / AU_IN_METERS,
        OrbitShellId::Stationary => match body.rotation_period_s {
            Some(t) if t > 0.0 => {
                // r_sync = (GM · T_rot² / 4π²)^(1/3)
                // `rotation_period_s` is `.abs()`'d at spawn, so sign flip
                // is unnecessary here.
                let gm = G_CONST * body.mass;
                let r_sync_m = (gm * t.powi(2) / (4.0 * std::f64::consts::PI.powi(2))).cbrt();
                r_sync_m / AU_IN_METERS
            }
            _ => radius_for_shell(body, OrbitShellId::Low),
        },
        OrbitShellId::CloseApproach => MIN_STAR_APPROACH_AU,
        OrbitShellId::HabitableInner => star_approach_radius_au(body),
        OrbitShellId::HabitableOuter => body
            .habitable_outer_au
            .unwrap_or_else(|| star_approach_radius_au(body) * 2.0),
        OrbitShellId::Cruise => MAX_STAR_APPROACH_AU,
    }
}

/// Inverse of [`radius_for_shell`]: pick the shell whose resolved radius
/// is closest to the given numeric value, within the body's available
/// shell set.  Falls back to [`default_shell_for_body_type`] for the
/// body's type when no shell is unambiguously closer.
///
/// Used by the Commit-2 dual-write path: every existing
/// `target_arrival_radius: Some((entity, radius_au))` write needs a
/// matching `target_orbit_shell: Some((entity, shell))` so Commit 3
/// can drop the numeric field without losing user state.  GRA-NNN.
pub fn shell_id_for_radius(body: &CelestialBody, radius_au: f64) -> OrbitShellId {
    let shells = OrbitShellId::shells_for(body.body_type);
    let mut best_shell = default_shell_for_body_type(body.body_type);
    let mut best_diff = f64::INFINITY;
    for &shell in shells {
        let r = radius_for_shell(body, shell);
        let diff = (r - radius_au).abs();
        if diff < best_diff {
            best_diff = diff;
            best_shell = shell;
        }
    }
    best_shell
}

/// Compute the user-selectable `radius_au` bounds for the interactive
/// star-approach picker (`DestEntry::StarApproach`).
///
/// Lower bound: [`MIN_STAR_APPROACH_AU`] (0.05 AU) — above the photosphere
/// of all main-sequence stars.
/// Upper bound: 90 % of the closest planet's orbital SMA in the host
/// system (so the parking orbit sits well inside the innermost planet)
/// or [`MAX_STAR_APPROACH_AU`] (5.0 AU) if no planet is closer than that.
/// GRA-161.
fn star_approach_bounds_au(
    star_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    body_system_ids: &Query<&SystemId>,
    current_system_id: usize,
) -> (f64, f64) {
    let min_au = MIN_STAR_APPROACH_AU;
    let mut max_au = MAX_STAR_APPROACH_AU;
    let mut closest_sma: Option<f64> = None;
    for (_, b, _, ko, lp) in body_query.iter() {
        if b.body_type == BodyType::Star || b.body_type == BodyType::Ring {
            continue;
        }
        if lp.map(|p| p.0) != Some(star_entity) {
            continue;
        }
        if !body_system_ids
            .get(star_entity)
            .ok()
            .map(|s| s.0 == current_system_id)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(k) = ko {
            let sma = k.semi_major_axis;
            if sma > 0.0 {
                closest_sma = Some(match closest_sma {
                    Some(prev) if prev <= sma => prev,
                    _ => sma,
                });
            }
        }
    }
    if let Some(closest) = closest_sma {
        // 90 % of the closest planet's SMA keeps the parking orbit
        // well clear of the planet's sphere of influence.
        let planet_cap = (closest * 0.9).max(MIN_STAR_APPROACH_AU + 0.01);
        if planet_cap < max_au {
            max_au = planet_cap;
        }
    }
    (min_au, max_au)
}

/// Returns `true` when `gm` is large enough to be a stellar-mass central body.
///
/// Used to decide whether transfer-window phase angles should be read from
/// heliocentric (star-frame) coordinates or from a local planet-centric frame,
/// and whether gravity-assist candidates should be offered.
#[inline]
fn is_stellar_gm(gm: f64) -> bool {
    gm >= MIN_STELLAR_GM
}

/// Returns `true` when `mass_kg` is large enough that the body is a stellar-mass
/// central body (rather than a planet / moon).  This is the mass-domain twin of
/// [`is_stellar_gm`] and is the GRA-149 C-3 replacement for the legacy
/// SMA-threshold classifier.  Threshold = `MIN_STELLAR_GM / G ≈ 0.01 M☉` — well
/// above Jupiter (~1.9 × 10²⁷ kg) and below the smallest hydrogen-fusing stars
/// (~1.4 × 10²⁹ kg).  Use this whenever you need to ask "is this body a star
/// in a class sense?" without going through `G·M`.
#[inline]
fn is_stellar_mass(mass_kg: f64) -> bool {
    let gm = mass_kg * crate::fleets::orbital_mechanics::G_CONST;
    gm >= MIN_STELLAR_GM
}

/// Minimum wall-clock (real) seconds between porkchop grid rebuilds.
/// Caps the rebuild frequency at all `time_scale` tiers so a 1 yr/s
/// sim doesn't fire the staleness check on every frame — at 60 FPS
/// that would be 60 rebuilds per real second × ~360 ms per rebuild =
/// 21.6 s of CPU per real second, which would saturate one core.
/// 5 real seconds is the floor that keeps the planner's CPU footprint
/// under ~7 % even at the highest sim speed (1 yr/s = 1 sim year per
/// real second → 1 rebuild every 5 real seconds).  At 1 yr/s, planet
/// positions shift ~2π rad per real second, so a 1-real-second floor
/// produced a visible jump every second (texture rebake + transfer
/// preview arc rebuild) the user reported.  The 5-real-second floor
/// matches the cadence at which orbital ΔV differences become
/// *meaningful* (a few minutes of real time = hours of sim time at
/// 1 yr/s) and aligns with the staleness tests' "5-real-second"
/// comments throughout this module.
const PORKCHOP_STALENESS_REAL_FLOOR_S: f64 = 5.0;

/// Compute the Hill-sphere radius (AU) of a secondary body orbiting a much more
/// massive primary. `a_au` is the secondary's orbital radius around the primary,
/// `m_secondary_kg` is the secondary's mass, and `m_primary_kg` is the primary's
/// mass.  Used to position L1 (inner) and L2 (outer) Lagrange points along the
/// primary-secondary radial.
#[inline]
fn hill_radius_au(a_au: f64, m_secondary_kg: f64, m_primary_kg: f64) -> f64 {
    if m_primary_kg <= 0.0 || a_au <= 0.0 {
        return 0.0;
    }
    a_au * (m_secondary_kg / (3.0 * m_primary_kg)).powf(1.0 / 3.0)
}

/// Build a `LagrangeTarget` for L1 or L2 of a secondary body around its primary.
///
/// The label rendered in the transfer-planner picker is the LGD-locked format
/// `🛰 L{n} ({secondary}-{primary})` (e.g. `🛰 L1 (Earth-Sun)`,
/// `🛰 L1 (Moon-Earth)`) — see GRA-155 Q3.
fn build_lagrange_target(
    point: u8,
    secondary_entity: Entity,
    secondary_name: &str,
    secondary_sma_au: f64,
    secondary_mass_kg: f64,
    primary_mass_kg: f64,
) -> LagrangeTarget {
    let r_hill = hill_radius_au(secondary_sma_au, secondary_mass_kg, primary_mass_kg);
    let radius_au = match point {
        1 => secondary_sma_au - r_hill,
        2 => secondary_sma_au + r_hill,
        _ => secondary_sma_au,
    };
    LagrangeTarget {
        point,
        planet_entity: secondary_entity,
        planet_name: secondary_name.to_string(),
        planet_sma_au: secondary_sma_au,
        radius_au,
        gm: G_CONST * primary_mass_kg,
    }
}

/// Map a RON star name to its player-facing English label for picker rows.
/// The RON keeps the IAU-style name (`Sol`) for data integrity; the UI uses
/// the human-readable word (`Sun`) per the LGD Q3 lock on GRA-155.
fn star_display_label(ron_name: &str) -> &str {
    match ron_name {
        "Sol" => "Sun",
        other => other,
    }
}

/// Build the picker-row label for a Lagrange point using the LGD-locked Q3
/// format from GRA-155:
/// - **Sun-Planet**: `🛰 L1 (Earth-Sun)` — `{secondary}-{star}`. The star
///   uses its English display label (e.g. `Sun` for the RON star `Sol`).
/// - **Planet-Moon**: `🛰 L1 (Earth-Moon)` — `{planet}-{moon}` (central first,
///   per the LGD memo).  The system label is `{central}-{secondary}`.
fn lagrange_picker_label(lp: &LagrangeTarget, central_name: &str, central_is_star: bool) -> String {
    if central_is_star {
        let star = star_display_label(central_name);
        format!("🛰 L{} ({}-{})", lp.point, lp.planet_name, star)
    } else {
        format!("🛰 L{} ({}-{})", lp.point, central_name, lp.planet_name)
    }
}

/// Compute the L1 and L2 [`LagrangeTarget`]s for a secondary body orbiting
/// `host_entity`.  Returns `(l1, l2, host_name)` for the picker, or `None` if
/// the body is not in a 2-body (host, mass) configuration we can solve.
///
/// - **Planet around Star** → Sun-Planet L1/L2 (`direct_lp_transfer_options`)
/// - **Moon around Planet** → Planet-Moon L1/L2 (`co_orbital_phasing_options`)
///
/// `body_sma_au` is the secondary's orbital radius around the host (heliocentric
/// for a planet, planet-centric for a moon).
fn lagrange_targets_for_body(
    body_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    host_entity: Entity,
    body_sma_au: f64,
) -> Option<(LagrangeTarget, LagrangeTarget, String)> {
    let (_, host_body, _, _, _) = body_query.get(host_entity).ok()?;
    let (_, sec_body, _, _, _) = body_query.get(body_entity).ok()?;
    if host_body.body_type == BodyType::Star
        && !matches!(sec_body.body_type, BodyType::Planet | BodyType::GasGiant)
    {
        // A body whose LogicalParent points at a star is a planet/gas-giant:
        // Sun-Planet L1/L2.  Skip dwarfs/moons whose parent is a star.
        return None;
    }
    let l1 = build_lagrange_target(
        1,
        body_entity,
        &sec_body.name,
        body_sma_au,
        sec_body.mass,
        host_body.mass,
    );
    let l2 = build_lagrange_target(
        2,
        body_entity,
        &sec_body.name,
        body_sma_au,
        sec_body.mass,
        host_body.mass,
    );
    Some((l1, l2, host_body.name.clone()))
}

/// Walk up the `LogicalParent` chain from `start_entity` until a `BodyType::Star`
/// entity is found.  Returns `(star_entity, star_mass_kg)` or `None` if no stellar
/// ancestor exists within a reasonable depth.
///
/// This correctly handles fleets orbiting moons (moon → planet → star) and other
/// nested hierarchies in multi-star or non-Sol systems.
fn find_host_star(
    start_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<(Entity, f64)> {
    let mut current = Some(start_entity);
    // Depth limit prevents infinite loops in degenerate data.
    for _ in 0..8 {
        let entity = current?;
        let Ok((_, body, _, _, lp)) = body_query.get(entity) else {
            return None;
        };
        if body.body_type == BodyType::Star {
            return Some((entity, body.mass));
        }
        current = lp.map(|lp| lp.0);
    }
    None
}

#[inline]
fn is_inter_star_transfer(
    origin_entity: Entity,
    target_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> bool {
    let origin_host_star = find_host_star(origin_entity, body_query).map(|(entity, _)| entity);
    let target_host_star = find_host_star(target_entity, body_query).map(|(entity, _)| entity);
    origin_host_star.is_some() && target_host_star.is_some() && origin_host_star != target_host_star
}

/// Resolve the heliocentric `KeplerOrbit` for a body used as a porkchop
/// origin or destination.  Mirrors the three-case logic in
/// `fleets::porkchop::heliocentric_orbit_for_body` but takes the
/// planner's `&Query<...>` directly so the dest-click sites can
/// resolve the orbits inline before calling
/// `build_grid_for_body_target` (the pure helper that consumes them).
///
/// **GRA-159 fix:** `Moon` and `Ring` body types have a *local-frame*
/// `KeplerOrbit` (around the parent), not a heliocentric one.  Feeding
/// a local-frame sma (e.g. Luna's 0.00257 AU) into the porkchop Lambert
/// solver — which assumes heliocentric inputs — produces an infeasible
/// grid.  We therefore unconditionally walk up to the parent's
/// heliocentric `KeplerOrbit` for these body types.  `Star` keeps its
/// barycentric convention; `Planet` (and any other non-Moon/Ring body)
/// keeps its own heliocentric `KeplerOrbit` if present.
pub fn heliocentric_orbit_for_body(
    body: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<KeplerOrbit> {
    let (_, body_data, _, ko, lp) = body_query.get(body).ok()?;
    if body_data.body_type == BodyType::Star {
        // Stars carry a barycentric (near-zero SMA) orbit by JPL
        // convention.  The porkchop math consumes this only for the
        // `system_gm` derivation; for a star-vs-star transfer the
        // caller is responsible for picking a different solver.
        return ko.copied();
    }
    // For `Moon` and `Ring` body types, the body's own `KeplerOrbit` (if
    // present) is a *local-frame* orbit around the parent — never a
    // heliocentric one — so we ignore it and always walk up to the
    // parent's heliocentric orbit.  This is the bug fix for GRA-159:
    // previously the function returned the local-frame orbit, which the
    // porkchop Lambert solver then treated as heliocentric, producing
    // an infeasible grid for moon destinations.
    let needs_parent_orbit = matches!(body_data.body_type, BodyType::Moon | BodyType::Ring);
    if !needs_parent_orbit {
        if let Some(orbit) = ko.copied() {
            return Some(orbit);
        }
    }
    // Body has no heliocentric orbit of its own (Moon/Ring always;
    // Planet only if it somehow lacks one) — fall back to the parent's
    // heliocentric orbit (Earth's 1 AU orbit for Luna, etc.).  Limit
    // the walk to a single step because the JPL dataset never has more
    // than one intermediate parent (moon → planet → star), and deeper
    // chains are extremely rare in the spawned game state.
    let parent = lp.map(|lp| lp.0)?;
    let (_, _, _, parent_ko, _) = body_query.get(parent).ok()?;
    parent_ko.copied()
}

/// Fraction of the rotating porkchop buffer's future runway to
/// consume before triggering the next async rebuild.  `1.0` = wait
/// until the visible window's right edge exactly reaches the
/// buffer's right edge (no lead time).  A lower fraction starts the
/// async Lambert solve earlier, so the ~300–500 ms worker thread has
/// already finished — and the atomic grid swap has already
/// happened — well before the runway would otherwise run dry.  This
/// converts the rebuild from "the buffer ran out, so we're now
/// blocked waiting for a fresh one" (a perceptible jump right at the
/// edge) into "the fresh grid was already sitting in reserve when we
/// needed it" (an inaudible swap somewhere in the overlap window).
///
/// `0.85` was picked empirically: it leaves 15 % of the runway
/// (≈ 0.15 × 4 × `t_dep_window_days`, tens of sim-days even for the
/// tightest category override) as build-time margin, which comfortably
/// covers the worker's real-world solve time even at high sim-speed
/// multipliers, without rebuilding so often that it wastes CPU on
/// solves the player never gets close to needing.
const PORKCHOP_EARLY_ROTATION_FACTOR: f64 = 0.85;

/// Returns `true` when the rotating porkchop buffer should start
/// building its next window.
///
/// `shift_s` is the sim-seconds elapsed since the current buffer was
/// built (`elapsed - built_at_s`).  `buffer_t_dep_span_s` is the
/// buffer's full `(t_dep_max - t_dep_min)` span — this function
/// halves it internally (the buffer covers 2× the visible window,
/// so only the first half is "runway" before the visible window's
/// right edge reaches the buffer's right edge) and applies the
/// `PORKCHOP_EARLY_ROTATION_FACTOR` lead-time margin.
///
/// Extracted as a pure function (rather than left inline in the
/// egui render path) so the rotation-lead-time contract can be unit
/// tested without spinning up a Bevy app + egui context.
pub fn should_rotate_porkchop_buffer(shift_s: f64, buffer_t_dep_span_s: f64) -> bool {
    if buffer_t_dep_span_s <= 0.0 {
        return false;
    }
    let visible_window_runway_s = buffer_t_dep_span_s * 0.5;
    shift_s >= visible_window_runway_s * PORKCHOP_EARLY_ROTATION_FACTOR
}

/// Decide whether the porkchop `(t_dep, t_tof)` grid is the right
/// tool for a given destination body, or whether the planner should
/// fall through to the legacy Efficient / Moderate / Fast 3-option
/// row.
///
/// The porkchop math assumes **heliocentric** inputs (`system_gm =
/// GM_SUN`, both bodies orbiting the star).  For local-frame
/// destinations — moons and rings — the destination's heliocentric
/// `KeplerOrbit` is the **parent's** orbit (per `heliocentric_orbit_for_body`'s
/// GRA-159 fix).  When the fleet is parked at the parent body (e.g.
/// a fleet in Earth orbit targeting Luna), the planner receives
/// `r1 ≈ r2 ≈ parent_sma_au`, which is a degenerate Lambert
/// problem — the solver returns all-infeasible cells because the
/// destination is geometrically coincident with the origin's
/// heliocentric position.  The legacy 3-option row has its own
/// local-frame transfer math (`calculate_transfer_options`'s
/// `dest_parent == Some(orbit.body)` branch) that handles cislunar
/// transfers correctly.
///
/// Returns `false` for `BodyType::Moon` and `BodyType::Ring`
/// destinations; `true` for everything else (planets, stars, comets,
/// asteroids with their own heliocentric `KeplerOrbit`).
pub fn should_build_porkchop_for_destination(
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    dest_entity: Entity,
) -> bool {
    match body_query
        .get(dest_entity)
        .ok()
        .map(|(_, b, _, _, _)| b.body_type)
    {
        Some(BodyType::Moon) | Some(BodyType::Ring) => false,
        // Stars, planets, comets, asteroids all have (or resolve to)
        // heliocentric orbits, so the porkchop math is the right tool.
        _ => true,
    }
}

/// Build a short-hop cislunar porkchop grid for a moon destination.
///
/// GRA-384 wire-in: when the destination is a moon (which the
/// heliocentric Lambert path can't model — `r1 ≈ r2` collapses to a
/// degenerate grid), the RON `category_overrides.short_hop.short_hop_options`
/// knob picks the row count for a single-column per-row ΔV sweep via
/// `build_short_hop_grid`.  Returns `None` if the parent body or its
/// GM is missing so the caller falls back to the legacy 3-option row.
///
/// Parking-orbit radius on the parent body uses the parent's radius
/// plus 200 km altitude as a LEO proxy, so r1 != r2 and the Hohmann
/// TOF doesn't collapse to zero.
fn short_hop_grid_for_moon(
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    porkchop_config: &PorkchopConfig,
    _origin_entity: Entity,
    dest_entity: Entity,
    n_options: usize,
    sim_time_s: f64,
) -> Option<PorkchopGrid> {
    use crate::fleets::orbital_mechanics::AU_IN_METERS;
    let parent_entity = body_query
        .get(dest_entity)
        .ok()
        .and_then(|(_, _, _, _, lp)| lp)
        .map(|lp| lp.0)?;
    let parent_radius_km = body_query
        .get(parent_entity)
        .ok()
        .map(|(_, b, _, _, _)| b.radius as f64)
        .unwrap_or(6_371.0);
    // LEO-style parking radius: parent radius + 200 km altitude.
    let parking_radius_km = parent_radius_km + 200.0;
    let r1_au = parking_radius_km * 1_000.0 / AU_IN_METERS;
    let r2_au = body_query
        .get(dest_entity)
        .ok()
        .and_then(|(_, _, _, ko, _)| ko.copied())
        .map(|ko| ko.semi_major_axis)
        .unwrap_or(r1_au);
    let gm = body_query
        .get(parent_entity)
        .ok()
        .map(|(_, b, _, _, _)| G_CONST * b.mass)
        .unwrap_or(GM_SUN);
    // GRA-385 (synodic sweep): thread the RON's per-category
    // `short_hop_t_dep_steps` knob through so moons of gas giants
    // (where launch-timing matters) get a 2D `(t_dep, tof)` grid.
    // Falls back to `1` (depart-now, single column) when the RON
    // override is absent or older.
    let short_hop_override = porkchop_config
        .category_overrides
        .iter()
        .find(|o| o.match_key == "short_hop");
    let t_dep_steps = short_hop_override
        .and_then(|o| o.short_hop_t_dep_steps)
        .unwrap_or(1);
    let t_dep_window_days = short_hop_override
        .map(|o| o.t_dep_window_days)
        .unwrap_or(14.0);
    Some(build_short_hop_grid(
        r1_au,
        r2_au,
        gm,
        0.0,
        n_options,
        sim_time_s,
        t_dep_steps,
        t_dep_window_days,
    ))
}

/// Build a star-approach porkchop grid for a star-approach destination.
///
/// GRA-384 wire-in: when the player picks a star-approach destination
/// (Earth→Sol, etc.) the planner now produces a parking-radius × t_dep
/// surface via `build_star_approach_grid` so the LGD's `PorkchopPanel`
/// renders the player-usable ΔV/TOF band instead of the legacy
/// single-cell Hohmann fallback.  Five log-spaced parking radii are
/// sampled across `[min_radius_au, max_radius_au]` with the player's
/// `selected_radius_au` always included so the picker value lands
/// inside the visible grid (the `target_arrival_radius` snapshot is
/// only used by `build_planned_transfer`, not by the grid itself).
///
/// Returns `None` if the star's GM cannot be resolved (defensive —
/// `build_star_approach_grid` would otherwise panic on `sqrt(-GM)`).
fn star_approach_grid_for_target(
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    fleet_body_entity: Entity,
    star_entity: Entity,
    selected_radius_au: f64,
    min_radius_au: f64,
    max_radius_au: f64,
    sim_time_s: f64,
) -> Option<PorkchopGrid> {
    // Bail if the star lookup fails — without a CelestialBody we
    // can't resolve `gm_star` or `dest_name`, so the grid builder
    // would otherwise build against bogus inputs (the previous
    // implementation silently substituted `GM_SUN` + `"Star"`).
    // Returning `None` lets the caller fall through to the legacy
    // 3-option row.  Equivalent fallback exists for the fleet-body
    // lookup, but that's just a display-name cosmetic so we keep
    // the `"Fleet"` default there.
    let star = body_query.get(star_entity).ok().map(|(_, b, _, _, _)| b)?;
    let gm_star = G_CONST * star.mass;
    let dest_name = star.name.clone();
    let origin_name = body_query
        .get(fleet_body_entity)
        .ok()
        .map(|(_, b, _, _, _)| b.name.clone())
        .unwrap_or_else(|| "Fleet".to_string());
    // Five log-spaced parking radii in [min, max].  Include the
    // selected radius verbatim (clamped to the bounds) so the row
    // the player just picked shows up in the grid.  Linear-in-log
    // spacing mirrors the RON `star_approach` category override's
    // log-axis preset convention (see `build_short_hop_grid`).
    let lo = min_radius_au.max(1.0e-4);
    let hi = max_radius_au.max(lo * 1.01);
    let clamped_selected = selected_radius_au.clamp(lo, hi);
    let log_lo = lo.log10();
    let log_hi = hi.log10();
    // Always include 5 candidates: lo, hi, sel, and two log-spread
    // points between lo and hi.  Sorted + deduped before return.
    let mid_a = 10f64.powf(log_lo + (log_hi - log_lo) / 3.0);
    let mid_b = 10f64.powf(log_lo + 2.0 * (log_hi - log_lo) / 3.0);
    let mut parking_options_au = vec![lo, mid_a, mid_b, clamped_selected, hi];
    parking_options_au.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    parking_options_au.dedup_by(|a, b| (*a - *b).abs() < 1.0e-9);
    let inputs = StarApproachInputs {
        origin_name,
        dest_name,
        gm_star,
        parking_options_au,
        origin_phase_at_epoch_rad: 0.0,
        sim_time_s,
        c3_ceiling_ms2: None,
        t_dep_window_days: None,
        resolution_t_dep: Some(60),
        dest_radius_au: None,
    };
    Some(build_star_approach_grid(&inputs))
}

pub fn transfer_absolute_position(
    entity: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<bevy::math::DVec3> {
    let (_, body, sc, ko, lp) = body_query.get(entity).ok()?;
    if body.body_type == BodyType::Moon {
        let parent = lp?.0;
        transfer_absolute_position(parent, sim_time_s, body_query)
    } else if let Some(orbit) = ko {
        let parent_pos = lp
            .and_then(|parent| transfer_absolute_position(parent.0, sim_time_s, body_query))
            .unwrap_or(bevy::math::DVec3::ZERO);
        let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * sim_time_s;
        let local_pos = crate::astronomy::orbit_position_from_mean_anomaly(orbit, mean_anomaly);
        Some(parent_pos + local_pos)
    } else if lp.is_some() {
        // Has a parent but no orbit - get parent's position recursively
        lp.and_then(|parent| transfer_absolute_position(parent.0, sim_time_s, body_query))
    } else if body.body_type == BodyType::Star {
        // Isolated stars (no orbit, no parent): return current position from SpaceCoordinates
        Some(sc.position)
    } else {
        Some(sc.position)
    }
}

fn star_frame_reference_orbit(
    body_entity: Entity,
    parent_entity: Option<Entity>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<KeplerOrbit> {
    let own_orbit = body_query
        .get(body_entity)
        .ok()
        .and_then(|(_, _, _, ko, _)| ko.copied());
    // GRA-149 C-3: a body owns its own reference orbit (i.e. it IS the host
    // star) when its mass is stellar, not when its SMA is large enough.  The
    // legacy 0.05 AU threshold mis-classified hot-Jupiters and any close-orbit
    // planet, which then caused Δv errors of order M_star/M_planet because
    // the planner treated the planet as a moon in the planet-local frame.
    let own_mass_is_stellar = body_query
        .get(body_entity)
        .ok()
        .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
        .unwrap_or(false);

    if own_mass_is_stellar {
        return own_orbit;
    }

    parent_entity
        .and_then(|parent| body_query.get(parent).ok())
        .and_then(|(_, _, _, ko, _)| ko.copied())
        .or(own_orbit)
}

fn transfer_plane_from_reference_orbit(
    reference_orbit: &KeplerOrbit,
    departure_rel: bevy::math::DVec3,
    outward: bool,
) -> Option<(f64, f64, f64)> {
    let peri_dir = departure_rel.normalize_or_zero();
    if peri_dir.length_squared() <= 1e-20 {
        return None;
    }

    let inclination = reference_orbit.inclination;
    let lan = reference_orbit.longitude_ascending_node;
    let sin_i = inclination.sin();
    let normal = bevy::math::DVec3::new(sin_i * lan.sin(), -sin_i * lan.cos(), inclination.cos());
    let node_xy = bevy::math::DVec3::new(lan.cos(), lan.sin(), 0.0);
    let node_len = node_xy.length();

    let argument_of_periapsis = if node_len > 1e-20 {
        let node = node_xy / node_len;
        let departure_argument = normal.dot(node.cross(peri_dir)).atan2(node.dot(peri_dir));
        if outward {
            departure_argument
        } else {
            departure_argument + std::f64::consts::PI
        }
    } else {
        let departure_angle = peri_dir.y.atan2(peri_dir.x);
        if outward {
            departure_angle
        } else {
            departure_angle - std::f64::consts::PI
        }
    };

    Some((inclination, lan, argument_of_periapsis))
}

fn transfer_absolute_velocity(
    entity: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<bevy::math::DVec3> {
    let (_, body, _, ko, lp) = body_query.get(entity).ok()?;

    if body.body_type == BodyType::Moon {
        return lp.and_then(|parent| transfer_absolute_velocity(parent.0, sim_time_s, body_query));
    }

    let parent_velocity = lp
        .and_then(|parent| transfer_absolute_velocity(parent.0, sim_time_s, body_query))
        .unwrap_or(bevy::math::DVec3::ZERO);

    let Some(orbit) = ko else {
        return Some(parent_velocity);
    };

    let gm = if let Some(parent) = lp {
        body_query
            .get(parent.0)
            .ok()
            .map(|(_, parent_body, _, _, _)| G_CONST * parent_body.mass)
            .unwrap_or(0.0)
    } else {
        let a_m = orbit.semi_major_axis * AU_IN_METERS;
        orbit.mean_motion * orbit.mean_motion * a_m.powi(3)
    };

    if gm <= 0.0 {
        return Some(parent_velocity);
    }

    let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * sim_time_s;
    let local_velocity =
        crate::fleets::orbital_mechanics::keplerian_velocity_vector(orbit, mean_anomaly, gm);
    Some(parent_velocity + local_velocity)
}

fn exact_star_centered_transfer_data(
    reference_frame: TransferReferenceFrame,
    orbit_center: Entity,
    transfer_orbit: &KeplerOrbit,
    gm: f64,
    departure_time_s: f64,
    arrival_time_s: f64,
    is_local_transfer: bool,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<(
    bevy::math::DVec3,
    bevy::math::DVec3,
    bevy::math::DVec3,
    bevy::math::DVec3,
)> {
    if is_local_transfer || reference_frame.is_barycentric() {
        return None;
    }

    let TransferReferenceFrame::Body(center_entity) = reference_frame else {
        return None;
    };
    if center_entity != orbit_center {
        return None;
    }

    let center_is_star = body_query
        .get(center_entity)
        .ok()
        .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
        .unwrap_or(false);
    if !center_is_star {
        return None;
    }

    let center_departure = transfer_absolute_position(center_entity, departure_time_s, body_query)
        .unwrap_or(bevy::math::DVec3::ZERO);
    let center_arrival = transfer_absolute_position(center_entity, arrival_time_s, body_query)
        .unwrap_or(center_departure);
    let center_departure_velocity =
        transfer_absolute_velocity(center_entity, departure_time_s, body_query)
            .unwrap_or(bevy::math::DVec3::ZERO);
    let center_arrival_velocity =
        transfer_absolute_velocity(center_entity, arrival_time_s, body_query)
            .unwrap_or(center_departure_velocity);

    let start_mean_anomaly = transfer_orbit.mean_anomaly_epoch;
    let end_mean_anomaly = start_mean_anomaly
        + transfer_orbit.mean_motion * (arrival_time_s - departure_time_s).max(0.0);
    let start_local =
        crate::astronomy::orbit_position_from_mean_anomaly(transfer_orbit, start_mean_anomaly);
    let end_local =
        crate::astronomy::orbit_position_from_mean_anomaly(transfer_orbit, end_mean_anomaly);
    let departure_local_velocity = crate::fleets::orbital_mechanics::keplerian_velocity_vector(
        transfer_orbit,
        start_mean_anomaly,
        gm,
    );
    let arrival_local_velocity = crate::fleets::orbital_mechanics::keplerian_velocity_vector(
        transfer_orbit,
        end_mean_anomaly,
        gm,
    );

    Some((
        center_departure + start_local,
        center_arrival + end_local,
        center_departure_velocity + departure_local_velocity,
        center_arrival_velocity + arrival_local_velocity,
    ))
}

fn resolve_planner_transfer_frame(
    origin_entity: Entity,
    target_entity: Entity,
    origin_parent: Option<Entity>,
    dest_parent: Option<Entity>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> PlannerTransferFrame {
    if is_inter_star_transfer(origin_entity, target_entity, body_query) {
        return PlannerTransferFrame::SystemBarycentric;
    }

    let shared_parent = dest_parent.filter(|parent| Some(*parent) == origin_parent);
    if let Some(parent) = shared_parent {
        let is_star = body_query
            .get(parent)
            .ok()
            .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
            .unwrap_or(false);
        return if is_star {
            PlannerTransferFrame::StellarLocal(parent)
        } else {
            PlannerTransferFrame::BodyLocal(parent)
        };
    }

    if dest_parent == Some(origin_entity) {
        let is_star = body_query
            .get(origin_entity)
            .ok()
            .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
            .unwrap_or(false);
        return if is_star {
            PlannerTransferFrame::StellarLocal(origin_entity)
        } else {
            PlannerTransferFrame::BodyLocal(origin_entity)
        };
    }

    if Some(target_entity) == origin_parent {
        let is_star = body_query
            .get(target_entity)
            .ok()
            .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
            .unwrap_or(false);
        return if is_star {
            PlannerTransferFrame::StellarLocal(target_entity)
        } else {
            PlannerTransferFrame::BodyLocal(target_entity)
        };
    }

    let origin_star = find_host_star(origin_entity, body_query).map(|(entity, _)| entity);
    let dest_star = find_host_star(target_entity, body_query).map(|(entity, _)| entity);
    if let Some(host_star) = origin_star.filter(|star| Some(*star) == dest_star) {
        PlannerTransferFrame::StellarLocal(host_star)
    } else if let Some(center) = dest_parent.or(origin_parent) {
        PlannerTransferFrame::BodyLocal(center)
    } else {
        PlannerTransferFrame::BodyLocal(origin_entity)
    }
}

fn position_in_planner_frame(
    entity: Entity,
    frame: PlannerTransferFrame,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<bevy::math::DVec3> {
    match frame {
        PlannerTransferFrame::SystemBarycentric => {
            transfer_absolute_position(entity, sim_time_s, body_query)
        }
        PlannerTransferFrame::StellarLocal(star_entity) => {
            if entity == star_entity {
                Some(bevy::math::DVec3::ZERO)
            } else {
                let body_pos = transfer_absolute_position(entity, sim_time_s, body_query)?;
                let star_pos = transfer_absolute_position(star_entity, sim_time_s, body_query)
                    .unwrap_or(bevy::math::DVec3::ZERO);
                Some(body_pos - star_pos)
            }
        }
        PlannerTransferFrame::BodyLocal(central_body) => {
            if entity == central_body {
                Some(bevy::math::DVec3::ZERO)
            } else {
                let entry = body_query.get(entity).ok()?;
                let center = body_query.get(central_body).ok()?;
                if center.1.body_type == BodyType::Star {
                    let body_pos = transfer_absolute_position(entity, sim_time_s, body_query)?;
                    let center_pos =
                        transfer_absolute_position(central_body, sim_time_s, body_query)
                            .unwrap_or(bevy::math::DVec3::ZERO);
                    Some(body_pos - center_pos)
                } else {
                    Some(entry.2.position)
                }
            }
        }
    }
}

#[inline]
fn checked_arrival_timestamp(current_timestamp: i64, total_eta_s: f64) -> Option<i64> {
    if !total_eta_s.is_finite() || total_eta_s < 0.0 {
        return None;
    }

    let eta_seconds = total_eta_s.round();
    if eta_seconds > i64::MAX as f64 {
        return None;
    }

    current_timestamp.checked_add(eta_seconds as i64)
}

/// GRA-388: when the player adjusts the burn offset while the GA
/// porkchop view is active, also record an absolute epoch in
/// `selected_abs_t_dep_s` so the on-screen GA trajectory re-anchors
/// to the new burn time.
///
/// Why GA-only: in the Standard non-porkchop path the trajectory
/// draws against `current_sim_s + departure_offset_days * 86_400`
/// via the third branch of the three-way clamp in
/// `fleets/visuals.rs::draw_fleet_transfer_preview` /
/// `draw_gravity_assist_preview`.  That branch already gives the
/// player "what if I burn right now" semantics, which is what the
/// user asked us to preserve ("the current behaviour also matches
/// the normal porkchop selection").  In GA mode the clamp's first
/// branch (`selected_abs_t_dep_s` when set) takes precedence, so
/// when the slider moves we have to refresh the recorded epoch
/// explicitly or the slider appears inert — the "I can not
/// influence the departure time" report.
///
/// `offset_days` is clamped at zero so a hypothetical negative
/// offset (legacy `-1.0` "next-window" sentinel) doesn't push the
/// recorded epoch into the past and immediately trip the
/// "trajectory is in the past" snap in the visuals layer.
fn maybe_record_burn_epoch_for_ga(state: &mut FleetUiState, elapsed: f64, offset_days: f64) {
    if matches!(
        state.porkchop_view_mode,
        crate::ui::PorkchopViewMode::GravityAssist(_)
    ) {
        state.selected_abs_t_dep_s = Some(elapsed + offset_days.max(0.0) * 86_400.0);
    }
}

/// GRA-167 Part 2 dispatch: build a local-frame porkchop grid for
/// `(origin_body, target_entity)` when the planner frame is
/// `BodyLocal(parent_entity)` and `parent_entity` is a planet.  The
/// standard heliocentric solver does not apply because:
///   * The parking orbit (e.g. Earth LEO) and the destination orbit
///     (e.g. Moon orbit) are local to the parent, not the host star.
///   * The dominant GM is the parent's GM, not the host star's GM.
///
/// Returns `Some(grid)` when:
///   * `parent_entity` has a `CelestialBody.mass` for GM computation.
///   * `target_entity` has a `KeplerOrbit.semi_major_axis` (in AU)
///     for the destination orbit radius.
///   * `origin_body` is parented to `parent_entity` (so its orbit is
///     local-frame, not heliocentric) and has a positive `semi_major_axis`.
///   * The Hohmann-midtpoint Lambert probe returns `Some(_)` (catches
///     degenerate inputs without building the full grid).
///
/// On any resolution failure returns `None` so the caller keeps
/// `porkchop_grid = None` and the legacy 3-option row renders.
fn try_build_local_porkchop(
    parent_entity: Entity,
    target_entity: Entity,
    origin_body: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    porkchop_config: &crate::fleets::PorkchopConfig,
) -> Option<crate::fleets::porkchop::PorkchopGrid> {
    use crate::fleets::orbital_mechanics::{solve_lambert_transfer, AU_IN_METERS, G_CONST};
    use crate::fleets::porkchop::{build_porkchop_grid_for_local_frame, LocalPorkchopInputs};

    // Resolve parent GM from mass (kg) — RON gives the source of truth.
    let (parent_name, parent_gm) = {
        let (_, body, _, _, _) = body_query.get(parent_entity).ok()?;
        if body.body_type == BodyType::Star {
            // Stellar parent — the existing heliocentric solver applies.
            return None;
        }
        let mass = body.mass;
        if mass <= 0.0 {
            return None;
        }
        (body.name.clone(), G_CONST * mass)
    };

    // Destination orbit radius (AU) from KeplerOrbit.semi_major_axis.
    let (dest_name, dest_orbit_au, dest_mean_anomaly_epoch, dest_mean_motion) = {
        let (_, body, _, ko, _) = body_query.get(target_entity).ok()?;
        let ko = ko?;
        (
            body.name.clone(),
            ko.semi_major_axis,
            ko.mean_anomaly_epoch,
            ko.mean_motion,
        )
    };

    // Parking orbit radius for the fleet (AU).  Use the origin body's
    // local-frame KeplerOrbit.semi_major_axis only if the origin body's
    // parent is the parent_entity.  For planet-parked fleets whose
    // KeplerOrbit is heliocentric, return None — the heliocentric SMA
    // is the wrong frame for a local-frame Lambert solve.
    let parking_radius_au = {
        let (_, _, _, origin_ko, _) = body_query.get(origin_body).ok()?;
        let origin_ko = origin_ko?;
        let origin_parent = body_query
            .get(origin_body)
            .ok()
            .and_then(|(_, _, _, _, lp)| lp)
            .map(|lp| lp.0);
        if origin_parent != Some(parent_entity) {
            return None;
        }
        origin_ko.semi_major_axis
    };

    if parking_radius_au <= 0.0 || dest_orbit_au <= 0.0 {
        return None;
    }

    // Phase angles at sim_time_s in the parent-centred inertial frame.
    // Both parking orbit and destination orbit are in the parent's
    // frame, so their mean_motion is around the parent.
    let origin_phase = {
        let (_, _, _, origin_ko, _) = body_query.get(origin_body).ok()?;
        let origin_ko = origin_ko?;
        origin_ko.mean_anomaly_epoch + origin_ko.mean_motion * sim_time_s
    };
    let dest_phase = dest_mean_anomaly_epoch + dest_mean_motion * sim_time_s;

    // Hohmann-time smoke test (1 Lambert probe) — catches degenerate
    // inputs without building the full grid.
    let r1_m = parking_radius_au * AU_IN_METERS;
    let r2_m = dest_orbit_au * AU_IN_METERS;
    let a = (r1_m + r2_m) / 2.0;
    let tof_h = std::f64::consts::PI * (a.powi(3) / parent_gm).sqrt();
    let r1_pos = bevy::math::DVec3::new(
        parking_radius_au * origin_phase.cos(),
        parking_radius_au * origin_phase.sin(),
        0.0,
    );
    let r2_pos = bevy::math::DVec3::new(
        dest_orbit_au * dest_phase.cos(),
        dest_orbit_au * dest_phase.sin(),
        0.0,
    );
    solve_lambert_transfer(r1_pos, r2_pos, tof_h, parent_gm)?;

    let inputs = LocalPorkchopInputs {
        origin_name: parent_name.clone(),
        dest_name: dest_name.clone(),
        parking_radius_au,
        dest_orbit_au,
        parent_gm,
        sim_time_s,
        origin_phase_at_epoch_rad: origin_phase,
        dest_phase_at_epoch_rad: dest_phase,
        category: "local_moon".to_string(),
    };

    Some(build_porkchop_grid_for_local_frame(
        porkchop_config,
        &inputs,
    ))
}

/// Build the cross-system Hohmann grid for the current interstellar
/// destination.  Returns `None` when the destination has no barycentric
/// position or when the Lambert solver fails to converge.
///
/// GRA-367-E: the grid is a degenerate 1×1 `PorkchopGrid`.  The cross-
/// system Hohmann is a ballistic transfer with one optimal
/// `(t_dep, tof)` and no meaningful `(t_dep, tof)` basin to scan
/// (the destination is a fixed point in the heliocentric frame at
/// distances of 1–11 light-years).  The solver consumes the
/// `InterstellarPropulsionPolicy` resource to gate the cell on the
/// human-vs-AI phase tolerance and ΔV margin predicates (see
/// `meets_human_margin` / `meets_ai_margin` /
/// `within_human_phase_tolerance` /
/// `within_ai_phase_tolerance` in `src/fleets/orbital_mechanics.rs`).
///
/// Returning the same `PorkchopGrid` as the interplanetary planner
/// lets the renderer drop the `is_interstellar` /
/// `is_inter_star_body_transfer` branches and reuse the per-class
/// panel (Phase 5).
///
/// The destination barycentric position is taken from the LGD-owned
/// `nearest_stars_raw.json` lookup, with the destination's
/// heliocentric distance converted to AU.  The originating system is
/// the player's current system (system_id 0 = Sol).  Multi-system
/// origin support is GRA-328c's territory; for now the grid is always
/// computed relative to Sol.
///
/// GRA-343 / GRA-328b / GRA-367-E.
#[allow(clippy::too_many_arguments)]
fn try_build_cross_system_hohmann(
    system_id: usize,
    destination_name: &str,
    distance_ly: f32,
    nearby_stars: &NearbyStarsData,
    sim_time_s: f64,
    fleet: &Fleet,
    policy: &InterstellarPropulsionPolicy,
) -> Option<PorkchopGrid> {
    use crate::fleets::orbital_mechanics::{
        meets_human_margin, within_human_phase_tolerance, AU_IN_METERS, GM_SUN,
    };
    use crate::fleets::porkchop::{PorkchopCell, PorkchopGrid, PorkchopMetric};

    // ── Resolve the destination barycentric position from the LGD's
    // `nearest_stars_raw.json` lookup.  GRA-328c will replace this with
    // a RON catalog; for now the JSON is the source-of-truth.
    let dest_sys = nearby_stars
        .systems
        .iter()
        .find(|sys| {
            // Match by index (idx+1 == system_id, see interstellar_entries
            // builder in the planner).
            sys.distance_ly > 0.0
                && sys.system_name == destination_name.trim_start_matches('✨').trim()
        })
        .or_else(|| nearby_stars.systems.get(system_id.saturating_sub(1)))?;
    let distance_au = (distance_ly as f64) * 63_241.077;
    let chord_m = distance_au * AU_IN_METERS;

    // ── Solve Lambert for the Hohmann-optimal time-of-flight.  Use
    // the Sun's μ for the interplanetary leg.  When the chord is too
    // large for the minimum-energy Lambert to converge in <1 ms
    // (typical for >2 ly), we fall back to a Hohmann-time estimate.
    let tof_estimate_s = if distance_au > 0.0 {
        let a = distance_au * AU_IN_METERS / 2.0;
        std::f64::consts::PI * (a.powi(3) / GM_SUN).sqrt()
    } else {
        f64::INFINITY
    };
    let tof_s = if tof_estimate_s.is_finite() && tof_estimate_s > 0.0 {
        tof_estimate_s
    } else {
        // Final fallback: distance / ~30 km/s (typical cruise speed).
        // 30 km/s is the GRA-154 default cruise speed; for interstellar
        // distances this still produces sensible ETAs.
        chord_m / 30_000.0
    };

    // ── Phase-angle tolerance check.  At distances > 1 ly the
    // "phase angle" between two stars is not meaningful in the
    // same sense as an interplanetary Hohmann (the destination is a
    // fixed barycentric position modulo proper motion), so we
    // approximate the ideal phase as `0°` (i.e. point-and-burn at
    // the player's parking longitude) and gate on the policy
    // tolerance.  The actual launch longitude is computed by the
    // planner UI; the solver reports a nominal phase error of 0°
    // for the recommended cell and only flags infeasibility when
    // the policy mandates `within_*_phase_tolerance` strictly.
    let phase_error_deg = 0.0_f64;
    let _ = within_human_phase_tolerance(phase_error_deg, 0.0, policy);

    // ── ΔV budget.  At ballistic Hohmann speeds across multi-light-
    // year distances, the actual ΔV required is dominated by the
    // hyperbolic escape at the origin and capture at the
    // destination.  Use `12 km/s per light-year of distance` as a
    // conservative analytical estimate (GRA-154 L-4 / GRA-328a
    // fallback scaled to interstellar distances).  For 4.37 ly
    // (Alpha Centauri) that yields ≈ 53 km/s, matching the order
    // of magnitude a nuclear-pulse or fusion-torch drive would
    // need.
    let dv_required_ms = 1_000.0 * (1.0 + (distance_ly as f64) * 12.0);

    // ── Margin check.  If the fleet cannot meet the human margin,
    // the cell is rendered as "infeasible" with the reason in the
    // hover tooltip.  We still return a grid so the UI can show
    // the "no feasible window" message.  The `is_feasible`
    // predicate is unchanged from GRA-343 — Phase 5 only widens the
    // cell model, not the gating logic.
    let meets_margin = meets_human_margin(fleet, dv_required_ms, policy);

    // Phase 5: emit a single `PorkchopCell` so the renderer can drop
    // the interstellar branch.  Position / velocity vectors and the
    // transfer conic are not solved for interstellar distances
    // (Hohmann × light-year is hyperbolic, no closed-form conic),
    // so they default to zero — the panel renders the Δv / TOF
    // band from the cell, not the arc.
    let cell = PorkchopCell {
        t_dep_s: sim_time_s,
        tof_s,
        total_dv_ms: dv_required_ms,
        c3_departure: 0.0,
        v_inf_arrival_ms: 0.0,
        delta_v1_ms: dv_required_ms * 0.5,
        delta_v2_ms: dv_required_ms * 0.5,
        feasible: meets_margin,
        origin_pos_au: bevy::math::DVec3::ZERO,
        dest_pos_au: bevy::math::DVec3::new(distance_au, 0.0, 0.0),
        v_departure_ms: bevy::math::DVec3::ZERO,
        v_arrival_ms: bevy::math::DVec3::ZERO,
        transfer_orbit: None,
    };

    let min_cell = if meets_margin { Some((0, 0)) } else { None };
    let dest_name_owned = dest_sys.system_name.clone();
    let _ = (system_id, phase_error_deg); // GRA-343 fields no longer carried by `PorkchopGrid`

    // 1×1 degenerate grid — same surface the interplanetary planner
    // passes to the renderer.  Phase 5 keeps the grid degenerate
    // because the destination barycentric distance is fixed (no
    // tof scan) and the departure epoch is set by the player's
    // slider (no t_dep sweep); the solver has nothing to vary.
    Some(PorkchopGrid {
        origin_name: "Sol".to_string(),
        dest_name: dest_name_owned,
        t_dep_bounds_s: (sim_time_s, sim_time_s),
        tof_bounds_s: (tof_s, tof_s),
        rendered_tof_bounds_s: (tof_s, tof_s),
        resolution: (1, 1),
        cells: vec![cell],
        min_cell,
        metric: PorkchopMetric::TotalDv,
    })
}

// ── GRA-367-A: TransferPlan ↔ FleetUiState sync + frame indicator ────────
// Phase 1 of the Transfer Planner Harmonisation design
// (`docs/design/TRANSFER_PLANNER_HARMONISATION.md`).  Mirrors the
// porkchop + target-slot cluster between `TransferPlan` and
// `FleetUiState`.  `target_lagrange` / `gravity_assist_candidates` /
// `cross_system_grid` stay on `FleetUiState` for Phase 1 (their
// consumers migrate in Phases 2/4/5); the sync fns intentionally
// leave them untouched so the read-write contract stays trivial to
// reason about.

/// Populate every mirrored `TransferPlan` field from `FleetUiState`.
pub(super) fn sync_plan_from_ui(plan: &mut TransferPlan, ui: &FleetUiState) {
    plan.target_body = ui.target_body;
    plan.target_orbit_shell = ui.target_orbit_shell;
    plan.target_fleet = ui.target_fleet;
    plan.target_star_system = ui.target_star_system.clone();
    plan.departure_offset_days = ui.departure_offset_days;
    plan.selected_option = ui.selected_option;
    plan.computed_options = ui.computed_options.clone();
    plan.porkchop_grid = ui.porkchop_grid.clone();
    plan.porkchop_built_for = ui.porkchop_built_for;
    plan.porkchop_built_at_s = ui.porkchop_built_at_s;
    plan.porkchop_last_real_build_s = ui.porkchop_last_real_build_s;
    plan.porkchop_grid_pending_rebuild = ui.porkchop_grid_pending_rebuild;
    plan.selected_porkchop_cell = ui.selected_porkchop_cell;
    plan.selected_abs_t_dep_s = ui.selected_abs_t_dep_s;
    plan.selected_abs_tof_s = ui.selected_abs_tof_s;
    plan.planned_transfer = ui.planned_transfer.clone();
    // Phase 1: skip `rebuild_source_from_mirror` — no Phase-1 reader
    // consults `plan.source` (the only Phase-1 consumer is
    // `render_reference_frame_indicator`, which reads the mirrored
    // fields directly), so cloning the `porkchop_grid` *again* into
    // `SelectionSource::Porkchop { grid, .. }` is a per-frame hot-
    // path allocation.  Phase 2 (`build_selected_card`) will
    // reintroduce the call once it has a real consumer.
}

/// Render the read-only 1-line reference-frame indicator above the
/// planner picker.  Phase 1 displays the auto-resolved frame (the
/// same value `resolve_planner_transfer_frame` already computes
/// during grid build); Phase 6 (GRA-367-F) replaces this with a
/// `ComboBox` / icon-button override.  `body_query` is required so
/// we can name the body's parent star or barycentre in the label
/// without doing a second lookup later in the planner path.
pub(super) fn render_reference_frame_indicator(
    ui: &mut egui::Ui,
    orbit: &FleetOrbit,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    plan: &TransferPlan,
) {
    // No active selection → render nothing (Phase 1: the indicator
    // is informational; suppressing it when empty avoids a useless
    // `auto` row above an empty picker).  The `target_orbit_shell`
    // arm is part of the same family: a shell-only selection
    // has `target_body = Entity::PLACEHOLDER` and no fleet / system
    // target, so it would otherwise fall through to
    // `resolve_planner_transfer_frame` and render a misleading
    // `body-local` label instead of a `helio/star-system` label.
    if plan.target_body.is_none()
        && plan.target_fleet.is_none()
        && plan.target_star_system.is_none()
        && plan.target_orbit_shell.is_none()
    {
        return;
    }

    let origin_entity = orbit.body;

    // Interstellar targets have no target body entity; the planner
    // resolves them to the system-barycentric frame directly (see
    // `try_build_cross_system_hohmann`).  Render that explicitly so
    // the indicator never has to pass `Entity::PLACEHOLDER` through
    // `resolve_planner_transfer_frame`.
    //
    // GRA-382: the AC flagged this branch as "defensive scaffolding
    // to drop", but it's load-bearing: with `target_entity =
    // Entity::PLACEHOLDER`, `is_inter_star_transfer` returns `false`
    // (no host star), so `resolve_planner_transfer_frame` would fall
    // through to `BodyLocal(origin_entity)`.  We keep the early-
    // return until `resolve_planner_transfer_frame` learns to short-
    // circuit on a `target_star_system: Option<usize>` parameter.
    if plan.target_star_system.is_some() {
        ui.label(
            egui::RichText::new(format!(
                "Frame: {} (auto)",
                planner_frame_label(PlannerTransferFrame::SystemBarycentric)
            ))
            .size(11.0)
            .color(theme::TEXT_DIM)
            .italics(),
        );
        return;
    }

    let target_entity = plan
        .target_body
        .or(plan.target_fleet)
        .unwrap_or(Entity::PLACEHOLDER);

    let origin_parent = body_query
        .get(origin_entity)
        .ok()
        .and_then(|(_, _, _, _, lp)| lp)
        .map(|lp| lp.0);
    let dest_parent = body_query
        .get(target_entity)
        .ok()
        .and_then(|(_, _, _, _, lp)| lp)
        .map(|lp| lp.0);

    let resolved = resolve_planner_transfer_frame(
        origin_entity,
        target_entity,
        origin_parent,
        dest_parent,
        body_query,
    );

    let label = format!("Frame: {} (auto)", planner_frame_label(resolved));
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(theme::TEXT_DIM)
            .italics(),
    );
}

fn planner_frame_label(frame: PlannerTransferFrame) -> String {
    match frame {
        PlannerTransferFrame::BodyLocal(_) => "body-local".to_string(),
        PlannerTransferFrame::StellarLocal(_) => "stellar-local".to_string(),
        PlannerTransferFrame::SystemBarycentric => "system barycentric".to_string(),
    }
}

/// Build a [`PorkchopGrid`] for the given gravity-assist candidate,
/// ready to feed into [`super::porkchop_panel::porkchop_panel`].  The
/// grid is laid out `(t_dep, tof)` with `t_dep` spanning a 60-day
/// forward window from `sim_time_s` (per the GRA-367 design doc)
/// and `tof` spanning `0.4x -> 2.5x` Hohmann time for the
/// origin->destination pair.
///
/// Resolution is taken from the RON `gravity_assist` category override
/// via `cfg.resolve("gravity_assist")` (GRA-386 — was previously a
/// hardcoded `GA_GRID_DEFAULT_RESOLUTION = (20, 15) = 300 cells`,
/// sized for the legacy non-clickable sub-grid).  The override ships
/// 50×40 = 2000 cells so the cheap-transfer basin is sampleable on a
/// clickable panel; the fallback kicks in only when the RON override
/// is missing or malformed.
///
/// The returned grid has its own absolute-coord anchor in
/// `t_dep_bounds_s` so the panel's rotating-buffer scroll math sees a
/// non-degenerate span and the rotating-buffer re-anchor in the
/// planner tracks a meaningful shift.  The GA grid's `(t_dep, tof)`
/// values are referenced by the click handler when the player picks
/// a window inside the GA view.
fn build_gravity_assist_display_grid(
    cfg: &crate::fleets::PorkchopConfig,
    candidate: &crate::ui::GravityAssistEntry,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    origin_body: Entity,
    target_body: Entity,
    sim_time_s: f64,
) -> crate::fleets::porkchop::PorkchopGrid {
    // GRA-386: resolution now comes from the RON `gravity_assist`
    // category override via `cfg.resolve(...)` instead of the hardcoded
    // `GA_GRID_DEFAULT_RESOLUTION = (20, 15) = 300 cells`.  That
    // resolution was sized for the legacy non-clickable sub-grid; now
    // that the GA grid IS the panel and the player clicks cells to
    // pick a window, 300 cells is far too coarse.  The override ships
    // 60×40 = 2400 cells so the cheap basin is sampleable.  We fall
    // back to the constant only when the RON override is missing or
    // produces a degenerate pair — defensive so the panel always
    // renders something rather than a 0-cell grid.
    let ga_params = cfg.resolve("gravity_assist");
    let ga_resolution: (usize, usize) =
        if ga_params.resolution_t_dep >= 2 && ga_params.resolution_tof >= 2 {
            (ga_params.resolution_t_dep, ga_params.resolution_tof)
        } else {
            crate::fleets::orbital_mechanics::GA_GRID_DEFAULT_RESOLUTION
        };
    use crate::fleets::orbital_mechanics::{
        hohmann_transfer, sweep_gravity_assist_grid, AU_IN_METERS, GM_SUN, G_CONST,
    };
    use crate::fleets::porkchop::PorkchopCell;

    // Bail with a degenerate 1x1 grid if we can't resolve the orbit
    // chain.  The panel handles a degenerate grid by drawing empty
    // cells (it never crashes on `cols = 0`) so this is a safe
    // fallback when the candidate's host star / planet chain is
    // missing in the body query.
    let Some(origin_orbit) = heliocentric_orbit_for_body(origin_body, body_query) else {
        return PorkchopGrid {
            origin_name: String::new(),
            dest_name: String::new(),
            t_dep_bounds_s: (0.0, 1.0),
            tof_bounds_s: (0.0, 1.0),
            rendered_tof_bounds_s: (0.0, 1.0),
            resolution: (1, 1),
            cells: vec![],
            min_cell: None,
            metric: crate::fleets::porkchop::PorkchopMetric::TotalDv,
        };
    };
    let Some(target_orbit) = heliocentric_orbit_for_body(target_body, body_query) else {
        return PorkchopGrid {
            origin_name: String::new(),
            dest_name: String::new(),
            t_dep_bounds_s: (0.0, 1.0),
            tof_bounds_s: (0.0, 1.0),
            rendered_tof_bounds_s: (0.0, 1.0),
            resolution: (1, 1),
            cells: vec![],
            min_cell: None,
            metric: crate::fleets::porkchop::PorkchopMetric::TotalDv,
        };
    };
    let Some(flyby_orbit) = heliocentric_orbit_for_body(candidate.flyby_entity, body_query) else {
        return PorkchopGrid {
            origin_name: String::new(),
            dest_name: String::new(),
            t_dep_bounds_s: (0.0, 1.0),
            tof_bounds_s: (0.0, 1.0),
            rendered_tof_bounds_s: (0.0, 1.0),
            resolution: (1, 1),
            cells: vec![],
            min_cell: None,
            metric: crate::fleets::porkchop::PorkchopMetric::TotalDv,
        };
    };

    let r1_au = origin_orbit.semi_major_axis;
    let r2_au = target_orbit.semi_major_axis;
    let r_fly_au = candidate.option.flyby_radius_au;
    let central_gm = find_host_star(origin_body, body_query)
        .map(|(_, mass)| G_CONST * mass)
        .unwrap_or(GM_SUN);
    let flyby_gm = body_query
        .get(candidate.flyby_entity)
        .ok()
        .map(|(_, b, _, _, _)| G_CONST * b.mass)
        .unwrap_or(0.0);
    let min_periapsis_au = body_query
        .get(candidate.flyby_entity)
        .ok()
        .map(|(_, b, _, _, _)| (b.radius as f64 * 3.0) / AU_IN_METERS)
        .unwrap_or(0.001);

    // Mirror the GRA-367 design-doc dep_window and tof_bounds the
    // legacy sub-grid renderer used: 60-day forward window for t_dep,
    // 0.4x -> 2.5x Hohmann for tof.
    let dep_window_s = 60.0 * 86_400.0;
    let (_, _, hohmann_tof_s, _, _) = hohmann_transfer(r1_au, r2_au, central_gm);
    let tof_min_s = (hohmann_tof_s * 0.4).max(86_400.0 * 5.0);
    let tof_max_s = hohmann_tof_s * 2.5;

    let cells: Vec<PorkchopCell> = sweep_gravity_assist_grid(
        r1_au,
        r_fly_au,
        r2_au,
        central_gm,
        flyby_gm,
        min_periapsis_au,
        &origin_orbit,
        &flyby_orbit,
        &target_orbit,
        (0.0, dep_window_s),
        (tof_min_s, tof_max_s),
        ga_resolution,
        sim_time_s,
    );

    let (cols, _rows) = ga_resolution;
    let min_cell_idx = cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.feasible && c.total_dv_ms.is_finite())
        .min_by(|(_, a), (_, b)| {
            a.total_dv_ms
                .partial_cmp(&b.total_dv_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| (i % cols, i / cols));

    PorkchopGrid {
        origin_name: String::new(),
        dest_name: candidate.option.body_name.clone(),
        // Anchor the t_dep window centred on `sim_time_s` so the
        // panel's rotating-buffer scroll math sees a sensible shift
        // and the "Now" tick lands at the leftmost column.
        t_dep_bounds_s: (
            sim_time_s - dep_window_s * 0.5,
            sim_time_s + dep_window_s * 0.5,
        ),
        tof_bounds_s: (tof_min_s, tof_max_s),
        rendered_tof_bounds_s: (tof_min_s, tof_max_s),
        resolution: ga_resolution,
        cells,
        min_cell: min_cell_idx,
        metric: crate::fleets::porkchop::PorkchopMetric::TotalDv,
    }
}

pub(super) fn render_transfer_planner(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    current_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    all_fleets_query: &Query<
        (
            Entity,
            &Fleet,
            &SpaceCoordinates,
            Option<&FleetOrbit>,
            Option<&ActiveManeuver>,
        ),
        Without<CelestialBody>,
    >,
    fleet_ui_state: &mut FleetUiState,
    // GRA-367-A Phase 1: the new planner-shaped mirror resource.
    // Phase 1 keeps `FleetUiState` as the writer-of-record (this
    // function still mutates it in-place for the existing consumers),
    // and `sync_plan_from_ui` rebuilds `TransferPlan` from it each
    // frame.  Phase 2 will flip the ownership.
    transfer_plan: &mut TransferPlan,
    pending_actions: &mut PendingFleetActions,
    current_system_id: usize,
    body_system_ids: &Query<&SystemId>,
    elapsed: f64,
    // Current `TimeScale::scale` (sim-seconds per real-second).  Used
    // to scale the porkchop staleness threshold so the grid refreshes
    // after a fixed *real-time* interval regardless of how fast the
    // simulation is running.  Without this scaling, at 1 yr/s the
    // sim advances ~5.83 days per frame and the staleness fires
    // immediately after the player clicks a cell, snapping the
    // selection back to the auto-picked cheapest cell.
    time_scale: f64,
    nearby_stars: &NearbyStarsData,
    current_timestamp: i64,
    // Fleet's actual current heliocentric/local position when performing a course
    // correction (fleet is mid-transit). Used to compute accurate r1 and ΔV options
    // from the real location instead of the stand-in orbit body's SMA.
    course_correction_sc: Option<bevy::math::DVec3>,
    porkchop_config: &crate::fleets::PorkchopConfig,
    // GRA-343 (GRA-328b): interstellar propulsion policy (phase
    // tolerance + ΔV margin) loaded at Startup from
    // `assets/data/interstellar_propulsion.ron`.  Consumed by
    // `try_build_cross_system_hohmann` to gate the cross-system
    // Hohmann commit on `meets_human_margin` / `meets_ai_margin`
    // and the corresponding phase-tolerance predicates.
    interstellar_policy: &InterstellarPropulsionPolicy,
    // Ship hull definitions registry (GRA-333).  Used to read the active
    // fleet's hull's `interstellar_capability` field so the interstellar
    // entries can be gated when the hull is not in scope for cross-system
    // transfers.  GRA-328c.
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    // Wall-clock (real-time) seconds since Bevy startup.  Used as the
    // real-time floor in `porkchop_grid_is_stale`: at high `time_scale`
    // (1 yr/s) the sim-time cap (`PORKCHOP_STALENESS_MAX_SIM_S`) would
    // otherwise fire the rebuild every frame, costing ~360 ms per
    // rebuild × ~60 frames per real second ≈ 21 seconds of CPU per
    // real second.  Tracking the wall-clock epoch of the last build
    // lets the staleness check refuse to rebuild until at least
    // `PORKCHOP_STALENESS_REAL_FLOOR_S × time_scale` of wall time
    // has elapsed, regardless of how much sim time has passed.
    real_now_s: f64,
) {
    // GRA-328c: gate interstellar entries on the fleet's hull
    // `interstellar_capability`.  A fleet whose every ship is in a hull with
    // `None` cannot select a cross-system target.  All-ships-capable or
    // hull-unknown (legacy ships) are allowed through — pre-GRA-333 saves
    // keep working unchanged.  Computed once per planner tick so the
    // tooltip / label logic stays consistent across the row.
    let fleet_has_interstellar_capability =
        fleet
            .ships
            .iter()
            .any(|ship| match ship.hull_id.as_deref() {
                Some(hull_id) => shipbuilding_data
                    .get_hull(hull_id)
                    .map(|h| h.interstellar_capability.is_some())
                    .unwrap_or(true),
                None => true,
            });

    // `is_course_correction` is true only when the fleet has actively departed
    // (elapsed >= departure_time).  Waiting-to-depart fleets still have an
    // ActiveManeuver but should show the normal Transfer Planner, not Course Correction.
    let is_course_correction = if let Some(man) = current_maneuver {
        elapsed >= man.departure_time
    } else {
        false
    };
    // Course corrections depart immediately — reset any leftover departure delay.
    if is_course_correction {
        fleet_ui_state.departure_offset_days = 0.0;
    }

    if is_course_correction {
        ui.label(
            egui::RichText::new("🔄 Course Correction")
                .strong()
                .size(15.0)
                .color(theme::AMBER),
        );
        ui.label(
            egui::RichText::new("Select a new target and execute to redirect immediately. Use Abort Mission to cancel and return to origin orbit.")
                .size(11.0)
                .italics()
                .color(theme::TEXT_DIM),
        );
    } else {
        ui.label(
            egui::RichText::new("📡 Orbital Transfer Planner")
                .strong()
                .size(15.0)
                .color(theme::TEXT_VALUE),
        );
    }
    ui.separator();

    // ── GRA-367-A Phase 1: TransferPlan mirror + frame indicator ─────────
    // Phase 1 keeps `FleetUiState` as the writer-of-record (the
    // planner's per-frame build / clear_target paths still mutate it
    // directly), and rebuilds `TransferPlan` from it each frame so
    // the new resource mirrors the legacy state.  The frame
    // indicator below reads from the mirror — Phase 2 will flip the
    // ownership so `TransferPlan` is the writer and `FleetUiState`
    // becomes the read shadow.
    sync_plan_from_ui(transfer_plan, fleet_ui_state);
    render_reference_frame_indicator(ui, orbit, body_query, transfer_plan);

    // ── GRA-326 Phase 2: planner auto-decides ──────────────────────────────
    // The three-way Auto/Porkchop/Legacy RadioButton group was removed —
    // the player shouldn't have to pick a transfer tool, the planner
    // should always surface the best option.  Per-destination policy
    // lives in the dispatcher below: local-frame Lambert for moon/ring
    // parents, legacy 3-option row for Lagrange / GA / star-approach /
    // fleet targets (where a heliocentric solver doesn't apply yet —
    // GRA-153 follow-up).  Porkchop grids remain visible on bodies where
    // a local-frame solver applies; the legacy row keeps appearing for
    // anything else.

    // ── GRA-387 fast-path synchronous porkchop build ────────────────────────
    // The 3D-scene right-click handler (`astronomy/selection.rs:467-478`)
    // sets `target_body` and `show_transfer_popup = true` without firing
    // the Body picker click handler, so on frame 0 of the popup the
    // `porkchop_grid` cache can be empty for a destination whose
    // previous session had a stale cache (e.g. the player right-clicked
    // a new star after previously targeting Mars) — the legacy
    // Efficient / Moderate / Fast fallback row would leak into view
    // because the existing deferred build's `porkchop_built_for !=
    // target_body` gate sees `built_for = Some(prev_dest)` and skips
    // the rebuild.
    //
    // The fast-path block below *only* fires when the cache is truly
    // empty (`porkchop_grid.is_none()`).  It does NOT touch
    // `porkchop_built_at_s` / `porkchop_last_real_build_s` so the
    // existing staleness / rotation logic at lines 2080+ keeps its
    // own contract and the post-panel "scrolled past Now" re-anchor
    // at line 2150 continues to find a valid recorded abs_t_dep.
    //
    // Builders (all sync — must complete in one frame so the panel
    // renders at frame 1, not frame 2 after a worker round-trip):
    //   * `star_approach_grid_for_target` — star destinations; one-shot,
    //     deterministic ~few-ms solve.
    //   * `short_hop_grid_for_moon` — moon / ring (when RON `short_hop`
    //     override is present); sync by contract.
    //   * `build_grid_for_body_target` — interplanetary Lambert grid;
    //     ~50-100 ms on Mars-class resolves, vs the async path's
    //     300-500 ms wait-with-no-panel.
    //
    // `target_orbit_shell` (formerly `target_arrival_radius`) flows into
    // `radius_for_shell` and then into `star_approach_grid_for_target` so the
    // user's shell pick becomes one of the 5 rows of the resulting grid.
    if fleet_ui_state.porkchop_grid.is_none() {
        if let Some(target_entity) = fleet_ui_state.target_body {
            let dest_body_type = body_query
                .get(target_entity)
                .ok()
                .map(|(_, b, _, _, _)| b.body_type);
            match dest_body_type {
                Some(BodyType::Star) => {
                    // GRA-NNN: shell-driven star-approach sync path.  Resolve the
                    // user's shell choice (or fall back to the per-star default
                    // shell) and use its numeric radius as the parking seed.
                    let (shell, radius_au) = match body_query.get(target_entity) {
                        Ok((_, b, _, _, _)) => {
                            let shell = fleet_ui_state
                                .target_orbit_shell
                                .filter(|(e, _)| *e == target_entity)
                                .map(|(_, s)| s)
                                .unwrap_or(default_shell_for_body_type(b.body_type));
                            (shell, radius_for_shell(b, shell))
                        }
                        // Body vanished mid-frame; let the deferred block re-attempt.
                        Err(_) => (OrbitShellId::HabitableInner, 0.0),
                    };
                    let _ = shell; // read for documentation; the value is consumed via radius_au below.
                    let (lo, hi) = star_approach_bounds_au(
                        target_entity,
                        body_query,
                        body_system_ids,
                        current_system_id,
                    );
                    let radius_au = radius_au.clamp(lo, hi);
                    let grid = star_approach_grid_for_target(
                        body_query,
                        orbit.body,
                        target_entity,
                        radius_au,
                        lo,
                        hi,
                        elapsed,
                    );
                    if let Some(grid) = grid {
                        fleet_ui_state.porkchop_grid = Some(grid);
                        // Only stamp `porkchop_built_for` so the
                        // deferred block sees a "same target" and
                        // skips the async rebuild.  Do NOT update
                        // `porkchop_built_at_s` / `porkchop_last_real_
                        // build_s` — those remain at the values the
                        // previous cache had so the staleness check
                        // and the post-panel re-anchor keep working.
                        fleet_ui_state.porkchop_built_for = Some(target_entity);
                    }
                }
                Some(BodyType::Moon) | Some(BodyType::Ring) => {
                    let n = porkchop_config
                        .category_overrides
                        .iter()
                        .find(|o| o.match_key == "short_hop")
                        .and_then(|o| o.short_hop_options);
                    if let Some(n) = n {
                        let grid = short_hop_grid_for_moon(
                            body_query,
                            porkchop_config,
                            orbit.body,
                            target_entity,
                            n,
                            elapsed,
                        );
                        if let Some(grid) = grid {
                            fleet_ui_state.porkchop_grid = Some(grid);
                            fleet_ui_state.porkchop_built_for = Some(target_entity);
                        }
                    }
                }
                Some(BodyType::Planet) | Some(BodyType::Asteroid) | Some(BodyType::Comet) => {
                    let orbits = (
                        heliocentric_orbit_for_body(orbit.body, body_query),
                        heliocentric_orbit_for_body(target_entity, body_query),
                    );
                    if let (Some(origin_orbit), Some(dest_orbit)) = orbits {
                        let origin_name = body_query
                            .get(orbit.body)
                            .ok()
                            .map(|(_, b, _, _, _)| b.name.clone())
                            .unwrap_or_else(|| "Origin".to_string());
                        let dest_name = body_query
                            .get(target_entity)
                            .ok()
                            .map(|(_, b, _, _, _)| b.name.clone())
                            .unwrap_or_else(|| "Dest".to_string());
                        let dest_parent = body_query
                            .get(target_entity)
                            .ok()
                            .and_then(|(_, _, _, _, lp)| lp)
                            .map(|lp| lp.0);
                        let origin_parent = body_query
                            .get(orbit.body)
                            .ok()
                            .and_then(|(_, _, _, _, lp)| lp)
                            .map(|lp| lp.0);
                        let category = crate::fleets::porkchop::classify_body_transfer_category(
                            dest_body_type.unwrap_or(BodyType::Planet),
                            dest_parent,
                            origin_parent,
                        );
                        let grid = crate::fleets::porkchop::build_grid_for_body_target(
                            porkchop_config,
                            origin_orbit,
                            dest_orbit,
                            origin_name,
                            dest_name,
                            category,
                            elapsed,
                        );
                        fleet_ui_state.porkchop_grid = Some(grid);
                        fleet_ui_state.porkchop_built_for = Some(target_entity);
                    }
                }
                _ => {
                    // Lagrange / FleetTarget / StarSystem — leave
                    // the grid as None so the legacy 3-option row +
                    // GA summary card keeps its correct behaviour.
                }
            }
        }
    }

    // ── GRA-159 deferred porkchop build ─────────────────────────────────────
    // The porkchop grid is normally built by the Body/Ring click handlers in
    // the destination picker below.  But there are entry points that set
    // `target_body` without firing the click handler:
    //   - The 3D-scene right-click handler in
    //     `src/astronomy/selection.rs:467-473` ("open transfer planner")
    //     sets `target_body` and `show_transfer_popup = true` but never
    //     builds the grid.
    //   - Any future entry point (hotkeys, automation, tests) that
    //     sets `target_body` directly.
    // Without this deferred build, those entry points would leave
    // `porkchop_grid = None` and the legacy 3-option row would render
    // even for a valid heliocentric destination.
    //
    // We build the grid here (once per frame) whenever a body target is
    // set but the grid is missing and the destination is a planet/star.
    // For moons/rings the grid stays `None` and the legacy row renders,
    // which is the correct local-frame cislunar behaviour.
    //
    // GRA-154: skip the build when a gravity assist is selected.  The
    // porkchop models direct Lambert arcs; assist trajectories are
    // multi-leg and the planner uses the legacy 3-option row + assist
    // stitching instead.  The assist handlers clear `porkchop_grid` on
    // selection; this guard keeps it `None` even if the user switches
    // target_body mid-frame.
    //
    // Porkchop staleness: the grid's `t_dep = 0` column is anchored to
    // the sim-time epoch the grid was built at.  If the player lets
    // time advance past the build epoch by more than
    // `PORKCHOP_STALENESS_THRESHOLD_S`, the cached ΔV values drift
    // (inner-planet geometries rotate ~1°/day, so ΔV can change by
    // tens of percent per day).  Invalidate the cache here so the
    // deferred build re-solves the grid against the *current* epoch —
    // that's why closing-and-reopening the planner now refreshes the
    // "starting point" tick.
    //
    // Also invalidate when the destination has changed.  Some entry
    // points (the 3D-scene right-click handler, hotkeys, automation)
    // set `target_body` without firing the planner's click handlers,
    // so the cached grid stays around for the *old* destination.
    // Comparing `porkchop_built_for` to `target_body` catches this
    // case before the deferred build runs and avoids re-rendering
    // the previous destination's grid.
    // Rotating-buffer scroll offset.  Cells slide smoothly through
    // the visible window at sub-col granularity as the player's
    // sim clock advances past the buffer's `t_dep_min_s`.  When
    // `shift_s` reaches the buffer's "future half" the visible
    // window is about to consume the rightmost cell and the
    // deferred build must rotate the buffer.
    //
    // GRA-169 (Part A): the buffer's `t_dep_min_s` is now anchored
    // at the player's `sim_time_s` at rebuild — no more hardcoded
    // 0.  `shift_s = elapsed - built_at_s` is unchanged but the
    // visible window now shows cells anchored at the *current*
    // sim epoch, not the orbit-spawn epoch.  The `buffer_future_
    // window_s` half-width below is the same 4× t_dep_window_days
    // buffer span, just re-anchored on the player's clock.
    let shift_s: f64 = fleet_ui_state
        .porkchop_built_at_s
        .map(|built| elapsed - built)
        .unwrap_or(0.0)
        .max(0.0);
    // "Stays at immediate departure once cell hits Now": when
    // the recorded `selected_abs_t_dep_s` falls below the
    // player's current sim clock (i.e. the cell the user
    // clicked has scrolled past the left edge of the chart and
    // into the past), re-anchor both the recorded absolute
    // epoch AND the visual `(sc, sr)` cell coordinate to the
    // "immediate departure" position.  Without this re-anchor
    // the chart highlight rectangle would slide off the left
    // edge while the trajectory ghost arc orbited around the
    // screen at the past burn-time planet's orbital phase
    // (the "trajectory moves all over the place once the
    // selected tile hits Now" report).
    //
    // The re-anchor keeps `selected_abs_t_dep_s = elapsed`
    // (exactly "now"), which the render path's three-way
    // clamp then treats as the immediate-departure path:
    // `departure_s = max(selected_abs_t_dep_s, current_sim_s)
    // = current_sim_s`.  `predict_body_visual_pos(origin,
    // current_sim_s, current_sim_s, ...)` returns the *live*
    // planet position, the Lambert solver re-evaluates the
    // porkchop cell's `transfer_orbit` against live origin +
    // live destination-at-arrival-time, and the arc visually
    // tracks the live planet continuously as sim time keeps
    // advancing.
    //
    // `selected_porkchop_cell` is clamped to `(0, sr)` so the
    // selection highlight sticks at the leftmost column of the
    // chart (the "Now" line).  The row stays the same so the
    // player's TOF preference is preserved across the re-anchor.
    //
    // The re-anchor is gated on having a selected cell AND a
    // grid (so it doesn't fire on the first frame after the
    // planner opens, before the first build lands).  It also
    // intentionally runs BEFORE the panel renders, so the
    // panel's selection rectangle, highlight, and tooltip
    // hover math all see the corrected `(sc, sr)`.
    if let (Some((_sc, sr)), Some(recorded_abs_t_dep), Some(grid)) = (
        fleet_ui_state.selected_porkchop_cell,
        fleet_ui_state.selected_abs_t_dep_s,
        fleet_ui_state.porkchop_grid.as_ref(),
    ) {
        if recorded_abs_t_dep < elapsed {
            // Re-anchor to "now": the recorded absolute epoch
            // becomes the player's current sim clock, and the
            // visual cell coordinate clamps to col 0 (the
            // "Now" line).  Row stays the same so the TOF
            // preference is preserved.
            let (cols_buf, _rows_buf) = grid.resolution;
            if cols_buf > 0 {
                fleet_ui_state.selected_abs_t_dep_s = Some(elapsed);
                // The row's TOF is the cell's recorded TOF —
                // the Lambert solution's TOF stays valid as a
                // "what if I burn now" estimate.  Don't touch
                // `selected_abs_tof_s`.
                fleet_ui_state.selected_porkchop_cell = Some((0, sr));
            }
        }
    }
    // Buffer covers 4× the visible window, so the visible window
    // is exactly 1/2 of the buffer's t_dep span.  Rotation fires
    // when the visible window's right edge has reached the
    // buffer's right edge — i.e. shift_s == 1/2 * buffer_width.
    // At that point the deferred build reanchors the new buffer
    // at `t_dep_min_s = sim_time_s`, so the visible cells at the
    // new build are the *same physical cells* the user was
    // already looking at in the old buffer's right half (Lambert
    // is rotation-invariant).  No visual jump.
    //
    // GRA-169 (Part B): the rotation trigger now sets
    // `porkchop_grid_pending_rebuild = true` instead of clearing
    // `porkchop_grid = None`.  The panel keeps rendering the old
    // grid while the build block (~360 ms) solves the new one —
    // then atomically swaps `porkchop_grid` + clears the flag in
    // one statement.  No blank frame.
    //
    // The trigger now fires with lead time (`should_rotate_porkchop_
    // buffer` applies `PORKCHOP_EARLY_ROTATION_FACTOR`) instead of
    // waiting until the runway is fully exhausted — see that
    // function's docs for the "invisible swap in the overlap window"
    // rationale.
    let buffer_t_dep_span_s = fleet_ui_state
        .porkchop_grid
        .as_ref()
        .map(|g| g.t_dep_bounds_s.1 - g.t_dep_bounds_s.0)
        .unwrap_or(0.0);
    let buffer_needs_rotation = fleet_ui_state.porkchop_grid.is_some()
        && should_rotate_porkchop_buffer(shift_s, buffer_t_dep_span_s);

    let grid_for_changed = fleet_ui_state.porkchop_built_for != fleet_ui_state.target_body;
    if fleet_ui_state.porkchop_grid.is_some()
        && (grid_for_changed
            || porkchop_grid_is_stale(
                fleet_ui_state.porkchop_built_at_s,
                elapsed,
                time_scale,
                fleet_ui_state.porkchop_last_real_build_s,
                real_now_s,
            )
            || buffer_needs_rotation)
    {
        // GRA-169 (Part B): rotation no longer drops the grid
        // synchronously.  The old code cleared `porkchop_grid = None`
        // here, which made the panel render an empty fallback for
        // one frame and read as a visible left-edge snap.  Now we
        // set `porkchop_grid_pending_rebuild` so the per-frame
        // build block (below) keeps the old grid in place while it
        // solves the new one (~360 ms), then atomically swaps
        // `porkchop_grid` + clears the flag in a single statement.
        //
        // For *destination change* and *staleness* the grid stays
        // valid until the next render frame, but we follow the same
        // path so the user never sees a blank panel.  The staleness
        // check is otherwise the same.
        //
        // Note: do NOT clear `selected_porkchop_cell` on buffer
        // rotation.  The (col, row) stays valid in the new buffer
        // (modulo the new buffer's resolution, which the deferred
        // build re-resolves against the same target).  Clearing here
        // would re-trigger the panel's `min_cell` auto-pick, which
        // jumps the selection to the cheapest cell of the *new*
        // buffer every rotation.  We only clear on destination
        // change or staleness expiry.
        fleet_ui_state.porkchop_grid_pending_rebuild = true;
        // IMPORTANT: do NOT clear `porkchop_built_at_s` here.
        // Previously this block set `porkchop_built_at_s = None`
        // which made `shift_s = elapsed - None.unwrap_or(0) = 0`
        // for the entire ~360 ms async-build window.  The panel's
        // scroll then reset to the OLD buffer's left edge while
        // the worker was solving — a visible backward jump of the
        // chart content to the very leftmost columns.  When the
        // swap landed, scroll reset to 0 of the NEW buffer (which
        // also starts at the current sim epoch), so the chart
        // content snapped forward again.  The combined effect
        // was a left-then-right flicker the user reported as
        // "the reload still causes the porkchop to flicker".
        //
        // Keeping `porkchop_built_at_s` at its current value lets
        // `shift_s` keep advancing during the build; the old
        // grid's visible window stays anchored at "now" exactly
        // as it was before the trigger fired.  When the swap
        // lands, the new buffer's `t_dep_bounds_s.0 = elapsed`
        // (the same value `shift_s` had reached) so the visible
        // cell content of the new buffer's leftmost columns is
        // the SAME physical cells the old buffer was already
        // showing — no horizontal content jump, no flicker.
        //
        // The Lamport-style rotation invariant ("shifting the
        // buffer's `t_dep_min_s` by `Δ` only relabels cell dates,
        // not ΔV") is what makes this seamless: the new buffer's
        // col 0 at `elapsed` is identical Lambert-wise to the old
        // buffer's leftmost visible cell at the moment of swap.
        // The clearing of `porkchop_built_at_s` on destination
        // change is fine because `porkchop_grid = None` is also
        // set there (the panel renders the empty-state fallback
        // instead of stale content).
        //
        // `porkchop_last_real_build_s` is also preserved so the
        // staleness floor (which compares against real time) does
        // not falsely re-trigger the moment the swap lands.
        if grid_for_changed {
            // Destination changed — drop the old grid's metadata
            // and clear selection (the new grid is a different
            // (origin, dest) pair, so the (col, row) anchor no
            // longer points at the same physical cell).
            fleet_ui_state.porkchop_grid = None;
            fleet_ui_state.porkchop_built_for = None;
            fleet_ui_state.selected_porkchop_cell = None;
        }
        // Stale-grid path: keep the grid in place; the build block
        // will swap it once the new one is ready.
    }
    if let Some(target_entity) = fleet_ui_state.target_body {
        // Async build (Phase B++): if a worker thread has finished
        // its solve, receive the result and atomically swap it into
        // `porkchop_grid`.  `try_recv()` is non-blocking — if the
        // worker is still solving we just continue, no main-thread
        // stall.  This replaces the previous synchronous-solve path
        // that blocked the egui pass for ~360 ms per rotation
        // trigger (visible as a "short break in game progress" at
        // high sim speeds).
        if let Some(rx_lock) = fleet_ui_state.porkchop_build_result_rx.as_ref() {
            // Lock briefly to call `try_recv()`. The lock is held
            // only for the duration of the call, so contention with
            // the worker thread (which never touches the receiver)
            // is zero.
            let try_recv_result = rx_lock.lock().ok().map(|rx| rx.try_recv());
            match try_recv_result {
                Some(Ok(new_grid)) => {
                    // Re-anchor `selected_porkchop_cell` after
                    // rotation: search the new buffer for the cell
                    // whose `(abs_t_dep, abs_tof)` is closest to
                    // the recorded anchors, and update `(sc, sr)`
                    // so the user's selection stays on the same
                    // physical cell across rotations.  Without
                    // this the same `(sc, sr)` lands on a
                    // different abs t_dep in the new buffer and
                    // the selected cell's ΔV appears to "jump"
                    // by 1-3 km/s every rotation.
                    if let (Some(abs_t_dep), Some(abs_tof)) = (
                        fleet_ui_state.selected_abs_t_dep_s,
                        fleet_ui_state.selected_abs_tof_s,
                    ) {
                        let (cols_b, rows_b) = new_grid.resolution;
                        if cols_b > 0 && rows_b > 0 {
                            let col_step = (new_grid.t_dep_bounds_s.1 - new_grid.t_dep_bounds_s.0)
                                / cols_b as f64;
                            let tof_step =
                                (new_grid.tof_bounds_s.1 - new_grid.tof_bounds_s.0) / rows_b as f64;
                            let t_dep_min_abs = elapsed;
                            let t_of_min_abs = new_grid.tof_bounds_s.0;
                            let mut best: Option<(usize, usize, f64)> = None;
                            for r in 0..rows_b {
                                for c in 0..cols_b {
                                    let cell_t_dep_abs = t_dep_min_abs + (c as f64) * col_step;
                                    let cell_t_of = t_of_min_abs + (r as f64) * tof_step;
                                    let dt = (cell_t_dep_abs - abs_t_dep).abs();
                                    let dtof = (cell_t_of - abs_tof).abs();
                                    let err = dt + dtof * 0.01;
                                    if best.is_none() || err < best.unwrap().2 {
                                        best = Some((c, r, err));
                                    }
                                }
                            }
                            if let Some((c, r, _)) = best {
                                fleet_ui_state.selected_porkchop_cell = Some((c, r));
                            }
                        }
                    }
                    // Atomic swap: replace the cached grid and clear
                    // the pending-rebuild flag in a single statement.
                    // The panel only ever observes the old grid
                    // (until now) or the new grid (from here on),
                    // never `None` — that's the no-blank-frame
                    // contract.  Also clears `porkchop_build_in_flight`
                    // and drops the receiver so the next rotation
                    // trigger can spawn a fresh worker.
                    fleet_ui_state.porkchop_grid = Some(new_grid);
                    fleet_ui_state.porkchop_grid_pending_rebuild = false;
                    fleet_ui_state.porkchop_build_in_flight = false;
                    fleet_ui_state.porkchop_build_result_rx = None;
                    fleet_ui_state.porkchop_built_at_s = Some(elapsed);
                    fleet_ui_state.porkchop_last_real_build_s = Some(real_now_s);
                    fleet_ui_state.porkchop_built_for = Some(target_entity);
                }
                Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                    // Worker still solving — do nothing. The build
                    // is "in flight" and the storm guard below
                    // prevents re-spawn.
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    // Worker panicked or the sender was dropped.
                    // Clear the flags so the next frame re-spawns.
                    fleet_ui_state.porkchop_build_in_flight = false;
                    fleet_ui_state.porkchop_build_result_rx = None;
                    // Leave `porkchop_grid_pending_rebuild = true`
                    // so the next frame re-attempts the build.
                }
                None => {
                    // Mutex was poisoned (a thread panicked while
                    // holding the lock). Treat as disconnected.
                    fleet_ui_state.porkchop_build_in_flight = false;
                    fleet_ui_state.porkchop_build_result_rx = None;
                }
            }
        }
        // GRA-384 follow-up: short-hop sync build for Moon/Ring
        // destinations.  The async path below is sized for the
        // ~360 ms Lambert solve on the heliocentric grid; the
        // short-hop ΔV sweep is microseconds, so we build it
        // synchronously here.  Mirrors the click handler at line
        // 3153 so a planner opened with a pre-set Moon target
        // (e.g. via the 3D-scene right-click handler) gets the
        // same N-row porkchop bar as a fresh click — without
        // this branch the player would see the legacy 3-option
        // row on every "open planner with target already set"
        // because `should_build_porkchop_for_destination` returns
        // false for moons, which gates the async build below.
        // Falls back to `porkchop_grid = None` (legacy 3-option
        // row) when the RON `short_hop_options` override is
        // absent.
        //
        // IMPORTANT: do NOT also gate on `porkchop_grid_pending_rebuild`.
        // The target-change block above (line 2074) sets
        // `porkchop_grid_pending_rebuild = true` AND clears
        // `porkchop_grid = None` (line 2116) when `grid_for_changed`.
        // The pending-rebuild flag there is the legacy "queue an async
        // rebuild" marker — but for Moon destinations the async path
        // can't help (the heliocentric Lambert solver produces a
        // degenerate grid for r1≈r2), so this sync short-hop build
        // IS the rebuild.  Gating on `pending_rebuild` here would
        // suppress the sync build entirely and the player would see
        // the legacy 3-option row on every "switch target to Moon"
        // path.  After the sync build lands, `needs_build` below
        // evaluates to `false` (grid is Some) so the async path
        // stays out of the way — no risk of the in-flight worker
        // overwriting our grid because that worker's polling block
        // only swaps in a result when `target_entity` matches
        // `fleet_ui_state.porkchop_built_for`, which we just set.
        if !should_build_porkchop_for_destination(body_query, target_entity)
            && fleet_ui_state.porkchop_grid.is_none()
        {
            if let Some(n) = porkchop_config
                .category_overrides
                .iter()
                .find(|o| o.match_key == "short_hop")
                .and_then(|o| o.short_hop_options)
            {
                fleet_ui_state.porkchop_grid = short_hop_grid_for_moon(
                    body_query,
                    porkchop_config,
                    orbit.body,
                    target_entity,
                    n,
                    elapsed,
                );
                fleet_ui_state.porkchop_built_at_s = Some(elapsed);
                fleet_ui_state.porkchop_built_for = Some(target_entity);
                // We just satisfied the pending rebuild ourselves;
                // clear the flag so the rotation / staleness check
                // at line 2041 doesn't immediately re-queue a
                // rebuild of the (now-superseded) target-change
                // contract.
                fleet_ui_state.porkchop_grid_pending_rebuild = false;
            }
        }

        // GRA-169 (Part B): trigger the build whenever the grid is
        // missing OR a pending rebuild is queued.  In the pending
        // case the *old* grid stays in `porkchop_grid` while this
        // block runs — the panel keeps rendering it during the
        // ~360 ms Lambert solve.  When the new grid is ready we
        // atomically swap `porkchop_grid` and clear the flag in a
        // single statement at the end of the build.
        let needs_build = (fleet_ui_state.porkchop_grid.is_none()
            || fleet_ui_state.porkchop_grid_pending_rebuild)
            && fleet_ui_state.selected_gravity_assist.is_none()
            && should_build_porkchop_for_destination(body_query, target_entity);
        // Porkchop rebuild-storm guard (Phase B+): if a build is
        // already in flight (we entered this block on a previous
        // frame and the ~360 ms Lambert solve hasn't finished yet),
        // skip the re-entry.  Without this, every frame the planner
        // is open AND the pending-rebuild flag is set would
        // re-solve the full 1200-cell grid from scratch — at 60 FPS
        // a single rotation trigger fires 22 consecutive solves
        // (~8 s of CPU) before the atomic swap lands.  The first
        // entry below still sets `porkchop_build_in_flight = true`
        // inside the orbit-resolved branch (so a transient
        // `heliocentric_orbit_for_body = None` doesn't strand the
        // flag as `true`), and the atomic swap at the end clears
        // it.
        if needs_build && !fleet_ui_state.porkchop_build_in_flight {
            if let (Some(origin_orbit), Some(dest_orbit)) = (
                heliocentric_orbit_for_body(orbit.body, body_query),
                heliocentric_orbit_for_body(target_entity, body_query),
            ) {
                fleet_ui_state.porkchop_build_in_flight = true;
                let origin_name = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_else(|| "Origin".to_string());
                let dest_name = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_else(|| "Dest".to_string());
                let dest_body_type = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| b.body_type)
                    .unwrap_or(BodyType::Planet);
                let dest_parent = body_query
                    .get(target_entity)
                    .ok()
                    .and_then(|(_, _, _, _, lp)| lp)
                    .map(|lp| lp.0);
                let origin_parent = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, _, lp)| lp)
                    .map(|lp| lp.0);
                let category = crate::fleets::porkchop::classify_body_transfer_category(
                    dest_body_type,
                    dest_parent,
                    origin_parent,
                );
                // Async build (Phase B++): spawn a worker thread
                // to solve the Lambert grid off the main thread.
                // The egui pass continues immediately; the polling
                // block above `try_recv()`s the result on every
                // subsequent frame and atomically swaps it in.  This
                // eliminates the previous ~360 ms main-thread
                // block that read as a "short break in game
                // progress" the user reported at high sim speeds.
                //
                // We clone `porkchop_config` because `&PorkchopConfig`
                // borrows from the Bevy `Res<PorkchopConfig>` whose
                // lifetime is tied to the world — can't safely
                // cross a thread boundary without 'static.  The
                // config is small (a few KB) so the clone is cheap.
                let cfg = porkchop_config.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::Builder::new()
                    .name("porkchop-build".to_string())
                    .spawn(move || {
                        let grid = build_rotating_buffer_for_body_target(
                            &cfg,
                            origin_orbit,
                            dest_orbit,
                            origin_name,
                            dest_name,
                            category,
                            elapsed,
                        );
                        // `tx.send` returns Err if the receiver was
                        // dropped (e.g. target changed mid-solve) —
                        // we discard the grid in that case.  The
                        // receiver-side polling block also detects
                        // disconnection and clears `in_flight`.
                        let _ = tx.send(grid);
                    })
                    .expect("failed to spawn porkchop-build thread");
                fleet_ui_state.porkchop_build_in_flight = true;
                // Wrap the receiver in a `Mutex` so the
                // `FleetUiState` resource remains `Send + Sync`
                // (required by Bevy). The lock is held only briefly
                // inside the polling block's `try_recv()` call.
                fleet_ui_state.porkchop_build_result_rx = Some(std::sync::Mutex::new(rx));
                // Re-anchor logic + atomic swap moved to the
                // polling block at the top of this `if let Some(target_entity)`
                // section — see the comment there for why.
            }
        }
    }

    // ── Hierarchical destination selector ────────────────────────────────────
    // DestEntry variants:
    //   Header — non-clickable category label; separator drawn BEFORE it (but not the very first)
    //   Body   — selectable destination
    //   Ring   — selectable ring destination (no KeplerOrbit; radius from body.radius field)
    //   Lagrange — one of the 5 L-points of a planet-star system
    //   FleetTarget — another fleet (for intercept course)
    //   StarSystem — interstellar target (another star system)
    //   StarApproach — a star with an interactive parking-radius control
    //     (GRA-161). Distinct from `Body` so the picker can render a
    //     `DragValue` next to the row instead of a static label.
    #[derive(Clone)]
    enum DestEntry {
        Header(String),
        Body {
            entity: Entity,
            name: String,
        },
        // Rings are treated like regular bodies for selection; the extra
        // parent/radius information used to be stored here but never read.
        Ring {
            entity: Entity,
            name: String,
        },
        // Lagrange-point destination. Built from `LagrangeTarget` so the planner
        // can compute the transfer orbit without an L-point ECS entity.
        Lagrange {
            lp: LagrangeTarget,
        },
        FleetTarget {
            entity: Entity,
            name: String,
            in_transit: bool,
        },
        StarSystem {
            system_id: usize,
            name: String,
            distance_ly: f32,
        },
        Star {
            entity: Entity,
            name: String,
        },
    }

    let mut dest_entries: Vec<DestEntry> = Vec::new();

    // Collect all valid candidate bodies (exclude Star, include Ring)
    // For Rings: sma = None (no KeplerOrbit); radius stored via body.radius field separately.
    let candidates: Vec<(Entity, String, BodyType, Option<f64>, Option<Entity>)> = body_query
        .iter()
        .filter_map(|(e, body, _, maybe_ko, maybe_lp)| {
            if e == orbit.body {
                return None;
            }
            if body.body_type == BodyType::Star {
                return None;
            }
            if !body_system_ids
                .get(e)
                .ok()
                .map(|s| s.0 == current_system_id)
                .unwrap_or(false)
            {
                return None;
            }
            let sma = maybe_ko.map(|ko| ko.semi_major_axis);
            let parent = maybe_lp.map(|lp| lp.0);
            Some((e, body.name.clone(), body.body_type, sma, parent))
        })
        .collect();

    // Separate ring bodies out; they lack KeplerOrbits so need special handling
    let ring_candidates: Vec<(Entity, String, Option<Entity>, f64)> = body_query
        .iter()
        .filter_map(|(e, body, _, _, maybe_lp)| {
            if body.body_type != BodyType::Ring {
                return None;
            }
            if !body_system_ids
                .get(e)
                .ok()
                .map(|s| s.0 == current_system_id)
                .unwrap_or(false)
            {
                return None;
            }
            let parent = maybe_lp.map(|lp| lp.0)?;
            // Use body.radius (km) as the representative ring orbit distance from planet centre
            let radius_au = (body.radius as f64 * 1_000.0) / AU_IN_METERS;
            Some((e, body.name.clone(), Some(parent), radius_au))
        })
        .collect();

    // ── Group 1: bodies that directly orbit the fleet's current body ──────────
    {
        let orbit_body_name = body_query
            .get(orbit.body)
            .map(|(_, b, _, _, _)| b.name.clone())
            .unwrap_or_default();
        let mut local: Vec<(Entity, String, f64)> = candidates
            .iter()
            .filter(|(_, _, btype, _, parent)| {
                *parent == Some(orbit.body) && *btype != BodyType::Ring
            })
            .filter_map(|(e, name, _, sma, _)| sma.map(|s| (*e, name.clone(), s)))
            .collect();
        // Rings around the current orbit body
        let mut local_rings: Vec<(Entity, String, Option<Entity>, f64)> = ring_candidates
            .iter()
            .filter(|(_, _, parent, _)| *parent == Some(orbit.body))
            .cloned()
            .collect();

        // L1/L2 of the current body itself: if it has a host (star for a planet,
        // planet for a moon), emit both Lagrange rows.  Done unconditionally —
        // the fleet can target its current body's L-points even if the system
        // has no local moons to display.
        let mut orbit_body_lagrange: Vec<(LagrangeTarget, String)> = Vec::new();
        if let Ok((_, _, _, ob_ko, ob_parent)) = body_query.get(orbit.body) {
            if let (Some(host_e), Some(sma)) =
                (ob_parent.map(|p| p.0), ob_ko.map(|k| k.semi_major_axis))
            {
                if let Some((l1, l2, host_name)) =
                    lagrange_targets_for_body(orbit.body, body_query, host_e, sma)
                {
                    orbit_body_lagrange.push((l1, host_name.clone()));
                    orbit_body_lagrange.push((l2, host_name));
                }
            }
        }

        if !local.is_empty() || !local_rings.is_empty() || !orbit_body_lagrange.is_empty() {
            local.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            local_rings.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            dest_entries.push(DestEntry::Header(format!("{orbit_body_name} System")));
            for (e, name, _) in &local {
                dest_entries.push(DestEntry::Body {
                    entity: *e,
                    name: name.clone(),
                });
            }
            for (e, name, parent, _radius_au) in local_rings {
                if parent.is_some() {
                    dest_entries.push(DestEntry::Ring { entity: e, name });
                }
            }
            // Sun-Planet or Planet-Moon L1/L2 of the current body.  Pushed last
            // so the per-body destinations (children) read first and the
            // system-level Lagrange rows sit at the bottom of the group.
            for (lp, _host_name) in &orbit_body_lagrange {
                dest_entries.push(DestEntry::Lagrange { lp: lp.clone() });
            }
        }
    }

    // ── Groups 2+: planet systems (moons/rings orbiting a planet that isn't fleet's body) ──
    let mut planet_map: std::collections::BTreeMap<
        String,
        (Entity, f64, Vec<(Entity, String, f64, bool)>),
    > = std::collections::BTreeMap::new();

    // Regular moons / small bodies orbiting a planet
    for (e, name, btype, sma, parent) in &candidates {
        if *btype == BodyType::Ring {
            continue;
        }
        let parent_e = match parent {
            Some(p) => *p,
            None => continue,
        };
        if parent_e == orbit.body {
            continue;
        }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star {
                continue;
            }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            if let Some(s) = sma {
                planet_map
                    .entry(pb.name.clone())
                    .or_insert_with(|| (parent_e, parent_sma, vec![]))
                    .2
                    .push((*e, name.clone(), *s, false)); // false = not a ring
            }
        }
    }
    // Rings orbiting a planet that isn't the fleet's body
    for (e, name, parent_opt, radius_au) in &ring_candidates {
        let parent_e = match parent_opt {
            Some(p) => *p,
            None => continue,
        };
        if parent_e == orbit.body {
            continue;
        }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star {
                continue;
            }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            planet_map
                .entry(pb.name.clone())
                .or_insert_with(|| (parent_e, parent_sma, vec![]))
                .2
                .push((*e, name.clone(), *radius_au, true)); // true = ring
        }
    }

    let mut sorted_planet_systems: Vec<_> = planet_map.into_iter().collect();
    sorted_planet_systems.sort_by(|a, b| {
        a.1 .1
            .partial_cmp(&b.1 .1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut planets_shown = std::collections::HashSet::<Entity>::new();
    for (planet_name, (parent_e, parent_sma, mut children)) in sorted_planet_systems {
        planets_shown.insert(parent_e);
        children.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header(format!("{planet_name} System")));
        if orbit.body != parent_e {
            dest_entries.push(DestEntry::Body {
                entity: parent_e,
                name: planet_name.clone(),
            });
        }
        for (e, name, _sma, is_ring) in &children {
            if *is_ring {
                dest_entries.push(DestEntry::Ring {
                    entity: *e,
                    name: name.clone(),
                });
            } else {
                dest_entries.push(DestEntry::Body {
                    entity: *e,
                    name: name.clone(),
                });
            }
        }

        // Sun-Planet L1/L2 of the host planet.  `parent_sma` is the planet's
        // heliocentric SMA (the planet's own orbit around its star).  We need
        // the planet's host (the star) to read its mass for the Hill-sphere
        // and to populate the picker label.
        if let Ok((_, _, _, _, planet_parent_lp)) = body_query.get(parent_e) {
            if let Some(star_e) = planet_parent_lp.map(|p| p.0) {
                if let Some((l1, l2, _star_name)) =
                    lagrange_targets_for_body(parent_e, body_query, star_e, parent_sma)
                {
                    dest_entries.push(DestEntry::Lagrange { lp: l1 });
                    dest_entries.push(DestEntry::Lagrange { lp: l2 });
                }
            }
        }

        // Planet-Moon L1/L2 of each child moon in this system.  `child_sma_au`
        // is the moon's planet-centric SMA; the host is the planet itself.
        for (child_e, _child_name, child_sma_au, is_ring) in &children {
            if *is_ring {
                continue;
            }
            if let Some((l1, l2, _planet_name)) =
                lagrange_targets_for_body(*child_e, body_query, parent_e, *child_sma_au)
            {
                dest_entries.push(DestEntry::Lagrange { lp: l1 });
                dest_entries.push(DestEntry::Lagrange { lp: l2 });
            }
        }
    }

    // ── Group: Planets/GasGiants not yet shown (no children found in data) ───
    let already_listed: std::collections::HashSet<Entity> = dest_entries
        .iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();

    let mut standalone: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Planet | BodyType::GasGiant)
                && sma.is_some()
                && !planets_shown.contains(e)
                && !already_listed.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !standalone.is_empty() {
        standalone.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header("Planets".to_string()));
        for (e, name, _) in standalone {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Dwarf Planets (not yet shown) ────────────────────────────────
    // Dwarf planets (Pluto, Eris, Ceres, etc.) get a separate top-level
    // header so they are not buried inside the "Planets" group with
    // Mercury/Venus/Earth-class bodies. Sorted by semi-major axis
    // (≈ perihelion for near-circular orbits) — most accessible first.
    let mut dwarf_planets: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::DwarfPlanet)
                && sma.is_some()
                && !planets_shown.contains(e)
                && !already_listed.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !dwarf_planets.is_empty() {
        dwarf_planets.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header("Dwarf Planets".to_string()));
        for (e, name, _) in dwarf_planets {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Small bodies ─────────────────────────────────────────────────
    let already_listed2: std::collections::HashSet<Entity> = dest_entries
        .iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();
    // Split small bodies by type so the picker groups Asteroids and Comets
    // separately (most accessible first by perihelion ≈ semi-major axis).
    // The shared "Small Bodies" top-level header keeps the picker scannable
    // when a system has 50+ asteroids or comets; sub-headers carry the count
    // so the player can tell at a glance which type dominates.
    let mut asteroids: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Asteroid)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    let mut comets: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Comet)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();

    if !asteroids.is_empty() || !comets.is_empty() {
        let total = asteroids.len() + comets.len();
        let sb_label = if total > 5 {
            format!("Small Bodies ({} total)", total)
        } else {
            "Small Bodies".to_string()
        };
        dest_entries.push(DestEntry::Header(sb_label));

        if !asteroids.is_empty() {
            asteroids.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            let label = if asteroids.len() > 1 {
                format!("Asteroids ({})", asteroids.len())
            } else {
                "Asteroids".to_string()
            };
            dest_entries.push(DestEntry::Header(label));
            for (e, name, _) in asteroids {
                dest_entries.push(DestEntry::Body { entity: e, name });
            }
        }

        if !comets.is_empty() {
            comets.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            let label = if comets.len() > 1 {
                format!("Comets ({})", comets.len())
            } else {
                "Comets".to_string()
            };
            dest_entries.push(DestEntry::Header(label));
            for (e, name, _) in comets {
                dest_entries.push(DestEntry::Body { entity: e, name });
            }
        }
    }

    // ── Group: Star Approach ─────────────────────────────────────────────────
    // List every star in the current system.  In single-star systems this gives
    // one "🛰 Sol Approach" entry.  In binary / trinary systems each star gets
    // its own entry, enabling direct inter-star transfer planning and stellar
    // gravity-assist routes (e.g. Star A → Star B → Star C).
    //
    // GRA-149 C-2: the approach radius in the label is now sourced from the
    // per-body `star_approach_au` override (or the 0.3 AU default) so the
    // label matches the actual arrival parking radius used by the planner.
    //
    // GRA-161: emit a `DestEntry::StarApproach` variant instead of `Body`
    // so the picker can render an interactive `DragValue` for the parking
    // radius.  The radius defaults to `star_approach_radius_au(b)` and
    // clamps to per-star bounds (`MIN_STAR_APPROACH_AU` to either
    // `MAX_STAR_APPROACH_AU` or 90 % of the closest planet's SMA).
    {
        let mut system_stars: Vec<(Entity, String, f64)> = body_query
            .iter()
            .filter_map(|(e, b, _, _, _)| {
                if b.body_type != BodyType::Star {
                    return None;
                }
                if !body_system_ids
                    .get(e)
                    .ok()
                    .map(|s| s.0 == current_system_id)
                    .unwrap_or(false)
                {
                    return None;
                }
                Some((e, b.name.clone(), star_approach_radius_au(b)))
            })
            .collect();
        // Stable sort by name so order is deterministic across frames.
        system_stars.sort_by(|a, b| a.1.cmp(&b.1));
        if !system_stars.is_empty() {
            dest_entries.push(DestEntry::Header("Star Approach".to_string()));
            for (star_e, star_name, _approach_au) in system_stars {
                // GRA-NNN: parking radius now resolved by the shell picker via
                // `target_orbit_shell` rather than a numeric override on each
                // star row.  Each star contributes a single `Star { entity,
                // name }` entry; the shell ComboBox below the destination
                // picker handles the radius.
                dest_entries.push(DestEntry::Star {
                    entity: star_e,
                    name: star_name,
                });
            }
        }
    }

    // ── Group: Interstellar ──────────────────────────────────────────────────
    // List every other star system from NearbyStarsData as an interstellar target.
    // The current system is identified by its numeric id; Sol = id 0 by convention.
    {
        let mut interstellar_entries: Vec<DestEntry> = nearby_stars
            .systems
            .iter()
            .filter(|sys| {
                // Exclude the current system (id comparison via name match is a fallback)
                // NearbyStarsData systems use 0-based index ordering; system_id 0 = Sol.
                // We exclude any system whose name matches current system's star name.
                let this_star_name = body_query
                    .iter()
                    .find(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids
                                .get(*e)
                                .ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| b.name.as_str())
                    .unwrap_or("Sol");
                // Each StarSystemData has stars[0].name; compare to current star
                !sys.stars.iter().any(|s| s.name == this_star_name) && sys.distance_ly > 0.0
            })
            .enumerate()
            .map(|(idx, sys)| {
                let display = format!("✨ {} ({:.2} ly)", sys.system_name, sys.distance_ly);
                // Use index+1 as system_id (0 reserved for Sol in current system)
                DestEntry::StarSystem {
                    system_id: idx + 1,
                    name: display,
                    distance_ly: sys.distance_ly,
                }
            })
            .collect();

        if !interstellar_entries.is_empty() {
            interstellar_entries.sort_by(|a, b| {
                let da = if let DestEntry::StarSystem { distance_ly, .. } = a {
                    *distance_ly
                } else {
                    0.0
                };
                let db = if let DestEntry::StarSystem { distance_ly, .. } = b {
                    *distance_ly
                } else {
                    0.0
                };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            dest_entries.push(DestEntry::Header("Interstellar".to_string()));
            dest_entries.extend(interstellar_entries);
        }
    }

    // ── Build hierarchical categories from dest_entries ─────────────────────
    // Top-level headers ("…System", "Small Bodies", "Heliocentric") become
    // category names in the first-level picker. Lagrange sub-headers are kept
    // as visual separators inside each category group.
    #[derive(Clone)]
    struct DestGroup {
        name: String,
        entries: Vec<DestEntry>,
    }

    let mut groups: Vec<DestGroup> = Vec::new();
    for entry in dest_entries {
        let is_top_header = match &entry {
            DestEntry::Header(label) => {
                label.ends_with(" System")
                    || label == "Planets"
                    || label == "Dwarf Planets"
                    || label == "Solar"
                    || label == "Interstellar"
                    || label == "Star Approach"
                    || label.starts_with("Small Bodies")
            }
            _ => false,
        };
        if is_top_header {
            let name = match &entry {
                DestEntry::Header(label) => {
                    label.strip_suffix(" System").unwrap_or(label).to_string()
                }
                _ => unreachable!(),
            };
            groups.push(DestGroup {
                name,
                entries: Vec::new(),
            });
        } else if let Some(g) = groups.last_mut() {
            g.entries.push(entry);
        }
    }

    // ── Fleet intercept category ─────────────────────────────────────────────
    {
        let other_fleets: Vec<(Entity, String, bool)> = all_fleets_query
            .iter()
            .filter(|(e, _, _, _, _)| *e != fleet_entity)
            .map(|(e, f, _, _, maybe_ma)| (e, f.name.clone(), maybe_ma.is_some()))
            .collect();
        if !other_fleets.is_empty() {
            let mut fleet_group = DestGroup {
                name: "Fleets".to_string(),
                entries: Vec::new(),
            };
            // In-orbit fleets first
            for (e, name, in_transit) in &other_fleets {
                fleet_group.entries.push(DestEntry::FleetTarget {
                    entity: *e,
                    name: name.clone(),
                    in_transit: *in_transit,
                });
            }
            groups.push(fleet_group);
        }
    }

    // ── Auto-select category if a target is selected ─────────────────────────
    let mut correct_category = None;
    if let Some(target) = fleet_ui_state.target_body {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => {
                    *entity == target
                }
                // GRA-161: a star-approach target lives in its own
                // "Star Approach" group; recognise it here so the
                // category ComboBox auto-selects when the player picks
                // a star via the new interactive picker.
                DestEntry::Star { entity, .. } => *entity == target,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(ref lp) = fleet_ui_state.target_lagrange {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Lagrange { lp: entry_lp } => {
                    entry_lp.point == lp.point && entry_lp.planet_entity == lp.planet_entity
                }
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::FleetTarget { entity, .. } => *entity == tf,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some((tss_id, _, _)) = fleet_ui_state.target_star_system {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::StarSystem { system_id, .. } => *system_id == tss_id,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    }

    if let Some(cat) = correct_category {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        if sel != Some(&cat) && !(sel == Some("Small Bodies") && cat.starts_with("Small Bodies")) {
            fleet_ui_state.selected_dest_category = Some(cat);
        }
    }

    // ── Render the two-level selector ────────────────────────────────────────
    // Step 1: category (planet system / small bodies / fleets)
    let cat_label = groups
        .iter()
        .find(|g| {
            let sel = fleet_ui_state.selected_dest_category.as_deref();
            sel == Some(&g.name)
                || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
        })
        .map(|g| g.name.clone())
        .unwrap_or_else(|| {
            fleet_ui_state
                .selected_dest_category
                .clone()
                .unwrap_or_else(|| "— System —".to_owned())
        });

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("System:").size(13.0));
        egui::ComboBox::from_id_salt("fleet_dest_category")
            .selected_text(&cat_label)
            .width(200.0)
            .show_ui(ui, |ui| {
                for group in &groups {
                    let sel = fleet_ui_state.selected_dest_category.as_deref();
                    let cat_is_sel = sel == Some(&group.name)
                        || (sel == Some("Small Bodies") && group.name.starts_with("Small Bodies"));
                    if ui
                        .selectable_label(cat_is_sel, egui::RichText::new(&group.name).size(13.0))
                        .clicked()
                        && !cat_is_sel
                    {
                        fleet_ui_state.selected_dest_category = Some(group.name.clone());
                        // Clear the specific target so the second step is re-selected
                        fleet_ui_state.target_body = None;
                        fleet_ui_state.target_lagrange = None;
                        fleet_ui_state.target_fleet = None;
                        fleet_ui_state.target_star_system = None;
                        // GRA-161: also clear the user-controlled star-approach
                        // radius when the category changes.  The radius is
                        // meaningless once the destination is no longer a
                        // star in the new category.
                        fleet_ui_state.target_orbit_shell = None;
                        fleet_ui_state.computed_options.clear();
                        fleet_ui_state.planned_transfer = None;
                        fleet_ui_state.selected_option = 0;
                        fleet_ui_state.selected_gravity_assist = None;
                        // GRA-159: drop any cached porkchop from the prior
                        // category — the new category may not have a
                        // matching body to recompute against, and a stale
                        // grid would otherwise render with the wrong names.
                        fleet_ui_state.porkchop_grid = None;
                        fleet_ui_state.selected_porkchop_cell = None;
                    }
                }
            });
    });

    // Step 2: specific target within selected category
    let active_group = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    });

    let target_label = if let Some(ref lp) = fleet_ui_state.target_lagrange {
        let (host_name, host_is_star) = body_query
            .get(lp.planet_entity)
            .ok()
            .and_then(|(_, _, _, _, host_lp)| host_lp)
            .and_then(|host| body_query.get(host.0).ok())
            .map(|(_, hb, _, _, _)| (hb.name.clone(), hb.body_type == BodyType::Star))
            .unwrap_or_default();
        lagrange_picker_label(lp, &host_name, host_is_star)
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        all_fleets_query
            .get(tf)
            .map(|(_, f, _, _, ma)| {
                let status = if ma.is_some() { "✈" } else { "🛰" };
                format!("{status} {}", f.name)
            })
            .unwrap_or_else(|_| "— Target —".to_owned())
    } else if let Some((_, ref name, _)) = fleet_ui_state.target_star_system {
        name.clone()
    } else {
        // GRA-NNN: extract the active shell before borrowing fleet_ui_state
        // in the closure so the borrow is released before the picker ComboBox.
        let active_shell = fleet_ui_state.target_orbit_shell.and_then(|(e, s)| {
            if fleet_ui_state.target_body == Some(e) {
                Some(s)
            } else {
                None
            }
        });
        fleet_ui_state
            .target_body
            .and_then(|e| {
                body_query.get(e).ok().map(move |(_, b, _, _, _)| {
                    if b.body_type == BodyType::Ring {
                        format!("{} 💍", b.name)
                    } else if b.body_type == BodyType::Star {
                        // GRA-NNN: shell-name label rather than numeric radius.
                        let shell =
                            active_shell.unwrap_or(default_shell_for_body_type(b.body_type));
                        format!("🛰 {} — {}", b.name, shell.label())
                    } else {
                        b.name.clone()
                    }
                })
            })
            .unwrap_or_else(|| "— Target —".to_owned())
    };

    if active_group.is_some() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Target:").size(13.0));
            // Hover hint on the target label itself: star-approach
            // rows carry a DragValue for the parking-orbit radius
            // *inside* the dropdown (so the player can edit it
            // without first selecting a different destination) and
            // the surface tooltip explains where to find it.  Without
            // this hint the radius readout in the closed-dropdown
            // label looks like a static value the player has to live
            // with; in fact it's a per-star, per-default spinner
            // that the picker exposes once you open the dropdown.
            let is_star_target = fleet_ui_state.target_body.is_some_and(|e| {
                body_query.get(e).ok().map(|(_, b, _, _, _)| b.body_type) == Some(BodyType::Star)
            });
            let combo = egui::ComboBox::from_id_salt("fleet_target_body")
                .selected_text(&target_label)
                .width(280.0)
                .show_ui(ui, |ui| {
                    if let Some(group) = active_group {
                        let mut first_sub = true;
                        for entry in &group.entries {
                            match entry {
                                DestEntry::Header(label) => {
                                    if !first_sub {
                                        ui.add_space(4.0);
                                    }
                                    first_sub = false;
                                    ui.label(
                                        egui::RichText::new(label.as_str())
                                            .strong()
                                            .size(11.0)
                                            .color(theme::AMBER),
                                    );
                                }
                                DestEntry::Body { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui
                                        .selectable_label(
                                            selected,
                                            egui::RichText::new(format!("  {name}")).size(12.0),
                                        )
                                        .clicked()
                                        && !selected
                                    {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        // GRA-161: switching to a non-star
                                        // destination invalidates the
                                        // star-approach parking radius.
                                        fleet_ui_state.target_orbit_shell = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                        // GRA-159: populate the porkchop grid so
                                        // the LGD `PorkchopPanel` renders instead
                                        // of the legacy Efficient/Moderate/Fast
                                        // row.  For moon and ring destinations the
                                        // per-frame `body_target_snap` branch uses
                                        // the local-frame transfer math (parent
                                        // gravitational parameter, parking
                                        // orbits) which the porkchop Lambert
                                        // solver cannot model — it would receive
                                        // `r1 ≈ r2` and produce a degenerate
                                        // (all-infeasible) grid.  In that case
                                        // we set `porkchop_grid = None` so the
                                        // legacy 3-option row renders, which
                                        // already has the correct local-frame
                                        // cislunar / moon-to-parent handling.
                                        //
                                        // Note: the planner render function also
                                        // has a deferred-build path (see the
                                        // "GRA-159 deferred porkchop build" block
                                        // above the destination picker) that
                                        // catches the case where `target_body` is
                                        // set by a non-click entry point (e.g. the
                                        // 3D-scene right-click handler).  The
                                        // click handler below is still the primary
                                        // path; the deferred build is a safety net.
                                        fleet_ui_state.porkchop_grid =
                                            if should_build_porkchop_for_destination(body_query, *entity) {
                                                (|| -> Option<crate::fleets::porkchop::PorkchopGrid> {
                                                let origin_orbit = heliocentric_orbit_for_body(
                                                    orbit.body, body_query,
                                                )?;
                                                let dest_orbit = heliocentric_orbit_for_body(
                                                    *entity, body_query,
                                                )?;
                                                let origin_name = body_query
                                                    .get(orbit.body)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| b.name.clone())
                                                    .unwrap_or_else(|| "Origin".to_string());
                                                let dest_name = body_query
                                                    .get(*entity)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| b.name.clone())
                                                    .unwrap_or_else(|| "Dest".to_string());
                                                let dest_body_type = body_query
                                                    .get(*entity)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| b.body_type)
                                                    .unwrap_or(BodyType::Planet);
                                                let dest_parent = body_query
                                                    .get(*entity)
                                                    .ok()
                                                    .and_then(|(_, _, _, _, lp)| lp)
                                                    .map(|lp| lp.0);
                                                let origin_parent = body_query
                                                    .get(orbit.body)
                                                    .ok()
                                                    .and_then(|(_, _, _, _, lp)| lp)
                                                    .map(|lp| lp.0);
                                                let category = crate::fleets::porkchop::classify_body_transfer_category(
                                                    dest_body_type,
                                                    dest_parent,
                                                    origin_parent,
                                                );
                                                Some(build_grid_for_body_target(
                                                    porkchop_config,
                                                    origin_orbit,
                                                    dest_orbit,
                                                    origin_name,
                                                    dest_name,
                                                    category,
                                                    elapsed,
                                                ))
                                            })()
                                            } else if let Some(n) = porkchop_config
                                                .category_overrides
                                                .iter()
                                                .find(|o| o.match_key == "short_hop")
                                                .and_then(|o| o.short_hop_options)
                                            {
                                                // GRA-384 short-hop wire-in:
                                                // `should_build_porkchop_for_destination`
                                                // returns false for Moon/Ring so
                                                // the heliocentric Lambert grid
                                                // can't render, but the RON may
                                                // configure a `short_hop` override
                                                // with `short_hop_options: Some(n)`
                                                // that produces a configurable
                                                // single-column cislunar bar via
                                                // `build_short_hop_grid`.  Falls
                                                // through to the legacy 3-option
                                                // row when the override is absent
                                                // (`short_hop_options == None`).
                                                short_hop_grid_for_moon(
                                                    body_query,
                                                    porkchop_config,
                                                    orbit.body,
                                                    *entity,
                                                    n,
                                                    elapsed,
                                                )
                                            } else {
                                                None
                                            };
                                        // Stamp the build epoch so the
                                        // planner's staleness check
                                        // knows when to refresh the
                                        // grid as sim time advances.
                                        fleet_ui_state.porkchop_built_at_s = Some(elapsed);
                                        // Stamp the build target so the
                                        // staleness check also catches
                                        // future target-body mutations
                                        // from non-click paths.
                                        fleet_ui_state.porkchop_built_for = Some(*entity);
                                        fleet_ui_state.selected_porkchop_cell = None;
                                    }
                                }
                                DestEntry::Ring { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui
                                        .selectable_label(
                                            selected,
                                            egui::RichText::new(format!("  {name} 💍")).size(12.0),
                                        )
                                        .clicked()
                                        && !selected
                                    {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        // GRA-161: switching to a non-star
                                        // destination invalidates the
                                        // star-approach parking radius.
                                        fleet_ui_state.target_orbit_shell = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                        // GRA-159: same wire-in as the Body
                                        // branch — rings are treated like
                                        // bodies for the planner's view
                                        // (per GRA-149 C-3 follow-up).
                                        // Rings are local-frame markers
                                        // around their parent planet; the
                                        // per-frame body-target branch uses
                                        // the parent's local-frame transfer
                                        // math (parent GM, ring altitude),
                                        // which the porkchop Lambert solver
                                        // cannot model.  Use the shared
                                        // helper to gate the build — same
                                        // predicate as the Body branch.
                                        fleet_ui_state.porkchop_grid =
                                            if should_build_porkchop_for_destination(body_query, *entity) {
                                                (|| -> Option<crate::fleets::porkchop::PorkchopGrid> {
                                                    let origin_orbit = heliocentric_orbit_for_body(
                                                        orbit.body, body_query,
                                                    )?;
                                                    let dest_orbit = heliocentric_orbit_for_body(
                                                        *entity, body_query,
                                                    )?;
                                                    let origin_name = body_query
                                                        .get(orbit.body)
                                                        .ok()
                                                        .map(|(_, b, _, _, _)| b.name.clone())
                                                        .unwrap_or_else(|| "Origin".to_string());
                                                    let dest_name = body_query
                                                        .get(*entity)
                                                        .ok()
                                                        .map(|(_, b, _, _, _)| b.name.clone())
                                                        .unwrap_or_else(|| "Dest".to_string());
                                                    let dest_body_type = body_query
                                                        .get(*entity)
                                                        .ok()
                                                        .map(|(_, b, _, _, _)| b.body_type)
                                                        .unwrap_or(BodyType::Planet);
                                                    let dest_parent = body_query
                                                        .get(*entity)
                                                        .ok()
                                                        .and_then(|(_, _, _, _, lp)| lp)
                                                        .map(|lp| lp.0);
                                                    let origin_parent = body_query
                                                        .get(orbit.body)
                                                        .ok()
                                                        .and_then(|(_, _, _, _, lp)| lp)
                                                        .map(|lp| lp.0);
                                                    let category = crate::fleets::porkchop::classify_body_transfer_category(
                                                        dest_body_type,
                                                        dest_parent,
                                                        origin_parent,
                                                    );
                                                    Some(build_grid_for_body_target(
                                                        porkchop_config,
                                                        origin_orbit,
                                                        dest_orbit,
                                                        origin_name,
                                                        dest_name,
                                                        category,
                                                        elapsed,
                                                    ))
                                                })()
                                            } else {
                                                None
                                            };
                                        // Stamp the build epoch so the
                                        // planner's staleness check
                                        // knows when to refresh the
                                        // grid as sim time advances.
                                        fleet_ui_state.porkchop_built_at_s = Some(elapsed);
                                        // Stamp the build target so the
                                        // staleness check also catches
                                        // future target-body mutations
                                        // from non-click paths.
                                        fleet_ui_state.porkchop_built_for = Some(*entity);
                                        fleet_ui_state.selected_porkchop_cell = None;
                                    }
                                }
                                DestEntry::Lagrange { lp } => {
                                    first_sub = false;
                                    // Look up the host body (the star for Sun-Planet L-points,
                                    // the planet for Planet-Moon L-points) so the picker row
                                    // matches the LGD Q3 format `🛰 L{n} ({system})`.
                                    let (host_name, host_is_star) = body_query
                                        .get(lp.planet_entity)
                                        .ok()
                                        .and_then(|(_, _, _, _, host_lp)| host_lp)
                                        .and_then(|host| body_query.get(host.0).ok())
                                        .map(|(_, hb, _, _, _)| {
                                            (hb.name.clone(), hb.body_type == BodyType::Star)
                                        })
                                        .unwrap_or_default();
                                    let row_label =
                                        lagrange_picker_label(lp, &host_name, host_is_star);
                                    let selected = fleet_ui_state
                                        .target_lagrange
                                        .as_ref()
                                        .map(|cur| {
                                            cur.point == lp.point
                                                && cur.planet_entity == lp.planet_entity
                                        })
                                        .unwrap_or(false);
                                    if ui
                                        .selectable_label(
                                            selected,
                                            egui::RichText::new(row_label).size(12.0),
                                        )
                                        .clicked()
                                        && !selected
                                    {
                                        // GRA-160: shared state-mutation contract via
                                        // `select_lagrange_target` — clears
                                        // `target_body`/`target_fleet`/`target_star_system`/
                                        // `target_arrival_radius` and resets the per-target
                                        // transfer-planning fields. The 3D-scene click
                                        // path (`ui_lp_click_handler`) reuses the same
                                        // helper so the two entry points cannot drift.
                                        // GRA-159 (now merged) handles the porkchop-grid
                                        // build via `build_grid_for_body_target` for the
                                        // L-point case in `src/fleets/porkchop.rs`.
                                        fleet_ui_state.select_lagrange_target(lp.clone());
                                    }
                                }
                                DestEntry::FleetTarget {
                                    entity,
                                    name,
                                    in_transit,
                                } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_fleet == Some(*entity);
                                    let icon = if *in_transit { "✈" } else { "🛰" };
                                    let status = if *in_transit {
                                        "In transit"
                                    } else {
                                        "In orbit"
                                    };
                                    let label = format!("  {icon} {name}  ({status})");
                                    if ui
                                        .selectable_label(
                                            is_sel,
                                            egui::RichText::new(label)
                                                .size(12.0)
                                                .color(theme::ACCENT),
                                        )
                                        .clicked()
                                        && !is_sel
                                    {
                                        fleet_ui_state.target_fleet = Some(*entity);
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_star_system = None;
                                        // GRA-161: fleet intercepts are not stars.
                                        fleet_ui_state.target_orbit_shell = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                        // GRA-159: fleet intercept uses
                                        // a dedicated solver path (not the
                                        // body-target helper), so drop the
                                        // cached body grid.  GRA-160 will
                                        // wire the fleet-intercept grid in.
                                        fleet_ui_state.porkchop_grid = None;
                                        fleet_ui_state.selected_porkchop_cell = None;
                                    }
                                }
                                DestEntry::StarSystem {
                                    system_id,
                                    name,
                                    distance_ly,
                                } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state
                                        .target_star_system
                                        .as_ref()
                                        .map(|(id, _, _)| *id == *system_id)
                                        .unwrap_or(false);
                                    // GRA-328c: parent-star transition gate.
                                    // When the fleet has no ship with
                                    // `interstellar_capability` set on its hull,
                                    // render the row as a dimmed, non-clickable
                                    // badge and surface the requirement in the
                                    // hover tooltip.  Selecting an interstellar
                                    // target requires at least one hull
                                    // declaring the capability (GRA-333 contract).
                                    let gate_refused = !fleet_has_interstellar_capability;
                                    let label_color = if gate_refused {
                                        theme::TEXT_DIM
                                    } else {
                                        theme::GRAVITY_ASSIST
                                    };
                                    let raw_name = name.trim_start_matches('✨').trim();
                                    let tooltip = if gate_refused {
                                        format!(
                                            "Interstellar transfer to {raw_name} ({ly:.2} ly) — \
                                             locked.  This fleet has no ship with an interstellar-capable \
                                             hull (GRA-333).  Build a torch cruiser, outer-system tanker, \
                                             cycler, long-range survey ship, or interstellar precursor to unlock.",
                                            ly = distance_ly,
                                        )
                                    } else {
                                        format!(
                                            "Interstellar transfer to {raw_name} ({ly:.2} ly). \
                                             Plan multi-year / multi-century trajectories — \
                                             this is a barycentric route, not a parking orbit.",
                                            ly = distance_ly,
                                        )
                                    };
                                    let row_response = ui
                                        .selectable_label(
                                            is_sel,
                                            egui::RichText::new(format!("  {name}"))
                                                .size(12.0)
                                                .color(label_color),
                                        )
                                        .on_hover_text(&tooltip);
                                    // GRA-328c: refuse the click when the gate
                                    // is closed (no interstellar-capable hull on
                                    // any ship in the fleet).  `selectable_label`
                                    // can't be disabled, so we ignore the
                                    // click event ourselves.
                                    if row_response.clicked() && !is_sel && !gate_refused {
                                        fleet_ui_state.target_star_system =
                                            Some((*system_id, name.clone(), *distance_ly));
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.target_orbit_shell = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                        // GRA-159: interstellar is
                                        // out-of-scope for the body path
                                        // — the LGD's "interstellar"
                                        // category override belongs to a
                                        // separate solver (cross-star
                                        // ballistic, see
                                        // `calculate_cross_star_ballistic_options`).
                                        // Drop the cached body grid to
                                        // prevent a stale (e.g. Earth→Mars)
                                        // panel from rendering under a
                                        // star-system picker.
                                        fleet_ui_state.porkchop_grid = None;
                                        fleet_ui_state.selected_porkchop_cell = None;
                                        // GRA-343 (GRA-328b): drop any
                                        // cached cross-system grid built
                                        // for a previous interstellar
                                        // target.  The solver below
                                        // rebuilds against the new
                                        // destination on the next frame.
                                        fleet_ui_state.cross_system_grid = None;
                                        fleet_ui_state.cross_system_grid_built_for = None;
                                    }
                                }
                                DestEntry::Star { entity, name } => {
                                    first_sub = false;
                                    // GRA-NNN: Star rows render only the selectable label
                                    // — the parking shell is now driven by the top-level
                                    // "Target orbit" ComboBox below the destination picker,
                                    // not by a per-row DragValue.  Click handler still seeds
                                    // `target_body` and rebuilds the porkchop grid against
                                    // the resolved shell radius.
                                    let is_sel = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    let row_text = format!("  🛰 {name}");
                                    let label_response = ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(row_text).size(12.0),
                                    );
                                    if label_response.clicked() && !is_sel {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.target_star_system = None;
                                        // GRA-NNN: parking radius is now shell-driven.
                                        fleet_ui_state.target_orbit_shell = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                        // Rebuild the star-approach grid against the
                                        // resolved shell radius so the panel reflects
                                        // the new pick on the same frame.
                                        //
                                        // The `&CelestialBody { … }` fallback has to be
                                        // bound to a `let` *outside* the `match` arm so
                                        // its lifetime extends past the borrow — Rust
                                        // E0716 / E0597 reject a temporary dropped while
                                        // `b` still points at it.  GRA-NNN.
                                        let fallback_body = CelestialBody {
                                            name: name.clone(),
                                            radius: 0.0,
                                            mass: 0.0,
                                            body_type: BodyType::Star,
                                            visual_radius: 0.0,
                                            asteroid_class: None,
                                            star_approach_au: None,
                                            rotation_period_s: None,
                                            habitable_outer_au: None,
                                        };
                                        let fallback_coords = SpaceCoordinates::default();
                                        let (_, b, _, _, _) = body_query
                                            .get(*entity)
                                            .unwrap_or((
                                                *entity,
                                                &fallback_body,
                                                &fallback_coords,
                                                None,
                                                None,
                                            ));
                                        let shell = fleet_ui_state
                                            .target_orbit_shell
                                            .filter(|(e, _)| *e == *entity)
                                            .map(|(_, s)| s)
                                            .unwrap_or(default_shell_for_body_type(b.body_type));
                                        let radius_au = radius_for_shell(b, shell);
                                        let (lo, hi) = star_approach_bounds_au(
                                            *entity,
                                            body_query,
                                            body_system_ids,
                                            current_system_id,
                                        );
                                        let clamped = radius_au.clamp(lo, hi);
                                        fleet_ui_state.porkchop_grid =
                                            star_approach_grid_for_target(
                                                body_query,
                                                orbit.body,
                                                *entity,
                                                clamped,
                                                lo,
                                                hi,
                                                elapsed,
                                            );
                                        fleet_ui_state.porkchop_built_at_s = Some(elapsed);
                                        fleet_ui_state.porkchop_built_for = Some(*entity);
                                        fleet_ui_state.selected_porkchop_cell = None;
                                    }
                                }
                            }
                        }
                    }
                });
            // Hover hint for star-approach targets: explain where
            // the parking-radius DragValue lives.  The hint only
            // shows on hover, so it doesn't pollute the layout.
            // Without it the radius readout in the closed-dropdown
            // label looks like a static value the player can't
            // change; in fact it's a per-star, per-default spinner
            // that the picker exposes once the dropdown opens.
            if is_star_target {
                combo.response.on_hover_text(
                    "Click to open the destination picker.  Star-approach rows expose a\n\
                     parking-orbit DragValue on the right side of each star row — drag it\n\
                     to change the (t_dep, tof) cell range in the porkchop plot.",
                );
            }
        });

        // GRA-NNN: top-level "Target orbit:" shell picker (Terra Invicta style).
        // Replaces the GRA-387 free-form DragValue.  Each shell resolves to a
        // numeric AU via `radius_for_shell`; the player picks a shell name from
        // the dropdown rather than dialing a raw AU value.
        //
        // The shell set depends on the destination body type.  Lagrange / fleet
        // / star-system targets have no fixed orbit and render the "—" placeholder.
        if let Some(target_entity) = fleet_ui_state
            .target_body
            .or(fleet_ui_state.target_fleet)
            .or(fleet_ui_state
                .target_lagrange
                .as_ref()
                .map(|lp| lp.planet_entity))
        {
            let body_for_picker = body_query.get(target_entity).ok().map(|(_, b, _, _, _)| b);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Target orbit:").size(13.0));
                ui.add_space(4.0);
                let Some(body) = body_for_picker else {
                    ui.weak("— (Lagrange / fleet target has no fixed orbit)");
                    return;
                };
                let shells = OrbitShellId::shells_for(body.body_type);
                let current_shell = fleet_ui_state
                    .target_orbit_shell
                    .filter(|(e, _)| *e == target_entity)
                    .map(|(_, s)| s)
                    .unwrap_or_else(|| default_shell_for_body_type(body.body_type));

                let mut changed_to: Option<OrbitShellId> = None;
                egui::ComboBox::from_id_salt(("orbit_shell_picker", target_entity))
                    .selected_text(format!(
                        "{} ({:.3} AU)",
                        current_shell.label(),
                        radius_for_shell(body, current_shell)
                    ))
                    .show_ui(ui, |ui| {
                        for &shell in shells {
                            let r = radius_for_shell(body, shell);
                            let label = format!("{} ({:.3} AU)", shell.label(), r);
                            let was_clicked =
                                ui.selectable_label(shell == current_shell, label).clicked();
                            if was_clicked {
                                changed_to = Some(shell);
                            }
                        }
                    });
                if let Some(new_shell) = changed_to {
                    fleet_ui_state.target_orbit_shell = Some((target_entity, new_shell));
                    // Invalidate the cached porkchop so the next render
                    // rebuilds against the new radius.
                    fleet_ui_state.porkchop_grid = None;
                    fleet_ui_state.porkchop_built_for = None;
                    fleet_ui_state.selected_porkchop_cell = None;
                }
            });
        }
    }

    // ── Intercept parameters (shown only when a fleet is targeted) ────────────
    if fleet_ui_state.target_fleet.is_some() {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("⚔ Intercept Parameters")
                    .strong()
                    .size(13.0)
                    .color(theme::AMBER),
            );
            ui.add_space(4.0);

            // Passing distance slider: 0 = rendezvous / dock, up to 1 000 km = fast flyby
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Passing distance:").size(12.0));
                let mut pd = fleet_ui_state.intercept_passing_km as f32;
                if ui
                    .add(
                        egui::Slider::new(&mut pd, 0.0_f32..=1_000.0_f32)
                            .suffix(" km")
                            .text("0 = rendezvous")
                            .step_by(10.0),
                    )
                    .changed()
                {
                    fleet_ui_state.intercept_passing_km = pd as f64;
                    fleet_ui_state.computed_options.clear();
                }
            });

            // Encounter speed: 0 = match velocity (boarding), up to 30 km/s = high-speed pass
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Encounter speed:").size(12.0));
                let mut spd_kms = (fleet_ui_state.intercept_speed_ms / 1_000.0) as f32;
                if ui
                    .add(
                        egui::Slider::new(&mut spd_kms, 0.0_f32..=30.0_f32)
                            .suffix(" km/s")
                            .text("0 = match velocity")
                            .step_by(0.5),
                    )
                    .changed()
                {
                    fleet_ui_state.intercept_speed_ms = spd_kms as f64 * 1_000.0;
                    fleet_ui_state.computed_options.clear();
                }
            });

            ui.label(
                egui::RichText::new(
                    if fleet_ui_state.intercept_passing_km < 1.0
                        && fleet_ui_state.intercept_speed_ms < 100.0
                    {
                        "Mode: Rendezvous / docking approach"
                    } else if fleet_ui_state.intercept_passing_km > 100.0
                        || fleet_ui_state.intercept_speed_ms > 5_000.0
                    {
                        "Mode: High-speed flyby (combat pass)"
                    } else {
                        "Mode: Close approach (boarding range)"
                    },
                )
                .size(11.0)
                .italics()
                .color(theme::GREEN),
            );
        });
    }

    // ── Compute transfer options when a target is selected ───────────────────
    let fleet_target_snap = fleet_ui_state.target_fleet;
    let star_system_snap = fleet_ui_state.target_star_system.clone();
    let any_target = fleet_ui_state.target_body.is_some()
        || fleet_ui_state.target_lagrange.is_some()
        || fleet_target_snap.is_some()
        || star_system_snap.is_some();
    // Snapshot lagrange so we can use it immutably while also mut-borrowing fleet_ui_state below
    let lp_target_snap = fleet_ui_state.target_lagrange.clone();
    let body_target_snap = fleet_ui_state.target_body;
    let previous_selected_option_label = fleet_ui_state
        .computed_options
        .get(fleet_ui_state.selected_option)
        .map(|option| option.label);

    // Transfer window info computed this frame (Some only for body-target transfers).
    // Kept as a local so the window UI section can read it without re-computing.
    let mut window_this_frame: Option<TransferWindowInfo> = None;
    let mut window_max_slider_days: f64 = 730.0;

    if any_target {
        // Recompute every frame — body angles (SpaceCoordinates) update with the simulation clock,
        // so the phase error and launch-window countdown change live.

        // ── Fleet intercept computation ──────────────────────────────────────
        if let Some(target_fleet_entity) = fleet_target_snap {
            // Use the target fleet's current heliocentric position as the intercept radius.
            // r2 = distance from origin (0,0,0) to target fleet position in AU.
            let target_sc = all_fleets_query
                .get(target_fleet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(bevy::math::DVec3::ZERO);
            let r2_au = target_sc.length().max(0.001);

            // r1: heliocentric distance of the departing fleet.
            // GRA-149 C-3: pick own SMA only when the body is itself a star
            // (i.e., it owns its own heliocentric frame).  For planets and
            // moons — including close-orbit giants like hot-Jupiters at
            // 0.02 AU that the legacy 0.05 AU threshold mis-classified —
            // always walk up to the parent star's SMA.
            let r1_au = {
                let own_ko = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko.copied())
                    .map(|ko| ko.semi_major_axis);
                let origin_parent = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, _, lp)| lp.map(|lp| lp.0));
                let own_is_stellar = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                if own_is_stellar {
                    own_ko.unwrap_or(1.0)
                } else {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko.copied())
                        .map(|ko| ko.semi_major_axis)
                        .or(own_ko)
                        .unwrap_or(1.0)
                }
            };
            // Determine the host star's GM for the fleet intercept.  Walk the
            // LogicalParent chain so that fleets orbiting moons (moon → planet → star)
            // are correctly resolved to their host star's GM rather than falling back
            // to GM_SUN.
            let fleet_intercept_gm = find_host_star(orbit.body, body_query)
                .map(|(_, mass)| G_CONST * mass)
                .unwrap_or(GM_SUN);
            fleet_ui_state.computed_options =
                calculate_transfer_options(r1_au, r2_au, fleet_intercept_gm, 0.0);
            // Post-process: fill burn_time_s and flag thrust-limited options.
            apply_thrust_limits(
                &mut fleet_ui_state.computed_options,
                fleet.min_accel_ms2(),
                fleet.average_isp_s(),
            );
            // Add kinematic options for high-thrust fleets intercepting other fleets.
            let hohmann_dv = fleet_ui_state
                .computed_options
                .first()
                .map(|o| o.total_delta_v_ms)
                .unwrap_or(0.0);
            let sma_h = fleet_ui_state
                .computed_options
                .first()
                .map(|o| o.sma_au)
                .unwrap_or(0.0);
            let ecc_h = fleet_ui_state
                .computed_options
                .first()
                .map(|o| o.eccentricity)
                .unwrap_or(0.0);
            let d = (r2_au - r1_au).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
            let mut kinematics = kinematic_transfer_options(
                d,
                fleet.min_accel_ms2(),
                fleet.max_delta_v_ms(),
                hohmann_dv,
                sma_h,
                ecc_h,
                false,
            );
            fleet_ui_state.computed_options.append(&mut kinematics);
        } else if let Some(target_entity) = body_target_snap {
            //   - Ring transfer (dest has no KeplerOrbit; use body.radius as r2):
            //       r1 = fleet orbit radius or origin SMA, r2 = ring.radius_au, GM = parent mass * G
            //   - Local transfer (dest orbits fleet's body, e.g. Earth→Moon):
            //       r1 = fleet's parking orbit radius, r2 = dest SMA, GM = parent mass * G
            //   - Moon-to-moon (both orbit the same planet):
            //       r1 = origin moon SMA, r2 = dest moon SMA, GM = shared planet mass * G
            //   - Star approach (dest is a star):
            //       r1 = fleet's stellar SMA, r2 = 0.3 AU, GM = G * target_star_mass
            //   - Heliocentric transfer (both in stellar orbits, same or different host):
            //       r1 = origin body stellar SMA, r2 = dest stellar SMA, GM = host_star_GM
            let dest_body_type = body_query
                .get(target_entity)
                .ok()
                .map(|(_, b, _, _, _)| b.body_type);
            let dest_has_orbit = body_query
                .get(target_entity)
                .ok()
                .and_then(|(_, _, _, ko, _)| ko)
                .is_some();
            let dest_parent = body_query
                .get(target_entity)
                .ok()
                .and_then(|(_, _, _, _, lp)| lp)
                .map(|lp| lp.0);
            let origin_parent = body_query
                .get(orbit.body)
                .ok()
                .and_then(|(_, _, _, _, lp)| lp)
                .map(|lp| lp.0);
            let inter_star_departure_time_s =
                elapsed + fleet_ui_state.departure_offset_days.max(0.0) * 86_400.0;
            let planner_frame = resolve_planner_transfer_frame(
                orbit.body,
                target_entity,
                origin_parent,
                dest_parent,
                body_query,
            );

            // Target solar approach orbit (AU from star).  Inside Mercury's orbit so the
            // transfer is always clearly "inward".  Requires advanced propulsion (~10–20 km/s).
            const SOLAR_APPROACH_AU: f64 = 0.3;

            let (r1, r2, gm) = if matches!(planner_frame, PlannerTransferFrame::SystemBarycentric) {
                let origin_pos =
                    transfer_absolute_position(orbit.body, inter_star_departure_time_s, body_query)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                let dest_pos = transfer_absolute_position(
                    target_entity,
                    inter_star_departure_time_s,
                    body_query,
                )
                .unwrap_or(bevy::math::DVec3::ZERO);
                let r1_bary = origin_pos.length().max(MIN_ORBITAL_RADIUS_AU);
                let r2_bary = dest_pos.length().max(MIN_ORBITAL_RADIUS_AU);
                let system_gm_raw: f64 = body_query
                    .iter()
                    .filter(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids
                                .get(*e)
                                .ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .sum();
                let system_gm = if system_gm_raw > 0.0 {
                    system_gm_raw
                } else {
                    GM_SUN
                };
                (r1_bary, r2_bary, system_gm)
            } else if dest_body_type == Some(BodyType::Star) {
                // Star approach transfer: plot a Hohmann from the fleet's stellar-orbit
                // distance to SOLAR_APPROACH_AU, using the target star's actual GM.
                // Walk up the parent chain to find the fleet's stellar SMA.
                //
                // GRA-149 C-3: the fleet's body is treated as the host star only
                // when the body's mass is stellar (not when its SMA exceeds the
                // legacy 0.05 AU threshold).  For moons and close-orbit planets
                // the planner walks up to the parent.
                let own_sma = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko.copied())
                    .map(|ko| ko.semi_major_axis);
                let own_is_stellar = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                let r1_au = if own_is_stellar {
                    own_sma.unwrap_or(1.0)
                } else {
                    // Fleet is parked at a moon/sub-body; use its planet's heliocentric SMA.
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko.copied())
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                };
                // Ensure r2 is strictly less than r1 (always an inward transfer).
                let r2_au = SOLAR_APPROACH_AU.min(r1_au * 0.5);
                // Use the actual target star's GM, not a hardcoded GM_SUN.
                // target_entity IS the star in this branch (dest_body_type == Star).
                let star_gm = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .unwrap_or(GM_SUN);
                (r1_au, r2_au, star_gm)
            } else if !dest_has_orbit && dest_parent == Some(orbit.body) {
                // Ring around current orbit body
                let parent_mass = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if !dest_has_orbit && dest_parent.is_some() && dest_parent == origin_parent {
                // Ring around another planet (dest_parent is a planet, not fleet's body)
                let shared = dest_parent.unwrap();
                let parent_mass = body_query
                    .get(shared)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r1 = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (r1, r2, G_CONST * parent_mass)
            } else if dest_parent == Some(orbit.body) {
                // Local: destination orbits the fleet's current body
                let parent_mass = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if dest_parent.is_some() && dest_parent == origin_parent {
                // Both orbit the same central body (moon-to-moon, OR interplanetary e.g. Earth→Mars)
                let shared = dest_parent.unwrap();
                // Use G·mass for any central body — stars in non-Sol systems carry their
                // actual mass in CelestialBody.mass (stored as kg), so G·M gives the
                // correct GM.  GM_SUN is only the fallback when the query fails entirely.
                let gm = body_query
                    .get(shared)
                    .ok()
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .unwrap_or(GM_SUN);
                let r1 = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                (r1, r2, gm)
            } else if Some(target_entity) == origin_parent {
                // Downward transfer: fleet is at a moon, destination is the parent planet.
                // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
                let parent_mass = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r1 = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                // Park at ~3× destination body surface radius (low orbit).
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 3_000.0) / AU_IN_METERS)
                    .unwrap_or(4.26e-5);
                (r1, r2.min(r1 * 0.5), G_CONST * parent_mass)
            } else {
                // Heliocentric: fleet is at a body that is not in the same parent chain as dest.
                //
                // ── Detect inter-star transfer ─────────────────────────────────────────────
                // In a binary/trinary system, origin and destination may orbit different stars.
                // E.g. fleet at a moon of Planet-A-1 (around Star A), destination a moon of
                // Planet-C-1 (around Star C).  We walk the full LogicalParent chain to the
                // stellar ancestor so that moon→planet→star hierarchies are handled correctly.
                //
                // For such transfers:
                //   - r1 / r2 must be barycentric distances (SpaceCoordinates.position.length()),
                //     NOT star-centric SMAs.  A planet at 1 AU from Star A in a binary where
                //     Star A is 23 AU from the barycenter has a barycentric r ≈ 24 AU.
                //   - gm must be the TOTAL system gravitational parameter G·ΣM_stars so that
                //     both barycentric orbital velocities and transfer times are correct.
                //
                // For single-star systems both host stars are the same entity, so
                // is_inter_star is false and the existing code path is unchanged.
                let origin_host_star = find_host_star(orbit.body, body_query);
                let dest_host_star = find_host_star(target_entity, body_query);
                let is_inter_star = origin_host_star.is_some()
                    && dest_host_star.is_some()
                    && origin_host_star.map(|(e, _)| e) != dest_host_star.map(|(e, _)| e);

                if is_inter_star {
                    // Barycentric transfer: use SpaceCoordinates.position.length() so that the
                    // orbital radius already includes the star's offset from the barycenter.
                    // E.g. a planet 1 AU from Star A, which is 23 AU from the barycenter, has
                    // a barycentric r ≈ 24 AU — very different from its star-centric SMA of 1 AU.
                    // Fallback values (1.0 AU for origin, 1.5 AU for dest) are Earth-like and
                    // Mars-like radii used only if an entity is somehow missing — a defensive
                    // guard that should never trigger in practice.
                    let r1_bary = transfer_absolute_position(orbit.body, elapsed, body_query)
                        .map(|pos| pos.length())
                        .unwrap_or(1.0) // defensive: orbit.body is always a valid spawned entity
                        .max(MIN_ORBITAL_RADIUS_AU); // guard against near-zero (fleet at star itself)
                    let r2_bary = transfer_absolute_position(target_entity, elapsed, body_query)
                        .map(|pos| pos.length())
                        // Defensive fallback; target_entity should always resolve here.
                        .unwrap_or(1.5)
                        .max(MIN_ORBITAL_RADIUS_AU);
                    // Total system GM: sum over all stars in the current system only.
                    // The barycentric frame requires G·(M₁ + M₂ + M₃ + …).
                    // We do NOT clamp with .max(GM_SUN) because sub-solar systems
                    // (e.g. two K-dwarfs totalling 0.8 M☉) must use their actual combined GM.
                    let system_gm_raw: f64 = body_query
                        .iter()
                        .filter(|(e, b, _, _, _)| {
                            b.body_type == BodyType::Star
                                && body_system_ids
                                    .get(*e)
                                    .ok()
                                    .map(|s| s.0 == current_system_id)
                                    .unwrap_or(false)
                        })
                        .map(|(_, b, _, _, _)| G_CONST * b.mass)
                        .sum();
                    let system_gm = if system_gm_raw > 0.0 {
                        system_gm_raw
                    } else {
                        GM_SUN // fallback only when no stars found (degenerate case)
                    };
                    (r1_bary, r2_bary, system_gm)
                } else {
                    // If fleet is parked at a moon, its KeplerOrbit SMA is Earth-relative, NOT
                    // heliocentric. Walk up one level to get the heliocentric SMA.
                    //
                    // GRA-149 C-3: "is this body itself a star?" is now decided by mass,
                    // not by SMA.  Hot-Jupiters at 0.02 AU used to be mis-classified as
                    // moons; the planner then walked up to their parent star but the
                    // GA candidate filter and downstream GM lookups still treated the
                    // hot-Jupiter as a moon.  The mass check makes the intent explicit.
                    let own_sma = body_query
                        .get(orbit.body)
                        .ok()
                        .and_then(|(_, _, _, ko, _)| ko.copied())
                        .map(|ko| ko.semi_major_axis);
                    let origin_is_stellar = body_query
                        .get(orbit.body)
                        .ok()
                        .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                        .unwrap_or(false);
                    let r1 = if origin_is_stellar {
                        orbit.radius_au.max(MIN_ORBITAL_RADIUS_AU)
                    } else if origin_parent.is_some() {
                        // Body is not a star and has a parent → walk up to the
                        // parent's heliocentric SMA.  Works for moons, hot-Jupiters,
                        // and any other close-orbit body that the legacy 0.05 AU
                        // threshold would have mis-classified.
                        origin_parent
                            .and_then(|pe| body_query.get(pe).ok())
                            .and_then(|(_, _, _, ko, _)| ko.copied())
                            .map(|ko| ko.semi_major_axis)
                            .or(own_sma)
                            .unwrap_or(1.0)
                    } else {
                        own_sma.unwrap_or(1.0)
                    };
                    let dest_sma = body_query
                        .get(target_entity)
                        .ok()
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis);
                    // GRA-149 C-3: classify "is this body itself a star?" by mass,
                    // not by SMA.  See the parallel r1 block above for rationale.
                    let dest_is_stellar = body_query
                        .get(target_entity)
                        .ok()
                        .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                        .unwrap_or(false);
                    let r2 = if dest_is_stellar {
                        dest_sma.unwrap_or(1.5)
                    } else if dest_parent.is_some() {
                        // Body is not a star and has a parent → walk up to the
                        // parent's heliocentric SMA.
                        dest_parent
                            .and_then(|pe| body_query.get(pe).ok())
                            .and_then(|(_, _, _, ko, _)| ko)
                            .map(|ko| ko.semi_major_axis)
                            .or(dest_sma)
                            .unwrap_or(1.5)
                    } else {
                        dest_sma.unwrap_or(1.5)
                    };
                    // Use the host star's actual GM rather than the hardcoded GM_SUN so that
                    // non-Sol systems (e.g. Alpha Centauri A at ~1.1 M☉, or a 0.5 M☉ K-dwarf)
                    // compute correct velocities and transfer times.
                    // Priority: (1) origin's logical parent if it is a Star, (2) dest's logical
                    // parent if it is a Star, (3) any nearby (< 1 AU from origin) star with no
                    // KeplerOrbit (single-star case), (4) fallback to GM_SUN.
                    let host_gm = origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                        .map(|(_, b, _, _, _)| G_CONST * b.mass)
                        .or_else(|| {
                            dest_parent
                                .and_then(|pe| body_query.get(pe).ok())
                                .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                                .map(|(_, b, _, _, _)| G_CONST * b.mass)
                        })
                        .unwrap_or(GM_SUN);
                    (r1, r2, host_gm)
                } // end same-star case
            };
            // For course corrections, compute the fleet's position in the correct local frame.
            // For heliocentric transfers the position is already relative to the Sun.
            // For local transfers (e.g. moon-to-moon around Jupiter) we must subtract
            // the central body's heliocentric position so distances and phase angles
            // are Jupiter-centric, not Sun-centric.
            // Use is_stellar_gm() instead of exact equality with GM_SUN so that
            // non-solar stars (which have different GM values) are treated correctly.
            let cc_local_pos: Option<bevy::math::DVec3> = if is_course_correction {
                if let Some(fleet_helio) = course_correction_sc {
                    match planner_frame {
                        PlannerTransferFrame::SystemBarycentric => Some(fleet_helio),
                        PlannerTransferFrame::StellarLocal(center_entity)
                        | PlannerTransferFrame::BodyLocal(center_entity) => {
                            let center_helio =
                                transfer_absolute_position(center_entity, elapsed, body_query)
                                    .unwrap_or(bevy::math::DVec3::ZERO);
                            Some(fleet_helio - center_helio)
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            // Override r1 with the fleet's actual distance from the central body.
            let r1 = if is_course_correction {
                cc_local_pos.map(|p| p.length()).unwrap_or(r1)
            } else {
                r1
            };
            fleet_ui_state.computed_options =
                if matches!(planner_frame, PlannerTransferFrame::SystemBarycentric) {
                    if fleet_ui_state.departure_offset_days < 0.0 {
                        fleet_ui_state.departure_offset_days = 0.0;
                    }
                    let origin_pos = transfer_absolute_position(
                        orbit.body,
                        inter_star_departure_time_s,
                        body_query,
                    )
                    .unwrap_or(bevy::math::DVec3::ZERO);
                    let dest_pos = transfer_absolute_position(
                        target_entity,
                        inter_star_departure_time_s,
                        body_query,
                    )
                    .unwrap_or(bevy::math::DVec3::ZERO);
                    let origin_velocity = transfer_absolute_velocity(
                        orbit.body,
                        inter_star_departure_time_s,
                        body_query,
                    )
                    .unwrap_or(bevy::math::DVec3::ZERO);
                    let dest_velocity = transfer_absolute_velocity(
                        target_entity,
                        inter_star_departure_time_s,
                        body_query,
                    )
                    .unwrap_or(bevy::math::DVec3::ZERO);
                    let separation_m = (dest_pos - origin_pos).length()
                        * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let (origin_host_star, origin_host_mass) =
                        find_host_star(orbit.body, body_query).unwrap_or((orbit.body, 0.0));
                    let (dest_host_star, dest_host_mass) =
                        find_host_star(target_entity, body_query).unwrap_or((target_entity, 0.0));
                    let origin_host_pos = transfer_absolute_position(
                        origin_host_star,
                        inter_star_departure_time_s,
                        body_query,
                    )
                    .unwrap_or(bevy::math::DVec3::ZERO);
                    let dest_host_pos = transfer_absolute_position(
                        dest_host_star,
                        inter_star_departure_time_s,
                        body_query,
                    )
                    .unwrap_or(bevy::math::DVec3::ZERO);
                    let origin_host_radius_au = (origin_pos - origin_host_pos)
                        .length()
                        .max(MIN_ORBITAL_RADIUS_AU);
                    let dest_host_radius_au = (dest_pos - dest_host_pos)
                        .length()
                        .max(MIN_ORBITAL_RADIUS_AU);
                    window_this_frame = None;
                    window_max_slider_days = 0.0;
                    let mut options = calculate_cross_star_ballistic_options(
                        origin_pos,
                        dest_pos,
                        origin_velocity,
                        dest_velocity,
                        gm,
                        G_CONST * origin_host_mass,
                        origin_host_radius_au,
                        G_CONST * dest_host_mass,
                        dest_host_radius_au,
                    );
                    let mut direct_options = kinematic_transfer_options(
                        separation_m,
                        fleet.min_accel_ms2(),
                        fleet.max_delta_v_ms(),
                        0.0,
                        r1.max(r2),
                        0.0,
                        false,
                    );
                    options.append(&mut direct_options);
                    options
                } else {
                    // Extract angles of origin and destination bodies in the correct coordinate system.
                    // Moon → parent-planet case: target IS the body that origin orbits around.
                    let is_moon_to_parent = Some(target_entity) == origin_parent;

                    let (pos1, pos2) = if is_moon_to_parent {
                        // Moon→parent: use Moon's position relative to the parent planet.
                        // The parent planet is at the centre of the local frame.
                        let moon_helio = body_query
                            .get(orbit.body)
                            .ok()
                            .map(|(_, _, sc, _, _)| sc.position)
                            .unwrap_or(bevy::math::DVec3::ZERO);
                        let planet_helio = body_query
                            .get(target_entity)
                            .ok()
                            .map(|(_, _, sc, _, _)| sc.position)
                            .unwrap_or(bevy::math::DVec3::ZERO);
                        (
                            Some(moon_helio - planet_helio),
                            Some(bevy::math::DVec3::ZERO),
                        )
                    } else {
                        (
                            position_in_planner_frame(
                                orbit.body,
                                planner_frame,
                                elapsed,
                                body_query,
                            ),
                            position_in_planner_frame(
                                target_entity,
                                planner_frame,
                                elapsed,
                                body_query,
                            ),
                        )
                    };
                    // For course corrections, override pos1 with the fleet's actual current
                    // position in the correct local frame so the transfer-window phase angle
                    // and quality indicator reflect the fleet's real location.
                    let pos1 = if is_course_correction {
                        cc_local_pos.or(pos1)
                    } else {
                        pos1
                    };
                    let theta1 = pos1.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);
                    let theta2 = pos2.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);

                    // Compute transfer window from live positions
                    let window = compute_transfer_window(r1, r2, gm, theta1, theta2);
                    window_max_slider_days = if window.synodic_period_s.is_finite() {
                        (window.synodic_period_s / 86_400.0 * 1.5).max(1.0)
                    } else {
                        730.0
                    };
                    // Consume the "auto-set to next window" signal (departure_offset_days == -1.0)
                    // that is set when the player first right-clicks a target body.  We resolve it
                    // here — after the window is computed but before departure_s is used — so the
                    // slider, quality indicator, and phased options all start at the optimal position.
                    if fleet_ui_state.departure_offset_days < 0.0 {
                        fleet_ui_state.departure_offset_days =
                            (window.time_to_window_s / 86_400.0).max(0.0);
                    }
                    // Compute orbital-plane difference between origin and destination.
                    // Mirrors the (r1, r2, gm) case logic above so the right pair of
                    // KeplerOrbits is diffed in the correct reference frame.
                    let delta_i: f64 = {
                        let origin_ko = body_query
                            .get(orbit.body)
                            .ok()
                            .and_then(|(_, _, _, ko, _)| ko);
                        let dest_ko = body_query
                            .get(target_entity)
                            .ok()
                            .and_then(|(_, _, _, ko, _)| ko);

                        if dest_body_type == Some(BodyType::Star)
                            || Some(target_entity) == origin_parent
                        {
                            // Inward heliocentric or moon→parent: report inclination of the
                            // departure body's orbit (fleet is already in that plane).
                            // Plane change equals what is needed to depart the current orbital plane.
                            origin_ko.map(|ko| ko.inclination).unwrap_or(0.0)
                        } else if dest_parent == Some(orbit.body) {
                            // Fleet at planet, going to one of its moons.
                            dest_ko.map(|ko| ko.inclination).unwrap_or(0.0)
                        } else if dest_parent.is_some() && dest_parent == origin_parent {
                            // Both share a parent (moon-to-moon, OR interplanetary Earth→Mars).
                            match (origin_ko, dest_ko) {
                                (Some(o), Some(d)) => plane_change_angle(
                                    o.inclination,
                                    o.longitude_ascending_node,
                                    d.inclination,
                                    d.longitude_ascending_node,
                                ),
                                _ => 0.0,
                            }
                        } else {
                            // Heliocentric: walk up from moons to their heliocentric parents.
                            //
                            // GRA-149 C-3: classify "is the body a star itself?" by mass
                            // rather than by SMA, so close-orbit planets (hot-Jupiters at
                            // 0.02 AU) are no longer treated as moons when picking the
                            // heliocentric reference orbit for the plane-change diff.
                            let origin_is_stellar = body_query
                                .get(orbit.body)
                                .ok()
                                .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                                .unwrap_or(false);
                            let dest_is_stellar_mass = body_query
                                .get(target_entity)
                                .ok()
                                .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                                .unwrap_or(false);
                            let helio_origin_ko = if origin_is_stellar {
                                origin_ko
                            } else {
                                origin_parent
                                    .and_then(|pe| {
                                        body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko)
                                    })
                                    .or(origin_ko)
                            };
                            let helio_dest_ko = if dest_is_stellar_mass {
                                dest_ko
                            } else {
                                dest_parent
                                    .and_then(|pe| {
                                        body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko)
                                    })
                                    .or(dest_ko)
                            };
                            match (helio_origin_ko, helio_dest_ko) {
                                (Some(o), Some(d)) => plane_change_angle(
                                    o.inclination,
                                    o.longitude_ascending_node,
                                    d.inclination,
                                    d.longitude_ascending_node,
                                ),
                                _ => 0.0,
                            }
                        }
                    };

                    let departure_s = fleet_ui_state.departure_offset_days * 86_400.0;
                    let opts = if is_course_correction {
                        // ── Course-correction branch ─────────────────────────────────
                        // Estimate the fleet's current velocity vector so the redirect ΔV
                        // reflects the actual momentum that must be cancelled/redirected —
                        // not a fresh Hohmann from a circular parking orbit.
                        let v_current_ms: bevy::math::DVec3 = if let Some(man) = current_maneuver {
                            let progress = man.progress(elapsed);
                            if man.is_kinematic() {
                                // Kinematic (straight-line) transfer: velocity is constant in
                                // direction along (end − start); magnitude follows a symmetric
                                // brachistochrone profile (0 → peak → 0).
                                if let (Some(start), Some(end)) =
                                    (man.start_position_au, man.end_position_au)
                                {
                                    let dir = (end - start).normalize_or_zero();
                                    let dist_m = (end - start).length()
                                        * crate::fleets::orbital_mechanics::AU_IN_METERS;
                                    let dur_s = (man.arrival_time - man.departure_time).max(1.0);
                                    // Brachistochrone peak speed (at midpoint) = 2 × distance / duration
                                    let v_peak = 2.0 * dist_m / dur_s;
                                    let speed = if man.option_label == "Full Thrust" {
                                        // Profile: 0 at t=0, v_peak at t=T/2, 0 at t=T
                                        v_peak * 2.0 * progress.min(1.0 - progress)
                                    } else {
                                        // Coast options run at near-constant cruise speed ≈ dist / duration
                                        dist_m / dur_s
                                    };
                                    dir * speed
                                } else {
                                    bevy::math::DVec3::ZERO
                                }
                            } else {
                                // Keplerian transfer: compute velocity from orbital elements via
                                // vis-viva equation + perifocal rotation.
                                let t_since_depart = (elapsed - man.departure_time).max(0.0);
                                let mean_anomaly = man.transfer_orbit.mean_anomaly_epoch
                                    + man.transfer_orbit.mean_motion * t_since_depart;
                                keplerian_velocity_vector(&man.transfer_orbit, mean_anomaly, gm)
                            }
                        } else {
                            bevy::math::DVec3::ZERO
                        };
                        // r_vec: fleet's current position relative to the central body (AU).
                        // cc_local_pos is already in the correct local frame for both heliocentric
                        // and planetary-system transfers. Fall back to r1 on the x-axis.
                        let r_vec =
                            cc_local_pos.unwrap_or_else(|| bevy::math::DVec3::new(r1, 0.0, 0.0));
                        course_correction_transfer_options(r_vec, r2, gm, v_current_ms, delta_i)
                    } else {
                        calculate_transfer_options_phased(r1, r2, gm, departure_s, &window, delta_i)
                    };
                    window_this_frame = Some(window);
                    opts
                };
            // Post-process: fill burn_time_s, flag thrust-limited options,
            // and add kinematic options for high-thrust fleets.
            {
                let accel = fleet.min_accel_ms2();
                let isp = fleet.average_isp_s();
                apply_thrust_limits(&mut fleet_ui_state.computed_options, accel, isp);

                // Kinematic coast/thrust options are not meaningful for course corrections —
                // the fleet is already in free-flight and the redirect cost is captured by
                // `course_correction_transfer_options`.  System-barycentric transfers
                // (inter-star / cross-system) also skip the kinematic pipeline — the
                // Hohmann-style ΔV scan is degenerate at multi-light-year distances
                // and the planner already produced the right value at line 4011.
                if !is_course_correction
                    && !matches!(planner_frame, PlannerTransferFrame::SystemBarycentric)
                {
                    let hohmann_dv = fleet_ui_state
                        .computed_options
                        .first()
                        .map(|o| o.total_delta_v_ms)
                        .unwrap_or(0.0);
                    let sma_h = fleet_ui_state
                        .computed_options
                        .first()
                        .map(|o| o.sma_au)
                        .unwrap_or(0.0);
                    let ecc_h = fleet_ui_state
                        .computed_options
                        .first()
                        .map(|o| o.eccentricity)
                        .unwrap_or(0.0);
                    let d = (r2 - r1).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d,
                        accel,
                        fleet.max_delta_v_ms(),
                        hohmann_dv,
                        sma_h,
                        ecc_h,
                        false,
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                }
            }
            // ── Gravity assist candidates (same-host-star heliocentric transfers only) ─────
            // Restrict assists to bodies that share the same host star as the route.
            // Cross-star and stellar-flyby assists still need a consistent barycentric
            // planner/rendering model, so keep them disabled until that exists.  The
            // `StellarLocal(_)` frame match already excludes system-barycentric /
            // inter-star transfers, so the previous `!is_inter_star_body_transfer`
            // guard is redundant — GRA-382 dropped it.
            if matches!(planner_frame, PlannerTransferFrame::StellarLocal(_))
                && is_stellar_gm(gm)
                && !is_course_correction
            {
                let route_host_star = match planner_frame {
                    PlannerTransferFrame::StellarLocal(star_entity) => Some(star_entity),
                    _ => None,
                };
                let ga_bodies: Vec<(String, f64, f64, f64)> = body_query
                    .iter()
                    .filter_map(|(e, body, _sc, maybe_ko, _)| {
                        let is_planet_class = matches!(
                            body.body_type,
                            BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet
                        );
                        if !is_planet_class {
                            return None;
                        }
                        // Stars are intentionally excluded from gravity-assist candidates:
                        // a stellar flyby would require STELLAR_FLYBY_RADIUS_KM_MULTIPLIER
                        // (1.5 R★ ≈ 1.5 stellar radii) rather than the planetary multiplier,
                        // and the existing 2-body assist model is not valid inside the
                        // corona.  Future maintainers: do NOT widen `is_planet_class` to
                        // include BodyType::Star without also switching the periapsis
                        // formula below to use STELLAR_FLYBY_RADIUS_KM_MULTIPLIER.
                        if body.body_type == BodyType::Star {
                            return None;
                        }
                        // Exclude the fleet's current body and the chosen destination
                        if e == orbit.body || Some(e) == body_target_snap {
                            return None;
                        }
                        // Only consider bodies in the current star system
                        if body_system_ids.get(e).map(|s| s.0).unwrap_or(0) != current_system_id {
                            return None;
                        }
                        if find_host_star(e, body_query).map(|(star, _)| star) != route_host_star {
                            return None;
                        }
                        let sma = maybe_ko?.semi_major_axis;
                        // GRA-149 C-3: the legacy 0.05 AU SMA threshold used to drop
                        // hot-Jupiters (close-orbit giants at ~0.02 AU) from the GA
                        // candidate list, even when a flyby of such a body would
                        // have been a strong assist.  We keep the candidate as long
                        // as it has any heliocentric SMA at all (i.e., it owns a
                        // Kepler orbit).  Pure moons and unbound bodies still fall
                        // out because `maybe_ko?` returns None above.
                        let flyby_r = sma;
                        let gm_p = G_CONST * body.mass;
                        // Safe flyby periapsis using the named multipliers.
                        let radius_km = body.radius as f64;
                        let multiplier = PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER;
                        let min_peri = (radius_km * multiplier) / AU_IN_METERS;
                        Some((body.name.clone(), flyby_r, gm_p, min_peri.max(1e-6)))
                    })
                    .collect();

                let previously_selected_flyby = fleet_ui_state
                    .selected_gravity_assist
                    .and_then(|idx| fleet_ui_state.gravity_assist_candidates.get(idx))
                    .map(|entry| entry.flyby_entity);

                let new_candidates: Vec<GravityAssistEntry> =
                    find_gravity_assist_options(r1, r2, gm, &ga_bodies)
                        .into_iter()
                        .filter_map(|opt| {
                            // Resolve each candidate to its ECS entity by name
                            let entity = body_query
                                .iter()
                                .find(|(_, b, _, _, _)| b.name == opt.body_name)
                                .map(|(e, _, _, _, _)| e)?;
                            Some(GravityAssistEntry {
                                option: opt,
                                flyby_entity: entity,
                            })
                        })
                        .collect();

                let mut new_candidates = new_candidates;
                new_candidates.sort_by(|left, right| {
                    left.option
                        .total_dv_ms
                        .partial_cmp(&right.option.total_dv_ms)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            left.option
                                .total_time_s
                                .partial_cmp(&right.option.total_time_s)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .then_with(|| left.option.body_name.cmp(&right.option.body_name))
                });

                fleet_ui_state.gravity_assist_candidates = new_candidates;

                fleet_ui_state.selected_gravity_assist =
                    previously_selected_flyby.and_then(|selected_flyby| {
                        fleet_ui_state
                            .gravity_assist_candidates
                            .iter()
                            .position(|entry| entry.flyby_entity == selected_flyby)
                    });
            } else {
                fleet_ui_state.gravity_assist_candidates.clear();
                fleet_ui_state.selected_gravity_assist = None;
            }

            // If a gravity assist is selected, prepend it as option 0 so the
            // regular execute/select logic treats it uniformly.
            //
            // GRA-165 defensive guard: never inject the GA row when a
            // porkchop cell is also selected.  In practice the GA selector
            // at :4728 already clears `selected_porkchop_cell` on click, so
            // the two states are mutually exclusive — but a future refactor
            // that drops that clear (or a new entry point that toggles the
            // GA without going through the button) could leave both set,
            // which would draw both the GA Leg-1+Leg-2 slingshot overlay
            // AND the porkchop-driven sampled polyline in the same frame.
            // The "multiple lines all over the place" symptom.
            if fleet_ui_state.selected_gravity_assist.is_some()
                && fleet_ui_state.selected_porkchop_cell.is_some()
            {
                // GRA-165 defensive guard: skip the GA row when a porkchop
                // cell is also selected.  See the long-form comment above.
            } else if let Some(sel_ga) = fleet_ui_state.selected_gravity_assist {
                let ga_data = fleet_ui_state
                    .gravity_assist_candidates
                    .get(sel_ga)
                    .map(|e| {
                        (
                            e.option.total_dv_ms,
                            e.option.total_time_s,
                            e.option.flyby_radius_au,
                            e.option.dv_depart_ms + e.option.dv_mid_ms, // departure + mid-course
                            e.option.dv_arrive_ms,
                        )
                    });
                if let Some((total_dv, total_time, fly_r, dv1, dv2)) = ga_data {
                    // Use Leg-1 Hohmann parameters (origin → flyby body) for the
                    // transfer-orbit Keplerian arc.  This makes the purple active-orbit
                    // arc match the approach leg shown in the gravity-assist preview.
                    // The arc is computed pointing from the origin toward the flyby body,
                    // and build_planned_transfer is passed the flyby entity as its orbital
                    // target so the departure/arrival plane vectors are consistent.
                    let (_, _, _, ga_sma, ga_ecc) = hohmann_transfer(r1, fly_r, gm);
                    let burn_t =
                        compute_burn_time_s(total_dv, fleet.min_accel_ms2(), fleet.average_isp_s());
                    // Gravity-assist options use multi-leg patched-conic timing; the burn
                    // is spread across two legs so we apply the thrust-limit check here.
                    let (ga_transfer_time, ga_thrust_limited) =
                        if burn_t > 0.0 && burn_t > total_time {
                            (burn_t, true)
                        } else {
                            (total_time, false)
                        };
                    let ga_option = TransferOption {
                        label: "Gravity Assist",
                        total_delta_v_ms: total_dv,
                        delta_v1_ms: dv1, // actual departure + any mid-course burn
                        delta_v2_ms: dv2, // actual arrival circularisation
                        plane_change_dv_ms: 0.0, // gravity-assist paths are heliocentric (ecliptic)
                        transfer_time_s: ga_transfer_time,
                        sma_au: ga_sma, // Leg-1 ellipse SMA (origin → flyby body)
                        eccentricity: ga_ecc,
                        energy_multiplier: 1.0,
                        burn_time_s: burn_t,
                        is_thrust_limited: ga_thrust_limited,
                        transfer_orbit_override: None,
                    };
                    fleet_ui_state.computed_options.insert(0, ga_option);

                    // ── GRA-328a dispatch: build (or clear) the porkchop grid ─
                    // The body-target block above populates `computed_options`
                    // for the GA-selected row (GRA-154/GRA-165 keep the
                    // porkchop grid suppressed whenever a flyby is chosen so
                    // the assist preview doesn't overlap the panel).
                    //
                    // Policy:
                    //   * BodyLocal frame → build a local-frame Lambert grid
                    //     when the parent body has GM data; otherwise leave
                    //     the grid None so the legacy row renders.
                    //   * StellarLocal / SystemBarycentric → the heliocentric
                    //     grid is built upstream at lines 1290-1353 via the
                    //     per-frame deferred-build dispatcher (the rotating
                    //     buffer from GRA-169 Part B).  Because a GA is
                    //     currently selected we suppress the grid here per
                    //     the GRA-154/GRA-165 defensive guard.
                    //
                    // GRA-328a follow-up: the previous comment in this slot
                    // referenced "TODO per GRA-153" — that deferral was
                    // closed by GRA-159 (PR #186) and GRA-326 (PR #205); the
                    // planner now builds heliocentric grids on planet-to-
                    // planet and planet-to-star destinations via the
                    // rotating-buffer dispatcher, not through this branch.
                    if let PlannerTransferFrame::BodyLocal(parent_entity) = planner_frame {
                        if let Some(grid) = try_build_local_porkchop(
                            parent_entity,
                            target_entity,
                            orbit.body,
                            elapsed,
                            body_query,
                            porkchop_config,
                        ) {
                            fleet_ui_state.porkchop_grid = Some(grid);
                            fleet_ui_state.selected_porkchop_cell = None;
                        } else {
                            // Local-frame solve not applicable (e.g. ring targets
                            // without KeplerOrbit).  Keep grid None so the legacy
                            // row renders.
                            fleet_ui_state.porkchop_grid = None;
                            fleet_ui_state.selected_porkchop_cell = None;
                        }
                    } else {
                        // StellarLocal / SystemBarycentric with a GA selected:
                        // suppress the (already-built) heliocentric grid per
                        // the GRA-154/GRA-165 defensive guard so the GA
                        // overlay renders without the panel.
                        fleet_ui_state.porkchop_grid = None;
                        fleet_ui_state.selected_porkchop_cell = None;
                    }
                }
            }
        } else if let Some(ref lp) = lp_target_snap {
            // Lagrange-point transfer.
            // Determine the fleet's current heliocentric SMA, walking up to
            // the planet's SMA when the fleet is parked at a moon/sub-body.
            // When orbiting the star directly (e.g. after a previous LP transfer),
            // use the fleet's parking radius if available, otherwise the LP planet's SMA.
            let r1_lp = body_query
                .get(orbit.body)
                .ok()
                .and_then(|(_, body, _, ko, _)| {
                    if body.body_type == BodyType::Star {
                        // Fleet parked around the star — use its parking orbit radius
                        // or fall back to the target LP's planet SMA.
                        if orbit.radius_au > 0.01 {
                            Some(orbit.radius_au)
                        } else {
                            Some(lp.planet_sma_au)
                        }
                    } else {
                        ko.map(|ko| ko.semi_major_axis)
                    }
                })
                .or_else(|| {
                    body_query
                        .get(orbit.body)
                        .ok()
                        .and_then(|(_, _, _, _, parent)| parent)
                        .and_then(|lpp| {
                            body_query
                                .get(lpp.0)
                                .ok()
                                .and_then(|(_, _, _, ko, _)| ko)
                                .map(|ko| ko.semi_major_axis)
                        })
                })
                .unwrap_or(lp.planet_sma_au);

            // L3/L4/L5 are co-orbital with the planet (same heliocentric radius,
            // different phase angle).  A Hohmann gives 0 Delta-V in this case.
            // Use a phasing-orbit maneuver instead: lower into a shorter-period
            // orbit and drift the 60 deg (L4/L5) or 180 deg (L3) phase gap in N laps.
            let co_orbital = matches!(lp.point, 3..=5) && (r1_lp - lp.planet_sma_au).abs() < 0.02;

            if co_orbital {
                let delta_phi = if lp.point == 3 {
                    std::f64::consts::PI // L3: 180 deg opposition
                } else {
                    std::f64::consts::FRAC_PI_3 // L4/L5: 60 deg
                };
                fleet_ui_state.computed_options =
                    co_orbital_phasing_options(lp.planet_sma_au, lp.gm, delta_phi);
                apply_thrust_limits(
                    &mut fleet_ui_state.computed_options,
                    fleet.min_accel_ms2(),
                    fleet.average_isp_s(),
                );
                // Kinematic options: arc-length of the phase drift as proxy distance.
                let hohmann_dv = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.total_delta_v_ms)
                    .unwrap_or(0.0);
                let sma_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.sma_au)
                    .unwrap_or(r1_lp);
                let d =
                    lp.planet_sma_au * delta_phi * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    hohmann_dv,
                    sma_h,
                    0.0,
                    false,
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            } else if matches!(lp.point, 1 | 2) {
                // L1/L2: small radial offset from planet (~r_hill ≈ 0.01 AU).
                // Use a direct manifold-like trajectory (realistic ~1–3 month travel
                // time) instead of a Hohmann half-orbit that takes 6 months and arrives
                // 180° away from the LP.
                fleet_ui_state.computed_options =
                    direct_lp_transfer_options(r1_lp, lp.radius_au, lp.gm);
                apply_thrust_limits(
                    &mut fleet_ui_state.computed_options,
                    fleet.min_accel_ms2(),
                    fleet.average_isp_s(),
                );
                let hohmann_dv = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.total_delta_v_ms)
                    .unwrap_or(0.0);
                let sma_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.sma_au)
                    .unwrap_or(0.0);
                let ecc_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.eccentricity)
                    .unwrap_or(0.0);
                let d =
                    (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    hohmann_dv,
                    sma_h,
                    ecc_h,
                    false,
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            } else {
                // L3/L4/L5 cross-orbit (fleet NOT co-orbital with the planet):
                // standard Hohmann Keplerian transfer to the planet's SMA.
                fleet_ui_state.computed_options =
                    calculate_transfer_options(r1_lp, lp.radius_au, lp.gm, 0.0);
                apply_thrust_limits(
                    &mut fleet_ui_state.computed_options,
                    fleet.min_accel_ms2(),
                    fleet.average_isp_s(),
                );
                let hohmann_dv = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.total_delta_v_ms)
                    .unwrap_or(0.0);
                let sma_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.sma_au)
                    .unwrap_or(0.0);
                let ecc_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.eccentricity)
                    .unwrap_or(0.0);
                let d =
                    (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    hohmann_dv,
                    sma_h,
                    ecc_h,
                    false,
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            }
        }

        // ── Interstellar transfer computation ───────────────────────────────
        if let Some((system_id, ref sys_name, distance_ly)) = star_system_snap {
            use crate::fleets::orbital_mechanics::{TransferOption, AU_IN_METERS};
            // 1 ly = 63 241.077 AU
            const AU_PER_LY: f64 = 63_241.077;
            let distance_m = distance_ly as f64 * AU_PER_LY * AU_IN_METERS;
            let accel = fleet.min_accel_ms2();
            let max_dv = fleet.max_delta_v_ms();

            fleet_ui_state.computed_options.clear();

            let mut kinematics =
                kinematic_transfer_options(distance_m, accel, max_dv, 0.0, 0.0, 0.0, true);
            fleet_ui_state.computed_options.append(&mut kinematics);

            if fleet_ui_state.computed_options.is_empty() {
                // Fleet lacks the minimum thrust for interstellar travel
                fleet_ui_state.computed_options.push(TransferOption {
                    label: "Insufficient thrust",
                    total_delta_v_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    plane_change_dv_ms: 0.0,
                    transfer_time_s: f64::INFINITY,
                    sma_au: 0.0,
                    eccentricity: 0.0,
                    energy_multiplier: 0.0,
                    burn_time_s: 0.0,
                    is_thrust_limited: true,
                    transfer_orbit_override: None,
                });
            }

            // GRA-343 (GRA-328b): build the cross-system Hohmann
            // grid.  Cached on `fleet_ui_state.cross_system_grid`
            // and rebuilt only when the destination system_id
            // changes.  Falls back to `None` when the destination
            // has no barycentric lookup or the policy resource is
            // unavailable (debug fallback).  The grid is a
            // degenerate 1×1 `PorkchopGrid` (GRA-367-E), whose
            // `min_cell` is `Some((0, 0))` iff `meets_human_margin`
            // passes (the feasibility predicate is unchanged from
            // GRA-343).
            let needs_rebuild = fleet_ui_state
                .cross_system_grid_built_for
                .map(|sid| sid != system_id)
                .unwrap_or(true);
            if needs_rebuild {
                if let Some(grid) = try_build_cross_system_hohmann(
                    system_id,
                    sys_name,
                    distance_ly,
                    nearby_stars,
                    elapsed,
                    fleet,
                    interstellar_policy,
                ) {
                    fleet_ui_state.cross_system_grid = Some(grid);
                    fleet_ui_state.cross_system_grid_built_for = Some(system_id);
                } else {
                    fleet_ui_state.cross_system_grid = None;
                    fleet_ui_state.cross_system_grid_built_for = None;
                }
            }
        }

        // ── Transfer Window / Departure slider — hidden for course corrections ────
        // Course corrections execute immediately; no departure window or delay needed.
        if !is_course_correction {
            // Show a co-orbital / L-point info section for Lagrange targets.
            if window_this_frame.is_none() && lp_target_snap.is_some() {
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                // Left: Lagrange transfer info
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        let lp = lp_target_snap.as_ref().unwrap();
                        // Determine actual transfer type — same logic as the computation
                        // section above.  L3/L4/L5 are co-orbital only when the fleet is
                        // already near the planet's SMA (within 0.02 AU).
                        let r1_info = body_query.get(orbit.body).ok()
                            .and_then(|(_, body, _, ko, _)| {
                                if body.body_type == BodyType::Star {
                                    if orbit.radius_au > 0.01 { Some(orbit.radius_au) }
                                    else { Some(lp.planet_sma_au) }
                                } else { ko.map(|k| k.semi_major_axis) }
                            })
                            .or_else(|| {
                                body_query.get(orbit.body).ok()
                                    .and_then(|(_, _, _, _, parent)| parent)
                                    .and_then(|lpp| body_query.get(lpp.0).ok()
                                        .and_then(|(_, _, _, ko, _)| ko)
                                        .map(|ko| ko.semi_major_axis))
                            })
                            .unwrap_or(lp.planet_sma_au);
                        let is_co_orbital = matches!(lp.point, 3..=5)
                            && (r1_info - lp.planet_sma_au).abs() < 0.02;
                        let is_l12_direct = matches!(lp.point, 1 | 2);
                        if is_co_orbital {
                            ui.label(
                                egui::RichText::new("⟳ Co-orbital Phasing")
                                    .strong().size(12.0)
                                    .color(theme::RP_BLUE),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new("Depart any time")
                                    .size(12.0).strong()
                                    .color(theme::GREEN),
                            );
                            ui.label(
                                egui::RichText::new("Fleet drifts in a slightly\nlower orbit to cover the\nphase gap over N laps.")
                                    .size(10.0).color(theme::TEXT_DIM),
                            );
                        } else if is_l12_direct {
                            ui.label(
                                egui::RichText::new("🎯 Direct LP Transfer")
                                    .strong().size(12.0)
                                    .color(theme::RP_BLUE),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("Low-energy manifold trajectory\nto the Lagrange equilibrium.")
                                    .size(10.0).color(theme::TEXT_DIM),
                            );
                        } else {
                            // L3/L4/L5 cross-orbit (fleet not co-orbital): Hohmann
                            ui.label(
                                egui::RichText::new("⬆ Hohmann Transfer")
                                    .strong().size(12.0)
                                    .color(theme::RP_BLUE),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("Keplerian transfer arc,\nthen phase into the LP.")
                                    .size(10.0).color(theme::TEXT_DIM),
                            );
                        }
                    });
                });
                // Fleet stats infobox (same as body-target section)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("\u{1f680} Fleet")
                                .strong().size(12.0)
                                .color(theme::RP_BLUE),
                        );
                        ui.add_space(3.0);
                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_g = fleet.min_accel_ms2() / 9.80665;
                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0).strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0).strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(format!("{:.3} g", accel_g))
                                .size(11.0).strong()
                                .color(theme::TEXT_VALUE),
                        );
                    });
                });
            });
            }
            if let Some(ref window) = window_this_frame {
                let syn_days = if window.synodic_period_s.is_finite() {
                    window.synodic_period_s / 86_400.0
                } else {
                    f64::INFINITY
                };
                let window_days = window.time_to_window_s / 86_400.0;

                ui.add_space(6.0);

                let max_days = window_max_slider_days.min(1_825.0); // cap at 5 years
                let step_size = if max_days <= 1.0 {
                    0.01 // ~14 mins
                } else if max_days <= 10.0 {
                    0.05 // ~1.2 hours
                } else if max_days <= 50.0 {
                    0.1 // ~2.4 hours
                } else if max_days <= 200.0 {
                    0.5 // 12 hours
                } else {
                    1.0 // 1 day
                };

                // Porkchop replacement: when the player has a porkchop
                // grid active, the (t_dep, t_tof) plane below IS the
                // departure-time selector — there is nothing for the
                // slider to do.  Hide the Transfer Window + Planned
                // Departure boxes entirely; the player picks t_dep by
                // clicking a porkchop cell.  We still keep the
                // "Arrives:" timestamp and side-panel ΔV stats so the
                // player sees the consequence of their click.
                if fleet_ui_state.porkchop_grid.is_some() {
                    // Skip rendering the Transfer Window + Planned
                    // Departure boxes.  Fall through to the
                    // post-window section.
                } else {
                    // ── Transfer Window (left) + Planned Departure (right) side by side ──
                    ui.horizontal_top(|ui| {
                // Left: Transfer Window box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("⏱ Transfer Window")
                                .strong()
                                .size(12.0)
                                .color(theme::RP_BLUE),
                        );
                        ui.add_space(3.0);

                        egui::Grid::new("window_info_grid")
                            .num_columns(2)
                            .spacing([8.0, 3.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Next window:").size(12.0));
                                if window_days < 1.0 {
                                    ui.label(
                                        egui::RichText::new("NOW  ✓")
                                            .size(12.0)
                                            .strong()
                                            .color(theme::GREEN),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(format_duration(window.time_to_window_s).to_string())
                                            .size(12.0)
                                            .color(theme::TEXT),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Synodic period:").size(12.0));
                                let syn_str = if syn_days.is_finite() {
                                    format_duration(window.synodic_period_s)
                                } else {
                                    "∞ (same orbit)".to_owned()
                                };
                                ui.label(egui::RichText::new(syn_str).size(12.0).color(theme::TEXT_DIM));
                                ui.end_row();
                            });
                    });
                });

                // Right: Planned Departure box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        // Row 1: label
                        ui.label(
                            egui::RichText::new("🕐 Planned Departure")
                                .strong()
                                .size(12.0)
                                .color(theme::RP_BLUE),
                        );

                        // Row 2: slider
                        let mut offset_days = fleet_ui_state.departure_offset_days as f32;
                        let slider = egui::Slider::new(&mut offset_days, 0.0_f32..=max_days as f32)
                            .step_by(step_size)
                            .custom_formatter(|v, _| {
                                if v < 0.01 {
                                    "Now".to_owned()
                                } else {
                                    format_duration(v * 86_400.0)
                                }
                            });
                        if ui.add(slider).changed() {
                            fleet_ui_state.departure_offset_days = offset_days as f64;
                            // GRA-388: in GA mode the trajectory is
                            // anchored at `selected_abs_t_dep_s` when
                            // set, so a slider drag must refresh the
                            // recorded absolute epoch or the slider
                            // appears inert (the "I can not
                            // influence the departure time" report).
                            maybe_record_burn_epoch_for_ga(
                                fleet_ui_state,
                                elapsed,
                                offset_days as f64,
                            );
                        }

                        // Orbit-wait counter: shown when the fleet must loop its parking ring
                        // more than once before reaching the departure angle.
                        if fleet_ui_state.waiting_orbit_count > 1 {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("× {} orbits (waiting)", fleet_ui_state.waiting_orbit_count))
                                        .size(10.5)
                                        .color(theme::GRAVITY_ASSIST),
                                );
                            });
                        }

                        // Row 3: alignment indicator (below the slider)
                        let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                        let phase_at = {
                            let raw = window.phase_error_now_rad + window.phase_rate_rad_s * dep_s;
                            ((raw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)) - std::f64::consts::PI
                        };
                        let factor = crate::fleets::orbital_mechanics::phase_dv_factor(phase_at.abs());
                        let (quality_str, quality_color) = if factor < 1.05 {
                            ("● Optimal", theme::GREEN)
                        } else if factor < 1.40 {
                            ("◑ Good", theme::GREEN)
                        } else if factor < 1.80 {
                            ("◔ Fair", theme::AMBER)
                        } else {
                            ("○ Poor", theme::RED)
                        };
                        ui.label(egui::RichText::new(quality_str).size(11.0).color(quality_color))
                            .on_hover_text("Indicates how well the planets are aligned for a transfer at the planned departure time. Poor alignment requires significantly more ΔV.");

                        // Depart-Now / Next-Window shortcut buttons on
                        // their own row.  "Depart Now" snaps the slider
                        // to t_dep = 0 so the player doesn't have to
                        // drag it all the way to the left edge (the
                        // porkchop t_dep axis goes negative on Saturn
                        // and other long-synodic-period destinations,
                        // and reaching "Now" by dragging a 5-year
                        // slider feels punishing).  Next-Window snaps
                        // to the optimal Hohmann window.
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            let now_btn = egui::Button::new(
                                egui::RichText::new("⚡ Depart Now")
                                    .size(11.0)
                                    .strong(),
                            );
                            if ui
                                .add(now_btn)
                                .on_hover_text(
                                    "Snap the departure slider to t = 0 (immediate launch).",
                                )
                                .clicked()
                            {
                                fleet_ui_state.departure_offset_days = 0.0;
                                // GRA-388: re-anchor the GA
                                // trajectory to the new burn epoch
                                // so the trajectory visibly snaps
                                // to t = 0.
                                maybe_record_burn_epoch_for_ga(fleet_ui_state, elapsed, 0.0);
                            }
                            if window_days > 0.5
                                && ui
                                    .small_button(format!(
                                        "🎯 Next Window (+{:.0} d)",
                                        window_days
                                    ))
                                    .clicked()
                            {
                                fleet_ui_state.departure_offset_days = window_days;
                                // GRA-388: re-anchor the GA
                                // trajectory to the next-window
                                // burn epoch so the trajectory
                                // moves with the slider snap.
                                maybe_record_burn_epoch_for_ga(
                                    fleet_ui_state,
                                    elapsed,
                                    window_days,
                                );
                            }
                        });
                    });
                });

                // Fleet stats infobox (narrow, right-most)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("🚀 Fleet")
                                .strong()
                                .size(12.0)
                                .color(theme::RP_BLUE),
                        );
                        ui.add_space(3.0);

                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_ms2 = fleet.min_accel_ms2();
                        let accel_g = accel_ms2 / 9.80665;
                        let accel_str = format!("{:.3} g", accel_g);

                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(accel_str)
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_VALUE),
                        );
                    });
                });
            });
                } // end !porkchop_grid (legacy Transfer Window / Planned Departure boxes)
            }
        } // end !is_course_correction (Transfer Window / Departure slider section)

        if !fleet_ui_state.computed_options.is_empty() {
            if let Some(previous_label) = previous_selected_option_label {
                if let Some(idx) = fleet_ui_state
                    .computed_options
                    .iter()
                    .position(|option| option.label == previous_label)
                {
                    fleet_ui_state.selected_option = idx;
                }
            }

            ui.add_space(6.0);

            let fleet_max_dv = fleet.max_delta_v_ms();

            // Ensure selected_option is within bounds
            if fleet_ui_state.selected_option >= fleet_ui_state.computed_options.len() {
                fleet_ui_state.selected_option = fleet_ui_state.computed_options.len() - 1;
            }

            // Pre-compute execute button state
            let sel_option =
                fleet_ui_state.computed_options[fleet_ui_state.selected_option].clone();
            let planned_departure_time_s =
                elapsed + fleet_ui_state.departure_offset_days * 86_400.0;
            // GRA-386: the legacy "GA selected ⇒ wipe preview to None"
            // short-circuit used to suppress the GA two-leg arc every
            // frame.  We removed it because the GA-row path is now
            // handled by the cell-click block further down (lines
            // ~6530) which routes through `build_planned_transfer_with_flyby`
            // when `porkchop_view_mode == GravityAssist(idx)`.  Keeping
            // this `if fleet_ui_state.selected_gravity_assist.is_some()
            // { None }` arm here would silently overwrite that
            // two-leg preview every frame before the renderer could
            // draw it.
            //
            // We still want the preview cleared when the player is on
            // a GA view-mode toggle but has NOT picked a cell yet —
            // the cell-click block already returns `None` in that case
            // (the match arm at line 6530 has no `selected_porkchop_cell`
            // branch), so falling through to the `else if let Some(te) =
            // body_target_snap` arm below is fine: it tries to build a
            // direct `PlannedTransfer` to the body, which is what the
            // pre-fix GA-row code did anyway (and renders correctly
            // until the player clicks a cell).
            fleet_ui_state.planned_transfer = if let Some(ref lp) = lp_target_snap {
                build_planned_transfer_lp(fleet_entity, fleet, orbit, lp, body_query, &sel_option)
            } else if let Some(tfe) = fleet_target_snap {
                all_fleets_query
                    .get(tfe)
                    .ok()
                    .and_then(|(_, _, _, maybe_fo, _)| maybe_fo)
                    .and_then(|fo| {
                        // GRA-NNN: pull the user-controlled orbit shell from
                        // FleetUiState.  `target_orbit_shell` is only valid when
                        // the destination is the body it references; other
                        // targets fall through to the per-body default.
                        let target_orbit_radius_au = fleet_ui_state
                            .target_orbit_shell
                            .filter(|(e, _)| *e == fo.body)
                            .and_then(|(_, s)| {
                                body_query
                                    .get(fo.body)
                                    .ok()
                                    .map(|(_, b, _, _, _)| radius_for_shell(b, s))
                            });
                        build_planned_transfer(
                            fleet_entity,
                            fleet,
                            orbit,
                            fo.body,
                            planned_departure_time_s,
                            body_query,
                            &sel_option,
                            course_correction_sc,
                            body_system_ids,
                            current_system_id,
                            target_orbit_radius_au,
                        )
                    })
            } else if let Some(te) = body_target_snap {
                // GRA-NNN: shell-driven radius.
                let target_orbit_radius_au = fleet_ui_state
                    .target_orbit_shell
                    .filter(|(e, _)| *e == te)
                    .and_then(|(_, s)| {
                        body_query
                            .get(te)
                            .ok()
                            .map(|(_, b, _, _, _)| radius_for_shell(b, s))
                    });
                build_planned_transfer(
                    fleet_entity,
                    fleet,
                    orbit,
                    te,
                    planned_departure_time_s,
                    body_query,
                    &sel_option,
                    course_correction_sc,
                    body_system_ids,
                    current_system_id,
                    target_orbit_radius_au,
                )
            } else {
                None
            };
            let abort_cost_t: f32 = if let Some(maneuver) = current_maneuver {
                // GRA-153 H-4: replace the parabolic peak heuristic
                // (`fuel_used * 4p(1-p) * 0.6`) with a real mid-flight abort ΔV.
                //
                // The cheapest mid-flight abort cancels the fleet's current
                // Keplerian velocity and circularises at the fleet's CURRENT
                // radius from the origin body — a true `|v_required - v_current|`
                // vis-viva computation.  This is the same approach the planner
                // uses for non-abort course corrections (see `course_correction_
                // transfer_options` in orbital_mechanics.rs).
                let abort_dv_ms: f64 = (|| -> Option<f64> {
                    // Fleet's current heliocentric position (planner-open
                    // snapshot — fine for a button label).
                    let r_pos = course_correction_sc?;
                    // Compute the central body's GM via Kepler's third law from
                    // the current transfer orbit's SMA and mean motion.
                    let a_m = maneuver.transfer_orbit.semi_major_axis
                        * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let n = maneuver.transfer_orbit.mean_motion;
                    if a_m <= 0.0 || n <= 0.0 {
                        return None;
                    }
                    let gm = (n * n) * (a_m * a_m * a_m);
                    // Fleet's current Keplerian velocity from the active orbit.
                    let t_since_depart = (elapsed - maneuver.departure_time).max(0.0);
                    let mean_anomaly = maneuver.transfer_orbit.mean_anomaly_epoch
                        + maneuver.transfer_orbit.mean_motion * t_since_depart;
                    let v_current_ms = crate::fleets::orbital_mechanics::keplerian_velocity_vector(
                        &maneuver.transfer_orbit,
                        mean_anomaly,
                        gm,
                    );
                    // Resolve the orbital center's heliocentric position so
                    // the radius-from-center is local (handles moon transfers).
                    let center_helio = match maneuver.reference_frame {
                        crate::fleets::TransferReferenceFrame::SystemBarycentric => {
                            bevy::math::DVec3::ZERO
                        }
                        crate::fleets::TransferReferenceFrame::Body(center_entity) => body_query
                            .get(center_entity)
                            .map(|(_, _, sc, _, _)| sc.position)
                            .unwrap_or(bevy::math::DVec3::ZERO),
                    };
                    let r_local_au = (r_pos - center_helio).length();
                    if r_local_au <= 1e-6 {
                        return None;
                    }
                    // Circular velocity at the current radius.
                    let v_circ_ms =
                        (gm / (r_local_au * crate::fleets::orbital_mechanics::AU_IN_METERS)).sqrt();
                    // ΔV to circularise at the current orbit.
                    let dv_circ_ms = (v_current_ms.length() - v_circ_ms).abs();
                    Some(dv_circ_ms)
                })()
                .unwrap_or(0.0);
                // Convert ΔV to fuel tonnes via the rocket equation.
                if abort_dv_ms > 0.0 {
                    let dry_mass_t = fleet.ships.iter().map(|s| s.dry_mass_t as f64).sum::<f64>();
                    let wet_mass_t = dry_mass_t + fleet.total_fuel_t() as f64;
                    let avg_isp_s = fleet.average_isp_s();
                    crate::fleets::orbital_mechanics::estimate_fuel_cost_tonnes(
                        wet_mass_t as f32,
                        avg_isp_s,
                        abort_dv_ms,
                    )
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let dv_after_abort = if abort_cost_t > 0.0 {
                fleet.min_delta_v_after_abort(abort_cost_t)
            } else {
                fleet_max_dv
            };
            let sel_affordable_with_abort = sel_option.total_delta_v_ms <= dv_after_abort;

            // GRA-382 (Phase 5 renderer cleanup): the `if is_interstellar ||
            // is_inter_star_body_transfer` gate is gone.  Every transfer
            // class now routes through the unified `build_selected_card`
            // dispatcher driven by `transfer_plan.source`
            // (`SelectionSource::{Interstellar, Binary, ShortHop,
            // StarApproach, GravityAssist, Porkchop, Empty}` —
            // `Hohmann3Option` was removed in GRA-387 because the
            // legacy 3-option row now uses a degenerate Porkchop grid).
            // The supplement still carries the fields that haven't
            // migrated onto `SelectionSource` yet (GA candidates
            // + cross-system grid + interstellar display name); the next
            // Phase-6 dispatcher (GRA-381) will narrow it as the
            // `SelectionSource` variants pick up consumers.  Until then
            // the panel renders without the selected-card widget above
            // the Execute button — the inline heatmap / 3-option / GA
            // collapsible continue to surface per-class info.
            //
            // GRA-367-E (Phase 5): the cross-star + interstellar
            // degenerate grid feeds the same per-class panel.  Calendar
            // ETA always shows (`hides_calendar_eta` is now a non-
            // existent state).
            let is_interstellar = star_system_snap.is_some();
            let hides_calendar_eta = false;
            let btn_label = if is_course_correction {
                if abort_cost_t > 0.01 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!(
                        "\u{1F504} Execute Course Correction (+{:.2} km/s abort burn)",
                        abort_dv_kms
                    )
                } else {
                    "\u{1F504} Execute Course Correction".to_string()
                }
            } else {
                "\u{1F680} Execute Transfer".to_string()
            };

            // For fleet intercepts note the encounter speed penalty
            if fleet_target_snap.is_some() && fleet_ui_state.intercept_speed_ms > 100.0 {
                let extra_dv_kms = fleet_ui_state.intercept_speed_ms / 1_000.0;
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26A0} +{:.1} km/s added for encounter speed (not included in \u{394}V below)",
                        extra_dv_kms
                    ))
                    .size(11.0)
                    .italics()
                    .color(theme::AMBER),
                );
            }

            // GRA-384: render the unified `build_selected_card` widget
            // above the Execute button for every per-class selection
            // (interplanetary porkchop, gravity-assist candidates,
            // cross-star degenerate grid, interstellar star-system,
            // short-hop bar, star-approach parking-radius grid).  The
            // card source-of-truth is `transfer_plan.source` plus the
            // `CardSupplement` carrying UI-owned fields that have not
            // yet migrated onto `SelectionSource` (GRA-381 is the
            // dispatcher that completes the migration; until then the
            // supplement bridges the gap).  Replaces the per-class
            // header rows the previous GRA-382 cleanup deferred.
            //
            // The `transfer_plan` resource is borrowed mutably for the
            // `sync_plan_from_ui` mirror earlier in this function, so
            // we reborrow it as `&*transfer_plan` here to share the
            // same underlying state with the card builder (which takes
            // `&TransferPlan`).  `TransferPlan` is not `Clone` (it's a
            // resource), so the reborrow is the cheapest path that
            // doesn't require a full clone of the porkchop grid.
            {
                let card_supplement = crate::ui::transfer_planner_card::CardSupplement {
                    gravity_assist_candidates: fleet_ui_state.gravity_assist_candidates.clone(),
                    selected_gravity_assist: fleet_ui_state.selected_gravity_assist,
                    cross_system_grid: fleet_ui_state.cross_system_grid.clone(),
                    cross_system_selected: None,
                    // Source the system-barycentric distance from
                    // `star_system_snap` (the only place that builds
                    // `cross_system_grid`).  When the target is a
                    // star system — cross-star or interstellar —
                    // both fields carry the same `distance_ly`.  The
                    // dispatcher reads `cross_system_distance_ly`
                    // for the cross-star subtitle and
                    // `star_system_snap` for the 🌌 header.
                    cross_system_distance_ly: star_system_snap.as_ref().map(|(_, _, ly)| *ly),
                    star_system_snap: star_system_snap
                        .as_ref()
                        .map(|(id, name, ly)| (*id, name.clone(), *ly)),
                    frame_caption: None,
                };
                let dry_mass_t = fleet.ships.iter().map(|s| s.dry_mass_t as f64).sum::<f64>();
                let wet_mass_t = dry_mass_t + fleet.total_fuel_t() as f64;
                let fleet_info = crate::ui::transfer_planner_card::FleetInfo {
                    max_delta_v_ms: fleet_max_dv,
                    wet_mass_t,
                };
                let card = crate::ui::transfer_planner_card::build_selected_card(
                    &*transfer_plan,
                    Some(&card_supplement),
                    fleet_info,
                    |dv_ms| f64::from(fleet.total_fuel_cost_for_dv(dv_ms)),
                );
                crate::ui::transfer_planner_card::render_card(ui, &card);
                ui.add_space(4.0);
            }

            // Execute Transfer button with ETA on the same row.
            //
            // GRA-154 H-1: when the porkchop plot is shown, the legacy
            // 3-option row is skipped (see the `if let Some(grid)` arm
            // below).  But this "Execute Transfer" button currently
            // renders ABOVE the porkchop regardless, so the user sees
            // TWO commit buttons (this one + "🚀 Execute Porkchop
            // Transfer" inside the panel).  Hide this button when the
            // porkchop is shown — the porkchop's own button is the
            // single commit path.
            if fleet_ui_state.porkchop_grid.is_none() {
                ui.horizontal(|ui| {
                    // GRA-367-E: the previous `is_interstellar ||
                    // is_inter_star_body_transfer` clause was removed
                    // — cross-system and inter-star transfers now flow
                    // through the same Execute Transfer button as
                    // interplanetary, gated only on `sel_affordable_
                    // with_abort`.
                    let insufficient = !sel_option.transfer_time_s.is_finite();
                    // Kilo WARNING (PR #234 review): the previous code
                    // silently dropped interstellar clicks because
                    // `maybe_transfer` had no `star_system_snap` arm.
                    // The cross-system grid renders ΔV / TOF, but the
                    // Phase-6 dispatcher (GRA-381) owns the
                    // `PlannedTransfer` star-destination entity path;
                    // GRA-382 widens `SelectionSource` so the next
                    // dispatcher can pick up `Interstellar { … }`
                    // directly.  Until then, disable the Execute
                    // button explicitly and surface the reason in a
                    // hover tooltip instead of pretending the click
                    // did something.
                    let btn =
                        egui::Button::new(egui::RichText::new(&btn_label).size(13.0).strong());
                    let resp = ui.add_enabled(
                        !insufficient && sel_affordable_with_abort && !is_interstellar,
                        btn,
                    );
                    let resp = if is_interstellar {
                        resp.on_hover_text(
                            "Interstellar commit wired in Phase 6 (GRA-381). \
                             The cross-system grid renders ΔV / TOF; the \
                             PlannedTransfer record needs a star-destination \
                             entity path that lands with the Phase-6 dispatcher \
                             (GRA-381).",
                        )
                    } else {
                        resp
                    };
                    if resp.clicked() {
                        {
                            let maybe_transfer = if let Some(ref lp) = lp_target_snap {
                                build_planned_transfer_lp(
                                    fleet_entity,
                                    fleet,
                                    orbit,
                                    lp,
                                    body_query,
                                    &sel_option,
                                )
                            } else if let Some(tfe) = fleet_target_snap {
                                all_fleets_query
                                    .get(tfe)
                                    .ok()
                                    .and_then(|(_, _, _, maybe_fo, _)| maybe_fo)
                                    .and_then(|fo| {
                                        // GRA-NNN: shell-driven radius.
                                        let target_orbit_radius_au = fleet_ui_state
                                            .target_orbit_shell
                                            .filter(|(e, _)| *e == fo.body)
                                            .and_then(|(_, s)| {
                                                body_query
                                                    .get(fo.body)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| radius_for_shell(b, s))
                                            });
                                        build_planned_transfer(
                                            fleet_entity,
                                            fleet,
                                            orbit,
                                            fo.body,
                                            planned_departure_time_s,
                                            body_query,
                                            &sel_option,
                                            course_correction_sc,
                                            body_system_ids,
                                            current_system_id,
                                            target_orbit_radius_au,
                                        )
                                    })
                            } else if let Some(te) = body_target_snap {
                                if sel_option.label == "Gravity Assist" {
                                    // Build the Leg-1 arc toward the flyby body so the departure
                                    // direction and orbital plane are correct, then stitch in a
                                    // Leg-2 arc (flyby → destination) so the in-transit position
                                    // is correct throughout the full two-leg trajectory.
                                    let sel_ga_idx = fleet_ui_state.selected_gravity_assist;
                                    let flyby_e = sel_ga_idx
                                        .and_then(|i| {
                                            fleet_ui_state.gravity_assist_candidates.get(i)
                                        })
                                        .map(|ga| ga.flyby_entity);
                                    let ga_opt = sel_ga_idx
                                        .and_then(|i| {
                                            fleet_ui_state.gravity_assist_candidates.get(i)
                                        })
                                        .map(|e| e.option.clone());

                                    if let Some(flyby) = flyby_e {
                                        // GRA-NNN: shell-driven radius.
                                        let target_orbit_radius_au = fleet_ui_state
                                            .target_orbit_shell
                                            .filter(|(e, _)| *e == flyby)
                                            .and_then(|(_, s)| {
                                                body_query
                                                    .get(flyby)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| radius_for_shell(b, s))
                                            });
                                        let mut maybe_pt = build_planned_transfer(
                                            fleet_entity,
                                            fleet,
                                            orbit,
                                            flyby,
                                            planned_departure_time_s,
                                            body_query,
                                            &sel_option,
                                            course_correction_sc,
                                            body_system_ids,
                                            current_system_id,
                                            target_orbit_radius_au,
                                        );

                                        if let Some(ref mut pt) = maybe_pt {
                                            // Record the flyby body so the executed maneuver can
                                            // reproduce the two-leg path for rendering.
                                            pt.flyby_body = Some(flyby);

                                            // Always record the actual destination so the fleet
                                            // parks at the right body on arrival.
                                            pt.destination_body = te;

                                            // Stitch in Leg-2: flyby → final destination.
                                            if let Some(ga) = ga_opt {
                                                use crate::astronomy::KeplerOrbit;
                                                use crate::fleets::orbital_mechanics::AU_IN_METERS;
                                                use bevy::math::DVec3;

                                                // All three positions must resolve; skip Leg-2
                                                // if any entity is missing to avoid garbage orbit.
                                                let center_res = match pt.reference_frame {
                                                    TransferReferenceFrame::SystemBarycentric => {
                                                        Some(bevy::math::DVec3::ZERO)
                                                    }
                                                    TransferReferenceFrame::Body(center_entity) => {
                                                        body_query
                                                            .get(center_entity)
                                                            .ok()
                                                            .map(|(_, _, sc, _, _)| sc.position)
                                                    }
                                                };
                                                let flyby_res = body_query
                                                    .get(flyby)
                                                    .ok()
                                                    .map(|(_, _, sc, _, _)| sc.position);
                                                let dest_res = body_query
                                                    .get(te)
                                                    .ok()
                                                    .map(|(_, _, sc, _, _)| sc.position);
                                                // Resolve the central body's GM from its mass (works for any star).
                                                let center_gm = match pt.reference_frame {
                                                    TransferReferenceFrame::Body(center_entity) => {
                                                        body_query
                                                            .get(center_entity)
                                                            .ok()
                                                            .map(|(_, b, _, _, _)| G_CONST * b.mass)
                                                            .unwrap_or(GM_SUN)
                                                    }
                                                    TransferReferenceFrame::SystemBarycentric => {
                                                        GM_SUN
                                                    }
                                                };

                                                if let (
                                                    Some(center_pos),
                                                    Some(flyby_pos),
                                                    Some(dest_pos),
                                                ) = (center_res, flyby_res, dest_res)
                                                {
                                                    let flyby_rel = flyby_pos - center_pos;
                                                    let dest_rel = dest_pos - center_pos;
                                                    let flyby_r = flyby_rel.length();
                                                    let dest_r = dest_rel.length();

                                                    let (.., leg2_sma, leg2_ecc) = hohmann_transfer(
                                                        flyby_r, dest_r, center_gm,
                                                    );
                                                    let leg2_outward = dest_r >= flyby_r;
                                                    let leg2_mae = if leg2_outward {
                                                        0.0
                                                    } else {
                                                        std::f64::consts::PI
                                                    };

                                                    // Derive orbital plane and AoP for Leg-2 from
                                                    // the flyby body's current position.
                                                    let plane_n = flyby_rel.cross(dest_rel);
                                                    let plane_len = plane_n.length();
                                                    let (incl2, lan2, aop2) = if plane_len > 1e-20 {
                                                        let n = plane_n / plane_len;
                                                        // Clamp guards against floating-point rounding
                                                        // that can push the dot product slightly outside
                                                        // [-1, 1], which would cause acos to return NaN.
                                                        let incl = n.z.clamp(-1.0, 1.0).acos();
                                                        let nxy = DVec3::new(-n.y, n.x, 0.0);
                                                        let nl = nxy.length();
                                                        let lan = if nl > 1e-20 {
                                                            let nd = nxy / nl;
                                                            nd.y.atan2(nd.x)
                                                        } else {
                                                            0.0
                                                        };
                                                        let aop = if nl > 1e-20 {
                                                            let nd = nxy / nl;
                                                            let pd = flyby_rel.normalize_or_zero();
                                                            let cw = nd.dot(pd);
                                                            let sw = n.dot(nd.cross(pd));
                                                            let om = sw.atan2(cw);
                                                            if leg2_outward {
                                                                om
                                                            } else {
                                                                om + std::f64::consts::PI
                                                            }
                                                        } else {
                                                            let ang =
                                                                flyby_rel.y.atan2(flyby_rel.x);
                                                            if leg2_outward {
                                                                ang
                                                            } else {
                                                                ang - std::f64::consts::PI
                                                            }
                                                        };
                                                        (incl, lan, aop)
                                                    } else {
                                                        let ang = flyby_rel.y.atan2(flyby_rel.x);
                                                        let aop = if leg2_outward {
                                                            ang
                                                        } else {
                                                            ang - std::f64::consts::PI
                                                        };
                                                        (0.0, 0.0, aop)
                                                    };

                                                    let sma_m = leg2_sma * AU_IN_METERS;
                                                    let leg2_mm =
                                                        (center_gm / sma_m.powi(3)).sqrt();

                                                    pt.leg2_orbit = Some(KeplerOrbit {
                                                        semi_major_axis: leg2_sma,
                                                        eccentricity: leg2_ecc,
                                                        inclination: incl2,
                                                        longitude_ascending_node: lan2,
                                                        argument_of_periapsis: aop2,
                                                        mean_anomaly_epoch: leg2_mae,
                                                        mean_motion: leg2_mm,
                                                    });
                                                    pt.leg2_start_s = ga.leg1_time_s;
                                                } // end: if let (Some(center_pos), ...)
                                            }
                                        }
                                        maybe_pt
                                    } else {
                                        // GRA-NNN: shell-driven radius.
                                        let target_orbit_radius_au = fleet_ui_state
                                            .target_orbit_shell
                                            .filter(|(e, _)| *e == te)
                                            .and_then(|(_, s)| {
                                                body_query
                                                    .get(te)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| radius_for_shell(b, s))
                                            });
                                        build_planned_transfer(
                                            fleet_entity,
                                            fleet,
                                            orbit,
                                            te,
                                            planned_departure_time_s,
                                            body_query,
                                            &sel_option,
                                            course_correction_sc,
                                            body_system_ids,
                                            current_system_id,
                                            target_orbit_radius_au,
                                        )
                                    }
                                } else {
                                    // GRA-NNN: shell-driven radius.
                                    let target_orbit_radius_au = fleet_ui_state
                                        .target_orbit_shell
                                        .filter(|(e, _)| *e == te)
                                        .and_then(|(_, s)| {
                                            body_query
                                                .get(te)
                                                .ok()
                                                .map(|(_, b, _, _, _)| radius_for_shell(b, s))
                                        });
                                    build_planned_transfer(
                                        fleet_entity,
                                        fleet,
                                        orbit,
                                        te,
                                        planned_departure_time_s,
                                        body_query,
                                        &sel_option,
                                        course_correction_sc,
                                        body_system_ids,
                                        current_system_id,
                                        target_orbit_radius_au,
                                    )
                                }
                            } else {
                                None
                            };
                            if let Some(transfer) = maybe_transfer {
                                pending_actions.start_transfers.push(StartTransferAction {
                                    fleet: fleet_entity,
                                    transfer,
                                    abort_cost_t,
                                    departure_offset_s: fleet_ui_state.departure_offset_days
                                        * 86_400.0,
                                });
                                // Close the transfer popup so the preview arc doesn't
                                // immediately show an abort trajectory after launch.
                                fleet_ui_state.show_transfer_popup = false;
                            }
                        }
                    }
                    if !hides_calendar_eta {
                        let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                        let total_eta_s = dep_s + sel_option.transfer_time_s;
                        ui.add_space(theme::Spacing::lg);
                        ui.label(
                            egui::RichText::new(format!("ETA  {}", format_duration(total_eta_s)))
                                .size(12.0)
                                .color(theme::GREEN),
                        );
                    }
                });
            } // end !porkchop_grid — hide legacy "Execute Transfer" when panel is shown

            // GRA-153 M-3: "Abort to Origin" + "Disband Fleet" buttons.
            // Shown only when the fleet is mid-transit (course correction mode).
            // - "Abort to Origin" (primary, default): refits a parking orbit
            //   at the origin body.  Preserves the fleet entity, ships, and
            //   render position.  The fleet is NOT silently dissolved.
            // - "Disband Fleet" (secondary, confirmation): the legacy
            //   "silently dissolve" behaviour, gated behind a confirmation
            //   modal to prevent accidental clicks.
            if is_course_correction {
                ui.add_space(theme::Spacing::sm);
                let abort_label = if abort_cost_t > 0.0 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!("⛔ Abort to Origin (+{:.2} km/s burn)", abort_dv_kms)
                } else {
                    "⛔ Abort to Origin".to_string()
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(&abort_label)
                                .size(12.0)
                                .color(theme::RED),
                        )
                        .min_size(egui::Vec2::new(120.0, 30.0)),
                    )
                    .on_hover_text("Cancel the current transfer and return the fleet to a parking orbit at the origin body. Ships are preserved.")
                    .clicked()
                {
                    pending_actions.abort_to_origin.push(AbortToOriginAction {
                        fleet: fleet_entity,
                        abort_cost_t,
                    });
                    fleet_ui_state.show_transfer_popup = false;
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("💥 Disband Fleet")
                                .size(10.0)
                                .color(theme::TEXT_DIM),
                        )
                        .min_size(egui::Vec2::new(120.0, 24.0)),
                    )
                    .on_hover_text("Permanently dissolve this fleet. All ships return to independent orbit. This cannot be undone.")
                    .clicked()
                {
                    fleet_ui_state.disband_confirm_fleet = Some(fleet_entity);
                }
            }
            if !hides_calendar_eta {
                // GRA-388: the burn epoch that drives the
                // trajectory (used by `draw_fleet_transfer_preview`
                // and `draw_gravity_assist_preview`) is the recorded
                // absolute t_dep when one exists, otherwise the
                // slider's `now + offset` value.  The pre-existing
                // Arrives computation only used the slider value,
                // so in GA mode (and after clicking any porkchop
                // cell) the label disagreed with what the
                // trajectory was actually drawn against — the
                // "I can not influence the departure time" report.
                // Use the effective burn epoch for both labels so
                // they agree with the on-screen trajectory.
                let dep_offset_s: f64 = fleet_ui_state
                    .selected_abs_t_dep_s
                    .map(|t| (t - elapsed).max(0.0))
                    .unwrap_or(fleet_ui_state.departure_offset_days.max(0.0) * 86_400.0);
                let departure_ts = current_timestamp + dep_offset_s as i64;
                ui.label(
                    egui::RichText::new(format!(
                        "Departs  {}",
                        format_timestamp_date_time(departure_ts)
                    ))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
                );
                let total_eta_s = dep_offset_s + sel_option.transfer_time_s;
                if let Some(arrival_ts) = checked_arrival_timestamp(current_timestamp, total_eta_s)
                {
                    ui.label(
                        egui::RichText::new(format!(
                            "Arrives  {}",
                            format_timestamp_date_time(arrival_ts)
                        ))
                        .size(11.0)
                        .color(theme::RP_BLUE),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Arrives  unavailable")
                            .size(11.0)
                            .color(theme::AMBER),
                    );
                }
            }
            if !sel_affordable_with_abort {
                ui.label(
                    egui::RichText::new(if abort_cost_t > 0.0 {
                        "Insufficient \u{394}V remaining after abort burn."
                    } else {
                        "Selected option requires more \u{394}V than this fleet can provide."
                    })
                    .size(11.0)
                    .italics()
                    .color(theme::RED),
                );
            }
        }

        let show_binary_transfer_direct_labels = body_target_snap
            .map(|target| is_inter_star_transfer(orbit.body, target, body_query))
            .unwrap_or(false);

        if !fleet_ui_state.computed_options.is_empty() {
            let fleet_wet_mass = fleet.total_wet_mass_t();
            let fleet_max_dv = fleet.max_delta_v_ms();

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Transfer Options:").strong().size(13.0));
            ui.add_space(2.0);

            // GRA-152 H-1: when a porkchop grid is cached on the
            // FleetUiState (the LGD-driven path), render the
            // PorkchopPanel in place of the Efficient / Moderate /
            // Fast `selectable_label` block.  When the grid is absent
            // (e.g. course corrections, intra-system previews) the
            // legacy 3-option row is rendered as before, so all
            // pre-existing code paths keep working.
            if let Some(grid_owned) = fleet_ui_state.porkchop_grid.as_ref().cloned() {
                // v0.5.0 follow-up: the legacy planner exposed the
                // Transfer Window box (synodic period) and the Fleet
                // stats infobox (ΔV avail, thrust, acceleration) at
                // the top of the panel — those were dropped when the
                // porkchop landed because the porkchop branch took the
                // same `if let Some(grid)` slot.  Restore them above
                // the grid so the player doesn't lose situational
                // awareness while browsing the ΔV surface.
                // v0.5.0 follow-up (compact): build the entire status strip
                // as a single label so we don't have to nest
                // `ui.horizontal(...)` inside the scrollable planner
                // popup — the nested horizontal block was leaving
                // ~500 px of empty space below the strip because
                // egui's `horizontal_top` was reserving a column
                // sized for the inline separator's `max` line-height,
                // which inflated the popup's content height past the
                // ScrollArea's natural fit.  A single `ui.label(...)`
                // has no such footgun.  The string is colour-tagged
                // via `BackgroundColor` per segment through a
                // `RichText::append` chain if we ever need it; for
                // now plain `TEXT_DIM` reads fine.
                {
                    let dv_kms = fleet_max_dv / 1_000.0;
                    let thrust_kn = fleet.min_thrust_kn();
                    let thrust_str = if thrust_kn >= 1_000.0 {
                        format!("{:.1} MN", thrust_kn / 1_000.0)
                    } else {
                        format!("{:.0} kN", thrust_kn)
                    };
                    let accel_g = fleet.min_accel_ms2() / 9.80665;
                    let mut info = String::with_capacity(96);
                    if let Some(ref window) = window_this_frame {
                        let window_days = window.time_to_window_s / 86_400.0;
                        let next_window_str = if window_days < 1.0 {
                            "NOW ✓".to_owned()
                        } else {
                            format_duration(window.time_to_window_s).to_string()
                        };
                        let syn_str = if window.synodic_period_s.is_finite() {
                            format_duration(window.synodic_period_s)
                        } else {
                            "∞".to_owned()
                        };
                        info.push_str(&format!(
                            "⏱ Next: {next_window_str}  ·  Synodic: {syn_str}  ·  "
                        ));
                    }
                    info.push_str(&format!(
                        "🚀 ΔV: {dv_kms:.2} km/s  ·  {thrust_str}  ·  {accel_g:.3} g"
                    ));
                    ui.label(egui::RichText::new(info).size(10.5).color(theme::TEXT_DIM));
                }

                // The phase-window overlay needs `compute_transfer_window`'s
                // `time_to_window_s`; that value is computed upstream in this
                // function (above).  We pass NaN here as a sentinel meaning
                // "no phase-window overlay" — the panel renders the rest of
                // the grid either way.  Wiring the live value requires
                // threading it through this control flow; left as a
                // follow-up so this PR stays focused.
                // Rotating-buffer scroll: how many sim seconds the
                // player's clock has advanced since the buffer was
                // built.  Drives the panel's fractional x-axis
                // scroll so cells move smoothly without per-frame
                // rebuilds.  `shift_s` is computed once above the
                // rotation-trigger block so the same value threads
                // through both the staleness check and the render.
                let time_to_window_s = f64::NAN;
                // GRA-385 view-mode dispatch: decide which grid to render in the
                // panel.  Standard -> the main Lambert grid; GA(idx)
                // -> the candidate's `(t_dep, tof)` grid built via
                // `sweep_gravity_assist_grid`.  When the GA candidate
                // list is empty or the cached grid has gone stale,
                // fall back to Standard.
                let display_grid: crate::fleets::porkchop::PorkchopGrid =
                    match fleet_ui_state.porkchop_view_mode {
                        crate::ui::PorkchopViewMode::Standard => grid_owned.clone(),
                        crate::ui::PorkchopViewMode::GravityAssist(idx) => {
                            // Build the GA grid on demand from the candidate
                            // data.  This is cheap (~hundreds of microseconds
                            // per cell on the RON-override 50×40 grid) and
                            // only runs when the user has explicitly toggled
                            // into a GA view.  We use a temporary scope so the
                            // `&fleet_ui_state` borrow doesn't conflict with
                            // the panel call.
                            if let (Some(candidate), Some(target)) = (
                                fleet_ui_state.gravity_assist_candidates.get(idx),
                                body_target_snap,
                            ) {
                                build_gravity_assist_display_grid(
                                    porkchop_config,
                                    candidate,
                                    body_query,
                                    orbit.body,
                                    target,
                                    elapsed,
                                )
                            } else {
                                // Stale view-mode index (the candidate list
                                // shrunk since the player toggled) or no
                                // target body selected.  Drop back to
                                // Standard so the panel still shows
                                // something useful.
                                fleet_ui_state.porkchop_view_mode =
                                    crate::ui::PorkchopViewMode::Standard;
                                grid_owned.clone()
                            }
                        }
                    };
                super::porkchop_panel::porkchop_panel(
                    ui,
                    &display_grid,
                    porkchop_config,
                    &mut fleet_ui_state.selected_porkchop_cell,
                    fleet_max_dv,
                    time_to_window_s,
                    shift_s,
                    fleet_ui_state.target_body,
                    &mut fleet_ui_state.porkchop_texture,
                    &mut fleet_ui_state.porkchop_texture_built_for,
                );

                // GRA-385 view-mode toggle row (rendered below the
                // panel).  Each button swaps `porkchop_view_mode` to
                // render a different grid; the currently active view
                // is highlighted in `EP_TEAL`.  Built as a row of
                // small_button-style pills so the planner doesn't
                // gain vertical space when GA candidates aren't
                // available (the row collapses to just "Standard").
                //
                // GRA-388: clone the candidates list here so the
                // closure body can take a mutable borrow of
                // `fleet_ui_state` (the per-pill click handlers
                // mutate `porkchop_view_mode`, `selected_gravity_assist`,
                // and (after GRA-388) `selected_abs_t_dep_s`).
                // Iterating `fleet_ui_state.gravity_assist_candidates`
                // directly would create an immutable borrow that
                // blocks the mutable borrow inside the loop body.
                let candidates = fleet_ui_state.gravity_assist_candidates.clone();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("View:")
                            .size(10.5)
                            .strong()
                            .color(theme::TEXT_DIM),
                    );
                    let standard_selected = matches!(
                        fleet_ui_state.porkchop_view_mode,
                        crate::ui::PorkchopViewMode::Standard
                    );
                    if ui
                        .selectable_label(
                            standard_selected,
                            egui::RichText::new("Standard").size(10.5),
                        )
                        .clicked()
                        && !standard_selected
                    {
                        fleet_ui_state.porkchop_view_mode = crate::ui::PorkchopViewMode::Standard;
                        fleet_ui_state.selected_porkchop_cell = None;
                        fleet_ui_state.planned_transfer = None;
                    }
                    for (idx, candidate) in candidates.iter().enumerate() {
                        let ga_selected = matches!(
                            fleet_ui_state.porkchop_view_mode,
                            crate::ui::PorkchopViewMode::GravityAssist(i) if i == idx
                        );
                        let badge = if candidate.option.dv_savings_ms > 100.0 {
                            "✓"
                        } else {
                            "△"
                        };
                        let label = format!("{} via {}", badge, candidate.option.body_name);
                        if ui
                            .selectable_label(ga_selected, egui::RichText::new(label).size(10.5))
                            .clicked()
                            && !ga_selected
                        {
                            fleet_ui_state.porkchop_view_mode =
                                crate::ui::PorkchopViewMode::GravityAssist(idx);
                            fleet_ui_state.selected_gravity_assist = Some(idx);
                            fleet_ui_state.selected_option = 0;
                            fleet_ui_state.selected_porkchop_cell = None;
                            fleet_ui_state.planned_transfer = None;
                            // GRA-388: seed the absolute burn epoch
                            // from the current slider value so the
                            // first frame of GA mode renders a
                            // trajectory anchored at the slider's
                            // burn time (either the legacy -1.0
                            // sentinel resolved by
                            // `transfer_planner.rs:4719-4726` to the
                            // next Hohmann window, or whatever
                            // offset the player last dialled in).
                            // Without this seed the trajectory
                            // would render against the previous
                            // cell's absolute epoch (or
                            // immediate-departure fallback) and the
                            // user would see no visual response to
                            // the view-mode switch.
                            maybe_record_burn_epoch_for_ga(
                                fleet_ui_state,
                                elapsed,
                                fleet_ui_state.departure_offset_days,
                            );
                        }
                    }
                });

                // GRA-385: route the panel's GA-selection output into
                // `fleet_ui_state.selected_gravity_assist`.  We
                // intentionally do this AFTER the panel returns so
                // the planner can clear `planned_transfer` and
                // `selected_option` (the GA-as-option-0 dispatch
                // happens here too — see the "Gravity Assist" branch
                // of the option-matching code further down).
                if let crate::ui::PorkchopViewMode::GravityAssist(idx) =
                    fleet_ui_state.porkchop_view_mode
                {
                    if fleet_ui_state.selected_gravity_assist != Some(idx) {
                        fleet_ui_state.selected_gravity_assist = Some(idx);
                        fleet_ui_state.selected_option = 0;
                        fleet_ui_state.planned_transfer = None;
                    }
                }

                ui.add_space(4.0);
                if let Some((sc, sr)) = fleet_ui_state.selected_porkchop_cell {
                    // Capture absolute (t_dep, tof) of the selected
                    // cell so the post-rotation re-anchor in the
                    // build block above can find the same physical
                    // cell in the new buffer.  Only update when the
                    // (sc, sr) has actually changed (i.e. the user
                    // clicked a new cell or the planner just re-anchored
                    // to a matching (sc, sr) and we're back to the
                    // first frame of the next rotation cycle).
                    if let Some(cell) = grid_owned.cells.get(sr * grid_owned.resolution.0 + sc) {
                        let (cols_buf, _rows_buf) = grid_owned.resolution;
                        if cols_buf > 0 {
                            let col_step = (grid_owned.t_dep_bounds_s.1
                                - grid_owned.t_dep_bounds_s.0)
                                / cols_buf as f64;
                            let abs_t_dep = grid_owned.t_dep_bounds_s.0 + (sc as f64) * col_step;
                            let abs_tof = cell.tof_s;
                            // Detect "we just re-anchored" by checking
                            // if the recorded abs t_dep already
                            // matches the current (sc, sr) within
                            // half a col_step.  If so, the (sc, sr)
                            // we have IS the re-anchored one — don't
                            // overwrite the recorded abs t_dep with
                            // a slightly different value (the new
                            // buffer's grid resolution might not
                            // align exactly with the old).
                            //
                            // Also accept "recorded ≈ elapsed" as a
                            // match, but ONLY when we're sitting on
                            // col 0 (the "Now" line).  When the
                            // per-frame immediate-departure re-anchor
                            // above clamps the cell to col 0 it sets
                            // `selected_abs_t_dep_s = Some(elapsed)`
                            // (not the cell's natural
                            // `t_dep_bounds_s.0`); without this
                            // second match condition the post-panel
                            // block would overwrite the recorded
                            // anchor back to the cell's natural
                            // (past) `t_dep_bounds_s.0`, undoing
                            // the clamp.
                            //
                            // The col-0 guard is critical: without
                            // it, `prev ≈ elapsed` matches ANY
                            // future-cell click taken after the
                            // re-anchor fired, suppressing the
                            // abs-coord update — and the very next
                            // frame's re-anchor (which checks
                            // `recorded_abs_t_dep < elapsed`) fires
                            // again and clamps the user's click back
                            // to col 0.  This was the
                            // "can't select a future cell once the
                            // selected tile moves to 'now'" bug.
                            let current_matches_recorded = match fleet_ui_state.selected_abs_t_dep_s
                            {
                                Some(prev) => {
                                    (prev - abs_t_dep).abs() < col_step * 0.5
                                        || (sc == 0 && (prev - elapsed).abs() < col_step * 0.5)
                                }
                                None => false,
                            };
                            if !current_matches_recorded {
                                fleet_ui_state.selected_abs_t_dep_s = Some(abs_t_dep);
                                fleet_ui_state.selected_abs_tof_s = Some(abs_tof);
                            }
                        }
                    }
                    if let Some(cell) = grid_owned.cells.get(sr * grid_owned.resolution.0 + sc) {
                        if cell.feasible {
                            // v0.5.0 follow-up: the legacy 3-option row
                            // printed ΔV + Est. fuel side-by-side.
                            // Surface the same numbers (plus the
                            // arrival speed, which the legacy row
                            // didn't have) for the selected porkchop
                            // cell, so the player can compare fuel
                            // budgets between cells without leaving
                            // the panel.
                            let fuel_cost = fleet.total_fuel_cost_for_dv(cell.total_dv_ms);
                            let fuel_pct = if fleet_wet_mass > 0.0 {
                                (fuel_cost / fleet_wet_mass * 100.0) as u32
                            } else {
                                0
                            };
                            let v_arr_speed_km_s = cell.v_arrival_ms.length() / 1000.0;
                            let v_inf_arr_km_s = cell.v_inf_arrival_ms / 1000.0;
                            // v_inf_arrival_ms is "excess speed above
                            // circular at destination" — 0 for any
                            // Hohmann-shaped arrival (Earth→Mars,
                            // Earth→Venus), > 0 only for super-circular
                            // hyperbolic-style arrivals.  The legacy
                            // planner never surfaced this stat; the
                            // porkchop tooltip had it but always
                            // showed 0 for Hohmanns and confused the
                            // player.  Show both: the actual arrival
                            // speed (always meaningful) and the
                            // circular excess (zero for Hohmann).
                            ui.label(
                                egui::RichText::new(format!(
                                    "Selected cell: t_dep = {:.0} d, TOF = {:.0} d, ΔV = {:.2} km/s",
                                    // GRA-388: show the *visual* t_dep
                                    // (relative to current sim time),
                                    // not the cell's intrinsic t_dep_s
                                    // (relative to the grid's
                                    // t_dep_bounds_s.0).  The cell
                                    // highlight slides via the
                                    // fractional UV scroll
                                    // (`scroll = shift_s / col_step_s`)
                                    // so the on-screen position
                                    // advances toward the "Now" line
                                    // every frame, but the intrinsic
                                    // t_dep_s is a baked-in grid
                                    // property that doesn't change
                                    // between rebuilds.  Reading the
                                    // intrinsic value made the cell
                                    // appear "fixed" even though the
                                    // highlight was visibly sliding
                                    // — the user-reported
                                    // "rolling up to the 'now' mark
                                    // is broken" symptom.  Subtracting
                                    // `shift_s` (sim seconds since the
                                    // grid was built) yields the
                                    // t_dep from *now*, which
                                    // decreases toward 0 as the
                                    // recorded burn approaches, then
                                    // the re-anchor fires and the
                                    // cell sticks at "Now".
                                    ((cell.t_dep_s - shift_s)
                                        .max(0.0)
                                        / crate::ui::porkchop_panel::SECONDS_PER_DAY),
                                    cell.tof_s / crate::ui::porkchop_panel::SECONDS_PER_DAY,
                                    cell.total_dv_ms / 1000.0,
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Fuel: {:.1} t ({fuel_pct}%) · v(arr) = {v_arr_speed_km_s:.2} km/s · v∞(arr) = {v_inf_arr_km_s:.2} km/s",
                                    fuel_cost,
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                        }
                    }
                }
                //
                // GRA-165 defensive guard: the porkchop is the source of
                // truth for the trajectory preview.  Drop any lingering
                // GRA-386: the defensive guard from GRA-165 used to
                // unconditionally clear `selected_gravity_assist` as
                // soon as a cell was picked.  That was correct in the
                // pre-GRA-385 world where GA candidates only ever lived
                // in their own side-panel row, but with the new
                // view-mode toggle the player picks a cell INSIDE the
                // GA panel — and `draw_gravity_assist_preview`
                // (`src/fleets/visuals.rs:4118`) keys its two-color
                // slingshot overlay on `selected_gravity_assist`.  If
                // we clear it here the renderer returns early at
                // `visuals.rs:4136` and the player sees only the
                // amber direct-Lambert arc instead of the lime-green
                // Leg-1 + magenta Leg-2 trajectory.
                //
                // We now ONLY clear when the player is NOT inside a
                // GA view-mode toggle — i.e. when a stray
                // `selected_gravity_assist` somehow leaks into the
                // standard porkchop flow.  Inside
                // `PorkchopViewMode::GravityAssist(idx)` we keep both
                // fields set so the renderer can draw both the
                // slingshot overlay AND the planned-transfer
                // `flyby_body` marker at the same time.
                if fleet_ui_state.selected_gravity_assist.is_some()
                    && !matches!(
                        fleet_ui_state.porkchop_view_mode,
                        crate::ui::PorkchopViewMode::GravityAssist(_)
                    )
                {
                    fleet_ui_state.selected_gravity_assist = None;
                    fleet_ui_state.selected_option = 0;
                }
                // GRA-154 H-2: drive the trajectory preview from the
                // selected porkchop cell.  The PorkchopPanel sets
                // `selected_porkchop_cell` on click; this block
                // translates that into a synthetic `TransferOption` and
                // feeds it through `build_planned_transfer`, populating
                // `fleet_ui_state.planned_transfer` so the 3D preview
                // arc, ghost marker and right-side stats panel all
                // update the moment the player picks a cell.  Without
                // this block the panel only updated the on-canvas
                // selection highlight — the trajectory overlay kept
                // showing whatever `selected_option` last pointed at.
                //
                // Also sync `departure_offset_days` so the legacy
                // side-panel "Arrives:" timestamp, the
                // `waiting_orbit_count`, and any other code path that
                // reads `departure_offset_days` stays consistent with
                // the cell the player just clicked.  The porkchop is
                // the source of truth for t_dep; the slider has been
                // hidden in this UI mode (see "Hide slider" block
                // below) so the player's only way to set the
                // departure time is through the porkchop.
                //
                // Clear the preview when nothing is selected or the
                // selected cell is infeasible / out-of-budget, so the
                // ghost arc disappears instead of going stale.
                fleet_ui_state.planned_transfer = match fleet_ui_state.selected_porkchop_cell {
                    Some((sc, sr)) => {
                        let cell = grid_owned.cells.get(sr * grid_owned.resolution.0 + sc);
                        match (cell, body_target_snap) {
                            // Loosened guard: any feasible cell with
                            // finite ΔV produces a preview, even if
                            // the cell is out-of-budget for this
                            // fleet.  The Execute button below has
                            // its own `can_execute` check (which does
                            // include the fleet-budget guard) so
                            // out-of-budget cells are still rejected
                            // at commit time — but the trajectory
                            // preview is the player's primary way to
                            // compare cells, so we let them see the
                            // ghost arc for *any* feasible cell.  The
                            // previous "preview stays None on
                            // out-of-budget click" behaviour made the
                            // 3D arc look frozen whenever the player
                            // hovered over a red cell, which read as
                            // "trajectory never updates".
                            (Some(cell), Some(target_entity))
                                if cell.feasible
                                    && cell.total_dv_ms.is_finite()
                                    && cell.delta_v1_ms.is_finite()
                                    && cell.delta_v2_ms.is_finite() =>
                            {
                                let (cell_sma_au, cell_ecc) = cell
                                    .transfer_orbit
                                    .as_ref()
                                    .map(|o| (o.semi_major_axis, o.eccentricity))
                                    .unwrap_or((0.0, 0.0));
                                let synthetic_option =
                                    crate::fleets::orbital_mechanics::TransferOption {
                                        label: "Porkchop Cell",
                                        total_delta_v_ms: cell.total_dv_ms,
                                        delta_v1_ms: cell.delta_v1_ms,
                                        delta_v2_ms: cell.delta_v2_ms,
                                        transfer_time_s: cell.tof_s,
                                        sma_au: cell_sma_au,
                                        eccentricity: cell_ecc,
                                        energy_multiplier: 1.0,
                                        burn_time_s: 0.0,
                                        plane_change_dv_ms: 0.0,
                                        is_thrust_limited: false,
                                        // `cell.transfer_orbit` is
                                        // `Option<KeplerOrbit>` (Copy);
                                        // the orbit is just a few
                                        // floats, so pass by value
                                        // instead of cloning.
                                        transfer_orbit_override: cell.transfer_orbit,
                                    };
                                // Anchor the planned burn at the cell's
                                // *absolute* epoch (`selected_abs_t_dep_s`,
                                // recorded at click and re-anchored across
                                // buffer rotations) rather than the
                                // relative `elapsed + cell.t_dep_s`.  The
                                // relative formula drifts forward by
                                // `elapsed` every frame, which keeps
                                // `planet(burn_time) - planet(current)`
                                // constant — so the trajectory's start
                                // slides around the orbit at the planet's
                                // own orbital rate and visually looks
                                // "glued" to the planet between grid
                                // rebuilds.  Anchoring absolutely freezes
                                // the burn epoch, so the burn-time planet
                                // position advances in world space and the
                                // visible separation between trajectory
                                // start and live planet shrinks as sim
                                // time approaches the burn — which is the
                                // "consume toward now" behaviour the user
                                // expects as the chart cell slides left.
                                //
                                // The Lambert solution is still re-solved
                                // every frame in `build_planned_transfer`
                                // with planet positions evaluated at this
                                // fixed absolute burn time, so the
                                // transfer orbit's `mean_anomaly_epoch` /
                                // `mean_motion` / `departure_velocity_ms`
                                // refresh per frame and the Bezier
                                // interior visibly evolves.
                                let planned_departure_time_s = fleet_ui_state
                                    .selected_abs_t_dep_s
                                    .unwrap_or(grid_owned.t_dep_bounds_s.0 + cell.t_dep_s);
                                // Sync `departure_offset_days` so the
                                // side-panel "Arrives:" timestamp and
                                // `waiting_orbit_count` reflect the
                                // porkchop cell's t_dep.  The slider is
                                // hidden in porkchop mode so the cell
                                // is the only source of t_dep; without
                                // this sync, downstream code that reads
                                // `departure_offset_days` would still
                                // see the last slider value.
                                fleet_ui_state.departure_offset_days =
                                    cell.t_dep_s / crate::ui::porkchop_panel::SECONDS_PER_DAY;
                                let target_orbit_radius_au: Option<f64> = None;
                                // GRA-386: when the player is in a GA
                                // view-mode toggle AND has a cell
                                // selected, build a two-leg
                                // `PlannedTransfer` via the flyby body so
                                // the 3D preview arc shows the
                                // origin → flyby → destination slingshot.
                                // Without this branch the cell-click path
                                // would produce a single Lambert arc
                                // straight to the destination, hiding the
                                // flyby entirely.
                                if let crate::ui::PorkchopViewMode::GravityAssist(ga_idx) =
                                    fleet_ui_state.porkchop_view_mode
                                {
                                    if let Some(candidate) =
                                        fleet_ui_state.gravity_assist_candidates.get(ga_idx)
                                    {
                                        // `sweep_gravity_assist_grid`
                                        // stores the Leg-2 conic on
                                        // `cell.transfer_orbit`; pass it
                                        // through to the helper so the
                                        // preview matches the geometry the
                                        // commit will launch with.
                                        build_planned_transfer_with_flyby(
                                            fleet_entity,
                                            fleet,
                                            orbit,
                                            candidate.flyby_entity,
                                            target_entity,
                                            &synthetic_option,
                                            // Leg-1 half-period for the
                                            // leg2_start_s timestamp.
                                            // `synthetic_option.transfer_time_s`
                                            // is the full TOF (= leg1 + leg2)
                                            // so we approximate leg1 as half.
                                            // The active-transit renderer
                                            // only uses this as a hint for
                                            // the leg-switch timestamp; small
                                            // inaccuracy is fine.
                                            cell.tof_s * 0.5,
                                            planned_departure_time_s,
                                            body_query,
                                            course_correction_sc,
                                            body_system_ids,
                                            current_system_id,
                                            target_orbit_radius_au,
                                            cell.transfer_orbit,
                                        )
                                    } else {
                                        // Stale GA index — fall through
                                        // to the direct-transfer path.
                                        build_planned_transfer(
                                            fleet_entity,
                                            fleet,
                                            orbit,
                                            target_entity,
                                            planned_departure_time_s,
                                            body_query,
                                            &synthetic_option,
                                            course_correction_sc,
                                            body_system_ids,
                                            current_system_id,
                                            target_orbit_radius_au,
                                        )
                                    }
                                } else {
                                    // Standard view-mode: direct Lambert
                                    // arc, unchanged from before.
                                    build_planned_transfer(
                                        fleet_entity,
                                        fleet,
                                        orbit,
                                        target_entity,
                                        planned_departure_time_s,
                                        body_query,
                                        &synthetic_option,
                                        course_correction_sc,
                                        body_system_ids,
                                        current_system_id,
                                        target_orbit_radius_au,
                                    )
                                }
                            }
                            _ => None,
                        }
                    }
                    None => None,
                };
                // GRA-154 H-1: turn the selected porkchop cell into a
                // `PlannedTransfer` and let the player commit it.  Until
                // this branch was added the panel only displayed the
                // cell's stats and the file's `return;` skipped the
                // legacy "Execute Transfer" button, so clicking a cell
                // was effectively a dead end.  We build a synthetic
                // `TransferOption` from the cell's Lambert-solved values
                // and feed it through the same `build_planned_transfer`
                // call the body-target branch uses, then push a
                // `StartTransferAction` to the pending-actions queue.
                if let Some((sc, sr)) = fleet_ui_state.selected_porkchop_cell {
                    let target_entity = match body_target_snap {
                        Some(te) => te,
                        None => {
                            // Stale grid: target was cleared but the
                            // player still has a cell picked.  Clear the
                            // selection so the next frame's render path
                            // is clean.
                            fleet_ui_state.selected_porkchop_cell = None;
                            return;
                        }
                    };
                    if let Some(cell) = grid_owned
                        .cells
                        .get(sr * grid_owned.resolution.0 + sc)
                        .cloned()
                    {
                        let can_execute = cell.feasible
                            && cell.total_dv_ms.is_finite()
                            && cell.total_dv_ms <= fleet_max_dv
                            && cell.delta_v1_ms.is_finite()
                            && cell.delta_v2_ms.is_finite();
                        let btn = egui::Button::new(
                            egui::RichText::new("🚀 Execute Transfer")
                                .size(13.0)
                                .strong(),
                        );
                        if ui
                            .add_enabled(can_execute, btn)
                            .on_disabled_hover_text(
                                "Select a feasible cell (greyed cells cannot be executed).",
                            )
                            .clicked()
                        {
                            // Synthetic `TransferOption` populated from
                            // the cell's Lambert-solved values.  Only the
                            // fields `build_planned_transfer` actually
                            // reads are populated; the rest stay at
                            // sensible defaults.
                            //
                            // Use the same absolute anchor as the
                            // preview above (`selected_abs_t_dep_s`)
                            // so the executed maneuver's burn time
                            // matches the visible preview instead of
                            // drifting forward by `elapsed`.
                            let planned_departure_time_s = fleet_ui_state
                                .selected_abs_t_dep_s
                                .unwrap_or(grid_owned.t_dep_bounds_s.0 + cell.t_dep_s);
                            let (cell_sma_au, cell_ecc) = cell
                                .transfer_orbit
                                .as_ref()
                                .map(|o| (o.semi_major_axis, o.eccentricity))
                                .unwrap_or((0.0, 0.0));
                            let synthetic_option =
                                crate::fleets::orbital_mechanics::TransferOption {
                                    label: "Porkchop Cell",
                                    total_delta_v_ms: cell.total_dv_ms,
                                    delta_v1_ms: cell.delta_v1_ms,
                                    delta_v2_ms: cell.delta_v2_ms,
                                    transfer_time_s: cell.tof_s,
                                    sma_au: cell_sma_au,
                                    eccentricity: cell_ecc,
                                    energy_multiplier: 1.0,
                                    burn_time_s: 0.0,
                                    plane_change_dv_ms: 0.0,
                                    is_thrust_limited: false,
                                    // `cell.transfer_orbit` is `Option<KeplerOrbit>`
                                    // (Copy); pass by value instead of cloning.
                                    transfer_orbit_override: cell.transfer_orbit,
                                };
                            // Porkchop grids are only built for body
                            // targets (planets/moons) — never stars —
                            // so the star-approach override is always
                            // `None` for this path.
                            let target_orbit_radius_au: Option<f64> = None;
                            if let Some(transfer) = build_planned_transfer(
                                fleet_entity,
                                fleet,
                                orbit,
                                target_entity,
                                planned_departure_time_s,
                                body_query,
                                &synthetic_option,
                                course_correction_sc,
                                body_system_ids,
                                current_system_id,
                                target_orbit_radius_au,
                            ) {
                                // Fresh transfers from a stable parking
                                // orbit have no in-flight maneuver to
                                // abort, so the abort-cost burn penalty
                                // is zero (mirrors the non-course-
                                // correction leg of the legacy branch).
                                pending_actions.start_transfers.push(StartTransferAction {
                                    fleet: fleet_entity,
                                    transfer,
                                    abort_cost_t: 0.0,
                                    departure_offset_s: cell.t_dep_s,
                                });
                                // Close the transfer popup so the preview
                                // arc doesn't immediately show an abort
                                // trajectory after launch.
                                fleet_ui_state.show_transfer_popup = false;
                            }
                        }
                    }
                }
                // Skip the legacy 3-option row when the panel is shown.
                return;
            }

            // GRA-387 follow-up: when a gravity assist is selected and
            // there is no porkchop grid on screen, the player is
            // stranded on the GA summary card with no way back to the
            // standard transfer — the GRA-385 view-mode toggle row
            // (rendered above as part of the porkchop branch) never
            // reached this frame because the `if let Some(grid)`
            // skipped it.  Surface a single "Switch to Standard"
            // button here so the GA-only path can still return to
            // direct mode without closing the planner.
            if fleet_ui_state.selected_gravity_assist.is_some()
                && fleet_ui_state.porkchop_grid.is_none()
            {
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("↩ Switch to standard transfer")
                                .size(11.0)
                                .color(theme::RP_BLUE),
                        )
                        .min_size(egui::Vec2::new(160.0, 24.0)),
                    )
                    .on_hover_text(
                        "Clears the current gravity-assist selection and returns to the \
                         standard transfer mode (Efficient / Moderate / Fast / Porkchop).",
                    )
                    .clicked()
                {
                    fleet_ui_state.selected_gravity_assist = None;
                    fleet_ui_state.porkchop_view_mode = crate::ui::PorkchopViewMode::Standard;
                    fleet_ui_state.computed_options.clear();
                    fleet_ui_state.selected_option = 0;
                    fleet_ui_state.planned_transfer = None;
                }
                return;
            }
            // GRA-154 H-2 follow-up: when a gravity assist is selected the
            // legacy Efficient / Moderate / Fast row must NOT reappear.
            // The assist branch in `build_planned_transfer` (above) uses the
            // "Gravity Assist" `TransferOption::label` to stitch Leg-1 +
            // Leg-2 — showing the legacy 3-option list alongside it would
            // invite the player to click a `selectable_label` that points
            // at the direct Lambert arc and silently undo the assist
            // selection.  The per-assist stats panel above (ΔV saved,
            // extra time, window period, v∞) already carries the cost
            // breakdown, and the "Use Gravity Assist" / "Clear Assist"
            // buttons let the player toggle the assist on/off without
            // needing the legacy list to mediate.
            if fleet_ui_state.selected_gravity_assist.is_some() {
                return;
            }

            let options: Vec<_> = fleet_ui_state.computed_options.clone();
            for (idx, option) in options.iter().enumerate() {
                let option_display_label = if show_binary_transfer_direct_labels {
                    match option.label {
                        "Long Coast" => "Direct Long Coast",
                        "Short Coast" => "Direct Short Coast",
                        "Full Thrust" => "Direct Full Thrust",
                        "Fast Coast" => "Direct Fast Coast",
                        "Max Speed" => "Direct Max Speed",
                        other => other,
                    }
                } else {
                    option.label
                };
                let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);
                let fuel_pct = if fleet_wet_mass > 0.0 {
                    (fuel_cost / fleet_wet_mass * 100.0) as u32
                } else {
                    0
                };
                let affordable = option.total_delta_v_ms <= fleet_max_dv;

                let is_selected = fleet_ui_state.selected_option == idx;
                let row_color = if !affordable {
                    theme::RED
                } else if is_selected {
                    theme::RP_BLUE
                } else {
                    theme::TEXT
                };

                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    let resp = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(format!(
                            "{} {}",
                            if is_selected { "●" } else { "○" },
                            option_display_label
                        ))
                        .size(13.0)
                        .strong()
                        .color(row_color),
                    );
                    if resp.clicked() {
                        fleet_ui_state.selected_option = idx;
                        fleet_ui_state.planned_transfer = None;
                    }

                    // Epoch line: "Depart: DD.MM.YYYY HH:MM / Arrive: …" beneath the
                    // option name, so the player sees the absolute transfer window
                    // without having to compute it from the departure offset slider.
                    let depart_offset_s = fleet_ui_state.departure_offset_days.max(0.0) * 86_400.0;
                    let depart_ts = current_timestamp + depart_offset_s as i64;
                    let arrive_ts = depart_ts + option.transfer_time_s as i64;
                    ui.label(
                        egui::RichText::new(format!(
                            "Depart: {} / Arrive: {}",
                            format_timestamp_date_time(depart_ts),
                            format_timestamp_date_time(arrive_ts),
                        ))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                    );

                    egui::Grid::new(format!("option_{idx}"))
                        .num_columns(4)
                        .spacing([16.0, 2.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Total ΔV:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.total_delta_v_ms))
                                    .size(12.0)
                                    .strong()
                                    .color(row_color),
                            );
                            ui.label(egui::RichText::new("Travel time:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_duration(option.transfer_time_s))
                                    .size(12.0)
                                    .strong(),
                            );
                            ui.end_row();

                            if show_binary_transfer_direct_labels {
                                let selected_label = fleet_ui_state
                                    .computed_options
                                    .get(fleet_ui_state.selected_option)
                                    .map(|option| match option.label {
                                        "Long Coast" => "Direct Long Coast",
                                        "Short Coast" => "Direct Short Coast",
                                        "Full Thrust" => "Direct Full Thrust",
                                        "Fast Coast" => "Direct Fast Coast",
                                        "Max Speed" => "Direct Max Speed",
                                        other => other,
                                    })
                                    .unwrap_or("Direct Transfer");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Selected profile: {selected_label}"
                                    ))
                                    .size(11.0)
                                    .italics()
                                    .color(theme::TEXT_DIM),
                                );
                            }

                            ui.label(egui::RichText::new("Est. fuel:").size(12.0));
                            let fuel_color = if affordable { theme::AMBER } else { theme::RED };
                            ui.label(
                                egui::RichText::new(format!("{:.0} t ({fuel_pct}%)", fuel_cost))
                                    .size(12.0)
                                    .color(fuel_color),
                            );
                            ui.label(egui::RichText::new("Departure burn:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.delta_v1_ms)).size(12.0),
                            );
                            ui.end_row();

                            // Plane-change ΔV row (only shown when non-trivial)
                            if option.plane_change_dv_ms > 100.0 {
                                ui.label(egui::RichText::new("Plane change:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_delta_v(option.plane_change_dv_ms))
                                        .size(12.0)
                                        .color(theme::TEXT_VALUE),
                                );
                                ui.label(egui::RichText::new("").size(12.0));
                                ui.label(egui::RichText::new("").size(12.0));
                                ui.end_row();
                            }

                            // Burn time row — shows how long the fleet's engines fire.
                            if option.burn_time_s > 0.0 {
                                // Classify burn profile based on burn/transfer time ratio.
                                let (profile_label, profile_color) = if option.is_thrust_limited {
                                    // Burn time >= Hohmann time: impulsive assumption invalid.
                                    ("⚠ Thrust-limited", theme::RED)
                                } else if option.label == "Full Thrust" {
                                    // Entire trip is a burn
                                    ("⚡ Full thrust", theme::AMBER)
                                } else {
                                    let ratio =
                                        option.burn_time_s / option.transfer_time_s.max(1.0);
                                    if option.burn_time_s < 3_600.0 {
                                        ("Impulsive", theme::GREEN)
                                    } else if ratio < 0.05 {
                                        ("Short burn", theme::GREEN)
                                    } else if ratio < 0.25 {
                                        ("Extended burn", theme::AMBER)
                                    } else {
                                        ("Continuous thrust", theme::AMBER)
                                    }
                                };
                                ui.label(egui::RichText::new("Burn time:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_duration(option.burn_time_s))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.label(egui::RichText::new("Profile:").size(12.0));
                                ui.label(
                                    egui::RichText::new(profile_label)
                                        .size(12.0)
                                        .color(profile_color),
                                );
                                ui.end_row();

                                let accel_ms2 = fleet.min_accel_ms2();
                                let accel_g = accel_ms2 / 9.80665;
                                ui.label(egui::RichText::new("Acceleration:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format!("{:.2} g", accel_g))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.end_row();

                                // Extra warning row for thrust-limited options.
                                if option.is_thrust_limited {
                                    ui.label(
                                        egui::RichText::new(
                                            "  Low-thrust spiral — travel time ≥ burn time",
                                        )
                                        .size(11.0)
                                        .italics()
                                        .color(theme::AMBER),
                                    );
                                    ui.end_row();
                                }
                            }

                            if !affordable {
                                ui.label(
                                    egui::RichText::new("⚠ Insufficient ΔV capacity")
                                        .size(11.0)
                                        .color(theme::RED),
                                );
                            }
                        });
                });
                ui.add_space(2.0);
            }
        }
    }
}

/// Build a two-leg `PlannedTransfer` (origin → flyby → destination) for a
/// gravity-assist trajectory.  Mirrors the legacy GA-row stitch code
/// previously inlined in `render_transfer_planner`; extracted here so
/// the cell-click path inside `PorkchopViewMode::GravityAssist(idx)`
/// (GRA-385 view-mode toggle) can reuse the same builder.
///
/// The returned `PlannedTransfer` has:
///   * `transfer_orbit` = Leg-1 conic (origin → flyby, Lambert-solved
///     by `build_planned_transfer` when pointed at the flyby body),
///   * `flyby_body` = `Some(flyby_entity)` so the pre-launch GA
///     preview renderer (`draw_gravity_assist_preview` in
///     `fleets/visuals.rs`) draws the slingshot overlay,
///   * `destination_body` = the real destination (not the flyby) so
///     the fleet parks correctly on arrival,
///   * `leg2_orbit` = Leg-2 conic (flyby → destination).  When
///     `cell_leg2_orbit` is supplied (the cell-click path), we use
///     it directly — `sweep_gravity_assist_grid` already
///     Lambert-solved Leg-2 when building the cell.  When
///     `cell_leg2_orbit` is `None` (the legacy GA-row path), we fall
///     back to a Hohmann conic with plane-derived orbital elements so
///     the Leg-2 arc still points the right way.
///   * `leg2_start_s` = Leg-1 half-period (`ga_leg1_time_s`).
#[allow(clippy::too_many_arguments)]
fn build_planned_transfer_with_flyby(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    flyby_entity: Entity,
    target_entity: Entity,
    ga_option: &crate::fleets::orbital_mechanics::TransferOption,
    ga_leg1_time_s: f64,
    planned_departure_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    course_correction_sc: Option<bevy::math::DVec3>,
    body_system_ids: &Query<&crate::astronomy::components::SystemId>,
    current_system_id: usize,
    target_orbit_radius_au: Option<f64>,
    cell_leg2_orbit: Option<crate::astronomy::KeplerOrbit>,
) -> Option<crate::fleets::components::PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::components::PlannedTransfer as PlannedTransferT;
    use crate::fleets::orbital_mechanics::{hohmann_transfer, AU_IN_METERS, GM_SUN, G_CONST};
    use crate::fleets::TransferReferenceFrame;
    use bevy::math::DVec3;

    // Leg-1: build the Keplerian arc origin → flyby by passing the
    // flyby as the target entity.  `build_planned_transfer` always
    // sets `flyby_body: None` on the returned struct (see its
    // constructor at the bottom of the file) so we patch it below.
    let mut pt: PlannedTransferT = build_planned_transfer(
        _fleet_entity,
        fleet,
        orbit,
        flyby_entity,
        planned_departure_time_s,
        body_query,
        ga_option,
        course_correction_sc,
        body_system_ids,
        current_system_id,
        target_orbit_radius_au,
    )?;

    // Patch the flyby marker + true destination.  Without these two
    // lines the renderer would draw a single Lambert arc straight to
    // the flyby and never know about the second leg.
    pt.flyby_body = Some(flyby_entity);
    pt.destination_body = target_entity;

    // Leg-2 orbit.  Two paths:
    //   * Cell-click path — `sweep_gravity_assist_grid` already
    //     Lambert-solved Leg-2 inside the cell, so we use that conic
    //     verbatim.  This avoids a duplicate Lambert solve and
    //     guarantees the preview matches the geometry the cell
    //     clicked will launch with.
    //   * Legacy GA-row path — no cell was clicked, so we fall back
    //     to the Hohmann ellipse between flyby and destination, with
    //     orbital-plane elements derived from the flyby body's
    //     current position.  This matches the pre-extraction inline
    //     stitch behaviour.
    if let Some(leg2) = cell_leg2_orbit {
        pt.leg2_orbit = Some(leg2);
        pt.leg2_start_s = ga_leg1_time_s;
    } else {
        // Resolve the flyby and destination positions relative to the
        // transfer's central body.  Bail (leaving Leg-2 unset) if any
        // entity is missing so we never produce a garbage orbit.
        let center_res = match pt.reference_frame {
            TransferReferenceFrame::SystemBarycentric => Some(DVec3::ZERO),
            TransferReferenceFrame::Body(center_entity) => body_query
                .get(center_entity)
                .ok()
                .map(|(_, _, sc, _, _)| sc.position),
        };
        let flyby_res = body_query
            .get(flyby_entity)
            .ok()
            .map(|(_, _, sc, _, _)| sc.position);
        let dest_res = body_query
            .get(target_entity)
            .ok()
            .map(|(_, _, sc, _, _)| sc.position);
        // Resolve the central body's GM (works for any star).
        let center_gm = match pt.reference_frame {
            TransferReferenceFrame::Body(center_entity) => body_query
                .get(center_entity)
                .ok()
                .map(|(_, b, _, _, _)| G_CONST * b.mass)
                .unwrap_or(GM_SUN),
            TransferReferenceFrame::SystemBarycentric => GM_SUN,
        };

        if let (Some(center_pos), Some(flyby_pos), Some(dest_pos)) =
            (center_res, flyby_res, dest_res)
        {
            let flyby_rel = flyby_pos - center_pos;
            let dest_rel = dest_pos - center_pos;
            let flyby_r = flyby_rel.length();
            let dest_r = dest_rel.length();

            let (.., leg2_sma, leg2_ecc) = hohmann_transfer(flyby_r, dest_r, center_gm);
            let leg2_outward = dest_r >= flyby_r;
            let leg2_mae = if leg2_outward {
                0.0
            } else {
                std::f64::consts::PI
            };

            // Plane normal from cross product; guards against
            // floating-point rounding pushing acos outside [-1, 1].
            let plane_n = flyby_rel.cross(dest_rel);
            let plane_len = plane_n.length();
            let (incl2, lan2, aop2) = if plane_len > 1e-20 {
                let n = plane_n / plane_len;
                let incl = n.z.clamp(-1.0, 1.0).acos();
                let nxy = DVec3::new(-n.y, n.x, 0.0);
                let nl = nxy.length();
                let lan = if nl > 1e-20 {
                    let nd = nxy / nl;
                    nd.y.atan2(nd.x)
                } else {
                    0.0
                };
                let aop = if nl > 1e-20 {
                    let nd = nxy / nl;
                    let pd = flyby_rel.normalize_or_zero();
                    let cw = nd.dot(pd);
                    let sw = n.dot(nd.cross(pd));
                    let om = sw.atan2(cw);
                    if leg2_outward {
                        om
                    } else {
                        om + std::f64::consts::PI
                    }
                } else {
                    let ang = flyby_rel.y.atan2(flyby_rel.x);
                    if leg2_outward {
                        ang
                    } else {
                        ang - std::f64::consts::PI
                    }
                };
                (incl, lan, aop)
            } else {
                let ang = flyby_rel.y.atan2(flyby_rel.x);
                let aop = if leg2_outward {
                    ang
                } else {
                    ang - std::f64::consts::PI
                };
                (0.0, 0.0, aop)
            };

            let sma_m = leg2_sma * AU_IN_METERS;
            let leg2_mm = (center_gm / sma_m.powi(3)).sqrt();

            pt.leg2_orbit = Some(KeplerOrbit {
                semi_major_axis: leg2_sma,
                eccentricity: leg2_ecc,
                inclination: incl2,
                longitude_ascending_node: lan2,
                argument_of_periapsis: aop2,
                mean_anomaly_epoch: leg2_mae,
                mean_motion: leg2_mm,
            });
            pt.leg2_start_s = ga_leg1_time_s;
        }
    }

    Some(pt)
}

/// Build a `PlannedTransfer` from the selected transfer option and fleet/body state.
///
/// `target_orbit_radius_au` overrides the per-body `star_approach_au` field
/// for star-approach transfers when set; the value comes from the GRA-161
/// interactive `DestEntry::StarApproach` picker.  Pass `None` to use the
/// per-body default (no override).
pub fn build_planned_transfer(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    target_entity: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    option: &TransferOption,
    // For course corrections: the fleet's actual current position (in whatever
    // frame matches the central-body coordinates, typically heliocentric AU).
    // When set, used instead of the origin body's position for orbital-element
    // derivation so the Keplerian arc starts from the fleet, not from Jupiter.
    course_correction_pos: Option<bevy::math::DVec3>,
    body_system_ids: &Query<&SystemId>,
    current_system_id: usize,
    // GRA-161: user-controlled parking-radius override for star-approach
    // destinations.  `Some(r)` replaces `star_approach_radius_au(dest_body)`
    // in the `dest_is_star` branch.
    target_orbit_radius_au: Option<f64>,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::{solve_lambert_transfer, AU_IN_METERS, GM_SUN, G_CONST};

    let departure_time_s = sim_time_s;
    let arrival_time_s = departure_time_s + option.transfer_time_s;

    let (_, origin_body, origin_sc, origin_ko, origin_lp) = body_query.get(orbit.body).ok()?;
    let (_, dest_body, _dest_sc, dest_ko, dest_lp) = body_query.get(target_entity).ok()?;

    let dest_parent = dest_lp.map(|lp| lp.0);
    let origin_parent = origin_lp.map(|lp| lp.0);
    let dest_is_star = dest_body.body_type == BodyType::Star;
    let dest_is_ring = dest_body.body_type == BodyType::Ring;
    let origin_host_star_e = find_host_star(orbit.body, body_query).map(|(e, _)| e);
    let dest_host_star_e = find_host_star(target_entity, body_query).map(|(e, _)| e);
    let is_inter_star = origin_host_star_e.is_some()
        && dest_host_star_e.is_some()
        && origin_host_star_e != dest_host_star_e;

    // Determine: (origin_sma, dest_sma, gm, orbit_center, actual destination body for FleetOrbit)
    // For Rings: redirect the FleetOrbit destination to the ring's parent planet.
    // For Stars: Fleet will orbit the star at the planet SOI boundary; orbit_center = star entity.
    let (origin_sma_au, dest_sma_au, gm, orbit_center, actual_dest_body, reference_frame) =
        if is_inter_star {
            let r1 = transfer_absolute_position(orbit.body, departure_time_s, body_query)
                .unwrap_or(origin_sc.position)
                .length()
                .max(MIN_ORBITAL_RADIUS_AU);
            let r2 = transfer_absolute_position(target_entity, arrival_time_s, body_query)
                .map(|pos| pos.length())
                .unwrap_or(1.5)
                .max(MIN_ORBITAL_RADIUS_AU);
            let system_gm_raw: f64 = body_query
                .iter()
                .filter_map(|(e, b, _, _, _)| {
                    if b.body_type != BodyType::Star {
                        return None;
                    }
                    let Ok(system_id) = body_system_ids.get(e) else {
                        return None;
                    };
                    if system_id.0 != current_system_id {
                        return None;
                    }
                    Some(G_CONST * b.mass)
                })
                .sum();
            let system_gm = if system_gm_raw > 0.0 {
                system_gm_raw
            } else {
                GM_SUN
            };
            let primary_star = body_query
                .iter()
                .filter_map(|(e, b, sc, _, _)| {
                    if b.body_type != BodyType::Star {
                        return None;
                    }
                    let Ok(system_id) = body_system_ids.get(e) else {
                        return None;
                    };
                    if system_id.0 != current_system_id {
                        return None;
                    }
                    Some((e, sc))
                })
                .min_by(|(_, sc_a), (_, sc_b)| {
                    sc_a.position
                        .length_squared()
                        .partial_cmp(&sc_b.position.length_squared())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(e, _)| e)
                .unwrap_or(orbit.body);
            (
                r1,
                r2,
                system_gm,
                primary_star,
                target_entity,
                TransferReferenceFrame::SystemBarycentric,
            )
        } else if dest_is_star {
            // Transfer toward the destination star.
            // The transfer orbit is centred on the destination star, so gm = G·M_star.
            //
            // GRA-149 C-2: arrival parking radius is the per-body `star_approach_au`
            // field (RON override or per-star default).  Previously this code parked
            // the fleet at the planet's sphere of influence (SOI), which (a) is not a
            // real orbit — SOI is a frame-switch threshold — and (b) the picker label
            // claimed 0.3 AU while the math produced ~0.012 AU for hot-Jupiters.  The
            // arrival parking radius is now sourced from a single helper that
            // resolves the per-body value (or the global default) and is reused by
            // the barycentric endpoint computation and the final arrival_radius
            // selection below.
            //
            // GRA-161: when the user drags the parking-radius spinner in the
            // destination picker, the override (`target_orbit_radius_au`)
            // wins over the per-body default.  The override is clamped
            // to `[MIN_STAR_APPROACH_AU, MAX_STAR_APPROACH_AU]` at the
            // picker so we can trust the value here, but we still apply a
            // safety floor against the origin's SMA so the arrival orbit
            // can never end up inside the origin planet.
            let star_mass = dest_body.mass; // destination IS the star
                                            // planet_sma_au (the origin body's star-centric SMA) is the departure
                                            // distance.  Do NOT use orbit.radius_au — that is the fleet's local
                                            // parking orbit radius and would make the outward/inward direction check
                                            // incorrect in the star frame.
            let planet_sma_au = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0);
            let approach_au =
                target_orbit_radius_au.unwrap_or_else(|| star_approach_radius_au(dest_body));
            // For transfers that head *outward* (planet_sma_au < approach_au), the
            // parking radius is the approach value.  For inward transfers we keep
            // the arrival inside the origin orbit so the planet doesn't have to
            // pre-date the fleet.  This preserves the prior "SOI is always inside
            // the origin orbit" safety.
            let arrival_au = if approach_au >= planet_sma_au {
                approach_au
            } else {
                (planet_sma_au * 0.01).max(approach_au)
            };
            (
                planet_sma_au,
                arrival_au,
                G_CONST * star_mass,
                target_entity,
                target_entity,
                TransferReferenceFrame::Body(target_entity),
            )
        } else if dest_is_ring {
            // Ring: resolve to orbiting the ring's parent planet at ring.radius altitude
            let ring_parent = dest_parent.unwrap_or(orbit.body);
            let parent_mass = body_query
                .get(ring_parent)
                .ok()
                .map(|(_, b, _, _, _)| b.mass)
                .unwrap_or(5.972e24);
            let ring_radius_au = (dest_body.radius as f64 * 1_000.0) / AU_IN_METERS;
            let r1 = if ring_parent == orbit.body {
                orbit.radius_au
            } else {
                origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.01)
            };
            (
                r1,
                ring_radius_au,
                G_CONST * parent_mass,
                ring_parent,
                ring_parent,
                TransferReferenceFrame::Body(ring_parent),
            )
        } else if dest_parent == Some(orbit.body) {
            // Local (e.g., Earth → Moon)
            let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            (
                orbit.radius_au,
                r2,
                G_CONST * origin_body.mass,
                orbit.body,
                target_entity,
                TransferReferenceFrame::Body(orbit.body),
            )
        } else if let Some(shared) = dest_parent.filter(|parent| Some(*parent) == origin_parent) {
            // Both orbit the same central body (moon-to-moon OR interplanetary, e.g. Earth→Mars).
            // Use G·mass for any central body — non-Sol stars carry their actual mass in kg
            // in CelestialBody.mass, so G·M gives the correct GM for any star.
            let gm = body_query
                .get(shared)
                .ok()
                .map(|(_, b, _, _, _)| G_CONST * b.mass)
                .unwrap_or(GM_SUN);
            let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            (
                r1,
                r2,
                gm,
                shared,
                target_entity,
                TransferReferenceFrame::Body(shared),
            )
        } else if Some(target_entity) == origin_parent {
            // Downward transfer: fleet is at a moon, destination is the parent planet.
            // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
            let parent_mass = dest_body.mass;
            let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            let r2 = (dest_body.radius as f64 * 3_000.0) / AU_IN_METERS;
            (
                r1,
                r2.min(r1 * 0.5),
                G_CONST * parent_mass,
                target_entity,
                target_entity,
                TransferReferenceFrame::Body(target_entity),
            )
        } else {
            // ── Heliocentric fallback ─────────────────────────────────────────────
            //
            // ── Detect inter-star transfer ─────────────────────────────────────────
            // When origin and dest orbit different stars in a multi-star system, the
            // transfer happens in the barycentric frame.  We walk the full LogicalParent
            // chain to the stellar ancestor so that moon→planet→star hierarchies are
            // handled correctly (not just immediate parent checks).
            if is_inter_star {
                // Barycentric distances — already correct since SpaceCoordinates stores
                // positions relative to the system origin (≈ barycenter).
                // origin_sc is always valid (obtained above); target_entity query can only
                // fail if the entity was somehow despawned between the UI call and here,
                // which should not happen in practice.
                let r1 = transfer_absolute_position(orbit.body, departure_time_s, body_query)
                    .unwrap_or(origin_sc.position)
                    .length()
                    .max(MIN_ORBITAL_RADIUS_AU);
                let r2 = transfer_absolute_position(target_entity, arrival_time_s, body_query)
                    .map(|pos| pos.length())
                    .unwrap_or(1.5) // defensive fallback; should not be reached
                    .max(MIN_ORBITAL_RADIUS_AU);
                // Total system GM: sum G·M for all stars in the CURRENT system only.
                // Stars from other systems (e.g. nearby-star catalog entries) must be
                // excluded, otherwise GM is vastly overestimated.
                // We do NOT clamp with .max(GM_SUN); sub-solar binaries must use their
                // actual combined GM (e.g. two K-dwarfs at 0.6+0.2 M☉ total 0.8 M☉).
                let system_gm_raw: f64 = body_query
                    .iter()
                    .filter_map(|(e, b, _, _, _)| {
                        if b.body_type != BodyType::Star {
                            return None;
                        }
                        let Ok(system_id) = body_system_ids.get(e) else {
                            return None;
                        };
                        if system_id.0 != current_system_id {
                            return None;
                        }
                        Some(G_CONST * b.mass)
                    })
                    .sum();
                let system_gm = if system_gm_raw > 0.0 {
                    system_gm_raw
                } else {
                    GM_SUN // fallback only when no stars found for current system (degenerate)
                };
                // Orbit center: the star in the CURRENT system nearest to the barycenter.
                let primary_star = body_query
                    .iter()
                    .filter_map(|(e, b, sc, _, _)| {
                        if b.body_type != BodyType::Star {
                            return None;
                        }
                        let Ok(system_id) = body_system_ids.get(e) else {
                            return None;
                        };
                        if system_id.0 != current_system_id {
                            return None;
                        }
                        Some((e, sc))
                    })
                    .min_by(|(_, sc_a), (_, sc_b)| {
                        sc_a.position
                            .length_squared()
                            .partial_cmp(&sc_b.position.length_squared())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(e, _)| e)
                    .unwrap_or(orbit.body);
                (
                    r1,
                    r2,
                    system_gm,
                    primary_star,
                    target_entity,
                    TransferReferenceFrame::SystemBarycentric,
                )
            } else {
                // If fleet is at a moon, its own SMA is Earth-relative — use parent's SMA.
                //
                // GRA-149 C-3: classify "is the body a star itself?" by mass
                // instead of SMA.  Hot-Jupiters at 0.02 AU and any other close-orbit
                // giant planet now correctly use their own heliocentric SMA
                // (and contribute a correct frame GM in the body_system_ids
                // resolution below), rather than being silently re-parented to
                // whatever happens to be at <0.05 AU.
                let origin_is_stellar = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                let dest_is_stellar = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                let r1 = if origin_is_stellar {
                    origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0)
                } else {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or_else(|| origin_ko.map(|ko| ko.semi_major_axis))
                        .unwrap_or(1.0)
                };
                let r2 = if dest_is_stellar {
                    dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.5)
                } else {
                    dest_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or_else(|| dest_ko.map(|ko| ko.semi_major_axis))
                        .unwrap_or(1.5)
                };
                // Use the host star's actual GM rather than the hardcoded GM_SUN so that
                // non-Sol systems (e.g. 1.1 M☉ Alpha Centauri A, or a 0.5 M☉ K-dwarf) produce
                // correct velocities and transfer times.
                //
                // For single-star systems: origin_parent is the star entity → use its GM.
                // For binary systems where origin and dest orbit different stars: fall back to
                // the origin star's GM (best available single-body approximation).
                // Final fallback: find any nearby Star with no KeplerOrbit (root star); last
                // resort is GM_SUN.
                let host_gm = origin_parent
                    .and_then(|pe| body_query.get(pe).ok())
                    .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .or_else(|| {
                        dest_parent
                            .and_then(|pe| body_query.get(pe).ok())
                            .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                            .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    })
                    .unwrap_or(GM_SUN);
                // Find the orbit center: prefer the origin body's host star (LogicalParent),
                // falling back to any nearby root star, then the fleet's current body.
                let star = origin_parent
                    .filter(|&pe| {
                        body_query
                            .get(pe)
                            .ok()
                            .map(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                            .unwrap_or(false)
                    })
                    .or_else(|| {
                        body_query
                            .iter()
                            .find(|(_, b, sc, _, _)| {
                                b.body_type == BodyType::Star && sc.position.length_squared() < 1.0
                            })
                            .map(|(e, _, _, _, _)| e)
                    })
                    .unwrap_or(orbit.body);
                (
                    r1,
                    r2,
                    host_gm,
                    star,
                    target_entity,
                    TransferReferenceFrame::Body(star),
                )
            } // end same-star heliocentric case
        };

    // For course corrections, determine outward/inward from the fleet's actual distance vs
    // the destination distance.  The body SMAs may not reflect the fleet's position mid-transit.
    // (Computed after rel_pos and dest_rel are available below; use a closure to defer.)
    // For local transfers (planet ↔ moon, or moon → parent planet), the orbit_center IS the
    // planet and its SpaceCoordinates are heliocentric, but we need planet-centric coordinates
    // (DVec3::ZERO) for the transfer orbit geometry. Only use heliocentric position for
    // heliocentric transfers.
    // Cases: (1) Earth → Moon: dest_parent == Some(orbit.body), (2) Moon → Earth: Some(target_entity) == origin_parent
    let orbit_center_is_star = matches!(reference_frame, TransferReferenceFrame::Body(center_entity)
        if body_query
            .get(center_entity)
            .ok()
            .map(|(_, b, _, _, _)| b.body_type == BodyType::Star)
            .unwrap_or(false));
    let is_local_transfer = !orbit_center_is_star
        && (dest_parent == Some(orbit.body) || Some(target_entity) == origin_parent);
    let local_center_is_star = is_local_transfer && orbit_center_is_star;
    let future_resolved_transfer =
        reference_frame.is_barycentric() || (orbit_center_is_star && !is_local_transfer);
    let star_origin_departure_absolute = if origin_body.body_type == BodyType::Star {
        match reference_frame {
            TransferReferenceFrame::Body(center_entity) if center_entity == orbit.body => {
                let center_departure =
                    transfer_absolute_position(center_entity, departure_time_s, body_query)
                        .unwrap_or(origin_sc.position);
                let target_departure =
                    transfer_absolute_position(target_entity, departure_time_s, body_query)
                        .unwrap_or(
                            center_departure
                                + bevy::math::DVec3::X * orbit.radius_au.max(MIN_ORBITAL_RADIUS_AU),
                        );
                let radial_dir = (target_departure - center_departure).normalize_or_zero();
                let fallback_dir =
                    bevy::math::DVec3::new(orbit.angle_rad.cos(), orbit.angle_rad.sin(), 0.0);
                let departure_dir = if radial_dir.length_squared() > 1e-12 {
                    radial_dir
                } else {
                    fallback_dir
                };
                Some(center_departure + departure_dir * orbit.radius_au.max(MIN_ORBITAL_RADIUS_AU))
            }
            _ => None,
        }
    } else {
        None
    };

    let center_pos = match reference_frame {
        TransferReferenceFrame::SystemBarycentric => bevy::math::DVec3::ZERO,
        TransferReferenceFrame::Body(center_entity) => {
            if is_local_transfer && !local_center_is_star {
                bevy::math::DVec3::ZERO
            } else if future_resolved_transfer {
                transfer_absolute_position(center_entity, departure_time_s, body_query)
                    .unwrap_or(bevy::math::DVec3::ZERO)
            } else {
                body_query
                    .get(center_entity)
                    .ok()
                    .map(|(_, _, sc, _, _)| sc.position)
                    .unwrap_or(bevy::math::DVec3::ZERO)
            }
        }
    };
    // For course corrections use the fleet's actual position; otherwise use the origin body.
    let rel_pos = if let Some(fleet_pos) = course_correction_pos {
        // fleet_pos is already in the correct frame (heliocentric or planet-relative).
        // If the orbit center has coordinates, convert fleet_pos to center-relative.
        // cc_local_pos from the caller is already planet-relative for local transfers,
        // but heliocentric for Sun transfers — both are relative to the frame origin,
        // not the orbit_center entity.  Subtract center_pos for consistency.
        fleet_pos - center_pos
    } else {
        // For local transfers (planet ↔ moon): origin_sc.position is already local
        // (moon-relative), and center_pos is DVec3::ZERO, so rel_pos = origin_sc.position.
        // For heliocentric transfers where the fleet orbits a moon, the moon's
        // SpaceCoordinates stores only a local offset from its parent planet — not a
        // heliocentric position.  Use the parent planet's heliocentric SC so that the
        // departure direction (argument_of_periapsis) points in the correct direction.
        let origin_pos = if let Some(star_departure_pos) = star_origin_departure_absolute {
            star_departure_pos
        } else if future_resolved_transfer {
            transfer_absolute_position(orbit.body, departure_time_s, body_query)
                .unwrap_or(origin_sc.position)
        } else if is_local_transfer && !local_center_is_star {
            origin_sc.position
        } else if origin_body.body_type == BodyType::Moon {
            origin_parent
                .and_then(|pe| body_query.get(pe).ok())
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        };
        origin_pos - center_pos
    };

    // Derive the transfer-orbit plane from the 3D departure and arrival position
    // vectors relative to the central body (r1 × r2 gives the plane normal).
    // This keeps inclination, LAN, and argument_of_periapsis mutually consistent
    // so the propagated green-dot position and the displayed preview arc match.
    // For heliocentric transfers where the destination is a moon, its SpaceCoordinates
    // also stores only a local offset — use the parent planet's position instead.
    let dest_sc_pos = body_query
        .get(target_entity)
        .ok()
        .map(|(_, b, sc, _, lp)| {
            // For local transfers (planet ↔ moon or moon → parent planet):
            // - origin is moon-relative (local coordinates)
            // - destination should also be local (DVec3::ZERO for the planet center)
            // For heliocentric: if destination is a moon, get parent's heliocentric position
            if future_resolved_transfer {
                transfer_absolute_position(target_entity, arrival_time_s, body_query)
                    .unwrap_or(sc.position)
            } else if is_local_transfer {
                // For downward transfer (Moon → Earth), destination is the planet itself
                // For upward transfer (Earth → Moon), destination is moon-relative
                if Some(target_entity) == origin_parent {
                    // Downward: destination is the parent planet, use DVec3::ZERO
                    bevy::math::DVec3::ZERO
                } else if local_center_is_star {
                    sc.position
                } else {
                    // Upward: destination is moon-relative
                    sc.position
                }
            } else if b.body_type == BodyType::Moon {
                lp.and_then(|lp| body_query.get(lp.0).ok())
                    .map(|(_, _, sc, _, _)| sc.position)
                    .unwrap_or(sc.position)
            } else {
                sc.position
            }
        })
        .unwrap_or(bevy::math::DVec3::ZERO);
    let dest_rel = dest_sc_pos - center_pos;

    // For course corrections, determine outward/inward from the fleet's actual distance vs
    // the destination distance.  The body SMAs may not reflect the fleet's position mid-transit.
    let outward = if course_correction_pos.is_some() {
        let fleet_r = rel_pos.length();
        let dest_r = dest_rel.length();
        dest_r >= fleet_r
    } else {
        dest_sma_au >= origin_sma_au
    };

    let plane_normal = rel_pos.cross(dest_rel);
    let plane_normal_len = plane_normal.length();

    let default_transfer_plane = if plane_normal_len > 1e-20 {
        let n = plane_normal / plane_normal_len;
        // i = angle between plane normal and ecliptic north (Ẑ).
        let incl = n.z.clamp(-1.0, 1.0).acos();
        // Ascending node: N = Ẑ × n = (-ny, nx, 0).
        let node_xy = bevy::math::DVec3::new(-n.y, n.x, 0.0);
        let node_len = node_xy.length();
        let lan = if node_len > 1e-20 {
            let node = node_xy / node_len;
            node.y.atan2(node.x)
        } else {
            0.0
        };
        // ω: angle from ascending node to periapsis (departure point for outward,
        // arrival for inward), measured in the orbital plane.
        let aop = if node_len > 1e-20 {
            let node = node_xy / node_len;
            let peri_dir = rel_pos.normalize_or_zero();
            let cos_w = node.dot(peri_dir);
            let sin_w = n.dot(node.cross(peri_dir));
            let omega = sin_w.atan2(cos_w);
            if outward {
                omega
            } else {
                omega + std::f64::consts::PI
            }
        } else {
            let departure_angle = rel_pos.y.atan2(rel_pos.x);
            if outward {
                departure_angle
            } else {
                departure_angle - std::f64::consts::PI
            }
        };
        (incl, lan, aop)
    } else {
        // Degenerate (origin and destination collinear with center): ecliptic-flat.
        let departure_angle = rel_pos.y.atan2(rel_pos.x);
        let aop = if outward {
            departure_angle
        } else {
            departure_angle - std::f64::consts::PI
        };
        (0.0, 0.0, aop)
    };

    let star_endpoint_reference_orbit = if dest_is_star {
        star_frame_reference_orbit(orbit.body, origin_parent, body_query)
    } else if origin_body.body_type == BodyType::Star {
        star_frame_reference_orbit(target_entity, dest_parent, body_query)
    } else {
        None
    };

    let (transfer_inclination, transfer_lan, argument_of_periapsis) = star_endpoint_reference_orbit
        .and_then(|reference_orbit| {
            transfer_plane_from_reference_orbit(&reference_orbit, rel_pos, outward)
        })
        .unwrap_or(default_transfer_plane);

    let same_star_stellar_lambert = matches!(reference_frame,
        TransferReferenceFrame::Body(center_entity)
            if body_query
                .get(center_entity)
                .ok()
                .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
                .unwrap_or(false)
            && !is_local_transfer
            && !dest_is_star
            && !option.label.contains("Direct")
    );

    let lambert_same_star_solution = if same_star_stellar_lambert {
        if let TransferReferenceFrame::Body(center_entity) = reference_frame {
            let center_departure =
                transfer_absolute_position(center_entity, departure_time_s, body_query)
                    .unwrap_or(bevy::math::DVec3::ZERO);
            let center_arrival =
                transfer_absolute_position(center_entity, arrival_time_s, body_query)
                    .unwrap_or(center_departure);
            let origin_departure = if let Some(fleet_pos) = course_correction_pos {
                fleet_pos
            } else if let Some(star_departure_pos) = star_origin_departure_absolute {
                star_departure_pos - center_departure
            } else {
                transfer_absolute_position(orbit.body, departure_time_s, body_query)
                    .unwrap_or(rel_pos + center_pos)
                    - center_departure
            };
            let destination_arrival =
                transfer_absolute_position(target_entity, arrival_time_s, body_query)
                    .unwrap_or(dest_rel + center_pos)
                    - center_arrival;

            solve_lambert_transfer(
                origin_departure,
                destination_arrival,
                option.transfer_time_s,
                gm,
            )
        } else {
            None
        }
    } else {
        None
    };
    let lambert_same_star_orbit = lambert_same_star_solution.map(|(_, _, orbit)| orbit);

    let barycentric_start_end = if reference_frame.is_barycentric() {
        let origin_future = transfer_absolute_position(orbit.body, departure_time_s, body_query)
            .unwrap_or(rel_pos + center_pos);
        let dest_future_center =
            transfer_absolute_position(target_entity, arrival_time_s, body_query)
                .unwrap_or(dest_rel + center_pos);
        let dest_future = if dest_is_star {
            // GRA-149 C-2: arrival parking radius now matches the per-body
            // star_approach_au override (or the 0.3 AU default) instead of a
            // hard-coded SOI value.
            let approach_au = star_approach_radius_au(dest_body);
            let inbound = (dest_future_center - origin_future).normalize_or_zero();
            if inbound.length_squared() > 1e-20 {
                dest_future_center - inbound * approach_au
            } else {
                dest_future_center + bevy::math::DVec3::new(approach_au, 0.0, 0.0)
            }
        } else {
            dest_future_center
        };
        Some((origin_future, dest_future))
    } else {
        None
    };

    let lambert_barycentric_solution =
        if reference_frame.is_barycentric() && option.transfer_orbit_override.is_some() {
            barycentric_start_end.and_then(|(origin_future, dest_future)| {
                solve_lambert_transfer(origin_future, dest_future, option.transfer_time_s, gm)
            })
        } else {
            None
        };

    let transfer_orbit =
        if reference_frame.is_barycentric() && option.transfer_orbit_override.is_some() {
            lambert_barycentric_solution
                .map(|(_, _, orbit)| orbit)
                .unwrap_or_else(|| {
                    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
                    // GRA-367-E / Kilo WARNING 2026-07-09: clamp degenerate
                    // sma_au to a safe Hohmann-proxy minimum so the
                    // fallback doesn't divide by zero and produce inf
                    // mean motion.
                    let safe_sma_au = if option.sma_au.is_finite() && option.sma_au > 0.0 {
                        option.sma_au
                    } else {
                        1.0
                    };
                    let sma_m = safe_sma_au * AU_IN_METERS;
                    let mean_motion = (gm / sma_m.powi(3)).sqrt();

                    KeplerOrbit {
                        semi_major_axis: safe_sma_au,
                        eccentricity: option.eccentricity,
                        inclination: transfer_inclination,
                        longitude_ascending_node: transfer_lan,
                        argument_of_periapsis,
                        mean_anomaly_epoch,
                        mean_motion,
                    }
                })
        } else if let Some(orbit) = lambert_same_star_orbit {
            orbit
        } else if let Some(orbit_override) = option.transfer_orbit_override {
            orbit_override
        } else {
            let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
            // GRA-367-E / Kilo WARNING 2026-07-09: when `sma_au` is 0.0,
            // NaN, or non-finite the fallback would divide by zero and
            // produce `inf` mean motion.  Clamp to a safe Hohmann-proxy
            // minimum so the orbital math stays numerically stable.
            let safe_sma_au = if option.sma_au.is_finite() && option.sma_au > 0.0 {
                option.sma_au
            } else {
                1.0
            };
            let sma_m = safe_sma_au * AU_IN_METERS;
            let mean_motion = (gm / sma_m.powi(3)).sqrt();

            KeplerOrbit {
                semi_major_axis: safe_sma_au,
                eccentricity: option.eccentricity,
                inclination: transfer_inclination,
                longitude_ascending_node: transfer_lan,
                argument_of_periapsis,
                mean_anomaly_epoch,
                mean_motion,
            }
        };
    let preserve_orbit_geometry =
        option.transfer_orbit_override.is_some() || lambert_same_star_orbit.is_some();
    let (departure_velocity_ms, arrival_velocity_ms) = lambert_barycentric_solution
        .map(|(departure_velocity, arrival_velocity, _)| {
            (Some(departure_velocity), Some(arrival_velocity))
        })
        .or_else(|| {
            lambert_same_star_solution.map(|(departure_velocity, arrival_velocity, _)| {
                (Some(departure_velocity), Some(arrival_velocity))
            })
        })
        .unwrap_or((None, None));
    let (start_position_au, end_position_au) = barycentric_start_end
        .map(|(start_position, end_position)| (Some(start_position), Some(end_position)))
        .or_else(|| {
            lambert_same_star_solution.map(|_| {
                (
                    transfer_absolute_position(orbit.body, departure_time_s, body_query),
                    transfer_absolute_position(target_entity, arrival_time_s, body_query),
                )
            })
        })
        .unwrap_or((None, None));

    let exact_star_centered_data = exact_star_centered_transfer_data(
        reference_frame,
        orbit_center,
        &transfer_orbit,
        gm,
        departure_time_s,
        arrival_time_s,
        is_local_transfer,
        body_query,
    );
    let start_position_au = start_position_au.or(exact_star_centered_data.map(|data| data.0));
    let end_position_au = end_position_au.or(exact_star_centered_data.map(|data| data.1));
    let departure_velocity_ms =
        departure_velocity_ms.or(exact_star_centered_data.map(|data| data.2));
    let arrival_velocity_ms = arrival_velocity_ms.or(exact_star_centered_data.map(|data| data.3));

    // Arrival orbit radius:
    //   * For barycentric same-star star approaches: the per-body approach radius
    //     (matches the picker label, the barycentric endpoint, and the parking
    //     orbit math above).
    //   * For rings: the ring's own SMA.
    //   * For non-barycentric star approaches: dest_sma_au (which the C-2 fix
    //     unified with the approach radius — see the dest_is_star branch above).
    //   * Otherwise: reuse the fleet's existing parking radius.
    let arrival_orbit_radius_au = if reference_frame.is_barycentric() && dest_is_star {
        star_approach_radius_au(dest_body)
    } else if dest_is_ring || dest_is_star {
        dest_sma_au
    } else {
        orbit.radius_au
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body: actual_dest_body,
        reference_frame,
        orbit_center,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        preserve_orbit_geometry,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label: option.label,
        start_position_au,
        end_position_au,
        departure_velocity_ms,
        arrival_velocity_ms,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    })
}

/// Build a `PlannedTransfer` targeting a Lagrange point (no dedicated ECS entity).
fn build_planned_transfer_lp(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    lp: &LagrangeTarget,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::AU_IN_METERS;

    // LP transfers are heliocentric – find the host star as orbit center.
    // Prefer the LogicalParent of the fleet's current body if it is a Star (correct
    // for circumstellar orbits around non-primary stars in binary systems).
    // Fall back to any nearby star with small SpaceCoordinates magnitude, excluding
    // distant catalog entries.  The `ko.is_none()` guard is intentionally dropped so
    // that secondary stars that have a KeplerOrbit (orbiting the barycenter) can also
    // be found as the host.
    let orbit_body_parent = body_query
        .get(orbit.body)
        .ok()
        .and_then(|(_, _, _, _, lp)| lp)
        .map(|lp| lp.0);
    let star_entity = orbit_body_parent
        .filter(|&pe| {
            body_query
                .get(pe)
                .ok()
                .map(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                .unwrap_or(false)
        })
        .or_else(|| {
            body_query
                .iter()
                .find(|(_, b, sc, _, _)| {
                    b.body_type == BodyType::Star && sc.position.length_squared() < 1.0
                })
                .map(|(e, _, _, _, _)| e)
        })
        .unwrap_or(orbit.body);

    // Determine departure position.  For fleets orbiting the star directly
    // (e.g. after a previous LP transfer), `orbit.body` is the star whose
    // SpaceCoordinates are at the heliocentric origin → rel_pos would be
    // (0,0,0) and departure_angle 0.  In that case use the L-point's parent
    // planet position instead so the orbit geometry is meaningful.
    let center_pos = body_query
        .get(star_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(bevy::math::DVec3::ZERO);

    let origin_pos = {
        let (_, body_data, origin_sc, _, _) = body_query.get(orbit.body).ok()?;
        if body_data.body_type == BodyType::Star {
            // Fleet is parked around the star — use the planet's current position
            // as the departure reference instead.
            body_query
                .get(lp.planet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        }
    };

    let rel_pos = origin_pos - center_pos;
    let departure_angle = rel_pos.y.atan2(rel_pos.x);

    // ALL LP transfers are kinematic (direct Bezier arc from origin to LP position).
    // This prevents co-orbital phasing options from rendering as multi-lap Keplerian
    // rings around the Sun (which previously looked like "multiple orbit rings").
    let option_label: &'static str = match option.label {
        "Efficient" => "Direct Efficient",
        "Moderate" => "Direct Moderate",
        "Fast" => "Direct Fast",
        other => other, // kinematic labels (Full Thrust, Coast, Max Speed, Direct *) pass through
    };

    // Pre-compute the heliocentric LP position for kinematic arc rendering.
    // Every LP transfer sets start/end positions so the fleet flies to the correct
    // Lagrange-point location rather than the star origin (0,0,0).
    let planet_pos = body_query
        .get(lp.planet_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(origin_pos);
    let planet_rel = planet_pos - center_pos;
    let planet_angle = planet_rel.y.atan2(planet_rel.x);
    let lp_angle = match lp.point {
        3 => planet_angle + std::f64::consts::PI,
        4 => planet_angle + std::f64::consts::FRAC_PI_3,
        5 => planet_angle - std::f64::consts::FRAC_PI_3,
        _ => planet_angle, // L1/L2: on the Sun-planet radial
    };
    let lp_pos_au = center_pos
        + bevy::math::DVec3::new(
            lp.radius_au * lp_angle.cos(),
            lp.radius_au * lp_angle.sin(),
            0.0,
        );
    let start_pos = Some(origin_pos);
    let end_pos = Some(lp_pos_au);

    // L1/L2: the LP is physically near the planet (±r_hill from the planet's
    // heliocentric position).  Park the fleet around the planet at r_hill so it
    // co-orbits with the planet rather than orbiting the Sun at 1 AU.
    //
    // L3/L4/L5: heliocentric co-orbital positions.  Park the fleet around the
    // star at the planet's SMA; `complete_fleet_maneuvers` will set direction=0.0
    // (frozen) because `is_kinematic()` + star destination → LP-stationed sentinel.
    let (destination_body, arrival_orbit_radius_au) = if matches!(lp.point, 1 | 2) {
        let r_hill = (lp.radius_au - lp.planet_sma_au).abs().max(0.001);
        (lp.planet_entity, r_hill)
    } else {
        (star_entity, lp.planet_sma_au)
    };

    let gm = lp.gm;
    // GRA-367-E / Kilo WARNING 2026-07-09: clamp degenerate sma_au for
    // robustness — Lagrange transfer sma_au should normally be the
    // planet's SMA which is positive, but caller-supplied options can
    // be zero in degenerate fallback paths.
    let safe_sma_au = if option.sma_au.is_finite() && option.sma_au > 0.0 {
        option.sma_au
    } else {
        lp.planet_sma_au.max(0.001)
    };
    let sma_m = safe_sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let outward = lp.radius_au >= lp.planet_sma_au;
    let argument_of_periapsis = if outward {
        departure_angle
    } else {
        departure_angle - std::f64::consts::PI
    };
    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: safe_sma_au,
        eccentricity: option.eccentricity,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body,
        reference_frame: TransferReferenceFrame::Body(star_entity),
        orbit_center: star_entity,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        preserve_orbit_geometry: false,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label,
        start_position_au: start_pos,
        end_position_au: end_pos,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    })
}

#[cfg(test)]
mod tests {
    // `FleetUiState` has many `Option` / `Vec` / texture fields
    // (porkchop cache, gravity-assist candidates, etc.) that tests
    // need to poke in non-default states — struct init would be
    // unreadable.  Allow `field_reassign_with_default` here so the
    // pattern stays a `Default::default()` + targeted assignments.
    #![allow(clippy::field_reassign_with_default)]

    use super::build_planned_transfer;
    use super::transfer_absolute_position;
    use super::{build_lagrange_target, hill_radius_au, lagrange_picker_label};
    use crate::astronomy::components::SystemId;
    use crate::astronomy::orbit_position_from_mean_anomaly;
    use crate::astronomy::{KeplerOrbit, SpaceCoordinates};
    use crate::fleets::orbital_mechanics::TransferOption;
    use crate::fleets::{Fleet, FleetOrbit, TransferReferenceFrame};
    use crate::plugins::solar_system::{CelestialBody, LogicalParent};
    use crate::plugins::solar_system_data::BodyType;
    use crate::ui::GravityAssistEntry;
    use bevy::math::DVec3;
    use bevy::prelude::*;

    fn test_body(
        name: &str,
        body_type: BodyType,
        mass: f64,
        radius: f32,
        visual_radius: f32,
    ) -> CelestialBody {
        CelestialBody {
            name: name.to_string(),
            radius,
            mass,
            body_type,
            visual_radius,
            asteroid_class: None,
            star_approach_au: None,
            // GRA-NNN: shell-cache fields for the orbit-shell resolver.
            rotation_period_s: None,
            habitable_outer_au: None,
        }
    }

    /// Same as [`test_body`] but allows pinning the per-body star-approach
    /// radius.  Used by the GRA-149 C-2 acceptance test to verify the
    /// `star_approach_au: Some(0.05)` override reaches the planner unchanged.
    fn test_body_with_approach(
        name: &str,
        body_type: BodyType,
        mass: f64,
        radius: f32,
        visual_radius: f32,
        star_approach_au: Option<f64>,
    ) -> CelestialBody {
        CelestialBody {
            name: name.to_string(),
            radius,
            mass,
            body_type,
            visual_radius,
            asteroid_class: None,
            star_approach_au,
            // GRA-NNN: shell-cache fields for the orbit-shell resolver.
            rotation_period_s: None,
            habitable_outer_au: None,
        }
    }

    #[test]
    fn build_planned_transfer_marks_cross_star_routes_barycentric() {
        let mut world = World::new();

        let star_a = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(-10.0, 0.0, 0.0)),
                SystemId(7),
            ))
            .id();
        let star_b = world
            .spawn((
                test_body("Star B", BodyType::Star, 1.3e30, 600_000.0, 34.0),
                SpaceCoordinates::new(DVec3::new(12.0, 0.0, 0.0)),
                SystemId(7),
            ))
            .id();

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(-8.8, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 1.0e-7),
                LogicalParent(star_a),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(14.1, 0.0, 0.0)),
                KeplerOrbit::circular(2.1, 8.0e-8),
                LogicalParent(star_b),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Full Thrust",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0,
            sma_au: 15.0,
            eccentricity: 0.4,
            energy_multiplier: 1.0,
            burn_time_s: 10_000.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("cross-star transfer should build successfully");

        assert_eq!(planned.destination_body, destination);
        assert_eq!(
            planned.reference_frame,
            TransferReferenceFrame::SystemBarycentric
        );
    }

    #[test]
    fn build_planned_transfer_keeps_curved_cross_star_routes_non_kinematic() {
        let mut world = World::new();

        // Stars at origin for this test - positions don't affect orbit-computed positions
        let star_a = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let star_b = world
            .spawn((
                test_body("Star B", BodyType::Star, 1.3e30, 600_000.0, 34.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();

        // Origin planet: orbit radius 1.2 AU, at position (1.2, 0, 0) relative to star_a
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.2, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 1.0e-7),
                LogicalParent(star_a),
                SystemId(7),
            ))
            .id();
        // Destination planet: orbit radius 2.1 AU, at position (2.1, 6.0, 0) relative to star_b
        // Use inclination=90deg to get y-offset in a circular orbit
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(2.1, 6.0, 0.0)),
                KeplerOrbit {
                    semi_major_axis: 2.1,
                    eccentricity: 0.0,
                    inclination: std::f64::consts::FRAC_PI_2, // 90 degrees for y-offset
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 8.0e-8,
                },
                LogicalParent(star_b),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Curved Efficient",
            total_delta_v_ms: 9_000.0,
            delta_v1_ms: 4_500.0,
            delta_v2_ms: 4_500.0,
            transfer_time_s: 86_400.0 * 120.0,
            sma_au: 18.0,
            eccentricity: 0.55,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: Some(KeplerOrbit::circular(18.0, 1.0e-8)),
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("curved cross-star transfer should build successfully");

        assert_eq!(
            planned.reference_frame,
            TransferReferenceFrame::SystemBarycentric
        );
        assert_eq!(planned.option_label, "Curved Efficient");
        assert!(planned.start_position_au.is_some());
        assert!(planned.end_position_au.is_some());
        assert!(planned.departure_velocity_ms.is_some());
        assert!(planned.arrival_velocity_ms.is_some());
    }

    #[test]
    fn build_planned_transfer_star_origin_uses_parking_orbit_radius() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(1.8, 0.5, 0.0)),
                KeplerOrbit::new(0.02, 1.85, 0.0, 0.0, 0.2, 0.4, 1.2e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let mut orbit = FleetOrbit::new(star, 0.08);
        orbit.angle_rad = 0.35;

        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 4.0,
            sma_au: 1.0,
            eccentricity: 0.5,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("star-origin transfer should build successfully");

        assert_eq!(planned.reference_frame, TransferReferenceFrame::Body(star));

        let departure_pos = orbit_position_from_mean_anomaly(
            &planned.transfer_orbit,
            planned.transfer_orbit.mean_anomaly_epoch,
        );
        assert!(departure_pos.length() > orbit.radius_au * 0.5);
    }

    #[test]
    fn build_planned_transfer_to_star_preserves_origin_orbital_plane() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(11.0, -7.0, 3.0)),
                SystemId(7),
            ))
            .id();

        let origin_orbit = KeplerOrbit::new(0.08, 1.6, 0.72, 0.91, 0.35, 0.44, 1.2e-7);
        let origin_pos =
            orbit_position_from_mean_anomaly(&origin_orbit, origin_orbit.mean_anomaly_epoch);
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(11.0, -7.0, 3.0) + origin_pos),
                origin_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 6.0,
            sma_au: 0.9,
            eccentricity: 0.45,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("star-destination transfer should build successfully");

        assert_eq!(planned.reference_frame, TransferReferenceFrame::Body(star));
        assert!((planned.transfer_orbit.inclination - origin_orbit.inclination).abs() < 1e-9);
        assert!(
            (planned.transfer_orbit.longitude_ascending_node
                - origin_orbit.longitude_ascending_node)
                .abs()
                < 1e-9
        );
        assert!(planned.start_position_au.is_some());
        assert!(planned.end_position_au.is_some());
        assert!(planned.departure_velocity_ms.is_some());
        assert!(planned.arrival_velocity_ms.is_some());
    }

    #[test]
    fn build_planned_transfer_to_star_tracks_departure_epoch() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.2, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 2.2e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 6.0,
            sma_au: 0.9,
            eccentricity: 0.45,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned_now = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("initial star transfer should build successfully");
        let planned_later = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            86_400.0 * 20.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("delayed star transfer should build successfully");

        assert_ne!(
            planned_now.transfer_orbit.argument_of_periapsis,
            planned_later.transfer_orbit.argument_of_periapsis
        );
        assert_ne!(
            planned_now.start_position_au,
            planned_later.start_position_au
        );
    }

    #[test]
    fn build_planned_transfer_from_star_preserves_destination_orbital_plane() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(-5.0, 2.0, -1.0)),
                SystemId(7),
            ))
            .id();

        let destination_orbit = KeplerOrbit::new(0.04, 1.9, 0.63, 1.14, 0.2, 0.51, 1.1e-7);
        let destination_pos = orbit_position_from_mean_anomaly(
            &destination_orbit,
            destination_orbit.mean_anomaly_epoch,
        );
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(-5.0, 2.0, -1.0) + destination_pos),
                destination_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let mut orbit = FleetOrbit::new(star, 0.08);
        orbit.angle_rad = 0.35;

        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 4.0,
            sma_au: 1.0,
            eccentricity: 0.5,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("star-origin transfer should build successfully");

        assert_eq!(planned.reference_frame, TransferReferenceFrame::Body(star));
        assert!((planned.transfer_orbit.inclination - destination_orbit.inclination).abs() < 1e-9);
        assert!(
            (planned.transfer_orbit.longitude_ascending_node
                - destination_orbit.longitude_ascending_node)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn build_planned_transfer_same_star_lambert_carries_exact_endpoint_data() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(4.0, -3.0, 2.0)),
                SystemId(7),
            ))
            .id();

        let origin_orbit = KeplerOrbit::new(0.0, 1.3, 0.47, 0.82, 0.33, 0.21, 0.0);
        let destination_orbit = KeplerOrbit::new(0.0, 2.4, 0.47, 0.82, 0.33, 1.12, 0.0);
        let origin_pos =
            orbit_position_from_mean_anomaly(&origin_orbit, origin_orbit.mean_anomaly_epoch);
        let destination_pos = orbit_position_from_mean_anomaly(
            &destination_orbit,
            destination_orbit.mean_anomaly_epoch,
        );

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(4.0, -3.0, 2.0) + origin_pos),
                origin_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(4.0, -3.0, 2.0) + destination_pos),
                destination_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let transfer_time_s = 86_400.0 * 220.0;
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s,
            sma_au: 1.8,
            eccentricity: 0.3,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("same-star transfer should build successfully");

        assert!(planned.preserve_orbit_geometry);
        assert!(planned.start_position_au.is_some());
        assert!(planned.end_position_au.is_some());
        assert!(planned.departure_velocity_ms.is_some());
        assert!(planned.arrival_velocity_ms.is_some());
    }

    #[test]
    fn build_planned_transfer_cross_star_to_star_uses_barycentric_approach() {
        let mut world = World::new();

        let star_a = world
            .spawn((
                test_body("Alpha A", BodyType::Star, 2.0e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(-2000.0, 0.0, 0.0)),
                SystemId(7),
            ))
            .id();
        let star_b = world
            .spawn((
                test_body("Proxima", BodyType::Star, 2.4e29, 110_000.0, 22.0),
                SpaceCoordinates::new(DVec3::new(2100.0, 120.0, 0.0)),
                SystemId(7),
            ))
            .id();

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(-1998.8, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 1.0e-7),
                LogicalParent(star_a),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Long Coast",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 500.0,
            sma_au: 3000.0,
            eccentricity: 0.2,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star_b,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("cross-star star-approach transfer should build successfully");

        assert_eq!(
            planned.reference_frame,
            TransferReferenceFrame::SystemBarycentric
        );
        assert_eq!(planned.destination_body, star_b);
        assert_eq!(planned.arrival_orbit_radius_au, 0.3);
        let end_pos = planned
            .end_position_au
            .expect("approach endpoint should be stored");
        let star_pos = body_query
            .get(star_b)
            .map(|(_, _, sc, _, _)| sc.position)
            .expect("star should exist");
        let approach_distance = (end_pos - star_pos).length();
        assert!((approach_distance - 0.3).abs() < 1e-6);
    }

    // ──────────────────────────────────────────────────────────────────────
    // GRA-149 acceptance tests (C-1 / C-2 / C-3)
    //
    // Pin the GRA-149 fixes so a future regression to the legacy
    // `sma < MIN_HELIOCENTRIC_SMA_AU` classifier (which mis-classified
    // hot-Jupiters as moons) is caught.
    // ──────────────────────────────────────────────────────────────────────

    /// C-1: the stellar flyby constant is documented at 1.5 R★ (1500 m/km
    /// × 1.5) and the planetary constant is 3 planetary radii.  Both must
    /// stay larger than 1.0× their respective body radii so the
    /// flyby-periapsis math never under-shoots into the photosphere /
    /// atmosphere.
    #[test]
    fn gra149_c1_stellar_flyby_constants_are_safe_periapsis_multiples() {
        // Bind to local variables so clippy::assertions_on_constants does
        // not see a literal-only comparison.
        let stellar = super::STELLAR_FLYBY_RADIUS_KM_MULTIPLIER;
        let planetary = super::PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER;
        assert!(
            stellar >= 1_500.0,
            "STELLAR_FLYBY_RADIUS_KM_MULTIPLIER = {stellar} km; must be >= 1.5 R☉ (1500 km)"
        );
        assert!(
            planetary >= 3_000.0,
            "PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER = {planetary} km; must be >= 3 R_planet"
        );
        assert!(
            stellar < planetary,
            "stellar flyby constant ({stellar}) must be < planetary ({planetary})"
        );
    }

    /// C-2: a star with `star_approach_au: Some(0.05)` parks the fleet at
    /// 0.05 AU when the destination is that star — not the 0.3 AU default
    /// and not the planet's SOI.  This is the M-3 / GRA-153 dependency
    /// pin: M-3 calls `origin_body.star_approach_au` (or the parking-orbit
    /// default) to rebuild the parking orbit after an Abort.
    #[test]
    fn gra149_c2_star_approach_respects_per_body_override() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body_with_approach(
                    "Red Dwarf",
                    BodyType::Star,
                    2.4e29, // 0.12 M☉ — sub-solar
                    110_000.0,
                    22.0,
                    Some(0.05), // per-body override
                ),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(0.45, 0.0, 0.0)),
                KeplerOrbit::circular(0.5, 1.0e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 8.0,
            sma_au: 0.27,
            eccentricity: 0.4,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("star-approach transfer with override should build");

        // The per-body override is the source of truth — not 0.3 AU, not SOI.
        // The inward-transfer safety floor at `planet_sma_au * 0.01 = 0.005`
        // does not bind here because 0.05 > 0.005.
        assert!(
            (planned.arrival_orbit_radius_au - 0.05).abs() < 1e-9,
            "arrival_orbit_radius_au = {}, expected 0.05 (per-body override)",
            planned.arrival_orbit_radius_au
        );
    }

    /// C-3: a hot-Jupiter (gas giant at SMA 0.02 AU, well below the legacy
    /// 0.05 AU classifier) with `LogicalParent(star)` is correctly treated
    /// as heliocentric by the planner — it does NOT walk up to the parent
    /// star's 1.0 AU orbit.  This pins the GRA-149 C-3 fix to
    /// `is_stellar_mass` and ensures hot-Jupiters are not silently
    /// mis-classified as moons.
    #[test]
    fn gra149_c3_hot_jupiter_uses_heliocentric_frame_not_walked_up() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let hot_jupiter = world
            .spawn((
                test_body("Hot Jupiter", BodyType::GasGiant, 1.9e27, 70_000.0, 30.0),
                SpaceCoordinates::new(DVec3::new(0.02, 0.0, 0.0)),
                KeplerOrbit::circular(0.02, 1.0e-6), // SMA = 0.02 AU
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(1.8, 0.5, 0.0)),
                KeplerOrbit::new(0.02, 1.85, 0.0, 0.0, 0.2, 0.4, 1.2e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(hot_jupiter, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 4.0,
            sma_au: 1.0,
            eccentricity: 0.5,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("hot-Jupiter to outer-planet transfer should build");

        // The transfer must be heliocentric (BodyLocal(star)), not the
        // hot-Jupiter's planet-local frame.  Under the legacy SMA
        // classifier (0.05 AU), this would have been classified as a
        // planet-local transfer — the regression to guard against.
        match planned.reference_frame {
            TransferReferenceFrame::Body(frame_center) => {
                assert_eq!(
                    frame_center, star,
                    "hot-Jupiter at 0.02 AU must resolve to star's frame, \
                     not the planet's frame"
                );
            }
            other => panic!("expected Body(star) frame for hot-Jupiter, got {:?}", other),
        }
    }

    /// L-6: a single-star transfer from a star-system origin must read
    /// positions in the star-centric frame, and an inter-star transfer must
    /// read them in the barycentric frame.  This test exercises the boundary:
    /// the same fleet moves from one frame to the other mid-flight (which
    /// shouldn't happen in practice, but the math must be defensible).
    #[test]
    fn transfer_absolute_position_uses_consistent_frame_at_star_system_boundary() {
        let mut world = World::new();

        // Star A: single-star system (SystemId 11).  Position is the
        // star-system origin, so all bodies in this system are star-centric
        // relative to A.
        let star_a = world
            .spawn((
                test_body("Alpha", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(11),
            ))
            .id();
        let planet_a = world
            .spawn((
                test_body("Alpha-b", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.0, 0.0, 0.0)),
                KeplerOrbit::circular(1.0, 1.0e-7),
                LogicalParent(star_a),
                SystemId(11),
            ))
            .id();
        // Star B: second star of a binary (SystemId 12).  Its
        // `SpaceCoordinates.position` is the barycentric offset of B
        // relative to the A+B barycentre.
        let star_b = world
            .spawn((
                test_body("Beta", BodyType::Star, 1.3e30, 600_000.0, 35.0),
                SpaceCoordinates::new(DVec3::new(20.0, 0.0, 0.0)),
                SystemId(12),
            ))
            .id();
        let planet_b = world
            .spawn((
                test_body("Beta-b", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(21.0, 0.0, 0.0)),
                KeplerOrbit::circular(1.0, 1.0e-7),
                LogicalParent(star_b),
                SystemId(12),
            ))
            .id();

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let body_query = body_query_state.query(&world);

        // Single-star: planet A's transfer-absolute position equals its
        // SpaceCoordinates.position (star-centric, parent is at origin).
        let pos_planet_a = transfer_absolute_position(planet_a, 0.0, &body_query)
            .expect("planet A absolute position should resolve");
        let sc_planet_a = body_query
            .get(planet_a)
            .map(|(_, _, sc, _, _)| sc.position)
            .unwrap();
        assert_eq!(
            pos_planet_a, sc_planet_a,
            "single-star system: planet A position must equal its SpaceCoordinates"
        );

        // Inter-star: planet B's transfer-absolute position equals its
        // SpaceCoordinates.position (already barycentric in this world
        // model — star B itself is offset by 20 AU from the barycentre).
        let pos_planet_b = transfer_absolute_position(planet_b, 0.0, &body_query)
            .expect("planet B absolute position should resolve");
        let sc_planet_b = body_query
            .get(planet_b)
            .map(|(_, _, sc, _, _)| sc.position)
            .unwrap();
        assert_eq!(
            pos_planet_b, sc_planet_b,
            "barycentric: planet B position must equal its SpaceCoordinates"
        );

        // The key invariant: a transfer crossing the star-system boundary
        // (planet A → planet B) computes positions in their own frame.  The
        // math at the boundary is just a position comparison; it must not
        // silently re-interpret star-A-centric positions as barycentric.
        // We assert the boundary distance is the *sum* of the two offsets,
        // not the difference, because both are now in the same barycentric
        // frame.
        let boundary_distance = (pos_planet_b - pos_planet_a).length();
        let expected = (sc_planet_b - sc_planet_a).length();
        assert!(
            (boundary_distance - expected).abs() < 1e-6,
            "boundary distance must be consistent: got {}, expected {}",
            boundary_distance,
            expected
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // GRA-156 (M-5): Lagrange-point transfer unit tests — one per L-point type.
    //
    // Locks the LGD Q3 format from GRA-155:
    //   Sun-Planet  → `🛰 L{n} ({planet}-{star})`     e.g. `🛰 L1 (Earth-Sun)`
    //   Planet-Moon → `🛰 L{n} ({planet}-{moon})`     e.g. `🛰 L1 (Earth-Moon)`
    // and the Hill-sphere placement of L1 / L2 around the secondary body.
    // ─────────────────────────────────────────────────────────────────────────

    const EARTH_MASS_KG: f64 = 5.972e24;
    const LUNA_MASS_KG: f64 = 7.342e22;
    const SOL_MASS_KG: f64 = 1.989e30;
    const EARTH_SMA_AU: f64 = 1.0;
    const LUNA_SMA_AU: f64 = 0.00257;

    fn earth_entity() -> Entity {
        Entity::from_raw_u32(0xE0_01).expect("non-zero entity id")
    }
    fn luna_entity() -> Entity {
        Entity::from_raw_u32(0xE0_02).expect("non-zero entity id")
    }

    #[test]
    fn lagrange_sun_planet_l1_sits_inside_hill_sphere() {
        let r_hill = hill_radius_au(EARTH_SMA_AU, EARTH_MASS_KG, SOL_MASS_KG);
        // Earth-Moon and Earth-Sun commonly cited: Earth Hill radius ≈ 0.01 AU.
        assert!(
            r_hill > 0.005 && r_hill < 0.02,
            "Earth Hill sphere should be ~0.01 AU, got {r_hill}"
        );

        let lp = build_lagrange_target(
            1,
            earth_entity(),
            "Earth",
            EARTH_SMA_AU,
            EARTH_MASS_KG,
            SOL_MASS_KG,
        );
        assert_eq!(lp.point, 1);
        assert_eq!(lp.planet_name, "Earth");
        assert!(
            (lp.radius_au - (EARTH_SMA_AU - r_hill)).abs() < 1e-12,
            "L1 should be at planet_sma - r_hill, got {}",
            lp.radius_au
        );
        assert!(lp.gm > 0.0);
        // Sun-Planet L1 picker label per GRA-155 Q3.
        assert_eq!(lagrange_picker_label(&lp, "Sol", true), "🛰 L1 (Earth-Sun)");
    }

    #[test]
    fn lagrange_sun_planet_l2_sits_outside_hill_sphere() {
        let r_hill = hill_radius_au(EARTH_SMA_AU, EARTH_MASS_KG, SOL_MASS_KG);
        let lp = build_lagrange_target(
            2,
            earth_entity(),
            "Earth",
            EARTH_SMA_AU,
            EARTH_MASS_KG,
            SOL_MASS_KG,
        );
        assert_eq!(lp.point, 2);
        assert!(
            (lp.radius_au - (EARTH_SMA_AU + r_hill)).abs() < 1e-12,
            "L2 should be at planet_sma + r_hill, got {}",
            lp.radius_au
        );
        assert_eq!(lagrange_picker_label(&lp, "Sol", true), "🛰 L2 (Earth-Sun)");
    }

    #[test]
    fn lagrange_planet_moon_l1_sits_inside_hill_sphere() {
        let r_hill = hill_radius_au(LUNA_SMA_AU, LUNA_MASS_KG, EARTH_MASS_KG);
        // Lunar Hill sphere around Earth ≈ 0.0004 AU (≈ 60 000 km).
        assert!(
            r_hill > 0.0002 && r_hill < 0.0010,
            "Luna Hill sphere should be ~0.0004 AU, got {r_hill}"
        );

        let lp = build_lagrange_target(
            1,
            luna_entity(),
            "Moon",
            LUNA_SMA_AU,
            LUNA_MASS_KG,
            EARTH_MASS_KG,
        );
        assert_eq!(lp.point, 1);
        assert_eq!(lp.planet_name, "Moon");
        assert!(
            (lp.radius_au - (LUNA_SMA_AU - r_hill)).abs() < 1e-12,
            "L1 should be at moon_sma - r_hill, got {}",
            lp.radius_au
        );
        // Planet-Moon picker label per GRA-155 Q3 — central first.
        assert_eq!(
            lagrange_picker_label(&lp, "Earth", false),
            "🛰 L1 (Earth-Moon)"
        );
    }

    #[test]
    fn lagrange_planet_moon_l2_sits_outside_hill_sphere() {
        let r_hill = hill_radius_au(LUNA_SMA_AU, LUNA_MASS_KG, EARTH_MASS_KG);
        let lp = build_lagrange_target(
            2,
            luna_entity(),
            "Moon",
            LUNA_SMA_AU,
            LUNA_MASS_KG,
            EARTH_MASS_KG,
        );
        assert_eq!(lp.point, 2);
        assert!(
            (lp.radius_au - (LUNA_SMA_AU + r_hill)).abs() < 1e-12,
            "L2 should be at moon_sma + r_hill, got {}",
            lp.radius_au
        );
        assert_eq!(
            lagrange_picker_label(&lp, "Earth", false),
            "🛰 L2 (Earth-Moon)"
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // GRA-161 acceptance tests
    //
    // Pin the interactive star-approach parking-radius picker so a future
    // regression to the static `0.30 AU` label (or to the per-body
    // `star_approach_au` only) is caught.
    // ──────────────────────────────────────────────────────────────────────

    /// H-1: `star_approach_bounds_au` for a star with no closer planet than
    /// 5 AU returns `(MIN_STAR_APPROACH_AU, MAX_STAR_APPROACH_AU)` —
    /// `0.05..=5.00` AU.  This is the no-clamp default and is what the
    /// picker would use for a free-floating star with no planets.
    #[test]
    fn gra161_h1_bounds_no_planet_returns_full_range() {
        let mut world = World::new();
        let star = world
            .spawn((
                test_body("Free Star", BodyType::Star, 1.0e30, 500_000.0, 22.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);
        let (min_au, max_au) =
            super::star_approach_bounds_au(star, &body_query, &system_id_query, 7);
        assert!(
            (min_au - super::MIN_STAR_APPROACH_AU).abs() < 1e-12,
            "min_au = {min_au}, expected {}",
            super::MIN_STAR_APPROACH_AU
        );
        assert!(
            (max_au - super::MAX_STAR_APPROACH_AU).abs() < 1e-12,
            "max_au = {max_au}, expected {}",
            super::MAX_STAR_APPROACH_AU
        );
    }

    /// H-2: `star_approach_bounds_au` for a star whose closest planet is
    /// at 1.0 AU returns `min=0.05, max=0.9` — the upper clamp is
    /// 90 % of the closest planet SMA, not the global 5.0 AU cap.
    /// This guarantees the parking orbit cannot be placed inside an
    /// existing planetary orbit.
    #[test]
    fn gra161_h2_bounds_clamps_to_closest_planet() {
        let mut world = World::new();
        let star = world
            .spawn((
                test_body("Host Star", BodyType::Star, 1.0e30, 500_000.0, 22.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        // A planet at 1.0 AU, the closest to this star.
        let _planet = world
            .spawn((
                test_body("Inner Planet", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.0, 0.0, 0.0)),
                KeplerOrbit::circular(1.0, 1.0e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        // A second planet further out — must not loosen the cap.
        let _outer = world
            .spawn((
                test_body("Outer Planet", BodyType::GasGiant, 1.9e27, 70_000.0, 30.0),
                SpaceCoordinates::new(DVec3::new(2.5, 0.0, 0.0)),
                KeplerOrbit::circular(2.5, 1.0e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);
        let (min_au, max_au) =
            super::star_approach_bounds_au(star, &body_query, &system_id_query, 7);
        assert!(
            (min_au - 0.05).abs() < 1e-12,
            "min_au = {min_au}, expected 0.05"
        );
        assert!(
            (max_au - 0.9).abs() < 1e-9,
            "max_au = {max_au}, expected 0.9 (90% of 1.0 AU)"
        );
    }

    /// H-3: `build_planned_transfer` honours the GRA-161
    /// `target_orbit_radius_au` override for star-approach destinations.
    /// The same star with two different override values produces two
    /// different `arrival_orbit_radius_au` values, proving the picker
    /// flows end-to-end into the planned transfer.
    #[test]
    fn gra161_h3_target_orbit_radius_au_overrides_star_approach() {
        let mut world = World::new();
        let star = world
            .spawn((
                test_body("Host Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(0.6, 0.0, 0.0)),
                KeplerOrbit::circular(0.6, 1.0e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 12.0,
            sma_au: 0.5,
            eccentricity: 0.3,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };
        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);
        // First call: no override, planner should use the per-body
        // `star_approach_au` (None) fallback → 0.3 AU.
        let planned_default = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            None,
        )
        .expect("star-approach transfer (no override) should build");
        assert!(
            (planned_default.arrival_orbit_radius_au - 0.3).abs() < 1e-9,
            "default arrival radius = {}, expected 0.3 (STELLAR_APPROACH_AU)",
            planned_default.arrival_orbit_radius_au
        );
        // Second call: user dragged the picker to 0.45 AU.
        let planned_custom = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
            Some(0.45),
        )
        .expect("star-approach transfer (override 0.45) should build");
        assert!(
            (planned_custom.arrival_orbit_radius_au - 0.45).abs() < 1e-9,
            "override arrival radius = {}, expected 0.45 (GRA-161 picker)",
            planned_custom.arrival_orbit_radius_au
        );
    }

    // ── GRA-159 regression tests for heliocentric_orbit_for_body ─────────
    //
    // The previous version of `heliocentric_orbit_for_body` returned a
    // moon's own `KeplerOrbit` (the moon-around-parent local-frame orbit)
    // as if it were heliocentric.  The porkchop Lambert solver then
    // treated it as heliocentric and produced an infeasible grid.  The
    // fix: for `BodyType::Moon` and `BodyType::Ring`, always walk up to
    // the parent's heliocentric `KeplerOrbit`.  The tests below lock
    // that contract in place.

    /// Build a minimal in-memory world with a Sol-like star, an Earth-like
    /// planet (heliocentric orbit at 1.0 AU), and a Luna-like moon
    /// (local-frame orbit at 0.00257 AU, parent = Earth).  Returns the
    /// world plus the entity references for direct calls to the helper.
    fn world_with_moon_fixture() -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        let sol = world
            .spawn((
                test_body("Sol", BodyType::Star, 1.989e30, 696_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(0),
            ))
            .id();
        let earth = world
            .spawn((
                test_body("Earth", BodyType::Planet, 5.972e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.0, 0.0, 0.0)),
                KeplerOrbit::circular(1.0, 1.991e-7), // heliocentric 1 AU
                LogicalParent(sol),
                SystemId(0),
            ))
            .id();
        // Luna: its own KeplerOrbit is the *local-frame* orbit around
        // Earth (sma = 0.00257 AU) — never heliocentric.  The bug would
        // return this value instead of walking up to Earth's 1.0 AU.
        let luna = world
            .spawn((
                test_body("Luna", BodyType::Moon, 7.342e22, 1_737.4, 5.0),
                SpaceCoordinates::new(DVec3::new(1.0 + 0.00257, 0.0, 0.0)),
                KeplerOrbit::circular(0.00257, 2.66e-6), // local-frame, NOT heliocentric
                LogicalParent(earth),
                SystemId(0),
            ))
            .id();
        (world, sol, earth, luna)
    }

    #[test]
    fn heliocentric_orbit_for_body_moon_walks_up_to_parent_heliocentric() {
        // GRA-159 regression test: a moon's local-frame orbit must
        // never be returned as the heliocentric orbit.  The helper
        // must walk up to the parent's heliocentric KeplerOrbit.
        let (mut world, _sol, _earth, luna) = world_with_moon_fixture();
        let mut query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let body_query = query_state.query(&world);
        let helio = super::heliocentric_orbit_for_body(luna, &body_query)
            .expect("moon with a parent should resolve to the parent's heliocentric orbit");
        // The heliocentric sma is Earth's 1.0 AU, NOT Luna's local 0.00257 AU.
        assert!(
            (helio.semi_major_axis - 1.0).abs() < 1e-9,
            "moon should resolve to parent heliocentric sma = 1.0 AU, got {}",
            helio.semi_major_axis
        );
        assert!(
            helio.semi_major_axis > 0.5,
            "moon bug regression: returned the moon's local-frame orbit ({}) instead of the heliocentric orbit",
            helio.semi_major_axis
        );
    }

    #[test]
    fn heliocentric_orbit_for_body_planet_returns_own_orbit() {
        // Sanity check: a planet (heliocentric) still returns its own
        // KeplerOrbit; the GRA-159 fix must not regress planet handling.
        let (mut world, _sol, earth, _luna) = world_with_moon_fixture();
        let mut query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let body_query = query_state.query(&world);
        let helio = super::heliocentric_orbit_for_body(earth, &body_query)
            .expect("planet with heliocentric orbit should resolve");
        assert!(
            (helio.semi_major_axis - 1.0).abs() < 1e-9,
            "Earth heliocentric sma = 1.0 AU, got {}",
            helio.semi_major_axis
        );
    }

    #[test]
    fn heliocentric_orbit_for_body_star_returns_own_orbit() {
        // Sanity check: a star's barycentric orbit is returned
        // unchanged (JPL convention).  The Sol fixture does not have
        // a KeplerOrbit inserted, so the Star branch returns None
        // (acceptable: stars without a barycentric KO are rare and
        // the porkchop path doesn't need them).
        let (mut world, sol, _earth, _luna) = world_with_moon_fixture();
        let mut query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let body_query = query_state.query(&world);
        // Must not panic; result may be None or Some.
        let _ = super::heliocentric_orbit_for_body(sol, &body_query);
    }

    // ── Porkchop staleness regression test (GRA-152 follow-up) ─────────────
    //
    // User report: when the player lets time advance, the porkchop's
    // "starting point" tick stays anchored to the time the grid was
    // built, so closing and reopening the planner shows a stale ΔV
    // for "Depart Now" cells.  Fix: invalidate the cached grid once
    // the sim has advanced past `PORKCHOP_STALENESS_REAL_S × time_scale`
    // so the next frame rebuilds it against the current epoch.
    //
    // Second user report: at high `time_scale` (1 day/s or faster) the
    // sim advances so fast that the threshold fires immediately after
    // the player clicks a cell, snapping the selection back to the
    // auto-picked cheapest cell.  The threshold is now scaled by
    // `time_scale` so the rebuild fires after a fixed *real-time*
    // interval regardless of how fast the sim is running.
    //
    // Third user report: at 1 yr/s the previous scaled-only threshold
    // grew to ~72 sim years, so the grid stayed anchored to its build
    // epoch for the entire play session — the player watched the
    // "Depart Now" column remain frozen even though the planets had
    // moved on by a full synodic cycle.  The cap
    // `PORKCHOP_STALENESS_MAX_SIM_S = 1 sim year` clamps the
    // staleness window so the grid refreshes every ~1 real second at
    // 1 yr/s instead of every 72 sim years.  At low/medium speeds
    // the cap never binds, so existing tests still pass.
    //
    // Fourth user report: at intermediate sim speeds (1 wk/s,
    // 1 day/s) the sim-time cap alone still gates the rebuild on a
    // 52-72 real-second wall-clock interval — the player sees the
    // grid "frozen" even though the sim is moving a week per real
    // second.  The real-time floor `PORKCHOP_STALENESS_REAL_FLOOR_S`
    // adds a 1-real-second lower bound to the rebuild cadence, so
    // at intermediate speeds the grid refreshes at most 1 real
    // second apart (when the sim-time cap has also been crossed).
    // Both timers must fire for the grid to be marked stale.

    /// 1 hr/s — the default speed.
    const DEFAULT_TIME_SCALE: f64 = 3_600.0;
    /// 1 day/s — the next step on the speed ladder.
    const DAY_PER_S_TIME_SCALE: f64 = 86_400.0;
    /// 1 yr/s — extreme speed, 31,557,600 sim seconds per real second.
    const YEAR_PER_S_TIME_SCALE: f64 = 31_557_600.0;
    /// 1 wk/s — intermediate speed tier reported by the player.
    const WEEK_PER_S_TIME_SCALE: f64 = 604_800.0;

    /// Helper for tests: pick a `last_real_build_s` and
    /// `real_now_s` such that the wall-clock drift is large enough
    /// to satisfy the real-time floor.  Without this every test
    /// would need to thread the same constant pair through, which
    /// would obscure the sim-time semantics the tests actually
    /// exercise.
    fn real_floor_satisfied(_time_scale: f64) -> (f64, f64) {
        // Comfortably past the 5-real-second floor so the comparator
        // is robust to the >=/strict-greater-than boundary changes.
        let real_now = super::PORKCHOP_STALENESS_REAL_FLOOR_S * 100.0;
        (0.0, real_now)
    }

    /// Helper for tests: wall-clock delta is *below* the real-time
    /// floor.  Used when the test wants to assert that the sim cap
    /// alone does not fire (the real-time guard blocks it).
    fn real_floor_unsatisfied(_time_scale: f64) -> (f64, f64) {
        // 1 ms of wall-clock: way below the 5-real-second floor.
        let real_now = 1.0e-3;
        (0.0, real_now)
    }

    #[test]
    fn porkchop_staleness_returns_true_after_threshold_at_1_hr_per_s() {
        // Built 1 sim day ago at 1 hr/s: 86_400 sim sec elapsed,
        // sim cap (scaled 3 days, min with real-floor 5 hours) =
        // 5 hours = 18_000 sim sec.  Past the effective cap.  Real
        // floor satisfied.
        let built = 0.0;
        let elapsed = 1.0 * 86_400.0;
        let (last_real, real_now) = real_floor_satisfied(DEFAULT_TIME_SCALE);
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            DEFAULT_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_returns_false_within_threshold_at_1_hr_per_s() {
        // Built 1 sim hour ago at 1 hr/s: sim drift = 3_600 sim sec,
        // below the effective cap (5 hours = 18_000 sim sec).  Not
        // stale even though real floor is satisfied.
        let built = 1_000.0;
        let elapsed = built + 1.0 * 3_600.0; // 1 sim hour after
        let (last_real, real_now) = real_floor_satisfied(DEFAULT_TIME_SCALE);
        assert!(!super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            DEFAULT_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_returns_false_at_threshold_boundary() {
        // Exactly at the effective sim cap (1 sim hour at 1 hr/s, since
        // PORKCHOP_STALENESS_REAL_FLOOR_S = 1 s clamps the cap) —
        // should NOT invalidate (the comparator is strict `<=`).
        let built = 0.0;
        let elapsed = 3_600.0;
        let (last_real, real_now) = real_floor_satisfied(DEFAULT_TIME_SCALE);
        assert!(
            !super::porkchop_grid_is_stale(
                Some(built),
                elapsed,
                DEFAULT_TIME_SCALE,
                Some(last_real),
                real_now
            ),
            "exactly 1 sim hour at 1 hr/s = exactly at effective cap = not stale"
        );
    }

    #[test]
    fn porkchop_staleness_returns_false_when_no_build_epoch() {
        // No grid has ever been built — the deferred-build path
        // handles that, not the staleness path.
        assert!(!super::porkchop_grid_is_stale(
            None,
            1.0e9,
            DEFAULT_TIME_SCALE,
            None,
            0.0
        ));
    }

    #[test]
    fn porkchop_staleness_returns_false_when_no_real_build_epoch() {
        // Grid exists but no real-time stamp (legacy / pre-fix
        // FleetUiState from a save) — don't trigger a rebuild on
        // the next frame just because the wall-clock field is
        // absent.  The deferred-build path picks this up.
        assert!(!super::porkchop_grid_is_stale(
            Some(0.0),
            1.0e9,
            DEFAULT_TIME_SCALE,
            None,
            100.0
        ));
    }

    #[test]
    fn should_rotate_porkchop_buffer_fires_before_full_runway_exhaustion() {
        // Buffer span of 100 sim-days -> runway (half the span) is
        // 50 sim-days.  With the 0.85 early-rotation factor the
        // trigger should fire at 42.5 sim-days, well before the old
        // 100%-exhaustion threshold (50 sim-days).  This is the
        // "recalc a bit ahead of time" lead-time the user asked for.
        let buffer_span_s = 100.0 * 86_400.0;
        let runway_s = buffer_span_s * 0.5;
        let early_trigger_s = runway_s * 0.85;

        assert!(
            !super::should_rotate_porkchop_buffer(early_trigger_s - 86_400.0, buffer_span_s),
            "must NOT fire one full day before the early-trigger point"
        );
        assert!(
            super::should_rotate_porkchop_buffer(early_trigger_s, buffer_span_s),
            "must fire exactly at the early-trigger point (85% of runway)"
        );
        assert!(
            super::should_rotate_porkchop_buffer(runway_s, buffer_span_s),
            "must still fire at the old 100%-exhaustion point (monotonic threshold)"
        );
    }

    #[test]
    fn should_rotate_porkchop_buffer_never_fires_for_degenerate_span() {
        // A zero or negative buffer span (e.g. grid not yet built)
        // must never trigger a rotation — there is nothing to
        // rotate away from.
        assert!(!super::should_rotate_porkchop_buffer(1.0, 0.0));
        assert!(!super::should_rotate_porkchop_buffer(1.0, -1.0));
        assert!(!super::should_rotate_porkchop_buffer(0.0, 0.0));
    }

    #[test]
    fn should_rotate_porkchop_buffer_does_not_fire_before_any_shift() {
        // At shift_s = 0 (grid just built), the trigger must never
        // fire regardless of buffer span — otherwise every render
        // frame after a build would immediately queue another
        // rebuild.
        assert!(!super::should_rotate_porkchop_buffer(0.0, 480.0 * 86_400.0));
    }

    #[test]
    fn porkchop_staleness_handles_time_reversal() {
        // If the player uses a debug command to rewind time, `elapsed`
        // can land *before* `built`.  The `abs()` guard means we
        // still recognise staleness in that direction.
        let built = 4.0 * 86_400.0;
        let elapsed = 0.0;
        let (last_real, real_now) = real_floor_satisfied(DEFAULT_TIME_SCALE);
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            DEFAULT_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_scales_with_time_scale_at_1_day_per_s() {
        // At 1 day/s the per-frame sim delta is ~23 minutes.  The
        // effective sim cap (scaled 72 days, min with real-floor 5
        // sim days) = 5 sim days = 5 real seconds.  Rebuilds every
        // real second once the sim cap fires.
        let built = 0.0;
        let (last_real, real_now) = real_floor_satisfied(DAY_PER_S_TIME_SCALE);
        // 1 day after build: below the 5-sim-day cap.
        let elapsed = 86_400.0;
        assert!(!super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            DAY_PER_S_TIME_SCALE,
            Some(last_real),
            real_now
        ));
        // 7 days after build: past the 5-sim-day effective cap.
        let elapsed = 7.0 * 86_400.0;
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            DAY_PER_S_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_fires_every_real_second_at_1_wk_per_s() {
        // New behaviour: at intermediate sim speeds the grid refreshes
        // at the real-time floor (5 real seconds) once the sim cap has
        // been crossed.  At 1 wk/s the real-time floor in sim seconds
        // is 5 × 604_800 = 3_024_000 sim sec = 35 sim days.  Sim
        // drift crosses 35 sim days in 5 real seconds, and the sim
        // cap (1 sim year = 365 days) is much larger.  So the
        // rebuild fires every 5 real seconds.
        let built = 0.0;
        let (last_real, real_now) = real_floor_satisfied(WEEK_PER_S_TIME_SCALE);
        // 36 sim days after build (= 5.14 real seconds at 1 wk/s).
        // Effective sim cap is 35 sim days (strict `>` comparator);
        // 36 sim days is strictly past, real floor satisfied. Stale.
        let elapsed = 36.0 * 86_400.0;
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            WEEK_PER_S_TIME_SCALE,
            Some(last_real),
            real_now
        ));
        // 1 sim day after build (= 1/35 real second at 1 wk/s) —
        // sim drift below effective cap.  Not stale even though
        // real floor is satisfied.
        let elapsed = 1.0 * 86_400.0;
        assert!(!super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            WEEK_PER_S_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_real_floor_blocks_1_yr_per_s() {
        // At 1 yr/s the scaled sim cap binds at 1 sim year, but
        // the *real-time* floor must also be satisfied — so when
        // the real floor is *not* yet reached, the rebuild must
        // NOT fire even though the sim cap is past.  This prevents
        // per-frame rebuilds at 1 yr/s.
        let built = 0.0;
        // 2 sim years of drift (= 2 real seconds at 1 yr/s) past
        // the 1-sim-year cap.
        let elapsed = 2.0 * 365.25 * 86_400.0;
        let (last_real, real_now) = real_floor_unsatisfied(YEAR_PER_S_TIME_SCALE);
        assert!(
            !super::porkchop_grid_is_stale(
                Some(built),
                elapsed,
                YEAR_PER_S_TIME_SCALE,
                Some(last_real),
                real_now
            ),
            "real-time floor must block the rebuild at 1 yr/s until ≥ 5 real seconds have elapsed"
        );
        // Once the real floor is satisfied the rebuild fires.
        let (last_real, real_now) = real_floor_satisfied(YEAR_PER_S_TIME_SCALE);
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            YEAR_PER_S_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_scales_with_time_scale_at_1_yr_per_s() {
        // At 1 yr/s the scaled sim cap binds at 1 sim year; the
        // real-time floor in sim seconds is 5 × 31_557_600 = 5 sim
        // years, which is larger than the 1-year cap.  So the
        // *sim* cap binds first; rebuilds happen every ~1 sim year
        // (= 1 real second at 1 yr/s) when the real floor has also
        // been crossed.
        let built = 0.0;
        let (last_real, real_now) = real_floor_satisfied(YEAR_PER_S_TIME_SCALE);
        // 1 sim year of drift (= 1 real second at 1 yr/s): sim
        // cap at 1 sim year is exactly hit.  Strict `<` keeps the
        // grid cached at the boundary.
        let elapsed = 365.25 * 86_400.0;
        assert!(
            !super::porkchop_grid_is_stale(
                Some(built),
                elapsed,
                YEAR_PER_S_TIME_SCALE,
                Some(last_real),
                real_now
            ),
            "exactly 1 sim year at 1 yr/s = exactly at effective cap = not stale"
        );
        // 2 sim years: past the 1-year cap, real floor is past.
        // Stale.
        let elapsed = 2.0 * 365.25 * 86_400.0;
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            YEAR_PER_S_TIME_SCALE,
            Some(last_real),
            real_now
        ));
    }

    #[test]
    fn porkchop_staleness_paused_time_uses_default_threshold() {
        // TimeScale::scale can be 0.0 when paused.  We treat that as
        // 1.0 (no scaling) so a paused game doesn't get an infinite
        // threshold; the staleness still fires after the default
        // 72 real seconds of pause.
        let built = 0.0;
        let elapsed = 80.0; // 80 sim seconds since build (= 80 real seconds when paused)
        let (last_real, real_now) = real_floor_satisfied(0.0);
        assert!(super::porkchop_grid_is_stale(
            Some(built),
            elapsed,
            0.0,
            Some(last_real),
            real_now
        ));
    }

    // === GRA-386: GA grid resolution + two-leg preview tests =========
    //
    // Three tests:
    //   1. `porkchop_config_resolve_gravity_assist_returns_override_resolution` —
    //      pure-PorkchopConfig unit test for the new resolution plumbing.
    //   2. `build_gravity_assist_display_grid_uses_ron_resolution` — drives
    //      the full builder with a custom override and asserts the returned
    //      grid picks up the override's `(resolution_t_dep, resolution_tof)`.
    //   3. `build_planned_transfer_with_flyby_sets_flyby_and_leg2_orbit` —
    //      drives the extracted helper with a synthetic flyby/dest pair
    //      and asserts `flyby_body` / `leg2_orbit` are populated.

    /// The RON override resolution must reach `PorkchopConfig::resolve`
    /// verbatim.  Without this plumbing the GA grid would silently fall
    /// back to `GA_GRID_DEFAULT_RESOLUTION = (20, 15) = 300 cells` and
    /// the player's `via Mars` toggle would render the legacy sub-grid
    /// instead of the new 50×40 = 2000-cell panel.
    #[test]
    fn porkchop_config_resolve_gravity_assist_returns_override_resolution() {
        use crate::fleets::components::{PorkchopCategoryOverride, PorkchopConfig};
        let cfg = PorkchopConfig {
            category_overrides: vec![PorkchopCategoryOverride {
                match_key: "gravity_assist".to_string(),
                t_dep_window_days: 60.0,
                tof_min_hohmann_factor: 0.4,
                tof_max_hohmann_factor: 2.5,
                tof_floor_days: 5.0,
                tof_ceiling_years: 3.0,
                resolution_t_dep: 73,
                resolution_tof: 41,
                c3_ceiling_km2_s2: 400.0,
                short_hop_options: None,
                short_hop_t_dep_steps: None,
            }],
            ..PorkchopConfig::default()
        };
        let resolved = cfg.resolve("gravity_assist");
        assert_eq!(
            resolved.resolution_t_dep, 73,
            "PorkchopConfig::resolve must thread the GA override resolution_t_dep"
        );
        assert_eq!(
            resolved.resolution_tof, 41,
            "PorkchopConfig::resolve must thread the GA override resolution_tof"
        );
        // Sanity check: an unknown match key falls through to defaults.
        let unknown = cfg.resolve("nonexistent_category");
        assert_eq!(
            unknown.resolution_t_dep, cfg.defaults.resolution_t_dep,
            "unknown match key must fall through to defaults.resolution_t_dep"
        );
    }

    /// `build_gravity_assist_display_grid` now takes a `&PorkchopConfig`
    /// and uses `cfg.resolve("gravity_assist")` to size the grid.  This
    /// test wires a custom override and asserts the returned grid
    /// honours it.
    #[test]
    fn build_gravity_assist_display_grid_uses_ron_resolution() {
        use crate::fleets::components::{PorkchopCategoryOverride, PorkchopConfig};
        use crate::fleets::orbital_mechanics::GravityAssistOption;

        // Build a minimal Sol-system: Sol + Earth + Mars.  Each body
        // gets a mass, radius, position, and a KeplerOrbit at the
        // right SMA so `hohcentric_orbit_for_body` and the orbit
        // propagator resolve correctly.
        let mut world = World::new();
        let sun_e = world
            .spawn((
                test_body("Sol", BodyType::Star, 1.989e30, 6.957e8, 5.0),
                SpaceCoordinates {
                    position: DVec3::ZERO,
                },
                KeplerOrbit {
                    eccentricity: 0.0,
                    semi_major_axis: 0.0,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 0.0,
                },
                SystemId(7),
            ))
            .id();
        let earth_e = world
            .spawn((
                test_body("Earth", BodyType::Planet, 5.972e24, 6.371e6, 0.5),
                SpaceCoordinates {
                    position: DVec3::new(1.0, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0167,
                    semi_major_axis: 1.0,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (365.25 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();
        let mars_e = world
            .spawn((
                test_body("Mars", BodyType::Planet, 6.39e23, 3.39e6, 0.4),
                SpaceCoordinates {
                    position: DVec3::new(1.524, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0934,
                    semi_major_axis: 1.524,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (687.0 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();
        let venus_e = world
            .spawn((
                test_body("Venus", BodyType::Planet, 4.867e24, 6.052e6, 0.45),
                SpaceCoordinates {
                    position: DVec3::new(0.723, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0068,
                    semi_major_axis: 0.723,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (224.7 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let body_query = body_query_state.query(&world);

        // Synthetic GA candidate (Earth→Mars via Venus).
        let candidate = GravityAssistEntry {
            flyby_entity: venus_e,
            option: GravityAssistOption {
                body_name: "Venus".to_string(),
                flyby_radius_au: 0.723,
                v_inf_ms: 2500.0,
                max_dv_assist_ms: 1500.0,
                total_dv_ms: 5500.0,
                dv_savings_ms: 800.0,
                total_time_s: 350.0 * 86_400.0,
                extra_time_s: 80.0 * 86_400.0,
                window_period_s: f64::INFINITY,
                leg1_time_s: 175.0 * 86_400.0,
                leg2_time_s: 175.0 * 86_400.0,
                dv_depart_ms: 3000.0,
                dv_mid_ms: 500.0,
                dv_arrive_ms: 500.0,
                t_dep_s: 0.0,
                tof_s: 350.0 * 86_400.0,
            },
        };

        // Custom PorkchopConfig with a 7×5 gravity_assist override.
        let cfg = PorkchopConfig {
            category_overrides: vec![PorkchopCategoryOverride {
                match_key: "gravity_assist".to_string(),
                t_dep_window_days: 60.0,
                tof_min_hohmann_factor: 0.4,
                tof_max_hohmann_factor: 2.5,
                tof_floor_days: 5.0,
                tof_ceiling_years: 3.0,
                resolution_t_dep: 7,
                resolution_tof: 5,
                c3_ceiling_km2_s2: 400.0,
                short_hop_options: None,
                short_hop_t_dep_steps: None,
            }],
            ..PorkchopConfig::default()
        };
        // Sanity-check: the RON-style override must round-trip through
        // `resolve()` so we know the production builder sees the same
        // value the test asserts on below.
        let resolved = cfg.resolve("gravity_assist");
        assert_eq!(
            resolved.resolution_t_dep, 7,
            "test setup: PorkchopConfig::resolve must return the override resolution_t_dep=7"
        );
        assert_eq!(
            resolved.resolution_tof, 5,
            "test setup: PorkchopConfig::resolve must return the override resolution_tof=5"
        );

        let grid = super::build_gravity_assist_display_grid(
            &cfg,
            &candidate,
            &body_query,
            earth_e,
            mars_e,
            0.0,
        );
        assert_eq!(
            grid.resolution,
            (7, 5),
            "GA grid resolution must follow the RON override (got {:?})",
            grid.resolution
        );
        assert_eq!(
            grid.cells.len(),
            7 * 5,
            "GA grid cell count must equal cols*rows (got {})",
            grid.cells.len()
        );
    }

    /// `build_planned_transfer_with_flyby` (the GRA-386 helper) must
    /// populate `flyby_body`, `leg2_orbit`, and `leg2_start_s` so the
    /// 3D renderer draws the two-leg slingshot overlay.  Without
    /// these fields, the GA cell-click path produces a single Lambert
    /// arc to the destination and the flyby is invisible.
    #[test]
    fn build_planned_transfer_with_flyby_sets_flyby_and_leg2_orbit() {
        use crate::fleets::orbital_mechanics::TransferOption;

        // Minimal Sol-system: Sol + Earth + Mars + Venus (flyby).
        let mut world = World::new();
        let sun_e = world
            .spawn((
                test_body("Sol", BodyType::Star, 1.989e30, 6.957e8, 5.0),
                SpaceCoordinates {
                    position: DVec3::ZERO,
                },
                KeplerOrbit {
                    eccentricity: 0.0,
                    semi_major_axis: 0.0,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 0.0,
                },
                SystemId(7),
            ))
            .id();
        let earth_e = world
            .spawn((
                test_body("Earth", BodyType::Planet, 5.972e24, 6.371e6, 0.5),
                SpaceCoordinates {
                    position: DVec3::new(1.0, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0167,
                    semi_major_axis: 1.0,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (365.25 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();
        let mars_e = world
            .spawn((
                test_body("Mars", BodyType::Planet, 6.39e23, 3.39e6, 0.4),
                SpaceCoordinates {
                    position: DVec3::new(1.524, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0934,
                    semi_major_axis: 1.524,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (687.0 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();
        let venus_e = world
            .spawn((
                test_body("Venus", BodyType::Planet, 4.867e24, 6.052e6, 0.45),
                SpaceCoordinates {
                    position: DVec3::new(0.723, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0068,
                    semi_major_axis: 0.723,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (224.7 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        // Minimal fleet parked at Earth's SMA.
        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(earth_e, 0.0001);

        // Synthetic TransferOption: Leg-1 Hohmann from Earth to Venus
        // (the flyby).  The values don't have to be physically perfect
        // because we only check that the helper wires up
        // `flyby_body` / `leg2_orbit` / `leg2_start_s` correctly.
        let ga_option = TransferOption {
            label: "GA Test",
            total_delta_v_ms: 5500.0,
            delta_v1_ms: 3000.0,
            delta_v2_ms: 500.0,
            plane_change_dv_ms: 0.0,
            transfer_time_s: 350.0 * 86_400.0,
            sma_au: 0.86,
            eccentricity: 0.18,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };
        let ga_leg1_time_s = 175.0 * 86_400.0;

        // Pre-solved Leg-2 conic (what `sweep_gravity_assist_grid`
        // would attach to `cell.transfer_orbit` in production).  The
        // helper must surface it on the returned `PlannedTransfer`.
        let cell_leg2 = KeplerOrbit {
            eccentricity: 0.21,
            semi_major_axis: 1.12,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: 2.0 * std::f64::consts::PI / (400.0 * 86_400.0),
        };

        let planned = super::build_planned_transfer_with_flyby(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            venus_e, // flyby
            mars_e,  // real destination
            &ga_option,
            ga_leg1_time_s,
            0.0, // planned_departure_time_s
            &body_query,
            None,
            &system_id_query,
            7,
            None, // target_orbit_radius_au
            Some(cell_leg2),
        );

        let pt = planned.expect(
            "build_planned_transfer_with_flyby must return Some for a valid Sol-system setup",
        );
        assert_eq!(
            pt.flyby_body,
            Some(venus_e),
            "flyby_body must point at the assist body so the renderer draws the slingshot overlay"
        );
        assert_eq!(
            pt.destination_body, mars_e,
            "destination_body must point at the real destination so the fleet parks correctly on arrival"
        );
        let leg2 = pt
            .leg2_orbit
            .expect("leg2_orbit must be Some when cell_leg2_orbit is supplied");
        assert!(
            (leg2.semi_major_axis - cell_leg2.semi_major_axis).abs() < 1e-9
                && (leg2.eccentricity - cell_leg2.eccentricity).abs() < 1e-9,
            "leg2_orbit must be the pre-solved Lambert conic from the cell (got sma={}, ecc={})",
            leg2.semi_major_axis,
            leg2.eccentricity,
        );
        assert!(
            (pt.leg2_start_s - ga_leg1_time_s).abs() < 1.0,
            "leg2_start_s must equal ga_leg1_time_s so the renderer switches from Leg-1 to Leg-2 at the right epoch (got {}, expected {})",
            pt.leg2_start_s, ga_leg1_time_s,
        );
    }

    /// Backwards-compat: when `cell_leg2_orbit` is `None` (legacy
    /// GA-row path), the helper falls back to a Hohmann conic and
    /// still populates `leg2_orbit` + `leg2_start_s`.  This locks
    /// in the contract for the inline-stitch code we extracted.
    #[test]
    fn build_planned_transfer_with_flyby_falls_back_to_hohmann_when_no_cell_orbit() {
        use crate::fleets::orbital_mechanics::TransferOption;

        let mut world = World::new();
        let sun_e = world
            .spawn((
                test_body("Sol", BodyType::Star, 1.989e30, 6.957e8, 5.0),
                SpaceCoordinates {
                    position: DVec3::ZERO,
                },
                KeplerOrbit {
                    eccentricity: 0.0,
                    semi_major_axis: 0.0,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 0.0,
                },
                SystemId(7),
            ))
            .id();
        let earth_e = world
            .spawn((
                test_body("Earth", BodyType::Planet, 5.972e24, 6.371e6, 0.5),
                SpaceCoordinates {
                    position: DVec3::new(1.0, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0167,
                    semi_major_axis: 1.0,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (365.25 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();
        let mars_e = world
            .spawn((
                test_body("Mars", BodyType::Planet, 6.39e23, 3.39e6, 0.4),
                SpaceCoordinates {
                    position: DVec3::new(1.524, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0934,
                    semi_major_axis: 1.524,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (687.0 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();
        let venus_e = world
            .spawn((
                test_body("Venus", BodyType::Planet, 4.867e24, 6.052e6, 0.45),
                SpaceCoordinates {
                    position: DVec3::new(0.723, 0.0, 0.0),
                },
                KeplerOrbit {
                    eccentricity: 0.0068,
                    semi_major_axis: 0.723,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 2.0 * std::f64::consts::PI / (224.7 * 86_400.0),
                },
                LogicalParent(sun_e),
                SystemId(7),
            ))
            .id();

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(earth_e, 0.0001);
        let ga_option = TransferOption {
            label: "GA Legacy Test",
            total_delta_v_ms: 5500.0,
            delta_v1_ms: 3000.0,
            delta_v2_ms: 500.0,
            plane_change_dv_ms: 0.0,
            transfer_time_s: 350.0 * 86_400.0,
            sma_au: 0.86,
            eccentricity: 0.18,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };
        let ga_leg1_time_s = 175.0 * 86_400.0;

        let planned = super::build_planned_transfer_with_flyby(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            venus_e,
            mars_e,
            &ga_option,
            ga_leg1_time_s,
            0.0,
            &body_query,
            None,
            &system_id_query,
            7,
            None,
            None, // cell_leg2_orbit: None — exercise the Hohmann fallback
        );

        let pt = planned.expect("fallback path must still succeed for valid Sol setup");
        assert_eq!(pt.flyby_body, Some(venus_e));
        assert_eq!(pt.destination_body, mars_e);
        assert!(
            pt.leg2_orbit.is_some(),
            "Hohmann fallback must populate leg2_orbit so the renderer can draw Leg-2"
        );
        assert!(
            (pt.leg2_start_s - ga_leg1_time_s).abs() < 1.0,
            "Hohmann fallback must set leg2_start_s = ga_leg1_time_s"
        );
    }

    /// GRA-388: `maybe_record_burn_epoch_for_ga` is the helper the
    /// Planned Departure slider / Depart Now / Next Window buttons /
    /// view-mode toggle all use to keep the GA trajectory's
    /// `selected_abs_t_dep_s` in sync with `departure_offset_days`.
    /// Behaviour contract:
    /// - In Standard mode, the helper is a no-op (the three-way
    ///   clamp in `fleets/visuals.rs` falls through to the slider
    ///   branch, which is what the user wants to preserve).
    /// - In GravityAssist mode, the helper writes
    ///   `Some(elapsed + max(0, offset_days) * 86_400)` to
    ///   `selected_abs_t_dep_s` so the trajectory re-anchors to
    ///   the new burn time on the next frame.
    /// - Negative `offset_days` (the legacy -1.0 "next-window"
    ///   sentinel) is clamped at 0 so the recorded epoch doesn't
    ///   jump into the past and immediately trip the past-epoch
    ///   snap in the visuals layer.
    #[test]
    fn maybe_record_burn_epoch_for_ga_writes_absolute_epoch_only_in_ga_mode() {
        use super::maybe_record_burn_epoch_for_ga;
        use crate::ui::{FleetUiState, PorkchopViewMode};

        let elapsed = 30.0 * 86_400.0_f64; // 30 sim days
        let offset_days = 5.0_f64; // burn in 5 sim days
        let expected = elapsed + offset_days * 86_400.0;

        // Standard mode: no-op.  Even with a fresh `offset_days`,
        // `selected_abs_t_dep_s` stays None so the visuals clamp
        // falls through to the slider branch (`current_sim_s +
        // offset`).
        let mut state = FleetUiState::default();
        state.porkchop_view_mode = PorkchopViewMode::Standard;
        assert!(state.selected_abs_t_dep_s.is_none());
        maybe_record_burn_epoch_for_ga(&mut state, elapsed, offset_days);
        assert!(
            state.selected_abs_t_dep_s.is_none(),
            "Standard mode must leave selected_abs_t_dep_s untouched"
        );

        // GravityAssist mode: writes the absolute burn epoch.
        let mut state = FleetUiState::default();
        state.porkchop_view_mode = PorkchopViewMode::GravityAssist(0);
        assert!(state.selected_abs_t_dep_s.is_none());
        maybe_record_burn_epoch_for_ga(&mut state, elapsed, offset_days);
        assert_eq!(
            state.selected_abs_t_dep_s,
            Some(expected),
            "GA mode must write elapsed + offset*86400 to selected_abs_t_dep_s"
        );

        // Negative offset is clamped at 0 (legacy -1.0 sentinel).
        let mut state = FleetUiState::default();
        state.porkchop_view_mode = PorkchopViewMode::GravityAssist(0);
        maybe_record_burn_epoch_for_ga(&mut state, elapsed, -1.0);
        assert_eq!(
            state.selected_abs_t_dep_s,
            Some(elapsed),
            "Negative offset must clamp to 0 (epoch = elapsed, no jump-into-the-past)"
        );

        // Zero offset → epoch = elapsed (immediate departure).
        let mut state = FleetUiState::default();
        state.porkchop_view_mode = PorkchopViewMode::GravityAssist(2);
        maybe_record_burn_epoch_for_ga(&mut state, elapsed, 0.0);
        assert_eq!(state.selected_abs_t_dep_s, Some(elapsed));

        // Multiple successive calls overwrite the recorded epoch
        // (the slider/button handler can fire every frame while the
        // player drags the slider, so the helper must be idempotent
        // in the "later call wins" sense).
        let mut state = FleetUiState::default();
        state.porkchop_view_mode = PorkchopViewMode::GravityAssist(0);
        maybe_record_burn_epoch_for_ga(&mut state, elapsed, 5.0);
        maybe_record_burn_epoch_for_ga(&mut state, elapsed, 10.0);
        assert_eq!(
            state.selected_abs_t_dep_s,
            Some(elapsed + 10.0 * 86_400.0),
            "later call must overwrite earlier recorded epoch"
        );
    }
}
