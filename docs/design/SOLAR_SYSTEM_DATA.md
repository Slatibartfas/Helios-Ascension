# Solar System Data Documentation

## Overview

Helios Ascension uses realistic astronomical data for all celestial bodies in the
solar system. The primary catalog lives in `assets/data/solar_system.ron` and is
loaded at runtime by `src/plugins/solar_system_data.rs` → `src/plugins/solar_system.rs`.

**Current Status: 713 solar-system bodies** plus 60 nearby star systems loaded
from `assets/data/nearest_stars_raw.json` and ~5,000 confirmed exoplanets staged
in CSV for v0.6.

## Data Sources

- NASA JPL Horizons System, JPL Small-Body Database
- International Astronomical Union (IAU)
- Planetary and Lunar Coordinates
- Minor Planet Center
- NASA Exoplanet Archive (staged, runtime loader deferred to v0.6)

## Body Type Summary (solar_system.ron)

```
Star:           1   (Sol)
Planet:         4   (Mercury, Venus, Earth, Mars)
GasGiant:       4   (Jupiter, Saturn, Uranus, Neptune)
Ring:           2   (Saturn main ring + Uranus ε ring)
DwarfPlanet:   55   (Ceres, Pluto, Eris, Makemake, Haumea, Sedna, Orcus, …)
Moon:         147   (Galilean + Titan + Europa + Ganymede + Callisto + Io
                     + irregulars across all giants + Charon)
Asteroid:     450   (C / S / M / V / D / P distribution per
                     procedural generation; gameplay sidecar in
                     `asteroids.ron`)
Comet:         50   (inline in `solar_system.ron` — no separate sidecar)
────────────────────
TOTAL:        713 bodies in the active scene
```

`BodyType` is defined in `src/plugins/solar_system_data.rs` and includes the
`Star | Planet | GasGiant | DwarfPlanet | Moon | Asteroid | Comet | Ring`
variants used throughout the runtime, save path, transfer planner, and UI.

## Celestial Bodies by Category

### Star (1)
- **Sol** — 1.9885×10³⁰ kg, 695,700 km radius.

### Planets (4): Mercury, Venus, Earth, Mars.

### Gas Giants (4): Jupiter, Saturn, Uranus, Neptune. Jupiter hosts the
79-moon Jovian system; Saturn's main ring is `Ring` #1.

### Rings (2)
- **Saturn main ring** — 7,000–80,000 km radial extent, particle system.
- **Uranus ε ring** — narrow dusty ring.

### Dwarf Planets & Kuiper Belt Objects (55)
- **Main Belt:** Ceres.
- **Classical KBOs:** Pluto, Eris, Makemake, Haumea, Quaoar, Sedna, Orcus,
  Salacia, Varda, plus 45 more KBOs and scattered-disc objects.

### Moons (147 total)
- **Earth (1):** Moon.
- **Mars (2):** Phobos, Deimos.
- **Jupiter (79):** Galilean (4) + Amalthea (4) + Himalia (5) + Ananke (10) +
  Carme (7) + Pasiphae (9) + irregulars (17) + recent S/2003 discoveries.
- **Saturn (83):** Major (7) + co-orbital (2) + inner small (15) +
  Hyperion + Phoebe + Norse (20) + Inuit (1) + Gallic (2) + named moons.
- **Uranus (27):** Major (5) + inner (13) + irregular (9).
- **Neptune (14):** Triton + inner (7) + outer irregular (6).
- **Pluto (1):** Charon.

### Asteroids (450)
Main belt + Jupiter Trojans (L4/L5) + near-Earth objects (Apollo / Amor /
Aten / Atira). Spectral classes C / S / M / V / D / P are assigned by the
procedural generator with real taxonomic proportions (~75% C, ~17% S,
~8% M, plus rare V/D/P).

### Comets (50)
Short-period (e.g. 1P/Halley, 2P/Encke, 9P/Tempel, 67P/Churyumov-Gerasimenko,
109P/Swift-Tuttle), long-period (Hale-Bopp, Hyakutake, McNaught, NEOWISE, …),
and Jupiter-family (Shoemaker-Levy 9).

## Sidecar Data Files

The catalog in `solar_system.ron` carries bodies only. Three sidecar files
augment it with gameplay, astrometric, and archival data:

