# Expanding the Solar System Simulation

## Currently Implemented (40+ Bodies)

The simulation currently includes:
- 1 Star (Sol)
- 8 Planets (Mercury through Neptune)
- 5 Dwarf Planets (Ceres, Pluto, Eris, Makemake, Haumea)
- 12+ Major Moons
- 10 Main Belt Asteroids

## Easy Additions

The data-driven architecture makes it trivial to add more celestial bodies. Here are suggestions:

### Additional Moons (~200+ possible)

#### Jupiter's Moons (79 known)
Currently have: Io, Europa, Ganymede, Callisto
Can easily add:
- Amalthea, Thebe, Adrastea, Metis (inner moons)
- Himalia, Elara, Pasiphae, Sinope, Lysithea, Carme, Ananke, Leda (irregular moons)

#### Saturn's Moons (83 known)
Currently have: Titan, Rhea
Can easily add:
- Mimas, Enceladus, Tethys, Dione, Iapetus (major moons)
- Hyperion, Phoebe (irregular)
- Many smaller moonlets

#### Uranus's Moons (27 known)
Currently have: Titania
Can easily add:
- Oberon, Umbriel, Ariel, Miranda (major moons)

#### Neptune's Moons (14 known)
Currently have: Triton
Can easily add:
- Proteus, Nereid, Larissa, Galatea

#### Mars Additional
- Many more small moons could be added

### Main Belt Asteroids (Hundreds possible)

Currently have 10 largest. Next tier includes:
- Euphrosyne, Juno, Psyche, Eunomia
- Camilla, Elektra, Bamberga, Doris
- Fortuna, Egeria, Iris, Amphitrite
- Ursula, Herculina, Siwa, etc.

**Recommendation:** Add top 50-100 asteroids

### Kuiper Belt Objects (50+ possible)

Currently have: Pluto, Eris, Makemake, Haumea
Can add:
- Quaoar, Sedna, Orcus, Salacia, Varda
- Ixion, Varuna, 2002 MS4, Chaos
- And many more trans-Neptunian objects

### Trojan Asteroids

Jupiter Trojans (thousands known):
- 588 Achilles, 617 Patroclus, 624 Hektor
- Leading and trailing Trojan groups
- **Recommendation:** Add ~20 largest Trojans

Mars Trojans:
- 5261 Eureka and others

### Near-Earth Objects (NEOs)

Popular near-Earth asteroids:
- 433 Eros
- 25143 Itokawa
- 101955 Bennu
- 162173 Ryugu
- 99942 Apophis

### Centaurs (between Jupiter and Neptune)

- 2060 Chiron
- 5145 Pholus
- 10199 Chariklo
- And others

### Additional Dwarf Planet Candidates

- Gonggong, Quaoar, Sedna, Orcus
- 2002 MS4, Salacia, Varda
- Many more candidates

## Implementation Guide

To add any of these bodies:

1. **Gather Data**: Use NASA JPL Horizons (https://ssd.jpl.nasa.gov/horizons/)
   - Get orbital elements (a, e, i, period)
   - Get physical properties (mass, radius)

2. **Edit assets/data/solar_system.ron**:
```ron
(
    name: "NewBody",
    body_type: Asteroid, // or Moon, DwarfPlanet, etc.
    mass: 1.0e18,        // kg from JPL
    radius: 50.0,        // km from JPL
    color: (0.6, 0.6, 0.55), // Gray for asteroids
    emissive: (0.0, 0.0, 0.0),
    parent: Some("Sol"), // or planet name for moons
    orbit: Some((
        semi_major_axis: 2.5,     // AU
        eccentricity: 0.08,
        inclination: 5.0,          // degrees
        orbital_period: 1200.0,    // days
        initial_angle: 45.0,       // degrees
    )),
    rotation_period: 0.5, // days
),
```

3. **Save and Run**: The game automatically loads the new data

## Performance Considerations

### Current Performance
- 40+ bodies: Excellent
- Estimated capacity: 500+ bodies on modern hardware

### Optimization Strategies

For 100+ bodies:
- **Level of Detail (LOD)**: Reduce mesh quality for distant objects
- **Culling**: Don't render objects outside camera view
- **Instancing**: Use GPU instancing for similar objects

For 500+ bodies:
- **Spatial Partitioning**: Octree/quadtree for efficient queries
- **Async Loading**: Load/unload distant objects
- **Simplified Physics**: Skip distant object updates

For 1000+ bodies:
- **Clustering**: Group distant asteroids
- **Impostor Rendering**: Use sprites for very distant objects
- **Compute Shaders**: GPU-accelerated orbit calculations

## Recommended Expansion Plan

### Phase 1: Complete Major Bodies (50 bodies)
- Add all major moons of Jupiter, Saturn, Uranus, Neptune
- Add top 20 main belt asteroids
- Add top 10 Kuiper belt objects
**Estimated work:** 1-2 hours of data gathering

### Phase 2: Minor Bodies (100 bodies)
- Add top 50 asteroids
- Add 20 Jupiter Trojans
- Add remaining significant moons
**Estimated work:** 2-4 hours

### Phase 3: Comprehensive Catalog (200+ bodies)
- Add top 100 asteroids
- Add significant Kuiper belt objects
- Add centaurs
- Add near-Earth objects
**Estimated work:** 4-8 hours

### Phase 4: Extreme Detail (500+ bodies)
- Add all named asteroids above certain size
- Add all Kuiper belt objects
- Add many smaller moons
**Estimated work:** 8-16 hours
**Note:** May require performance optimizations

## Data Sources

### Primary Sources
1. **NASA JPL Horizons**: https://ssd.jpl.nasa.gov/horizons/
   - Most accurate orbital data
   - Physical properties
   - Regular updates

2. **Minor Planet Center**: https://minorplanetcenter.net/
   - Asteroid catalog
   - Orbital elements

3. **NASA Planetary Data System**: https://pds.nasa.gov/
   - Comprehensive planetary data
   - Moon information

### Useful Websites
- Wikipedia - Good summaries and quick reference
- NASA Solar System Exploration - Educational resources
- IAU - Official naming authority

## Automation Potential

For very large expansions, consider:
- **Script Data Generation**: Parse JPL Horizons output directly
- **Batch Processing**: Convert CSV/JSON to RON format
- **Database Integration**: Store data in SQLite, generate RON on demand

Example Python script outline:
```python
import requests
# Fetch from JPL Horizons API
# Parse orbital elements
# Generate RON entries
# Append to solar_system.ron
```

## Visual Enhancements

For large numbers of objects:
- **Color Coding**: Different colors for different types
  - Rocky asteroids: Gray/brown
  - Icy bodies: White/blue
  - Metallic: Silver/gold
  
- **Size Representation**: Scale by actual size within limits

- **Labels**: Optional text labels for named bodies

- **Trails**: Orbital path visualization

## Conclusion

The current implementation can easily scale to hundreds of bodies with minimal code changes. The main work is data gathering, which can be automated for large expansions.

**Recommended Next Step:** Add Phase 1 (50 bodies) for a comprehensive major body catalog.
