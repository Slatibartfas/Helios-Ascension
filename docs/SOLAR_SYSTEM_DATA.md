# Solar System Data Documentation

## Overview

Helios Ascension uses realistic astronomical data for all celestial bodies in the solar system. This data is stored in the `assets/data/solar_system.ron` file and loaded at runtime.

## Data Sources

All astronomical data is based on real-world measurements from:
- NASA JPL Horizons System
- International Astronomical Union (IAU)
- Planetary and Lunar Coordinates
- Minor Planet Center

## Celestial Bodies

### Star
- **Sol** (The Sun)
  - Mass: 1.9885×10³⁰ kg
  - Radius: 695,700 km
  - Central star of the solar system

### Planets (8)
1. **Mercury** - Smallest planet, closest to the Sun
2. **Venus** - Hottest planet with thick atmosphere
3. **Earth** - Our home planet
4. **Mars** - The Red Planet
5. **Jupiter** - Largest gas giant
6. **Saturn** - Ringed gas giant
7. **Uranus** - Ice giant with extreme axial tilt
8. **Neptune** - Outermost planet, deep blue

### Dwarf Planets (5)
1. **Ceres** - Largest object in the asteroid belt
2. **Pluto** - Former 9th planet, now classified as dwarf planet
3. **Eris** - Massive trans-Neptunian object
4. **Makemake** - Bright Kuiper belt object
5. **Haumea** - Fast-rotating elongated dwarf planet

### Major Moons (12+)

**Earth System:**
- Moon - Earth's only natural satellite

**Mars System:**
- Phobos - Larger moon, irregular shape
- Deimos - Smaller, outer moon

**Jupiter System (Galilean Moons):**
- Io - Volcanically active
- Europa - Subsurface ocean candidate
- Ganymede - Largest moon in solar system
- Callisto - Heavily cratered ancient surface

**Saturn System:**
- Titan - Second-largest moon, thick atmosphere
- Rhea - Second-largest moon of Saturn

**Uranus System:**
- Titania - Largest moon of Uranus

**Neptune System:**
- Triton - Largest moon with retrograde orbit

**Pluto System:**
- Charon - Large moon relative to Pluto

### Main Belt Asteroids (10)
1. **Vesta** - Brightest asteroid, differentiated body
2. **Pallas** - Second-largest asteroid
3. **Hygiea** - Dark C-type asteroid
4. **Interamnia** - Large dark asteroid
5. **Davida** - One of largest asteroids
6. **Cybele** - Outer main belt asteroid
7. **52 Europa** - Bright asteroid
8. **Sylvia** - Triple asteroid system
9. **Thisbe** - Large C-type asteroid
10. **10 Hygiea** (Note: Listed with various names)

## Orbital Parameters

Each celestial body includes the following orbital parameters:

- **Semi-major axis**: Average orbital distance (in AU)
- **Eccentricity**: Orbital shape (0 = circle, <1 = ellipse)
- **Inclination**: Tilt of orbit relative to ecliptic (degrees)
- **Orbital period**: Time to complete one orbit (Earth days)
- **Initial angle**: Starting position in orbit (degrees)

## Physical Properties

Each body includes:

- **Mass**: In kilograms
- **Radius**: In kilometers
- **Color**: RGB values for rendering (0.0 to 1.0)
- **Emissive**: RGB emissive color (for stars)
- **Rotation period**: In Earth days (negative = retrograde)

## Visualization Scaling

To make the solar system viewable in a 3D game engine, we apply scaling:

### Distance Scaling
- **1 AU = 50 game units**
- Mercury: 0.387 AU → 19.35 units
- Earth: 1.0 AU → 50 units
- Pluto: 39.48 AU → 1,974 units

### Radius Scaling
- **Scale factor: 0.0001**
- **Minimum radius: 0.3 game units** (for visibility)
- Sun: 695,700 km → ~5 units (with minimum)
- Earth: 6,371 km → ~0.64 units
- Small asteroids: Uses minimum for visibility

### Time Scaling
- **Time multiplier: 1000x**
- Makes orbital motion visible at game speeds
- Earth year (365 days) takes ~31 seconds in-game
- Jupiter orbit (12 years) takes ~6 minutes in-game

## Performance Considerations

### Current Implementation
- **40+ bodies** rendered simultaneously
- Simplified circular orbits (eccentricity stored but not used)
- 2D orbital plane (inclination partially implemented)
- No collision detection
- No gravitational interactions

### Future Optimizations
1. **Level of Detail (LOD)**
   - Reduce mesh complexity for distant objects
   - Hide very small distant asteroids

2. **Instancing**
   - Use GPU instancing for similar objects
   - Batch asteroid rendering

3. **Culling**
   - Frustum culling for off-screen objects
   - Distance-based culling for very far objects

4. **More Objects**
   - Can easily add 100s more asteroids
   - Kuiper belt could have 50+ objects
   - Moon systems can be completed (200+ moons)

## Adding New Bodies

To add a new celestial body:

1. Open `assets/data/solar_system.ron`
2. Add a new body entry with all required fields:

```rust
(
    name: "NewBody",
    body_type: Asteroid, // Star, Planet, DwarfPlanet, Moon, Asteroid
    mass: 1.0e20,        // kg
    radius: 100.0,       // km
    color: (0.7, 0.7, 0.7), // RGB 0-1
    emissive: (0.0, 0.0, 0.0), // RGB 0-1
    parent: Some("Sol"), // Parent body name or None
    orbit: Some((
        semi_major_axis: 2.5, // AU
        eccentricity: 0.05,
        inclination: 5.0,      // degrees
        orbital_period: 1000.0, // days
        initial_angle: 0.0,     // degrees
    )),
    rotation_period: 0.5, // days
),
```

3. Save and restart the game
4. The new body will be loaded automatically

## Data Accuracy

The data in this simulation is:
- ✅ Realistic masses and radii
- ✅ Accurate semi-major axes
- ✅ Real eccentricities
- ✅ Correct inclinations
- ✅ Accurate orbital periods
- ✅ Realistic rotation periods
- ⚠️ Simplified to 2D orbits for now
- ⚠️ No perturbations or gravitational interactions
- ⚠️ Circular approximation (eccentricity not fully implemented)

## Educational Value

This accurate data makes the simulation useful for:
- Learning relative sizes of planets
- Understanding orbital speeds and periods
- Visualizing the scale of the solar system
- Exploring moon systems
- Seeing asteroid belt distribution
- Comparing dwarf planets to planets

## References

1. NASA JPL Horizons: https://ssd.jpl.nasa.gov/horizons/
2. IAU Minor Planet Center: https://minorplanetcenter.net/
3. NASA Planetary Fact Sheets: https://nssdc.gsfc.nasa.gov/planetary/
4. Wikipedia Planetary Data: Various planet articles