### `assets/data/asteroids.ron` (GRA-313)
- Per-asteroid gameplay data: `body` (joins on `solar_system.ron` name),
  `asteroid_class` (`CType | SType | MType | VType | DType | PType | Unknown`),
  `total_mass_kg`, `composition: { Resource → mass fraction ≤ 1.0 }`,
  `discovery_tier` (0..=8, gates dossier entries), `delta_v_to_redirect_kms`
  (Option), `terraforming_source: bool`.
- Loader: `src/astronomy/asteroids.rs::AsteroidDataPlugin` joins on the body
  name; unmatched entries warn and skip. The 450-body count above is the
  authoritative population — `asteroids.ron` augments, never replaces it.
- Modding: copy an existing entry. See `docs/design/ASTEROID_ENTITIES.md` §3.1.

### `assets/data/nearest_stars.ron` + `assets/data/nearest_stars_raw.json`
- `nearest_stars.ron` is the **position table** of the 60 closest star systems
  to Sol (light-year Cartesian J2000 coordinates + spectral type). Static,
  checked in.
- `nearest_stars_raw.json` (~45 KB) carries the **astrometric + exoplanet data**
  for those same 60 systems: stellar mass/radius/temp/luminosity, [Fe/H]
  metallicity, confirmed planets. Loaded once at startup into
  `NearbyStarsData` by `src/astronomy/nearby_stars.rs`.
- Authoritative count: 60 systems (matches the 60-entry `NEARBY_STARS_POSITIONS`
  slice in `nearby_stars.rs`).

### Reference dumps (not loaded)
- `assets/data/Exoplanets_NASA.csv` — ~5,000+ confirmed exoplanets from the
  NASA Exoplanet Archive. **Staged only**; the CSV → `ConfirmedPlanet` loader
  is deferred to v0.6 (see `assets/data/README.md`).
- `assets/data/JPL_SmallBodiesList.csv`, `assets/data/JPL_CometsList.csv` —
  JPL reference dumps kept for modders and future re-seeding; not loaded at
  runtime.

## Multi-Star System Data

The 60 nearby star systems populate the starmap view and the
`system_populator` chain (`src/astronomy/nearby_stars.rs` + `src/plugins/system_populator.rs`):

- **Source**: `assets/data/nearest_stars_raw.json` + position table.
- **Real data**: ~40 stars include measured [Fe/H] metallicity; the rest get
  a procedural fallback in `[-0.5, +0.5]`.
- **Confirmed exoplanets**: when present in the JSON, spawn with real
  mass / radius / period. Procedural generation fills gaps to ~5 planets
  per system + 80% asteroid-belt chance + 70% cometary cloud chance.
- **Stellar materials**: multi-layer `src/plugins/star_materials.rs`
  (Glow / Surface / Diffraction / Corona / Halo), LOD per Performance.

Historical design notes: see `docs/archive/MULTI_STAR_SYSTEMS.md`.

## Exoplanet Catalogue (deferred to v0.6)

`assets/data/Exoplanets_NASA.csv` is staged but **not loaded at runtime in
v0.5.x**. The CSV → `ConfirmedPlanet` deserialiser and integration with the
procedural gap-filler ship in v0.6. The 5,000+ entries are kept under version
control so modders can pre-stage custom dumps in the same schema and the
v0.6 loader can ship without re-fetching the upstream archive. See
`assets/data/EXOPLANETS_IMPLEMENTATION.md` for the loader stub and field
mapping.

## Orbital & Physical Parameters

Each body entry includes:

- **Orbital**: `semi_major_axis` (AU), `eccentricity`, `inclination` (°),
  `orbital_period` (Earth days), `initial_angle` (°).
- **Physical**: `mass` (kg), `radius` (km), `color` / `emissive` (RGB 0–1),
  `rotation_period` (Earth days; negative = retrograde).
- **Atmospheric**: `AtmosphereComposition` (gas + mole-fraction %; drives the
  Rayleigh/Mie scattering parameters in `src/plugins/atmosphere.rs`).
