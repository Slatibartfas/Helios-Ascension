# Testing Guide

Authoritative testing reference for Helios Ascension: the automated gates,
canonical regression patterns for known-buggy subsystems, and a manual
click-through appendix that still has value as a release-build smoke test.

> **Gates at a glance**
> - `cargo fmt --all -- --check` and
>   `cargo clippy --all-targets --all-features -- -D warnings` — green-build.
> - UI-lint audits + SFX audits — CI-gated `--strict`.
> - B0001 dual-Query advisory — `audit_b0001.py`, print-only (pass `--strict` to fail).
> - Splash / launch timers clamp per-frame dt via
>   `MAX_SPLASH_FRAME_DT_S = 0.25 s` (`src/ui/launch/splash.rs`).

---

## 1. Overview

Helios tests **automated-first**. CI (`.github/workflows/cargo.yml`) gates
merges on four job families: `cargo fmt` → `cargo clippy` → `cargo test --all`
(best-effort, see GRA-165) → UI-lint + SFX audits.

`cargo test --all` is **non-authoritative**: the Bevy 0.18 + bevy_egui test
target hits the 60-min `ubuntu-latest` GHA ceiling. The authoritative gate
is `cargo clippy --all-targets --all-features -- -D warnings`, which
compiles the same crate graph in ~14s and fires on real type/lint
regressions. State-mutation guards prefer resource roundtrips over a full
`App::new()` schedule — see `tests/state_store_v2_e2e.rs`.

**Per-PR workflow:** `cargo fmt` → `cargo clippy` → `cargo test <name>` for
each new test → the `scripts/audit_*.py` for the area you touched.

---

## 2. Automated test suite

### 2.1 Running the suite

```bash
cargo test --all                # full unit + integration suite
cargo test <test_name>          # single test (fast iteration)
cargo test -- --nocapture       # show stdout/stderr
cargo nextest run               # parallel, faster local feedback
```

### 2.2 The 16 integration test files (grouped by domain)

| Domain | Test file | What it pins |
|---|---|---|
| **Buildings** | `tests/buildings_cost_audit.rs` | Cost balance across the 96 building types in `assets/data/buildings.ron`. |
| **Forecast** | `tests/forecast_e2e.rs` | End-to-end mining-forecast pipeline (spectral-class resources, planet_resources generation). |
| **Freighters** | `tests/freighter_templates_data_tests.rs` | `freighter_templates.ron` integrity (mass, thrust, Isp). |
| **Orbital mechanics** | `tests/orbital_mechanics_margin_tests.rs` | Δv / Hohmann / synodic-period numeric tolerances. |
| **Persistence** | `tests/state_store_v2_e2e.rs` | StateStore extract → apply roundtrip; dirty-marker coverage; regen-minimal + `init_missing_resources_for_apply` guard. |
| **Ship hulls / research** | `tests/research_shipbuilding_startup_tests.rs`, `tests/ship_hulls_ron_data_tests.rs` | `ship_hulls.ron` slot layouts, `required_tech` linkage, startup tech/hull visibility. |
| **Stars** | `tests/nearest_stars_ron_data_tests.rs` | `nearest_stars_raw.json` load + validation. |
| **Survey (v0.5.0)** | `tests/survey_anomaly_tests.rs`, `tests/survey_resource_reveal_tests.rs` | Anomaly confidence + reveal pipeline. |
| **Transfer planner** | `tests/gra_153_transfer_planner_fixes.rs`, `tests/porkchop_rotation_no_snap.rs`, `tests/planner_integration.rs`, `tests/transfer_card_unified.rs`, `tests/transfer_porkchop.rs` | GRA-153 fixes, porkchop no-snap rotation, planner e2e, transfer-card unification (goldens in `tests/golden/`). |
| **Notifications** | `tests/notifications_e2e.rs` | Event bus, toast panel, coalesce window. |
| **Test data** | `tests/data/interstellar_propulsion_ron_tests.rs` | Interstellar propulsion RON integrity. |

