# Tundra Planet Textures

Textures for cold, partially frozen worlds shaped by permafrost, seasonal ice, and exposed rocky ground.

## Characteristics
- Permafrost plains, patchy ice, and cold dry basins
- Grey, blue-grey, white, and muted brown tones
- Colder than habitable alpine worlds, but not fully frozen like `ice`
- Typical procedural band: -100C to below -20C

## Runtime Category
- Manifest key: `tundra`
- Used for cold terrestrial worlds between habitable and fully frozen conditions

## Adding Textures

Drop any equirectangular planet texture here and register it in `assets/data/planet_textures.ron`:

```ron
"tundra": [
    "textures/celestial/planets/tundra/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA Solar System Exploration textures (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom frost-world or permafrost terrain textures
