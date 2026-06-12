# Helios Ascension — User Interface Guide

This document is the source of truth for Helios Ascension's UI conventions:
the design tokens panels draw from, the per-panel anatomy, and the
contribution rules every PR touching `src/ui/` must follow.

For panel-specific design intent and feature-level details, see the per-domain
docs:

- Colonies and buildings → `docs/COLONIES.md`
- Research, technology tree, RON data → `docs/RESEARCH_MODDING.md`
- Shipbuilding modules, hulls, templates → `docs/SHIPBUILDING.md`
- Astronomy, bodies, star systems → `docs/ASTRONOMY.md`
- Resource catalogue and economy rules → `docs/RESOURCES.md`

## 1. Overview

Helios Ascension uses a mixed UI stack:

- **egui 0.33** for the dashboard, the resource bar, and most full-screen
  panels (Survey, Construction, Research, Economy, Fleets, Dossier).
- **Native Bevy UI 0.18** for the Shipbuilding workspace (Logistics Hub /
  Design Blueprint / Engineering Analytics) — this is the canonical
  shipbuilding UI, not an alternate.

Every panel — egui or Bevy UI — pulls its colours, fonts, and spacing from
`src/ui/theme.rs`. There is no per-panel palette. The egui visuals
(backgrounds, hover/active/focus, separators) are configured once via
`theme::apply_global_visuals(ctx)` at startup.

## 2. Design Tokens

All UI tokens live in `src/ui/theme.rs`. The only place new constants of this
shape get added. If you find yourself reaching for a literal value, reach for
a token first.

### 2.1 Spacing scale

A 4-px-based grid with five stops. Reference these via `theme::Spacing::*`
instead of inlining `f32` literals.

| Token | Value (px) | When to use |
| ----- | ---------- | ----------- |
| `Spacing::xs` | 4 | Hairline gaps and tight separators. |
| `Spacing::sm` | 8 | Default intra-row gap; the most common value. |
| `Spacing::md` | 10 | Panel inner padding, section breathing room. |
| `Spacing::lg` | 12 | Sub-section separation, generous tooltip padding. |
| `Spacing::xl` | 16 | Top-level panel separation. |

### 2.2 Colour palette

Tactical-OS dark navy. Every constant below is a `pub const` in `theme.rs`.

#### Core backgrounds

| Token | RGB | Use |
| ----- | --- | --- |
| `BG` | `(8, 13, 26, 244)` | Panel fills (slight translucency). |
| `BG_SOLID` | `(8, 13, 26)` | Fully opaque background for `CentralPanel` and `Visuals`. |
| `SURFACE` | `(13, 17, 23)` | Cards, tiles, sub-sections. |
| `SURFACE_RAISED` | `(20, 26, 36)` | Hovered / raised widgets. |
| `SURFACE_INPUT` | `(16, 20, 30)` | Bright widget / input background tint. |

#### Accents and borders

| Token | RGB | Use |
| ----- | --- | --- |
| `ACCENT` | `(0, 242, 255)` | Primary cyan for highlights, selection, interactive elements. |
| `ACCENT_DIM` | `(0, 242, 255, 80)` | Secondary outlines and inactive glyphs. |
| `BORDER` | `(0, 242, 255, 40)` | Faint accent for borders and grid lines. |
| `BORDER_DIM` | `(40, 45, 55)` | Inset dividers slightly darker than `BORDER`. |

#### Semantic colours

| Token | RGB | Use |
| ----- | --- | --- |
| `GREEN` | `(39, 174, 96)` | Positive / success — good status, income, production. |
| `AMBER` | `(230, 170, 50)` | Warning / amber — caution states, moderate thresholds. |
| `GOLD` | `(255, 215, 0)` | Treasury / financial values. |
| `RED` | `(231, 76, 60)` | Negative / danger — errors, deficits, damage. |
| `RP_BLUE` | `(100, 200, 255)` | Research Points. |
| `EP_TEAL` | `(100, 255, 200)` | Engineering Points. |
| `STAR_GOLD` | `(255, 220, 100)` | Star names and starmap labels. |
| `GRAVITY_ASSIST` | `(180, 130, 255)` | Flyby / gravity-assist accent. |

#### Text colours