Shared fixtures: `tests/data/`, `tests/golden/`. The transfer-card tests
compare against five goldens (`transfer_card_cross_star.txt`,
`..._gravity_assist.txt`, `..._interstellar.txt`, `..._porkchop.txt`,
`..._three_option.txt`). A legitimate output change requires deliberate
golden regeneration — never paper over a real regression.

---

## 3. Lint-as-test

Gates, not advisories. Must be green before merge.

### 3.1 Format / Clippy

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

`rustfmt` defaults (no `rustfmt.toml`); ≤ 100 chars/line. `[lints.clippy]`
in `Cargo.toml` allows `too_many_arguments` and `type_complexity` because
Bevy systems/queries are dense — every other lint is on at `-D warnings`.
Prefer a `ParamSet` refactor over adding another allow.

### 3.2 UI-lint audits

Raw colour literals are not permitted outside the theme files.

```bash
# egui — Color32::from_* outside src/ui/theme.rs fails
python3 scripts/audit_color32_literals.py --strict \
    --baseline scripts/audit_color32_literals_baseline.txt src

# Bevy UI — bevy::Color::* outside src/ui/bevy_theme.rs fails
python3 scripts/audit_bevy_color_literals.py --strict \
    --baseline scripts/audit_bevy_color_literals_baseline.txt src
```

Tolerated violations live in the matching `_baseline.txt`; promote them
into `theme.rs` / `bevy_theme.rs` rather than letting the baseline grow.

### 3.3 B0001 dual-Query advisory

A Bevy 0.18 system function MUST NOT declare two `Query<…>` parameters that
both access the same component (e.g. `Query<(Entity, &T)>` + `Query<&mut T>`).
The error fires at runtime on the first schedule tick — `cargo build` /
`cargo test` won't catch it, only `cargo run` does.

```bash
python3 scripts/audit_b0001.py src           # print candidates
python3 scripts/audit_b0001.py src --strict  # fail on candidates
```

Fix by folding into one query (`iter()` then `get_mut(entity)`), using a
`ParamSet`, or applying disjoint `With`/`Without` filters. Canonical
patterns: `process_company_ai`, `auto_freight_loop`, `propagate_orbits`.

### 3.4 SFX audits

3-way sync: Rust `SfxCueId` enum ↔ `assets/data/sfx_manifest.ron` ↔ WAV
files under `assets/audio/sfx/`. Runtime silently drops mismatched manifests
(HashMap keyed by id), so sync failures surface here instead of as missing
cues at runtime. See `docs/SFX.md` "Manifest ↔ enum ↔ files".

```bash
python3 scripts/audit_sfx_manifest.py     # enum ↔ manifest ↔ files
python3 scripts/audit_sfx_coverage.py     # each SFX wired to a trigger
```

---

## 4. Splash / first-frame regression

A ~20 s splash black-box regression was git-bisected to `4d4dc23`
(energy-icon) and fixed in `2d3223d` (2026-08-05). Bisection recipe +
pattern catalogue: [`memories/repo/splash-stall-prevention.md`](../memories/repo/splash-stall-prevention.md).

### 4.1 The invariant

Any "use up real time" system in splash / launch / menu must clamp its
per-frame `dt`. Canonical clamp at `src/ui/launch/splash.rs`:

```rust
pub const MAX_SPLASH_FRAME_DT_S: f32 = 0.25;
let raw_dt = real_time.delta_secs();
let dt = raw_dt.min(MAX_SPLASH_FRAME_DT_S);
```

Without it, `Time<Real>` records a multi-second first-frame stall (DX12 +
custom-shader warm-up, asset IO) as the frame's `delta`, and any
max-duration fallback trips instantly. The player sees the splash live for
~3 s and dismiss — but the timer logged ~23 s of "served time" because it
consumed the stall as dt.

### 4.2 The regression test pattern

```rust
// src/ui/launch/splash.rs::tests::splash_timer_clamps_first_frame_stall_delta
let stall_dt = Duration::from_secs_f32(20.0);
let clamped = stall_dt.min(MAX_SPLASH_FRAME_DT_S);
assert_eq!(clamped, MAX_SPLASH_FRAME_DT_S);   // <-- guard
```

Any new "auto-dismiss after N seconds" timer in splash / launch / menu
must apply the same clamp and ship a similar regression test.

