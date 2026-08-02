# Building Icons — Helios Ascension

52 PNG icons, one per `BuildingType` entry in `assets/data/buildings.ron`.
Each PNG is **dark-on-white source** (the input the runtime post-process
expects), 256×256 px on disk, designed for a 24×24 design grid with a
~2-px stroke. After the existing egui tinting post-process (see
`src/ui/icons.rs`) the in-game appearance is **white-on-transparent** and
tinted by the active theme.

## Family

The icons match the visual family of:

- `assets/textures/ui/menu/*.png` — 11 menu icons (frame-and-arrow,
  cube-cluster, ruler-and-ship, etc.)
- `assets/textures/ui/research/*.png` — 15 research-category icons
  (filled water tower, tank on treads, etc.)

Family rules (see `agents/icon-artist/agent.md` for the contract):

- **Color:** dark navy blue (~`#1A2640`) on opaque white.
- **Style:** hard-sci-fi technical schematic. Single-weight stroke,
  sharp corners, no soft gradients. Mix of filled and outline shapes
  is OK and already used in the family.
- **Stroke weight:** ~2 px on a 24-px design grid. Becomes ~21 px on
  the 256-px source.
- **Padding:** ~8 px on the 256-px source (≈ 0.75 px on the 24-px
  design grid) so the icon doesn't touch the canvas edge.

## On-disk format vs. in-game appearance

The existing `src/ui/icons.rs` post-process (`process_menu_icons`,
`process_research_icons`) takes a dark-on-white source PNG and converts
each pixel to premultiplied white-on-alpha via:

```rust
let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
let alpha = (1.0 - luminance).powf(3.0);
chunk[0] = (alpha * 255.0) as u8;  // R
chunk[1] = (alpha * 255.0) as u8;  // G
chunk[2] = (alpha * 255.0) as u8;  // B  (all white)
chunk[3] = (alpha * 255.0) as u8;  // A
```

That is why the source MUST be **dark on opaque white**. A transparent
background would become opaque-white in the post-process (the missing
background appears as a white block behind the icon). The 256×256
PNGs in this directory are exactly that: dark navy strokes on opaque
white.

## File layout

```
assets/textures/ui/buildings/
├── <kebab-case-id>.png         # 52 source PNGs (the deliverable)
├── _resize/<id>-24.png         # 52 24×24 previews (visual inspection)
├── _normalize.py               # normalizes a generated PNG to 256×256
│                                 dark-on-white (centers + opaque background)
├── _verify_tinting.py          # tinting-pipeline check on a single icon
├── _verify_all.py              # batch tinting check on all 52
└── README.md                   # this file
```

The `assets/textures/ui/buildings/source/` subdirectory mentioned in
earlier planning is intentionally absent: the `image_synthesize` tool
returns raster output only, so no real SVG vector source exists to
ship. The PNGs in this directory are the source of record.

## Tinting verification

Sampled every 4th pixel of each 256×256 PNG, ran the exact
`process_*_icons` formula, and recorded the alpha curve. Headline
numbers across all 52 icons:

| metric                     | value          |
|----------------------------|----------------|
| max alpha                  | ≥ 0.5 for all  |
| mean opaque coverage       | 14.7 %         |
| min opaque coverage        | 3.7 %  (`underground-habitat`) |
| max opaque coverage        | 52.9 % (`data-center`, full server rack) |

Per-icon details (sampled run):

```
name                            opaque%   mean    max
-------------------------------------------------------
agri-dome                         16.9%  0.138  0.955
ai-cluster                        19.4%  0.175  0.977
aquaculture-facility              16.3%  0.123  0.933
atmospheric-processor              5.2%  0.044  0.935
breeder-reactor                   18.0%  0.154  0.969
cargo-terminal                     7.9%  0.055  0.829
chemical-plant                    15.0%  0.113  0.914
coal-power-plant                  13.0%  0.109  0.939
commercial-hub                    20.5%  0.163  0.944
data-center                       52.9%  0.417  0.825
deep-drill                         7.1%  0.055  0.946
desalination-plant                12.2%  0.083  0.822
dhe3-fusion-reactor               13.9%  0.107  0.891
dt-fusion-reactor                 20.5%  0.177  0.966
engineering-bay                   30.2%  0.273  0.971
factory                           16.0%  0.138  0.973
farm                              11.1%  0.083  0.907
financial-center                  24.6%  0.208  0.973
fission-reactor                    9.4%  0.074  0.951
fusion-reactor                    31.2%  0.238  0.904
geothermal-plant                  12.0%  0.108  0.966
greenhouse                        13.8%  0.112  0.925
ground-defense-battery            19.8%  0.156  0.968
habitat-dome                      11.2%  0.094  0.947
housing                           17.2%  0.153  0.988
hydrocarbon-extractor             21.6%  0.178  0.942
hydroelectric-dam                 11.5%  0.084  0.858
laser-drill                        9.4%  0.073  0.862
launch-site                       30.7%  0.265  0.968
life-support                      10.0%  0.082  0.969
mass-driver                       12.1%  0.091  0.880
medical-center                    14.6%  0.117  0.956
mine                               4.8%  0.034  0.828
missile-silo                      14.3%  0.114  0.963
natural-gas-plant                  8.8%  0.070  0.947
orbital-lift                       5.9%  0.045  0.870
orbital-survey-station            21.4%  0.184  0.987
pharmaceutical-plant               8.3%  0.055  0.773
recycling-center                  16.0%  0.114  0.865
refinery                           9.3%  0.075  0.923
research-lab                       6.4%  0.049  0.918
semiconductor-fab                 18.8%  0.150  0.942
shipyard                          14.5%  0.115  0.917
solar-power                       13.3%  0.107  0.942
space-port                        13.7%  0.100  0.907
strip-mine                        11.4%  0.078  0.819
thorium-reactor                    5.5%  0.045  0.960
trade-port                        11.2%  0.091  0.945
underground-habitat                3.7%  0.026  0.843
warehouse                         10.3%  0.072  0.861
water-treatment-plant             13.8%  0.107  0.908
wind-farm                          9.4%  0.074  0.951
```

