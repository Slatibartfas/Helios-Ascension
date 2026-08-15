# UI Migration Continuation Playbook (2026-08-14)

This note is the operator's continuation guide for the v0.5.2 bevy_ui
migration. Construction graduated on 2026-08-14 (branch
`rework-ui-design`); the next panel to migrate (Research / Economy /
Fleets / Personnel / Notifications / Launch) should start here.

The **meaty** rollout document is
`docs/design/UI_MIGRATION_PLAN.md`. This memory note is the **short
operator-facing checklist + known traps** for the next agent.

## TL;DR

1. **Migrate one panel at a time.** Don't try to bite off two menus.
   The constructors chain was 12+ phases over 5 working days; even
   with the `widgets` library, expect 2–4 phases per menu.
2. **Wire the Bevy-UI surface into the existing `GameMenu` enum.**
   Don't add a parallel `BevyResearchMenu`. `active_menu.current`
   must remain the single source of truth (the v0.5.2 top-bar
   hover-to-expand experiment was reverted in commit `74aec75`
   because it tried to grow a parallel menu system).
3. **Compose from `src/ui/widgets.rs`** — never hand-roll hover,
   scrollbar, tooltip, chip, tab, marquee, progress fill, or
   card chrome. Add a primitive to `widgets.rs` if it's missing.
4. **Egui palette stays in `theme.rs`.** Bevy-UI palette goes in
   `bevy_theme.rs`. Don't duplicate a token across both.
5. **CI audits.** `scripts/audit_bevy_color_literals.py` is the gate;
   it walks `src/` and fails on raw `Color::srgba*` literals
   outside `bevy_theme.rs`. Same idea as the existing
   `audit_color32_literals.py` for egui.
6. **No per-pixel RGBA loops on 1024² buffers in a single frame**
   (the splash-stall trap from `4d4dc23` → `2d3223d`). No
   dual-`Query<T>` system parameters (B0001 — bisect with
   `audit_b0001.py`).

## What the `widgets` library already ships

`src/ui/widgets.rs` (2195 LOC). All menu-agnostic. Full primitive
inventory lives at `docs/UI.md` §9.3.1. The Construction canary uses
**all** of these — anything you might need is already battle-tested.

```text
UiFonts + init_ui_fonts
spawn_scrollable_container / child
spawn_text_label
HoverElevation + tick_ui_hover_elevation
card_shadow / card_shadow_hover / CARD_SHADOW* consts
ChipGroup / ActiveChips + tick_chip_button_hover / overlay / glow
detect_rising_edges / detect_rising_edges_no_marker
TooltipTone/Entry/Content + TooltipRequest/Overlay/Title/Body
  + populate_tooltip_body + tick_tooltip
ScrollbarTrack/Thumb/Metrics/DragState + on_thumb_press/release
  + on_track_press/release + spawn_scrollbar + tick_scrollbar
  + tick_ui_scroll_on_wheel
KeyedList<K, V> + EntityContainer trait
tick_tab_body_visibility
ActiveTabs<K> / TabButton<K> / TabBody<K>
  + tick_active_tab_body_visibility
Marquee + tick_marquee
ProgressFill(pub f32) + tick_progress_fill
CardShellOpts + card_shell / card_icon / card_data_chip
  / card_marquee_subtitle / card_label_value_row / card_footer_cta
```

## Known traps (the ones that bit the canary)

- **`construction.rs` as a single 11 865-LOC file** — the canary era.
  That monolith was split into `src/ui/construction/` (15 files) in
  commit `6e9e8f4`. Don't recreate a similar monolith in another
  menu's module — prefer a directory from the start.
- **`#![allow(dead_code, unused_imports)]` on `construction/mod.rs`.**
  The split introduced broad wildcard re-exports; the comment in
  `mod.rs:30-39` documents the follow-up (Phase 1.5D). Same pattern
  should not be needed in a new menu because you'll be writing
  against the `widgets` library from the start.
- **CSS Grid + flex_grow + Overflow::scroll_y** produces
  single-pixel-tall rows. Use `Flex + FlexWrap::Wrap` with `min_height: 0`
  on the grid AND every flex wrapper above it. Without `min_height: 0`
  the inner scroll overflows upward and the scrollbar thumb hides —
  the "scrollbar dead" trap fixed in `b20ebd0`. Comment lives at
  `construction/cards.rs:5108` (the line-numbered note for the
  SizedGridTrack trap).
- **`Transform` on `Node` children is clobbered.** Bevy 0.18 UI scale
  / translation goes through `UiTransform`. The hover system writes
  `UiTransform` only.
- **BoxShadow without spread vanishes on near-black panels.**
  `widgets::card_shadow()` uses a tight 1-px-spread contact + a soft
  wide cast. Single low-alpha `(0,0,0,0.45)` looks flat. The
  pre-v0.5.3 flat-card bug was this; see `bevy_theme.rs` header
  comment for the v0.5.3 / v0.5.3.5 colour-lift decisions.
