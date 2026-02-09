# Research & Technology System - Modding Guide

This guide explains how to add new technologies to Helios Ascension.

## Overview

The research system is data-driven, using RON (Rusty Object Notation) files in `assets/data/technologies.ron`. This makes it easy to add, modify, or balance technologies without touching Rust code.

## Technology Structure

Each technology has the following fields:

```ron
(
    id: "unique_tech_id",              // Unique identifier (no spaces)
    name: "Display Name",              // Human-readable name
    category: TechCategory,            // One of the 15 categories
    description: "Description text",   // What this tech does
    research_cost: 5000.0,             // Research points required
    prerequisites: ["tech_id_1"],      // List of required techs (can be empty)
    unlocks_components: ["comp_1"],    // Components unlocked by this tech
    unlocks_engineering: ["proj_1"],   // Engineering projects unlocked
    modifiers: [                       // Bonuses granted by this tech
        (modifier_type: ResearchSpeed, value: 10.0),
    ],
    tier: 2,                           // Tech tier (1-10, for UI organization)
)
```

## Technology Categories

Choose one of these 15 categories for your technology:

- `Electronics` - Computing, AI, sensors
- `Military` - Weapons, tactics
- `SpaceTechnology` - Orbital mechanics, asteroid mining
- `Biology` - Life support, genetics, terraforming
- `Physics` - Fundamental research, particle physics
- `Energy` - Power generation, reactors
- `Sociology` - Administration, efficiency
- `Construction` - Building, manufacturing
- `Propulsion` - Engines, drives
- `Materials` - Alloys, composites, armor
- `Sensors` - Detection, scanning
- `Weapons` - Specific weapon systems
- `DefensiveSystems` - Shields, armor, countermeasures
- `LifeSupport` - Habitation, environmental systems
- `Industry` - Production, automation

## Modifier Types

Technologies can grant these bonuses:

```ron
// Percentage bonuses (10.0 = +10%)
ResearchSpeed         // Faster research globally
EngineeringSpeed      // Faster engineering globally
ConstructionCost      // Reduced construction costs (-15.0 = -15%)
MiningEfficiency      // Increased mining output
PowerGeneration       // Increased power output
ShipMaintenance       // Reduced ship upkeep costs
PopulationGrowth      // Faster population growth

// Category-specific bonuses
CategoryResearchBonus(Physics)  // +% research speed for Physics category

// Special unlocks
UnlockMechanic("feature_name")  // Enables new game mechanics
```

## Component Definitions

Components are designs that require engineering after research:

```ron
(
    id: "component_id",
    name: "Component Name",
    description: "What this component does",
    engineering_cost: 2500.0,        // Engineering points required
    required_tech: "tech_that_unlocks_this",
)
```

## Example: Adding a New Technology

Let's add "Quantum Communications" technology:

```ron
(
    id: "quantum_comms",
    name: "Quantum Communications",
    category: Electronics,
    description: "Instantaneous communication across solar system distances using quantum entanglement.",
    research_cost: 15000.0,
    prerequisites: ["neural_networks", "particle_physics"],
    unlocks_components: ["quantum_comm_array"],
    unlocks_engineering: [],
    modifiers: [],
    tier: 4,
)
```

And its component:

```ron
(
    id: "quantum_comm_array",
    name: "Quantum Communication Array",
    description: "Zero-latency communication system for fleet coordination",
    engineering_cost: 7500.0,
    required_tech: "quantum_comms",
)
```

## Tech Tree Design Guidelines

### Tier Organization
- **Tier 1**: 2026 baseline technology (0 research cost)
- **Tier 2-3**: Near-term advances (5,000-10,000 RP)
- **Tier 4-5**: Mid-game breakthroughs (15,000-25,000 RP)
- **Tier 6-8**: Advanced technology (30,000-50,000 RP)
- **Tier 9-10**: Late-game wonder tech (75,000+ RP)

### Prerequisites
- Keep dependency chains reasonable (2-3 levels deep max)
- Technologies can have multiple prerequisites
- Empty prerequisites `[]` means available from start

### Balancing Research Costs
Consider the tech tree position:
- **Foundation techs**: Lower cost, many dependents
- **Specialist techs**: Higher cost, fewer dependents
- **Wonder techs**: Very high cost, game-changing effects

### Modifier Balance
- Keep percentage bonuses modest (5-20% per tech)
- Stack multiplicatively for realism
- Category bonuses should be meaningful but not overpowered

## Testing Your Changes

1. Edit `assets/data/technologies.ron`
2. Run the game: `cargo run --release`
3. Open the Research menu (🔬 icon)
4. Check that your tech appears in the correct category
5. Verify prerequisites work correctly

## Common Issues

**Tech doesn't appear**:
- Check the RON syntax (commas, parentheses)
- Verify the category name matches exactly
- Ensure the file is valid RON format

**Prerequisites not working**:
- Check that prerequisite IDs match exactly (case-sensitive)
- Verify prerequisite techs exist in the file

**Component not appearing**:
- Ensure component ID in `unlocks_components` matches a component definition
- Check that `required_tech` in component points to correct tech ID

## Advanced: Technology Chains

Create progression paths by linking related technologies:

```ron
// Basic → Advanced → Expert progression
(id: "basic_materials", prerequisites: [], tier: 1),
(id: "materials_science", prerequisites: ["basic_materials"], tier: 2),
(id: "metamaterials", prerequisites: ["materials_science"], tier: 3),
```

## Mod Distribution

When creating a mod:
1. Copy the entire `technologies.ron` file
2. Make your changes
3. Document what you changed
4. Share with attribution

## Community Resources

- Example tech tree: See `assets/data/technologies.ron`
- RON syntax guide: https://github.com/ron-rs/ron
- Report issues: GitHub Issues
- Share mods: Community Discord

## Future Expansion

The system supports:
- Technology requirements based on resources
- Time-limited research bonuses
- Random tech tree generation
- Technology trading between factions
- Research espionage and theft

These features may be added in future updates!
