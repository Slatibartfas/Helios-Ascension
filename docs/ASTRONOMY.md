# Astronomy System

This document describes the astronomy and procedural generation systems in Helios: Ascension.

## Table of Contents

1. [Procedural Star System Generation](#procedural-star-system-generation)
2. [Spectral Classification](#spectral-classification)
3. [Stellar Metallicity](#stellar-metallicity)
4. [Asteroid Classification](#asteroid-classification)

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
- Types: M (metal), S (silicate), V (basaltic)

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

Each star in the game has a `SpectralClass` component that affects:
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

Asteroids are classified based on their spectral properties and composition. The game implements three major asteroid types based on real astronomical observations.

### Classification System

#### M-type (Metallic)

**Composition:**
- 85-95% iron and nickel
- 5-15% silicates
- Trace: gold, platinum, other rare metals

**Resources:**
- High: Iron (75-90%), Nickel (20-25%)
- Medium: Platinum, Gold, Silver
- Low: Silicates (5-10%)

**Visual Characteristics:**
- Color: Metallic gray
- Albedo: 0.10-0.18 (relatively bright)
- Texture: Metallic, crater-marked

**Examples:**
- 16 Psyche (largest known M-type)
- Cleopatra asteroid

#### S-type (Silicaceous/Stony)

**Composition:**
- 60-70% silicates
- 20-30% metals (iron, nickel)
- <1% volatiles (water ice)

**Resources:**
- High: Silicates (60-70%), Iron (15-25%)
- Medium: Aluminum, Titanium, Nickel
- Very Low: Water (<1%)

**Visual Characteristics:**
- Color: Gray to reddish-gray
- Albedo: 0.10-0.22
- Texture: Rocky, cratered

**Examples:**
- 433 Eros
- 951 Gaspra

#### V-type (Basaltic)

**Composition:**
- 70-80% basaltic minerals (pyroxene, plagioclase)
- 15-20% metals
- Differentiated crust material

**Resources:**
- High: Silicates (70-80%), Aluminum (8-12%)
- Medium: Titanium, Iron
- Low: Rare Earth elements

**Visual Characteristics:**
- Color: Dark gray to black with reddish tint
- Albedo: 0.30-0.40 (brightest of asteroid types)
- Texture: Basaltic, smooth volcanic flows

**Examples:**
- 4 Vesta (progenitor body)
- Vestoid family

### Game Implementation

Asteroids in belts are procedurally assigned types with realistic distributions:
- M-type: ~5-10% (rare but valuable)
- S-type: ~70-80% (most common)
- V-type: ~5-10% (differentiated fragments)

Each asteroid type has distinct visual appearance, resource deposits, and mining economics.

---

## References

- NASA Exoplanet Archive: https://exoplanetarchive.ipac.caltech.edu/
- SIMBAD Astronomical Database: http://simbad.u-strasbg.fr/
- Hypatia Catalog: https://www.hypatiacatalog.com/
- Geneva-Copenhagen Survey
- Chen & Kipping (2017): "Probabilistic Forecasting of the Masses and Radii of Other Worlds"
- Raymond et al. (2004): "Making Other Earths: Dynamical Simulations of Terrestrial Planet Formation"
- Santos et al. (2004): "The Planet-Metallicity Correlation"
