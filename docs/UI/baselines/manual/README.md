# Screenshot baselines (manual capture)

This directory holds PNGs captured by the live `Shift+F12` keybind
implemented in `src/ui/screenshot.rs` and `src/ui/screenshot_state.rs`.
The capture handler auto-picks the slot from the active
`GameMenu` (in-game) or `LaunchState` (main menu / subviews), so
each press writes to the file matching the menu the player is
currently looking at.

For the full capture sequence see
[`../../audit/MENU_AUDIT_RUNBOOK.md`](../../audit/MENU_AUDIT_RUNBOOK.md).

## Default slot map

The slot list lives in
`src/ui/screenshot_state.rs::ScreenshotSlots::default`. The default
order is the deterministic F1–F11 audit walk (with the launch
subviews prepended):

| File                       | Where to be when you press Shift+F12              |
|----------------------------|---------------------------------------------------|
| `main_menu.png`            | Main menu shell                                    |
| `new_game_subview.png`     | New Game subview (press `2` on the main menu)      |
| `load_game_subview.png`    | Load Game subview (press `3` on the main menu)     |
| `settings_subview.png`     | Settings subview (press `4` on the main menu)      |
| `save_subview.png`         | In-game Save panel                                 |
| `survey.png`               | In-game Survey / dossier (F1)                      |
| `starmap.png`              | In-game Starmap (F2)                               |
| `settings.png`             | In-game settings panel (F3)                        |
| `construction.png`         | Construction panel (F4)                            |
| `research.png`             | Research panel (F5)                                |
| `fleets.png`               | Fleets panel (F6)                                  |
| `shipbuilding.png`         | Shipbuilding workspace (F7)                        |
| `economy.png`              | Economy panel (F8)                                 |
| `personnel.png`            | Personnel Roster (F9)                              |
| `intel.png`                | Intel panel (F10)                                  |
| `diplomacy.png`            | Diplomacy panel (F11)                              |

`load_slots` overrides the list from
`assets/data/ui/screenshot_slots.ron` if present; see
`ScreenshotSlots::default` for the schema.

## Workflow

1. Run the game locally (`cargo run --release`; debug builds show
   the Bevy inspector overlay).
2. Open the menu you want to capture.
3. Press **Shift+F12**. The capture writes to the file that
   matches the active menu; each press also advances the cursor so
   the next round-robin capture lands on the next slot when the
   active menu does not match a fixed slot.
4. Commit the PNG alongside any UI change that affected the look.

## When to re-capture

- After any change to `src/ui/theme.rs` (palette, spacing, frames).
- After any change to a panel that visibly re-arranges the layout.
- After any change that adds, removes, or restyles a focus ring.

Baselines are not auto-tested today — the rule is "if a reviewer
asks for a screenshot, you should have one ready." A manifest-driven
capture pipeline is parked as a GRA-60 follow-up.