| Token | RGB | Use |
| ----- | --- | --- |
| `TEXT` | `(210, 220, 235)` | Primary text. |
| `TEXT_VALUE` | `(200, 215, 240)` | Slightly brighter foreground for data values. |
| `TEXT_DIM` | `(120, 140, 170)` | Secondary information and captions. |
| `TEXT_HINT` | `(90, 105, 130)` | Faint hint text. |
| `ICON_INACTIVE` | `(190, 205, 225)` | Inactive nav-bar icons (readable but distinct from `ACCENT`). |

#### Resource category colours

| Token | Use |
| ----- | --- |
| `CAT_VOLATILES` | Volatiles — Water, H₂, NH₃. |
| `CAT_ATMOSPHERIC` | Atmospheric gases. |
| `CAT_CONSTRUCTION` | Construction materials — Iron, Al, Ti, … |
| `CAT_FUSION` | Fusion fuel. |
| `CAT_FISSILES` | Fissile materials. |
| `CAT_PRECIOUS` | Precious metals. |
| `CAT_STRATEGIC` | Strategic materials — Copper, REE, Li, S. |
| `CAT_EXOTIC` | Exotic materials — Antimatter, Exotic Matter, Metamaterials, Computronium. |

Use `theme::category_color(name)` for the `&str → Color32` lookup.

#### Body / ocean / gas colours

The dossier, starmap, and body-tooltip code all colour celestial bodies from
the same table. Use the dispatcher helpers instead of constructing colours
yourself.

- `body_type_color(BodyType) -> Color32` — `BODY_STAR`, `BODY_TERRESTRIAL`,
  `BODY_GAS_GIANT`, `BODY_DWARF_PLANET`, `BODY_MOON`, `BODY_ASTEROID`,
  `BODY_COMET`, `BODY_RING`.
- `ocean_color(OceanType) -> Color32` — `OCEAN_WATER`, `OCEAN_METHANE`,
  `OCEAN_HYDROCARBON`, `OCEAN_AMMONIA`, `OCEAN_SUBSURFACE`.
- `gas_color(name: &str) -> Color32` — case-insensitive prefix match for N₂,
  O₂, CO₂, Ar, CH₄, H₂, He, SO₂, Ne, plus `GAS_DEFAULT` for unrecognised
  gases.

#### Status / difficulty / tier colours

- `STATUS_WARN`, `STATUS_ERROR`, `STATUS_SUCCESS`, `STATUS_SUCCESS_DIM`,
  `STATUS_NEUTRAL`, `STATUS_MUTED` — read by HUDs and chip badges.
- `DIFFICULTY_MODERATE`, `TIER_4`, `TIER_3`, `TIER_OTHER`, plus `ACCENT` for
  tier 5 — used by dossier cost-indicator chips. `tier_color(u8)` dispatches.
- `SOLAR_STAR` — the ★ glyph in dossier headers.

#### Tech tree node colours

Eight `TECH_NODE_*` fill tokens (one per `in_path × {unlocked, researching,
available, locked}` state) and two `TECH_TEXT_*` text tokens. Use
`tech_node_color(in_path, unlocked, researching, can_research)` instead of
open-coding the match.

#### Resources bar metric colours

`RB_POPULATION`, `RB_COLONIES`, `RB_SHIPS`, `RB_SURVEY`, `RB_HOUSING` —
distinguish metrics in the history-panel bar chart and the housing capacity
bar.

#### Ship-module category colours

`module_slot_accent_color(ShipModuleCategory) -> Color` paints the heavy slot
border chrome. `module_category_color(ShipModuleCategory) -> Color` paints
the lighter detail / chip variant. Both functions are exhaustive over the
`ShipModuleCategory` enum.

#### Misc

- `ANCHOR` — ⚓ glyph and similar emphasis marks.
- `BUTTON_ACTIVE_BG` — dark teal background for active / selected row
  buttons and time-scale presets; sits between `SURFACE_RAISED` and `ACCENT`
  so accent text reads cleanly.
- `FOCUS_RING` / `FOCUS_RING_WIDTH` — drawn around widgets that hold
  keyboard focus; pair with `focus_ring_stroke()` or `paint_focus_ring()`.

### 2.3 Typography

