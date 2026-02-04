# Example Mod: Custom Mars Texture

This example shows how to add a custom texture to replace Mars's default texture.

## What This Mod Does

Replaces the default Mars texture with a custom high-resolution texture.

## Files Included

```
custom_mars_mod/
├── README.md                                    # This file
├── textures/
│   └── celestial/
│       └── planets/
│           └── mars_custom_hd_8k.jpg          # Your custom Mars texture
└── patches/
    └── solar_system_mars_patch.ron             # RON modification
```

## Installation

### Step 1: Copy the Texture

Copy the texture file to your game directory:

```bash
# From the mod directory, copy to game:
cp textures/celestial/planets/mars_custom_hd_8k.jpg \
   /path/to/game/assets/textures/celestial/planets/
```

### Step 2: Update the RON File

Open `assets/data/solar_system.ron` and find the Mars entry (around line 124):

**Find this**:
```ron
(
    name: "Mars",
    body_type: Planet,
    mass: 6.4171e23,
    radius: 3389.5,
    color: (0.8, 0.4, 0.2),
    emissive: (0.0, 0.0, 0.0),
    parent: Some("Sol"),
    orbit: Some((
        semi_major_axis: 1.524,
        eccentricity: 0.0934,
        inclination: 1.85,
        longitude_ascending_node: 49.57,
        argument_of_periapsis: 286.5,
        orbital_period: 686.98,
        initial_angle: 180.0,
    )),
    rotation_period: 1.026,
    texture: Some("textures/celestial/planets/mars_8k.jpg"),  // <-- This line
),
```

**Change to**:
```ron
    texture: Some("textures/celestial/planets/mars_custom_hd_8k.jpg"),  // <-- Changed!
```

### Step 3: Restart the Game

Launch the game and fly to Mars to see your custom texture!

## Uninstallation

### Revert RON Changes

Change the texture line back to:
```ron
    texture: Some("textures/celestial/planets/mars_8k.jpg"),
```

### Optional: Remove Texture File

```bash
rm /path/to/game/assets/textures/celestial/planets/mars_custom_hd_8k.jpg
```

## Creating Your Own Texture

### Requirements

- **Format**: JPEG or PNG
- **Resolution**: 8192x4096 pixels (8K) or 4096x2048 (4K)
- **Projection**: Equirectangular (latitude-longitude)
- **Quality**: 80-90% JPEG compression

### Tools

- **Photoshop/GIMP**: For editing
- **G.Projector** (NASA): For reprojecting maps
- **TextureTools**: For optimizing

### Sources

Use real Mars data:
- NASA Mars Reconnaissance Orbiter
- Viking mission mosaics
- Mars Global Surveyor data

Or create artistic versions!

## Technical Details

### How It Works

The game's texture system prioritizes dedicated textures:

```rust
// In src/plugins/solar_system.rs
let texture_path = body_data.texture.clone()      // Try dedicated first
    .or_else(|| get_generic_texture_path(body_data));  // Fallback to generic
```

When you specify a texture in the RON file, it **always** takes priority over any procedural or generic texture.

### Performance

8K textures use approximately:
- **Memory**: ~12-15 MB per texture
- **VRAM**: ~12-15 MB
- **Load time**: ~1-2 seconds

This is acceptable for a major planet like Mars that players will view up close.

## Troubleshooting

### Texture Not Showing

**Problem**: Mars still shows the old texture  
**Solution**: Check that:
1. File path in RON is correct
2. File exists at that location
3. Filename matches exactly (case-sensitive)
4. Game was restarted after changes

### Texture Looks Distorted

**Problem**: Texture appears stretched or warped  
**Solution**: Ensure your texture uses equirectangular projection, not other map projections

### Game Won't Start

**Problem**: Game crashes or won't load  
**Solution**: Check RON file syntax - you might have broken the formatting. Revert changes and try again carefully.

## License

This is an example mod. Replace with your actual texture and license information:

- **Texture**: [Your Name/Source]
- **License**: [CC BY 4.0 / Public Domain / etc]
- **Based on**: [NASA data / Original creation / etc]

## Credits

- Mod created by: [Your Name]
- Mars texture from: [Source]
- Game: Helios Ascension

## Version

- **Mod Version**: 1.0
- **Compatible with**: Helios Ascension v0.1.0+

---

**Enjoy your custom Mars texture!** 🔴
