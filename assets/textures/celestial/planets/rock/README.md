# Rock Planet Textures

Textures for bare stony and mineral-rich rocky worlds that are less dusty than `barren` and less oxidized than `martian`.

## Characteristics
- Exposed bedrock, fractured stone plains, and mineral-rich crusts
- Grey, slate, brown-grey, and muted metallic tones
- Little or no vegetation or stable surface water
- Suitable for rocky dwarf planets and airless rocky worlds

## Runtime Category
- Manifest key: `rock`
- Used for a deterministic share of dwarf planets and some airless rocky planets

## Adding Textures

Drop any equirectangular rocky-world texture here and register it in `assets/data/planet_textures.ron`:

```ron
"rock": [
    "textures/celestial/planets/rock/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA rocky body and asteroid mission imagery (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom procedural stone-world or mineral-rich textures