### 4.3 The async-batch pattern

Any system that processes an unknown/large number of items in one `Update`
tick where each item is O(pixels) or O(vertices) must cap per-frame work
and resume on later frames. Canonical pattern in `src/ui/resource_icons.rs`:

```rust
const MAX_ICONS_PER_FRAME: usize = 2;
let mut processed_this_frame = 0usize;
for &resource in ResourceType::all() {
    if processed_this_frame >= MAX_ICONS_PER_FRAME { break; }
    // ... load + process ...
    processed_this_frame += 1;
}
```

1024² × N full-frame RGBA = ~4.2M pixel writes × N items; at 38 items that
exceeds the regression budget by an order of magnitude on any modern CPU.
Rule of thumb: if a transform is "free" because output shape matches input,
question whether it's redundant with another pass.

---

## 5. Save / load regression tests

Helios uses a **regenerate-from-seed + divergence overlay** save format
(`StateStore`). The save persists only bodies whose state diverges from the
regen chain's seed-derived output. Any per-body mutation that the regen
chain would otherwise re-derive silently reverts on load unless the
mutating system marks the body dirty via `ResMut<DirtyBodies>`.

### 5.1 The dirty-marker rule

Every system that mutates a per-body component MUST mark dirty:

```rust
fn my_mutating_system(
    mut bodies: Query<(Entity, &mut LocalStockpile)>,
    mut dirty: ResMut<DirtyBodies>,
) {
    for (entity, mut stock) in bodies.iter_mut() {
        stock.consume(ResourceType::Iron, 1.0);
        dirty.mark_stockpile(entity);  // <-- mandatory
    }
}
```

Adding a new mutating system: pick or add a `DirtyReason` variant in
`src/economy/components.rs`; wire `dirty.mark(...)` in the system; add a
`match` arm in `src/persistence/state_store_extract.rs::extract_bodies`;
append the system to the catalog in `.github/copilot-instructions.md`
"Save-game Compatibility"; add a regression test in
`tests/state_store_v2_e2e.rs` exercising mark + extract end-to-end.

### 5.2 Canonical regression test patterns

Three signatures — each guards a different restore-path regression:

```rust
// 1. regen-minimal fallback — body round-trips even when the restore
//    factory starts with zero body entities.
let mut fresh = build_minimal_world_for_restore();
apply_state_store(&mut fresh, &store);
// → `regenerate_bodies_minimal` rehydrates bodies; without it
//   per-body divergences are silently dropped.

// 2. init-missing-resources fallback — non-default resources
//    (treasury, ViewMode, research state) survive even when the restore
//    factory hasn't seeded them.
world.insert_resource(GlobalBudget { treasury: 12_345.0, ..Default::default() });
let store = extract_state_store(&mut world, 0xCAFE, 0).expect("extract");
let mut fresh = build_minimal_world_for_restore();
apply_state_store(&mut fresh, &store);
assert_eq!(fresh.resource::<GlobalBudget>().treasury, 12_345.0);
// → `init_missing_resources_for_apply` seeds defaults; without it
//   every `get_resource_mut::<T>()` is a silent no-op.

// 3. Frankenstein-world guard — previous live-world session is despawned
//    before the save's entities are swapped in.
swap_world_into(live_world, pending_world);
// → `swap_pending_into_target` calls
//   `despawn_helios_simulation_entities` at the top of the swap.
//   Regression test:
//   `swap_world_into_despawns_helios_simulation_entities_from_target`
```

The harness helper is at the bottom of `tests/state_store_v2_e2e.rs`. Add
new tests next to the existing `state_store_v2_*` family.

---

## 6. Manual smoke tests (appendix)

The original click-through checklist — preserved as a release-build smoke
test or a player-issue reproducer. **Not** a substitute for the automated
gates above.

