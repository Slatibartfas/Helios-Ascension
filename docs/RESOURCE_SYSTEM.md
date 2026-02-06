# Resource System

## Overview

Helios Ascension features a comprehensive resource system with **20 different resource types** organized into **7 categories**. All celestial bodies (planets, dwarf planets, moons, asteroids, and comets) can contain resource deposits. Resource abundances are based on realistic planetary compositions to support future mining and depletion mechanics.

## Resource Types

The game includes 20 resource types divided into 7 categories:

### Volatiles
- **Water (H2O)**: Essential for life support and terraforming
- **Hydrogen (H2)**: Fuel and industrial use
- **Ammonia (NH3)**: Terraforming and fertilizer
- **Methane (CH4)**: Fuel and chemical feedstock

### Atmospheric Gases (Terraforming)
- **Nitrogen (N2)**: Primary component of breathable atmospheres
- **Oxygen (O2)**: Essential for life support and combustion
- **Carbon Dioxide (CO2)**: Greenhouse gas for terraforming warm atmospheres
- **Argon (Ar)**: Inert gas for industrial processes and atmospheres

### Construction Materials
- **Iron (Fe)**: Primary structural material (~15-35% of rocky bodies)
- **Aluminum (Al)**: Lightweight construction (~5-12% of crust)
- **Titanium (Ti)**: High-strength applications (~0.3-1% of crust)
- **Silicates (SiO2)**: Glass, ceramics, and major rock component (~25-45%)

### Fusion Fuel
- **Helium-3 (He3)**: Extremely rare but invaluable fusion fuel

### Fissiles
- **Uranium (U)**: Nuclear fission fuel (~3 ppm in crust)
- **Thorium (Th)**: Alternative nuclear fuel (~12 ppm in crust)

### Precious Metals
- **Gold (Au)**: High-value applications (~0.004 ppm in crust)
- **Silver (Ag)**: Electronics and currency (~0.08 ppm in crust)
- **Platinum (Pt)**: Catalysts and high-tech (~0.005 ppb in crust)

### Specialty Materials
- **Copper (Cu)**: Electronics and conductors (~60 ppm in crust)
- **Rare Earths (REE)**: Advanced technology and magnets (~200 ppm combined)

## Realistic Resource Abundances

Resource abundances are based on real-world planetary compositions and designed to support mining depletion mechanics:

### Construction Materials
Most abundant resources in rocky bodies:
- **Silicates**: 25-45% of body composition (major component of rocks)
- **Iron**: 15-35% of body composition (Earth's core is ~30% iron)
- **Aluminum**: 5-12% of body composition (~8% of Earth's crust)
- **Titanium**: 0.3-1% of body composition (~0.6% of Earth's crust)

### Volatiles
Common in outer solar system:
- **Water, Hydrogen, Ammonia, Methane**: 30-70% beyond frost line
- Nearly absent (<2%) in inner solar system

### Atmospheric Gases
Present in atmospheres and trapped in ice:
- **Nitrogen, Oxygen, CO2**: 0-40% depending on location and body type
- Can be mined from atmospheres or extracted from ice

### Rare Materials
Extremely scarce - will deplete quickly with mining:
- **Precious Metals**: Parts per million (ppm) to parts per billion (ppb)
- **Fissiles**: Parts per million (ppm)
- **Helium-3**: Parts per billion (ppb) - extremely valuable
- **Copper**: Parts per million (ppm)
- **Rare Earths**: Parts per million (ppm)

## Resource Generation

Resources are procedurally generated for all celestial bodies based on:

1. **Distance from Parent Star**: Determines resource distribution via the frost line
2. **Frost Line**: Beyond ~2.5 AU (for Sun-like stars), volatiles become abundant
3. **Body Type**: Different body types have varying resource profiles
4. **Realistic Composition**: Resources reflect actual planetary chemistry

### Distribution Rules

- **Inner System (< frost line)**: High construction materials, trace volatiles, some atmospheric gases
- **Outer System (> frost line)**: High volatiles, moderate atmospheric gases (in ice), lower construction materials
- **Atmospheric Gases**: Present everywhere but more abundant in outer system (trapped in ice)
- **Fusion Fuel (He3)**: Extremely rare everywhere, slightly more in outer system
- **Fissiles**: Very rare everywhere, slightly more common in inner system
- **Precious Metals**: Extremely rare, peak in asteroid belt regions
- **Specialty Materials**: Moderate rarity, peak around optimal orbital distances

## Resource Deposits

Each resource deposit has two properties:

1. **Abundance**: The concentration of the resource in the body (0.0 to 1.0 = 0% to 100% of body mass)
2. **Accessibility**: How easy it is to extract (0.0 to 1.0)

### Calculating Absolute Amounts

Resource amounts are displayed in **megatons (Mt)** based on:
- Body mass (in kg)
- Resource abundance (as fraction of body mass)

Formula: `Amount (Mt) = (Body Mass × Abundance) / 1e9`

### Mining and Depletion

The realistic abundance system supports future mining mechanics:

**Example: Earth-like Planet (5.972×10²⁴ kg)**
- Iron (30% abundance): 1.79×10¹⁵ Mt available
- Gold (0.0000005 abundance): ~3×10⁶ Mt available
- Platinum (0.00000005 abundance): ~3×10⁵ Mt available

**When mining:**
- Extracting resources decreases the deposit's abundance
- Body mass decreases proportionally
- Rare materials (precious metals, fissiles) will deplete much faster than common materials
- This creates strategic decisions about resource exploitation

**Example Mining Impact:**
```
Before mining: 
  - Body mass: 1.0×10²⁰ kg
  - Iron: 30% abundance = 3.0×10¹⁰ Mt
  
After mining 1.0×10⁹ Mt of iron:
  - Body mass: 9.9×10¹⁹ kg (0.1% decrease)
  - Iron: 29.7% abundance (slightly decreased)
  - Total iron remaining: ~2.94×10¹⁰ Mt
```

## UI Display

### Empire View (Header Panel)

The header panel shows resource totals grouped by category:
- **Volatiles**: Total Mt of water, hydrogen, ammonia, methane
- **Atmospheric Gases**: Total Mt of nitrogen, oxygen, CO2, argon
- **Construction**: Total Mt of construction materials
- **Fusion Fuel**: Total Mt of He-3
- **Fissiles**: Total Mt of fissile materials
- **Precious Metals**: Total Mt of gold, silver, platinum
- **Specialty**: Total Mt of specialty materials

**Hover** over any category to see detailed breakdown of individual resources.

### Selected Body View (Side Panel)

When a celestial body is selected, the resource panel displays:
- Body mass (important for understanding absolute amounts)
- Resources grouped by category
- For each resource:
  - Name and symbol
  - **Amount**: Absolute quantity in megatons (Mt)
  - **Concentration**: Percentage of body composition
  - **Accessibility**: Ease of extraction percentage

## Implementation Details

### Key Components

- `ResourceType` (enum): Defines all 20 resource types with categorization methods
- `MineralDeposit` (struct): Stores abundance and accessibility for a resource
- `PlanetResources` (component): HashMap of all resources on a celestial body
- `GlobalBudget` (resource): Tracks empire-wide resource stockpiles

### Key Methods

- `ResourceType::all()`: Returns all 20 resource types
- `ResourceType::by_category()`: Returns resources grouped by 7 categories
- `ResourceType::category()`: Gets category name for a resource
- `ResourceType::is_atmospheric_gas()`: Check if resource is for terraforming
- `ResourceType::is_precious_metal()`: Check if resource is precious metal
- `MineralDeposit::calculate_megatons(body_mass_kg)`: Calculates absolute amount
- `PlanetResources::viable_deposits()`: Filters economically useful deposits

### Generation System

The `generate_solar_system_resources` system runs at startup and:
1. Queries all planets, dwarf planets, moons, asteroids, and comets without resources
2. Determines distance from parent star and frost line
3. Generates appropriate resources based on location with realistic abundances
4. Adds `PlanetResources` component to each body

## Future Expansion

The resource system is designed to support:
- **Mining operations**: Extract resources from bodies
- **Resource depletion**: Mining reduces abundance and body mass
- **Refining and processing chains**: Convert raw materials to useful products
- **Terraforming**: Use atmospheric gases (N2, O2, CO2) to create breathable atmospheres
- **Construction**: Build stations and ships using materials
- **Energy production**: Fission (U, Th) and fusion (He-3) power
- **Trade and economic simulation**: Rare resources drive interplanetary commerce
- **Strategic resource management**: Balancing extraction vs. preservation

## UI Display

### Empire View (Header Panel)

The header panel shows resource totals grouped by category:
- **Volatiles**: Total Mt of all volatiles
- **Construction**: Total Mt of construction materials
- **Noble Gases**: Total Mt of noble gases
- **Fissiles**: Total Mt of fissile materials
- **Specialty**: Total Mt of specialty materials

**Hover** over any category to see detailed breakdown of individual resources.

### Selected Body View (Side Panel)

When a celestial body is selected, the resource panel displays:
- Body mass
- Resources grouped by category
- For each resource:
  - Name and symbol
  - **Amount**: Absolute quantity in megatons (Mt)
  - **Concentration**: Percentage of body composition
  - **Accessibility**: Ease of extraction percentage

## Implementation Details

### Key Components

- `ResourceType` (enum): Defines all 15 resource types with categorization methods
- `MineralDeposit` (struct): Stores abundance and accessibility for a resource
- `PlanetResources` (component): HashMap of all resources on a celestial body
- `GlobalBudget` (resource): Tracks empire-wide resource stockpiles

### Key Methods

- `ResourceType::all()`: Returns all 15 resource types
- `ResourceType::by_category()`: Returns resources grouped by category
- `ResourceType::category()`: Gets category name for a resource
- `MineralDeposit::calculate_megatons(body_mass_kg)`: Calculates absolute amount
- `PlanetResources::viable_deposits()`: Filters economically useful deposits

### Generation System

The `generate_solar_system_resources` system runs at startup and:
1. Queries all planets, dwarf planets, moons, asteroids, and comets without resources
2. Determines distance from parent star and frost line
3. Generates appropriate resources based on location
4. Adds `PlanetResources` component to each body

## Future Expansion

The resource system is designed to support:
- Mining operations
- Resource extraction rates based on accessibility
- Refining and processing chains
- Terraforming using volatiles and noble gases
- Construction using materials
- Energy production using fissiles and He-3
- Trade and economic simulation
