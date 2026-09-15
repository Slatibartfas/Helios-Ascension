# AGENTS.md

A 4X grand strategy game with realistic orbital mechanics and a big focus on resource management, logistics, and research. Climb the Kardashev scale starting at 0.7 and expand your civilization across the stars. Built on Rust + Bevy 0.18. License: MIT.

## Critical rules (read first)

Three invariants have caused real damage in this repo. Read them before any worktree mutation, any system that mutates per-body state, or any icon/asset/launch-path work.

### Multi-Agent Worktree Safety

This repo is frequently worked on by **multiple agents in parallel worktrees** (Copilot sessions, Claude Code, subagents, CI bots), often sharing `target/` and `Cargo.lock`. Naive worktree mutations have repeatedly destroyed concurrent work.

**Never run in a shared worktree:** `git stash` (any form, including `-u` / `--include-untracked`); `git checkout -- <path>`, `git restore --source=HEAD <path>`, `git reset --hard`, or `git clean -fdx` without an explicit `--` path scope and a dry-run first; `git switch` / `git checkout` to a different branch; `cargo clean`, `rm -rf target/`, `rm Cargo.lock`. Stashing a shared worktree can sweep up another agent's uncommitted edits, and "verifying a pre-existing failure" by stashing is a known foot-gun.

**Safe alternatives:** `git worktree add ../<branch>-worktree <branch>` for hermetic work; re-run failing tests on a fresh `main` worktree to confirm pre-existence rather than stashing; `cargo clean -p <specific-package>` instead of full `cargo clean`. See `.github/copilot-instructions.md` "Multi-Agent Worktree Safety" for the full decision table.

### Save-Game Dirty-Marker Rule (GRA-358 PR-I)

Helios uses a **regenerate-from-seed + divergence overlay** save format. Any per-body mutation that the regen chain would otherwise re-derive **silently reverts on load** unless the system marks the body dirty via `ResMut<DirtyBodies>`. Every system that mutates a per-body component MUST call `dirty.mark_stockpile(entity)` (or `dirty.mark(entity, DirtyReason::*)`).
`LaunchCapacity` follows the same contract: `process_ship_launches_and_completions` marks the body `DirtyReason::LaunchCapacity` (and `Stockpile`) when a surface ship commits a launch. The v2 extract path reads `LaunchCapacity` whenever the body carries the component or is dirty with `LaunchCapacity`/`Multiple`. The apply path inserts `LaunchCapacity` from `BodyDivergence::launch_capacity_override`. Older saves without the field skip the insert and the provisioning system bootstraps a 25 %-charged balance on first run.
When adding a new mutating system: pick or add a `DirtyReason` variant in `src/economy/components.rs`; wire `dirty.mark(...)` in the system; add a `match` arm in `src/persistence/state_store_extract.rs::extract_bodies`; append the system to the catalog in `.github/copilot-instructions.md` "Save-game Compatibility"; add a regression test in `tests/state_store_v2_e2e.rs`. The full catalog of mutating systems and `DirtyReason` variants lives there.

### Splash / First-Frame Stall Prevention

A ~20 s splash black-box regression was bisected to a single full-frame per-pixel loop on a 1024² buffer (commits `4d4dc23` → `2d3223d`). **Do not silently re-introduce the pattern.** Any system that processes an unknown/large number of items per tick where each item is O(pixels) or O(vertices) must cap its per-frame work (canonical pattern: `MAX_ICONS_PER_FRAME = 2` in `src/ui/resource_icons.rs`) and resume on later frames. No per-pixel RGBA loops on 1024×1024 buffers in a single frame. Splash / launch timer systems must clamp per-frame dt (`MAX_SPLASH_FRAME_DT_S = 0.25 s` in `src/ui/launch/splash.rs`) and ship a regression test asserting a 20 s simulated first-frame dt does not trip the max duration. Bisection recipe: `memories/repo/splash-stall-prevention.md`.

## Setup commands

- Toolchain: `rust-toolchain.toml` pins **Rust 1.94.0** — `rustup` fetches it on first build. Required components: `rust-src`, `rustfmt`, `clippy`, `rust-analyzer`.
- Quick iteration: `cargo check` (fastest, no codegen).
- Install deps / build: `cargo build` or `cargo build --profile fast` (Bevy-optimized; `.cargo/config.toml` already wires sccache + rust-lld).
- Local playtest: `cargo run --profile fast` (recommended over release for day-to-day).
- Release build: `cargo build --release` (only for packaging, profiling, or final optimization).
- Lint: `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`.
- Test: `cargo test --all` — CI runs with `continue-on-error: true` (the full Bevy test target hits the GHA 60-min ceiling; `clippy` is the authoritative gate). Faster local runs: `cargo test <name>` or `cargo nextest run`.

## Project layout

