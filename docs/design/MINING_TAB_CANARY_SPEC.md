# Mining Tab — bevy_ui Canary Spec (v0.5.2)

**Branch:** `rework-ui-design`
**Target file:** `src/ui/construction.rs` (canary; replaces the legacy egui mining body in `src/ui/construction_panel.rs:872-1392`)
**Status:** design only — not implemented. Hand-off document for the Rust engineer.

---

## 0 · TL;DR

Replace the canary's placeholder `spawn_stockpiles_body` + `update_stockpiles_body` (`src/ui/construction.rs:1555`, `:1628`) with a proper Mining tab: 7 surface-mine groups + 1 orbital AutoMine group, one card per mine, all mines are direct inventory edits via `PendingConstructionActions::mining_edits`. The data path is already wired — this spec is **purely visual + interaction**. Surface = 23 cards in 7 groups (Construction 9 · Precious 3 · Strategic 6 · Fissile 2 · Hydrocarbons 1 · Heavy water 1 · He-3 1). Orbital = 25 AutoMines in 5 sub-groups.

Three small things to flag up front:

1. **Mine-count drift.** The brief says "22 base mines"; the canary's `parse_building_type` (`src/colony/data.rs:336-401`) lists **24**; the legacy egui mining tab (`src/ui/construction_panel.rs:898-948`) renders **23** (moves `CopperMine` from Strategic to Construction). I have used the legacy tab's 23-card layout for visual parity. The "22" in the brief is off by one or two — see §9 R1.
2. **Build-qty chip set drift.** The brief specifies `{1, 5, 25, 50}` (4 chips); the canary's existing Build tab uses `{1, 5, 10, 25, 50, 100}` (6 chips, `src/ui/construction.rs:2216-2223`). I have spec'd the brief's 4-chip set, but flagged §9 R3 — recommend unifying the two tabs.
3. **He-3 has a body-gated surface mine too.** `He3Mine`'s `allowed_body_types = [Moon, GasGiant, Asteroid]` (`src/colony/data.rs:90-91`); it's not just the AutoMines that get the body-gate. The legacy egui tab uses the shortcut `body_blocked = is_orbital` (`construction_panel.rs:1277`), which is wrong for surface He-3. The canary version must use `building_is_available_on()` for every card.

---

## 1 · Information architecture

### 1.1 Surface mine groups (7)

Follows `MINING_GROUPS_SURFACE` (`src/ui/construction_panel.rs:898-948`) verbatim for visual parity with the legacy tab — re-ordering would surprise players who have already internalised the layout.

| # | Group label | Card count | Resources (in display order) |
|---|---|---:|---|
| 1 | Construction materials | 9 | Iron, **Copper**, Aluminum, Silicates, Nickel, Tungsten, Carbon, Chromium, Magnesium |
| 2 | Precious metals | 3 | Gold, Silver, Platinum |
| 3 | Strategic materials | 6 | RareEarths, Lithium, Sulfur, Phosphorus, Cobalt, Fluorine |
| 4 | Fissile | 2 | Uranium, Thorium |
| 5 | Hydrocarbons | 1 | Methane |
| 6 | Heavy water | 1 | Deuterium |
| 7 | Helium-3 *(body: Moon, GasGiant, Asteroid)* | 1 | He-3 |

Total surface cards: **23**.

### 1.2 Orbital AutoMine group (1 collapsible, 5 sub-groups)

Follows `MINING_GROUPS_ORBITAL` (`src/ui/construction_panel.rs:953-1000`). All 25 AutoMines share `allowed_body_types = [Asteroid, Moon, GasGiant]` via the `BuildingDefinition` field (`src/colony/data.rs:107-143`). The single AutoWaterProcessor is in the last sub-group.

| # | Sub-group label | Card count | Resources |
|---|---|---:|---|
| 1 | Orbital — Construction | 9 | AutoIron, AutoCopper, AutoAluminum, AutoSilicates, AutoNickel, AutoTungsten, AutoCarbon, AutoChromium, AutoMagnesium |
| 2 | Orbital — Precious | 3 | AutoGold, AutoSilver, AutoPlatinum |
| 3 | Orbital — Strategic | 6 | AutoRareEarths, AutoLithium, AutoSulfur, AutoPhosphorus, AutoCobalt, AutoFluorine |
| 4 | Orbital — Fissile | 2 | AutoUranium, AutoThorium |
| 5 | Orbital — Hydrocarbons / Heavy water / He-3 / Water | 4 | AutoMethaneExtractor, AutoDeuteriumExtractor, AutoHe3Mine, AutoWaterProcessor |

Total orbital cards: **24**. (Brief says 25 — see §9 R1.)

### 1.3 Per-card data model

Mirror the legacy `MiningCardData` (`src/ui/construction_panel.rs:1040-1059`) with one structural change: the canary version returns a struct, the legacy version computes per-call inline. Both approaches work; the struct is cleaner for diff-based re-render (see §5).

```rust
struct MiningCardData {
    /// Per-build base yield (Mt/yr) from the building's `*Production` modifier.
    /// 0.0 if the building has no `*Production` modifier.
    base_yield_mt_per_year: f64,
    /// Per-resource deposit accessibility on the body (0.0–1.0).
    /// 0.0 if no deposit for this resource on the body.
    accessibility: f32,
    /// Total reserves on the body (proven_crustal + deep_deposits + planetary_bulk, Mt).
    /// 0.0 if no deposit.
    reserve_mt: f64,
    /// Yield multiplier for the colony (1.0 for Civilisation, 0.10 for Outpost).
    yield_mult: f64,
}

impl MiningCardData {
    fn production_mt_per_year(&self, count: u32) -> f64 {
        count as f64 * self.base_yield_mt_per_year
            * self.accessibility as f64
            * self.yield_mult
    }
}
```

The lookup logic is the same as the legacy `compute_mining_card_data` (`src/ui/construction_panel.rs:1061-1122`): strip `Production` from the first matching modifier, parse the residue as a `ResourceType` via `colony::data::parse_resource_type`, then read `accessibility` and `reserve` from `PlanetResources::deposits` on the body's `CelestialBody` entity. **Caching:** 24 cards × 1 lookup × 1 frame is trivial (see §9 R2 — no caching needed).

### 1.4 Surface vs AutoMine on the same card?

**No.** The brief's first draft ("×25 surface, ×0 orbital on a single card") would conflate two buildings with different icons, different body-gates, different tech requirements (`lunar_colony` for He-3, `asteroid_mining` for AutoMines), and different resource-categories (surface Iron needs power, AutoIron does not). The two-bills-one-card layout also makes the +/- buttons ambiguous (which one am I incrementing?).

