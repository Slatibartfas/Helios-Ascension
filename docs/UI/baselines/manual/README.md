# Screenshot baselines (manual capture)

This directory holds PNGs captured by the live `Shift+F12` keybind
implemented in `src/ui/screenshot.rs` and `src/ui/screenshot_state.rs`.
Captures rotate through five named slots and write the result to this
directory. The slot list is the default; `load_slots` overrides it from
`assets/data/ui/screenshot_slots.ron` if present.

## Default slots

| File | Menu to capture |
|------|-----------------|
| `overview.png` | Main dashboard / overview of the current game state |
| `shipbuilding.png` | Logistics Hub with a hull selected |
| `research.png` | Research panel with the tech tree at default zoom |
| `construction.png` | Construction panel showing building cards |
| `starmap.png` | Starmap view of the active star system |

## Workflow

1. Run the game locally (`cargo run` or `cargo run --release`).
2. Open the menu you want to capture.
3. Press **Shift+F12**. The capture writes to the next slot
   (wrapping back to `overview.png` after `starmap.png`).
4. Commit the PNG alongside any UI change that affected the look.

## When to re-capture

- After any change to `src/ui/theme.rs` (palette, spacing, frames).
- After any change to a panel that visibly re-arranges the layout.
- After any change that adds, removes, or restyles a focus ring.

Baselines are not auto-tested today — the rule is "if a reviewer asks
for a screenshot, you should have one ready." A manifest-driven
capture pipeline is parked as a GRA-60 follow-up.
