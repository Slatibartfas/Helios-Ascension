# Example Mod: Adding a New Moon

This example shows how to add a completely new celestial body with a custom texture.

## What This Mod Does

Adds "Helios", a fictional moon orbiting Jupiter, with a custom texture.

## Scenario

Perfect for:
- Testing new bodies
- Adding fictional moons/planets
- Preparing for future solar systems (e.g., exoplanets)
- Creating custom scenarios

## Files Needed

```
new_moon_mod/
├── README.md                          # This file
├── textures/
│   └── celestial/
│       └── moons/
│           └── helios_2k.jpg         # Your custom moon texture
└── patches/
    └── helios_moon.ron                # Complete body definition
```

## Installation

### Step 1: Add Your Texture

Copy your moon texture to:
```
assets/textures/celestial/moons/helios_2k.jpg
```

### Step 2: Add Body to RON File

Open `assets/data/solar_system.ron` and add this new entry **in the `bodies` array**:

**Find a good insertion point** (e.g., after the Galilean moons, around line 280):

```ron
        // After Callisto, add:

        // Custom moon - Helios
        (
            name: "Helios",
            body_type: Moon,
            mass: 8.0e21,                    // Similar to Europa
            radius: 1560.0,                  // Radius in km
            color: (0.9, 0.85, 0.7),        // Yellowish-tan color
            emissive: (0.0, 0.0, 0.0),      // Not glowing
            parent: Some("Jupiter"),         // Orbits Jupiter
            orbit: Some((
                semi_major_axis: 0.0045,     // Distance from Jupiter (in AU)
                eccentricity: 0.02,          // Nearly circular
                inclination: 0.5,            // Slight tilt
                longitude_ascending_node: 45.0,
                argument_of_periapsis: 90.0,
                orbital_period: 4.5,         // Orbits every 4.5 days
                initial_angle: 270.0,        // Starting position
            )),
            rotation_period: 4.5,            // Tidally locked (same as orbital period)
            texture: Some("textures/celestial/moons/helios_2k.jpg"),  // Your texture!
        ),
```

**Important**: Make sure the comma after the previous body is present, and add a comma after your new body!

### Step 3: Restart the Game

Launch the game and navigate to Jupiter. You should see your new moon "Helios" orbiting!

## Body Parameters Explained

### Basic Properties

- **name**: String identifier (used for parent references)
- **body_type**: `Moon`, `Planet`, `Asteroid`, `Comet`, `Star`, `DwarfPlanet`
- **mass**: Mass in kilograms (affects gravity, use scientific notation)
- **radius**: Radius in kilometers (affects visual size)
- **color**: RGB values 0.0-1.0 (fallback if texture fails)
- **emissive**: RGB glow (only for stars, usually (0,0,0) for others)

### Orbital Parameters

- **parent**: Name of body to orbit (e.g., "Jupiter", "Sol")
- **semi_major_axis**: Average distance from parent in AU
  - For moons: Use small fractions (0.001-0.01 typically)
  - For planets: Use astronomical units (1 AU = Earth-Sun distance)
- **eccentricity**: Orbit shape (0 = circle, 0.01-0.1 = ellipse)
- **inclination**: Tilt of orbit in degrees
- **orbital_period**: Time to complete one orbit (Earth days)
- **initial_angle**: Starting position in orbit (0-360 degrees)

### Rotation

- **rotation_period**: Time for one rotation (Earth days)
  - Positive = normal rotation
  - Negative = retrograde (backwards)
  - Same as orbital_period = tidally locked

### Texture

- **texture**: `Some("path/to/texture.jpg")` or `None`
  - If `None`, uses generic texture + procedural variation
  - If `Some(path)`, always uses your custom texture

## Creating a Realistic Moon

### Use Real Physics

Calculate orbital parameters:

```python
# Semi-major axis (distance from parent)
# For Jupiter moons, typical range: 0.001-0.05 AU
distance_km = 420000  # Distance from Jupiter
distance_au = distance_km / 149597870.7  # Convert to AU

# Orbital period (Kepler's Third Law approximation)
# P² ∝ a³ (for objects orbiting the same parent)
# Use Europa as reference: 3.551 days at 0.00448 AU
period_days = (distance_au / 0.00448) ** 1.5 * 3.551
```

