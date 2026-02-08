# Procedural Star System Generation

This document describes the procedural generation system for populating star systems in Helios: Ascension.

## Overview

The procedural generation system fills in missing planets, asteroid belts, and cometary clouds for star systems that have incomplete real data. It uses scientifically-based rules to create realistic systems while maintaining gameplay variety.

**The system actively generates at game start**, creating a unique universe for each playthrough using a random seed. This seed can be saved to recreate the same universe.

## Active Generation at Game Start

When you start a new game, the system:
1. Generates a `GameSeed` from the current system time (or a specified value)
2. Loads nearby star data from `assets/data/nearest_stars_raw.json`
3. For each star system (except Sol, which is pre-defined):
   - Spawns the star with random metallicity (-0.5 to +0.5 [Fe/H])
   - Spawns any confirmed exoplanets from the data
   - Generates procedural planets to fill gaps (targeting 5 planets per system)
   - Spawns asteroid belts (80% chance)
   - Spawns cometary clouds (70% chance)
   - Applies resource generation with metallicity bonuses

**Every game is unique** because the seed is based on system time, but **every game is reproducible** because the seed determines all generation.

## Key Components

### 1. Exoplanet Data Integration (`src/astronomy/exoplanets.rs`)

The `ConfirmedPlanet` struct holds real exoplanet data from the NASA Exoplanet Archive:
- Mass, radius, orbital parameters
- Discovery method and year
- Equilibrium temperature
- Mass-radius relationship estimates for missing data

Planets with real data are spawned first and marked with the `RealPlanet` component.

### 2. Procedural Generation Logic (`src/astronomy/procedural.rs`)

The core procedural generation system uses stellar properties to determine system architecture.

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

### 3. Resource Generation with Metallicity (`src/economy/components.rs`, `src/economy/generation.rs`)

Stars have a metallicity value `[Fe/H]` that represents their heavy element abundance relative to the Sun:
- `[Fe/H] = 0.0`: Solar metallicity
- `[Fe/H] > 0.0`: Metal-rich (more heavy elements)
- `[Fe/H] < 0.0`: Metal-poor (fewer heavy elements)

**Metallicity Data Sources:**
- **Real data when available**: 40+ stars have measured metallicity from astronomical databases (SIMBAD, Hypatia Catalog, Geneva-Copenhagen Survey)
- **Random when unavailable**: Stars without measurements get random values (-0.5 to +0.5)
- See `docs/METALLICITY_DATA.md` for complete data sources and references

**Metallicity Multiplier:**
```rust
multiplier = (1.0 + [Fe/H] × 0.6).clamp(0.5, 1.5)
```

