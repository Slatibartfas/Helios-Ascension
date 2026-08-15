# UI Migration Plan (egui → native Bevy UI)

Status: **Construction graduated** (v0.5.2, branch `rework-ui-design`,
2026-08-14). The Shipbuilding workspace was already on native Bevy UI.
All other panels remain on egui today.

## Goal

Move every full-screen panel (`GameMenu` variant) to native Bevy UI so
they can share the menu-agnostic primitive library in
`src/ui/widgets.rs`. End state: the egui surface becomes a thin layer
that only handles launch, save/load, and a few HUD strips
(`resources_bar`, `notifications`, `interaction`).

## Where we are now

| Menu | Backend | Status |
| ---- | ------- | ------ |
| Construction | **Native Bevy UI** (`src/ui/construction/`) | ✅ Shipped v0.5.2 |
| Shipbuilding | **Native Bevy UI** (`src/ui/shipbuilding_workspace.rs`) | ✅ Shipped (was the original canary) |
| Survey / Dossier | egui (`src/ui/dashboard.rs`, `src/ui/dossier_panel.rs`) | ⚪ Not started |
| Research | egui (`src/ui/research_panel.rs`, `src/ui/tech_tree.rs`) | ⚪ Not started |
| Economy | egui (`src/ui/economy_panel.rs`) | ⚪ Not started |
| Fleets / Transfer planner | egui (`src/ui/fleets_panel.rs`, `src/ui/transfer_planner*.rs`) | ⚪ Not started |
| Personnel Roster | egui (`src/ui/personnel_panel.rs`) | ⚪ Not started — v0.5.x blocker |
| Notifications | egui (`src/ui/notifications/`) | ⚪ Trivially small; not started |
| Launch / main menu / save-load | egui (`src/ui/launch/`) | ⚪ Not started |

## Architecture (construction as the reference)

Every future bevy_ui menu must compose from primitives that already
live in `src/ui/widgets.rs`:

```text
src/ui/widgets.rs (2195 LOC, menu-agnostic)
├── UiFonts resource + init_ui_fonts             font handles loaded once
├── spawn_scrollable_container(_child)           one scroll helper
├── spawn_text_label                             one-line Text
├── HoverElevation + tick_ui_hover_elevation    shared hover/press
├── card_shadow / card_shadow_hover + CARD_SHADOW* consts
├── ChipGroup / ActiveChips + tick_chip_button_* (hover/active/glow)
├── detect_rising_edges / detect_rising_edges_no_marker
├── TooltipTone/Entry/Content + TooltipRequest/Overlay/Title/Body
│   + populate_tooltip_body + tick_tooltip
├── ScrollbarTrack/Thumb/Metrics/DragState
│   + on_thumb_press/release + on_track_press/release
│   + spawn_scrollbar + tick_scrollbar + tick_ui_scroll_on_wheel
├── KeyedList<K, V> + EntityContainer trait      diff-based reconciler
├── tick_tab_body_visibility
├── ActiveTabs<K> / TabButton<K> / TabBody<K>
│   + tick_active_tab_body_visibility
├── Marquee + tick_marquee
├── ProgressFill(pub f32) + tick_progress_fill
└── CardShellOpts + card_shell / card_icon / card_data_chip
    / card_marquee_subtitle / card_label_value_row / card_footer_cta
```

`src/ui/bevy_theme.rs` holds the Bevy-UI palette mirror (`CARD_BG`,
`CYAN`, `CARD_BORDER`, shadow styles); it coexists with `theme.rs`
(the egui palette). New token writes go to **whichever** palette the
surface uses — never both.

## Recipe (per panel)

The Construction canary followed this sequence (12 phases). New menus
can do the same; see the commit trail at
`memories/repo/ui-migration-2026-08-14.md` for the exact phase
mapping.

1. **Wire the bevy_ui surface into `GameMenu`** — never add a parallel
   menu. The `active_menu.current` switch is the single source of
   truth.
2. **Bespoke chrome → widgets.** Every chip / scrollbar / tooltip /
   tab body / card shell / marquee / progress fill must use the
   `widgets` library. If a primitive doesn't exist, add it to
   `widgets.rs` (or open an issue), do **not** re-roll it in the
   menu's own module.
3. **Theme migration.** Move shared palette tokens to
   `src/ui/bevy_theme.rs`. Keep `theme.rs` (egui) untouched — the
   egui and Bevy-UI palettes coexist.
