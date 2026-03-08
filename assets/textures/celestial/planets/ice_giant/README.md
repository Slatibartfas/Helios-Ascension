# Ice Giant Textures

Textures for cold ice giants such as Neptune- and Uranus-like worlds in outer systems.

## Characteristics
- Smooth atmospheres with subtle banding or haze layers
- Blue, cyan, teal, and pale blue-green tones
- Methane-rich upper atmospheres and ice-rich interiors
- Typically found far from the host star

## Runtime Category
- Manifest key: `ice_giant`
- Used for cold giant planets below the gas-giant temperature threshold or below the giant-mass split

## Adding Textures

Drop any equirectangular planet texture here and register it in `assets/data/planet_textures.ron`:

```ron
"ice_giant": [
    "textures/celestial/planets/ice_giant/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA Solar System Exploration textures (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom procedural methane-atmosphere renders
