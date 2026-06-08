# UI baselines

Reference PNGs the harmonization roadmap (GRA-52) uses to diff the
**before** and **after** of each menu. v1 is captured manually (see
"How to capture" below); v1.1 is a follow-up tracked in GRA-60 once
the operator enumerates the missing submenus.

## Layout

```text
docs/UI/baselines/
├── README.md           this file
└── manual/             operator-driven Shift+F12 captures
    ├── overview.png
    ├── shipbuilding.png
    ├── ... (5 slots, wrap)
```

## How to capture

1. Run the game locally (`cargo run` or your usual entry point).
2. Open the menu you want to baseline.
3. Press `Shift+F12` to write `docs/UI/baselines/manual/{slot}.png`
   against the current slot. The slot index advances each press,
   wrapping at the end. Default slots are `overview`,
   `shipbuilding`, `research`, `construction`, `starmap`. The slot
   list can be overridden via `assets/data/ui/screenshot_slots.ron`.
4. Wait ~10 frames (~0.17 s at 60 fps) for the render-thread
   observer to write the PNG before pressing again.
5. `git add docs/UI/baselines/manual/` and commit.

Why manual: a previous headless `cargo make screenshot` target
existed (and was removed 2026-06-09) but its test-target compile
footprint exceeded the GitHub Actions runner's 30-min window. The
capture pipeline is unchanged — `Shift+F12` still spawns a Bevy
0.18 `Screenshot::primary_window()` with a `save_to_disk` observer.
Re-introducing a manifest driver is parked; if it returns, the
manifest reuses this same pipeline and adds zero new Bevy features.

## Why `Shift+F12`

F1–F11 are bound to menu switches in `src/ui/mod.rs:786-796`; bare
F12 is the construction/research debug toggle in
`src/ui/research_panel.rs:129` and
`src/ui/construction_panel.rs:461`. `Shift+F12` is the only clean
slot in that family.

## Submenus

Construction (buildings / ships / defenses), Research (tech tree /
available / engineering), Economy (logistics / mining / resources /
...), Fleets (list / details), etc. are all captured manually. The
slot name is up to the operator; the harmonization PRs (GRA-54..58)
diff against whatever PNG is in `docs/UI/baselines/manual/`. If a
deeper integration is later needed (a keybind or panel selector for
the submenu itself), that lives in a follow-up PR, not this baseline.

## Acceptance criteria for GRA-53

- [x] `src/ui/screenshot.rs` defines the resources, queue, and
  capture pump.
- [x] `assets/data/ui/screenshot_slots.ron` lists the 5 manual slot
  names.
- [x] `Shift+F12` is wired in the UI keybind block, does not collide
  with F1–F11 (menu switches) or bare F12 (debug toggle).
- [x] No new dependency added. (`image`, `ron`, `bevy`, `bevy_egui`
  were already in `Cargo.toml`.)
- [x] `cargo test --all` stays green (the previous headless bin
  forced the test target to re-compile the full Bevy render
  pipeline; its removal restores the prior cache hit).

## What this PR is *not*

- A real CI step that captures PNGs on every commit. The headless
  bin works in principle but pulling in a working Xvfb + Vulkan
  setup on GitHub Actions is a separate piece of plumbing
  (separate `cargo make screenshot` target + workflow change).
  Tracked as a follow-up child of the harmonization roadmap, not
  GRA-53.
- A submenu selector. The pump captures whatever is on screen when
  `Shift+F12` fires; the operator chooses the menu.
- A visual diff tool. The harmonization PRs (GRA-54..58) review
  their PNGs by eye or by `git diff --numstat`. A `cargo make
  diff-baseline` target is a follow-up.
