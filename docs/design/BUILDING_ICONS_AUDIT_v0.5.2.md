# Building Icon Audit — `rework-ui-design` (read-only)

> **Scope.** Read-only audit of `assets/textures/ui/buildings/*.png` (102 files)
> against the post-processing recipe in `src/ui/construction.rs:78-129`
> (luminance key + premultiplied white, byte-for-byte the same as
> `src/ui/icons.rs:54-87` for menus and `:137-185` for research).
> Coverage is the runtime `alpha = (1.0 - luminance).powf(3.0)` summed over
> every pixel of a 256×256 PNG, expressed as a percentage of total area —
> the higher the number, the more the icon will look like a tinted
> "filled blob" once the bevy_ui cyan tint is applied.
>
> **Correction up front.** The hypothesis that `iron-mine.png` is a
> "dumbbell with solid weight blocks" and the related audit targets
> render as "filled" is **incorrect**. The 4 named audit targets are
> all clean line art (3.8 – 16.3 % coverage). The actual
> "filled-blob" offenders in this set are in the **original 52** —
> `data-center.png` (52.9 %), `launch-site.png` (33.3 %),
> `engineering-bay.png` (31.9 %), `financial-center.png` (25.7 %) —
> not the v0.5.2 batch the brief pointed at. See §3.
>
> **Conclusion.** The 4 named audit targets and the 7 v0.5.2 spot-checks
> are all in good shape and need no regeneration for "filled-blob"
> reasons. The real MUST-regenerate list is three original-52 icons
> that were designed as solid silhouettes in the previous era and now
> read as dense cyan blocks at 16 – 24 px.

---

## 1. Post-processing recipe (reference)

From `src/ui/construction.rs:105-125`, the runtime does this to every
building-icon pixel:

```rust
let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
let alpha = (1.0_f32 - luminance).powf(3.0);
let a = alpha.clamp(0.0, 1.0);
let pa = (a * 255.0) as u8;
chunk[0] = pa;  // R
chunk[1] = pa;  // G
chunk[2] = pa;  // B  (all white — tinted at draw time)
chunk[3] = pa;  // A
```

Implication: a **solid dark area** in the source PNG (luminance ≈ 0)
becomes a **solid opaque white** area in the post-processed texture,
which bevy_ui then tints with the UI color (`#00F2FF` cyan in
`src/ui/theme.rs:61`). The on-screen result is "a solid cyan shape".

The on-disk format is verified end-to-end: all 102 PNGs are 256×256
`Format24bppRgb` (no alpha), and the `scripts/invert_icons.py` script
already converted the v0.5.2 batch from white-on-transparent to
dark-on-white. The post-process is working as designed; the question
is whether the source art is appropriate input.

The family contract (`assets/textures/ui/buildings/README.md:19-28`,
`docs/design/BUILDING_ICONS_MAPPING.md:14-19`) is:

- 24×24 design grid, 2-px stroke, 2-px corner radius, legible at 16 px
- 256×256 on disk, ~8 px padding
- Dark navy `#1A2640` on opaque white
- "**Mix of filled and outline shapes is OK** and already used in the
  family" — this is explicit permission for solid silhouettes

So solid fill is allowed in the family, but it is allowed when the
fill IS the subject (e.g. `research/industry.png` is a solid factory
silhouette and reads correctly). The problem is when the fill is
**incongruent with the subject** — e.g. `data-center` is 3 solid
black bars where 3 outlined server-blade rectangles would be more
legible.

---

## 2. Coverage analysis — full set

102 PNGs analyzed (every file in `assets/textures/ui/buildings/`).
Mean alpha follows coverage closely; max alpha is the deepest single
pixel after the runtime pow-3 curve.

### 2.1 Top 12 by coverage (most "filled" after post-process)

