# Resource System

## Overview

Helios Ascension features a comprehensive resource system with 15 different resource types organized into 5 categories. All celestial bodies (planets, dwarf planets, moons, asteroids, and comets) can contain resource deposits.

## Resource Types

The game includes 15 resource types divided into 5 categories:

### Volatiles
- **Water (H2O)**: Essential for life support and terraforming
- **Hydrogen (H2)**: Fuel and industrial use
- **Ammonia (NH3)**: Terraforming and fertilizer
- **Methane (CH4)**: Fuel and chemical feedstock

### Construction Materials
- **Iron (Fe)**: Primary structural material
- **Aluminum (Al)**: Lightweight construction
- **Titanium (Ti)**: High-strength applications
- **Silicates (SiO2)**: Glass and ceramics

### Noble Gases
- **Helium-3 (He3)**: Fusion fuel
- **Argon (Ar)**: Industrial applications and atmospheres

### Fissiles
- **Uranium (U)**: Nuclear fission fuel
- **Thorium (Th)**: Alternative nuclear fuel

### Specialty Materials
- **Copper (Cu)**: Electronics and conductors
- **Noble Metals (Au/Pt)**: High-value applications
- **Rare Earths (REE)**: Advanced technology

## Resource Generation

Resources are procedurally generated for all celestial bodies based on:

1. **Distance from Parent Star**: Determines resource distribution via the frost line
2. **Frost Line**: Beyond ~2.5 AU (for Sun-like stars), volatiles become abundant
3. **Body Type**: Different body types have varying resource profiles

### Distribution Rules

- **Inner System (< frost line)**: High construction materials, low volatiles
- **Outer System (> frost line)**: High volatiles, lower construction materials
- **Noble Gases**: More abundant in outer system
- **Fissiles**: Rare everywhere, slightly more common in inner system
- **Specialty Materials**: Peak around optimal orbital distance

## Resource Deposits

Each resource deposit has two properties:

1. **Abundance**: The concentration of the resource in the body (0.0 to 1.0)
2. **Accessibility**: How easy it is to extract (0.0 to 1.0)

### Calculating Absolute Amounts

Resource amounts are displayed in **megatons (Mt)** based on:
- Body mass (in kg)
- Resource abundance (as fraction of body mass)

Formula: `Amount (Mt) = (Body Mass × Abundance) / 1e9`

Example:
- Earth mass: 5.972×10²⁴ kg
- Iron abundance: 50%
- Iron amount: 2.986×10¹⁵ Mt

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