### Make It Look Good

1. **Size matters**: Large moons (>1000km) should have detailed 2K textures
2. **Color scheme**: Match your texture color to the `color` field
3. **Rotation**: Moons usually rotate once per orbit (tidally locked)
4. **Position**: Spread new moons around different angles

## Example: Adding Multiple Bodies

Want to add several moons? Just repeat the pattern:

```ron
bodies: [
    // ... existing bodies ...

    // New fictional Jovian system
    (
        name: "Helios",
        body_type: Moon,
        // ... parameters ...
        texture: Some("textures/celestial/moons/helios_2k.jpg"),
    ),
    (
        name: "Nyx",
        body_type: Moon,
        // ... parameters ...
        texture: Some("textures/celestial/moons/nyx_2k.jpg"),
    ),
    (
        name: "Aether",
        body_type: Moon,
        // ... parameters ...
        texture: Some("textures/celestial/moons/aether_2k.jpg"),
    ),

    // ... rest of existing bodies ...
]
```

## Future: Complete Solar System

This same technique lets you add entire solar systems:

```ron
// Alpha Centauri star
(
    name: "Alpha Centauri A",
    body_type: Star,
    mass: 2.187e30,
    radius: 853070.0,
    color: (1.0, 0.95, 0.8),
    emissive: (10.0, 9.0, 7.0),
    parent: None,  // No parent = central star
    orbit: None,
    rotation_period: 22.0,
    texture: Some("textures/celestial/stars/alpha_centauri_a.jpg"),
),

// Planet orbiting Alpha Centauri A
(
    name: "Proxima b",
    body_type: Planet,
    mass: 1.27e24,
    radius: 7160.0,
    color: (0.6, 0.5, 0.4),
    emissive: (0.0, 0.0, 0.0),
    parent: Some("Alpha Centauri A"),
    orbit: Some((
        semi_major_axis: 0.05,
        eccentricity: 0.1,
        inclination: 2.0,
        // ... other orbit params ...
        orbital_period: 11.2,
        initial_angle: 0.0,
    )),
    rotation_period: 11.2,  // Tidally locked
    texture: Some("textures/celestial/planets/proxima_b.jpg"),
),
```

## Troubleshooting

### Body Not Appearing

**Check**:
1. RON syntax is correct (commas, parentheses)
2. Parent body name matches exactly
3. Semi-major axis isn't too large (body far away)
4. Body isn't inside parent (radius issues)

### Texture Not Loading

**Check**:
1. File exists at specified path
2. Path is relative to `assets/`
3. Filename matches exactly (case-sensitive)
4. File format is JPEG or PNG

### Orbit Looks Wrong

**Check**:
1. Semi-major axis is appropriate for parent
2. Orbital period matches the distance
3. Initial angle places it where you expect

### Game Performance

**Too many bodies**:
- Keep total bodies under 500 for good performance
- Use smaller textures for distant/small bodies
- Consider spreading bodies across multiple systems

## Advanced: Body Classification

For asteroids, you can specify their type:

```ron
(
    name: "CustomAsteroid",
    body_type: Asteroid,
    // ... other fields ...
    texture: Some("textures/celestial/asteroids/my_asteroid.jpg"),
    asteroid_class: Some(MType),  // Metallic asteroid
)
```

Types:
- `CType`: Carbonaceous (dark)
- `SType`: Silicaceous (rocky)
- `MType`: Metallic
- `Unknown`: Default

## Testing Your Mod

1. Start game
2. Look for your body in the system
3. Fly close to check texture quality
4. Verify orbital motion is smooth
5. Check no errors in console/log

## Sharing Your Mod

When distributing:

1. **Package structure**:
```
my_custom_bodies_mod/
├── README.md
├── textures/
│   └── celestial/
│       └── moons/
│           └── helios_2k.jpg
└── bodies_to_add.ron        # Just the new body entries
```

2. **Installation instructions**: Clear steps to add bodies to RON file

3. **Credits**: List texture sources and licenses

## Version

- **Mod Version**: 1.0
- **Compatible with**: Helios Ascension v0.1.0+

---

**Create your own celestial bodies!** 🌙✨