| Builder | Returns | When to use |
| ------- | ------- | ----------- |
| `theme::heading()` | `FontId` 13pt "semibold" | Section heading inside a panel. |
| `theme::title()` | `FontId` 20pt "heading" | Panel or body title. |
| `theme::mono(size)` | `FontId` monospace at `size` | Numeric data and labels. |
| `theme::body(size)` | `FontId` proportional at `size` | Long-form text. |

The font files live under `assets/fonts/`. The family names registered with
egui are `"heading"`, `"semibold"`, and the monospace / proportional families
that ship with egui.

### 2.4 RichText builders

Use these in place of `RichText::new(text).font(mono(N)).color(TEXT_DIM)`
chains — they keep the typographic scale coherent.

| Builder | Style | Use |
| ------- | ----- | --- |
| `theme::label(text)` | Mono 10pt, `TEXT_DIM` | Small uppercase stat-row labels (`DISTANCE`, `MASS`). |
| `theme::value(text)` | Mono 12pt, `TEXT_VALUE` | Stat-row values; brighter than the label. |
| `theme::caption(text)` | Body 10pt, `TEXT_HINT` | Explanatory hint under a value. |
| `theme::kbd_shortcut_label(text)` | Bold mono 10pt, `ACCENT` | Keycap chip (`F1`, `Shift+F12`, …). |

### 2.5 Standard frames and widgets

| Helper | Returns | Use |
| ------ | ------- | --- |
| `theme::panel_frame()` | `egui::Frame` | Standard dark side-panel frame. |
| `theme::central_frame()` | `egui::Frame` | Fully opaque frame for central panels. |
| `theme::section_frame()` | `egui::Frame` | Section cards inside full-screen menus. |
| `theme::elevated_frame()` | `egui::Frame` | Nested summary blocks. |
| `theme::tooltip_frame()` | `egui::Frame` | Tooltip popups. |
| `theme::divider(ui)` | `()` | Thin horizontal tactical divider. |
| `theme::stat_row(ui, label, value)` | `()` | Dim-label + value row in a grid. |
| `theme::stat_row_with_tooltip(ui, label, value, tooltip)` | `()` | Stat row with a hover tooltip on the label cell. |
| `theme::pause_button_fill(blink)` | `Color32` | Pulsed fill for the dashboard's pause button. |

### 2.6 Global visuals

`theme::apply_global_visuals(ctx)` is called once at startup. It configures
the dark `Visuals` palette (backgrounds, widget states, separators, selection)
and sets `interaction.tooltip_delay = 0.2`. Panels should not call this
themselves.

## 3. Per-Panel Anatomy

### 3.1 Main Dashboard

The dashboard is visible at the top of the screen and provides access to
all major game panels.

#### Navigation tabs

- **Survey** — Explore celestial bodies and star systems.
- **Construction** — Build and manage colony infrastructure.
- **Research** — Navigate the technology tree.
- **Economy** — Track resources and budget.
- **Fleets** — Manage spacecraft.
- **Shipbuilding** — Design ship hull/module layouts and inspect live
  engineering metrics.

#### Time controls

Located in the dashboard header:

- **Pause/Play Button** — Pause or resume simulation. The fill pulses via
  `theme::pause_button_fill(blink)`.
- **Speed Selection** — Choose simulation speed.
  - 1 hr/s (3,600× real-time)
  - 1 day/s (86,400× real-time)
  - 1 week/s (604,800× real-time)
  - 1 month/s (~2.6M× real-time)
  - 1 year/s (31.5M× real-time)
- **Date Display** — Current in-game date and time.

### 3.2 Survey Panel

View and select celestial bodies or star systems.

#### System view (zoomed in)

- **Body List** — Scrollable list of all bodies in current system.
- **Body Selection** — Click a body to select it.
- **Body Information** (right panel when selected):
  - Name and body type.
  - Physical properties (mass, radius, gravity).
  - Orbital parameters.
  - Surface conditions (temperature, atmosphere).
  - Mineral deposits (if surveyed).
  - Colony information (if colonized).
  - Population and buildings.

#### Starmap view (zoomed out, ≥ ~100 AU)

- **Star Icons** — Visual representation of nearby star systems, coloured
  via `BODY_STAR` and labelled in `STAR_GOLD`.
- **Hover Tooltips** — Star system name, distance from Sol, body count.
- **Star Selection** — Double-click a star to view detailed information.
- **Star System Panel** (right panel when selected) — System info, star
  properties (spectral type, mass, luminosity, temperature, metallicity),
  body counts by type, total surveyed resources, population statistics.

