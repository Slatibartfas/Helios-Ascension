# Helios Ascension — UI Layout Patterns

This document is the source of truth for Helios Ascension's *layout-level*
UI patterns. It complements `docs/UI.md` (design tokens, per-panel anatomy,
contribution rules) by codifying the four canonical *layout shells* every
panel composes from, and mapping every current panel to one of them.

> **Why a separate doc?** `docs/UI.md` §2 codifies *components* (frames,
> tokens, builders). §3 codifies *panels* (per-domain anatomy). Neither
> captures the *layout shells* that panels share — the four recurring
> skeletons we keep re-implementing. The v1 UI harmonization chain
> (GRA-53..GRA-59) closed clean at `32a8365a` and standardised tokens;
> this doc standardises the *shells* so v2 work (GRA-66..GRA-72) has a
> stable vocabulary.
>
> **Anchor refresh (2026-06-12, post-v2-UI chain).** The v2-UI chain
> has landed (PRs #125, #127, #128, #129, #132, #133). All `src/...`
> line references in this doc have been re-verified against `origin/main`
> at SHA `83417334c6459e7429c45cf19fe2d33f88a5b5c6`. See issues
> GRA-95 / GRA-96 / GRA-97 for the audit trail.

## 1. Overview

Helios Ascension has a mixed UI stack (egui 0.33 for most panels, native
Bevy UI 0.18 for the Shipbuilding workspace — see `docs/UI.md` §1).
Every panel — egui or Bevy UI — is built from one or more of the four
**canonical layout patterns** below.

| # | Pattern | Stack | Reference implementation |
| - | ------- | ----- | ------------------------ |
| 1 | Top menu bar | egui | `src/ui/mod.rs:610-896` (`ui_top_menu_bar`) |
| 2 | Right-side ledger | egui | `src/ui/dossier_panel.rs` (`ui_planet_dossier`) |
| 3 | Tabbed workspace (3 panes) | Bevy UI | `src/ui/shipbuilding_workspace.rs:1241-1296` (`populate_tab_strip`) |
| 4 | In-panel sub-tab strip | egui | `src/ui/construction_panel.rs:760-776` (`theme::tab_strip<ConstructionTab>` call site) |

Panels can compose multiple patterns. The Shipbuilding workspace is
*Pattern 3 + Pattern 4*. The Construction menu is *Pattern 4 + the
8-`BuildingCategory` strip (a Pattern 4 sub-instance)*. The Main Menu
is a *full-screen modal* built on `theme::central_frame()` and Pattern 2's
ledger — it does not introduce a fifth pattern.

## 2. Pattern 1 — Top menu bar (egui)

The F1..F11 pictogram strip that lives in the persistent top panel and
switches the active `GameMenu`. See `docs/UI.md` §3.1 for the navigation
contract; this section codifies the *layout* contract.

### 2.1 Skeleton

```text
┌──────────────────────────────────────────────────────────────────────┐
│ F1 🔭   F2 🗺   F3 ⚙   F4 🏗   F5 🔬   F6 🚀   F7 ⚓   F8 💰   F9 👤   F10 🔍   F11 🤝
│ Survey  Starmap Menu  Const  Rsch  Fleets Ship  Econ   Pers  Intel   Diplo
└──────────────────────────────────────────────────────────────────────┘
```

### 2.2 Layout contract

- **Container:** `egui::TopBottomPanel::top("top_menu_bar")` (single
  fixed-height strip). Rendered in `UiSystemSet::TopBar` so it
  reserves space before any side panel.
- **Items:** one icon button per `GameMenu` variant, ordered by
  `GameMenu::all()` (`src/game_state.rs:75-89`).
- **Active state:** icon tinted `theme::ACCENT` + 2px stroke ring around
  the widget at `theme::ACCENT`. Inactive icons tinted `theme::ICON_INACTIVE`.
- **Tooltip:** `theme::tooltip_frame()` + bold-mono `theme::kbd_shortcut_label("F1")`
  chip — see `src/ui/mod.rs:670-686` for the canonical tooltip block
  (the `render_tooltip` closure).
- **Hotkey:** `F1`..`F11` mapped by index into `GameMenu::all()`. Defined
  in `src/ui/mod.rs:835-855`. **`Escape`** toggles between the active
  `GameMenu` and the base view (Survey for `ViewMode::System`, Starmap
  for `ViewMode::Starmap`).
- **Click semantics:** clicking an icon switches `active_menu.current`
  to that `GameMenu`. Switching to `GameMenu::Starmap` or `GameMenu::Survey`
  also re-targets the camera via `switch_to_starmap_menu` /
  `switch_to_survey_menu`.

### 2.3 What *not* to do

- Do not add a fifth icon-row above or below this strip. New top-level
  navigation goes through the `GameMenu` enum, not through a parallel
  row.
- Do not use number-key 1..5 inside panels for sub-tab navigation —
  the speed-preset bindings at `src/ui/dashboard.rs:1320-1326` (Digit1..5)
  own those keys. (GRA-57's 1-9 numkey sub-tab aliases collided with
  those and were reverted in PR #119.)

## 3. Pattern 2 — Right-side ledger (egui)

The collapsible right-hand panel that presents a *body* (planet, star,
system, fleet) as a vertical sequence of header → body → collapsible
sections → inline stat rows. Endorsed as the *good* baseline in the
2026-06-09 operator comment (see `issue_comments.id=2ed30e26-…`).

### 3.1 Skeleton

```text
┌─ Jupiter ──────────────────────── ⓘ ✕ ─┐
│ GAS GIANT · Sol system · 5.2 AU       │  ← H1 + sub-caption
│ ────────────────────────────────────  │  ← theme::divider
│ ▼ Physical                            │  ← CollapsingHeader
│   MASS      1.90 × 10²⁷ kg            │  ← stat_row
│   RADIUS    69,911 km                 │
│   GRAVITY   24.79 m/s²                │
│ ▶ Composition                         │  ← CollapsingHeader (closed)
│ ▶ Resources (37 surveyed)             │
│ ▼ Orbital mechanics                   │
│   SEMI-MAJOR  5.204 AU                │
│   ECCENTRICITY  0.0489                │
└───────────────────────────────────────┘
```

### 3.2 Layout contract

- **Container:** a `egui::SidePanel::right(...)` per top-level `GameMenu`
  view, or an `egui::Window` modal for floating ledgers. Width is
  `ui.available_width()` after the central canvas claims its share.
- **Outer frame:** `theme::panel_frame()`.
- **H1:** panel title (body or system name) in `theme::title()` 20pt
  "heading" + a body-type chip in `theme::STATUS_*` colour.
- **H2 / section header:** `CollapsingHeader` (id-stable across frames),
  default-open on first render. Heading text in `theme::heading()` 13pt
  "semibold" with `theme::ACCENT_DIM` left-edge stripe.
- **Stat rows:** `theme::stat_row(ui, label, value)` with `theme::label(text)`
  uppercase dim labels and `theme::value(text)` mono values. For tooltips
  on a label cell, use `theme::stat_row_with_tooltip`.
- **Dividers:** `theme::divider(ui)` between sections (1px `theme::BORDER`).
- **Per-row resources / minerals / items:** `paint_resource_tile(...)` from
  `dossier_panel.rs` for the dossier; `chip_row(...)` is the generic
  equivalent for other panels (one row, N chips, no inner card).

### 3.3 What *not* to do

- Do not put a `egui::TopBottomPanel::bottom` or a third `SidePanel`
  inside a Pattern 2 container. The ledger owns the full side strip.
- Do not paint custom `egui::Frame` instead of `theme::panel_frame()`.
  The egui visuals are configured once in `theme::apply_global_visuals`.
- Do not mix `RichText::new(...).strong()` heading text with
  `theme::heading()`. The two are typographically distinct and read as
  drift across panels (per the v1 audit's 81 `Color32::from_rgb` sweep
  + 6 hardcoded `Color::srgb(...)` sites in shipbuilding).

## 4. Pattern 3 — Tabbed workspace (Bevy UI 0.18)

The three-pane shell used by the Shipbuilding workspace (Logistics Hub /
Design Blueprint / Engineering Analytics). **This is the only native Bevy
UI panel** — see `docs/UI.md` §1.

### 4.1 Skeleton

```text
┌─ Design │ Archive │ Construction Control │ Component Database ─────┐  ← TabStrip
├────────────────┬──────────────────────────┬────────────────────────┤
│                │                          │                        │
│  Logistics Hub │  Design Blueprint        │  Engineering Analytics │
│  (left pane)   │  (centre pane)           │  (right pane)          │
│                │                          │                        │
│  hull select   │  slot canvas             │  gauges + chips        │
│  slot nav      │  hover/select            │  delta-v, mass, ...    │
│  module list   │                          │                        │
│                │                          │                        │
└────────────────┴──────────────────────────┴────────────────────────┘
```

### 4.2 Layout contract

- **Container:** a `NodeBundle` root that owns the tab strip and the
  three-pane child grid. `populate_tab_strip` uses `theme::Color`
  throughout (the PR-B Bevy Color mirror was landed in PR #122, and the
  previous GRA-54 `Color::srgb(...)` regression was replaced in PR #127
  / GRA-69).
- **Tab strip:** `parent.spawn((Button, ShipbuildingWorkspaceTabButton { tab }, Node, BackgroundColor, BorderColor, Text, TextFont, TextColor))`
  per active tab. `min_width` is per-tab: `Components=188px`,
  `Construction=168px`, others `136px`. `min_height=30px`. Padding
  `UiRect::axes(12px, 6px)`. Border `1px`.
- **Active / inactive state:** the active button uses the
  `theme::Color::TAB_ACTIVE_BG` and `theme::Color::TAB_ACTIVE_BORDER`
  constants (Bevy Color mirror of the existing egui tokens, added in
  PR #122). Inactive uses `theme::Color::TAB_INACTIVE_BG` /
  `theme::Color::TAB_INACTIVE_BORDER`. The six call sites in
  `populate_tab_strip` (`src/ui/shipbuilding_workspace.rs:1274-1291`)
  are now `theme::Color`-based; the post-merge note at
  `src/ui/shipbuilding_workspace.rs:1262-1269` documents the
  replacement.
- **Three-pane grid:** columns of `Val::Percent(33.3)` (or `Flex` with
  weights). The centre pane (Design Blueprint) gets a heavy `1px` border
  on both sides, painted via `theme::Color::BORDER` (Bevy mirror).
- **Per-pane header:** `theme::section_h1(label)` in Bevy (added in
  PR-B). Currently ad-hoc `Text::new(...)` with hardcoded `TextFont`.

### 4.3 What *not* to do

- Do not bypass `theme::Color`. The Bevy Color mirror landed in PR #122
  and the previous `Color::srgb(0.0, 0.95, 1.0)` regression was
  replaced in PR #127 — no new hardcoded Bevy `Color` literals
  in this function (or its PR-D / PR-E successors).
- Do not introduce a fourth pane. The three-pane structure mirrors the
  intended user flow (browse → design → measure); a fourth pane is a
  feature, not a layout pattern.
- Do not re-implement this shell in egui. The v1 plan endorsed the
  native Bevy UI choice (per `docs/UI.md` §1) — it stays Bevy UI.

## 5. Pattern 4 — In-panel sub-tab strip (egui)

The most common pattern, currently bespoke in Construction, Research,
Economy, and (via Bevy mirror) Shipbuilding. The `ConstructionTab`,
`EconomyTab`, and `ShipbuildingTab` enums are instances of this pattern.

### 5.1 Skeleton

```text
┌─ Overview │ Buildings │ Build │ Stockpiles ─┐  ← top-level sub-tab
├──────────────────────────────────────────────┤
│  ┌─ Infra │ Industry │ Logistics │ Power …  │  ← optional second-level
│  │                                          │     (BuildingCategory)
│  │  building card                           │
│  │  building card                           │
│  └──────────────────────────────────────────┘
└──────────────────────────────────────────────┘
```

### 5.2 Layout contract

- **Container:** the top of the panel, after the title. The strip is
  rendered as a horizontal `ui.horizontal(|ui| { ... })` block — *not* a
  separate `egui::TopBottomPanel`.
- **Items:** one button per variant of the panel's `*Tab` enum. Order
  is the enum's `Default` first, then by gameplay frequency.
- **Active state:** the active button's label is rendered in
  `theme::ACCENT` with a 2px bottom underline in `theme::ACCENT`; the
  inactive labels in `theme::TEXT`. This matches the top menu bar's
  accent treatment (Pattern 1) but at a smaller font size.
- **PR-B primitive:** `theme::tab_strip<T: Tab>(ui, tabs, active, on_select) -> ActiveTab`.
  The `Tab` trait lives in `src/ui/tab.rs` and has `id() -> &'static str`,
  `label() -> &'static str`, `icon() -> Option<&'static str>`. Each
  panel's `*Tab` enum (Construction, Research, Economy) implements it
  via a one-line `impl Tab for ConstructionTab { ... }`.
- **Second-level strip (optional):** the 8-`BuildingCategory` strip in
  Construction is a nested `theme::tab_strip<BuildingCategory>(...)` on
  a row indented by `theme::Spacing::md`. Categories are dispatched
  from `BuildingCategory` via a category-coloured icon (using
  `theme::category_color(name)`).
- **Hotkey:** none at the panel level (numkey sub-tab bindings were
  attempted in GRA-57 and reverted in PR #119 because they collided
  with `dashboard.rs:1320` game-speed digit bindings).

### 5.3 What *not* to do

- Do not store the active tab in two places. The current
  `ConstructionUiState` has both `selected_tab: ConstructionTab` and
  `selected_build_tab: usize` (for the 8-category strip). The v2 plan
  collapses these into a single `Tab` enum chain.
- Do not call `selectable_label` for sub-tab buttons. The pattern
  wants the accent + underline treatment, not the list-row treatment
  (`render_selectable_label` in `dashboard.rs:8`).
- Do not use `RichText::new(...).strong()` for the active label.
  `theme::tab_strip` uses `theme::kbd_shortcut_label`-style tokens for
  consistency.

## 6. Panel-to-pattern mapping

Every current panel mapped to one of the four patterns. PRs C, D, E, F
collapse bespoke implementations onto the patterns above.

| `GameMenu` variant | Panel | Patterns used | v2 PR | Bespoke today |
| ------------------ | ----- | ------------- | ----- | ------------- |
| Survey             | `src/ui/dashboard.rs` (system view) | P1 + (none) | — | mostly tokens |
| Starmap            | `src/ui/dashboard.rs` (starmap view) | P1 + (none) | — | mostly tokens |
| Main               | `src/ui/dashboard.rs` (main menu overlay) | P2 + modal | — | tokens; menu is a single ledger |
| Construction       | `src/ui/construction_panel.rs:24` (`ConstructionTab` enum) | P4 + P4 (nested) | C (GRA-68) | bespoke `ConstructionTab` + `BuildFilter` |
| Research           | `src/ui/research_panel.rs:1568-1574` (P4 categories strip) + `src/ui/tech_tree.rs:233-300` (Archive tab category grouping) | P4 (categories) | D (GRA-69) | PR #127 replaced the inline category loop with `theme::tab_strip<TechCategory>` and the shipbuilding `Color::srgb(...)` literals with `theme::Color` |
| Fleets             | `src/ui/fleets_panel.rs:286` (`ui_fleets_panel`) | P1 (only) | — | flat list, no sub-tabs; company-filter chip |
| Shipbuilding       | `src/ui/shipbuilding_workspace.rs:1241-1296` (`populate_tab_strip`, Bevy UI) | P3 + P4 (mirror) | D + E (GRA-69 + GRA-70) | PR #127 replaced the `Color::srgb(...)` literals with `theme::Color`; PR #128 (GRA-70) consolidated the 3-pane shell |
| Economy            | `src/ui/economy_panel.rs:6` (`EconomyTab` enum, 7 variants) | P4 (7-way) + P2 (Colonies tab) | F (GRA-71) | bespoke `EconomyTab` with 7 variants |
| Personnel, Intel, Diplomacy | (panels not yet implemented) | P1 + TBD | future chain | not in scope for v2 |
| (modal)            | `src/ui/dossier_panel.rs:204` (`ui_planet_dossier`) — top-level system; `draw_survey_section` (SURVEY ledger, PR-F/GRA-108) and `draw_resource_section` (RESOURCES ledger, PR-B/GRA-67) are the two Pattern 2 instances | P2 | ref only | the reference P2 implementation |
| (modal)            | `src/ui/resources_bar.rs:1154` (`ui_resources_bar`) | (none) | — | persistent HUD strip, not a panel |

**Composition rules:**

- A panel uses at most one P3 instance (the tabbed workspace). P3
  implies P4-mirror for the tab strip.
- A panel uses at most one P2 instance (the right-side ledger). A
  P2 ledger can be modal (an `egui::Window`) or side-strip
  (`egui::SidePanel::right`).
- P4 can stack: a panel can have a top-level sub-tab strip *and* a
  nested second-level strip (Construction does this with the
  8-`BuildingCategory` rows).
- P1 is reserved for the persistent top menu bar. No panel-level use.

## 7. Application: what each v2 PR does

| PR  | Issue | Pattern(s) added or consolidated | File(s) touched |
| --- | ----- | -------------------------------- | --------------- |
| A   | GRA-66 (this doc) | (doc only) | `docs/UI_LAYOUT_PATTERNS.md` |
| B   | GRA-67 (Coder) | `theme::tab_strip<T>`, `theme::section_h1/h2/h3`, `theme::ledger_panel<T>`, `theme::tab_strip_bevy` + `Tab` trait + `theme::Color` (Bevy mirror) | `src/ui/theme.rs` (+~200 lines), `src/ui/tab.rs` (new, ~50 lines) |
| C   | GRA-68 (Coder) | replace `construction_panel.rs:760-776` bespoke strip with `theme::tab_strip<ConstructionTab>`; collapse `selected_build_tab: usize` + `BuildFilter` into a single primitive | `src/ui/construction_panel.rs` (-~100 lines net) |
| D   | GRA-69 (Coder) | replace `research_panel.rs:1568-1574` category loop with `theme::tab_strip<TechCategory>`; share the strip in Archive (the Archive tab category grouping in `tech_tree.rs:233-300`); shipbuilding `Color::srgb(...)` literals at `populate_tab_strip` (L1241-1296) replaced with `theme::Color` | `src/ui/research_panel.rs` (-~50 lines), `src/ui/shipbuilding_workspace.rs` (no semantic change) |
| E   | GRA-70 (Coder) | parameterise the 3-pane shell as `WorkspaceShell { tabs_root, library_root, canvas_root, analytics_root }`; apply `theme::section_h1` headers | `src/ui/shipbuilding_workspace.rs` (largest PR, ~1.5 d) |
| F   | GRA-71 (Coder) | 7-way `theme::tab_strip<EconomyTab>`; `theme::ledger_panel<T>` for the Colonies tab + (demonstration) Construction Overview | `src/ui/economy_panel.rs` (~1 d) |
| G   | GRA-72 (operator) | `docs/UI.md` §8 (new "Layout patterns" cross-reference) + visual sign-off | `docs/UI.md` (+~20 lines) |

## 8. Contribution rules for new panels

When a new `GameMenu` variant lands (e.g. Personnel, Intel, Diplomacy):

1. Open the *top menu bar* slot via the `GameMenu::all()` order in
   `src/game_state.rs:75-89`. Add the icon asset under
   `assets/textures/ui/menu/<name>.png`. Pattern 1.
2. Pick a primary pattern from the four above. Default to P2 (ledger)
   for read-only information; default to P4 (sub-tab strip) for
   multi-view interactions; default to P3 (tabbed workspace) only if
   the panel is a *design* surface (hull/module/template editing).
3. Use `theme::*` tokens for every colour, frame, spacing, label, and
   value. The CI lint at `scripts/audit_color32_literals.py` (added in
   GRA-58) blocks hardcoded `Color32::from_rgb` outside `theme.rs`; the
   corresponding `scripts/audit_bevy_color_literals.py` is added in
   PR-B alongside the `theme::Color` mirror.
4. If a layout need does not fit the four patterns, propose a *fifth*
   pattern in this doc first; do not introduce a one-off shell.
5. Map the new panel into the table in §6 before opening the PR.

## 9. Open questions for the next chain

The v2 chain (GRA-66..GRA-72) covers the four named panels
(Construction, Research, Shipbuilding, Economy). When the
`GameMenu::Personnel` / `Intel` / `Diplomacy` panels land, the same
four patterns apply. Some open questions to answer before then:

- **P2 reuse in non-ledger contexts.** `theme::ledger_panel<T>` is
  designed for the right-side strip. Should we also expose a
  `theme::ledger_card<T>` for *inline* ledger-style blocks (e.g. a
  summary card inside a P4 tab body)?
- **P4 deep nesting.** Construction has P4 → P4 (8 `BuildingCategory`
  rows under the Build tab). Research currently has P4 → (inline
  category grouping, not a sub-tab). Should the Archive view's
  category grouping be promoted to a second P4 strip?
- **Modal ledgers vs side ledgers.** `src/ui/dossier_panel.rs` uses
  a side strip. Some panels (e.g. Tooltips) use a modal. Should the
  contract distinguish them, or stay pattern-agnostic?

These are not blockers for the v2 chain; they are notes for the next
chain.

---

*Document author:* CTO `d46efd74-4de0-46e6-92f9-28c2de107111`
*Date:* 2026-06-10
*Issue:* GRA-66 (v2 PR-A)
*Anchor refresh:* 2026-06-12 (post-v2-UI chain landing). Authored by
LGD `8b113021-…` on branch `lgd/gra-95-96-97-ui-layout-anchor-bundle`.
Covers issues GRA-95 (wrong-context + 4 off-by-ones), GRA-96
(`construction_panel.rs:697-719` wrong-context), and GRA-97 (3
additional wrong-contexts + 2 off-by-ones).
*Supersedes:* the v1 plan's deferred layout-level work (per
`issue_comments.id=2ed30e26-…` and `df13b1ae-…` operator answer
`four-named` 2026-06-10T22:43:15Z)
