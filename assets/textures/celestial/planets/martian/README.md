# Martian Planet Textures

Textures for rust-red oxidised rocky worlds inspired by Mars-like terrain.

## Characteristics
- Iron-oxide red and brown regolith
- Basaltic plains, crater fields, dust basins, and weathered highlands
- Little or no stable standing surface water
- Distinctly redder than the generic `barren` category

## Runtime Category
- Manifest key: `martian`
- Shares the hot rocky band with `desert`, but represents oxidised red worlds instead of sandy yellow deserts

## Adding Textures

Drop any equirectangular Mars-like texture here and register it in `assets/data/planet_textures.ron`:

```ron
"martian": [
    "textures/celestial/planets/martian/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA Mars mission imagery and colour maps (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom procedural oxidised-rock textures
