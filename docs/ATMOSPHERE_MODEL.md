# Atmosphere Model

## Overview

Helios Ascension now includes a comprehensive atmosphere model for celestial bodies, based on real NASA data for solar system bodies. This system is inspired by Aurora 4X and will play a crucial role in future terraforming and colonization mechanics.

## Features

### Atmospheric Composition
Each celestial body with an atmosphere stores:
- **Surface Pressure**: Measured in millibars (1 bar = 1000 millibars)
- **Surface Temperature**: Average temperature in Celsius
- **Gas Composition**: List of atmospheric gases with their percentages

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
- **Colony Cost**: 8 (maximum) - extreme greenhouse effect
- **Note**: Thick atmosphere creates a runaway greenhouse effect

#### Earth
- **Pressure**: 1,013 millibars (1 bar) - perfect baseline
- **Temperature**: 15°C - ideal
- **Composition**: 78% N₂, 21% O₂, 0.93% Ar, 0.04% CO₂
- **Colony Cost**: 0 - perfect for human habitation
- **Breathable**: Yes

#### Mars
- **Pressure**: 6 millibars (0.006 bar) - very thin
- **Temperature**: -63°C - cold
- **Composition**: 95% CO₂, 2.7% N₂, 1.6% Ar, 0.13% O₂
- **Colony Cost**: 7 - difficult but possible target for terraforming
- **Note**: Low pressure makes liquid water impossible on surface

### Gas Giants

#### Jupiter
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -145°C at cloud tops
- **Composition**: 90% H₂, 10% He
- **Note**: No solid surface; pressure increases dramatically with depth

#### Saturn
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -178°C at cloud tops
- **Composition**: 96% H₂, 3% He, 0.4% CH₄
- **Note**: Less dense than water; could float

### Ice Giants

#### Uranus
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -224°C - coldest planetary atmosphere
- **Composition**: 83% H₂, 15% He, 2% CH₄
- **Note**: Methane gives the planet its blue-green color

#### Neptune
- **Pressure**: 1,000 millibars (1 bar) at reference level
- **Temperature**: -218°C
- **Composition**: 80% H₂, 19% He, 1.5% CH₄
- **Note**: Most dynamic weather in the solar system

### Moons with Atmospheres

#### Titan (Saturn's Moon)
- **Pressure**: 1,500 millibars (1.5 bar) - denser than Earth!
- **Temperature**: -179°C
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

## Future Development

### Planned Features
1. **Terraforming System**: 
   - Installations to add/remove atmospheric gases
   - Temperature regulation through greenhouse gas management
   - Pressure adjustment for Mars-like bodies

2. **Colonization Mechanics**:
   - Infrastructure requirements based on colony cost
   - Dome construction for hostile environments
   - Life support systems

3. **Procedural Generation**:
   - For exoplanets and procedurally generated systems
   - Based on planet type, distance to star, and mass
   - Realistic distribution of gas types

4. **Atmospheric Effects**:
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