| # | Check | Expected |
|---|---|---|
| 1 | Hover a body | Glowing cyan ring + top-left tooltip (name in bold cyan, type in gray); scales correctly for moons and planets; smooth fade. |
| 2 | Click body in ledger | Selection ring around it in 3D view; persists; moves when selection changes. |
| 3 | Hover the selected body | Single ring, no duplicate artefacts; hover and selection are independent. |
| 4 | Click anchor (⚓) on a body | Selects + anchors + zoom-to-fit. |
| 5 | Anchor a planet/moon | Body fills ~10 % of screen; Jupiter > Earth; small moons (Phobos) still visible; zoom clamped. |
| 6 | Anchor the Sun | Zooms out to ~40 AU; Mercury–Mars visible (solar-system overview). |
| 7 | Anchor + 100×/1000× time | Camera follows the body smoothly; no jitter; stays centred when paused. |
| 8 | Combined scenario | Hover Mars → ring+tooltip; click Mars in ledger → selection ring; click anchor → zoom+follow; hover Earth → Earth ring+tooltip while Mars selection ring persists. |
| 9 | Performance | Rapid cursor movement across many bodies at 1× and 1000×: no hover lag, smooth render, no leaks across extended sessions. |

---

## 7. Known limitations

1. **Hover labels** — corner tooltip only; future: 3D world-space labels.
2. **Zoom transitions** — instant snap on anchor; future: smooth interpolation.
3. **Multi-body hover** — only the closest body to the cursor highlights.
4. **`cargo test --all` SIGTERM cliff** — Bevy 0.18 test target hits the
   60-min GHA ceiling on `ubuntu-latest`. Authoritative gate is `cargo
   clippy --all-targets --all-features -- -D warnings` (GRA-165 in
   `.github/workflows/cargo.yml`); cliff hits fall under
   `continue-on-error: true`.
5. **Corner tooltip edge cases** — can clip at very small window sizes.

---

## 8. CI pipeline

`.github/workflows/cargo.yml` — two jobs on every push to `main` and every
PR open/synchronize/reopen targeting `main`:

### 8.1 `build` job

- Ubuntu, `timeout-minutes: 60`, sccache + Swatinem/rust-cache.
- Steps: `cargo fmt --check` (gate) → `cargo clippy -D warnings` (gate) →
  `cargo test --all` (best-effort, GRA-165) → `cargo doc --no-deps`
  (best-effort) → `audit_b0001.py src` (advisory). `RUSTFLAGS: -D warnings`
  exported for the whole job.

### 8.2 `ui-lint` job

- Ubuntu, `timeout-minutes: 10`. Steps: `audit_color32_literals.py
  --strict` (gate) → `audit_bevy_color_literals.py --strict` (gate) →
  `audit_sfx_manifest.py --strict` (gate).

### 8.3 `preflight-conflict` job

`.github/workflows/preflight-conflict.yml` runs a 4-arg `git merge-tree` to
catch stale-branch / textual-conflict cases that show "CI green but
mergeable=CONFLICTING". On failure:

```bash
git fetch origin main && git rebase origin/main &&
git push --force-with-lease=refs/heads/<branch>:<expected-sha>
```

### 8.4 Required local pre-PR checklist

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test <name>                                            # any new test you wrote
python3 scripts/audit_color32_literals.py --strict \
    --baseline scripts/audit_color32_literals_baseline.txt src
python3 scripts/audit_bevy_color_literals.py --strict \
    --baseline scripts/audit_bevy_color_literals_baseline.txt src
python3 scripts/audit_b0001.py src                           # candidates, expected
python3 scripts/audit_sfx_manifest.py && \
    python3 scripts/audit_sfx_coverage.py                    # if SFX touched
```

---

## See also

- [`memories/repo/splash-stall-prevention.md`](../memories/repo/splash-stall-prevention.md)
  — splash regression bisection recipe + invariant catalogue.
- [`.github/copilot-instructions.md`](../.github/copilot-instructions.md)
  "Save-game Compatibility" — `DirtyReason` catalog + per-system mark
  contract.
- [`CLAUDE.md`](../CLAUDE.md) "Bevy 0.18 Specifics" — B0001 dual-Query
  rule + canonical fixes.
- [`docs/UI.md`](./UI.md) — colour-token policy; `theme.rs` /
  `bevy_theme.rs` are the only authorised homes for raw literals.
- [`docs/SFX.md`](./SFX.md) — SFX manifest ↔ enum ↔ files contract.
