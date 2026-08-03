# Construction chip tooltip — stale-state + offset (2026-08-03)

## Bug summary

The `ResourceCostChip` hover tooltip overlay in the Construction canary
(`src/ui/construction.rs::update_resource_cost_tooltip`) had two
distinct defects reported by the player on the rework-ui-design branch:

1. The tooltip sat too far below the cursor (player: "offset quite a lot
   downwards") even though the per-frame code placed it at `cursor + 8`.
   The chips are 28 px tall, the tooltip is ~24 px tall — placing the
   tooltip *top* 8 px below cursor meant the tooltip's *vertical center*
   sat ≈32 px below the chip's vertical center. Subjectively "way below".

2. The tooltip did not disappear when the player switched sub-views
   (Build → Mining) or multiplier chips. The `Pointer<Out>` observer
   fires `on_chip_hover_out` only when the cursor leaves a still-live
   entity; when the chip is cascade-despawned (which happens every
   `update_mining_body` / `update_buildings_body` tick that re-spawns
   cards), `Pointer<Out>` is **not** fired for the vanished entity, so
   `ResourceCostHoverState.chip` retained its stale `Some(...)` reference
   and the per-frame system kept painting the overlay.

## Fix shape (applied 2026-08-03)

`src/ui/construction.rs::update_resource_cost_tooltip`:

- Reduces the vertical offset from `+8.0` → `+4.0`. Horizontal stays at
  `+8.0`. The tooltip now hugs the chip's bottom edge instead of
  floating below it.
- Adds three defensive hide-overlay paths:
  1. Menu active check — `active_menu.current != GameMenu::Construction`
     clears `hover_state.chip` and hides the overlay.
  2. Stale-entity check — `chip_query.get(data.entity).is_err()` clears
     `hover_state.chip`. This catches the despawn-during-hover race for
     Build ↔ Mining sub-tab switches, multiplier chip flips, colony
     switches, and queue-row despawns.
  3. Cursor off-screen — the existing `cursor_position().is_none()`
     early-out.

## B0001 audit notes

The fix folds `ResourceCostHoverState` into a single `ResMut` parameter
(avoiding the `Res` + `ResMut` pair that would trigger B0001). The
`chip_query` is the only `Query<&ResourceCostChip>` so it doesn't share
mutable access with the `Single<&mut Node>` / `Single<&mut Text>` /
`Single<&mut TextColor>` parameters. `python scripts/audit_b0001.py src`
reports the tooltip system as `[info]` (not `[RISK]`).

## Why not anchor to chip screen position

A more thorough fix would query the chip's `UiTransform.translation`
+ `ComputedNode.size` + walk the parent chain to compute the chip's
absolute screen position (the Bevy 0.18 `UiGlobalTransform` component is
available via `bevy::ui::UiGlobalTransform` but is not currently used
anywhere in this codebase — first-use would warrant its own change). For
the player-reported issues (offset + stale state), a cursor-relative
placement with a tighter offset is sufficient; chip-anchored placement
remains a future improvement tracked under the same area.

## Round 2 (2026-08-03, same session) — coordinate-frame bug

After round 1, the player reported the tooltip rendered at the
**bottom-right of the card**, never near the cursor. Screenshot showed
the tooltip ~200 px below the actual chip the cursor was over. Root
cause: the canary root that owns the overlay is positioned at
`top: 126.0; bottom: 72.0` so it lives below the top resource-bar
chrome. `Node::left` / `top` on absolutely-positioned descendants are
measured against the *parent's content-area origin*, not against the
window. `Window::cursor_position()` returns **window** coords. The first
round of the fix subtracted nothing, so the overlay landed at
`cursor.y + 4 + 126` (rounded to `bottom: 72` clamp). The visual
effect: the tooltip was always 126 px below the cursor.

The setup_construction comment block had this wrong: it said the canary
"spans the full window" and so `Val::Px(x)` on absolute children
maps to window coords. That's only true if the parent has
`top: 0; left: 0` — but `setup_construction` sets `top: 126` to clear
the global chrome. Future readers were guaranteed to trip on this.

### Round-2 fix

In `update_resource_cost_tooltip`, subtract a `CANARY_ROOT_TOP_PX`
constant (126.0) from `cursor.y` before assigning
`overlay_node.top`. Also clamp against the canary-root content-area
dimensions (window minus 126 top + 72 bottom, not raw window) so the
tooltip can't escape the canary root when the cursor is near the bottom.

The stale-state / menu guard from round 1 stayed in place — they're
independent bugs. Round 2 also updates the now-misleading comment in
`setup_construction` to spell out the coordinate-frame gotcha so the
bug doesn't get re-introduced if someone moves the canary root.

### Pattern (generalises)

When an absolutely-positioned overlay is parented to a node that is
itself anchored (e.g. its own `top`/`left` set to non-zero), remember
that `Node::top` on the overlay is in the *parent's content-area space*.
`Window::cursor_position()` always returns *window* coords. The two
coordinate systems differ by the parent's `top`/`left` constant. Either
subtract the constant every frame (what we did here), or make the
overlay a sibling of the anchored node by re-parenting during setup.

This same trap applies to any future tooltip / overlay / popup added
to `src/ui/construction.rs` — the canary root's `top: 126` will hit
every first-time author.