4. **CI audit hygiene.** Run the existing `audit_bevy_color_literals.py`
   audit (added in v0.5.2 alongside `bevy_theme.rs`) before pushing.
5. **Delete the egui spawners** once Bevy parity tests pass. The
   `theme.rs` egui helpers stay (every other egui panel uses them)
   until those panels migrate.
6. **Docs.** Update `docs/UI.md` (per-panel anatomy + §9.3 primitive
   inventory), `docs/UI_LAYOUT_PATTERNS.md` (§10 + the panel-to-pattern
   table), and `ARCHITECTURE.md` (the `src/ui/` block) **in the same
   PR** as the migration. Doc-only follow-up PRs get deprioritised.

## Known-good widgets inventory (v0.5.2)

These are battle-tested by the Construction canary and ship today.
Port them as the first thing whenever they apply:

- **Hover elevation** — every pickable card / chip gets
  `HoverElevation`; the `tick_ui_hover_elevation` system is registered
  in `UIPlugin::build` once. *Do not* write your own hover system.
- **Scrollbar** — three call sites in Construction (Build, Mining,
  Queue). All delegate to `widgets::Scrollbar`.
- **Tooltip** — four call sites in Construction. `widgets::TooltipRequest`
  is generic (no construction coupling).
- **KeyedList reconciler** — powers the Queue panel and the colony
  dropdown entry list. Use it for *any* keyed entity list.
- **ActiveTabs** — replaces the bespoke tab strip in
  `theme::tab_strip<T>`. Use `widgets::ActiveTabs<T>` for Bevy-UI
  panels; use `theme::tab_strip<T>` only for egui panels.
- **Card composers** — six `card_shell` + sibling composers replace
  what was 5 ad-hoc inline spawn blocks in the canary.
- **Marquee / ProgressFill** — every progress bar in Construction
  uses these. `update_colony_growth`-style and
  `process_company_ai`-style systems just write `ProgressFill(0..1)`.
- **ChipGroup** — replaces 5 ad-hoc chip-row copies in the chip-row
  machinery (phase 2).

## Per-frame budgets (do not regress)

These traps were the cause of every B0001 panic and ~20 s splash stall
on the canary branch. The legacy invariants are documented in
`.github/copilot-instructions.md` "Splash / First-Frame Stall Prevention"
and "Bevy 0.18 dual-Query rule".

- **No per-pixel RGBA loops on 1024² buffers in a single frame.** Batch
  icon bakes (`MAX_ICONS_PER_FRAME = 2` in `src/ui/resource_icons.rs`).
  This is a LINT — see the splash-stall rule for the bisection recipe.
- **No dual-`Query<T>` system parameters** (B0001). Fold queries into
  one `Query<(Entity, &mut T)>` and call `iter()` then `get_mut(entity)`.
  Audit: `python3 scripts/audit_b0001.py src`.
- **No UI Transform-on-`Transform`.** Bevy 0.18 UI scale / translation
  uses `UiTransform`. Writing `Transform` on a UI node is clobbered.

## History (for context)

- **GRA-67** — `theme::Color` mirror (2 Bevy-side primitives).
- **GRA-68..GRA-72** — v2 UI chain (P1..P4, ledger primitives,
  economy + research tabs). Pre-bevy_ui.
- **Phase 1** (`4794803`) — Construction menu lands on Bevy UI
  with the `widgets` library. Mining body graduates from the canary.
- **Phase 1.5b/c/d** (`6e9e8f4`, `9df3460`) — monolith split into the
  `construction/` directory; pure-data helpers moved to
  `colony/building_data`.
- **Phases 2..12b** — primitives (chip rows, click edges, tooltips,
  scrollbars, KeyedList, marquee, progress fills, tab bodies,
  six card composers, ActiveTabs primitive) all consolidated into
  `widgets.rs`. The `phase 12B bevy_ui top bar chrome` experiment was
  reverted (commit `74aec75`) — top-bar chrome stays on egui for now
  because there is no reason to force the migration before other
  surfaces are ready.
- **Snapshot taken on 2026-08-14** (this doc) — Construction is the
  first canary to graduate. The continuation lives at
  `memories/repo/ui-migration-2026-08-14.md`; future menus should
  also add a memory note when they graduate.