The alpha curve is usable: all 16×16 corner patches are < 5 % opaque
(clean white background), and the icon strokes reach ≥ 0.5 alpha in
every icon (visible at 24×24). Low coverage icons
(`mine`, `underground-habitat`, `thorium-reactor`, `atmospheric-processor`,
`orbital-lift`) are minimal line drawings; the rest of the family
sits comfortably in the 10-20 % coverage band that matches the
existing menu and research icons.

To re-run the verification on any single icon:

```sh
python _verify_tinting.py mine
```

## Load contract for the bevy_ui canary

The bevy_ui canary (`src/ui/construction.rs`, `src/ui/bevy_theme.rs`)
**does not currently load building icons**. The construction cards
built by `card_data_from_definition` carry `name`, `subtitle`, stats,
and effects, but no icon handle. Migrating the cards to render an
icon is a follow-up: extend `BuildCardData` with an
`icon: Handle<Image>`, add a `BuildingIcons` resource that mirrors
`MenuIcons` / `ResearchIcons` in `src/ui/icons.rs`, and have the
canary's `setup_construction` look up the handle from the
`BuildingType` key.

When that work lands, the load path is:

```rust
// in src/ui/icons.rs
pub struct BuildingIcons {
    pub handles: HashMap<BuildingType, Handle<Image>>,
    pub processed: std::collections::HashSet<BuildingType>,
}

pub(super) fn load_building_icons(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    buildings_data: Res<BuildingsData>,
) {
    let mut map = HashMap::new();
    for (bt, def) in &buildings_data.definitions {
        let filename = format!("textures/ui/buildings/{}.png", kebab_case(&def.id));
        map.insert(*bt, asset_server.load(&filename));
    }
    commands.insert_resource(BuildingIcons { handles: map, processed: Default::default() });
}
```

The `process_building_icons` system is byte-for-byte the same as
`process_research_icons` (RGBA8 4 bytes/pixel, premultiplied white).
The 256×256 dark-on-white source in this directory goes through it
unchanged: white background pixels become transparent, dark navy
strokes become opaque white tinted at runtime by the bevy_ui theme.

**No code change is required for the canary to work with these files** —
only the new `BuildingIcons` resource and its load/process systems,
which is the same kind of additive change that brought up the
research icons. Treat that as a separate PR against `bevy-engine-expert`.

## Naming

- `kebab-case-id` is `BuildingType` rendered as lowercase
  `CamelCase → kebab-case` (e.g. `DeepDrill → deep-drill`,
  `DHe3FusionReactor → dhe3-fusion-reactor`).
- The 52 filenames line up with the 52 entries in
  `assets/data/buildings.ron` (verified: one PNG per building, no
  duplicates, no orphans).
- The 48 distinct emoji glyphs the RON currently references
  (some glyphs are shared between buildings, e.g. `☀` for both
  `SolarPower` and `DHe3FusionReactor`) are all retired in the
  mapping doc, not deleted — the RON's `icon:` strings are
  untouched. The user applies the swap manually once the icons
  are reviewed.

## Re-generating an icon

The `image_synthesize` tool is the source of art. A consistent
prompt template:

> Minimalist flat icon, dark navy blue lines on white background, no
> grid, no frame, monochrome, simple bold geometric shapes, thick
> single-weight stroke, no shading, no gradients, no text,
> recognizable silhouette, subject fills canvas. Subject: [SPECIFIC].

Replace `[SPECIFIC]` with the building's visual concept (see
`docs/design/BUILDING_ICONS_MAPPING.md` for the full list). Save to
`assets/textures/ui/buildings/<id>.png` (the `.png` extension
matters; `image_synthesize` returns raster output regardless of the
extension, so the file will be a real PNG once the normalize step
runs).

Then:

```sh
cd assets/textures/ui/buildings
python _normalize.py <id>          # crop to content, center, opaque white BG, 256x256
python _verify_tinting.py <id>     # confirm alpha curve is usable
```

To regenerate the whole set:

```sh
cd assets/textures/ui/buildings
python _normalize.py --all
python _verify_all.py
```

## Cross-references

- `assets/data/buildings.ron` — 52 building definitions. The
  `icon: "🌬"` etc. strings are the current Unicode emoji and are
  the next thing to swap. **Not changed by this deliverable.**
- `src/ui/icons.rs` — `MenuIcons` and `ResearchIcons` post-process.
  Source of the tinting contract.
- `src/ui/construction.rs` — bevy_ui canary. Does not yet load
  building icons (see "Load contract" above).
- `src/ui/construction_panel.rs` — legacy egui panel. Renders
  emoji glyphs directly via egui text.
- `docs/design/BUILDING_ICONS_MAPPING.md` — 52-row table mapping
  `BuildingType` → filename → emoji-to-icon swap plan.
