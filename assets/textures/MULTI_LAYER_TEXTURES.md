# Multi-Layer Texture System

This document explains the multi-layer texture system implemented in Helios Ascension.

## Overview

The multi-layer texture system allows celestial bodies to have multiple texture layers for advanced visual effects:

- **Base/Daymap**: Primary surface texture
- **Night/Emissive**: City lights, lava, etc. (visible on dark side)
- **Clouds/Atmosphere**: Semi-transparent cloud layer
- **Normal Map**: Surface detail (bumps, craters)
- **Specular Map**: Shininess variation (oceans vs land)

## Current Implementation Status

### ✅ Implemented
- Data structure for multi-layer textures
- Base texture rendering
- Normal map support
- Automatic fallback to single texture if multi-layer not specified

### 🚧 Partial (TODO)
- Night/emissive texture rendering (needs shader work)
- Cloud layer rendering (needs separate mesh/material)
- Specular map integration
- Atmosphere scattering effects

## Data Structure

```rust
/// Multi-layer texture configuration for advanced rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiLayerTextures {
    /// Base color/albedo texture (day side for planets)
    pub base: String,
    /// Night-side emissive texture (city lights, etc.)
    #[serde(default)]
    pub night: Option<String>,
    /// Cloud/atmosphere layer texture
    #[serde(default)]
    pub clouds: Option<String>,
    /// Normal/bump map for surface detail
    #[serde(default)]
    pub normal: Option<String>,
    /// Specular/glossiness map (shininess variation)
    #[serde(default)]
    pub specular: Option<String>,
}
```

## Usage in RON Files

### Example: Earth with Full Multi-Layer
```ron
(
    name: "Earth",
    body_type: Planet,
    // ... other fields ...
    multi_layer_textures: Some((
        base: "textures/celestial/planets/earth_daymap_8k.jpg",
        night: Some("textures/celestial/planets/earth_nightmap_8k.jpg"),
        clouds: Some("textures/celestial/planets/earth_clouds_8k.jpg"),
        normal: Some("textures/celestial/planets/earth_normal_8k.png"),
        specular: Some("textures/celestial/planets/earth_specular_8k.png"),
    )),
)
```

### Example: Venus with Surface + Atmosphere
```ron
(
    name: "Venus",
    body_type: Planet,
    // ... other fields ...
    multi_layer_textures: Some((
        base: "textures/celestial/planets/venus_surface_8k.jpg",
        clouds: Some("textures/celestial/planets/venus_atmosphere_2k.jpg"),
    )),
)
```

### Example: Simple Single Texture (Backward Compatible)
```ron
(
    name: "Mars",
    body_type: Planet,
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_8k.jpg"),
)
```

## Texture Priority

The system checks for textures in this order:

1. **Multi-layer textures** (`multi_layer_textures`) - if present, uses base texture
2. **Single dedicated texture** (`texture`) - backward compatible
3. **Generic texture** - fallback based on body type
4. **Procedural variation** - applied to generic textures only

```rust
let (base_texture, normal_texture, is_dedicated) = 
    if let Some(ref multi) = body_data.multi_layer_textures {
        // Multi-layer: use base + normal
        (load(multi.base), multi.normal.map(load), true)
    } else if let Some(ref tex) = body_data.texture {
        // Single texture
        (load(tex), None, true)
    } else {
        // Generic texture
        (get_generic(body), None, false)
    };
```

## Downloaded Multi-Layer Textures

### Earth (Solar System Scope, CC BY 4.0)
- **Daymap**: `earth_daymap_8k.jpg` (4.4MB, 8192×4096)
- **Nightmap**: `earth_nightmap_8k.jpg` (3.0MB, 8192×4096) - city lights
- **Clouds**: `earth_clouds_8k.jpg` (12MB, 8192×4096)
- **Normal**: `earth_normal_8k.png` (9.1MB, 8192×4096)
- **Specular**: `earth_specular_8k.png` (1.8MB, 8192×4096)

**Total**: 30.3MB for complete Earth multi-layer

### Venus (Solar System Scope + existing)
- **Surface**: `venus_surface_8k.jpg` (12MB, 8192×4096)
- **Atmosphere**: `venus_atmosphere_2k.jpg` (225KB, 2048×1024)

## Implementation Roadmap

### Phase 1: Foundation ✅ COMPLETE
- [x] Data structure for multi-layer textures
- [x] Base texture rendering
- [x] Normal map support
- [x] Download Earth multi-layer textures
- [x] Download Venus surface + atmosphere

