# Generic Comet Textures

For comets, we use a generic nucleus texture with procedural variations.

## Current Implementation

1. **Generic Texture**: `generic_nucleus_2k.jpg` - Dark, icy comet nucleus texture
   - Based on comet 67P/Churyumov-Gerasimenko from Rosetta mission
   - Dark grey-brown with rough, porous surface features
   - 2K resolution (2048x1024 pixels)

2. **Procedural Variation**: Each comet gets unique appearance through:
   - Color variation to simulate different compositions
   - Brightness adjustments for surface properties
   - Deterministic seed based on comet name for consistency

3. **Automatic Application**: All bodies with `body_type: Comet` automatically use the generic nucleus texture with variations

## Adding Dedicated Textures

To add a dedicated texture for a specific comet:
1. Add the texture file to this directory (e.g., `halley_2k.jpg`)
2. Update the comet entry in `assets/data/solar_system.ron`:
   ```ron
   (
       name: "1P/Halley",
       body_type: Comet,
       // ... other properties ...
       texture: Some("textures/celestial/comets/halley_2k.jpg"),
   )
   ```

The dedicated texture will override the generic one.

## Sources for Additional Textures

1. **NASA Image Library**: https://images.nasa.gov/
2. **ESA Rosetta Mission**: https://www.esa.int/Science_Exploration/Space_Science/Rosetta
3. **NASA Small Bodies**: https://solarsystem.nasa.gov/asteroids-comets-and-meteors/

All NASA/ESA mission data is typically public domain and free to use.

