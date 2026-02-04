# Celestial Textures - High Resolution Version

## Current Status - DECISION MADE

After research, we have chosen to use **Solar System Scope 8K textures** (CC BY 4.0) instead of direct NASA public domain sources. Here's why:

### Why Solar System Scope (CC BY 4.0)?

✅ **8K Resolution** - 8192x4096 pixels (8K) for major bodies  
✅ **Convenient Packages** - Pre-processed, ready-to-use texture maps  
✅ **Consistent Quality** - All textures in same format and projection  
✅ **Based on NASA Data** - Uses original NASA mission imagery  

### Why Not Direct NASA Sources?

❌ **Lower Resolution** - NASA provides 2K-4K textures (not 8K)  
❌ **Inconsistent** - Different formats and projections  
❌ **More Work** - Requires manual processing and conversion  

### Trade-off Accepted

**We chose**: 8K resolution with simple attribution (CC BY 4.0)  
**Over**: 2-4K resolution with no attribution (NASA public domain)

For a game focused on visual quality, the 4x resolution improvement (16x more pixels) is worth the simple attribution requirement.

## License & Attribution

**Current Source**: Solar System Scope (CC BY 4.0 - requires attribution)  
**Attribution**: "Textures provided by Solar System Scope (https://www.solarsystemscope.com/)"  
**Based on**: NASA public domain mission data

**This attribution is now in the main README.md**

## Texture Resolutions

### 8K Textures (8192x4096 pixels)
- **Sun**: 3.6MB
- **Mercury**: 15MB
- **Venus Surface**: 12MB  
- **Earth**: 4.4MB
- **Mars**: 8.1MB
- **Jupiter**: 3.0MB
- **Saturn**: 1.1MB
- **Moon**: 15MB

### 2K Textures (2048x1024 pixels)
- **Venus Atmosphere**: 225KB
- **Uranus**: 76KB (8K not yet available)
- **Neptune**: 236KB (8K not yet available)
- **Pluto**: 16KB
- **Ceres**: 1.1MB
- **Eris**: 1.1MB

### 1K Textures (1024x512 pixels)
- **Jupiter's moons** (Io, Europa, Ganymede, Callisto): ~16KB each
- **Saturn's moons** (Titan, Enceladus, Rhea, Iapetus, Dione, Tethys): ~16KB each

## Why 8K?

8K textures provide:
- ✅ **4x the resolution** of 2K textures
- ✅ **Much better detail** when zooming in
- ✅ **Professional quality** suitable for close-up viewing
- ✅ **Future-proof** for 4K and 8K displays
- ✅ **Still based on real NASA mission data**

## Performance Considerations

- Total size: 64MB (reasonable for modern systems)
- Textures are loaded asynchronously by Bevy's asset system
- May take a few seconds longer to load initially
- Much better visual quality is worth the extra memory

## Summary

We use Solar System Scope's 8K textures (CC BY 4.0) for maximum visual quality. These provide:
- 4x the resolution of typical 2K textures
- 16x more pixels for dramatic detail improvement
- Convenient pre-processed packages
- Consistent quality across all bodies

Attribution is provided in the main README.md as required.

See `EXPANSION_OPPORTUNITIES.md` for information about potential future additions (Mars moons, asteroids, comets, additional Saturn/Uranus/Neptune moons).

See `SOURCES.md` for complete source information and technical details.