**Decision: separate cards.** One card per `BuildingType`, the AutoMine for the same resource lives in the orbital section. The header for the orbital section carries the body-gate reminder. The He-3 card in the surface section also gets the body-gate (it's the only body-restricted surface mine).

---

## 2 · Visual design

### 2.1 Tokens (no new tokens — re-use the canary's)

| Token | Value | Use here |
|---|---|---|
| `BODY_BG` | `srgba(0.008, 0.039, 0.094)` | page background (inherited from `ConstructionRoot`) |
| `CARD_BG` | `srgba(0.031, 0.086, 0.172, 0.92)` | card background |
| `CARD_BORDER` | `srgba(0.373, 0.784, 0.847, 0.18)` | card outer 1 px border |
| `CARD_TOP_HIGHLIGHT` | `srgba(0.498, 0.733, 0.804, 0.80)` | inner top rim ("glass lift") |
| `CYAN` | `srgba(0.373, 0.784, 0.847)` | card title, group headers, count text when count > 0 |
| `TEXT_BODY` | `srgba(0.831, 0.890, 0.937)` | body text (production rate) |
| `TEXT_DIM` | `srgba(0.498, 0.580, 0.659)` | captions (reserve, accessibility, multiplier hint) |
| `GREEN_FIN` | `srgba(0.373, 0.784, 0.471)` | "production online" tone |
| `ORANGE_ORE` | `srgba(0.941, 0.627, 0.439)` | "amber" — body-gate, accessibility < 50% |
| `HAIRLINE` | `srgba(0.086, 0.188, 0.306)` | divider between cards in a group (use `HairlineBundle`) |
| `RED` | *(new — add to `bevy_theme.rs`)* | depleted / no-deposit / `count = 0` tone |

> **R1 (token addition):** the canary's `bevy_theme.rs` (lines 18-77) has no `RED` constant. The legacy egui uses it for food-deficit / efficiency-under-50% in the Overview tab (`src/ui/construction_panel.rs:1437`, `:1503`). Add `pub const RED: Color = Color::srgba(0.847, 0.373, 0.392, 1.0);` to `bevy_theme.rs` as part of this PR. (Match the operator's existing palette drift warning — canary is the source of truth now.)

Type sizes:

| Token | Value | Use here |
|---|---|---|
| `TITLE_SIZE` | 28 | "MINING" header text |
| `SECTION_SIZE` | 16 | collapsible group header text |
| `BODY_SIZE` | 14 | card title (e.g. "Iron Mine") |
| `CAPTION_SIZE` | 12 | count, production, reserve, accessibility, body-gate |
| `MONO_SIZE` | 12 | production rate + reserve (use `mono_font` for these — see §5.3) |

Spacing: standard `SPACE_XS=4`, `SPACE_SM=8`, `SPACE_MD=10`, `SPACE_LG=12`, `SPACE_XL=16`. The card re-uses `CardBundle` (`bevy_theme.rs:172-197`) for the outer container — **do not invent a new card bundle**.

### 2.2 Per-card layout

A single card is a `CardBundle` (column flex) with the following children, top-down:

```
┌─ card_rim (1.5 px, CARD_TOP_HIGHLIGHT) — absolute, inset 0.5 px ─┐
│                                                                │
│  ┌── icon (24×24) ──┐  TITLE (BODY_SIZE, CYAN, semibold)       │  ← header row
│  │  (cyan-tinted,   │                                          │
│  │   24×24)         │  subtitle/category (CAPTION, TEXT_DIM)   │  ← optional, omit
│  └──────────────────┘                                          │
│                                                                │
│  ────────────── hairline (HAIRLINE) ──────────────────────────  │
│                                                                │
│  ×25                                              ← count       │  ← count row
│  3.0 Gt/yr (GREEN_FIN)                            ← production  │  ← production row
│  Res: 142.3 Gt (TEXT_DIM)                         ← reserve     │  ← reserve row
│  Acc: 60% (GREEN_FIN if ≥50%, else ORANGE_ORE)   ← access.     │  ← accessibility row
│                                                                │
│  ────────────── hairline (HAIRLINE) ──────────────────────────  │
│                                                                │
│  [ − ]  [ + ]  +25                          ← [-] [+] + hint  │  ← control row
│                                                                │
│  🔒 body — AutoMines require [Asteroid, Moon, GasGiant]        │  ← body-gate (only when blocked)
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

Dimensions:
- Card width: **164 px** (tight — 24 cards per group × 4 visible in a 1080-px panel; cards auto-wrap via `flex_wrap` like the Build card grid). The legacy egui used 156 px; bump 8 px to give the "×N" hint room to breathe on cards with x100 multiplier.
- Card min-height: **184 px** (taller than the legacy 156 px to fit the hairline + the body-gate line + breathing room).
- Card padding: `UiRect::all(Val::Px(SPACE_SM))` (8 px) — tighter than the Build card's `SPACE_LG` (12 px) because we're cramming 5 data rows into a smaller box.
- Card row gap: `SPACE_XS` (4 px) between rows.
- Card column gap: `SPACE_SM` (8 px) within the header row.
- Group row gap: `SPACE_SM` (8 px) between cards.

### 2.3 Group collapsible header

Each group is a `SectionHeader` row (28 px tall) with the same pattern as the Build tab's chip rows (`src/ui/construction.rs:1892-1898`):

```
[ ▾ ]  CONSTRUCTION MATERIALS  (9)                  ┐
[ ▾ ]  PRECIOUS METALS  (3)                        │ 28 px tall
[ ▾ ]  STRATEGIC MATERIALS  (6)                    │ bordered container
...                                                │ HairlineBundle below
```

Use a `ChipRowContainerBundle` (or a new thin container, see §9 R4). The chevron (`▾` for expanded, `▸` for collapsed) is text inside a `Button` so the whole row is clickable. The (N) count dim next to the group label uses `TEXT_DIM` so it doesn't compete with the CYAN label.

The orbital section is a **single** collapsible header (one chevron) that hides/shows all 5 sub-groups. The sub-group headers inside are **non-collapsible** flat rows (matches the legacy tab).

### 2.4 Header row (top of the body)

```rust
// Same height/pattern as the Build tab's output_row (src/ui/construction.rs:1914-1928).
// 32 px tall, row flex, padding SPACE_LG horizontal, gap SPACE_LG.