- `src/astronomy/`, `src/colony/`, `src/economy/`, `src/fleets/`, `src/personnel/`, `src/research/`, `src/shipbuilding/`, `src/ships/`, `src/survey/`, `src/plugins/`, `src/render/`, `src/ui/`, `src/persistence/` — file-by-file module breakdown lives in `CLAUDE.md` and `.github/copilot-instructions.md` (kept there because it's longer than AGENTS.md should be).
- `assets/data/*.ron` — data-driven gameplay (buildings, techs, ship hulls/modules, notifications, survey). Modding surface: `docs/MODDING.md`, `docs/RESEARCH_MODDING.md`, `docs/SHIPBUILDING.md`.
- `scripts/` — 17 CI audit + asset-bake scripts (B0001 dual-Query, Color32/Bevy Color literals, SFX coverage/manifest audits, icon build/audio/sfx generation, asteroid & rock normal baking, menu-icon cropping, debug-icon pipeline, JPL asteroid normal generator, menu timings parser). See `ls scripts/` for the full list.
- `docs/`, `tests/`, `memories/` — long-form docs, integration tests, repo knowledge notes.

## Code style

- `rustfmt` defaults (no `rustfmt.toml`); aim for ≤ 100 chars per line.
- `clippy`: `[lints.clippy]` in `Cargo.toml` allows `too_many_arguments` and `type_complexity` (Bevy systems/queries are necessarily dense).
- **B0001 dual-Query rule (Bevy 0.18)**: a system function MUST NOT declare two `Query<…>` parameters accessing the same component — that's a runtime panic that `cargo build` / `cargo test` don't catch, only `cargo run` does. Fold the queries into one, use a `ParamSet`, or apply disjoint `With`/`Without` filters. Audit: `python3 scripts/audit_b0001.py src`.
- **Game time**: use `SimulationTime` (`src/ui/time.rs`) for game-world calculations; never `Time<Virtual>` (capped at ~15×). Positional/rotational math must be analytical, not incremental.
- **Egui systems** must run in `EguiPrimaryContextPass`, not `Update`. Use `egui::load::SizedTexture` for image widgets.
- **Events**: in Bevy 0.17+ use `MessageReader<T>` / `MessageWriter<T>` (not the old `EventReader`/`EventWriter`).
- **UI colours**: raw `Color32::from_*` and `bevy::Color::*` literals are CI-gated — `src/ui/theme.rs` is the only authorized home. Add new tokens there, or the `audit_color32_literals.py` / `audit_bevy_color_literals.py` audits will fail the build. See `docs/UI.md`.
- **Icons** (menus, research categories): treat input as dark lines on a white background; compute `alpha = (1.0 - luminance).powf(3.0)`, set RGB to pure white, then tint at runtime via `egui::Image::tint`. New icon sets follow the canonical pattern in `src/ui/resource_icons.rs`.
- **Entity API**: `Entity::index()` (not `row()`). State transitions: `NextState::set_if_neq()` to skip same-state transitions. Bevy 0.18 auto-updates `Aabb`; opt out with `NoAutoAabb` if needed.

## Testing instructions

- Unit + integration: `cargo test` (Bevy test apps are fine; resource roundtrips are the preferred pattern for state-mutation guards).
- Lint-as-test: `cargo clippy --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check` are the green-build gates.
- UI-lint audits (run from repo root): `python3 scripts/audit_color32_literals.py --strict --baseline scripts/audit_color32_literals_baseline.txt src` and `python3 scripts/audit_bevy_color_literals.py --strict --baseline scripts/audit_bevy_color_literals_baseline.txt src`.
- SFX audits: `python3 scripts/audit_sfx_manifest.py` and `python3 scripts/audit_sfx_coverage.py` (each SFX in the manifest must be wired to at least one trigger site).
- B0001 advisory: `python3 scripts/audit_b0001.py src` (print-only by default; pass `--strict` to fail on candidates).
- Save/load regression: when touching per-body state, add a test in `tests/state_store_v2_e2e.rs` (the canonical pattern lives there).
- New behaviour: add a unit or integration test in the same module or `tests/`. All CI checks must pass before merge.

## PR & commit conventions

- Branch from `main`; never push to it directly.
- Commit messages: verb-led summary in the imperative, reference the GitHub issue / Linear key. Example: `Add resource management plugin (#123)` or `Survey rework: 8-dimension model (GRA-141)`.
- **No PR summaries in the repo.** Do not create `SUMMARY.md`, `IMPLEMENTATION_SUMMARY.md`, `FIXES.md`, or similar — these become stale documentation clutter. Update existing docs instead of creating new ones; move historical work to `docs/archive/` after merge.
- PR via the GitHub UI once `cargo fmt`, `cargo clippy`, and the UI-lint audits are green. Include screenshots for visual changes.

## Security

- Never commit secrets — `.gitignore` covers `.env` and similar.
- License: MIT. By contributing, you agree your contributions are MIT-licensed (see `CONTRIBUTING.md`).
- No `SECURITY.md` policy file; report issues via GitHub Issues.

## See also

- `CLAUDE.md` — Claude-Code-specific agent instructions (Localized Resources, Notification event bus, Per-Trip Freight Cap, Shipbuilding Progression, Fleet Mechanics).
- `.github/copilot-instructions.md` — GitHub Copilot instructions (full Save-Game Compatibility catalog + `DirtyReason` variants, Atmospheric Scattering, Window/Taskbar Icon, Custom game start dates, Music Playlist, full Multi-Agent Worktree Safety decision table).
- `ROADMAP.md` — per-item v0.5.x status and next milestones.
- `docs/MODDING.md`, `docs/RESEARCH_MODDING.md`, `docs/SHIPBUILDING.md`, `docs/UI.md` — domain references.
- `memories/repo/splash-stall-prevention.md` — splash regression bisection recipe.