| Rank | File | Opaque % | Mean α | Max α | Verdict |
|------|------|---------:|-------:|------:|---------|
| 1 | `data-center.png` | **52.9** | 0.42 | 0.91 | **MUST regenerate** — 3 solid server blades |
| 2 | `fusion-reactor.png` | 34.0 | 0.24 | 0.90 | Family-OK — line-art toroid with 8 detail nodes |
| 3 | `launch-site.png` | **33.3** | 0.26 | 0.98 | **MUST regenerate** — solid rocket + solid tower |
| 4 | `engineering-bay.png` | **31.9** | 0.27 | 0.98 | **MUST regenerate** — solid bolt + solid gear |
| 5 | `financial-center.png` | 25.7 | 0.21 | 0.98 | Family-OK — Greek-temple line art with thick columns |
| 6 | `hydrocarbon-extractor.png` | 23.8 | 0.18 | 0.96 | Family-OK — derrick + pump-jack line art |
| 7 | `orbital-survey-station.png` | 22.5 | 0.18 | 0.99 | Family-OK — satellite + panel-grid line art |
| 8 | `dt-fusion-reactor.png` | 22.3 | 0.18 | 0.97 | Family-OK — atom symbol with 3 crossing orbits |
| 9 | `commercial-hub.png` | 21.9 | 0.16 | 0.96 | Family-OK — storefront + awning line art |
| 10 | `semiconductor-fab.png` | 21.0 | 0.15 | 0.97 | Family-OK — chip + 4-side pin grid |
| 11 | `ai-cluster.png` | 20.8 | 0.17 | 0.98 | Family-OK — chip + 3 neural-network nodes |
| 12 | `ground-defense-battery.png` | 20.3 | 0.16 | 0.98 | Family-OK — turret + shield line art |

### 2.2 The 4 named audit targets (HYPOTHESIS WAS WRONG)

| File | Opaque % | Verdict |
|------|---------:|---------|
| `iron-mine.png` | **7.0** | Clean line art — outlined dumbbell (no solid weight blocks) |
| `gold-mine.png` | **16.3** | Clean line art — wash plant with 3 small dark dots |
| `silicates-mine.png` | **10.1** | Clean line art — 3-crystal cluster with facet lines |
| `auto-iron-mine.png` | **3.8** | Clean line art — outlined dumbbell + small satellite |

All four are well below the 20 % family band, render as crisp cyan
line drawings, and need no regeneration for "filled-blob" reasons.
**The user's premise that the icons were filled was not supported by
the source PNGs.** See §4 for the likely misdiagnosis.

### 2.3 v0.5.2 spot-checks (7 icons)

| File | Opaque % | Mean α | Max α | Verdict |
|------|---------:|-------:|------:|---------|
| `tungsten-mine.png` | 16.9 | 0.12 | 0.74 | Clean line art — 3-D isometric cube with "W" |
| `lithium-mine.png` | 7.6 | 0.05 | 0.74 | Clean line art — battery body with "Li" |
| `auto-tungsten-mine.png` | 4.4 | 0.03 | 0.74 | Clean line art — cube + small satellite + asteroid |
| `auto-lithium-mine.png` | 4.8 | 0.03 | 0.74 | Clean line art — battery + satellite + asteroid |
| `he3-mine.png` | 7.0 | 0.05 | 0.93 | Clean line art — industrial building + smokestack |
| `water-processor.png` | 10.9 | 0.07 | 0.95 | Clean line art — tank with helix coil + 3 droplets |
| `auto-he3-mine.png` | 4.6 | 0.03 | 0.74 | Clean line art — gas cylinder with "He3" label |

All 7 are line art (4.4 – 16.9 % coverage) and read as the
intended "schematic" look. **The v0.5.2 batch is in good shape.**

### 2.4 Distribution shape

The 102-icon coverage distribution is bimodal:

- **Original 52** band: 4 % – 53 %, mean ≈ 14 %, skewed by the 3
  silhouette outliers (data-center, launch-site, engineering-bay).
- **v0.5.1 batch (5)**: 7 % – 16 %, mean ≈ 11 %.
- **v0.5.2 batch (45)**: 2.4 % – 17 %, mean ≈ 8 %.

