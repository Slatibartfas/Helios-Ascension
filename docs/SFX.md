# Sound Effects (SFX)

The SFX system is the player-facing audio feedback layer for
every UI action and sim event that warrants a dedicated sound.
It mirrors the architecture of the [music playlist](music.md)
but for **one-shot stings** rather than a looping background
track.

## Architecture

```
   assets/data/sfx_manifest.ron   ─┐
                                    │  Startup
                                    ▼
        SfxRegistry (cue + asset-handle map)
                                    │
   UI clicks / notification toast  │  Update
   ──── MessageWriter<SfxEvent>──►│
                                    ▼
   play_sfx_system: filters by cooldown,
   reads `SfxBus` for per-category volume,
   spawns one AudioPlayer per cue
                                    │
                                    ▼
   bevy_audio sink plays + auto-despawns
```

### Components

| Type | Module | Purpose |
|---|---|---|
| `SfxPlugin` | `src/plugins/sfx/mod.rs` | Bevy plugin — registers the loader, bus sync, bridges, and playback system. |
| `SfxManifest` + `SfxCue` + `SfxCueId` + `SfxCategory` | `src/plugins/sfx/mod.rs` | Data model — the manifest schema + the Rust enum that mirrors it. |
| `SfxBus` | `src/plugins/sfx/bus.rs` | Per-category volume routing. Master volume from `PersistentSettings::sfx_volume`. |
| `SfxRegistry` | `src/plugins/sfx/playback.rs` | Live cue metadata + asset-handle map + cooldown tracker. |
| `SfxEvent` | `src/plugins/sfx/mod.rs` | Bevy `Message` — every cue play is one of these. |
| `bridges::UiSfxRequest` | `src/plugins/sfx/bridges/` | UI-side message bus; the UI writes `UiSfxRequest`, the bridge forwards to `SfxEvent`. |

## How to add a new cue

1. **Add the variant to `SfxCueId`** in `src/plugins/sfx/mod.rs`.
   - Update `as_str_id` and `from_str_id` (mirror mappings).
   - Add to the `ALL` constant.
2. **Add the manifest entry** to `assets/data/sfx_manifest.ron`.
3. **Add the prompt** to `assets/data/sfx_prompts.ron`.
4. **Generate the WAV** with `python3 scripts/generate_sfx.py --force`.
5. **Trigger the cue** by writing a `MessageWriter<SfxEvent>` at
   the desired callsite:
   ```rust
   sfx_events.write(SfxEvent(SfxCueId::YourNewCue));
   ```
   Or for UI callsites, write `UiSfxRequest(SfxCueId::YourNewCue)`
   — the bridge forwards to `SfxEvent` automatically.
6. **Verify** with `python3 scripts/audit_sfx_manifest.py --strict`.
   The audit fails CI if the Rust enum and manifest diverge.

## Volume composition

The final volume that an `AudioPlayer` receives is:

```
final = sfx_master × category_volume × cue.default_volume
```

clamped to `[0.0, 1.0]`.

- `sfx_master` — `PersistentSettings::sfx_volume` (player slider,
  persisted to `<userdata>/settings.ron`).
- `category_volume` — `0.0` when the matching notification
  category's `sound_on` toggle is off, otherwise `1.0`. UI /
  Camera / TimeControl / Launch / Persistence categories are
  always at `1.0` (only the master affects them).
- `cue.default_volume` — authored per cue in the manifest to
  balance loud-and-alarming against quiet-and-background.

## Cooldown

Each cue has a `cooldown_ms` (in the manifest) that prevents
saturation on rapid-fire inputs (slider drag, tab spam). The
cooldown is per-cue (not per-channel) and tracked on
`SfxRegistry::cooldowns`. A second `SfxEvent` for the same
cue arriving inside the cooldown window is silently dropped.

## Phase 1 surface

The current PR ships:

- **13 UI cues**: button click, tab switch, panel open/close,
  slider tick, dropdown open, row select, drag/drop, modal
  confirm/cancel, chip toggle, mode toggle.
- **1 universal notification chime**: plays once per *coalesced*
  toast (not per raw event — see
  `src/plugins/sfx/bridges/notifications.rs`).

Construction / research / shipbuilding / fleet / survey /
economy / colony / camera / time-control / launch / persistence
cues land in follow-up PRs.

## Existing wiring (Phase 1)

| Cue | Callsite | File |
|---|---|---|
| `SliderTick` | Time-controls speed preset click + digit hotkeys | `src/ui/dashboard.rs::ui_time_controls` |
| `ModeToggle` | Time-controls pause button + keyboard | `src/ui/dashboard.rs::ui_time_controls` |
| `ModalCancel` | Construction queue cancel button | `src/ui/construction/queue.rs::tick_queue_panel_row_cancel_click` |
| `NotificationChime` | Per-coalesced-toast (auto, via `Added<ActiveNotification>`) | `src/plugins/sfx/bridges/notifications.rs` |

Other UI callsites can be wired in follow-up PRs by adding
`MessageWriter<UiSfxRequest>` to the relevant egui system
function and writing one on the action (see the time-controls
patch for the pattern).

## Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Adds `"wav"` to `bevy_audio` features. |
| `assets/audio/sfx/` | WAV files (modder-overridable). |
| `assets/data/sfx_manifest.ron` | Cue metadata + file paths (modder surface). |
| `assets/data/sfx_prompts.ron` | Natural-language prompts used to generate each cue. |
| `src/plugins/sfx/` | Plugin, loader, bus, playback, bridges, tests. |
| `src/main.rs` | Registers `SfxPlugin`. |
| `scripts/generate_sfx.py` | Generates WAVs (local synthesis default, API stub for prod). |
| `scripts/audit_sfx_manifest.py` | CI audit: Rust enum ↔ manifest ↔ WAV files. |
| `docs/SFX.md` | This file. |
| `docs/SFX_CREDITS.md` | Asset attribution. |

## Known limitations

- **API mode is a stub.** `scripts/generate_sfx.py --api` falls
  back to local synthesis — the actual MiniMax Audio (or
  alternative SFX) API call lands in a follow-up PR. The local
  synthesis produces real, audible placeholder WAVs so the
  audio backend is functional out of the box; production audio
  arrives when the API integration ships.
- **No severity-tier chimes.** All four notification severities
  (Info / Notice / Warning / Critical) play the same chime.
  Splitting them is a Phase 2 follow-up.
- **No music ducking.** The notification chime does not lower
  the music volume. If the chime proves intrusive in
  playtesting, a per-frame `AudioSink::set_volume` on the music
  player would do the ducking.
- **No spatial audio.** Fleet maneuvers do not pan or
  attenuate by camera distance. Strategy-game overhead camera
  makes this overkill.