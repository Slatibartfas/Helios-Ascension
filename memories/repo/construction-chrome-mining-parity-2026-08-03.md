# Construction/Mining Chrome Parity (2026-08-03)

> **Historical note (updated 2026-08-15).** The single-file
> `src/ui/construction.rs` referenced below was split into the
> `src/ui/construction/` directory in commit `6e9e8f4` (v0.5.2).
> The chrome helper now lives across `src/ui/construction/setup.rs`
> (the `setup_construction` entry point), `src/ui/construction/cards.rs`
> (the card-shell composers in `src/ui/widgets.rs`), and the shared
> `widgets::ChipGroup` primitive. The `MiningQty` arms were removed
> entirely (chip-row machinery was unified in phase 2 — see
> `commit ff6c453`). Treat the rest of this memory note as the
> historical record of the parity fix.

v0.5.2 PR-A.6 (round 1) extracts a shared `spawn_construction_chrome` helper. PR-A.6 (round 2 — this commit) lifts the chrome out of the per-tab bodies so it lives at the canary root, visible on every tab. Only the body content swaps when the player switches tabs.

## What changed (round 2)

| File | Change |
|---|---|
| `src/ui/construction.rs` (legacy; see `src/ui/construction/`) | Renamed `build_header_stack` → `shared_chrome` (no longer carries `ConstructionTabBody::Build`); new `build_body` (`ConstructionTabBody::Build`) wraps the filter row + card grid; removed `MiningChromeStack` + `ColonyPickerDisplay` + `ChipKind::MiningQty` + `spawn_mining_qty_row` + `MINING_QTY_CHIPS` + `MiningHeader` (chrome is now shared); picker is always interactive; `tick_construction_chip_click` dispatches qty clicks by `selected_tab` to either `build_multiplier` or `mining_build_multiplier` and syncs `active.qty` on tab switches |
| `src/ui/bevy_theme.rs` | Removed `MiningQty` arms from `tick_chip_button_active_overlay` and `tick_active_chip_glow` |

## Architecture before / after

**Before** (chrome duplicated per tab):
```
root
├── tab_strip
├── build_header_stack [ConstructionTabBody::Build]
│   ├── output_row
│   ├── picker
│   ├── qty_row (x1..x100, ChipKind::Qty)
│   └── filter_row
├── card_grid [ConstructionTabBody::Build]
├── overview_body [ConstructionTabBody::Overview]
├── buildings_body [ConstructionTabBody::Buildings]
└── mining_body [ConstructionTabBody::Mining]
    ├── "MINING — Earth (colony)" header
    ├── chrome_stack [MiningChromeStack]
    │   └── duplicate output_row + picker + qty_row (×N, smaller font)
    └── content (scrollable)
```

**After** (chrome shared, only body swaps):
```
root
├── tab_strip
├── shared_chrome (no body marker — always visible)
│   ├── output_row
│   ├── picker (interactive)
│   └── qty_row (x1..x100, ChipKind::Qty)
├── build_body [ConstructionTabBody::Build]
│   ├── filter_row
│   └── card_grid
├── overview_body [ConstructionTabBody::Overview]
├── buildings_body [ConstructionTabBody::Buildings]
└── mining_body [ConstructionTabBody::Mining]
    └── content (scrollable, no chrome, no header text)
```

## v0.5.2 PR-A.7 follow-up: button + dialog polish

### Demolish confirmation dialog

Demolishing mines is destructive — `tick_mining_demolish_click`
now opens a centered modal dialog
(`DemolishConfirmDialog` + `DemolishConfirmYes` /
`DemolishConfirmNo` markers) instead of pushing the
`mining_edits` entry directly. State lives in the
`DemolishConfirmState` resource
(`open: bool, building_type: Option<BuildingType>, count: u32`).
Picking Yes applies the edit and closes the dialog; picking No
(or switching tabs while the dialog is open) closes without
action. `update_demolish_dialog_text` re-reads the live colony
count every frame so the "Demolish N Iron Mines?" label
reflects `min(multiplier, current_count)` — important when the
multiplier exceeds the live count.

### Hover brightness

The original `CTA_FILL_HOVER` and `DEMOLISH_FILL_HOVER` were
too close to the resting fill (subtle 0.04 RGB delta) and
players reported "no hover" on Mining tab buttons. Bumped both
to fully-opaque, much brighter values that read as
unmistakable hover affordances.

### `flex_shrink: 0.0` on bodies

The four `ConstructionTabBody` containers now carry
`flex_shrink: 0.0` in addition to `flex_grow: 1.0`. The default
`flex_shrink: 1.0` was making the active body shrink to 0 px
when its `card_grid` had no children yet (right after a tab
switch while `refresh_card_grid` was still spawning cards).
With `flex_shrink: 0.0`, the body refuses to shrink below its
content height — cards get a stable layout even on the first
frame.

### `card_grid` baseline height

`min_height: Val::Px(200.0)` on `card_grid` so the grid reserves
vertical space inside `build_body` before
`refresh_card_grid` populates. Without it, an empty grid was
collapsing to 0 px and the panel ended right after the filter
row.

### Mining Build button click handler

`tick_construction_cta_click` now dispatches by
`ui_state.selected_tab`: on the Mining tab it uses
`mining_build_multiplier`; on every other tab it uses
`build_multiplier`. Previously the Build (+) button on Mining
cards always used `build_multiplier`, so picking x100 on the
Mining tab still only queued 1 copy.

## Bevy 0.18 patterns learned

### `ChildOf` replaces `Parent`

The parent component in 0.18 is `ChildOf(Entity)` (tuple struct). `parent.0` gives the parent Entity. The old `Entity::get(&Parent)` API doesn't exist on `ChildOf`.

### Single shared chrome is the right answer

Trying to render two pickers (one Build, one Mining) so the chrome stays per-tab leads to the "two-picker + one-dropdown" trap (shared `ColonyDropdownState` open flag, dropdown Y anchored to wrong picker). Lifting the chrome out of any tab body solves it for free — there's only one chrome in the world, one dropdown, one picker. The trade-off is that ALL tabs see the same qty row (Build / Overview / Buildings have a Build qty chip row that's meaningless for them); the v0.5.2 PR-A.6 (round 2) design accepts that because the qty is harmless on read-only tabs and the chip's `interaction` only matters on Build / Mining.

### Shared qty chips route by selected tab

A single `ChipKind::Qty(n)` chip set can drive both `build_multiplier` and `mining_build_multiplier` if the click handler dispatches by `ui_state.selected_tab`. The chip's visual "active" state needs to mirror whichever multiplier the player is currently looking at, so the click handler also writes `active.qty = *n` after each click. Tab switches must also re-sync `active.qty` to the new tab's multiplier — otherwise switching from Mining (x25 selected) to Build leaves the x25 chip highlighted even though Build's `build_multiplier` is still 1.

### Parent-chain walk for per-tab logic

When the same component (`ConstructionCta`) lives on cards in multiple tabs, walking the parent chain via `Query<&ChildOf>` plus a marker filter (`Query<(), With<MiningCard>>`) is the cleanest way to identify which tab a CTA belongs to. Cap the loop at 8 hops — cards in this canary are at most 2-3 levels deep.

## Tests added

No new unit tests (the existing 1001 pass). Manual verification via `cargo build --profile fast` + `cargo run --profile fast` (B0001 trap-free).