The v0.5.2 batch is **the thinnest part of the set**, not the
densest. Whatever made the user perceive "filled blobs", it is not
the v0.5.2 batch.

---

## 3. Per-icon assessment for the 4 named audit targets

### 3.1 `iron-mine.png` (7.0 % coverage) — **MOSTLY LINE ART**

The icon is a **dumbbell** drawn entirely in outline: a horizontal
bar (~16 × 2 px on the design grid) with two outlined weight blocks
on each end (smaller inner blocks ~3 × 8 px and larger outer blocks
~4 × 10 px, all 2-px stroke, no fill). Ground line is a single
horizontal stroke at the bottom of the 256-px canvas. No solid dark
area anywhere except a few anti-aliasing pixels on the stroke
edges. **Hypothesis "solid weight blocks" is wrong** — the blocks
are rounded-rectangle outlines.

Semantic note (orthogonal to the filled-blob question): the dumbbell
is a fitness/weightlifting symbol, not a mining symbol, and does not
match the visual logic of the rest of the v0.5.2 mine series
(crystals, cubes, bars, atoms, derricks, geometric pentagons). See
§6.1 for the redesign.

### 3.2 `gold-mine.png` (16.3 % coverage) — **MOSTLY LINE ART**

A wash plant / sluice: a sloped rectangular trough at the top with a
faucet / pipe entering from the upper-left, three small dark dots
inside the trough (gold-nugget / water-bead indicators), a supporting
stand with two legs and a horizontal cross-brace below, and a ground
line. The 16.3 % comes from the 3 small dark dots and the trough
edges. No solid mass. The trough interior is **white** in the source
and remains transparent in-game.

### 3.3 `silicates-mine.png` (10.1 % coverage) — **MOSTLY LINE ART**

A 3-crystal cluster: a tall central crystal (~6 × 14 px) with a
horizontal facet line at 1/3 height and an angled facet line on the
right face, a medium crystal on the right (~6 × 10 px) with two
angled facet lines, and a small crystal on the left (~4 × 8 px)
with one facet line. All edges are 2-px outlines; interior facets
are 1-px. No solid dark area. This is the **cleanest icon in the
audit set** and is a useful style reference for the v0.5.2 batch.

### 3.4 `auto-iron-mine.png` (3.8 % coverage) — **MOSTLY LINE ART**

The same dumbbell as `iron-mine.png` but reduced to ~50 % of the
canvas (occupying the lower-left quadrant) with a small satellite
(body + two solar-panel wings) and a small irregular asteroid
above-and-to-the-right of the dumbbell. The 3.8 % is the lowest in
the entire set — the icon is genuinely thin line work and will look
under-weight at 16 px. The auto-* family in general sits at 3-8 %
coverage, which is at the lower bound of the family band. See §6.3.

---

## 4. Family consistency check

### 4.1 Reference family (menu / research)

Verified against:

- `assets/textures/ui/menu/main.png` — line-art doorframe with
  outlined ramp + small filled arrow inside
- `assets/textures/ui/menu/starmap.png` — outlined orbit ring with
  two **solid filled** planet/moons and 3 small filled stars
- `assets/textures/ui/menu/construction.png` — outlined hex grid
  with **solid filled** central hex + outlined surrounding hexes
- `assets/textures/ui/research/industry.png` — **solid filled**
  factory silhouette (52 % coverage equivalent — the most extreme
  silhouette in the reference family)
- `assets/textures/ui/research/materials.png` — **solid filled**
  brick stack
- `assets/textures/ui/research/energy.png` — **solid filled**
  lightning bolt inside an outlined circle
- `assets/textures/ui/research/construction.png` — outlined crane
  with triangulated truss, ground line, hook
- `assets/textures/ui/research/biology.png` — outlined DNA
  double-helix
- `assets/textures/ui/research/propulsion.png` — mix: outlined
  rocket body + **solid filled** nose cone + outlined fins

