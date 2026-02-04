# Alternative Texture Sources

This document lists alternative sources for high-quality planetary textures beyond Solar System Scope.

## Why Alternative Sources?

Solar System Scope has excellent Earth textures (8K daymap, nightmap, clouds, normal, specular) and major planet textures, but limited availability for moons and smaller bodies. These alternative sources provide public domain or free-to-use textures for comprehensive coverage.

## Recommended Sources

### 1. Planet Pixel Emporium ⭐ RECOMMENDED
**Website**: https://planetpixelemporium.com/planets.html  
**License**: Free for artwork/renders (including commercial), credit required  
**Quality**: 1K, 2K, 4K, up to 10K resolution  
**Coverage**: All planets, moons, asteroids

**Available Textures**:
- **Planets**: Mercury, Venus, Earth, Mars, Jupiter, Saturn, Uranus, Neptune
- **Moons**: Luna, Io, Europa, Ganymede, Callisto, Titan, Enceladus, Mimas, and more
- **Additional**: Bump maps, specular maps, night lights for Earth
- **Format**: JPEG, TIFF

**Direct Links**:
- Earth: https://planetpixelemporium.com/earth8081.html (up to 10K!)
- Mars: https://planetpixelemporium.com/mars.html
- Moon: https://planetpixelemporium.com/moon.html
- Jupiter: https://planetpixelemporium.com/jupiter.html
- Saturn: https://planetpixelemporium.com/saturn.html

**Attribution**: "Planet textures courtesy of James Hastings-Trew (planetpixelemporium.com)"

**Note**: Redistribution of raw files not permitted, but use in projects is fine.

### 2. USGS Astrogeology Science Center ⭐ PUBLIC DOMAIN
**Website**: https://astrogeology.usgs.gov/search  
**License**: Public domain (US Government)  
**Quality**: Varies (500m-4km per pixel), typically 1K-4K  
**Coverage**: Excellent for moons, good for planets

**Available Textures**:
- **Jovian Moons**: Io, Europa, Ganymede, Callisto (Voyager/Galileo data)
- **Saturnian Moons**: Titan, Enceladus, Mimas, Tethys, Dione, Rhea, Iapetus, Phoebe (Cassini data)
- **Mars**: High-resolution global mosaics
- **Moon**: Lunar Reconnaissance Orbiter data
- **Format**: TIFF, JPG

**How to Use**:
1. Visit https://astrogeology.usgs.gov/search
2. Search for body name (e.g., "Europa")
3. Look for "Global Mosaic" products
4. Download GeoTIFF or JPG
5. Convert to equirectangular projection if needed

**Direct Example**:
- Europa 500m: https://astrogeology.usgs.gov/search/map/europa_voyager_galileo_ssi_global_mosaic_500m

**Attribution**: None required (public domain), but citing USGS/NASA is polite

### 3. NASA 3D Resources
**Website**: https://github.com/nasa/NASA-3D-Resources  
**License**: Public domain / NASA open data  
**Quality**: Varies, 2K-8K available  
**Coverage**: Major planets and moons

**Available**:
- Direct 3D models with texture files
- Mars, Moon, Europa, Ganymede, and others
- Some include bump/normal maps

**Format**: PNG, JPG, OBJ with textures

### 4. NASA Image and Video Library
**Website**: https://images.nasa.gov/  
**License**: Public domain (with restrictions on some imagery)  
**Quality**: Varies greatly  
**Coverage**: Raw mission data

**How to Use**:
1. Search for "{planet} global mosaic"
2. Look for full-disk images
3. Download high-resolution versions
4. Process into equirectangular maps (advanced)

**Note**: Requires image processing skills for best results

## Comparison Table

| Source | License | Resolution | Planets | Moons | Ease of Use |
|--------|---------|-----------|---------|-------|-------------|
| **Planet Pixel Emporium** | Free w/ credit | 1K-10K | ✅ All | ✅ Many | ⭐⭐⭐⭐⭐ |
| **USGS Astrogeology** | Public Domain | 1K-4K | ⚠️ Some | ✅ Excellent | ⭐⭐⭐⭐ |
| **NASA 3D Resources** | Public Domain | 2K-8K | ✅ Major | ✅ Some | ⭐⭐⭐ |
| **NASA Image Library** | Public Domain | Varies | ⚠️ Raw | ⚠️ Raw | ⭐⭐ |
| **Solar System Scope** | CC BY 4.0 | 8K | ✅ All | ⚠️ Limited | ⭐⭐⭐⭐⭐ |

## Recommended Strategy

### For This Project:

1. **Earth** ✅ DONE: Use Solar System Scope 8K multi-layer (daymap, nightmap, clouds, normal, specular)
2. **Venus** ✅ DONE: Use Solar System Scope 8K surface + atmosphere
3. **Major Planets**: Use Solar System Scope 8K (already downloaded)
4. **Moons**: Use Planet Pixel Emporium or USGS Astrogeology (public domain)
5. **Asteroids**: Use NASA mission data (Vesta from Dawn, Bennu from OSIRIS-REx)

### Download Script Example

```bash
#!/bin/bash
# Download from Planet Pixel Emporium
# Note: Check their terms - download for use, not redistribution

# Example: Moon 4K
wget "https://planetpixelemporium.com/download/download.php?earthmap1k.jpg&271" -O moon_4k.jpg

# USGS Astrogeology direct downloads
wget "https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_Galileo_SSI_global_mosaic_500m.jpg" -O europa_4k.jpg
```

## Converting Textures

If textures are not in equirectangular format:

### Using ImageMagick
```bash
# Convert cylindrical to equirectangular (if needed)
convert input.jpg -distort Polar 0 output_equirect.jpg

# Resize to specific resolution
convert input.jpg -resize 8192x4096 output_8k.jpg
```

### Using Python/PIL
```python
from PIL import Image
img = Image.open('input.jpg')
img_resized = img.resize((8192, 4096), Image.LANCZOS)
img_resized.save('output_8k.jpg', quality=95)
```

## Future Enhancements

1. **Download all moons from USGS** (public domain, no attribution needed)
2. **Add Planet Pixel Emporium textures** for consistency
3. **Create texture pack manager** for easy switching between sources
4. **Support multiple resolution levels** (LOD system)

## Questions?

- For licensing questions, consult each source's terms
- For texture processing help, see TEXTURE_IMPLEMENTATION.md
- For adding textures to the game, see MODDING_GUIDE.md
