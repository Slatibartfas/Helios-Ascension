# Active Procedural Generation - User Guide

## What Happens at Game Start

When you launch Helios: Ascension, the game automatically:

1. **Generates a unique universe seed** from the current system time
2. **Loads the Sol system** (our solar system) from `assets/data/solar_system.ron`
3. **Loads nearby star data** from `assets/data/nearest_stars_raw.json` (~60 star systems)
4. **Procedurally generates each star system:**
   - Spawns the star with a random metallicity
   - Places any confirmed exoplanets from NASA data
   - Fills in missing planets (targeting 5 planets per system)
   - Adds asteroid belts (80% chance)
   - Adds cometary clouds (70% chance)
   - Generates resources based on planet type and star metallicity

**Result:** Every playthrough is unique with a different procedurally generated galaxy!

## Game Seed System

### What is a Game Seed?

The **Game Seed** is a number that determines all procedural generation. The same seed always creates the same universe.

### Current Behavior

- **Each game gets a unique seed** based on the system time when you start
- The seed is logged to the console: `Generated game seed from system time: 1738999999`
- This makes every playthrough different

### For Testing/Debugging

You can force a specific seed for reproducible testing by modifying `src/main.rs`:

```rust
fn main() {
    App::new()
        // ... plugins ...
        .insert_resource(GameSeed::new(12345))  // Force specific seed
        // ... more plugins ...
}
```

Or use a named seed:

```rust
.insert_resource(GameSeed::from_string("alpha_centauri_test"))
```

## Viewing Generated Systems

### In the Console

When the game starts, you'll see log messages like:

```
INFO helios_ascension::plugins::system_populator: Starting procedural population of nearby star systems with seed 1738999999
INFO helios_ascension::plugins::system_populator: Populating system 'Alpha Centauri' at 4.25 ly with 3 stars
INFO helios_ascension::plugins::system_populator: Spawning star 'Alpha Centauri A' (G2V): L=1.519L☉, frost_line=5.98AU, [Fe/H]=0.23
INFO helios_ascension::plugins::system_populator: Generated 3 rocky planets, 2 gas giants for 'Alpha Centauri'
...
INFO helios_ascension::plugins::system_populator: Completed procedural population of 60 star systems
```

### In the Game

- Sol (System 0) is at the origin
- Other systems are placed along the X-axis at their actual distances
- Use the starmap view to see all systems
- Each system has its own planets, asteroids, and comets

## What Makes Each System Unique?

1. **Metallicity** - Each star gets a random metallicity (-0.5 to +0.5)
   - Affects rare metal and fissile material abundance
   - Metal-rich stars have 30% more valuable resources

2. **Planet Count** - Systems target 5 planets but vary:
   - Rocky planets: 2-4 (inside frost line)
   - Gas giants: 1-3 (outside frost line)
   - Number depends on existing confirmed planets

3. **Asteroid Belts** - 80% chance of spawning
   - Location varies based on frost line
   - Contains M, S, and V type asteroids

4. **Cometary Clouds** - 70% chance of spawning
   - 20-80 comets in outer system
   - High in volatiles (water, ammonia, methane)

5. **Orbital Parameters** - All planets have:
   - Semi-major axis (distance from star)
   - Eccentricity (orbit shape)
   - Random inclination, longitude, and mean anomaly

## Save/Load Support (Future)

The system is **designed for save/load** but not yet implemented:

- `GameSeed` is serializable
- Can be saved to a file
- Loading a save will restore the seed
- Same seed = same universe every time

## Configuration Options (Future)

Potential future options:
- Choose seed at game start (UI input)
- Limit number of systems to generate
- Toggle procedural generation on/off
- Export/import seeds for sharing universes

## Technical Details

### Generation is Deterministic

- Same seed always produces the same universe
- Uses Rust's `StdRng` with the seed
- All random numbers come from this RNG
- Guarantees reproducibility

### Generation Happens Once

- All systems generate at game start (Startup stage)
- Takes a few seconds for 60 systems
- No runtime generation (no performance impact during gameplay)
- Systems are persistent until game closes

### Resource Generation

- Resources generate automatically for all spawned bodies
- Uses the existing `generate_solar_system_resources` system
- Applies metallicity bonuses from star properties
- Follows frost line rules (volatiles vs metals)

## Example: Alpha Centauri System

With seed `12345`, Alpha Centauri might generate:

```
Star: Alpha Centauri A (G2V)
- Luminosity: 1.519 L☉
- Frost Line: 5.98 AU
- Metallicity: +0.23 [Fe/H] (metal-rich)

Confirmed Planets:
- Proxima Centauri b (Real): 0.0485 AU, 1.17 M⊕
- Proxima Centauri d (Real): 0.0289 AU, 0.26 M⊕

Procedural Planets:
- Alpha Centauri A c: 1.2 AU, 2.1 M⊕ (rocky)
- Alpha Centauri A d: 3.8 AU, 0.9 M⊕ (rocky)
- Alpha Centauri A e: 9.2 AU, 67 M⊕ (ice giant)
- Alpha Centauri A f: 18.5 AU, 185 M⊕ (gas giant)

Asteroid Belt: 11-14 AU (142 asteroids)
Cometary Cloud: 25-50 AU (58 comets)

Resources:
- Rare metals: 118% of normal (metal-rich bonus)
- Construction materials: Normal
- Volatiles: High (beyond frost line)
```

## Performance

Generation of 60 systems typically takes:
- **System spawn:** ~50-100ms per system
- **Planet generation:** ~10-20ms per planet
- **Total startup time:** 3-5 seconds for all systems

This happens once at startup, so there's no impact on gameplay performance.

## Debugging

To see detailed generation logs, run with:
```bash
RUST_LOG=helios_ascension=info cargo run
```

Or for even more detail:
```bash
RUST_LOG=helios_ascension=debug cargo run
```

## Known Limitations

1. **No binary stars yet** - Only primary star spawns planets
2. **No circumbinary planets** - Binary orbit support not implemented
3. **Fixed system positions** - Systems placed linearly along X-axis
4. **No galactic structure** - No spiral arms, no 3D distribution
5. **No stellar ages** - All stars treated as main sequence

These will be addressed in future updates.