**The reference family is 50/50 line-art and solid-silhouette.**
Solid fill is allowed and is even the dominant style in some
research icons. The "filled blobs" the user is seeing in the
construction panel are not anomalous *for the family* — they are
expected family behaviour. The issue is the **density** of three
specific icons (`data-center`, `launch-site`, `engineering-bay`),
which are denser than any reference-family icon and the result is
visually "blockier" than the rest of the construction panel.

### 4.2 Building v0.5.1 batch (5 icons) vs reference family

The 5 v0.5.1 icons (gold, silver, platinum, he3, water-processor)
are all **line-art, no chemical-symbol labels**, and sit at 7-16 %
coverage. This is the closest-to-reference match. If the user
prefers the line-art look for the whole construction panel, **this
is the style bar to hold the rest of the set to**.

### 4.3 Building v0.5.2 batch (45 icons) — internal consistency

The v0.5.2 batch has a strong, consistent composition pattern:

- **Main subject** in the left/centre 60-70 % of the canvas
- **Satellite** (small body + 2 rectangular solar-panel wings) in
  the upper-right
- **Asteroid / moon** in the right half, below the satellite
- Often a **chemical-symbol letter** (`W`, `Li`, `He3`, `Th`, `Co`,
  `Ni`, `Ti`, `RE`, `Mg`, `F`, `P`, `Au`, `Ag`, `Pt`) on the main
  subject
- Sometimes a dashed **orbit arc** connecting the satellite to the
  asteroid (`auto-platinum-mine`, `auto-nickel-mine`, `auto-cobalt-mine`,
  `auto-rare-earths-mine`)
- Sometimes small **motion lines** (`auto-methane-extractor`,
  `auto-phosphorus-mine`, `auto-he3-mine`)

This composition is internally consistent across all 45 icons,
which is a real strength. **But the composition is novel** —
the menu and research families do not use a "main subject +
satellite + asteroid" template, and **none of the reference family
uses chemical-symbol letters** as a primary subject marker. The
v0.5.2 batch has therefore drifted from the reference family in
two ways:

1. **Chemical-symbol letters as primary markers.** `tungsten-mine`
   is "cube with W"; `lithium-mine` is "battery with Li";
   `nickel-mine` is "pentagon with Ni"; `auto-cobalt-mine` is
   "atom with Co" (an atom symbol with a chemical element stamped
   on it — somewhat redundant); `auto-rare-earths-mine` is a 3×3
   grid of squares with "RE" stamped in the lower-left; etc.
   This is a stylistic decision the icon-artist made for the
   per-resource mines; it is consistent within the batch but
   **inconsistent with the reference family**, where the subject
   carries its own meaning (a microscope, a DNA helix, a crane,
   a brick wall). If the user wants strict family consistency,
   the letters should be removed and the subjects should be made
   to read on their own.

2. **Orbital motif as a categorical signal for "auto mine".** The
   "satellite + asteroid + orbit" trio is used as a visual
   differentiator between `XxxMine` and `auto-Xxx-mine`. This is
   a sensible in-batch signal but adds three extra sub-shapes
   (satellite, asteroid, orbit) to a 24-px design grid. At 16-px
   display size, the auto-* sub-shapes are likely unreadable —
   they exist as 2-4 px silhouettes and won't be recognisable.
   This is why auto-* coverage bottoms out at 2.4 %
   (`auto-magnesium-mine`) — there is barely any subject left
   to see.

### 4.4 Building original 52 — internal consistency

The original 52 are mostly line-art (mean 14 %, no chemical-symbol
letters, no satellite motif), with three outliers that predate
the v0.5.1/v0.5.2 icon work. These three outliers are also the
ones that visually dominate the construction panel because they
are 2-4× denser than the next-densest icon.

---

## 5. Redesign spec for the offenders

Three MUST-regenerate icons (§5.1-5.3) and four SHOULD-regenerate
icons for consistency (§5.4-5.7). The output is a spec for the
icon-artist, not new art.

