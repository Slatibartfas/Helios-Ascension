# Astronomy System

This document describes the astronomy and procedural generation systems in Helios: Ascension.

The canonical solar-system dataset contains **713 bodies**: 1 star (Sol), 4 terrestrial planets, 4 gas giants, 2 rings, 55 dwarf planets, 147 moons, 450 asteroids, and 50 comets. Procedural generation extends the same rules to the 60 nearby star systems in `assets/data/nearest_stars_raw.json`.

## Table of Contents

1. [Procedural Star System Generation](#procedural-star-system-generation)
2. [Spectral Classification](#spectral-classification)
3. [Stellar Metallicity](#stellar-metallicity)
4. [Asteroid Classification](#asteroid-classification)
5. [Custom Start Dates and Ephemerides](#custom-start-dates-and-ephemerides)
6. [JPL Epoch and Stellar Motion](#jpl-epoch-and-stellar-motion)
7. [Lagrange Points and Multiple-Star Systems](#lagrange-points-and-multiple-star-systems)
8. [Asteroid Sidecar Data](#asteroid-sidecar-data)

---

## Procedural Star System Generation

The procedural generation system fills in missing planets, asteroid belts, and cometary clouds for star systems that have incomplete real data. It uses scientifically-based rules to create realistic systems while maintaining gameplay variety.

**The system actively generates at game start**, creating a unique universe for each playthrough using a random seed. This seed can be saved to recreate the same universe.

### Active Generation at Game Start

When you start a new game, the system:
1. Generates a `GameSeed` from the current system time (or a specified value)
2. Loads nearby star data from `assets/data/nearest_stars_raw.json`
3. For each star system (except Sol, which is pre-defined):
   - Spawns the star with real metallicity data when available (40+ stars), or random fallback (-0.5 to +0.5 [Fe/H])
   - Spawns any confirmed exoplanets from the data
   - Generates procedural planets to fill gaps (targeting 5 planets per system)
   - Spawns asteroid belts (80% chance)
   - Spawns cometary clouds (70% chance)
   - Applies resource generation with metallicity bonuses

**Every game is unique** because the seed is based on system time, but **every game is reproducible** because the seed determines all generation.

### Key Components

#### Exoplanet Data Integration

The `ConfirmedPlanet` struct holds real exoplanet data from the NASA Exoplanet Archive:
- Mass, radius, orbital parameters
- Discovery method and year
- Equilibrium temperature
- Mass-radius relationship estimates for missing data

Planets with real data are spawned first and marked with the `RealPlanet` component.

#### Frost Line Calculation

The frost line is the distance from a star where volatiles (water, ammonia, methane) can condense:

```rust
frost_line_au = 4.85 × √(L/L☉)
```

Where `L` is the star's luminosity in solar units.

**Examples:**
- Sun (G2V, L=1.0): frost line = 4.85 AU
- Alpha Centauri A (G2V, L=1.519): frost line = 5.98 AU
- Proxima Centauri (M5.5Ve, L=0.0017): frost line = 0.20 AU
- Sirius A (A1V, L=25.4): frost line = 24.4 AU

#### System Architecture

The `map_star_to_system_architecture` function generates complete system layouts:

**Target:** 5 planets per system (if fewer exist)

**Inner System (inside frost line):**
- 2-4 rocky planets
- Semi-major axis: 0.3 AU to 0.95 × frost_line
- Mass: 0.3-3.5 M⊕ (Sub-Earth to Super-Earth)
- Eccentricity: 0.0-0.15 (low)
- Minimum separation: 0.1 AU

**Asteroid Belt:**
- 80% probability
- Location: typically at 2.0 × frost_line ± 30%
- Width: 0.5-1.5 AU
- Count: 50-200 asteroids
- Types: C, S, M, V, D, and P

**Outer System (outside frost line):**
- 1-3 gas/ice giants
- Semi-major axis: 1.2 × frost_line to 30 AU
- Mass: Ice giants 10-25 M⊕, Gas giants 50-400 M⊕
- Eccentricity: 0.0-0.25 (moderate)
- Minimum separation: 0.5 AU

**Cometary Cloud:**
- 70% probability
- Location: 20-50 AU (or 4× frost_line, whichever is greater)
- Count: 20-80 comets
- Types: P (primitive) and D (dark, volatile-rich)
- Inclination: 0-60° (spherical distribution)

### Scientific Basis

The frost line calculation is based on equilibrium temperature calculations for water ice sublimation (~170K). The constant 4.85 AU matches our solar system's observed frost line and provides realistic variation for different stellar types.

---

## Spectral Classification

Stars are classified using the Morgan-Keenan (MK) system based on their spectral characteristics and luminosity. This system provides a standardized way to describe stellar properties.

### Spectral Classes

**O-type (Blue):**
- Temperature: 30,000-50,000 K
- Color: Blue
- Mass: 16-90+ M☉
- Examples: None within 20 ly (extremely rare)

**B-type (Blue-white):**
- Temperature: 10,000-30,000 K
- Color: Blue-white
- Mass: 2.1-16 M☉
- Examples: None within 20 ly (rare)

**A-type (White):**
- Temperature: 7,500-10,000 K
- Color: White
- Mass: 1.4-2.1 M☉
- Examples: Sirius A (A1V), Vega (A0V), Altair (A7V)

**F-type (Yellow-white):**
- Temperature: 6,000-7,500 K
- Color: Yellow-white
- Mass: 1.04-1.4 M☉
- Examples: Procyon A (F5IV-V)

**G-type (Yellow):**
- Temperature: 5,200-6,000 K
- Color: Yellow
- Mass: 0.8-1.04 M☉
- Examples: Sun (G2V), Alpha Centauri A (G2V), Tau Ceti (G8V)

**K-type (Orange):**
- Temperature: 3,700-5,200 K
- Color: Orange
- Mass: 0.45-0.8 M☉
- Examples: Alpha Centauri B (K1V), Epsilon Eridani (K2V), 61 Cygni A (K5V)

**M-type (Red):**
- Temperature: 2,400-3,700 K
- Color: Red
- Mass: 0.08-0.45 M☉
- Examples: Proxima Centauri (M5.5Ve), Barnard's Star (M4Ve), Wolf 359 (M6V)

### Luminosity Classes

- **V:** Main sequence (dwarfs)
- **IV:** Subgiants
- **III:** Giants
- **II:** Bright giants
- **I:** Supergiants

### Game Implementation

Each star system carries a `SpectralClass` field on its `StarSystem` component. It affects:
- Visual appearance (color, brightness)
- Frost line calculation (luminosity-dependent)
- Planetary system architecture
- Resource distribution via metallicity

---

## Stellar Metallicity

Stars have varying amounts of heavy elements (metals) beyond hydrogen and helium. This metallicity is expressed as [Fe/H], the logarithmic iron abundance relative to the Sun.

### Metallicity Scale

- **[Fe/H] = 0.0:** Solar metallicity (reference)
- **[Fe/H] > 0.0:** Metal-rich (more heavy elements)
- **[Fe/H] < 0.0:** Metal-poor (fewer heavy elements)

### Real Data Sources

40+ stars in the game have measured metallicity from:
- SIMBAD Astronomical Database
- Hypatia Catalog (stellar abundances)
- Geneva-Copenhagen Survey

### Notable Star Metallicities

| Star | [Fe/H] | Classification |
|------|--------|----------------|
| Alpha Centauri A | +0.20 | Metal-rich |
| Procyon A | 0.00 | Solar metallicity |
| Tau Ceti | -0.50 | Metal-poor |
| Barnard's Star | -0.50 | Metal-poor |
| Sirius A | +0.50 | Very metal-rich |
| Kapteyn's Star | -0.86 | Very metal-poor |

### Gameplay Impact

Metallicity affects resource abundance in star systems:

```rust
multiplier = (1.0 + [Fe/H] × 0.6).clamp(0.5, 1.5)
```

**Examples:**
- [Fe/H] = 0.0: 1.0× abundance (Solar)
- [Fe/H] = +0.2: 1.12× abundance
- [Fe/H] = -0.5: 0.7× abundance
- [Fe/H] = +0.5: 1.3× abundance

**Affected Resources:**
- Gold, Silver, Platinum
- Rare Earths
- Uranium, Thorium

Metal-rich systems are more valuable for mining operations, while metal-poor systems require more effort to extract rare materials.

---

## Asteroid Classification

Asteroids use six implemented compositional classes in `src/plugins/solar_system_data.rs::AsteroidClass`. The procedural belt distribution in `src/astronomy/procedural.rs` uses their broad orbital and spectral tendencies; `Unknown` remains available for data that cannot be classified.

| Class | Meaning and typical composition | Gameplay emphasis |
|---|---|---|
| **C** (`CType`) | Carbonaceous, dark, carbon-rich; hydrated minerals and volatiles | Water, ammonia, methane, hydrogen, organics |
| **S** (`SType`) | Silicaceous/stony; silicates with iron and nickel | Silicates, iron, aluminum, titanium, magnesium |
| **M** (`MType`) | Metallic; iron-nickel body with trace noble metals | Iron, nickel, copper, gold, platinum, rare earths |
| **V** (`VType`) | Vesta-like basaltic fragments from differentiated crust | Basaltic silicates, aluminum, titanium, iron |
| **D** (`DType`) | Dark/red outer-belt or Trojan material, carbon-rich and volatile-bearing | Volatiles and organics |
| **P** (`PType`) | Primitive Tagish Lake-like material, especially in outer regions | Very high volatiles, low metal content |

Visual appearance, albedo, deposits, and mining economics vary by class. C and S dominate the common belt populations; M and V are less common differentiated or metal-rich targets, while D and P are associated with dark, primitive outer-system populations.

---

## Custom Start Dates and Ephemerides

A new game may begin at any Unix timestamp. `src/astronomy/ephemeris.rs::calculate_positions_at_timestamp` evaluates the J2000 orbital elements at that date and returns mean anomalies in degrees for the named major bodies.

To configure a start date, create `SimulationTime` with `SimulationTime::with_start_timestamp(timestamp)` in `src/ui/time.rs`. During body setup, calculate the positions for the same timestamp and override each body's `KeplerOrbit.mean_anomaly_epoch` (or the loaded initial angle) before orbit propagation begins. Simulation elapsed time then starts at zero while the displayed calendar and analytical orbit propagation use the chosen date.

The default baseline is **2026-01-01 00:00 UTC**. Keep the clock timestamp and the ephemeris timestamp identical so the initial visual positions and displayed date agree.

---

## JPL Epoch and Stellar Motion

`src/astronomy/star_epoch.rs` loads the 60-system stellar ephemeris and provides the shared epoch used by interstellar positioning and transfer calculations. The catalog is anchored to game start (`EPOCH_BEACON_GAME_START_SIM_S = 0.0`), with positions in Galactic Cartesian J2000 coordinates and measured velocities where available.

For Sol and the major planets, `ephemeris.rs` uses NASA JPL-style J2000 orbital elements. The 2026-01-01 baseline is the canonical game-start epoch; the JPL-derived mean anomalies are applied during setup rather than relying on stale authored angles.

---

## Lagrange Points and Multiple-Star Systems

`src/astronomy/lagrange.rs` computes and renders the five classical equilibrium points for a primary-secondary pair: **L1** and **L2** lie between or beyond the bodies, **L3** lies opposite the secondary, and **L4/L5** form the equilateral triangle points. Hover and selection support the same L1–L5 targets used by transfer planning.

Procedural generation also supports binary and multi-star systems. `BinaryCompanionContext` models the companion separation, eccentricity, mass fraction, and orbital inclination for S-type (circumstellar) planets. Generated planets receive secular forced-eccentricity and binary-plane inclination effects, while system generation uses the relevant stellar luminosity and stability constraints.

Historical design notes: [SURVEY_REWORK.md](design/SURVEY_REWORK.md) and [MULTI_STAR_SYSTEMS.md](archive/MULTI_STAR_SYSTEMS.md) are archived references, not current implementation specifications. Current behavior is defined by the Rust modules and data files described here.

---

## Asteroid Sidecar Data

`assets/data/asteroids.ron` is the gameplay sidecar for the 450 asteroid entries in `solar_system.ron`. It joins records by body name and stores the class, composition, discovery tier, redirect Δv, terraforming-source flag, and lore metadata. Its provenance is the JPL small-body catalog and related survey data; it is intentionally separate from the main body/orbit definitions.

Future ingestion can expand or refresh the sidecar from `assets/data/JPL_SmallBodiesList.csv`. New records must preserve the body-name join and valid composition fractions, and should use the six-class taxonomy above.

---

## References

- NASA Exoplanet Archive: https://exoplanetarchive.ipac.caltech.edu/
- SIMBAD Astronomical Database: http://simbad.u-strasbg.fr/
- Hypatia Catalog: https://www.hypatiacatalog.com/
- Geneva-Copenhagen Survey
- NASA/JPL Small-Body Database and Horizons data
- Chen & Kipping (2017): "Probabilistic Forecasting of the Masses and Radii of Other Worlds"
- Raymond et al. (2004): "Making Other Earths: Dynamical Simulations of Terrestrial Planet Formation"
- Santos et al. (2004): "The Planet-Metallicity Correlation"
