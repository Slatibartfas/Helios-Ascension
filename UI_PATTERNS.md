# UI Patterns (egui)

**Author:** CTO
**Date:** 2026-06-04
**Status:** v1 — initial patterns, established by DELA-4
**Implements:** `ARCHITECTURE_BASELINE.md` §2 "Egui Context Pass"
**Audience:** any future contributor adding an egui-driven panel or overlay.

This document captures the canonical pattern for adding a new egui surface
to Helios Ascension. It exists so that the second, third, and Nth panel do
not each re-derive the schedule, the context access, and the styling
convention. If a future panel needs to deviate, document the deviation here
and link back to it from the new code.

---

## 1. The schedule: `EguiPrimaryContextPass`

The architecture baseline (§2) declares that `EguiPrimaryContextPass` is the
sole egui context owner. **All UI is drawn in systems scheduled into
`EguiPrimaryContextPass`.** Do not introduce a parallel schedule.

The import is `bevy_egui::EguiPrimaryContextPass` — pulled in from the
`bevy_egui` crate, not defined locally. Bevy 0.18 ships the pass as part of
`bevy_egui::EguiPlugin`, which is added once in `main.rs`:

```rust
.add_plugins(EguiPlugin::default())
```

after which every UI system schedules into the pass via:

```rust
app.add_systems(EguiPrimaryContextPass, my_panel_system);
```

## 2. The context: `EguiContexts::ctx_mut()`

The egui `Context` is **not** a global; every system that draws UI takes
`EguiContexts` as a parameter and obtains the primary context via
`contexts.ctx_mut()`. The baseline explicitly forbids creating a second
`egui::Context` instance, and a separate `EguiUserApp` plugin is not used.

Canonical pattern:

```rust
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

pub fn my_panel_system(mut contexts: EguiContexts, /* …resources… */) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return, // context not yet initialised; bail
    };

    egui::Window::new("My Panel").show(ctx, |ui| {
        // …draw widgets…
    });
}
```

Two rules fall out of this:

1. **Bail on `Err(_)`** rather than `unwrap()`. The context is unavailable
   during the very first frames (before the egui plugin has fully booted
   the primary context); a panel that panics on `Err` can deadlock the
   app at startup.
2. **Take `EguiContexts` by value once per system.** Each call to
   `ctx_mut()` borrows the same primary context; do not nest calls or
   hold the borrow across an `await`-like boundary.

## 3. Styling: `crate::ui::theme`

Colour constants, font helpers, and frame builders live in
`src/ui/theme.rs`. Panels should pull from `theme::*` rather than reaching
for raw `Color32::from_rgb(…)` values so that the Tactical OS aesthetic
stays consistent. The currently used surface set:

* `theme::BG`, `theme::BG_SOLID`, `theme::SURFACE`, `theme::SURFACE_RAISED` — panel and card fills.
* `theme::ACCENT`, `theme::ACCENT_DIM`, `theme::BORDER` — primary cyan, dimmed outline, hairline border.
* `theme::TEXT`, `theme::TEXT_VALUE`, `theme::TEXT_DIM`, `theme::TEXT_HINT` — text gradient.
* `theme::GREEN`, `theme::AMBER`, `theme::RED` — semantic state colours.
* `theme::RP_BLUE`, `theme::EP_TEAL`, `theme::STAR_GOLD` — domain-specific accents (research points, engineering points, stellar labels).
* `theme::heading()`, `theme::title()`, `theme::body(size)`, `theme::mono(size)` — font helpers tied to the font setup in `setup_egui_fonts`.
* `theme::panel_frame()`, `theme::central_frame()`, `theme::section_frame()`, `theme::elevated_frame()`, `theme::tooltip_frame()` — pre-styled `egui::Frame` builders.

Floating `Window`s (like the data-verification surfaces added in DELA-4)
should use `theme::panel_frame()`; full-screen menus (like the Research
panel) should use `theme::central_frame()`.

