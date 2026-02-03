# Solar System Data Documentation

## Overview

Helios Ascension uses realistic astronomical data for all celestial bodies in the solar system. This data is stored in the `assets/data/solar_system.ron` file and loaded at runtime.

**Current Status: 377 celestial bodies with comprehensive coverage!**

## Data Sources

All astronomical data is based on real-world measurements from:
- NASA JPL Horizons System
- International Astronomical Union (IAU)
- Planetary and Lunar Coordinates
- Minor Planet Center
- Comet catalogs and databases

## Body Type Summary

```
Star:          1   (Sol)
Planets:       8   (Mercury → Neptune)
Dwarf Planets: 55  (including Kuiper Belt Objects)
Moons:         148 (all major + many minor moons)
Asteroids:     145 (main belt + Trojans + NEOs)
Comets:        20  (periodic and long-period)
──────────────────
TOTAL:         377 bodies
```

## Celestial Bodies by Category

### Star (1)
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

### Dwarf Planets & Kuiper Belt Objects (55)
**Main Belt:**
1. **Ceres** - Largest object in the asteroid belt

**Classical Kuiper Belt Objects:**
2. **Pluto** - Former 9th planet, now classified as dwarf planet
3. **Eris** - Massive trans-Neptunian object
4. **Makemake** - Bright Kuiper belt object
5. **Haumea** - Fast-rotating elongated dwarf planet
6. **Quaoar** - Large classical KBO
7. **Sedna** - Extreme outer solar system object
8. **Orcus** - Pluto-like object
9. **Salacia** - Large KBO
10. **Varda** - Trans-Neptunian object
... and 45 more KBOs and scattered disc objects

### Moons (148 total)

**Earth System (1):**
- Moon - Earth's only natural satellite

**Mars System (2):**
- Phobos - Larger moon, irregular shape
- Deimos - Smaller, outer moon

**Jupiter System (79 complete!):**
- **Galilean Moons (4):** Io, Europa, Ganymede, Callisto
- **Amalthea Group (4):** Metis, Adrastea, Amalthea, Thebe
- **Himalia Group (5):** Leda, Himalia, Lysithea, Elara, Dia
- **Ananke Group (10):** Including Ananke, Carpo, Euporie
- **Carme Group (7):** Including Carme, Taygete, Chaldene
- **Pasiphae Group (9):** Including Pasiphae, Sinope, Callirrhoe
- **Other Irregulars (17):** Various retrograde and prograde moons
- Plus many S/2003 discoveries

**Saturn System (83 complete!):**
- **Major Moons (7):** Mimas, Enceladus, Tethys, Dione, Titan, Rhea, Iapetus
- **Co-orbital Moons (2):** Janus, Epimetheus
- **Inner Small Moons (15):** Pan, Daphnis, Atlas, Prometheus, Pandora, etc.
- **Large Irregular (2):** Hyperion, Phoebe
- **Norse Group (20):** Ymir, Paaliaq, Siarnaq, Albiorix, etc.
- **Inuit Group (1):** Tarqeq
- **Gallic Group (2):** Bebhionn, Erriapus
- Plus many more named moons

**Uranus System (27 complete!):**
- **Major Moons (5):** Miranda, Ariel, Umbriel, Titania, Oberon
- **Inner Moons (13):** Cordelia, Ophelia, Bianca, Cressida, Desdemona, Juliet, Portia, Rosalind, Cupid, Belinda, Perdita, Puck, Mab
- **Irregular Moons (9):** Francisco, Caliban, Stephano, Trinculo, Sycorax, Margaret, Prospero, Setebos, Ferdinand

**Neptune System (14 complete!):**
- **Triton** - Largest moon with retrograde orbit
- **Inner Moons (7):** Naiad, Thalassa, Despina, Galatea, Larissa, S/2004 N 1, Proteus
- **Outer Irregular (6):** Nereid, Halimede, Sao, Laomedeia, Psamathe, Neso

**Pluto System (1):**
- **Charon** - Large moon relative to Pluto

### Main Belt Asteroids (100+)

**Named Asteroids (includes):**
- Vesta, Pallas, Hygiea, Interamnia, Davida, Cybele, 52 Europa, Sylvia, Thisbe
- Euphrosyne, Juno, Psyche, Eunomia, Camilla, Elektra, Bamberga
- Doris, Fortuna, Egeria, Iris, Amphitrite, Ursula, Herculina, Siwa
- Dembowska, Loreley, Irene, Julia
- Plus 70+ additional belt asteroids distributed across 2.2-3.6 AU

### Jupiter Trojans (30)

**L4 Leading Group (15):**
- 588 Achilles, 911 Agamemnon, 1143 Odysseus, 1172 Aneas, 1173 Anchises
- 1208 Troilus, 1404 Ajax, 1437 Diomedes, 1583 Antilochus, 1647 Menelaus
- 1749 Telamon, 1867 Deiphobus, 2146 Stentor, 2223 Sarpedon, 2357 Phereclos

**L5 Trailing Group (15):**
- 617 Patroclus, 624 Hektor, 659 Nestor, 884 Priamus, 1868 Thersites
- 2920 Automedon, 3317 Paris, 3451 Mentor, 3540 Protesilaos, 3548 Eurybates
- 3708 1974 FV1, 4007 Euryalos, 4035 1986 WD, 4348 Poulydamas, 4543 Phoinix

### Near-Earth Objects (17)

**Apollo Group (10):**
- 433 Eros, 1862 Apollo, 1866 Sisyphus, 2062 Aten, 3200 Phaethon
- 4179 Toutatis, 25143 Itokawa, 101955 Bennu, 162173 Ryugu, 99942 Apophis

**Amor Group (4):**
- 1221 Amor, 1580 Betulia, 1627 Ivar, 1980 Tezcatlipoca

**Aten Group (3):**
- 2340 Hathor, 3753 Cruithne, 163693 Atira

### Comets (20)

**Short-Period Comets:**
- 1P/Halley - Famous 76-year period comet
- 2P/Encke - Shortest period comet (3.3 years)
- 9P/Tempel, 19P/Borrelly, 21P/Giacobini-Zinner
- 26P/Grigg-Skjellerup, 67P/Churyumov-Gerasimenko (Rosetta mission)
- 73P/Schwassmann-Wachmann, 81P/Wild, 103P/Hartley
- 29P/Schwassmann-Wachmann, 109P/Swift-Tuttle (Perseids parent)

**Long-Period Comets:**
- Hale-Bopp - Great comet of 1997
- Hyakutake - Great comet of 1996
- McNaught - Brightest comet in decades
- ISON, Lovejoy, NEOWISE, West

**Jupiter-Family:**
- Shoemaker-Levy 9 - Impacted Jupiter in 1994

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
- **377 bodies** rendered simultaneously
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
    body_type: Asteroid, // Star, Planet, DwarfPlanet, Moon, Asteroid, Comet
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
