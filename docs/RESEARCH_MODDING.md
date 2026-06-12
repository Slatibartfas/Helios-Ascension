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

## v0.5.0 Additions: Survey / Personnel / Geology Techs

> **v0.5.0 status (2026-06-12)** — pre-draft ahead of the v0.5.0 engineering chain (PR-A through PR-G). The 9 new tech names, tiers, prerequisites, and effects below are the design contract per `docs/design/SURVEY_REWORK.md` §[Tech Tree Integration]. The RON entries land in `assets/data/technologies.ron` with the v0.5.0 chain (PR-A's RON files were pre-staged in [PR #140](https://github.com/Slatibartfas/Helios-Ascension/pull/140) / GRA-98; the tech entries themselves land with PR-A and PR-B). If the Coder renames, drops, or reorders entries, this section mirrors the Coder's final tree at PR-H open.

The v0.5.0 survey rework adds **9 new techs** distributed across tiers 1–5, in the survey / personnel / geology area. None of these are Sensors — Sensors is the **reused existing family** that now acts as method gates for survey instruments (see sidebar below). All 9 entries follow the standard schema above (`id`, `name`, `category`, `prerequisites`, etc.).

### The 9 new techs

| # | Tier | Tech id | Display name | Prerequisites | What it unlocks |
|---|------|---------|--------------|---------------|-----------------|
| 1 | 1 | `survey_methodology` | Survey Methodology | `basic_sensors` | The analysis queue. +20% analysis throughput. |
| 2 | 2 | `planetary_geology` | Planetary Geology | `survey_methodology`, `basic_physics` | Geology scientists +25% throughput. |
| 3 | 2 | `geophysics` | Geophysics | `survey_methodology`, `basic_physics` | Seismic survey method. Seismic accuracy tier 2. |
| 4 | 2 | `field_science_operations` | Field Science Operations | `planetary_geology`, `closed_loop_ecology` | Surface lander with extended life support (≥ 1 sim-year surface ops). |
| 5 | 3 | `cryogenic_sampling` | Cryogenic Sampling | `cryogenics`, `field_science_operations` | Sample return from icy bodies (Europa, Titan, Enceladus, comet nuclei). |
| 6 | 3 | `deep_seismic_array` | Deep Seismic Array | `geophysics`, `deep_drilling` | Seismic accuracy tier 4. Deep mantle probes. |
| 7 | 3 | `roving_autonomy` | Roving Autonomy | `field_science_operations`, `basic_automation` | Rover survey. Rover range +50%. |
| 8 | 4 | `sample_return_architecture` | Sample Return Architecture | `cryogenic_sampling`, `orbital_mechanics` | Sample return from any body < 5 AU. |
| 9 | 5 | `interstellar_probe` | Interstellar Probe | `sample_return_architecture`, `fusion_propulsion` | Flyby of bodies in other star systems (v0.6+). |

Suggested RON entry — survey_methodology:

```ron
(
    id: "survey_methodology",
    name: "Survey Methodology",
    category: SpaceTechnology,
    description: "Formal methodology for processing and analysing survey data. Unlocks the analysis queue.",
    research_cost: 5000.0,
    prerequisites: ["basic_sensors"],
    unlocks_components: [],
    unlocks_engineering: [],
    modifiers: [
        (modifier_type: CategoryResearchBonus(SpaceTechnology), value: 5.0),
    ],
    tier: 1,
)
```

The exact `research_cost` and `modifiers` are balance decisions for the Coder / playtest. The 9 ids, tiers, and prerequisites above are the **design contract** — they should not change without an LGD re-sign.

### Existing techs reused as survey method gates (the "Sensors" family)

The v0.5.0 rework **does not add 9 new Sensors techs**. The 8 existing Sensors-family entries are reused as method gates — they already exist in the tree and now drive which survey instruments the player can build. Cross-reference for modders; do not re-author.

| Tech id | What it now also gates (as a method gate) |
|---------|--------------------------------------------|
| `basic_sensors` | Flyby probe (basic), remote sensing (basic) |
| `satellite_networks` | Orbital satellite, comms relay for data downlink |
| `remote_sensing` | Multispectral scan, IR telescope, hyperspectral imager |
| `radio_astronomy` | Radio dish array (deep-space data downlink) |
| `advanced_radar` | Phased-array radar (surface imaging through dust) |
| `deep_drilling` | Drill core sample (shallow) |
| `laser_drilling` | Drill core sample (deep) |
| `asteroid_prospecting` | Spectral analyzer, prospecting probe (small-body specialist) |
| `closed_loop_ecology` | Biological assay payload (habitability tier 4+) — *LifeSupport category, not Sensors* |

The instrument side that each gate unlocks is in `assets/data/survey/instruments.ron` (see `required_tech` field). The player manual for the new system is in `docs/SURVEY.md` §[Tech Tree].

### Adding a 10th survey tech (modder example)

A modder can add a 10th survey / personnel / geology tech — e.g. a "magnetometry" specialty for scientists — by:

1. Adding a new entry to `assets/data/technologies.ron` with one of the 15 standard categories, a unique `id` (snake_case), and a `tier` between 1 and 10.
2. Setting the `prerequisites` to whatever upstream techs make sense (typically one of the 9 listed above, or an existing sensor).
3. Adding 1–2 corresponding instruments to `assets/data/survey/instruments.ron` with `required_tech: Some("your_new_tech_id")` and the relevant `method` (e.g. `RemoteSensing`).
4. (Optionally) tier semantics to `assets/data/survey/tiers.ron` and an anomaly type to `assets/data/survey/anomalies.ron` that the new method can surface.

No Rust recompile. No new components. The RON modding surface is the player-influence path — see `docs/MODDING.md` and `docs/SURVEY.md` §[See Also] for the canonical modder references.

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