**Examples:**
- `[Fe/H] = 0.0`: 1.0× abundance (Solar)
- `[Fe/H] = +0.2`: 1.12× abundance (Alpha Centauri A - real data)
- `[Fe/H] = -0.5`: 0.7× abundance (Barnard's Star - real data)
- `[Fe/H] = +0.5`: 1.3× abundance (Sirius A - real data)
- `[Fe/H] = -0.86`: 0.48× abundance (Kapteyn's Star - very metal-poor)

**Affected Resources:**
The metallicity bonus applies to all tiers (proven, deep, bulk) of:
- Gold
- Silver
- Platinum
- Rare Earths
- Uranium
- Thorium

This means metal-rich systems are more valuable for mining operations, while metal-poor systems require more effort to extract rare materials.

### 4. System Populator Plugin (`src/plugins/system_populator.rs`)

The `SystemPopulatorPlugin` orchestrates the entire procedural generation process at game start:

1. **Initialize GameSeed** (`src/game_state.rs`)
   - Creates seed from system time (unique each game)
   - Or uses specified seed for testing/reproducibility
   - Serializable for save/load functionality

2. **Load nearby star data** from `assets/data/nearest_stars_raw.json`

3. **For each star system:**
   - Spawn star entity with `StarSystem` component
   - Assign random metallicity (-0.5 to +0.5 [Fe/H])
   - Spawn confirmed planets from real data (marked with `RealPlanet`)
   - Generate procedural architecture to fill gaps
   - Spawn procedural planets, asteroids, and comets
   - Apply resource generation with metallicity bonuses

4. **Resource generation runs automatically** for all spawned bodies via the existing `generate_solar_system_resources` system

## Usage

The system automatically runs at game startup via the `SystemPopulatorPlugin`:

```rust
App::new()
    .add_plugins(GameStatePlugin)      // Initialize seed first
    // ... other plugins ...
    .add_plugins(SystemPopulatorPlugin) // Generate systems
    // ...
```

### Controlling the Seed

For testing or specific scenarios, you can override the default seed:

```rust
// In your app setup, before plugins:
app.insert_resource(GameSeed::new(12345));  // Fixed seed
// Or:
app.insert_resource(GameSeed::from_string("my_universe")); // Named seed
```

### Save/Load Support

The `GameSeed` is serializable and will be included in save files:

```rust
#[derive(Resource, Serialize, Deserialize)]
pub struct GameSeed {
    pub value: u64,
}
```

When loading a saved game, restore the seed to ensure consistent universe generation.

## Scientific Basis

### Frost Line
Based on equilibrium temperature calculations for water ice sublimation (~170K). The constant 4.85 AU matches our solar system's observed frost line and provides realistic variation for different stellar types.

### Planet Distribution
- Inner rocky planets: Consistent with observed exoplanet systems and solar system structure
- Outer gas giants: Jupiter/Saturn formation requires volatiles available beyond frost line
- Ice giants: Neptune/Uranus-like bodies form at intermediate distances

### Metallicity Effects
Observations of exoplanet host stars show that metal-rich stars ([Fe/H] > +0.1) are more likely to host giant planets and have higher abundances of heavy elements in their planets. The ±30% variation for ±0.5 dex is conservative compared to observed variations.

### Asteroid Belt Formation
Typically forms where planet formation was disrupted by nearby giant planet (Jupiter in our system). Placed at ~2× frost_line to represent the transition zone between rocky and giant planet formation.

## Gameplay Implications

1. **System Diversity:** Different stellar types create varied system layouts
   - M-dwarfs: Compact, hot systems with frost lines < 0.5 AU
   - G-type (Solar): Moderate systems with frost lines ~5 AU
   - A-type: Expansive systems with frost lines > 20 AU

2. **Resource Distribution:**
   - Volatiles abundant beyond frost line (comets, ice giants)
   - Rocky materials abundant inside frost line (asteroids, rocky planets)
   - Rare metals boosted in metal-rich systems (gameplay incentive)

3. **Strategic Choices:**
   - Target metal-rich systems for rare resource extraction
   - Metal-poor systems require more advanced mining tech
   - Different star types offer different strategic advantages

## Testing

Comprehensive tests in `tests/procedural_generation_tests.rs` validate:
- Frost line calculations for different stellar types
- System generation respects existing planets
- Rocky planets stay inside frost line
- Gas giants stay outside frost line
- Asteroid belts and cometary clouds generate correctly
- Metallicity multipliers apply correctly
- Deterministic generation with fixed random seeds

## Future Enhancements

Potential improvements for future versions:

1. **Binary Star Systems:** Generate circumbinary planets and separate planetary systems around each star
2. **Migration:** Simulate planet migration (hot Jupiters, scattered disk objects)
3. **Resonances:** Place planets in orbital resonances (2:1, 3:2, etc.)
4. **Habitability:** Score planets for colony suitability based on temperature, atmosphere, etc.
5. **Advanced Metallicity:** Different metallicity effects for different resource types (Fe/Si ratio, CNO ratio)
6. **Stellar Age:** Younger systems have more comets and debris, older systems are cleaner
7. **Planetary Rings:** Procedurally generate ring systems for gas giants
8. **Trojan Asteroids:** Place asteroids at Lagrange points of major planets

## References

- NASA Exoplanet Archive: https://exoplanetarchive.ipac.caltech.edu/
- Chen & Kipping (2017): "Probabilistic Forecasting of the Masses and Radii of Other Worlds"
- Raymond et al. (2004): "Making Other Earths: Dynamical Simulations of Terrestrial Planet Formation"
- Santos et al. (2004): "The Planet-Metallicity Correlation"
- Ida & Lin (2004): "Toward a Deterministic Model of Planetary Formation"
