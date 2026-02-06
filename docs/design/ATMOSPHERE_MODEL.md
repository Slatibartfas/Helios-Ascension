# Atmosphere Model

## Overview

Helios Ascension now includes a comprehensive atmosphere model for celestial bodies, based on real NASA data for solar system bodies. This system is inspired by Aurora 4X and will play a crucial role in future terraforming and colonization mechanics.

## Features

### Atmospheric Composition
Each celestial body with an atmosphere stores:
- **Pressure**: Measured in millibars (1 bar = 1000 millibars)
  - **Terrestrial planets**: This is the actual surface pressure
  - **Gas giants**: This is the pressure at the 1 bar reference level (scientific convention)
  - The system explicitly tracks whether pressure is at a reference altitude or at the surface
- **Temperature**: Average temperature in Celsius
  - For gas giants, temperature at the 1 bar reference level
- **Gas Composition**: List of atmospheric gases with their percentages
- **Atmosphere Retention**: Calculated flag based on escape velocity physics
- **Pressure Type Flag**: Indicates whether this is a reference altitude pressure (gas giants) or surface pressure (terrestrial)

### Reference Pressure vs Surface Pressure
Since gas giants lack solid surfaces, atmospheric measurements are taken at a reference altitude where pressure equals 1 bar (Earth sea level). This distinction is important for:
- **Player understanding**: The UI displays "Pressure (at 1 bar ref)" for gas giants vs "Surface Pressure" for terrestrial planets
- **Scientific accuracy**: Reflects how real planetary scientists measure gas giant atmospheres
- **Future gameplay**: Matters for atmospheric mining, colonization attempts, and terraforming mechanics

### Atmospheric Retention Physics
The system calculates whether a body can physically support an atmosphere based on its escape velocity:
- **Escape Velocity Formula**: v_e = √(2GM/r)
  - G = gravitational constant (6.674×10⁻¹¹ N⋅m²/kg²)
  - M = body mass (kg)
  - r = body radius (m)
- **Retention Threshold**: Bodies with escape velocity ≥ 2.0 km/s can retain heavy gases
  - Earth: 11.2 km/s - excellent retention
  - Mars: 5.0 km/s - good retention
  - Titan: 2.6 km/s - can retain atmosphere
  - Moon: 2.4 km/s - marginal retention
  - Small asteroids: < 1 km/s - cannot retain atmosphere

### Breathability Detection
The system automatically determines if an atmosphere is breathable for humans based on oxygen partial pressure:
- Requires 0.1-0.3 atmospheres (100-300 millibars) of O₂
- Earth's atmosphere meets this requirement (21% O₂ at 1013 mbar ≈ 213 mbar O₂)

### Colony Cost Calculation
Following Aurora 4X's model, each atmosphere has a calculated colony cost (0-8):
- **0**: Earth-like conditions (perfect for colonization)
- **1-3**: Challenging but manageable
- **4-6**: Difficult conditions requiring significant infrastructure
- **7-8**: Extreme environments requiring extensive life support

Factors affecting colony cost:
- Temperature deviation from 15°C
- Atmospheric pressure (too low or too high)
- Lack of breathable oxygen

## Solar System Bodies with Atmospheres

### Terrestrial Planets

#### Venus
- **Pressure**: 92,000 millibars (92 bar) - crushing pressure
- **Temperature**: 465°C - hottest planet
- **Composition**: 96.5% CO₂, 3.5% N₂
- **Escape Velocity**: 10.4 km/s
- **Can Support Atmosphere**: Yes
- **Colony Cost**: 8 (maximum) - extreme greenhouse effect
- **Note**: Thick atmosphere creates a runaway greenhouse effect

#### Earth
- **Pressure**: 1,013 millibars (1 bar) - perfect baseline
- **Temperature**: 15°C - ideal
- **Composition**: 78% N₂, 21% O₂, 0.93% Ar, 0.04% CO₂
- **Escape Velocity**: 11.2 km/s
- **Can Support Atmosphere**: Yes
- **Colony Cost**: 0 - perfect for human habitation
- **Breathable**: Yes

#### Mars
- **Pressure**: 6 millibars (0.006 bar) - very thin
- **Temperature**: -63°C - cold
- **Composition**: 95% CO₂, 2.7% N₂, 1.6% Ar, 0.13% O₂
- **Escape Velocity**: 5.0 km/s
- **Can Support Atmosphere**: Yes
- **Colony Cost**: 7 - difficult but possible target for terraforming
- **Note**: Low pressure makes liquid water impossible on surface

### Gas Giants

#### Jupiter
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -108°C at 1 bar level (NASA data)
- **Composition**: 90% H₂, 10% He
- **Escape Velocity**: 60 km/s
- **Can Support Atmosphere**: Yes (massive retention)
- **Note**: No solid surface; pressure increases dramatically with depth

#### Saturn
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -133°C at 1 bar level (NASA data)
- **Composition**: 96% H₂, 3% He, 0.4% CH₄
- **Escape Velocity**: 36 km/s
- **Can Support Atmosphere**: Yes (massive retention)
- **Note**: Less dense than water; could float

### Ice Giants

#### Uranus
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -197°C at 1 bar level (NASA data)
- **Composition**: 83% H₂, 15% He, 2% CH₄
- **Escape Velocity**: 21 km/s
- **Can Support Atmosphere**: Yes (massive retention)
- **Note**: Methane gives the planet its blue-green color