## 4. System sets: `UiSystemSet`

`src/ui/mod.rs` defines an `enum UiSystemSet` with three variants that
group systems by z-order and stacking layer:

```rust
enum UiSystemSet {
    TopBar,    // resource bar, top menu, time controls
    MainPanels,// dashboard, dossier, research, construction, economy, fleets
    Overlays,  // tooltips, floating labels, resolution warning, transfer planner
}
```

The set is `configure_sets(...).chain()`-ed on `EguiPrimaryContextPass`, so
`TopBar` runs before `MainPanels`, which runs before `Overlays`. New
player-facing panels should pick the set that matches their layer:

* Resource / status bar → `TopBar`
* A panel that occupies a large region of the screen → `MainPanels`
* A floating tooltip or transient popup → `Overlays`

A panel that does not naturally fit any of these (e.g. a debug-style
"data inspection" surface that should always be visible) can be added
without a set; it will run in the pass in registration order, after the
configured set chain. The first such panel is `technologies_panel_system`
(DELA-4).

## 5. Module layout

Each panel lives in its own file under `src/ui/`:

```
src/ui/
├── mod.rs            # UIPlugin, UiSystemSet, shared state
├── theme.rs          # palette + frame builders
├── dashboard.rs      # MainPanels
├── research_panel.rs # MainPanels
├── technologies_panel.rs # first egui panel (DELA-4); no set
└── …
```

Conventions:

* File name is `snake_case` and ends in `_panel` for full-screen panels,
  no suffix for overlays and helpers.
* Each file is `mod foo;` (private) unless it needs to be reached from
  another module — `pub mod` is the exception, not the rule.
* A panel module exports a `pub fn foo_panel_system(...)` and the file's
  top-level `use` block pulls from `super::*` so it can reach
  `EguiPrimaryContextPass`, `theme`, and the shared `ActiveMenu` /
  `GameMenu` types without an extra import path.

## 6. Adding a new panel: the recipe

1. Create `src/ui/<name>_panel.rs` with a `pub fn <name>_panel_system`
   that follows §1–§2 above.
2. In `src/ui/mod.rs`:
   * Add `mod <name>_panel;` near the other `mod …;` lines.
   * Add `.add_systems(EguiPrimaryContextPass, <name>_panel::<name>_panel_system[in_set(UiSystemSet::<Layer>)])`
     inside `UIPlugin::build`. Place it near other systems in the same
     layer to keep the registration order predictable.
3. Use `theme::*` for colour, fonts, and frames. Do not introduce a
   parallel palette.
4. If the panel needs player interaction (start research, queue build,
   select fleet), push the intent into a `Pending*Actions` resource and
   process it in a non-egui `Update` system, so the panel stays a pure
   read/write-of-resources layer.
5. If a deviation from this pattern is necessary, document it here with
   a short subsection and link it from the panel's source.

---

## 7. Worked example: `technologies_panel_system` (DELA-4)

This is the first system built from this pattern, so its source is a
useful reference. It reads `Res<TechnologiesData>` (a resource inserted
by `DataLoaderPlugin`, see `DATA_LOADER.md`), iterates the loaded tech
list, and renders a `egui::Window` with a `ScrollArea` and a `Grid` of
tech rows. Key conformance points:

* The system takes `EguiContexts` and calls `contexts.ctx_mut()`, never
  constructing a second `egui::Context`.
* The system is registered with `.add_systems(EguiPrimaryContextPass, …)`
  and is **not** gated by `UiSystemSet::*` because it is a debug-style
  data surface, not a player-facing menu panel.
* It uses `theme::panel_frame()`, `theme::title()`, `theme::body(...)`,
  `theme::mono(...)`, and the standard `theme::TEXT_*` / `theme::ACCENT`
  colours.
* It sorts the `HashMap` of technologies by `(tier, id)` before drawing
  so the row order is deterministic across runs.

Future panels should look at `src/ui/technologies_panel.rs` first and
treat it as the canonical minimal example.