### 5.1 `data-center.png` — **MUST regenerate**

**Current state.** 256×256, 52.9 % coverage. A thick-outlined
server-rack frame (8 px wide) with three solid dark navy horizontal
bars inside (each ~30 % of the icon area, ~6 px tall, full rack
width). The 3 solid bars represent server blades.

**Problem.** The three solid black rectangles dominate the icon;
after cyan tint they read as a "cyan brick with three slits" rather
than as a server rack. The frame and the bars are stylistically
inconsistent (the frame is line art, the bars are solid fill).

**Redesign target.** A line-art server rack with three *outlined*
server-blade slots. Specification:

- **Outer frame.** Rounded-corner rectangle, ~18 × 22 px on the
  24-px design grid, 2-px stroke, no fill. Same outer dimensions
  as the current frame.
- **Three server-blade slots.** Three horizontal rounded rectangles
  stacked vertically, each ~14 × 4 px, **outlined** (1.5-px stroke,
  no fill) with ~2 px vertical gap between them. Each slot has a
  small 1×1 px solid fill square on its left edge (LED indicator).
- **Two vertical rack-mount rails** inside the frame, ~1.5-px
  stroke, full height, ~2 px in from each side.
- **One small solid dot** (~1.5 × 1.5 px) in the upper-right corner
  of the top slot (status LED).
- **Ground line** at the bottom, 2-px stroke, ~1 px below the rack.
- **Padding.** 2 px on each side on the 24-px design grid (= 21 px
  on the 256-px source).

**Resulting coverage.** ~18-20 % (down from 52.9 %). Three outlined
slots + frame + rails + ground line.

### 5.2 `launch-site.png` — **MUST regenerate**

**Current state.** 256×256, 33.3 % coverage. Solid filled launch
tower (left, rectangular column with bracket details) and solid
filled rocket (right, body + nose cone + 2 fins + service tower
gantry on the right side). Both subjects are entirely dark navy
with no internal line detail.

**Problem.** A solid filled rocket and a solid filled tower render
as two solid cyan blocks at 16-24 px. The user cannot tell what
they are looking at without the surrounding context. The reference
family's `research/construction.png` (a crane, line-art) is the
visual bar.

**Redesign target.** A line-art launch complex with one rocket
outlined and the service tower outlined.

- **Launch pad.** Horizontal ground rectangle, ~20 × 1.5 px,
  outlined, 2-px stroke. Optional small 1×1 px solid tick marks
  every 4 px along the top edge (pad markings).
- **Service / gantry tower.** Vertical rectangle ~3 × 16 px on the
  left, outlined (2-px stroke), with three horizontal cross-braces
  inside (1-px). A small catwalk arm extending right from the top
  at ~14-px height, ~6 px long, outlined.
- **Rocket.** Vertical, ~5 × 16 px on the right of the gantry,
  outlined (2-px stroke). Components:
  - Nose cone: triangle on top, ~5 × 4 px, outlined
  - Body: rectangle below, ~5 × 10 px, outlined, with 1 horizontal
    line at the midbody (stage separation indicator)
  - Fins: two small triangles at the base, ~2 × 3 px each, outlined
  - Engine bell: trapezoid at the bottom, ~3 × 2 px, outlined
  - Optional 3 small 1-px solid fill marks on the body (windows
    or markings)
- **Flame (optional).** Below the engine bell, 3 small wavy lines,
  1-px stroke, ~3 px tall, drawn lightly.
- **No solid silhouette anywhere.** Reference is
  `research/construction.png` (a line-art crane).

**Resulting coverage.** ~14-17 % (down from 33.3 %).

### 5.3 `engineering-bay.png` — **MUST regenerate**

**Current state.** 256×256, 31.9 % coverage. A solid filled bolt
(hexagonal head + threaded shaft) in front of a solid filled gear
behind it. Both shapes are entirely dark navy with no internal
detail.

