# Bevy 0.18 Architecture Baseline

**Author:** CTO
**Date:** 2026-06-03
**Status:** v1 — initial architecture lock

---

## 1. ECS Schedule

Bevy 0.18 uses a parallel ECS scheduler. The canonical loop per frame:

```
start:
  -> apply_deferred
  -> last_systick_check
  -> enter (ordered systems)
  -> run (parallel system pairs via dispatcher)
  -> exit
  -> exit_systick_check
  -> last_tick_before_view_change
end:
  -> apply_deferred
```

**Invariant:** All user-defined systems must live inside the `enter` → `run` → `exit` window. No game logic in `apply_deferred`.

---

## 2. Egui Context Pass

**Invariant:** `EguiPrimaryContextPass` must be the sole egui context owner.

- Placed in `App::build()` after all game systems
- No secondary `EguiUserApp` instances
- All UI drawn in systems scheduled after `EguiPlugin`

---

## 3. RON Data Pipeline

```
assets/data/*.ron
  -> ron::de::from_reader()
  -> App::resources (insert)
  -> Systems query via Query<&Component>)
```

Canonical files:
- `technologies.ron` — era progression, propulsion types
- `ship_modules.ron` — module templates
- `ship_hulls.ron` — hull templates and stats

**Rule:** No RON layout changes without CTO + LGD joint review.

---

## 4. CI Loop

```
push / PR opened
  -> Ubuntu latest runner
  -> Install Rust stable (dtolnay/rust-toolchain)
  -> Install Linux system deps (libwayland-dev, libxkbcommon-dev, libx11-dev, etc.)
  -> cargo build --release --locked
  -> cargo test --locked
  -> PR gate: must pass to merge
  -> human co-sign required for main/master
```

**System dependencies note:** Bevy uses Wayland/X11 on Linux. CI must install `libwayland-dev`, `libxkbcommon-dev`, `libx11-dev`, `libxcb-*` and `pkg-config` before `cargo build`.

---

## 5. Cargo Config Audit

Existing `.cargo/config.toml` (unchanged from main):

- LLD linker for `x86_64-unknown-linux-gnu` — 2-5x faster linking
- No `jobs` override — Bevy compilation defaults to safe value
- No `codegen-units` override — release profile in `Cargo.toml` controls it (`codegen-units = 1`)
- No `runner` override for standard targets

**Audit finding:** No thread over-allocation risk. Config is clean and correct for CI.

---

## 6. Next Steps (LGD-led, DELA-3)

- Rewrite `technologies.ron` for 5-era propulsion tree
- Design `PropulsionSystem` component schema
- Schedule `EguiPrimaryContextPass` → UI overlay in correct phase