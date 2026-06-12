# Modding Guide - Adding Custom Textures and Bodies

This guide explains how to add custom textures and celestial bodies to Helios Ascension, including texture packs, custom bodies, and even entire solar systems.

## Table of Contents
1. [Quick Start - Replace a Texture](#quick-start---replace-a-texture)
2. [Understanding the Texture System](#understanding-the-texture-system)
3. [Planet Texture Manifest (Exoplanet Modding)](#planet-texture-manifest-exoplanet-modding)
4. [Adding Custom Textures](#adding-custom-textures)
5. [Adding New Bodies](#adding-new-bodies)
6. [Creating a Texture Pack](#creating-a-texture-pack)
7. [Future: Multiple Solar Systems](#future-multiple-solar-systems)

## Quick Start - Replace a Texture

Want to add a custom Mars texture? Here's how:

1. Create your custom texture (JPEG, 2K-8K resolution, equirectangular projection)
2. Place it in: `assets/textures/celestial/planets/mars_custom_8k.jpg`
3. Edit `assets/data/solar_system.ron` and find the Mars entry:
```ron
(
    name: "Mars",
    body_type: Planet,
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_8k.jpg"),  // Change this line
)
```
4. Change the texture path to your new file:
```ron
    texture: Some("textures/celestial/planets/mars_custom_8k.jpg"),
```
5. Done! Your custom texture will now be used instead of the default.

## Understanding the Texture System

### Priority System

The game uses a **priority-based texture system**:

```
1. Dedicated Texture (if specified in RON file)
   ↓ (if not available)
2. Planet Texture Manifest (for exoplanet / procedural bodies)
   ↓ (pick from category list by body name hash)
3. Procedural Tint (colour-shift the texture by planet archetype)
```

**Key Code** (in `src/plugins/solar_system.rs`):
```rust
let texture_path = body_data.texture.clone()      // Try dedicated first
    .or_else(|| get_generic_texture_path(body_data));  // Fall back to generic
```

This means:
- ✅ **Dedicated textures ALWAYS override procedural ones**
- ✅ Adding a texture to the RON file immediately uses it
- ✅ Removing a texture path falls back to procedural
- ✅ Perfect for mods and customization!

### Texture Requirements

**Format**: JPEG (recommended for size) or PNG
**Resolution**: 1K to 8K (2048x1024 to 8192x4096)
**Projection**: Equirectangular (latitude-longitude mapping)
**Path**: Relative to `assets/` directory

**Examples**:
- Good: `textures/celestial/planets/custom_mars_4k.jpg`
- Good: `textures/custom/my_mod/earth_alternative.jpg`
- Bad: `/home/user/mars.jpg` (absolute paths won't work)
- Bad: `mars.jpg` (must be in assets directory)

## Planet Texture Manifest (Exoplanet Modding)

When the game visits a procedurally-generated star system, each planet is
classified into an **archetype** based on its temperature and body type, then a
texture is picked from `assets/data/planet_textures.ron`.

### Archetypes

| Category    | Temperature range              | Appearance                         |
|-------------|--------------------------------|------------------------------------|
| `lava`      | > 500 °C                       | Volcanic, molten surface           |
| `scorched`  | 200 – 500 °C                   | Greenhouse inferno, Venus-like     |
| `desert`    | 60 – 200 °C                    | Sandy yellow deserts, mesas, oases |
| `martian`   | 60 – 200 °C                    | Rust-red oxidised rocky terrain    |
| `rock`      | —                              | Bare stony rocky and dwarf worlds  |
| `savannah`  | > 45 °C (up to 500 °C)         | Hot grasslands, scorched plains    |
| `jungle`    | −20 – 60 °C                    | Dense green biosphere              |
| `ocean`     | −20 – 60 °C                    | Global blue ocean                  |
| `temperate` | −20 – 60 °C                    | Earth-like mixed biomes            |
| `alpine`    | −20 to 60 °C (< −5 °C)         | Cold habitable, frosty highlands   |
| `swamp`     | −20 to 45 °C                   | Murky, wet marshlands              |
| `tundra`    | −100 – −20 °C                  | Permafrost, grey-blue              |
| `ice`       | below −100 °C                  | Frozen, Pluto-like                 |
| `barren`    | any (default)                  | Dry neutral-toned rocky worlds     |
| `gas_giant` | ≥ −80 °C                       | Jupiter / Saturn-like              |
| `ice_giant` | < −80 °C                       | Neptune / Uranus-like              |
| `dwarf`     | —                              | KBOs, dwarf planets                |
| `moon`      | —                              | Natural satellites                 |

Jungle, ocean, and temperate worlds are all in the same temperature band; the
game distributes among them deterministically by body name. Desert and martian
textures likewise share the hot rocky band and are split deterministically.

> **Note:** The category is also shown in starmap tooltips and the selected-body
> panel, making it easier to identify generated worlds.

### Adding Textures to a Category

1. Place your texture in the matching subfolder:
   ```
   assets/textures/celestial/planets/<category>/my_texture.jpg
   ```

2. Register it in `assets/data/planet_textures.ron`:
   ```ron
   "jungle": [
       "textures/celestial/planets/jungle/my_jungle_planet.jpg",  // add here
       "textures/celestial/planets/earth_8k.jpg",
   ],
   ```

3. Restart the game — done!  The new texture is blended into the rotation for
   that category.  More entries = more variety.

### Example: Adding a Lava World Texture

1. Download or create a volcanic/lava planet texture (equirectangular, 2K–8K).
2. Save to `assets/textures/celestial/planets/lava/io_like_4k.jpg`.
3. Edit `assets/data/planet_textures.ron`:
   ```ron
   "lava": [
       "textures/celestial/planets/lava/io_like_4k.jpg",  // your new texture
       "textures/celestial/planets/mercury_8k.jpg",
       "textures/celestial/planets/venus_surface_8k.jpg",
   ],
   ```
4. Every lava world in procedural systems now has a 1-in-3 chance of getting
   your texture (picked by body name — fully deterministic).

### Using a Custom Category

You can add entirely new categories and reference them from code (requires a
small code change to `classify_exoplanet` in `src/plugins/starmap.rs`), or
simply add extra textures to an existing category to expand variety without any
code changes.

## Adding Custom Textures

### Method 1: Replace Existing Texture

**Easiest approach** - just swap the file:

1. **Keep the same filename**: Replace `mars_8k.jpg` with your texture
2. Restart the game - done!

**Pros**: No configuration needed
**Cons**: Harder to maintain multiple texture sets

### Method 2: New Texture with RON Edit

**Recommended approach** - add new file and update RON:

1. Add your texture: `assets/textures/celestial/planets/mars_realistic_8k.jpg`
2. Edit `assets/data/solar_system.ron`:
```ron
(
    name: "Mars",
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_realistic_8k.jpg"),
)
```
3. Restart the game

**Pros**: Can keep multiple textures, easy to switch
**Cons**: Need to edit RON file

### Method 3: Add Texture to Body Without One

Many small moons and asteroids use procedural textures. You can give them dedicated textures:

**Before** (using procedural):
```ron
(
    name: "Metis",  // Small Jupiter moon
    body_type: Moon,
    // ... other fields ...
    // No texture field = uses generic rocky texture
)
```

**After** (dedicated texture):
```ron
(
    name: "Metis",
    body_type: Moon,
    // ... other fields ...
    texture: Some("textures/celestial/moons/metis_custom_2k.jpg"),  // Add this!
)
```

Now Metis has a dedicated texture instead of the generic one!

## Adding New Bodies

Want to add a fictional moon or exoplanet? Here's how:

### Step 1: Create the Body Data

Edit `assets/data/solar_system.ron` and add a new body entry:

```ron
(
    name: "MyCustomMoon",
    body_type: Moon,
    mass: 1.0e20,              // Mass in kg
    radius: 500.0,             // Radius in km
    color: (0.8, 0.7, 0.6),   // RGB color (0-1)
    emissive: (0.0, 0.0, 0.0), // For stars only
    parent: Some("Jupiter"),   // Orbits Jupiter
    orbit: Some((
        semi_major_axis: 0.5,      // Distance in AU
        eccentricity: 0.01,        // Orbit shape (0=circle)
        inclination: 2.0,          // Tilt in degrees
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        orbital_period: 30.0,      // Days to orbit
        initial_angle: 0.0,        // Starting position
    )),
    rotation_period: 1.0,      // Rotation in days
    texture: Some("textures/celestial/moons/mycustommoon_2k.jpg"),
    asteroid_class: None,      // Only for asteroids
)
```

### Step 2: Add the Texture

Create and place your texture at the path specified.

### Step 3: Test

Restart the game and look for your custom moon orbiting Jupiter!

## Creating a Texture Pack

Want to create a complete texture replacement pack? Here's the structure:

### Directory Structure

```
assets/
├── textures/
│   └── celestial/
│       ├── planets/
│       │   ├── mars_mypack_8k.jpg
│       │   ├── earth_mypack_8k.jpg
│       │   └── jupiter_mypack_8k.jpg
│       ├── moons/
│       │   ├── moon_mypack_8k.jpg
│       │   └── titan_mypack_2k.jpg
│       └── stars/
│           └── sun_mypack_8k.jpg
└── data/
    └── solar_system.ron  # Modified with your texture paths
```

### Texture Pack RON Modifications

Create a script or guide to update texture paths:

**Original**:
```ron
texture: Some("textures/celestial/planets/mars_8k.jpg"),
```

**Your Pack**:
```ron
texture: Some("textures/celestial/planets/mars_mypack_8k.jpg"),
```

### Distribution

Package your texture pack as:
```
my_texture_pack/
├── README.md              # Installation instructions
├── textures/              # Your texture files
│   └── celestial/
│       └── planets/
│           └── mars_mypack_8k.jpg
└── solar_system_mod.ron   # Modified RON with your paths
```

**Installation instructions**:
1. Copy textures to `assets/textures/`
2. Copy modified bodies to `assets/data/solar_system.ron`
3. Restart game

## Future: Multiple Solar Systems

Planning for future multi-system support:

### Proposed Structure

```
assets/
└── data/
    ├── solar_system.ron      # Sol (our system)
    ├── alpha_centauri.ron    # Another system
    └── trappist1.ron         # Another system
```

### Loading Multiple Systems

**Future code** (not yet implemented):
```rust
// Load multiple star systems
let systems = vec![
    "assets/data/solar_system.ron",
    "assets/data/alpha_centauri.ron",
];

for system_file in systems {
    let data = SolarSystemData::load_from_file(system_file)?;
    // Spawn bodies...
}
```

### Texture Organization

```
assets/textures/
├── sol/              # Our solar system
│   ├── planets/
│   └── moons/
├── alpha_centauri/   # Alpha Centauri system
│   └── planets/
└── generic/          # Generic textures for any system
    ├── asteroids/
    └── comets/
```

## Tips and Best Practices

### Texture Creation

1. **Use equirectangular projection** - Required for proper sphere mapping
2. **Powers of 2** - Use 1024, 2048, 4096, 8192 pixel widths
3. **Aspect ratio 2:1** - Width should be 2x height (e.g., 4096x2048)
4. **JPEG compression** - Balance quality vs file size (80-90% quality)
5. **Test in-game** - Some textures look different when mapped to a sphere

### Performance

- **8K textures**: Use for major planets and Sun (high detail when close)
- **4K textures**: Good balance for most planets
- **2K textures**: Sufficient for moons and distant objects
- **1K textures**: Acceptable for small moons and asteroids

### Body Parameters

- **Mass**: Affects gravity (use realistic values)
- **Radius**: Affects visual size (use realistic km values)
- **Color**: Fallback if texture fails to load
- **Semi-major axis**: Distance from parent (in AU for planets, fraction for moons)
- **Orbital period**: Time to complete orbit (Earth days)

### Troubleshooting

**Texture not showing**:
- Check file path is relative to `assets/`
- Verify file exists at that location
- Check filename spelling and capitalization
- Check file format (JPEG or PNG)
- Look for errors in console/log

**Body not appearing**:
- Check RON syntax (commas, parentheses)
- Verify parent body exists
- Check orbital parameters are reasonable
- Verify body_type is correct

**Procedural texture instead of custom**:
- Verify texture path in RON file
- Check `texture: Some("path")` not `texture: None`
- Verify file exists at path

## Example Mods

### Simple Mars Retexture

**Files**:
- `assets/textures/celestial/planets/mars_hd_8k.jpg`

**RON Edit** in `solar_system.ron`:
```ron
// Find Mars entry and change:
texture: Some("textures/celestial/planets/mars_hd_8k.jpg"),
```

### Add Custom Asteroid

**Files**:
- `assets/textures/celestial/asteroids/psyche_2k.jpg`

**RON Addition** in `solar_system.ron`:
```ron
// Add new entry in bodies array:
(
    name: "Psyche",
    body_type: Asteroid,
    mass: 2.72e19,
    radius: 113.0,
    color: (0.5, 0.5, 0.5),
    emissive: (0.0, 0.0, 0.0),
    parent: Some("Sol"),
    orbit: Some((
        semi_major_axis: 2.92,
        eccentricity: 0.134,
        inclination: 3.1,
        longitude_ascending_node: 150.0,
        argument_of_periapsis: 228.0,
        orbital_period: 1826.0,
        initial_angle: 0.0,
    )),
    rotation_period: 0.175,
    texture: Some("textures/celestial/asteroids/psyche_2k.jpg"),
    asteroid_class: Some(MType),
)
```

### Complete Moon Texture Pack

Replace all Saturnian moon textures with custom set:

**Files** (7 textures):
- `assets/textures/celestial/moons/titan_mypack_2k.jpg`
- `assets/textures/celestial/moons/rhea_mypack_2k.jpg`
- `assets/textures/celestial/moons/iapetus_mypack_2k.jpg`
- ... etc for all Saturn moons

**RON Edits**: Update all Saturn moon entries with new paths.

## Community Resources

### Where to Find Textures

- **NASA**: https://science.nasa.gov/3d-resources/ (Public Domain)
- **Solar System Scope**: https://www.solarsystemscope.com/textures/ (CC BY 4.0)
- **Planet Pixel Emporium**: http://planetpixelemporium.com/ (Various licenses)
- **Community**: Check game forums for texture packs

### Sharing Your Mods

When sharing texture packs:
1. **Include README** with installation instructions
2. **Document licenses** for any textures used
3. **Provide credits** for texture sources
4. **List compatible game version**
5. **Show screenshots** of your textures in-game

## Advanced: Dynamic Texture Loading

**Future Feature** (not yet implemented):

The game could support a mod directory structure:

```
mods/
├── realistic_textures/
│   ├── mod.ron           # Mod metadata
│   ├── textures/
│   └── bodies.ron        # Body modifications
└── fantasy_bodies/
    ├── mod.ron
    ├── textures/
    └── bodies.ron
```

This would allow:
- Hot-swapping texture packs
- Enabling/disabling mods
- Mod priority ordering
- Automatic conflict resolution

## v0.5.0 Survey RON Files (Survey Rework)

> **v0.5.0 status** — schema is stable on `main` (PR #140 / GRA-98, PR #137 / GRA-80, PR #136 / GRA-81). This section pre-drafts the modder walkthrough for the six new RON files in `assets/data/survey/`. It is reconciled against the Coder-authored tech tree and 9-row mission roster as the chain lands.

The v0.5.0 survey rework added six RON files to `assets/data/survey/`. The first three are the **discovery primitives** (dimensions, instruments, anomalies), the next two are the **progression tables** (tiers, mining efficiency), and the last is the **player-actionable mission roster**. Together they let a modder rebalance the entire exploration loop without touching Rust.

### The discovery primitive trio

#### `dimensions.ron` — adding a 9th discovery dimension

The eight canonical dimensions are hardcoded in `SurveyDimension::ALL` (Rust enum) and are NOT in this file. The file is the **modder escape hatch** for adding a ninth.

```ron
(
    modder_dimensions: [
        // Example: a "Magnetosphere" dimension for magnetometer missions
        // (
        //     id: "magnetosphere",
        //     display_name: "Magnetosphere",
        //     description: "Strength and structure of the body's magnetic field.",
        // ),
    ],
)
```

To add a ninth dimension end-to-end:

1. Add one entry above with a stable RON `id`, `display_name`, and `description`.
2. Add tier semantics to `tiers.ron` (six rows: tier 0..5).
3. Add 1–2 instruments that advance the dimension to `instruments.ron` (set their `method` accordingly).
4. (Optionally) add an anomaly type that surfaces under the new dimension to `anomalies.ron`.

No Rust change. No recompile. The modder surface is the RON path.

#### `instruments.ron` — the physical-instrument catalog

Each row is a single physical instrument — a phased-array radar, a core sampler, a rover payload — that a mission can be dispatched with.

```ron
(
    instruments: [
        (
            id: "passive_sensor_array",
            display_name: "Passive Sensor Array",
            description: "Cheap flyby payload. Gives orbital mechanics + gross atmosphere at L1.",
            method: Flyby,                              // see SurveyMethod enum
            required_tech: Some("basic_sensors"),      // tech gate; None = always available
            base_duration_days: 540,                   // typical mission wall-clock
            scientist_requirement: 1,                  // scientists needed to process data
            accuracy_tier: 1,                          // 0..5; caps the dimension tier
            produces_anomalies: true,                  // can surface discoveries
        ),
        // …
    ],
)
```

**LGD rule (per `SURVEY_REWORK.md` §5): an instrument only does what its `method` permits.** A `phased_array_radar` (`RemoteSensing`) cannot advance `Subsurface` past accuracy tier 1 — that requires a `seismic_network`. `accuracy_tier` is the per-instrument cap; a follow-up by a higher-accuracy instrument is the only way to push a dimension further.

Method → tech-gate convention (existing techs from the v0.4 tree):

| Method             | Tech gate                | Notes                                            |
|--------------------|--------------------------|--------------------------------------------------|
| `Flyby`            | `basic_sensors`          | Cheapest entry; no in-situ contact.              |
| `Orbital`          | `satellite_networks`     | Sustained orbital observation.                   |
| `RemoteSensing`    | `remote_sensing`         | Radar variants use `advanced_radar`.              |
| `AtmosphericProbe` | `radio_astronomy`        | Drops a probe into the gas envelope.             |
| `SurfaceLander`    | `closed_loop_ecology`    | Required for biological assays.                  |
| `Rover`            | `roving_autonomy`        | v0.5.0 new tech (PR-B area).                     |
| `Seismic`          | `deep_seismic_array`     | v0.5.0 new tech (PR-B area).                     |
| `Drill`            | `deep_drilling`          | `laser_drilling` for hard rock.                  |
| `SampleReturn`     | `sample_return_architecture` + `asteroid_prospecting` | v0.5.0 new tech (PR-B area). |

Modder rebalance levers: edit `base_duration_days` and `accuracy_tier` to shift early/mid-game exploration cost without recompiling. Setting `produces_anomalies: false` on a survey-grade instrument narrows the discovery funnel and is a clean way to gate the anomaly system to dedicated follow-up payloads.

#### `anomalies.ron` — what the world can surprise the player with

Drives the r2 anomaly confidence model: per-tick detection roll, confidence accumulation, retry-pressure ramp, activation / refutation. See `docs/design/SURVEY_REWORK.md` §12.

```ron
(
    hardcoded: [ /* r1 terrestrial anomalies (9 from PR-A) */ ],
    modder_anomalies: [
        (
            id: "cryovolcanic_plume",
            display_name: "Cryovolcanic Plume",
            description: "Active water-ammonia eruption on a cold moon. Unlocks a research project on cryovolcanism.",
            detection_axes: [SurfaceFeatures, Anomalies], // dimensions whose tier must meet `detection_threshold`
            detection_threshold: 3,
            false_positive_rate: 0.18,                    // per-roll probability the system ignores the candidate (modder tune in [0.05, 0.30])
            activation_threshold: 0.75,                   // confidence required to promote to Verified; retry-pressure reduces it
            evidence_methods: [Flyby, Orbital, RemoteSensing],
            method_specificity: {Flyby: 0.60, Orbital: 0.85, RemoteSensing: 0.95},
            effect: (kind: "unlocks_tech", tech_id: "cryovolcanism"),  // None | unlocks_building | unlocks_tech | triggers_event
            coolness: 0.8,
        ),
    ],
)
```

Effect kinds:

- `None` — flavor-only; surfaces in the dossier's anomaly log but routes to no system.
- `unlocks_building` — the anomaly activation adds the named `building_id` to the construction catalog at the body.
- `unlocks_tech` — fires a research project (`tech_id`).
- `triggers_event` — emits a follow-up event (`event_id`), e.g. `methane_followup` for the r1 `methane_plume` anomaly.

`coolness` (0.0..1.0) tunes the dossier headline and the propaganda broadcast chance. Modders should keep this in `[0.3, 0.9]` — outside the band the UI either ignores it (`< 0.3`) or spams it (`> 0.9`).

### The progression tables

#### `tiers.ron` — what each tier of each dimension means

Six rows per dimension (tier 0..5). Tiers 0 is "no data", tier 5 is "exhaustive survey". The dossier SURVEY tab uses the row's `display_name` and `description` to label the dimension progress bar.

```ron
(
    dimension_tiers: [
        (
            dimension: OrbitalMech,
            tier: 0,
            display_name: "Unknown",
            description: "No orbital data. Only the body's mass and rough position are known.",
        ),
        // … tier 1..5 for OrbitalMech, then Atmosphere, then …
    ],
)
```

To add a 9th dimension (using the `dimensions.ron` path above), you must add a full 6-row tier block for it; otherwise the dossier falls back to a generic "Surveyed to tier N" string for that dimension.

#### `mining_efficiency.ron` — coupling survey to the economy

Each row binds a (dimension, tier) pair to a mining-yield multiplier for a specific resource. The mining system reads this on each extraction tick; it is the bridge between the survey rework and the v0.4 localized-resource economy.

```ron
(
    efficiency: [
        (
            resource: "Water",
            dimension: Subsurface,
            tier: 2,
            multiplier: 1.0,
        ),
        (
            resource: "Water",
            dimension: Subsurface,
            tier: 4,
            multiplier: 1.8,        // mid-game extraction doubles when you know the deposit
        ),
        // …
    ],
)
```

Modders tune the multiplier curve to gate resource availability against survey progression. Tier 0 entries are a no-op (you can't extract without data); tier 5 entries are the cap.

### The player-actionable mission roster

#### `missions.ron` — the nine dispatch buttons

This is the file the player sees most directly: the dossier SURVEY tab's "DISPATCH MISSION" button lists these and lets the player pick one. The dispatch system instantiates an `ActiveSurveyMission` from the chosen template.

```ron
(
    templates: [
        (
            id: "flyby_recon",
            display_name: "Flyby Recon",
            method: Flyby,
            instrument_id: "passive_sensor_array",            // must match an entry in instruments.ron
            target_tiers: { OrbitalMech: 1, Atmosphere: 1 },  // (dimension → target_tier) map
            base_duration_days: 540,                          // sim-days, before team modifiers
            axis_yield_per_day: 1.0,                          // coverage gain per day
            is_ground_team: false,                            // true = needs scientists on the surface (drives the CrewInjury failure roll)
        ),
        // … 8 more templates, covering Orbital / AtmosphericProbe / SurfaceLander / Rover /
        //     Seismic / Drill / SampleReturn / RemoteSensing
    ],
)
```

Per the SURVEY_REWORK design doc §5, durations and dimension targets are placeholders that LGD will tune over subsequent passes. The values in the shipped file are "reasonable defaults" — modders can rebalance without touching Rust.

**The full v0.5.0 template roster** (9 entries, in dispatch order):

| `id`                    | `display_name`         | `method`            | `target_tiers`                                                                  | `is_ground_team` |
|-------------------------|------------------------|---------------------|---------------------------------------------------------------------------------|------------------|
| `flyby_recon`           | Flyby Recon            | `Flyby`             | `OrbitalMech:1, Atmosphere:1`                                                    | false            |
| `remote_sensing_pass`   | Remote Sensing Pass    | `RemoteSensing`     | `MineralClasses:1, SurfaceFeatures:1`                                            | false            |
| `orbital_imaging`       | Orbital Imaging        | `Orbital`           | `SurfaceFeatures:2, MineralClasses:2, Atmosphere:2`                              | false            |
| `atmospheric_probe_drop`| Atmospheric Probe      | `AtmosphericProbe`  | `Atmosphere:3, Habitability:1`                                                   | false            |
| `surface_lander_v1`     | Surface Lander         | `SurfaceLander`     | `MineralDeposits:2, SurfaceFeatures:3`                                           | true             |
| `seismic_pass`          | Seismic Pass           | `Seismic`           | `Subsurface:3`                                                                  | true             |
| `drill_core_sample`     | Drill Core Sample      | `Drill`             | `MineralDeposits:4, Subsurface:4`                                                | true             |
| `rover_survey_v1`       | Rover Survey           | `Rover`             | `MineralDeposits:3, SurfaceFeatures:4, Habitability:2`                            | true             |
| `sample_return`         | Sample Return          | `SampleReturn`      | `MineralDeposits:5, MineralClasses:4`                                            | true             |

The progression is: cheap remote (Flyby / RemoteSensing) → orbital mapping → in-situ (Lander / Rover / Seismic) → deep access (Drill) → return-to-Earth (SampleReturn). Modders can reorder, but `target_tiers` should form a DAG — a tier 3 entry should require tier 2 results on the same dimension from a prior mission.

### Modder recipe: adding a tenth survey tech (full chain)

This is the canonical "add a new exploration lever" recipe. It exercises every file in this section end-to-end.

1. **`assets/data/technologies.ron`** — add a new tech row in the Survey / Geology family with the prerequisite chain from `SURVEY_REWORK.md` §[Tech Tree Integration] and `unlocks_instruments: ["your_new_instrument"]`. (See `docs/RESEARCH_MODDING.md` §[v0.5.0 Additions] for the existing 9 techs and the 8 reused as method gates.)
2. **`assets/data/survey/instruments.ron`** — add the new instrument with `method` matching the new tech, `required_tech: Some("your_new_tech_id")`, an `accuracy_tier` in `1..3`, and `produces_anomalies: true` if the player should be able to discover things with it.
3. **`assets/data/survey/missions.ron`** — add a mission template that uses the new `instrument_id`, with a `target_tiers` map pointing at one or two dimensions and a `base_duration_days` that fits the tier jump.
4. **`assets/data/survey/mining_efficiency.ron`** — add a tier curve for the dimensions the new instrument advances, so the survey-to-economy bridge is wired. (Skip this for purely anomaly-driven instruments.)

No Rust recompile. The RON loader picks up the new entries on next launch.

## Conclusion

The texture override system is already built into Helios Ascension! You can:

✅ Replace any texture by adding it to the RON file
✅ Add textures to bodies that use procedural ones
✅ Create new bodies with custom textures
✅ Build complete texture packs
✅ Prepare for future multi-solar-system support

**The dedicated texture ALWAYS takes priority** - just add it to the RON file and it works!

Happy modding! 🚀