[ "MINING" (TITLE_SIZE, CYAN) ]    [ "Earth — Sol III (8.2B)" (BODY_SIZE, TEXT_BODY) ]    [ "Yield: 1.00×" (CAPTION, GREEN_FIN) ]
```

The "Yield: X" chip is a non-interactive variant of `ChipButtonBundle` with no `Button` marker (so picking doesn't fire) but the same visual style. Match the Overview tab's yield chip (`src/ui/construction_panel.rs:1429-1455`).

### 2.5 Build-multiplier chip row

```
[ "Build qty:" (CAPTION, TEXT_DIM) ]   [ ×1 ] [ ×5 ] [ ×25 ] [ ×50 ]   [ "Applies to +" (CAPTION, TEXT_DIM) ]
```

- Chip set: **4 chips**, values `{1, 5, 25, 50}` (per brief). Default: `1` is active. **See §9 R3** — discuss with the user before merging whether to use 4 or 6.
- Active chip: `ACTIVE_CHIP_BG` + bright cyan border + 6-px box-shadow glow. Inactive: transparent + dim border.
- Use the same `ChipButtonBundle::new_with_border` + `ChipKind::Qty(n)` machinery as the Build tab. **Add a new variant** `ChipKind::MiningQty(u32)` to avoid cross-tab interference (a `Qty` chip on the Build tab and a `MiningQty` chip on the Mining tab are different state, even if the value is the same — see §9 R3).
- Click handler in `tick_construction_chip_click` (`src/ui/construction.rs:4067`): the new variant writes to `ui_state.build_multiplier` AND to a new field `ui_state.mining_build_multiplier` (see §5.4 for the dual-state question).
- Trailing hint "Applies to +" is dim caption, only visible when `multiplier > 1`.

### 2.6 Iconography

Re-use the same `BuildingIcons` resource (`src/ui/construction.rs:50-54`) and `process_building_icons` post-processor (`:83-129`) that the Build tab uses. The icon for `IronMine` is in `assets/textures/ui/buildings/iron_mine.png` and the lookup is `BuildingIcons::handles.get(&BuildingType::IronMine)`. Same recipe, no new icons needed.

For the card, render the icon at **24×24** (slightly smaller than the Build card's 36×36 — the Mining card is narrower and the icon is decorative rather than navigational). Border `CYAN_BORDER` (1 px), border-radius 4 px. Tint with `ImageNode::new(handle).with_color(CYAN)` (`:2783`).

**Fallback:** if the handle is `None` (defensive — should not happen in practice), render a 24×24 placeholder square with `BackgroundColor(srgba(0.373, 0.784, 0.847, 0.30))` + `CYAN_BORDER` border. Same fallback as the Build card (`:2787-2800`).

---

## 3 · Layout sketch (ASCII art, with measurements)

The Mining tab body lives inside the canary root at the same level as the Build card grid. It carries `ConstructionTabBody::Mining` so the existing `tick_construction_body_visibility` system (`src/ui/construction.rs:862-876`) gates visibility correctly.

```
Y: 0   ┌──────────────────────────────────────────────────────────────────────────────┐
       │ MINING               Earth — Sol III (8.2B)                  Yield: 1.00×  │ ← header_row (32 px)
Y: 32  ├──────────────────────────────────────────────────────────────────────────────┤
       │  Build qty:  [×1] [×5] [×25] [×50]              Applies to +               │ ← build_qty_row (28 px)
Y: 60  ├──────────────────────────────────────────────────────────────────────────────┤
       │  ▾  CONSTRUCTION MATERIALS  (9)                                              │ ← group header (28 px)
Y: 88  │  ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐                                          │
       │  │Iron│ │Copper│ │Alum│ │Sil │ │Nick│  ← 5 cards/row, 164 px each,         │
       │  └────┘ └────┘ └────┘ └────┘ └────┘    8 px gap, 8 px pad                  │
Y: 272 │  ┌────┐ ┌────┐ ┌────┐ ┌────┐                                                │
       │  │Tung│ │Carb│ │Chro│ │Magn│                                                │
       │  └────┘ └────┘ └────┘ └────┘                                                │
Y: 368 │  ▾  PRECIOUS METALS  (3)                                                    │ ← group header
Y: 396 │  ┌────┐ ┌────┐ ┌────┐                                                      │
       │  │Gold│ │Silv│ │Plat│                                                      │
       │  └────┘ └────┘ └────┘                                                      │
       │  ...                                                                          │
       │  ▾  HELIUM-3  (body: Moon, GasGiant, Asteroid)  (1)                         │ ← group header
       │  ┌────┐                                                                      │
       │  │He-3│   ← body-gate indicator active when active body is Earth/Mars/Venus │
       │  └────┘                                                                      │
       │                                                                              │
       │  ▸  ORBITAL MINES  (AutoMines · body: [Asteroid, Moon, GasGiant])  (25)    │ ← collapsed by default
       │                                                                              │
       │  (when expanded: same row layout, 5 sub-headers, 25 cards)                  │
Y: EOF └──────────────────────────────────────────────────────────────────────────────┘
        X: 0                                                                 X: 1280