**Problem.** The solid bolt + solid gear at 16-24 px reads as a
"dense blob" — the threads and gear teeth are visible as pixel-level
anti-aliasing on the edges but the body of the shapes is
indistinguishable from a generic dark mass.

**Redesign target.** A line-art engineering icon: a gear (outline)
with a bolt (outline) crossing it diagonally.

- **Gear.** Centered, ~16 px diameter, outlined (2-px stroke). 8
  teeth, each ~2 × 2 px. Inner circle (hub) ~6 px diameter, outlined
  (1-px). Center hole ~2 px diameter, solid fill (or 1-px outline
  with transparent center — family consistency with `industry.png`
  suggests solid fill is fine here for a small element).
- **Bolt.** Crossing the gear diagonally, ~14 × 4 px. Hex head
  ~4 × 3 px on the upper-left, outlined. Threaded shaft extending
  to the lower-right, with 4-5 short 1-px thread marks across the
  shaft.
- **No solid silhouette except** the 2-px center hole of the gear
  (which is family-OK per `research/industry.png`).
- **Optional:** one small solid fill rectangle on the bolt head
  (highlight indicator), 1.5 × 1 px.

**Resulting coverage.** ~18-22 % (down from 31.9 %).

### 5.4 `iron-mine.png` — **SHOULD regenerate (semantic)**

**Current state.** 256×256, 7.0 % coverage. A dumbbell with 4
outlined weight blocks. Renders cleanly as line art but the
**subject is wrong**: a dumbbell is a weightlifting symbol, not
a mining symbol. Every other per-resource mine in the v0.5.2
batch uses a subject specific to the resource (crystal for
silicates, diamond for carbon, cube with W for tungsten, battery
for lithium, etc.). The iron-mine is the only one in the series
with a generic non-mining subject.

**Redesign target.** Replace the dumbbell with an **iron-ore
cart or iron ingot**, in the same line-art style and 24-px
design grid as the rest of the mine series:

- **Option A — Iron ingot (trapezoidal bar).** A trapezoidal
  prism ~14 × 8 px, outlined (2-px stroke), with the chemical
  label "Fe" centered on the top face in 1-px line work. This
  mirrors the visual language of `auto-gold-mine.png`
  (gold bar with "Au") and `auto-silver-mine.png` (silver bar
  with "Ag"). 5 small 1-px solid squares on the front face
  (ingot stamp marks).
- **Option B — Mine cart on rails.** A trapezoidal cart
  ~12 × 8 px with two circular wheels (~3 px diameter) below,
  all outlined. Three small solid-fill rectangles inside the
  cart (ore chunks). A short rail segment under the wheels,
  ~16 × 1 px, outlined.
- **Padding.** 2-3 px on each side. No satellite / asteroid
  motif (this is `iron-mine`, not `auto-iron-mine`).

**Resulting coverage.** ~9-12 % (slightly up from 7.0 % to
match the rest of the v0.5.2 mine band).

### 5.5 `auto-magnesium-mine.png` — **SHOULD regenerate (legibility)**

**Current state.** 256×256, **2.4 % coverage — lowest in the
entire set**. The icon is just the "Mg" text label and a single
wavy line, plus the standard satellite + asteroid. There is no
recognisable magnesium subject. At 16-px display, the text is
illegible and the icon disappears.

**Redesign target.** A subject that represents magnesium
specifically:

- **Option A — Magnesium flame.** A small candle-flame shape
  (~6 × 10 px) outlined (2-px stroke) with a wavy 1-px stroke
  inside the flame (heat shimmer). Magnesium burns with a
  bright white flame; this is the iconic visual cue.
- **Option B — Dolomite / magnesite crystal cluster.** A
  3-crystal cluster similar to `silicates-mine.png` but
  hexagonal-prism shaped, with "Mg" on the central crystal.
- Keep the satellite + asteroid + dashed orbit line consistent
  with the rest of the auto-* family.

**Resulting coverage.** ~6-9 % (up from 2.4 %).