### 3.3 Construction Panel

Manage colony buildings and construction projects.

#### Sub-tab strip

The panel opens with a `theme::tab_strip<ConstructionTab>` (Pattern 4 in
[`UI_LAYOUT_PATTERNS.md` §5](UI_LAYOUT_PATTERNS.md#5-pattern-4--in-panel-sub-tab-strip-egui))
that exposes the panel's top-level sub-tabs (Overview, Buildings, Build,
Stockpiles). The Build tab nests a second
`theme::tab_strip<BuildingCategory>` strip with eight `BuildingCategory`
rows indented by `theme::Spacing::md` — see §8 for the primitive
contract. Bespoke `ConstructionTab` + `BuildFilter` state is gone; the
two strips are the single source of truth for "which sub-view is
active".

#### Features

- **Colony Selection** — Dropdown to choose which colony to manage.
- **Build Multiplier** — Queue ×1, ×5, or ×10 copies in one click.
- **Building Categories** (47 total):
  - **Infrastructure** — Housing, Habitat Dome, Underground Habitat, Life
    Support, Water Treatment, Desalination, Recycling.
  - **Industry** — Mines, Refineries, Factories, Atmospheric Processors,
    Chemical Plants, Drills, Semiconductor Fabs, Pharma Plants.
  - **Logistics** — Mass Drivers, Orbital Lifts, Cargo Terminals,
    Warehouses.
  - **Power** — Solar, Wind, Hydro, Geothermal, Coal, Gas, Fission,
    Fusion.
  - **Population** — Agri Domes, Farms, Greenhouses, Aquaculture
    Facilities, Medical Centers.
  - **Research** — Labs, Engineering Bays, AI Clusters, Data Centers.
  - **Financial** — Commercial Hubs, Financial Centers, Trade Ports.
  - **Military** — Shipyards, Missile Silos, Launch Sites, Space Ports,
    Defense Batteries.

#### Building card layout

Each building card shows (top to bottom):

1. **Icon + Name** — identifying header.
2. **Description** — what it is, one line.
3. **Separator** — drawn with `theme::divider`.
4. **Stats row** — `BP cost` | `👷 workforce` | `⚡ power demand`. Built with
   `theme::stat_row` so the typographic scale matches other panels.
5. **Build time** — estimated years or months based on current Factory BP
   output.
6. **▸ Effect lines** (green) — the actual numeric impact per building,
   e.g. `+25M housing capacity`, `+1,000 Mt/yr food (feeds ~10M ppl)`,
   `+20 GW power output`, `+15% mining efficiency`.
7. **Resource costs** — 2 per row, coloured green (`GREEN`) when affordable
   or red (`RED`) when insufficient.
8. **Queue button** — disabled in red if resources are insufficient.

#### Construction queue

- Appears at the bottom of the panel.
- Shows active projects with a progress bar and estimated completion.
- Cancel any project to refund queued resources.

#### Construction debug controls (F12)

- **Free Construction** — Build without resource costs.
- **Instant Build** — Complete construction immediately.
- **Bypass Tech** — Show and queue all buildings regardless of tech
  prerequisites.

> For a complete building reference with per-building outputs, capacities,
> and tech requirements see `docs/COLONIES.md`.

#### Construction Overview

The Overview sub-tab renders its top-level summary as a
`theme::ledger_panel` (see §8.2) so the section structure matches the
planet dossier's right-side ledger.

### 3.4 Research Panel

Browse and select technologies to research.

#### Archive sub-tab categories

The Archive tab groups technologies by `TechCategory` using a
`theme::tab_strip<TechCategory>` (Pattern 4 in
[`UI_LAYOUT_PATTERNS.md` §5](UI_LAYOUT_PATTERNS.md#5-pattern-4--in-panel-sub-tab-strip-egui)).
`TechCategory` implements `Tab` in `src/ui/tab.rs` — `id()` is the
snake_case variant name, `label()` reuses `TechCategory::display_name`,
`icon()` reuses `TechCategory::icon` so the strip and the inline labels
stay in sync. The strip replaces the bespoke loop that used to live at
`src/ui/research_panel.rs:1568-1574`. See §8.2 for the primitive
contract.

#### Technology tree

- **15 Categories** — Electronics, Military, Space Technology, Biology,
  Physics, Energy, Sociology, Construction, Propulsion, Materials, Sensors,
  Weapons, Defensive Systems, Life Support, Industry. Each category has a
  per-colour band on the tree (`tech_category_color(TechCategory)`).
- **Tech Cards** — Show technology name, description, cost (RP), and
  prerequisites.
- **Progress Tracking** — View research progress on active projects.
- **Tech Status** — Visual indicators for:
  - **Available** — all prerequisites met.
  - **Locked** — missing prerequisites.
  - **Researched** — already completed.
  - **Active** — currently being researched.

The node fill colours come from `tech_node_color(...)`; in-path variants are
the brighter of the two per state.

#### Technology information

- **Research Cost** — Amount of Research Points (`RP_BLUE`) required.
- **Prerequisites** — Technologies that must be completed first.
- **Unlocks** — Buildings, components, or capabilities unlocked.
- **Modifiers** — Bonuses provided (cost reductions, productivity
  increases).

#### Research debug controls (F12)

- **Instant Research** — Complete current research immediately.
- **Free Research** — Unlock all technologies.

### 3.5 Economy Panel

Track resources, production, and budget.

#### Sub-tab strip

The panel opens with a 7-way `theme::tab_strip<EconomyTab>` (Pattern 4
in [`UI_LAYOUT_PATTERNS.md` §5](UI_LAYOUT_PATTERNS.md#5-pattern-4--in-panel-sub-tab-strip-egui))
that exposes the panel's sub-tabs. The Colonies tab is a
`theme::ledger_panel<ColonyRow>` (Pattern 2 ledger) — see §8.2 for the
primitive contract and
[`UI_LAYOUT_PATTERNS.md` §3](UI_LAYOUT_PATTERNS.md#3-pattern-2--right-side-ledger-egui)
for the full Pattern 2 layout contract.

#### Resource overview

- **Stockpiles** — Current amount of each resource. Tinted by
  `category_color`.
- **Production Rate** — Resources generated per year.
- **Consumption Rate** — Resources used per year.
- **Net Rate** — Net production / consumption (`GREEN` positive, `RED`
  negative).

#### Resource types (31 total)

- **Volatiles** — Water, Hydrogen, Ammonia, Methane, Phosphorus.
- **Atmospheric Gases** — Nitrogen, Oxygen, Carbon Dioxide, Argon.
- **Construction Materials** — Iron, Aluminum, Titanium, Silicates,
  Nickel, Tungsten, Carbon.
- **Fusion Fuel** — Helium-3, Deuterium.
- **Fissiles** — Uranium, Thorium.
- **Precious Metals** — Gold, Silver, Platinum.
- **Strategic Materials** — Copper, Rare Earths, Lithium, Sulfur.
- **Exotic Materials** — Antimatter, Exotic Matter, Metamaterials,
  Computronium.

#### Budget information

- **Treasury** — Current monetary credits (MC) — drawn in `GOLD`.
- **Income** — Credits earned per year.
- **Expenses** — Credits spent per year (building maintenance, operations).
- **Net Income** — Overall financial balance.

#### Energy grid

- **Power Generation** — Total power produced by power plants.
- **Power Consumption** — Total power used by buildings.
- **Grid Status** — Surplus or deficit.

### 3.6 Shipbuilding Panel

Native Bevy UI workspace. There is no egui fallback — it is the only path
for designing hull layouts, picking modules into slots, queueing ships, and
inspecting live engineering metrics. See §8.3 for the Bevy UI
`theme::Color` mirror and the two Bevy primitives; the full Pattern 3
contract lives in
[`UI_LAYOUT_PATTERNS.md` §4](UI_LAYOUT_PATTERNS.md#4-pattern-3--tabbed-workspace-bevy-ui-018).

#### Tab strip

`populate_tab_strip` (`src/ui/shipbuilding_workspace.rs:1241-1296`)
uses `theme::tab_strip_bevy<ShipbuildingTab>` for the active/inactive
tab palette. The previous hardcoded `Color::srgb(0.0, 0.95, 1.0)`
literals (six sites in `populate_tab_strip`) were replaced with
`theme::Color::TAB_ACTIVE_BG` / `TAB_ACTIVE_BORDER` / `TAB_INACTIVE_BG`
/ `TAB_INACTIVE_BORDER` / `TAB_ACTIVE_TEXT` / `TAB_INACTIVE_TEXT` in
PR-D / GRA-69; the `theme::Color` mirror itself landed in PR-B /
GRA-67.

#### Workspace layout

Three panes (per the 3-pane shell parameterised in PR-E / GRA-70):

1. **Logistics Hub** — Hull selector, slot-category navigation, and the
   compatible module list for the focused slot. Slot borders are coloured
   via `module_slot_accent_color`. Pane header rendered with
   `theme::section_h1_bevy`.
2. **Design Blueprint** — Slot cards arranged on a schematic canvas, with
   hover and selection states for slot inspection. Click a slot to focus
   it, then click a module card to install it. Pane header rendered with
   `theme::section_h1_bevy`.
3. **Engineering Analytics** — Gauge-style metrics for delta-v, thrust,
   mass, acceleration, power, thermal capacity, sensor range, build
   points, fuel, and cargo, plus supplemental chip metrics for crew,
   docking, ISRU, generation/load, and ordnance. Pane header rendered with
   `theme::section_h1_bevy`.

#### Known constraints

- Blueprint placement is still partly heuristic on hulls whose slots in
  `assets/data/ship_hulls.ron` do not yet have authored `position` values.
  Hulls without authored positions fall back to a deterministic layout by
  slot index.

### 3.7 Resources Bar

The persistent horizontal bar at the top of the screen summarises live
status for the most-watched metrics.

- **Population** (`RB_POPULATION`) — current population vs housing
  capacity. Housing full-fill bar uses `RB_HOUSING`; over-budget states
  use `RED` / `AMBER`.
- **Colonies** (`RB_COLONIES`) — current colony count, drawn in warm gold.
- **Ships** (`RB_SHIPS`) — ship count, drawn in pale blue.
- **Survey coverage** (`RB_SURVEY`) — surveyed body fraction, drawn in pale
  teal.
- **History chart** — last-hour population / power-produced time series;
  legend swatches are the `RB_*` colours.

### 3.8 Tooltips & Interaction

#### Body tooltips (cyan border)

Hover over celestial bodies in system view to see:

- Body name and type.
- Distance from parent body.
- Key properties.

#### Star tooltips (orange border)

Hover over star icons in starmap view to see:

- Star system name.
- Distance from Sol.
- Body count.

#### Selection

- **Single Click** — Select bodies in system view.
- **Double Click** — Select star systems in starmap view.
- **Right Panel** — Detailed information appears for selected object.

#### Focus rings & keyboard nav

egui 0.33 does not draw an automatic focus ring on most widgets. Panels that
want a visible keyboard-focus indicator use `theme::paint_focus_ring(painter,
rect, focused)` (or `focus_ring_stroke()`) — the same amber-cyan tone reads
as "active, not pressed" against the dark `SURFACE` / `SURFACE_RAISED`
widget backgrounds. Hover / active states are handled by the global egui
visuals; focus is the only thing panels draw manually.

## 4. Screenshot Gallery

This section is the visual baseline for the post-harmonization UI. Captures
land in `docs/UI/baselines/manual/{slot}.png` when the operator runs the
game locally and hits `Shift+F12` (see `src/ui/screenshot.rs`). The slot
name is selected from a 5-slot rotating list — see
[`docs/UI/baselines/manual/README.md`](baselines/manual/README.md) for
the slot list and the capture workflow. The operator sign-off pass
(GRA-59) diffs these against the pre-PR-0 baseline.

> The screenshot pipeline currently requires a human operator with a
> working `cargo run` session — headless capture is on the backlog
> (GRA-53b). Until those runs happen, link pre-PR-0 baselines from the
> GRA-52 issue thread.

| Panel | Pre-PR-0 | Post-PR-5 |
| ----- | -------- | --------- |
| Main menu | GRA-52 attachment | _pending capture_ |
| Ship design | GRA-52 attachment | _pending capture_ |
| Component database | GRA-52 attachment | _pending capture_ |
| Logistics | GRA-52 attachment | _pending capture_ |
| Mining | GRA-52 attachment | _pending capture_ |
| Resources | GRA-52 attachment | _pending capture_ |
| Tech tree | GRA-52 attachment | _pending capture_ |
| Construction (×2) | GRA-52 attachment | _pending capture_ |
| Stockpiles | GRA-52 attachment | _pending capture_ |
| Jupiter selected | GRA-52 attachment | _pending capture_ |
| Jupiter resources | GRA-52 attachment | _pending capture_ |
| Starmap with SOL resources | GRA-52 attachment | _pending capture_ |
| Resource tooltip | GRA-52 attachment | _pending capture_ |
| Power tooltip | GRA-52 attachment | _pending capture_ |
| Statistics | GRA-52 attachment | _pending capture_ |

## 5. Contribution Rules

The CI lint (`scripts/audit_color32_literals.py --strict --baseline …`)
enforces two rules automatically:

1. **No hardcoded `Color32::from_*` literals outside `src/ui/theme.rs`.**
   Every raw colour — `from_rgb`, `from_rgba_premultiplied`,
   `from_rgba_unmultiplied`, `from_gray`, `from_hex`, … — must be a named
   constant in `theme.rs` (or constructed from a `theme::*` function). The
   CI job `ui-lint` fails the build on a new violation.

2. **The baseline file is the cleanup queue.** Existing violations are
   listed in `scripts/audit_color32_literals_baseline.txt`. Promote one to
   a `theme.rs` constant in a follow-up PR and remove the matching line
   from the baseline. Run `python3 scripts/audit_color32_literals.py
   --emit-baseline > scripts/audit_color32_literals_baseline.txt` if you
   move several at once.

Beyond the lint:

- **Prefer `theme::*` builders** for fonts, text colours, frames, and
  `stat_row` over hand-rolled `RichText` / `Frame` literals. They keep the
  typographic and spacing scale coherent.
- **Use `theme::Spacing::*`** instead of inlining `f32` gaps. The 4-px grid
  only stays coherent when every panel agrees on the stops.
- **If a token is missing, add it to `theme.rs`.** New colour, new frame,
  new builder — add it next to its kin and reference it. Don't inline.
- **The only Bevy-UI panel is Shipbuilding.** New panels are egui unless
  there's a documented reason to deviate.
- **Render-side work goes in `EguiPrimaryContextPass`.** egui context
  mutations must run on the egui primary pass — keep the `Update` schedule
  free of egui context writes.

## 6. Camera & Shortcuts

### 6.1 Camera controls

| Input | Action |
| ----- | ------ |
| **W / A / S / D** | Forward / left / backward / right. |
| **Q / E** | Down / up. |
| **Right-Click + Drag** | Rotate camera. |
| **Mouse Wheel** | Zoom in / out. |
| **Automatic View Switching** | System ↔ Starmap transition at ~100 AU. |

### 6.2 Keyboard shortcuts

| Key | Action |
| --- | ------ |
| **F12** | Toggle debug settings (when in Construction or Research panels). |
| **Space** | Pause / resume simulation. |
| **ESC** | Close current panel or deselect. |
| **Shift+F12** | Capture a screenshot into the next slot (`overview` → `shipbuilding` → `research` → `construction` → `starmap`, then wrap; manual-only). |

## 7. Troubleshooting

### 7.1 UI not responding

- Check if time is paused.
- Ensure you've selected the correct colony / body.
- Try clicking away and reselecting.

### 7.2 Missing information

- Some data requires specific technologies to be researched.
- Mineral deposits require survey operations.
- Resource information needs time to update after changes.

### 7.3 Performance issues

- Close unused panels.
- Zoom closer when not using starmap.
- Lower time acceleration if simulation is slow.

### 7.4 CI fails the UI token lint

- Run `python3 scripts/audit_color32_literals.py` locally to see what
  regressed. New violations show with `[NEW]`.
- Either move the literal to a `theme.rs` constant, or (only for true
  one-off values) add it to the baseline file with a one-line justification
  in the PR description.
- Re-run `python3 scripts/audit_color32_literals.py --strict --baseline
  scripts/audit_color32_literals_baseline.txt` before pushing.

## 8. Layout Patterns

§2 covers *components* (frames, tokens, builders) and §3 covers *panels*
(per-domain anatomy). This section covers the *layout primitives* the
panels compose from — the shared skeletons that re-appear across
Construction, Research, Shipbuilding, and Economy. The full layout
contract (skeletons, per-pattern do/don't, panel-to-pattern mapping,
contribution rules) lives in
[`docs/UI_LAYOUT_PATTERNS.md`](UI_LAYOUT_PATTERNS.md); this section is
the at-a-glance index for which primitive a panel calls where.

### 8.1 The `Tab` trait

`src/ui/tab.rs` defines the `Tab` trait (`id()`, `label()`,
`icon() -> Option<&'static str>`, `Default` is the canonical first tab).
Every panel's `*Tab` enum (`ConstructionTab`, `EconomyTab`, …) implements
it once; both the egui and the Bevy UI sub-tab primitives are generic
over `T: Tab`. The trait is the contract — see the doc comment for
extension points (dynamic `Cow::Owned` labels, per-tab icons).

### 8.2 egui primitives (Pattern 2 + Pattern 4)

| Primitive | Purpose | Used by |
| --------- | ------- | ------- |
| `theme::section_h1(ui, label)` | Panel title (20pt `title()` font, `ACCENT`). | Pattern 2 ledgers (planet dossier). |
| `theme::section_h2(ui, label)` | Section header (13pt `heading()` font, `ACCENT`). | Top of every collapsible section in a Pattern 2 ledger. |
| `theme::section_h3(ui, label)` | Sub-section header (10pt mono `TEXT_DIM`). | Nested inside a `section_h2` body. |
| `theme::tab_strip<T: Tab>(ui, tabs, active, on_select) -> T` | Horizontal sub-tab strip. Active tab in `ACCENT` + 2px bottom underline; inactive in `TEXT`. Returns `active` so callers that want the click to take effect within the same frame can re-assign their state. | Construction (top-level + 8-way `BuildingCategory` second-level), Research (Archive tab categories), Economy (7-way `EconomyTab`). |
| `theme::ledger_panel<T>(ui, id, title, _token, contents)` | Collapsible ledger section: `section_h2` title + `CollapsingHeader` (default-open). Generic `T` is reserved for future typed tokens; pass `&()` if you don't need a filter. | Economy Colonies tab, Construction Overview. |

### 8.3 Bevy UI 0.18 primitives (Pattern 3 mirror)

The Shipbuilding workspace is the only native Bevy UI panel
(`docs/UI.md` §1). PR-B added a `theme::Color` mirror of the egui
tokens and two Bevy-side primitives; PR-D replaced the previous
`Color::srgb(0.0, 0.95, 1.0)` literals at
`src/ui/shipbuilding_workspace.rs:1241-1296` with `theme::Color`, and
PR-E consolidated the 3-pane shell.

| Primitive | Purpose |
| --------- | ------- |
| `theme::section_h1_bevy(commands, parent_entity, label)` | Spawn a `Text` child of `parent_entity` styled as the canonical pane title (15pt body font, `Color::PANEL_TITLE`). |
| `theme::tab_strip_bevy<T: Tab>(commands, tabs_root, tabs, active)` | Spawn one `Button` per tab as a child of `tabs_root`, with the standard `Color::TAB_*` palette. |

### 8.4 Per-panel use

The §3 anatomy subsections reference these primitives inline at the
call site they appear in:

- **Construction** (§3.3) — `theme::tab_strip<ConstructionTab>` (top-level
  4-way) + nested `theme::tab_strip<BuildingCategory>` (8-way
  `BuildingCategory` second-level strip), `theme::ledger_panel` in
  Overview.
- **Research** (§3.4) — `theme::tab_strip<TechCategory>` in the Archive
  tab category grouping (replaces the bespoke loop in
  `src/ui/research_panel.rs:1568-1574`).
- **Economy** (§3.5) — `theme::tab_strip<EconomyTab>` (7-way) +
  `theme::ledger_panel<ColonyRow>` in the Colonies tab.
- **Shipbuilding** (§3.6) — `theme::tab_strip_bevy` in
  `populate_tab_strip` (Pattern 3 mirror), `theme::section_h1_bevy` for
  each sub-pane header, `theme::Color::TAB_*` for the active/inactive
  palette.

For the full contract — skeletons, "what *not* to do", composition
rules, contribution checklist for new panels — see
[`docs/UI_LAYOUT_PATTERNS.md`](UI_LAYOUT_PATTERNS.md) §1–§8.