- **Texture layers**: optional `texture` (surface), `night_lights_texture`,
  `cloud_texture`, `atmosphere_texture`, `scattering_replaces_clouds: bool`,
  and RON overrides for `scale_height_km`, `rayleigh_rgb`,
  `rayleigh_strength`, `mie_strength`, `mie_g`, `haze_color`,
  `atmosphere_intensity`. Layer ordering: surface (1.0×) → night lights
  (1.002×) → cloud deck (1.015×) → scattering shell (1.05×).

## Visualization Scaling

### Distance
- **1 AU = 50 game units.** Mercury 19.35, Earth 50, Pluto 1,974.

### Radius
- **Scale factor 0.0001**, **minimum 0.3 units** for visibility. The Sun clamps
  to ~5 units via the minimum; Earth renders at ~0.64 units; small asteroids
  use the minimum.

### Time
- **Time multiplier 1000×** for visual orbit motion. Earth year ≈ 31 s in-game;
  Jupiter orbit ≈ 6 minutes.

## Performance Considerations

### Actual implementation
- **713 solar-system bodies + 60 nearby star systems** rendered simultaneously.
- LOD distances tiered by body class:
  - **Stars** — full 5-layer material (`star_materials.rs`: Glow + Surface +
    Diffraction + Corona + Halo) < 100 AU; simplified corona 100–1,000 AU;
    billboard glow > 1,000 AU.
  - **Planets / Gas giants / Dwarf planets** — full mesh + atmosphere shell
    < 50 AU; sphere only 50–500 AU; billboard > 500 AU.
  - **Moons** — full mesh < 5 AU; billboard > 5 AU.
  - **Rings** — full particle ring < 20 AU; billboard > 20 AU.
  - **Asteroids / Comets** — instanced billboard when distance > 10 AU; full
    mesh within 10 AU. C-class share one dark-rock material; V/D/P tint from
    the spectral table.
- 2D orbital plane (inclination partial); no collisions; no N-body.

Modern GPUs handle this comfortably as of v0.5.x; per-frame cost is dominated
by the 5-layer star materials (≤ 60 stars active) and instanced asteroid
billboards. Future work: Bevy built-in frustum culling, distance-based
far-plane culling, swapped LOD meshes.

## Adding New Bodies

1. Open `assets/data/solar_system.ron`.
2. Add an entry with the schema below:

```rust
(
    name: "NewBody",
    body_type: Asteroid,        // Star | Planet | GasGiant | DwarfPlanet
                                // | M and append an entry:

```rust
(
    name: "NewBody",
    body_type: Asteroid,        // Star | Planet | GasGiant | DwarfPlanet
                                // | Moon | Asteroid | Comet | Ring
    mass: 1.0e20,               // kg
    radius: 100.0,              // km
    color: (0.7, 0.7, 0.7),     // RGB 0–1
    emissive: (0.0, 0.0, 0.0),  // RGB 0–1
    parent: Some("Sol"),        // Parent body name or None
    orbit: Some((
        semi_major_axis: 2.5, eccentricity: 0.05, inclination: 5.0,
        orbital_period: 1000.0, initial_angle: 0.0,
    )),
    rotation_period: 0.5,       // Earth days (negative = retrograde)
),
```

2. For asteroids with custom gameplay data, add a matching entry to
   `assets/data/asteroids.ron` keyed on the new `body` name.
3. Save and restarto perturbations / gravitational interactions.
- ⚠️ Eccentricity is used for orbit rendering but not for orbit propagation
  in the gameplay sense — bodies still travel their Keplerian ellipse at the
  analytical rate.

## Educational Value

Accurate scale data makes the simulation useful for learning relative sizes,
orbital speeds, the asteroid belt distribution, moon-system hierarchies,
and dwarf-planet diversity.

## References & See also

NASA JPL Horizons (https://ssd.jpl.nasa.gov/horizons/) · IAU Minor Planet
Center (https://minorplanetcenter.net/) · NASA Planetary Fact Sheets
(https://nssdc.gsfc.nasa.gov/planetary/) · NASA Exoplanet Archive
(https://exoplanetarchive.ipac.caltech.edu/) · `docs/ASTRONOMY.md`
(procedural-generation chain, spectral classification) ·
`docs/archive/MULTI_STAR_SYSTEMS.md` (multi-star design history, archived 2026) ·
`docs/design/ASTEROID_ENTITIES.md` (asteroid gameplay sidecar schema) ·
`assets/data/README.md` (sidecar inventory and CSV loader deferral note).