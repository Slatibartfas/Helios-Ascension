# Mining card multiplier fix (2026-08-03)

> **Historical note (updated 2026-08-15).** The single-file
> `src/ui/construction.rs` referenced below was split into the
> `src/ui/construction/` directory in commit `6e9e8f4` (v0.5.2).
> `build_mine_card_data` now lives in `src/ui/construction/cards.rs`
> (or `src/ui/construction/mining.rs` — both have the relevant
> helpers after the split); `card_data_with_multiplier` lives in
> `src/ui/construction/data.rs`. The link below to
> `[src/ui/construction.rs](src/ui/construction.rs)` is a dead link
> to the deleted monolith; treat the rest of this memory note as
> the historical record of the bug.

**Symptom**: In the Construction menu's Mining tab, the "Produces X
Mt/yr Aluminum/yr" line never scaled with the build-multiplier chip
(×5, ×10, ×25, …) while every other line on the card — Power
demand, resource costs, BP, workforce — did.

**Root cause**: `build_mine_card_data` in [src/ui/construction.rs](src/ui/construction.rs)
(line ~7670) used `count` (already-built mines, the inventory tally
shown in `stat_a` as "×N") instead of `mult` (the player's chosen
build-multiplier). Result: a ×25 build showed the same per-mine
production regardless of multiplier.

The Build tab's `card_data_with_multiplier` (same file, line ~1100)
already had the right pattern: per-unit, ×N, total.

**Fix applied**: Mirrored the Build tab's pattern. `per_unit` is now
`prod.value * accessibility` (per-mine yield after accessibility
modifier), `total = per_unit * mult`. When `mult > 1.0` the line
reads `"Produces <per_unit> X/yr × <N> = <total> X/yr"`. When `mult == 1`
it stays `"Produces <per_unit> X/yr"` for visual consistency with
single-quantity cards.

**Reserves (`Res:` line) intentionally NOT scaled**: Reserves
represent the body's geological endowment, not a per-mine
consumption rate. Multiplying by the build multiplier would imply
"reserves drop by N× when you queue N mines", which is wrong. The
per-mine yield (Produces) is what scales; the deposit pool is
planet-level.

**Lesson**: When adding a new effect line to a `BuildCardData`-style
card, always thread `mult` (build multiplier) into the displayed
value, not `count` (inventory). Audit checklist: every numeric
effect line should show a multiplied value matching the Power line's
`<per_unit> × N = <total>` shape.