### Phase 2: Night/Emissive Rendering (TODO)
- [ ] Custom shader to blend day/night based on light direction
- [ ] Night texture appears on dark side facing away from sun
- [ ] Smooth transition at terminator (day/night boundary)
- [ ] Emissive intensity based on surface darkness

### Phase 3: Cloud Layer (TODO)
- [ ] Separate mesh for cloud layer (slightly larger radius)
- [ ] Semi-transparent material for clouds
- [ ] Optional: Animated cloud rotation
- [ ] Optional: Cloud shadows on surface

### Phase 4: Advanced Materials (TODO)
- [ ] Specular map integration
- [ ] PBR workflow with metallic/roughness maps
- [ ] Atmosphere scattering shader
- [ ] Rim lighting for atmospheric glow

## Technical Challenges

### Night Texture Rendering
**Challenge**: Bevy's StandardMaterial doesn't natively support day/night blending  
**Solution Options**:
1. Custom shader material
2. Two separate meshes with different materials
3. Extend StandardMaterial with custom shader

**Recommended**: Custom shader material for best quality

### Cloud Layer Rendering
**Challenge**: Need semi-transparent second layer  
**Solution Options**:
1. Separate mesh at slightly larger radius with AlphaMode::Blend
2. Shader that samples both textures
3. Particle system for volumetric clouds

**Recommended**: Separate mesh (simplest, good performance)

### Specular/Roughness Maps
**Challenge**: Current system uses uniform roughness  
**Solution**: StandardMaterial supports metallic_roughness_texture

## Example Shader Pseudocode

### Day/Night Blending Shader
```wgsl
// Fragment shader
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(sun_position - world_position);
    let normal = normalize(in.world_normal);
    
    // Dot product: 1.0 = facing sun, -1.0 = away from sun
    let sun_factor = dot(normal, light_dir);
    
    // Sample both textures
    let day_color = textureSample(day_texture, sampler, in.uv);
    let night_color = textureSample(night_texture, sampler, in.uv);
    
    // Blend based on sun factor
    // 0.0-0.2: full night, 0.8-1.0: full day, 0.2-0.8: transition
    let blend = smoothstep(0.0, 0.4, sun_factor);
    let color = mix(night_color, day_color, blend);
    
    return color;
}
```

## Performance Considerations

### Memory Usage
- 8K textures: ~4-12MB each (JPEG compressed)
- Normal/specular PNG (lossless): ~2-9MB each (compressed/lossless)
- Full multi-layer Earth: ~30MB VRAM

### Optimization Strategies
1. **LOD system**: Use lower resolution textures for distant objects
2. **Streaming**: Load high-res textures only when needed
3. **Compression**: Convert TIF to compressed formats
4. **Mipmaps**: Auto-generated by Bevy for smooth appearance

### Recommended Resolutions
- **Close planets** (Earth, Mars): 8K textures
- **Medium distance** (Jupiter, Saturn): 4K textures
- **Distant planets** (Uranus, Neptune): 2K textures
- **Moons**: 1K-2K depending on importance

## Adding Multi-Layer Textures

### Step 1: Prepare Textures
Ensure all textures are:
- Equirectangular projection (360° longitude × 180° latitude)
- Same resolution (or at least same aspect ratio 2:1)
- Properly aligned (0° longitude at center)

### Step 2: Place Files
Copy texture files to appropriate directory:
```bash
assets/textures/celestial/planets/
assets/textures/celestial/moons/
```

### Step 3: Update RON File
Edit `assets/data/solar_system.ron`:
```ron
(
    name: "YourBody",
    // ... other fields ...
    multi_layer_textures: Some((
        base: "textures/celestial/planets/yourbody_day_4k.jpg",
        night: Some("textures/celestial/planets/yourbody_night_4k.jpg"),
        // ... optional fields ...
    )),
)
```

### Step 4: Test
Run the game and verify textures load correctly. Check console for any loading errors.

## Future Bodies to Add

### High Priority
- **Mars**: Add night texture (Phobos/Deimos lights from bases)
- **Gas Giants**: Add cloud normal maps for depth
- **Titan**: Add atmosphere layer

### Medium Priority
- **Europa**: Add ice crack normal maps
- **Io**: Add volcanic glow emissive map
- **Triton**: Add nitrogen ice specular map

### Low Priority
- **Small moons**: Most can use single texture
- **Asteroids**: Generally don't need multi-layer

## See Also

- `ALTERNATIVE_SOURCES.md` - Where to find multi-layer textures
- `MODDING_GUIDE.md` - How to add custom textures
- `TEXTURE_IMPLEMENTATION.md` - Technical implementation details
- `PROCEDURAL_SYSTEM.md` - Procedural variation system