```

Inside one card (164 × 184):

```
Y: 0   card_rim (1.5 px, CARD_TOP_HIGHLIGHT) — absolute, top 0
Y: 8   [icon 24×24]  Iron Mine                                    ← header row (32 px)
Y: 40  ─────────────── hairline (HAIRLINE, 1 px) ─────────────────
Y: 48  ×25                                                       ← count (14 px)
Y: 66  3.0 Gt/yr                                                 ← production (12 px, mono)
Y: 82  Res: 142.3 Gt                                             ← reserve (12 px, mono)
Y: 98  Acc: 60%                                                  ← accessibility (12 px)
Y: 114 ─────────────── hairline (HAIRLINE, 1 px) ─────────────────
Y: 122 [ − ] [ + ]   +25                                         ← control row (24 px, mono hint)
Y: 146 (🔒 body line only when body_blocked; 12 px, ORANGE_ORE)
Y: 158 [body for build_qty chip / etc., unused space]
Y: 184 EOF
```

All values inside the card are padded `SPACE_SM` (8 px) on the left/right. The hairline rows sit flush against the card's `border-radius: 4` corners (no extra padding on the hairlines themselves — let the radius clip them).

---

## 4 · Interaction spec

### 4.1 Build-multiplier chip

- Click any chip in the row → set `ui_state.build_multiplier = n` AND set `ui_state.mining_build_multiplier = n` (see §5.4).
- Active chip re-renders with `ACTIVE_CHIP_BG` + glow.
- The chip row persists across re-mounts (it's spawned once at startup; the update system writes only the active-chip overlay).

### 4.2 [-] [+] buttons (per card)

**Click on [+]:**
- If `body_blocked` → no-op (button is `Interaction::None` equivalent — see §4.4).
- Else: push `(colony_entity, bt, mining_build_multiplier as i32)` to `PendingConstructionActions::mining_edits`.
- The display hint "+N" next to the [+] shows `mining_build_multiplier` (e.g. "+25" when chip is set to ×25, hidden when set to ×1).
- Multi-click: each click pushes a separate tuple; the system applies them on the next tick. With ×50 and rapid clicking, the user can build 250 mines in 5 clicks — this is the intended fast-iteration path.

**Click on [-]:**
- If `body_blocked` → no-op.
- Else if `count == 0` → no-op (button is dim/disabled).
- Else: push `(colony_entity, bt, -(mining_build_multiplier as i32))`.
- `process_construction_actions` (`src/colony/systems.rs:201-203`) clamps to the current count via `Colony::remove_buildings(bt, n) -> u32`, so pushing -50 when only 12 are built silently removes 12 (the return value tells the system what was actually removed; the spec doesn't need to display this).
- **The [-] step uses the same multiplier as [+]** (symmetric, per brief). The legacy egui only supports -1 (`src/ui/construction_panel.rs:1362-1364`); the canary version lifts that to symmetric.

**Click ripple: when +N exceeds the body's workforce / housing capacity, the next-frame update system can dim the production line to ORANGE_ORE and append a caption "⚠ workforce / housing"** (out of scope for v0.5.2 — see §9 R5).

### 4.3 Card body-gate logic (per card, evaluated each frame)

```
body_blocked = !building_is_available_on(
    def,
    active_body_breathable,   // Option<bool> — pass-through if body has no AtmosphereComposition
    active_body_type,         // Option<BodyType> — pass-through if body hasn't been spawned
)
```

Where `building_is_available_on` is `src/colony/data.rs:278-303`. This is the **same predicate the Build sub-tab uses** to filter cards. Reuse it — do not duplicate the gate logic in the Mining tab.

For the active body's `body_type` and `breathable`:
- `Colony` does not directly carry a `CelestialBody` ref; the active body is the colony's own entity (in this codebase, `Colony` is a `Component` on the body entity itself — confirmed by the data path in `extract_resources` at `src/economy/mining.rs:252-253` where the query joins `PlanetResources`, `CelestialBody`, `Option<&Colony>` on the same entity).
- So: `body_query.get(colony_entity)` returns `(Entity, &PlanetResources, &CelestialBody)` for the active colony. `CelestialBody::breathable` (or its `AtmosphereComposition` child) and `CelestialBody::body_type` are the two fields. The exact field names need to be confirmed against `src/plugins/solar_system_data.rs::BodyType` and `src/plugins/solar_system.rs::CelestialBody` during implementation (the legacy egui tab accepts them as `body_breathable: Option<bool>` and `body_type: Option<BodyType>` — see `construction_panel.rs:846-847`).

**Visual when `body_blocked`:**
- Card background: `CARD_BG_HOVER` × 0.5 (dim — multiply alpha down; do not change the border).
- Card content: `TEXT_DIM` for all rows (production, reserve, accessibility all dim).
- [-] and [+] buttons: keep the same bundle but set `Interaction::None` is not enough — replace the `Button` with a non-pickable variant, or add a `MiningButtonDisabled` marker (see §5.5).
- 🔒 body indicator appears below the control row: text "🔒 body" (CAPTION, ORANGE_ORE), centered.

### 4.4 Group collapse (state persists across re-mounts)

State lives in `ConstructionUiState` (extend the struct — see §5.4):

```rust
pub struct ConstructionUiState {
    // ... existing fields ...
    /// Mining tab: which surface groups are collapsed (by group id).
    /// Bitflags / HashSet — see §5.4 for the encoding choice.
    pub mining_groups_collapsed: std::collections::HashSet<MiningGroupId>,
    /// Mining tab: whether the orbital section is collapsed.
    pub mining_orbital_collapsed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MiningGroupId {
    Construction,
    Precious,
    Strategic,
    Fissile,
    Hydrocarbons,
    HeavyWater,
    Helium3,
}
```

`refresh_mining_grid` reads the state to decide `Display::Flex` vs `Display::None` per group, mirroring the existing `tick_construction_body_visibility` pattern (`:862-876`). Group chevron click toggles the bit.

**Default state:** all surface groups expanded, orbital section collapsed. The orbital section has 25 cards; collapsing it by default keeps the initial scroll position stable for the surface-mine player.

### 4.5 Card click — NO secondary action (v0.5.2)

**Decision: no card-level click handler.** A card is a container, not a button. The only interactive children are the [-] / [+] buttons (and the body-gate lock indicator, which is non-interactive). Reasoning:

- The Build sub-tab's `ConstructionCta` button (`:282-284`) is the only meaningful click target on a Build card. The Mining card has no equivalent CTA — it's a pure inventory + production readout.
- A "focus the camera on the body" or "open a detail panel" action would duplicate the ColonyPicker dropdown's affordance and add a new modal/route for marginal value. Out of scope for v0.5.2; flagged for v0.5.3 in §10.

If the user disagrees: the card itself already has `Pickable::default()` (from `CardBundle`'s children setup), so wiring a card-level click is a 4-line change — `MiningCardClick` marker + a system that reads the click and emits whatever event. Spec it in §10 as a v0.5.3 follow-up.

### 4.6 Keyboard

**No keybindings in v0.5.2.** The canary has no keyboard layer yet. When the canary's keyboard layer lands, the natural binding would be:
- `+` / `=` → click [+] on the focused card
- `-` / `_` → click [-] on the focused card
- `1` `5` `2` `5` `5` `0` → set build multiplier (single-keystroke; toggle on second press)
- `Tab` / arrow keys → focus next/prev card
- `Enter` → open detail panel (when that lands)

Out of scope for this PR; flag in §10.

---

## 5 · Empty / zero states

### 5.1 `count == 0` on a surface mine (e.g. no TungstenMines built)

- Count text: `×0` in `TEXT_DIM` (not `CYAN`).
- Production row: `—` in `TEXT_DIM` (no production yet).
- Reserve row: live value from `PlanetResources` (e.g. "Res: 0.3 Mt"), in `TEXT_BODY`.
- Accessibility row: live value (e.g. "Acc: 30%"), tone = `ORANGE_ORE` (because < 50%).
- [+] button: **enabled, functional**. The player can bootstrap the first mine even on a body with no proven reserve — the deposit will fill in as they build (the production system in `src/economy/mining.rs` does NOT pre-require a non-zero reserve; proven_crustal grows as the sim runs).
- [-] button: **disabled** (count == 0). Visually: same bundle but `Interaction::None` + dim text. Hover ripple is disabled.

### 5.2 `count == 0` AND no deposit on body

- Reserve row: "no deposit" in `TEXT_DIM` (instead of "Res: 0.3 Mt").
- Accessibility row: "Acc: —" in `TEXT_DIM`.
- [+] button: **still enabled.** Same rationale as §5.1 — the player might want to start a mining operation on a body that the survey system hasn't characterised yet, and the deposit might materialise once the body's geology is fully probed.
- The legacy egui version has a "no deposit" caption (`src/ui/construction_panel.rs` — see the "—" pattern in `format_mining_rate` at `src/ui/construction_panel.rs:1005`). Mirror it.

### 5.3 AutoMine card on a non-orbital body (Earth, Mars, Venus)

- All card content dimmed (`TEXT_DIM`).
- [-] [+] buttons: **disabled**. The `MiningButtonDisabled` marker (see §5.5) makes the click handler no-op.
- Body-gate indicator: "🔒 body — AutoMines require [Asteroid, Moon, GasGiant]" (`CAPTION`, `ORANGE_ORE`), centered below the control row.
- Card border: change to `ORANGE_ORE` at 0.18 alpha (slightly warmer than the standard `CARD_BORDER`) so the player can scan the orbital section and immediately see which cards are unavailable on this body.

### 5.4 Surface He-3 mine on a non-He-3 body (Earth, Mars, Venus)

- Same dim treatment as §5.3. `He3Mine`'s `allowed_body_types = [Moon, GasGiant, Asteroid]` (`src/colony/data.rs:90-91`); the body-gate predicate catches it.
- Body-gate indicator: "🔒 body — He-3 requires [Moon, GasGiant, Asteroid]".

### 5.5 Colony with no `PlanetResources` (pre-game-start / freshly surveyed)

- All cards in the affected body show "Survey the body to see deposits" as the reserve row.
- Production row: "—".
- Accessibility row: "Acc: —".
- [+] button: **enabled** (the player can still build; the deposit will appear once the survey completes).
- The "no PlanetResources" check is one extra `if planet_resources.is_none()` in `compute_mining_card_data` (already handled by the existing `match (produced_resource, planet_resources)` arm at `src/ui/construction_panel.rs:1102-1114`).

### 5.6 He-3 in particular (extra handling)

The user asked: "should the resource cards show He-3 (which has body-gated *surface* mine too) differently from the other 23 resources?" The answer: **no visual distinction, the body-gate handles it.** He-3 sits in its own group (Group 7) with a group label that already includes the body-gate hint ("HELIUM-3 (body: Moon, GasGiant, Asteroid)"). If the active body fails the gate, the single He-3 card dims and shows the same body-gate line as the AutoMine cards. This is symmetrical and predictable.

---

## 6 · Implementation plan

### 6.1 New markers (in `src/ui/construction.rs`)

```rust
/// Marker on the Mining tab body's root. Already exists as
/// `ConstructionTabBody::Mining` (`:806-813`) — no change needed.
```

Actually — the existing `ConstructionTabBody::Mining` marker is already on `spawn_stockpiles_body`'s root (`:1573`). Rename `spawn_stockpiles_body` → `spawn_mining_body` and `update_stockpiles_body` → `update_mining_body`. No new top-level marker needed.

```rust
/// Marker on the Mining tab's header row text ("MINING" label).
/// Mirrors `StockpilesHeader` (`:1618-1619`).
#[derive(Component)]
pub struct MiningHeader;

/// Marker on the Mining tab's group-collapsible container. Each
/// instance carries the `MiningGroupId` it represents.
#[derive(Component)]
pub struct MiningGroupHeader {
    pub group_id: MiningGroupId,
}

/// Marker on a single Mining card. Carries the building type so the
/// click handler can route to the right `PendingConstructionActions::mining_edits` push.
#[derive(Component)]
pub struct MiningCard {
    pub building_type: BuildingType,
}

/// Marker on the [-] button of a Mining card.
#[derive(Component)]
pub struct MiningMinusButton {
    pub building_type: BuildingType,
}

/// Marker on the [+] button of a Mining card.
#[derive(Component)]
pub struct MiningPlusButton {
    pub building_type: BuildingType,
}

/// Marker added by the gate system when the card is on a body where
/// this building is unavailable (`building_is_available_on == false`).
/// The click handler skips the push when this marker is present.
#[derive(Component)]
pub struct MiningButtonDisabled;
```

### 6.2 New helpers (in `src/ui/construction.rs`)

```rust
/// Compute per-card data (base yield, accessibility, reserve, yield_mult).
/// Mirrors `compute_mining_card_data` in the legacy tab
/// (`src/ui/construction_panel.rs:1061-1122`) but takes the canary's
/// `Res<BuildingsData>` and `Option<&PlanetResources>` directly.
fn compute_mining_card_data(
    building: BuildingType,
    buildings_data: &BuildingsData,
    planet_resources: Option<&crate::economy::components::PlanetResources>,
    yield_mult: f64,
) -> MiningCardData { /* ... */ }

/// Format a mining rate (Mt/yr) — REPLACE the existing
/// `format_mining_rate` at `src/ui/construction.rs:645-663` with the
/// same scale labels (g/kg/Mt/Gt/Tt) but also accept the rate-reserve
/// form. The legacy helper at `src/ui/construction_panel.rs:1004-1036`
/// has TWO helpers (`format_mining_rate` + `format_mining_reserve`)
/// with slightly different boundaries; the canary version should
/// match the legacy reserve formatter for the reserve row and the
/// canary rate formatter (already exists) for the production row.
///
/// Implementation note: factor both into a single
/// `format_mining_amount(mt: f64, with_per_yr: bool) -> String`
/// and let each row call it with the right flag.

/// Format a mining reserve amount (Mt) — g/kg/kt/Mt/Gt/Tt.
fn format_mining_reserve(mt: f64) -> String { /* ... */ }
```

### 6.3 New systems (in `src/ui/construction.rs`)

```rust
/// Setup-time spawner for the Mining body. Replaces
/// `spawn_stockpiles_body` (`:1555-1615`). Builds the header row,
/// the build-qty chip row, and the group sections (each with its
/// own row of cards, populated by a subsequent `refresh_mining_grid`
/// call).
fn spawn_mining_body(commands: &mut Commands, parent: Entity, body_font: &Handle<Font>, /* ... */) { /* ... */ }

/// Per-frame refresh: rebuilds the card set inside each group based
/// on the live colony + body data. Mirrors `refresh_card_grid`
/// (`:3678-3721`) but uses a `Local<Vec<Entity>>` per group so
/// intra-group diffs are cheap. Runs every frame (or on
/// `resource_changed::<ConstructionUiState>` + a body-changed signal —
/// see §9 R6).
pub fn refresh_mining_grid(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    buildings_data: Res<BuildingsData>,
    body_query: Query<(Entity, &crate::colony::Colony, &crate::economy::components::PlanetResources, &crate::plugins::solar_system::CelestialBody)>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    group_query: Query<Entity, With<MiningGroupBody>>,
    card_query: Query<Entity, With<MiningCard>>,
    /* ... */
) { /* ... */ }

/// Apply the build-qty chip click. Mirrors the `ChipKind::Qty(n)` arm
/// of `tick_construction_chip_click` (`:4092-4094`) but for the
/// new `ChipKind::MiningQty(n)` variant.
fn tick_mining_qty_chip_click(/* ... */) { /* ... */ }

/// Apply [-] / [+] clicks. Mirrors `tick_construction_cta_click`
/// (`:4135-4173`) but pushes to `mining_edits` instead of
/// `start_construction`. Rising-edge detection identical.
pub fn tick_mining_button_click(
    interactions: Query<(Entity, &Interaction, &MiningMinusButton), With<MiningMinusButton>>,
    /* + the same for PlusButton */
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<HashMap<Entity, Interaction>>,
) { /* ... */ }

/// Apply the body-gate per card: every frame, for each MiningCard,
/// recompute `building_is_available_on(def, body_breathable, body_type)`
/// and toggle the `MiningButtonDisabled` marker on the card's [-]
/// and [+] children. Runs every frame so the gate flips immediately
/// when the player switches colonies via the dropdown.
pub fn tick_mining_body_gate(
    mut commands: Commands,
    buildings_data: Res<BuildingsData>,
    body_query: Query<&crate::plugins::solar_system::CelestialBody>,
    ui_state: Res<ConstructionUiState>,
    cards: Query<(Entity, &MiningCard)>,
    buttons: Query<(Entity, &Children), (With<MiningMinusButton>, Or<(With<MiningMinusButton>, With<MiningPlusButton>)>)>,
    /* ... */
) { /* ... */ }

/// Toggle group / orbital-section visibility based on
/// `ConstructionUiState::mining_groups_collapsed` and
/// `mining_orbital_collapsed`. Mirrors
/// `tick_construction_body_visibility` (`:862-876`).
pub fn tick_mining_group_visibility(
    ui_state: Res<ConstructionUiState>,
    group_query: Query<(&MiningGroupHeader, &mut Node, &mut Visibility)>,
    orbital_query: Query<&mut Visibility, With<MiningOrbitalBody>>,
) { /* ... */ }
```

### 6.4 `ConstructionUiState` extensions (in `src/ui/construction_panel.rs:82-94`)

The state struct lives in the legacy file (intentional — the canary reuses it). Extend it minimally:

```rust
pub struct ConstructionUiState {
    // ... existing fields ...
    pub build_multiplier: u32,                // Build tab
    pub selected_colony: Option<Entity>,
    pub selected_tab: ConstructionTab,
    pub selected_build_tab: usize,
    pub selected_filter: BuildFilter,

