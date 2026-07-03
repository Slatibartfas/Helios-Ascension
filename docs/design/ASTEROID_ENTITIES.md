# Per-Asteroid Entities — v0.5.x / v0.8 Design Intent

Design intent for the first-class asteroid entity layer that ties together
the three pillars enumerated in
[GRA-313](https://paperclip.klingspor.one/GRA/issues/GRA-313):

1. **Asteroid mining UI** (ROADMAP §5.x.2).
2. **Cyclical logistics** source objects — each asteroid is a shippable
   `ResourceRequest` origin.
3. **Terraforming input** (ROADMAP §8.1) — per-asteroid volatile composition
   becomes the source side of "redirect water-rich asteroid onto Mars /
   airless world."

Owner: **LGD**. Status: **design only**, awaiting CTO schema sign-off.
Rust delta scoping is the CTO's job once this lands. This document extends
(does not replace) [`SOLAR_SYSTEM_DATA.md`](SOLAR_SYSTEM_DATA.md) and
[`LOGISTICS_NETWORK.md`](LOGISTICS_NETWORK.md).

---

## 1. Problem statement

The current `solar_system.ron` carries every asteroid as a
`CelestialBodyData` tuple: orbit, mass, radius, `asteroid_class`, optional
texture. That is enough to render and to plot a porkchop — it is not enough
to:

* deliver a `ResourceRequest` whose `source_body` points to an asteroid,
* tell the mining UI which `ResourceType`s this asteroid can yield, or
* tell the terraforming planner whether the asteroid is a viable volatile
  reservoir.

The 8-dimension survey system was designed for *planets and moons*. An
asteroid is a different beast: a single body, no atmosphere, no
hydrosphere, no meaningful "subsurface" tier beyond a coarse regolith,
and a composition that is essentially a `HashMap<ResourceType, f64>` of
volatiles + metals + precious-metal traces. Forcing asteroids through
`MineralDeposit` + `ResourceReserve { proven_crustal, deep_deposits,
planetary_bulk, concentration }` would cost a layer of per-tier
fakery for no gameplay benefit.

The fix is a **new sidecar RON file keyed by body name** that holds the
gameplay data the existing `solar_system.ron` tuples do not. Per the
"data-driven — never hardcode in Rust" rule, this is RON, not Rust.

## 2. Schema proposal

### 2.1 File

`assets/data/asteroids.ron` — top-level `(asteroids: [AsteroidEntry, ...])`.

### 2.2 Struct

```ron
(
    asteroids: [
        (
            // Must match a CelestialBodyData.name from solar_system.ron
            // for which body_type == Asteroid. The loader joins on this key.
            body: "Ceres",

            // Coarse pre-survey classification. Overrides the
            // asteroid_class already on CelestialBodyData only if
            // Some(...) is set; absent entries fall back to the
            // existing value (modder-safe additive extension).
            asteroid_class: Some(CType),

            // Total dry mass available for mining AND redirect. kg.
            // Source of truth — overrides the smaller mass on
            // CelestialBodyData, which is the body's *bulk* mass
            // and includes unmineable silicates above the crust.
            total_mass_kg: 9.39e20,

            // Composition in mass-fraction (0.0..=1.0, sums to <=1.0).
            // Resources not listed have fraction 0.0; entries with
            // fractional sum <1.0 imply the remainder is unmineable
            // silicate/regolith matrix.
            composition: {
                "Water":        0.180,    // C-type outer belt — primarily hydrated silicates
                "Iron":         0.110,
                "Nickel":       0.022,
                "Silicates":    0.300,
                "Ammonia":      0.005,
                "Carbon":       0.022,
                "Magnesium":    0.040,
            },

            // Minimum survey tier (0..=8 — see §3.1) before ANY of
            // `composition` becomes visible in the dossier. Below
            // threshold, the dossier shows only the asteroid_class
            // and a "needs survey" badge. Modder-tunable per body.
            discovery_tier: 2,

            // Δv in km/s required to redirect this asteroid onto a
            // representative target orbit. Used by the terraforming
            // planner (§3.3) and surfaced in the asteroid mining UI
            // as the "tug cost" gauge. See §3.2 for the model behind
            // the per-body value.
            //
            // Sentinel: `None` = not divertable (e.g. a rock too
            // massive or too far from any practical target).
            delta_v_to_redirect_kms: Some(2.4),

            // Δv target. Used so the model is reproducible: the
            // same asteroid always quotes the same cost, regardless
            // of where the player wants to send it. Modders can
            // quote a different target per asteroid.
            //
            // Currently supported targets (resolved at load time):
            //   "Mars"   — for water-rich C-types (main belt)
            //   "Moon"   — for S/M-types (near-Earth)
            //   "L4"    — Jupiter L4 / Lagrange candidates
            //   "L5"    — Jupiter L5
            //   "Earth"  — for volatile-poor, metal-rich cases
            //
            // CTO will reject the trait-based string at load time
            // and surface a `ron::Error` if the target is unknown.
            redirect_target: "Mars",

            // True iff the asteroid is a viable Terraforming volatile
            // source for §8.1. Two conditions:
            //   (a) composition has Water > 0.05 OR Ammonia > 0.02
            //       OR Methane > 0.02, AND
            //   (b) delta_v_to_redirect_kms <= 6.0 km/s
            // The flag is hand-curated for the v0.5.x worked set; the
            // loader validates consistency and emits a warning if a
            // body marked terraforming_source is well outside the
            // automated threshold (or vice-versa).
            terraforming_source: true,

            // Optional explicit override for the auto-derived
            // `terraforming_source` rule. None = use the rule.
            // Some(true)/Some(false) = force the flag.
            #[serde(default)]
            terraforming_source_override: None,

            // Optional lore seed for the dossier narrative. Pure
            // display — searched in-game by the player but never
            // branched on. Tone: technical, neutral, slightly dry.
            #[serde(default)]
            lore_seed: Some(
                "C-class dwarf planet. ~25% water of hydration in the regolith; \
                 nitrogen and ammonia trapped in clays. Practically a giant dirty \
                 snowball dragged into the inner system by a Jupiter assist."
            ),
        ),
        // ... more entries ...
    ],
)
```

### 2.3 Loader invariants

The CTO-side `AsteroidRegistry` plugin (`src/astronomy/asteroids.rs`)
guarantees:

1. Every `body` key matches exactly one `CelestialBodyData` whose
   `body_type ∈ {Asteroid, DwarfPlanet}`. Unknown / non-matching
   names → loader hard error (`ron::Error`). DwarfPlanet is in the
   allowlist because Ceres is canonically a `DwarfPlanet` in this
   codebase (`solar_system.ron:707`) and is the single most important
   C-type reservoir in the inner system. Excluding it would be a
   gameplay regression for v0.5.x. Confirmed in CTO HB-303 review.
2. `composition` values sum to ≤ 1.0 (+ tiny epsilon). Sum > 1.0 →
   hard error (the regolith matrix is a definite resource, not an
   "infinite reservoir").
3. `composition` keys must be `ResourceType::all()` variants. Unknown
   string key → hard error.
4. `discovery_tier` ∈ 0..=8. Out-of-range → hard error.
5. `delta_v_to_redirect_kms` is either `None` or in (0.0, 30.0].
   30 km/s is *vastly* more than any plausible redirect; values above
   that almost certainly indicate a model bug.
6. `redirect_target` must be one of the supported strings (§2.2).
7. Every `AsteroidEntry` MUST declare `terraforming_source: bool`
   explicitly. Hard fail on a missing row (`#[serde(default)]` not
   used for this field). When the flag disagrees with the auto-rule
   (≥ 0.05 Water OR ≥ 0.02 Ammonia/Methane AND Δv ≤ 6.0), warn for
   v0.5.x and hard-fail for v0.8.x. A `cfg(debug_assertions)` switch
   can flip warn → error during dev builds. Confirmed in CTO HB-303
   review — strict-or-warn depends on the active milestone.

These belong in the loader as `#[test]`-able unit tests — exactly the
pattern `solar_system_data.rs:678 ff` uses for `asteroid_class`
distribution. Two CTO-named tests must ship with the loader:

* **Test A — DwarfPlanet allowlist.** Given a sidecar entry with
  `body: "Ceres"`, the loader accepts it (Ceres's `body_type ==
  DwarfPlanet`). Asserts the §2.3 #1 relaxation.
* **Test B — Override semantics.** Given a `CelestialBodyData` with
  `asteroid_class: Some(SType)` and a sidecar `asteroid_class:
  Some(CType)`, the loader reports CType. The Ryugu worked-example
  is the canonical input (Hayabusa2 sample-return science says CType;
  the existing `solar_system.ron:7080 ff` body tuple incorrectly
  labels it SType). Test locks the override semantics so future
  refactors don't silently re-introduce the inherit path.

### 2.4 What does NOT change

* `solar_system.ron` — the existing `CelestialBodyData` tuples are
  untouched. The orbit, mass, radius, rotation, `asteroid_class` all
  remain the source of truth for astronomy. The sidecar `asteroids.ron`
  adds game-design data; it never duplicates astronomy.
* `ResourceType` — no variants added; the schema uses existing ones.
* `MineralDeposit` / `ResourceReserve` — untouched. Asteroids do not
  use them; we deliberately do not fake the per-tier layout.
* `SurveyDimension` — unchanged. `discovery_tier` is *one number*, not
  eight. The dossier reads it as an integer gate on `composition`
  visibility (§3.1).

## 3. Design rules

### 3.1 Discovery — extending the 8-dimension survey system

The 8 survey dimensions (`src/survey/types.rs:19-46` —
`OrbitalMech, Atmosphere, SurfaceFeatures, MineralClasses,
MineralDeposits, Subsurface, Habitability, Anomalies`) were sized for
planets and moons. Asteroids only meaningfully interact with three:

| Dimension | Asteroid meaning |
|-----------|------------------|
| `OrbitalMech`     | Already known from the catalog (JPL/NASA); default tier 3 on game start. No surveying needed. |
| `MineralClasses`  | Surfaces the `asteroid_class` enum (C/S/M/V/D/P). Default tier 1 — visible at game start. |
| `MineralDeposits` | Drives `composition` reveal. Gates the integer `discovery_tier` per body. |

The new `discovery_tier` (0..=8) is a **single integer** that gates
visibility of `composition` entries:

| `discovery_tier` | What the dossier shows |
|------------------|------------------------|
| 0 (unsurveyed)   | Only `asteroid_class`; composition hidden. |
| 1                | + the *class-tier* generic mix (e.g. "C-type: water- and carbon-rich"). |
| 2                | + `Water`, `Iron`, `Silicates` (the bulk). |
| 3                | + `Nickel`, `Ammonia`, `Carbon`, `Magnesium` (the secondaries). |
| 4                | + everything else in `composition` (precious metals, trace volatiles). |
| 5+               | Same as 4 — diminishing returns; reserve headroom for future tiers. |

The tier-gate mapping is **not** in the RON. Modders set
`discovery_tier` per body; the mapping table is a Rust constant in
`src/astronomy/asteroids.rs`. This keeps the RON minimal and the loader
testable.

### 3.2 Redirect cost model

`delta_v_to_redirect_kms` is the *total mission Δv* (km/s) the player
must expend to nudge the asteroid onto a representative target orbit.
Two cost terms:

```
Δv_total ≈ Δv_orbit + Δv_land
Δv_orbit ≈ 0.5 * |v_current − v_target|     // prograde-only nudge
Δv_land  ≈ sqrt(GM_target / R_target)        // capture at target
```

with simplifications:

* `v_current` = asteroid's heliocentric velocity at the moment of
  diversion (use J2000 epoch mean, e.g. 19.9 km/s for a Ceres-class
  2.77-AU orbit; 24.5 km/s for a near-Earth 1.0-AU orbit).
* `v_target` = velocity at target circular orbit at the epoch when
  the asteroid's orbit intersects it.
* prograde-only is a conservative bound — gravity assists + plane
  changes add real cost in the eventual transfer planner; the
  auction of "find a cheaper Δv via assist" is an L4/L5 Lagrange
  redirection problem the player can pursue.

Worked examples (full set in §5):

| Body              | Δv_to (km/s) | Rationale |
|-------------------|--------------|-----------|
| Ceres             | 2.4          | Low-e inner-belt nudge; ~half the cost of a Mars-shuffle |
| 4 Vesta           | 2.7          | Slightly higher orbit, eccentric nudge |
| 101955 Bennu      | 5.8          | Earth-crosser, near-1:1 resonance; only land-friendly redirect |
| 588 Achilles      | 1.8          | L4 Trojan at Jupiter — already where you want it |
| 617 Patroclus     | 1.8          | L5 Trojan |
| Psyche            | 4.9          | M-type, large mass — redirect is hard; mine-in-place preferred |
| Davida            | 6.2          | Large C-type in outer belt; high cost |
| Juno              | 3.1          | Main belt S-type; redirect to Mars feasible |

The model is *not* a *near-future* spacecraft problem — Helios's fusion
torch Isp (~50 000 s) makes a 3 km/s Δv routine even at 10¹⁵-kg scale.
The player's real choice is **mine-in-place vs redirect**; Δv is
how we surface that choice.

### 3.3 Cyclical logistics integration

A `ResourceRequest` already has a `source_body: Option<Entity>` field
(`src/economy/logistics.rs:115-116`). Asteroids naturally fill that slot
when the player builds a `Mine` on the asteroid. No new request
mechanic is needed — just an `AsterroidBody` component on the
asteroid's `Entity`, set by the loader.

**Cap rule (lifted from
[`LOGISTICS_NETWORK.md`](LOGISTICS_NETWORK.md)):** a freighter built
from a low-mass asteroid can only load up to `mass_kg * 0.1` of
material in a single trip (a 10% mass-fraction lifting limit to match
fuel-and-structure accounting). Belt-and-braces, the request also
inherits the existing `cargo_capacity_t_for_components` cap. The 10%
fraction is **not** RON-driven; it's a Rust constant matching
`freighter_cargo_capacity_t_for_components` in
`src/ships/templates.rs:566-578`.

### 3.4 Terraforming integration (§8.1)

ROADMAP §8.1 lists "Comet redirect (impactor missions that deliver
volatiles to airless worlds)." Asteroids extend that pipeline — a
water-rich C-type main-belt object is a *much* cheaper volatiles
delivery than a comet impact for the same mass.

The `terraforming_source` flag, combined with `composition` reveal,
gives the terraforming planner exactly what it needs:

* read composition → pick the asteroids whose Water+fraction is above
  the planner's threshold (≥ 5% by mass for v0.5.x — modder-tunable);
* read delta_v_to_redirect_kms → rank the candidates;
* emit a "redirect target: Mars" `ResourceRequest` whose `source_body`
  is the asteroid Entity and `destination_body` is the target
  terraforming body.

The terraforming-side workflow lives in the v0.8.0 child issue; this
issue only seeds the data the planner will read.

### 3.5 Price volatility (LGD game-balance recommendation)

**Recommendation: per-asteroid prices are *fixed* on a sim-year basis.**
Rationale:

* Asteroids are small, finite resource reservoirs. A per-sim-day or
  per-tick price model would swing wildly on a single mine output,
  drowning the cyclical-logistics signal in noise.
* A per-year lock matches the cyclical logistics schedule — freighters
  plan in years, not days.
* The economic *value* of the asteroid — discovered composition,
  discovered mass — does not change once survey finishes. Volatility
  would have to come from player action (more mines tapping the same
  body, depleting reserves), which the existing `ResourceReserve` /
  mining tick already models.

The rule: `price_per_mt(asteroid, resource, sim_year) =
price_per_mt_base(resource) * class_multiplier(asteroid_class)`.
The `class_multiplier` is a Rust constant table
(`CType: 1.0`, `SType: 0.95`, `MType: 1.4`, `VType: 0.9`, `DType: 1.1`,
`PType: 1.05`) reflecting real-world rarity. CTO can tune the
constant; not LGD's wheelhouse.

A future v0.5+ ticket can introduce per-body price shocks (e.g. a
player uncovering a heavily-gold-laden M-type would crash the gold
price for that year). For v0.5.x, fixed per-year is the cleanest
ground truth.

## 4. Worked-example set (10 asteroids, hand-tuned)

The set below is the v0.5.x landing set. Selection criteria:

* **Span the asteroid classes.** C, S, M, V each represented.
* **Cover the gameplay loops.** Mining logistics + terraforming + a
  flag-worthy Lagrange Trojans.
* **Mix the orbital regimes.** Main belt, near-Earth, Jupiter L4/L5.
* **Composition that interests a player.** Each has at least one
  resource the player will *want* to mine.

The numbers below are derived from real-world JPL/Meteoritical Bulletin
data where available (Ceres, Vesta, Psyche, Bennu, Achilles, Patroclus)
and inferred from taxonomy for the rest.

### 4.1 Ceres — C-type, dwarf-planet-class reservoir

```
body: "Ceres"
asteroid_class: Some(CType)
total_mass_kg: 9.39e20    // JPL; entire dwarf-planet bulk
composition: {
    "Water":        0.180,  // hydrated silicates throughout
    "Iron":         0.110,
    "Nickel":       0.022,
    "Silicates":    0.300,
    "Ammonia":      0.005,
    "Carbon":       0.022,
    "Magnesium":    0.040,
}
discovery_tier: 2
delta_v_to_redirect_kms: Some(2.4)
redirect_target: "Mars"
terraforming_source: true
lore_seed: Some(
    "C-class dwarf planet. ~25% water of hydration in the regolith; \
     nitrogen and ammonia trapped in clays. Practically a giant dirty \
     snowball dragged into the inner system by a Jupiter assist."
)
```

### 4.2 4 Vesta — V-type, basaltic crust fragment

```
body: "Vesta"
asteroid_class: Some(VType)
total_mass_kg: 2.59e20
composition: {
    "Iron":         0.180,
    "Silicates":    0.420,
    "Titanium":     0.010,
    "Magnesium":    0.080,
    "Nickel":       0.025,
    "Aluminum":     0.060,
}
discovery_tier: 2
delta_v_to_redirect_kms: Some(2.7)
redirect_target: "Mars"
terraforming_source: false
lore_seed: Some(
    "V-type differentiated fragment. Iron core remnants, basaltic crust \
     rich in pyroxene. The HED meteorite parent body."
)
```

### 4.3 Psyche — M-type, the iconic metal asteroid

```
body: "Psyche"
asteroid_class: Some(MType)
total_mass_kg: 2.72e19
composition: {
    "Iron":         0.560,
    "Nickel":       0.180,
    "Platinum":     0.0001,
    "Gold":         0.00002,
    "Copper":       0.008,
    "Silicates":    0.080,
    "Cobalt":       0.005,
}
discovery_tier: 3
delta_v_to_redirect_kms: Some(4.9)
redirect_target: "Earth"
terraforming_source: false
lore_seed: Some(
    "M-type. Probably an exposed iron core of a stripped parent body. \
     Mass fractions are dominated by Fe-Ni with traces of noble metals \
     rivaling Earth-crust concentrations by an order of magnitude."
)
```

### 4.4 101955 Bennu — near-Earth C-type, low Δv to Earth

```
body: "101955 Bennu"
asteroid_class: Some(CType)
total_mass_kg: 7.8e10     // OSIRIS-REx measured
composition: {
    "Water":        0.080,
    "Iron":         0.060,
    "Nickel":       0.012,
    "Silicates":    0.420,
    "Carbon":       0.045,
    "Ammonia":      0.003,
}
discovery_tier: 1
delta_v_to_redirect_kms: Some(5.8)
redirect_target: "Earth"
terraforming_source: false
lore_seed: Some(
    "Near-Earth C-type. OSIRIS-REx sample returned hydrated clays and \
     organics. Surrogate for accessible water on a near-Earth orbit."
)
```

### 4.5 588 Achilles — Jupiter L4 Trojan, S-type

```
body: "588 Achilles"
asteroid_class: Some(SType)
total_mass_kg: 2.27e18
composition: {
    "Iron":         0.180,
    "Silicates":    0.430,
    "Nickel":       0.022,
    "Magnesium":    0.090,
    "Aluminum":     0.060,
    "Titanium":     0.005,
}
discovery_tier: 2
delta_v_to_redirect_kms: Some(1.8)
redirect_target: "L4"
terraforming_source: false
lore_seed: Some(
    "Greek camp at Jupiter L4. The first Jupiter Trojan discovered \
     and a useful staging body for a deep-space refuelling post."
)
```

### 4.6 617 Patroclus — Jupiter L5 binary, P-type

```
body: "617 Patroclus"
asteroid_class: Some(PType)
total_mass_kg: 1.36e18
composition: {
    "Water":        0.090,    // P-types are dark and volatile-rich
    "Ammonia":      0.012,
    "Methane":      0.008,
    "Iron":         0.060,
    "Silicates":    0.380,
    "Magnesium":    0.060,
    "Carbon":       0.030,
}
discovery_tier: 2
delta_v_to_redirect_kms: Some(1.8)
redirect_target: "L5"
terraforming_source: true
lore_seed: Some(
    "Trojan camp at Jupiter L5. Binary system with Menoetius; the \
     pair is primitive outer-belt material. Volatiles may exceed \
     bulk-fraction estimates from spectroscopy."
)
```

### 4.7 Juno — main-belt S-type, classically accessible

```
body: "Juno"
asteroid_class: Some(SType)
total_mass_kg: 2.67e19
composition: {
    "Iron":         0.220,
    "Silicates":    0.400,
    "Nickel":       0.030,
    "Magnesium":    0.080,
    "Aluminum":     0.040,
    "Water":        0.012,
}
discovery_tier: 2
delta_v_to_redirect_kms: Some(3.1)
redirect_target: "Mars"
terraforming_source: false
lore_seed: Some(
    "S-type main belt. One of the earliest-discovered asteroids and a \
     convenient mining anchor for the v0.5.x playbook."
)
```

### 4.8 Hygiea — large C-type, slow-rotating, low density

```
body: "Hygiea"
asteroid_class: Some(CType)
total_mass_kg: 8.32e19
composition: {
    "Water":        0.060,
    "Silicates":    0.500,
    "Iron":         0.040,
    "Carbon":       0.080,
    "Ammonia":      0.004,
    "Magnesium":    0.050,
}
discovery_tier: 2
delta_v_to_redirect_kms: Some(3.6)
redirect_target: "Mars"
terraforming_source: true
lore_seed: Some(
    "C-type, slow rotator, low bulk density. Probably a rubble pile \
     — high porosity means redirect Δv is lower per kg."
)
```

### 4.9 433 Eros — near-Earth S-type, classically accessible

```
body: "433 Eros"
asteroid_class: Some(SType)
total_mass_kg: 6.69e15
composition: {
    "Iron":         0.190,
    "Silicates":    0.420,
    "Nickel":       0.025,
    "Magnesium":    0.085,
    "Aluminum":     0.055,
    "Titanium":     0.005,
}
discovery_tier: 1
delta_v_to_redirect_kms: Some(5.4)
redirect_target: "Earth"
terraforming_source: false
lore_seed: Some(
    "Near-Earth Amor-class S-type. NEAR-Shoemaker landed 2001. The \
     v0.5.x reference body for surface-mining tutorials."
)
```

### 4.10 162173 Ryugu — near-Earth C-type, Hayabusa2 sample-return

```
body: "162173 Ryugu"
asteroid_class: Some(CType)
total_mass_kg: 4.50e11
composition: {
    "Water":        0.090,
    "Iron":         0.080,
    "Nickel":       0.015,
    "Silicates":    0.380,
    "Carbon":       0.060,
    "Ammonia":      0.005,
}
discovery_tier: 1
delta_v_to_redirect_kms: Some(5.1)
redirect_target: "Moon"
terraforming_source: true
lore_seed: Some(
    "Hayabusa2 sample returned 2020. Hydrated silicates and organics \
     confirmed. Low Δv to the Moon makes Ryugu the v0.5.x canonical \
     volatile redirect for terrestrial cislunar industry."
)
```

## 5. Acceptance / schema-sign-off checklist

For the LGD→CTO handoff:

- [ ] Schema fields cover the three pillars (mining, logistics, terraforming).
- [ ] Schema is modder-readable; no Rust-side knowledge needed.
- [ ] No new `ResourceType` variants.
- [ ] No `MineralDeposit` / `ResourceReserve` duplication.
- [ ] All 10 worked examples serialize round-trip via `ron::to_string` /
      `ron::from_str`.
- [ ] Loader invariants (`§2.3`) can each be expressed as a 5-line
      `#[test]` block in `src/astronomy/asteroids.rs`.
- [ ] `discovery_tier` table (`§3.1`) is a single Rust constant, not
      data-driven (deliberately — keeps RON slim).

## 6. Children / handoffs

* **CTO schema review (this issue)** — sign-off on the Rust delta list:
    * `src/economy/types.rs` — no enum additions.
    * `src/astronomy/asteroids.rs` (NEW) — `AsteroidRegistry` plugin +
      `AsteroidData` component + `AsteroidSystemSet` Update schedule.
    * `src/economy/discovery.rs` — extend `tier_breakdown_for_reserve`
      to surface asteroid composition when `body.asteroid_class` is
      present. Two-line addition; existing tests untouched.
    * `src/colony/` — terraforming-side plumbing; punt to §8.1
      child issue.
* **GRA-309-impl (proposed child)** — Rust implementation of the loader
  + the per-tier matrix extension. LGD provides the design comment;
  the CTO-side coder implements and merges.
* **GRA-312-asteroid-mining-ui (ROADMAP §5.x.2)** — UI consumption.
  Independent of this issue's loader work; reads
  `AsteroidRegistry` once it lands.

## 7. Open questions for the CTO / operator

1. **Should `asteroid_class` on the sidecar override the body's
   `asteroid_class` from `solar_system.ron`, or only fill in a default?**
   The schema proposal treats it as an optional override (`None` =
   inherit). Modders may want to leave the body tuple's class alone.
2. **Discovery-tier table location.** A Rust constant is cleanest;
   some modders might prefer it RON-driven. §3.1 commits to "single
   Rust constant" — flag if you want it data-driven.
3. **Redirect-target set.** §2.2 lists Mars / Moon / L4 / L5 / Earth.
   CTO can shrink the set (e.g. drop L4/L5 until §8.2 Lagrange
   colonies land) without breaking the v0.5.x playability.
4. **Terraforming-source validation.** §2.3 lists a strong invariant
   that the hand-curated flag must match the auto rule. The CTO can
   soft-fail (warn-only) for v0.5.x and harden in v0.8.x.

These are open — no LGD decision forced on CTO before sign-off.
