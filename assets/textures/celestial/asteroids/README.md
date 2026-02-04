# Generic Asteroid Textures

For asteroids without specific textures, we use generic textures based on their spectral classification.

## Current Implementation

1. **Generic Textures**: Three base textures for different asteroid types:
   - `generic_c_type_2k.jpg` - Carbonaceous (dark, carbon-rich)
   - `generic_s_type_2k.jpg` - Silicaceous (stony, rocky)
   - Generic metallic texture (procedurally darker for M-type)

2. **Procedural Variation**: Each asteroid gets unique appearance through:
   - Color variation based on asteroid properties
   - Roughness and metallic properties
   - Deterministic seed based on asteroid name

3. **Classification System**: Asteroids are classified in `solar_system.ron`:
   - `CType` - Most common (~75%), dark carbonaceous
   - `SType` - Rocky stony asteroids (~17%)
   - `MType` - Metallic iron-nickel asteroids (~8%)
   - `Unknown` - Unclassified asteroids

## Dedicated Textures

Some asteroids have dedicated high-quality textures:
- **Vesta** (`vesta_2k.jpg`) - 2K texture from NASA Dawn mission

## Adding More Textures

To add a dedicated texture for an asteroid:
1. Add the texture file to this directory
2. Update the asteroid entry in `assets/data/solar_system.ron`:
   ```ron
   texture: Some("textures/celestial/asteroids/your_asteroid_2k.jpg"),
   ```

## Sources for Additional Textures

1. **NASA 3D Resources**: https://science.nasa.gov/3d-resources/
2. **USGS Astrogeology**: https://astrogeology.usgs.gov/
3. **NASA Image Library**: https://images.nasa.gov/

All NASA textures are public domain and free to use.
