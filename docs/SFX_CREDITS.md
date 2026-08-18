# SFX Credits

Every sound effect bundled with Helios Ascension. The 13 UI
cues and 1 notification chime are AI-generated via the audio
generation pipeline (`scripts/generate_sfx.py`). All cues ship
under the same MIT license as the rest of the project.

| Cue | Duration | Recipe (synthesized placeholder) | API cue (when wired) |
|---|---|---|---|
| `ui.button_click` | 80 ms | 880→1320 Hz sweep, fast decay | TBD |
| `ui.tab_switch` | 120 ms | 660 Hz sine, fast decay | TBD |
| `ui.panel_open` | 280 ms | 220→880 Hz rising sweep | TBD |
| `ui.panel_close` | 240 ms | 880→220 Hz falling sweep | TBD |
| `ui.slider_tick` | 60 ms | 1320 Hz square wave | TBD |
| `ui.dropdown_open` | 140 ms | 990 Hz sine | TBD |
| `ui.row_select` | 100 ms | 440+660 Hz two-tone chord | TBD |
| `ui.drag_drop` | 180 ms | 220 Hz low sine (thud) | TBD |
| `ui.modal_confirm` | 320 ms | 660→880 Hz ascending | TBD |
| `ui.modal_cancel` | 280 ms | 880→660 Hz descending | TBD |
| `ui.chip_toggle` | 70 ms | 1100 Hz square wave | TBD |
| `ui.mode_toggle` | 160 ms | 330 Hz deep thud | TBD |
| `notifications.chime` | 600 ms | 440+660+880 Hz three-note chord | TBD |

The exact natural-language prompts used to (eventually) generate
the production audio are archived in
[`assets/data/sfx_prompts.ron`](../assets/data/sfx_prompts.ron).

## Phase 2+ (planned)

When the API integration lands, this file will gain columns
for **provenance** (which API / model produced the cue), **license**
(usually the API's standard reuse terms), and **SHA-256 hashes**
of the bundled WAVs for tamper detection.