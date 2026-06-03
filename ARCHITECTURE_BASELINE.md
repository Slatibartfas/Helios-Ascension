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
  -> cargo build --release --locked
  -> cargo test --locked
  -> PR gate: must pass to merge
  -> human co-sign required for main/master
```

---

## 5. Cargo Config Audit

Ryzen 7 5825U (8C/16T):
- `jobs = 4` — safe; leaves headroom for codegen parallelism
- `codegen-units = 16` on release — safe; prevents thread starvation
- No `runner` override for standard targets — correct

**Finding:** No thread over-allocation risk. Config is clean.

---

## 6. Next Steps (LGD-led, DELA-3)

- Rewrite `technologies.ron` for 5-era propulsion tree
- Design `PropulsionSystem` component schema
- Schedule `EguiPrimaryContextPass` → UI overlay in correct phase