### 5.6 `financial-center.png` — **COULD regenerate (density)**

**Current state.** 256×256, 25.7 % coverage. A Greek-temple
front: triangular pediment, 4 vertical columns, top and bottom
entablature, all outlined. The columns are drawn quite thick
(~2-3 px) which is why coverage is high. Renders correctly
but is denser than the rest of the family.

**Redesign target (only if the user wants lower density).**
Reduce column width from ~3 px to ~2 px on the design grid
(= ~21 px to ~14 px on the 256-px source), keep the pediment
and entablature as outlined. No structural change.

**Resulting coverage.** ~16-19 % (down from 25.7 %).

### 5.7 All auto-* mines — **COULD regenerate (letter density)**

**Current state.** 45 icons, all using a chemical-symbol letter
on the main subject. The letters are 1-px-stroke outlined glyphs
that work at 24 px but become illegible at 16 px.

**Redesign target (only if the user wants strict family
consistency).** Remove the chemical-symbol letter from each
auto-* mine and instead use a subject that reads as the resource
on its own (e.g. `auto-cobalt-mine` is currently "atom with Co
on it" — an atom alone is ambiguous, but a **blue crystal**
representing cobalt aluminate would be specific and would not
need a letter). This is a stylistic call. The v0.5.1 batch
demonstrates that the per-resource mine subject can carry the
meaning on its own (gold-mine = wash plant, he3-mine = factory,
platinum-mine = scaffolding). The v0.5.2 batch added the
letter as a crutch that breaks the family visual language.

**Resulting coverage.** No change.

---

## 6. Priority list

### 6.1 MUST regenerate (visual breakage in the construction panel)

| File | Opaque % | Reason | Effort |
|------|---------:|--------|--------|
| `assets/textures/ui/buildings/data-center.png` | 52.9 | 3 solid server blades read as cyan brick | Re-prompt per §5.1 |
| `assets/textures/ui/buildings/launch-site.png` | 33.3 | Solid rocket + tower read as 2 solid cyan blocks | Re-prompt per §5.2 |
| `assets/textures/ui/buildings/engineering-bay.png` | 31.9 | Solid bolt + solid gear read as dense cyan blob | Re-prompt per §5.3 |

After regeneration, re-run `python _normalize.py` and
`python _verify_tinting.py` for each. Acceptance: opaque coverage
in the 14-22 % band (the rest of the original 52 sits in this
range; the v0.5.1 + v0.5.2 batches sit at 7-17 %).

### 6.2 SHOULD regenerate (consistency with the v0.5.1 / v0.5.2 line-art family)

| File | Opaque % | Reason | Effort |
|------|---------:|--------|--------|
| `assets/textures/ui/buildings/iron-mine.png` | 7.0 | Dumbbell is not a mining symbol; replace with iron ingot or mine cart per §5.4 | Re-prompt |
| `assets/textures/ui/buildings/auto-magnesium-mine.png` | 2.4 | No recognisable subject; replace per §5.5 | Re-prompt |

The other 9 top-15 icons (`fusion-reactor`, `financial-center`,
`hydrocarbon-extractor`, `orbital-survey-station`, `dt-fusion-reactor`,
`commercial-hub`, `semiconductor-fab`, `ai-cluster`,
`ground-defense-battery`, `breeder-reactor`, `agri-dome`,
`copper-mine`) are all family-OK line art despite high coverage
— the coverage is from legitimate subject detail (radial
spokes, column bodies, panel-grid lines, atom orbits, chip pins,
neural-network nodes, triangulated geodesic dome). They are
**not** in the SHOULD list.

### 6.3 COULD regenerate (nice-to-have, no functional impact)

| Item | Reason | Effort |
|------|--------|--------|
| `assets/textures/ui/buildings/financial-center.png` | Column thickness slightly heavy; reduce from 2-3 px to 2 px on the 24-px grid (§5.6) | Edit SVG / re-prompt |
| All 28 `auto-*-mine.png` icons | Remove chemical-symbol letters and rely on the subject alone (§5.7) | Edit all 28 |
| `auto-iron-mine.png`, `auto-tungsten-mine.png` | Subject is reduced to ~50 % of canvas and barely visible at 16 px; consider increasing subject size to 60-65 % of canvas | Re-prompt |