- **Per-frame card body icon processing** must be bounded.
  `src/ui/resource_icons.rs` clamps to `MAX_ICONS_PER_FRAME = 2`
  per tick; the splash-stall regression tested that a 20 s
  first-frame dt doesn't trip it. New icon sets follow the same
  pattern (`process_and_downscale_bevy_icon` in
  `src/ui/icon_cache.rs`).
- **`bevy_ui` colour literals outside `bevy_theme.rs` trip the
  CI gate.** Same rule as the egui palette in `theme.rs`. Run
  `python3 scripts/audit_bevy_color_literals.py --strict` before
  opening the migration PR.
- **Construction `tiles 11 12 13 14` (the 5th-to-9th pre-frame
  bodies)** — `construction_canary_per-frame_bodies-2026-08-01.md`
  documents the audit. New menus shouldn't have this trap because
  they reuse the `widgets` library, but if you find yourself
  spawning per-frame entities, bisect by tick rate (skip every
  other frame) to find the offender.

## Phase chain that graduated Construction

For posterity (so the next bevy_ui menu can follow the same sequence):

| Phase | What landed | Key commit |
| ----- | ---------- | ---------- |
| 1 | rework-ui-design pass — `widgets` library + the first bevy_ui pass | `4794803` |
| 1.5b | Split `construction.rs` (11 865 LOC) into `construction/` (15 files) | `6e9e8f4` |
| 1.5c | Pure-data helpers moved to `colony::building_data` | `9df3460` |
| 2 | Chip-row machinery → `widgets` (`ChipGroup` + `ActiveChips`) | `ff6c453` |
| 3a | `detect_rising_edges` helper + first port | `2c41baf` |
| 3b | 7 more click systems ported | `d8790cd` |
| 4 | `TooltipRequest` primitive | `e6a13fa` |
| 4d | All 4 construction tooltips ported | `e7967c5` |
| 5a | `Scrollbar` primitive | `47342b7` |
| 5b | Construction + scrollbar ports | `124709f` |
| 6 | `KeyedList` reconciler + 2 ports | `13dba0d` |
| 6e | `update_overview_queue` port | `dbf8bd0` |
| 7 | Dropdown outside-click dismissal | `de85cbe` |
| 8 | Esc-to-close for dialogs | `5213ba5` |
| 9 | `tick_tab_body_visibility` extracted | `c4eb9ad` |
| 10 | `Marquee` + `ProgressFill` primitives + ports | `3b144fc` / `bc52064` |
| 11a | Six `CardShell*` composers in `widgets.rs` | `f0c33d0` |
| 11b | `spawn_card` rewritten on the 6 composers | `c717304` |
| 11c | Remove dead `CardBundle` references | `7c84473` |
| 12 | `ActiveTabs<T>` primitive | `429fb69` |
| 12B | Top-bar chrome experiment (reverted) | `cd26527` → `74aec75` |

## What the next agent should *not* redo

- Do **not** re-add `src/ui/construction_panel.rs` or
  `src/ui/construction.rs` as a single file. The directory +
  re-export pattern from `construction/mod.rs` is the canonical
  shape for a split module.
- Do **not** re-introduce a per-menu hover system. Use `widgets::HoverElevation`.
- Do **not** re-introduce raw `Color::srgba*` literals. The audit
  would fail.
- Do **not** drop the `Overflow::clip()` + fixed-height card cap
  in favour of "let flex grow". The 320×320 fixed-size card is a
  load-bearing choice; see `construction/cards.rs` for the
  heuristics.

## Snapshot of "what to test" before opening a PR

Before requesting review on a bevy_ui migration:

```bash
cargo check --all-targets            # canonical build gate
cargo test --lib                     # 1100+ tests; must be green
python3 scripts/audit_bevy_color_literals.py --strict src
python3 scripts/audit_b0001.py src   # advisory
```

Clippy is the lint that bit the canary branch (pre-existing clippy
noise in `mining.rs` test code). Don't be tempted to "fix" those
in the migration PR — they belong to a separate housekeeping PR.

## When you graduate (the checklist)

When the migration is done and a new panel is on Bevy UI:

1. Bump this memory note with a new entry: "Panel X graduated on
   YYYY-MM-DD, branch Z, follow this recipe; key issues were W."
2. Append a row to the menu table in
   `docs/design/UI_MIGRATION_PLAN.md`.
3. Update `docs/UI.md` §3 (the per-panel anatomy entry for that
   menu) and §9.3.1 (the primitive inventory — anything new you
   added to `widgets.rs`).
4. Update `docs/UI_LAYOUT_PATTERNS.md` §6 panel-to-pattern table.
5. Update `ARCHITECTURE.md`'s ui/ tree if the menu's module map
   changed.
6. Update `.github/copilot-instructions.md` UI Migration Status
   rule to note this menu is now on Bevy UI + drop the "next
   candidate" hint.

The snapshot itself is in this file — graduate, append, repeat.