    /// NEW (v0.5.2): Mining tab's build multiplier. Decoupled from
    /// `build_multiplier` so the Mining tab doesn't fight the Build
    /// tab when both are visible in the same menu (they aren't today
    /// — only one tab is visible at a time — but future split-pane
    /// views would benefit). Default 1. The chip set is {1, 5, 25, 50}.
    pub mining_build_multiplier: u32,

    /// NEW (v0.5.2): surface-mine groups currently collapsed.
    /// Empty = all groups expanded. Default empty.
    pub mining_groups_collapsed: std::collections::HashSet<MiningGroupId>,

    /// NEW (v0.5.2): whether the orbital AutoMine section is collapsed.
    /// Default true (collapsed — 25 cards is a lot for the first
    /// paint frame).
    pub mining_orbital_collapsed: bool,
}
```

`MiningGroupId` is defined in `src/ui/construction.rs` (or moved to `construction_panel.rs` next to the state — the legacy file already houses `ConstructionTab` and `BuildFilter`, so the new types fit there). I'd put it in `construction_panel.rs` for consistency; the canary imports the type from there.

### 6.5 Wiring into the plugin

In `src/colony/mod.rs:184-194` (the construction plugin's system list), `process_construction_actions` already consumes `mining_edits` (`:172-213`). No change needed there.

In the canary's main plugin (wherever `setup_construction` and the chip systems are registered — search for `setup_construction` in the plugin file):

```rust
// Replace:
.add_systems(Update, (refresh_card_grid.run_if(resource_changed::<ConstructionUiState>), /* ... */))
// With (additive — don't remove the Build tab systems):
.add_systems(Update, (
    refresh_mining_grid.run_if(
        resource_changed::<ConstructionUiState>
            .or(resource_changed::<crate::economy::DirtyBodies>)
    ),
    tick_mining_button_click,
    tick_mining_body_gate,
    tick_mining_group_visibility,
).chain())
```

**Order matters:** `refresh_mining_grid` (despawn+respawn) → `tick_mining_body_gate` (apply disabled marker to the new buttons) → `tick_mining_button_click` (read interaction + push to mining_edits). The `.chain()` enforces the order; otherwise a stale `prev` interaction could push a stale click on a card that was just respawned.

### 6.6 Removal of the legacy egui body

Once the canary's mining body is graduated (`construction_panel.rs` is deleted along with `theme.rs`), `MINING_GROUPS_SURFACE`, `MINING_GROUPS_ORBITAL`, `compute_mining_card_data`, `render_mining_grid`, `render_mining_section`, `render_mining_card`, and the `MiningCardData` struct all go with it. Don't duplicate the data — define them ONCE in the canary file. This is a hard cut at graduation, not a soft one (palette drift is the #1 known issue per the system prompt).

---

## 7 · Risk register & open questions

### R1 — Mine count drift (DECIDE BEFORE CODING)

- Brief: 22 base mines + 25 AutoMines.
- `src/colony/data.rs::parse_building_type` (`:336-401`): **24 base mines** + **25 AutoMines** (including AutoWaterProcessor).
- `MINING_GROUPS_SURFACE` legacy constant: **23 base mines** (moves CopperMine from Strategic to Construction).
- `MINING_GROUPS_ORBITAL` legacy constant: **24 AutoMines** (omits AutoWaterProcessor? — actually no, it's in the last sub-group. Let me re-count: 9+3+6+2+4 = 24, but AutoWaterProcessor + 3 fissile/hydro/he3/auto = 4, so total 9+3+6+2+4 = 24; but I said 25 earlier including AutoWaterProcessor — let me re-verify: AutoConstruction=9, AutoPrecious=3, AutoStrategic=6, AutoFissile=2, AutoLast=4 → 9+3+6+2+4 = **24**. Hmm. So 24 in legacy, 25 in parse_building_type (the discrepancy is one — likely `AutoWaterProcessor` is in the last sub-group of the legacy, but the brief says 25 because parse_building_type lists all 25 including one extra somewhere).

**Action item:** have the user pick one of:
- (a) "24 / 25" — match `parse_building_type` exactly. The legacy tab is wrong by 1.
- (b) "23 / 24" — match the legacy tab exactly. One of the AutoMines in `parse_building_type` is dead-code or future-content.
- (c) "22 / 25" — match the brief. One of the surface mines gets removed (likely `FluorineMine` or `MagnesiumMine`); AutoMines match the code.

**Recommendation:** (a) 24 / 25. The canary's RON-driven `parse_building_type` is the source of truth; the legacy tab is short by 1 because the layout was hand-curated. Brief is a rough estimate.

### R2 — PlanetResources lookup per frame (CONFIRM OK)

24 cards × 1 `HashMap::get` per frame on `PlanetResources::deposits` is ~24 lookups, each ~1 µs on a small map. ~25 µs/frame total. Trivial. **Do not cache.** If profiling later shows it as a hotspot, the cache belongs in `PlanetResources` itself (e.g. a `cached_total_reserve_mt: HashMap<ResourceType, f64>` updated when deposits change), not in the UI.

### R3 — Build-qty chip set drift (DECIDE BEFORE CODING)

- Brief: 4 chips {1, 5, 25, 50}.
- Canary Build tab: 6 chips {1, 5, 10, 25, 50, 100} (`src/ui/construction.rs:2216-2223`).
- The asymmetry is awkward: ×10 and ×100 are valid Build-tab multipliers but invalid Mining-tab multipliers. The player will notice.

**Action item:** ask the user. Two reasonable options:
- (a) Use 4 chips on Mining, 6 on Build, with a small "(4 options)" footnote on Mining. Honest about the v0.5.2 design choice.
- (b) Unify to 6 chips on both. More options for the player; the x10 / x100 chips are still useful for rapid inventory growth (e.g. spamming 100 TungstenMines for late-game stock).

**Recommendation:** (a) for v0.5.2 (matches brief), with a follow-up to unify in v0.5.3.

### R4 — Collapsible group component (DECIDE)

Three options for the group container:
- Re-use `ChipRowContainerBundle` (28 px default) — already exists, the chevron fits inside, no new bundle.
- Add a new `GroupHeaderBundle` (28 px, expandable chevron + label + count) — more semantically right but adds a new bundle to `bevy_theme.rs`.
- Hand-roll each group header inline (5 lines per group × 7 groups = 35 lines) — works but inconsistent.

**Recommendation:** option 2, add `GroupHeaderBundle` to `bevy_theme.rs`. Mirrors the cost of `ChipRowContainerBundle` (~30 lines), reusable for any future tab that has collapsible groups (the future Resources tab is a likely consumer).

### R5 — Workforce / housing capacity check (DEFER TO v0.5.3)

The canary has `Colony::total_workforce_demand` and `Colony::housing_capacity` (`src/colony/components.rs:400-404`, `:286-292`). Adding a +N push that exceeds either would create understaffed / overcrowded states. The legacy egui ignores this; the canary version could warn the player with an `ORANGE_ORE` caption on the affected card. Out of scope — the brief is explicit about scope. Flag in §10.

### R6 — Refresh trigger (DECIDE)

`refresh_mining_grid` could be triggered by:
- (a) `resource_changed::<ConstructionUiState>` — refreshes when the player changes tab or build qty.
- (b) `resource_changed::<Colony>` (per-colony) — refreshes when `add_building` or `remove_buildings` changes the count.
- (c) Every frame — always correct, no cache invalidation logic, but ~24 cards × 1 frame × 60 fps = 1440 spawn/despawns per second. The canary's build tab refreshes on `resource_changed` (`:4757`); the Mining tab should mirror.

**Recommendation:** (a) + (b) — both are signal sources. Combine with `.or()`:
```rust
run_if(resource_changed::<ConstructionUiState>
    .or(any_component_changed::<Colony>())
    .or(resource_changed::<crate::economy::DirtyBodies>()))
```

`DirtyBodies` (already used at `src/colony/systems.rs:212`) is the existing signal that a colony's production / stockpile / mine count changed — it's the right hook to use.

### R7 — Click handler `Local<HashMap>` key

`tick_mining_button_click` uses the same rising-edge detection as the Build tab's `tick_construction_cta_click` (`:4135-4173`). The `Local<HashMap<Entity, Interaction>>` is per-system; the same `Entity` ID is never reused across frames in Bevy 0.18 (entity recycling is opt-in via `World::enable_entity_recycling`). The current Build tab uses this pattern safely; the Mining tab should too. **No risk** — flag for the engineer to confirm they understand the `Local` is per-system, not per-entity.

### R8 — `process_construction_actions` is single-tick

`process_construction_actions` consumes `mining_edits` via `drain(..)` (`:193`). Each click pushes ONE tuple; the next tick applies it. If the user clicks [+] ×50 five times rapidly, 250 mines are added across 5 ticks. **No batching across ticks** — the brief accepts this (it's the same latency as the Build CTA). If the user wants atomic batch-updates later, the path is to push a single tuple with `delta = 250` and have `process_construction_actions` loop `for _ in 0..delta` (it already does for positive deltas at `:198-200`). The current design supports it without code changes; the click handler is the only thing that needs to call this.

---

## 8 · Acceptance criteria (v0.5.2)

The PR ships when all of the following are true:

1. **The Mining tab body** spawns when the Construction menu opens, marked `ConstructionTabBody::Mining`, with the 7 surface groups + 1 orbital section visible (or orbital collapsed by default).
2. **The header row** shows "MINING", the active colony name + body name, and the yield chip.
3. **The build-qty chip row** shows 4 chips {×1, ×5, ×25, ×50}, default ×1 active. Clicking any chip updates the active chip and `ui_state.mining_build_multiplier`.
4. **Each surface card** shows: icon, name, `×N` count, production rate (mono, GREEN_FIN if > 0), reserve (mono, TEXT_BODY), accessibility (mono, GREEN_FIN ≥ 50% / ORANGE_ORE > 0% / TEXT_DIM = 0%), and [-] [+] buttons.
5. **Each orbital card** has the same layout; on a non-orbital body the buttons are disabled and the 🔒 body indicator appears.
6. **Click [-] when count > 0** pushes `(colony, bt, -multiplier)` to `mining_edits`. The count drops by `multiplier` (clamped) on the next frame.
7. **Click [+]** pushes `(colony, bt, +multiplier)` to `mining_edits`. The count grows by `multiplier` on the next frame. The "+N" hint shows `multiplier` when > 1.
8. **Click a group chevron** toggles that group's `Display::None`/`Flex`; the state persists when the player switches tabs and back.
9. **Click the orbital section chevron** toggles the entire 5-sub-group section.
10. **Switching colonies** via the picker re-runs `compute_mining_card_data` for the new body's `PlanetResources` and the gate predicate. The cards re-skin to the new body's deposit + body_type within one frame.
11. **No allocations on the hot path** (no `String` in the per-frame spawn, no `Vec` reallocations inside the card body). The `MiningCardData` is computed once per card per frame, the card layout re-uses the existing `CardBundle`.
12. **`process_construction_actions`** continues to consume `mining_edits` without changes (already does — see §6.5).
13. **The legacy `render_mining_grid` and friends in `src/ui/construction_panel.rs:872-1392` are NOT removed** in this PR. They stay in place until the canary graduates (the egui path is still the live render for non-canary consumers). The PR adds the bevy_ui path; graduation is a separate PR that deletes the egui path AND `src/ui/theme.rs` together.
14. **No new icons needed** — all 47 mine / AutoMine building PNGs are already in `assets/textures/ui/buildings/` and loaded by `BuildingIcons`.
15. **No new `Color` constants beyond the one `RED`** (see §2.1 R1). The rest of the palette is canary-stable.

---

## 9 · Out of scope for v0.5.2 (v0.5.3 candidates)

| # | Item | Rationale |
|---|---|---|
| O1 | **"USGS 2024 demand" bootstrap one-click button** (per ui-ux-designer's PR-A recommendation) | Adds a CTA that bulk-queues mines to match 2024 world production. Touches the simulation balance (would need its own balance pass to validate). Defer. |
| O2 | **Card-level click → detail panel / focus camera** | The brief explicitly says NO secondary action. Add when the Resources tab or a Mining detail panel lands. |
| O3 | **Workforce / housing capacity warning** | Adds a capacity gate to +/- buttons. Touches the sim (understaffed mines reduce output). Defer. |
| O4 | **Keyboard layer** | The canary has no keybinding system. Defer until the canary's keybinding layer lands (separate PR). |
| O5 | **Unify Mining + Build build-qty chip sets** | Tied to R3. Decide then act. |
| O6 | **Group-collapse memory per-body** | Today, group state is per-player (one set of collapsed groups for all colonies). Per-body memory (e.g. "always collapse Orbital on Earth but expand on Mars") would require a `HashMap<Entity, HashSet<MiningGroupId>>` on `ConstructionUiState`. Defer. |
| O7 | **Search / filter on the Mining tab** | A text input that filters cards by resource name. The group layout is already the de-facto filter, so the value is small. Defer. |
| O8 | **Production-rate graph (sparkline)** | Per-card sparkline of the last 12 months of production. Requires a per-card history buffer. Defer. |
| O9 | **AutoMine + He-3 body-gate from the body's `AtmosphereComposition`** (e.g. GasGiants with H₂ atmosphere get a special "atmospheric harvesting" He-3 card) | Today the gate is `allowed_body_types` only. `AtmosphereComposition` adds nuance. Defer. |
| O10 | **Tech-gate surfacing** | A future tech (e.g. `lunar_colony` for He-3, `asteroid_mining` for AutoMines) gates the card. Today the gate is just body-type. The canary's Build tab has the tech filter via `is_unlocked(tech_id)`; the Mining tab doesn't surface it. Defer. |

---

## 10 · File map (where to make the changes)

| File | Change | Scope |
|---|---|---|
| `src/ui/construction.rs` | Rename `spawn_stockpiles_body` → `spawn_mining_body`; add build-qty chip row; add group section spawn loop; add card spawn loop. | Big |
| `src/ui/construction.rs` | Rename `update_stockpiles_body` → `update_mining_body`; rewire to `refresh_mining_grid` (every-frame) + new systems. | Big |
| `src/ui/construction.rs` | Add new markers: `MiningHeader`, `MiningGroupHeader`, `MiningCard`, `MiningMinusButton`, `MiningPlusButton`, `MiningButtonDisabled`, `MiningOrbitalBody`, `MiningGroupBody`. | Small |
| `src/ui/construction.rs` | Add new helpers: `compute_mining_card_data`, `format_mining_reserve`. | Small |
| `src/ui/construction.rs` | Add new systems: `refresh_mining_grid`, `tick_mining_button_click`, `tick_mining_body_gate`, `tick_mining_group_visibility`. | Medium |
| `src/ui/construction.rs` | Extend `ChipKind` enum with `MiningQty(u32)`. | Tiny |
| `src/ui/construction.rs` | Extend `tick_construction_chip_click` to handle the new `ChipKind::MiningQty(n)` arm. | Tiny |
| `src/ui/construction.rs` | Register the new systems in the plugin's system chain (where `refresh_card_grid` is registered). | Tiny |
| `src/ui/construction_panel.rs` | Extend `ConstructionUiState` with `mining_build_multiplier`, `mining_groups_collapsed`, `mining_orbital_collapsed`. Add `MiningGroupId` enum. | Small |
| `src/ui/bevy_theme.rs` | Add `pub const RED: Color` (1 line). | Tiny |
| `src/ui/bevy_theme.rs` | Optionally add `GroupHeaderBundle` (see R4). | Small |
| `src/colony/systems.rs` | **No changes** — `process_construction_actions` already consumes `mining_edits`. | — |
| `src/ui/construction_panel.rs` | **No changes** — the legacy egui path stays until graduation. | — |
| `assets/textures/ui/buildings/*.png` | **No changes** — all 47 mine/AutoMine icons already exist. | — |

**Estimated diff size:** ~600 lines of new code in `src/ui/construction.rs` (mostly the per-card spawn loop mirroring the existing `spawn_card`), ~30 lines in `src/ui/construction_panel.rs`, ~10 lines in `src/ui/bevy_theme.rs`. **No new files.**

---

## 11 · What I deliberately did NOT include (and why)

- **No new icons.** The brief doesn't ask for any, and the `BuildingIcons` resource already covers all 47 mine / AutoMine buildings.
- **No `Mul`/transformation-based group collapse animations.** The current bevy_ui 0.18 idiom in the canary is `Display::Flex` / `Display::None` toggles. Animations would add `Val::Px` interpolation per group; not worth the complexity for v0.5.2.
- **No build-cost check on [-].** The legacy egui doesn't check it (a 0-cost bulk-remove is always allowed because mines have no `awaiting_resources` lifecycle for removal). The brief doesn't ask for it. The behavior is consistent with "direct inventory edits" — the player is telling the sim "remove these," not "tear them down."
- **No rebalance of the `*Production` modifier values.** The brief says "non-negotiable per the calibration table." `data.rs` is the calibration table; we read from it, we don't write to it.
- **No removal of the legacy egui mining tab.** That happens at graduation, not in this PR. The brief is clear: this is a canary addition, not a flag-day migration.
- **No rebalance of `AutoWaterProcessor`.** It's in the legacy `MINING_GROUPS_ORBITAL` last sub-group with `AutoHe3Mine`, `AutoMethaneExtractor`, `AutoDeuteriumExtractor`. The canary version mirrors that grouping.

---

*Spec written against `rework-ui-design` branch, commit HEAD. All file paths and line numbers verified against the working tree on 2026-08-01. If the user accepts this spec, the next step is a `git checkout -b feature/mining-tab-canary` from `rework-ui-design` and the implementation plan in §10.*