#### Neptune
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -201°C at 1 bar level (NASA data)
- **Composition**: 80% H₂, 19% He, 1.5% CH₄
- **Escape Velocity**: 23 km/s
- **Can Support Atmosphere**: Yes (massive retention)
- **Note**: Most dynamic weather in the solar system

### Moons with Atmospheres

#### Titan (Saturn's Moon)
- **Pressure**: 1,500 millibars (1.5 bar) - denser than Earth!
- **Temperature**: -179°C
- **Escape Velocity**: 2.6 km/s
- **Can Support Atmosphere**: Yes (sufficient retention for nitrogen/methane)
- **Composition**: 98.4% N₂, 1.4% CH₄
- **Note**: Only moon with a substantial atmosphere; methane lakes on surface

## Implementation Details

### Data Structure
```rust
pub struct AtmosphereComposition {
    pub surface_pressure_mbar: f32,
    pub surface_temperature_celsius: f32,
    pub gases: Vec<AtmosphericGas>,
    pub breathable: bool,
}
```

### Data Source
Atmosphere data is stored in `assets/data/solar_system.ron` and loaded at game startup:

```ron
atmosphere: Some((
    surface_pressure_mbar: 1013.0,
    surface_temperature_celsius: 15.0,
    gases: [
        (name: "N2", percentage: 78.0),
        (name: "O2", percentage: 21.0),
        (name: "Ar", percentage: 0.93),
        (name: "CO2", percentage: 0.04),
    ],
)),
```

### Component System
The `AtmosphereComposition` component is automatically attached to celestial body entities during system initialization.

## Gas Giant Atmospheric Harvesting

### Harvest Altitude Mechanics

Gas giants support atmospheric harvesting (gas scooping) at various depths. Unlike the 1 bar reference level used for scientific measurement, harvesting operations occur at higher pressures for better efficiency.

#### Harvest Altitude Pressure
- **Default Harvest Altitude**: 10 bar for gas giants
- **Maximum Harvest Depth**: 50 bar with basic technology (upgradeable via research)
- **Yield Relationship**: Harvest yield scales linearly with pressure/density
  - At 10 bar: ~10× yield compared to 1 bar reference
  - At 50 bar: ~50× yield compared to 1 bar reference
  - At 100 bar: ~100× yield (requires advanced technology)

#### Physical Basis
Using the ideal gas law approximation, atmospheric density (and thus harvestable gas per volume) is proportional to pressure at roughly constant temperature. Deeper atmospheric harvesting provides:
- **Higher gas density**: More molecules per cubic meter
- **Better efficiency**: More gas collected per scoop operation
- **Greater yield**: Linear scaling with pressure level

#### Technology Progression
- **Basic Tech**: Harvest up to 10 bar (default starting point)
- **Standard Tech**: Harvest up to 50 bar (10× baseline capacity)
- **Advanced Tech**: Harvest up to 100+ bar (requires structural reinforcement research)
- **Future**: Extreme-depth harvesting at 200+ bar (experimental technology)

#### Gameplay Implications
1. **Early Game**: Start with shallow harvesting (10 bar) at lower yield
2. **Mid Game**: Research deeper harvesting technology to increase yield
3. **Late Game**: Achieve maximum efficiency with extreme-depth capability
4. **Trade-offs**: Deeper harvesting requires more advanced (expensive) infrastructure

#### Example: Jupiter
```
Reference Level: 1.00 bar (scientific baseline)
Harvest Altitude: 10.0 bar (10× yield)
Max Harvest Depth: 50.0 bar (tech-limited, upgradeable to 100+ bar)
Composition: 90% H₂, 10% He
```

At 10 bar harvest altitude on Jupiter, a gas scoop station would collect:
- Hydrogen: 90% × 10× yield multiplier = excellent H₂ source
- Helium: 10% × 10× yield multiplier = good He source

## Future Development

### Planned Features
1. **Atmospheric Harvesting System**:
   - Gas scoop stations at adjustable altitudes
   - Technology research to increase max harvest depth
   - Yield calculations based on harvest altitude and gas composition
   - Infrastructure costs scale with harvest depth

2. **Terraforming System**: 
   - Installations to add/remove atmospheric gases
   - Temperature regulation through greenhouse gas management
   - Pressure adjustment for Mars-like bodies

3. **Colonization Mechanics**:
   - Infrastructure requirements based on colony cost
   - Dome construction for hostile environments
   - Life support systems

4. **Procedural Generation**:
   - For exoplanets and procedurally generated systems
   - Based on planet type, distance to star, and mass
   - Realistic distribution of gas types

5. **Atmospheric Effects**:
   - Visual effects for thick atmospheres
   - Weather systems
   - Atmospheric entry calculations for spacecraft

## Data Sources

All atmosphere data is based on:
- NASA Planetary Fact Sheets
- NASA Planetary Data System (PDS)
- Mission data from Mariner, Viking, Voyager, Cassini, Galileo, and Juno
- Britannica Science Encyclopedia
- University planetary science research

## References

- [NASA Planetary Fact Sheets](https://nssdc.gsfc.nasa.gov/planetary/factsheet/)
- [Planetary Data System](https://pds.nasa.gov/)
- [Aurora 4X Wiki - Terraforming](https://aurorawiki2.pentarch.org/index.php?title=Terraforming)
- [Leiden University - Planetary Atmospheres](https://home.strw.leidenuniv.nl/~keller/Teaching/Planets_2010/atmospheres_2010.PDF)