### 6.4 Should NOT regenerate

The 4 named audit targets — `iron-mine.png`, `gold-mine.png`,
`silicates-mine.png`, `auto-iron-mine.png` — and the 7 v0.5.2
spot-checks — `tungsten-mine.png`, `lithium-mine.png`,
`auto-tungsten-mine.png`, `auto-lithium-mine.png`, `he3-mine.png`,
`water-processor.png`, `auto-he3-mine.png` — all render as clean
line art. No regeneration needed for "filled-blob" reasons.

The remaining 39 of the v0.5.2 batch (not individually sampled
in this audit) should be spot-checked at 16-px display size
against the family; if any have coverage > 17 % or have a
non-mining subject, regenerate them with the same recipe as the
v0.5.1 batch (subject-only, no chemical-symbol letter, no
satellite/asteroid/orbit motif unless the user wants the
auto-* differentiator).

---

## 7. Why the user perceived "filled blobs" when the source PNGs are line art

A note on the diagnosis, because the user's previous attempt to
fix this (inverting white-on-transparent → dark-on-white) was
the correct fix for a real problem that the **45 v0.5.2 icons**
had at the time, but it was not the problem the user was looking
at in the construction panel.

The `scripts/invert_icons.py` script docstring describes the
problem the v0.5.2 batch had before inverting:

> The game's tinting shader fills the white pixels with the
> building's tint color, so the new icons currently render as
> filled blobs instead of the dark-on-white outlined style of
> the older icons. Inverting to RGB dark-on-white makes the
> post-processing treat them the same way as gold-mine.png /
> he3-mine.png / etc.

That was a real problem with the v0.5.2 batch at one point and
it is now fixed — all 102 PNGs are 256×256 RGB and go through
the runtime recipe cleanly. The icons the user sees in the
construction panel that look like "filled cyan blobs" are the
**original 52 silhouette icons** (data-center, launch-site,
engineering-bay at the top of the list), not the v0.5.2 batch.
Inverting the v0.5.2 batch was necessary and correct, but the
"filled" look the user is complaining about comes from a
different set of icons (the three MUST-regenerate ones in §6.1)
that predate the v0.5.1 / v0.5.2 work and have always been solid
silhouettes.

To verify this in-game, open the construction panel and look at
the row containing **Data Center** (3 stacked cyan bars) versus
the row containing **Silicates Quarry** (3 cyan crystal
outlines). The Data Center will look filled; the Silicates
Quarry will look like line art. The two are processed by the
**same** post-process and tinted with the **same** cyan — the
difference is in the source PNG, not in the pipeline.

---

## 8. Files cited

- `src/ui/construction.rs:78-129` — building-icon post-processing
  recipe (luminance key + premultiplied white)
- `src/ui/icons.rs:54-87` — menu-icon post-processing (same recipe)
- `src/ui/icons.rs:137-185` — research-icon post-processing (same recipe)
- `src/ui/theme.rs:61` — `ACCENT = #00F2FF` (the cyan tint applied
  at draw time)
- `assets/textures/ui/buildings/README.md:19-28` — family contract
- `assets/textures/ui/buildings/README.md:73-138` — original-52
  coverage table (matches this audit's §2 measurements for the
  52 originals)
- `docs/design/BUILDING_ICONS_MAPPING.md:14-19, 22-75` — visual
  concepts and per-building mapping
- `scripts/invert_icons.py` — the v0.5.2 batch inversion
- `scratch/process_icon.py` — the synthesized-icon-to-PNG pipeline
  used to make the v0.5.2 batch in the first place

All 102 PNG files in `assets/textures/ui/buildings/*.png` were
analyzed (no new art produced).
