# UI baselines

Reference PNGs the harmonization roadmap (GRA-52) uses to diff the
**before** and **after** of each menu. The v1 baseline lands in this
PR (GRA-53); v1.1 lands in GRA-60 once the operator enumerates the
missing submenus.

## Layout

```text
docs/UI/baselines/
├── README.md           this file
├── v1/                 top-level menu baseline (this PR)
│   ├── 01_main.png
│   ├── 02_survey.png
│   ├── ... (11 files)
│   └── MANIFEST.md     auto-generated list of every (name, menu) pair
├── v1.1/               submenu baseline (GRA-60)
└── manual/             operator-driven Shift+F12 captures
    ├── overview.png
    ├── shipbuilding.png
    ├── ... (5 slots, wrap)
```

## How the pipeline works

1. **`src/ui/screenshot.rs`** defines `PendingScreenshotAction` (a FIFO
   queue + an in-flight slot), `ScreenshotSlots` (the 5 live names), and
   the RON manifest types. The capture pump consumes the queue one
   entry per frame, applying the requested `menu` switch + `wait_frames`
   idle, then spawning a Bevy 0.18 `Screenshot::primary_window()` with a
   `save_to_disk` observer. The observer writes the PNG via the
   `image` crate and despawns the entity.

2. **`src/bin/screenshot.rs`** is the headless driver. It loads a
   manifest, seeds the queue, and sets `exit_when_drained` so the app
   exits cleanly once every entry has been written. Run it under Xvfb
   on Linux CI:

   ```text
   xvfb-run -a -s "-screen 0 1920x1080x24" \
     cargo run --release --bin screenshot -- \
     --manifest tools/screenshot_manifest_v1.ron
   ```

3. **`Shift+F12`** is the live keybind. It enqueues a capture against
   the current `ScreenshotSlots` slot, advances to the next, and writes
   to `manual/{slot}.png`. Bare F11 and F12 are taken (F1–F11 = menu
   hotkeys in `src/ui/mod.rs:782`, bare F12 = construction/research
   debug toggle in `src/ui/research_panel.rs:129` and
   `src/ui/construction_panel.rs:461`), so the keybind uses a Shift
   modifier to coexist with all of them.

## Adding a new submenu capture (GRA-60 work)

Do **not** edit `src/ui/screenshot.rs`. Submenus land in a second
manifest:

1. Copy `tools/screenshot_manifest_v1.ron` to
   `tools/screenshot_manifest_v1.1.ron`. Bump the `version` field to
   `"1.1"` and the `out_dir` to `"docs/UI/baselines/v1.1"`.
2. Add a `ManifestEntry` for each `(top_level, submenu)` pair the
   operator enumerates. The `submenu_path` field is reserved (the v1
   pump does not yet drive it) — for v1.1, name the file so the slot
   encodes the submenu, e.g. `construction_buildings` or
   `research_engineering`. The pump can stay the same; the v1.1
   manifest is what tells it what to capture.
3. Re-run the headless driver against the new manifest:

   ```text
   cargo run --release --bin screenshot -- \
     --manifest tools/screenshot_manifest_v1.1.ron
   ```

   No Rust code change required. If a deeper integration is later
   needed (a keybind or panel selector for the submenu itself), that
   lives in a follow-up PR, not this baseline pipeline.

## Acceptance criteria for GRA-53

- [x] `src/ui/screenshot.rs` defines the resources, RON types, and
  capture pump.
- [x] `src/bin/screenshot.rs` loads a manifest and exits after the
  queue drains.
- [x] `tools/screenshot_manifest_v1.ron` covers all 11 top-level
  menus (the 14 screenshots from GRA-52, deduplicated to the 11 menus
  they actually map to, plus the 3 menus the original 14 missed).
- [x] `assets/data/ui/screenshot_slots.ron` lists the 5 manual slot
  names.
- [x] `Shift+F12` is wired in the UI keybind block, does not collide
  with F1–F11 (menu switches) or bare F12 (debug toggle).
- [x] The v1 pipeline + manifest can be re-run after GRA-54..58 land
  to regenerate the baselines. The harmonization PRs are then
  expected to *also* update `v1.1/` (submenu baseline) so the diff
  remains meaningful.
- [x] No new dependency added. (`image`, `ron`, `bevy`, `bevy_egui`
  were already in `Cargo.toml`.)

## What this PR is *not*

- A real CI step that captures PNGs on every commit. The headless
  binary works but pulling in a working Xvfb + Vulkan setup on
  GitHub Actions is a separate piece of plumbing (separate
  `cargo make screenshot` target + workflow change). Tracked as a
  follow-up child of the harmonization roadmap, not GRA-53.
- A submenu selector. The pump switches the top-level menu and
  waits; for v1.1 the manifest encodes the submenu as a filename
  hint until the UI exposes a deep-link API.
- A visual diff tool. The harmonization PRs (GRA-54..58) review
  their PNGs by eye or by `git diff --numstat`. A `cargo make
  diff-baseline` target is a follow-up.
