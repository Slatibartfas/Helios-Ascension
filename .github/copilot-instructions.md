# Helios Ascension - GitHub Copilot Instructions

Welcome to Helios Ascension, a 4X grand strategy game built with Rust and the Bevy game engine. These instructions help GitHub Copilot understand our project's architecture, conventions, and best practices.

## Multi-Agent Worktree Safety (CRITICAL)

This repository is frequently worked on by **multiple agents (Copilot sessions, Claude Code, subagents, CI bots) in parallel worktrees**, often on the same branch or on closely-related branches that share build artifacts (`target/`, `Cargo.lock`, asset caches). **Naive `git stash`, `git checkout`, `git reset`, or `git clean` commands have repeatedly destroyed concurrent work.** Treat any operation that mutates the working tree as load-bearing and read this section before running it.

### Hard rules

- **Never run `git stash` in a shared worktree.** It sweeps uncommitted edits (including edits made by other agents) into a stash that may not belong to you, and `git stash pop` from a parallel agent can clobber the original author's file. If you believe you need to "set the worktree back to a clean state" to test something, **do not stash** — work on a fresh worktree, or copy the specific files you need into a scratch directory and inspect them there.
- **Never run `git stash --include-untracked` or `git stash -u`.** It silently vacuums up untracked files (logs, scratch notes, `.copilot/`, generated assets, other agents' in-flight work) that are not in git but may be in active use.
- **Never run `git checkout -- <path>`, `git restore --source=HEAD <path>`, `git reset --hard`, or `git clean -fdx` without an explicit `--` path scope and a dry-run first.** These commands are unrecoverable and have repeatedly wiped parallel work in this repo. If you need to discard changes, restrict the scope to a single file or path that you authored, and prefer `git diff <path>` to read first.
- **Never run `git switch` or `git checkout` to a different branch in a worktree that may be in use by another agent.** Create a new worktree (`git worktree add ../<branch>-worktree <branch>`) and work there instead.
- **Never run `cargo clean`, `rm -rf target/`, or `rm Cargo.lock`** in a shared worktree. Other agents depend on incremental builds in `target/`. If you need a clean build, do it in a fresh clone or in your own dedicated worktree.
- **Never assume a test failure is "pre-existing" and revert.** Re-running `cargo test` on an unchanged tree is the correct way to confirm a pre-existing failure. Do **not** stash, reset, or rebase to "check whether the failure was there before" — you will destroy the other agent's in-flight changes. See "Verifying pre-existing failures" below.

### Verifying pre-existing failures (the safe way)

When a test fails and you suspect it pre-dates your work:

1. Note the exact failing test name and the failure output (copy/paste, don't truncate).
2. Run `git status` and `git log -1 --oneline` in **your own worktree only** — never across worktrees.
3. Use `git stash -k` is still unsafe on shared worktrees. Instead: copy the files you suspect of being yours (e.g., `git diff --name-only HEAD`) to a scratch directory, then re-run the test in a fresh worktree branched from `main` (or the branch's base). This proves pre-existence without touching anyone else's work.
4. If the failure reproduces on a clean worktree of `main`, it is pre-existing. Report it; do **not** "fix" it as part of your task unless that is the assigned scope.
5. If the failure does **not** reproduce on a clean `main` worktree, the regression is yours (or a peer agent's). Bisect by file ownership, not by `git stash`.

### Why this matters here

Multiple agents in this repo have run `git stash` "just to check whether a failure was pre-existing" and lost hours of unrelated edits — including data files under `assets/data/`, in-progress RON edits, generated icons, and scratch diagnostic logs. The repeated pattern is: agent sees red CI, stashes the worktree, runs the test, sees green, concludes "pre-existing", pops the stash — but another agent's untracked files or unstaged edits never made it into the stash in the first place, so when the second agent continues they find their workspace silently corrupted. **The cure is to never reach for `git stash` in a shared worktree.**

### Safe alternatives (use these instead)

| Want to do this… | Use this instead |
|---|---|
| "Is this failure pre-existing?" | Re-run the test on a fresh `main` worktree |
| "Set the worktree to a clean state to debug" | Make a new worktree: `git worktree add ../clean main` |
| "Stash my work and switch branches" | Commit on a branch (`git switch -c my-fix`) or use your own dedicated worktree |
| "Discard my local changes to file X" | `git checkout HEAD -- <single-file-path>` **only** for files you authored; confirm with `git log -- <path>` first |
| "Clean build artifacts" | `cargo clean -p <specific-package>` instead of full `cargo clean` |
| "Run a hermetic experiment" | Copy the file(s) you need into `/tmp/<scratch>/` and inspect there |

### Quick self-check before any worktree mutation

Before running **any** of `git stash`, `git checkout`, `git restore`, `git reset`, `git clean`, `git switch`, `cargo clean`, `rm -rf target/`, or `rm Cargo.lock`, answer:

1. Is this worktree shared with another agent (look at sibling `../<repo>-*` directories)?
2. Could any of the files I'm about to touch belong to a peer agent?
3. Is there a non-destructive alternative (new worktree, scoped `git checkout -- <path>`, fresh clone)?

If any answer is "yes" or "unsure", **stop and use the safe alternative.** Document the choice in your final report so the user can audit it.

## Project Overview

Helios Ascension is a high-performance space strategy game inspired by Aurora 4X and Terra Invicta. The project emphasizes:

- **Performance**: Optimized compilation profiles and runtime performance
- **Realism**: Accurate astronomical data for 377+ celestial bodies
- **Modularity**: Plugin-based architecture using Bevy's ECS
- **Maintainability**: Clear separation of concerns and testable code

## Technology Stack

- **Language**: Rust 2021 Edition
- **Game Engine**: Bevy 0.18
- **Architecture**: Entity Component System (ECS)
- **Serialization**: RON (Rusty Object Notation) and Serde
- **Math**: glam for high-performance vector/matrix operations
- **Development Tools**: bevy-inspector-egui for debugging

## Project Structure

```
helios_ascension/
├── src/
│   ├── main.rs              # Application entry point
│   ├── lib.rs               # Library root
│   ├── game_state.rs        # Top-level game state management
│   ├── astronomy/           # Orbital mechanics & coordinate systems
│   │   ├── components.rs    # SpaceCoordinates, KeplerOrbit, OrbitPath
│   │   ├── systems.rs       # Orbit propagation, comet tails, LOD visibility
│   │   ├── selection.rs     # Body selection/hover, markers, camera zoom
│   │   ├── lagrange.rs      # Lagrange point rings and LP hover interaction
│   │   ├── ephemeris.rs     # Ephemeris calculations for custom start dates
│   │   ├── nearby_stars.rs  # Star catalog (60 nearest star systems)
│   │   ├── exoplanets.rs    # Exoplanet data
│   │   ├── procedural.rs    # Procedural system generation
│   │   └── mod.rs           # AstronomyPlugin
│   ├── colony/              # Colony management system
│   │   ├── components.rs    # Colony, BuildingInventory, ConstructionQueue
│   │   ├── types.rs         # BuildingType enum (29 types)
│   │   ├── data.rs          # Building data loading from RON
│   │   ├── systems.rs       # Construction processing, population growth
│   │   └── mod.rs           # ColonyPlugin
│   ├── economy/             # Resource & budget systems
│   │   ├── components.rs    # PlanetResources, MineralDeposit
│   │   ├── budget.rs        # GlobalBudget, EnergyGrid
│   │   ├── generation.rs    # Procedural resource generation (orchestration + helpers)
│   │   ├── profiles.rs      # Special body profiles + spectral-class resource tables
│   │   ├── mining.rs        # Mining operations and efficiency
│   │   └── types.rs         # ResourceType definitions (20 types)
│   ├── fleets/              # Fleet management & orbital transfer
│   │   ├── components.rs    # Fleet, FleetOrbit, ActiveManeuver, PlannedTransfer
│   │   ├── orbital_mechanics.rs # Hohmann transfers, transfer windows, gravity assists
│   │   ├── systems.rs       # Fleet position, maneuver execution, spawn
│   │   ├── visuals.rs       # Gizmo drawing, mesh management, trajectory rendering
│   │   ├── types.rs         # ShipClass (7), PropulsionType (6)
│   │   └── mod.rs           # FleetPlugin
│   ├── research/            # Technology tree system
│   │   ├── components.rs    # TechnologyProgress, EngineeringProject
│   │   ├── types.rs         # TechCategory enum (15 categories)
│   │   ├── data.rs          # Technology data loading from RON
│   │   ├── systems.rs       # Research progression, tech unlocks
│   │   └── mod.rs           # ResearchPlugin
│   ├── shipbuilding/        # Data-driven hulls, modules, projects, refit, slipways
│   │   ├── components.rs    # Shipbuilding resources, projects, design drafts
│   │   ├── data.rs          # Hull/module RON loading and design summaries
│   │   ├── refit.rs         # Design upgrade and refit logic
│   │   ├── slipway.rs       # Construction capacity and slipway helpers
│   │   ├── systems.rs       # Project progression and queue processing
│   │   ├── types.rs         # ShipModuleCategory, design templates, construction modes
│   │   └── mod.rs           # ShipbuildingPlugin
│   ├── plugins/             # Game systems
│   │   ├── camera.rs        # Camera movement, anchoring & ViewMode
│   │   ├── music.rs         # Background music playlist & CC-BY attribution overlay
│   │   ├── solar_system.rs  # Body spawning, atmosphere shells, rotation, mesh helpers
│   │   ├── star_materials.rs    # Star material structs (Glow/Surface/Diffraction/Corona/Halo) + LOD systems
│   │   ├── solar_system_data.rs # RON data loader
│   │   ├── starmap.rs       # Starmap view (system icons, visibility toggle)
│   │   ├── system_populator.rs  # Populates visited star systems procedurally
│   │   ├── atmosphere.rs    # Atmospheric scattering (Rayleigh + Mie shell material)
│   │   ├── comet_vfx.rs     # Comet visual effects (tail, glow)
│   │   └── visual_effects.rs    # Bloom, starfield, night materials
│   ├── render/              # Rendering utilities
│   │   └── backdrop.rs      # Skybox background
│   └── ui/                  # User interface
│       ├── mod.rs                 # UIPlugin, shared constants, overlay systems, re-exports
│       ├── time.rs                # SimulationTime, TimeScale, time helpers
│       ├── icons.rs               # MenuIcons, ResearchIcons, icon loading/processing
│       ├── resources_bar.rs       # Top resource bar UI
│       ├── dashboard.rs           # Main dashboard, time controls, star system panel
│       ├── research_panel.rs      # Research/engineering UI (overview, available, bonuses, archive tabs)
│       ├── tech_tree.rs           # Tech tree tab, edit dialog, category colors
│       ├── construction_panel.rs  # Construction queue UI
│       ├── economy_panel.rs       # Economy/budget UI
│       ├── shipbuilding_state.rs  # Shared shipbuilding UI state (selected hull, focused slot, queued builds)
│       ├── shipbuilding_workspace.rs # Native Bevy UI shipbuilding workspace (Logistics Hub / Design Blueprint / Engineering Analytics)
│       ├── shipbuilding_tooltip.rs # Slot hover tooltips and module compatibility hints
│       ├── fleets_panel.rs        # Fleet list, detail, orbit/maneuver status, FleetUiState
│       ├── transfer_planner.rs    # Transfer planner sub-panel (destination, options, LP transfers)
│       └── interaction.rs         # Selection management
├── assets/
│   ├── audio/
│   │   └── music/           # Background music (CC-BY 4.0, Scott Buckley)
│   ├── data/
│   │   ├── buildings.ron    # 47 building definitions
│   │   ├── ship_hulls.ron   # Hull frames and slot layouts
│   │   ├── ship_modules.ron # Canonical ship module definitions
│   │   ├── technologies.ron # Technology tree data
│   │   ├── solar_system.ron # Solar system configuration
│   │   └── nearest_stars_raw.json # Star catalog
│   └── textures/            # Visual assets
├── docs/
│   ├── MODDING.md           # Texture & celestial body modding guide
│   ├── RESEARCH_MODDING.md  # Technology tree modding guide
│   ├── SHIPBUILDING.md      # Shipbuilding data and workflow reference
│   └── ...                  # Other reference docs
└── tests/                   # Integration tests
```

## Architecture Principles

### Plugin-Based Design
- Each major game system is a Bevy plugin
- Plugins should be self-contained and composable
- Use Bevy's `App::add_plugins()` to register plugins
- Keep plugins focused on a single responsibility

### ECS Best Practices
- **Components**: Pure data structures, no behavior
- **Systems**: Pure functions that operate on components
- **Resources**: Shared global state, use sparingly
- Use Bevy's query system for efficient entity filtering

### Performance Considerations
- The project is configured with optimized build profiles
- Prefer iterator chains over imperative loops
- Use Bevy's parallel system execution where possible
- Minimize entity spawning/despawning in hot loops
- Profile before optimizing - use `cargo flamegraph` or similar tools

### Build Profile Guidance for Fast Iteration
- Bevy's documented fast-compile advice is to keep day-to-day builds on the development profile rather than release-style profiles.
- The project's custom `fast` profile inherits from `dev` so incremental rebuilds stay responsive and preserve useful debug information.
- Prefer `cargo check` for quick iteration and `cargo run --profile fast` for local playtests; use `cargo build --release` only for final optimization, profiling, or packaging.
- Avoid switching to release-like settings for regular development, because they can make Bevy rebuilds feel much slower than the documented “fast compile” experience.

## Coding Standards

Follow standard Rust best practices:

- Write idiomatic Rust following the Rust API Guidelines
- Use strong types and leverage the ownership system
- Handle errors with `Result<T, E>`, avoid `unwrap()` in library code
- Document public APIs with `///` doc comments
- Keep functions focused and under ~50 lines when possible
- Use `cargo fmt` and `cargo clippy` for code quality

## Testing Strategy

- Write unit tests for individual components and systems
- Use integration tests for plugin interactions
- Test data loading and serialization
- Use `cargo test` for standard testing
- Consider `cargo nextest` for parallel test execution

## Development Workflow

### Building
```bash
cargo build              # Debug build
cargo build --release    # Optimized release
cargo build --profile fast  # Fast iteration profile
```

### Running
```bash
cargo run                # Run debug build
cargo run --release      # Run optimized
```

### Testing
```bash
cargo test               # Run all tests
cargo nextest run        # Parallel testing
```

### Code Quality
```bash
cargo fmt                # Format code
cargo clippy             # Linting
```

## Bevy-Specific Guidelines

### Component Design
- Keep components small and focused
- Use marker components for entity categorization
- Derive common traits: `Component`, `Debug`, `Clone`

### System Design
- Systems should have clear inputs (queries) and outputs (mutations)
- Use system ordering to manage dependencies
- Prefer change detection queries (`Changed<T>`, `Added<T>`) for efficiency
- Use run conditions to control system execution

### Resource Usage
- Resources for global configuration and state
- Use `Res<T>` for immutable access, `ResMut<T>` for mutable
- Consider using events instead of resources for cross-system communication

### Events
- Use Bevy events for loose coupling between systems
- Define custom event types as needed
- Use `MessageReader<T>` and `MessageWriter<T>` in systems (renamed from `EventReader`/`EventWriter` in Bevy 0.17+)

## UI & Asset Guidelines

### Icon Processing
When adding new UI icons (menus, research categories, etc.), applying the following post-processing ensures consistent styling and themeability:

1.  **Format**: Load icons as standard images (e.g. PNG).
2.  **Processing Logic**:
    - Treat input as **dark lines on a white background**.
    - **Alpha Channel**: Calculate alpha from inverted luminance (`alpha = (1.0 - luminance).powf(3.0)`). This makes white backgrounds transparent and dark lines opaque.
    - **Color Channels**: Set all RGB pixels to **pure white** (`255, 255, 255`).
3.  **Runtime Tinting**: Since icons are pure white, they can be tinted to any color using `egui` (e.g., `ui.add(egui::Image::new(...).tint(color))`).

### Egui Integration
- Use `egui::load::SizedTexture` when adding images to `ui.add()` to ensure explicit control over size.
- Example: `ui.add(egui::Image::new(egui::load::SizedTexture::new(texture_id, [width, height])))`.

### Save / Load Persistence UI
- `SavePreview` in `src/persistence/snapshot.rs` is additive player-facing metadata. New fields must use `#[serde(default)]` and have safe defaults so older saves remain loadable.
- Each save uses a `.ron` file plus an optional sibling `.png` map thumbnail. Delete both through `persistence::delete_save_files`; extend that helper if another sidecar format is introduced.
- Save thumbnails must be captured before switching from `LaunchState::InGame` to the Save panel. Keep screenshot staging and render-readback completion explicit to avoid capturing menu UI or partially rendered frames.
- In-game Save / Load entry points use `PendingInGameSaveRequest` and `PendingInGameLoadRequest`. Their return-state resources must preserve whether Back returns to `InGame` or `MainMenu`.
- Reuse `render_glass_button` for primary Save / Load panel actions, and keep destructive actions behind persistent confirmation state.
- Bevy runtime resources without loader type registration belong in `snapshot.rs::configure_builder`'s denylist with a comment explaining why; do not persist process-local audio, window, camera, or timing plumbing.

## Domain-Specific Knowledge

### Save-game Compatibility (GRA-358 PR-I — CRITICAL)

Helios uses a **regenerate-from-seed + divergence overlay** save format
(StateStore). The save only persists bodies whose state differs from the
regen chain's seed-derived output. **Any per-body mutation that the regen
chain would otherwise re-derive silently reverts on load** unless the
mutating system marks the body dirty.

#### The dirty-marker rule

Every system that **mutates a per-body component** MUST mark that body
dirty via `ResMut<DirtyBodies>`. The marker tells the v2 extract path
to write a divergence entry so the change survives the next save/load
cycle.

```rust
use crate::economy::{DirtyBodies, DirtyReason};

fn my_mutating_system(
    mut bodies: Query<(Entity, &mut LocalStockpile)>,
    mut dirty: ResMut<DirtyBodies>,
) {
    for (entity, mut stock) in bodies.iter_mut() {
        stock.consume(ResourceType::Iron, 1.0);
        // Mandatory: tell the save system this body changed.
        dirty.mark_stockpile(entity);
        // For multiple reasons (e.g. colony grew AND
        // stockpile mutated), use `mark(entity, DirtyReason::Multiple)`.
    }
}
```

#### Catalog of mutating systems (audit point)

This is the canonical list of systems that mutate per-body state. **Every
new system that falls into one of these categories MUST be added here and
MUST mark dirty.** Conversely, if a new system mutates a per-body component
that's NOT on this list, that's a bug — extend the list and add the
`DirtyReason` variant.

| System | File | `DirtyReason` |
|---|---|---|
| `extract_resources` (mining) | `src/economy/mining.rs` | `Multiple` (stockpile + body mass) |
| `process_construction_actions` (build-queue consume) | `src/colony/systems.rs` | `Stockpile` |
| `update_colony_growth` (food production / population drain) | `src/colony/systems.rs` | `Multiple` |
| `deduct_maintenance_resources` | `src/colony/systems.rs` | `Stockpile` |
| `deduct_environment_costs` (O₂ / water) | `src/colony/systems.rs` | `Stockpile` |
| `auto_freight_loop` (freighter pickup) | `src/economy/auto_freight.rs` | `Stockpile` |
| `complete_deliveries` (freighter delivery) | `src/economy/logistics.rs` | `Stockpile` |
| **(future)** terraforming system | `src/colony/` (planned) | `Atmosphere` |
| **(future)** asteroid redirect / orbit shift | `src/astronomy/` (planned) | `Orbit` |
| **(future)** body-mass / radius editor | `src/astronomy/` (planned) | `Body` |
| **(future)** colony founding / dissolution | `src/colony/` (planned) | `Colony` |
| **(future)** population growth / migration | `src/colony/` (planned) | `Population` |

#### `DirtyReason` variants

When a system mutates a per-body component, choose the variant that best
describes what changed. The extract path's `extract_bodies` uses an
explicit `match` per variant — that's the audit point that catches
forgotten marks.

```rust
pub enum DirtyReason {
    Stockpile,   // LocalStockpile mutated (mining, delivery, consumption)
    Atmosphere,  // AtmosphereComposition mutated (terraforming)
    Orbit,       // KeplerOrbit mutated (orbit shift, asteroid redirect)
    Body,        // CelestialBody mutated (mass, radius, rotation)
    Colony,      // Colony founded or dissolved
    Population,  // Population changed
    Multiple,    // Two or more of the above
}
```

If the new mutation doesn't fit any variant, **add a new variant** to
`src/economy/components.rs::DirtyReason` and wire the corresponding
divergence field in `src/persistence/state_store_extract.rs::extract_bodies`.

#### Restore-path regen fallback (PR-I follow-up)

The v2 restore factory (`build_minimal_world_for_restore`)
returns an empty `World`. The regen chain that would normally
spawn celestial bodies is gated on `BootState::Loading &&
WorldReady && !RestoredWorldGate` — i.e. it does NOT fire on
the restore path. Without intervention, `apply_state_store`
would find zero body entities and silently drop every
per-body divergence (`LocalStockpile`, `Colony`, `Population`,
orbital overrides, etc.) as a warning.

**Two safeguards run inside `apply_state_store`:**

1. **`regenerate_bodies_minimal`** (when the world has zero
   bodies): loads `assets/data/solar_system.ron` and spawns
   each body with the minimum components the apply path
   reads (`CelestialBody` + `SystemId`). No-op when the world
   already has bodies (New Game path).
2. **`init_missing_resources_for_apply`** (always): seeds
   every resource the `apply_*` family reads with its
   `Default` impl if it isn't already present. The list
   mirrors the test-side `bootstrap_world` helper at the
   bottom of `state_store_apply.rs` — keep them in sync. A
   missing resource would silently no-op via
   `get_resource_mut::<T>()` and the player would lose
   treasury, research points, unlocked tech, view mode,
   atmosphere quality, autosave cadence, notification
   settings, etc. on every Restore.

Trade-off: bodies restored via `regenerate_bodies_minimal`
are missing meshes, materials, transforms, `KeplerOrbit`,
and spectral-class resource deposits (the
`generate_solar_system_resources` system normally populates
`PlanetResources` with spectral-class data). The regen-minimal
populates an empty `PlanetResources` so the mining system
finds the component (and the per-frame
`Colony X has no PlanetResources` warning — now `debug!` in
`mining.rs` — doesn't fire). Mining rates compute to zero
until the full regen chain lands. PR-J was attempted (build
the minimal App with the full Helios plugin stack, run the
chain via `app.update()`, swap the populated world into the
live App). The chain ran end-to-end (710 bodies + 60 nearby
systems + day-one fleet + baseline tech) but Bevy 0.18's
`propagate_parent_transforms` then stack-overflowed on the
live App's `PostUpdate`: `despawn_helios_simulation_entities`
cleared prior-session body entities but left stale
`Children` collections on parent ring entities that the
recursive propagation walked indefinitely. The follow-up
must either (a) teach the pre-swap live-world cleanup to
also clear stale `Children` from despawned parents, or
(b) refactor `setup_solar_system` to not assign `ChildOf`
to rings (use only `LogicalParent`) so the live world never
has a `Children`-bearing parent after a swap.

#### Restore-path "Frankenstein world" prevention (PR-I follow-up)

Bevy 0.18's `swap_world_into` spawns fresh entities for the
save's pending world but never despawns the live world's
pre-existing session entities. Without intervention, a
player who loaded a save after playing a New Game session
would see two copies of every body (their original session's
entities + the apply's freshly-spawned entities), producing
confused orbital propagation and double-counted resources.

**`swap_pending_into_target` calls
`despawn_helios_simulation_entities` at the very top of the
swap.** The denylist `helios_simulation_marker_set()` lists
the components that identify a Helios simulation entity
(`SpaceCoordinates`, `Fleet`, `Colony`, `LocalStockpile`,
`SurveyState`, etc.). Bevy plumbing entities (Window,
camera helpers, audio output streams) are kept because
they don't carry any of these markers. See
[`src/persistence/swap.rs::helios_simulation_marker_set`] for
the full list and the rationale.

Regression test: `swap_world_into_despawns_helios_simulation_entities_from_target`
in `swap.rs`.

#### Components that need a `Serialize` derive (PR-I follow-ups)

These components need `Serialize` / `Deserialize` derived before their
divergence fields can carry real data on round-trip. Until then the
extract path emits a sentinel flag (empty JSON object) and the apply path
surfaces a warning via `ApplyOutcome.warnings`.

- `AtmosphereComposition` (`src/astronomy/components.rs`) — terraforming
  field stays empty in v2 saves; apply warns and falls through.
- `CelestialBody` (`src/plugins/solar_system.rs`) — body-mass / radius
  changes stay empty in v2 saves; apply warns and falls through.

Both derive flags belong on the existing `#[derive(...)]` attribute
(the components are already `Component, Debug, Clone, Reflect`). Adding
`Serialize, Deserialize` to those derives closes the loop — no schema
break required on `BodyDivergence` because the JSON blobs are already
`serde_json::Value`.

#### Save-load audit checklist (when adding a new system)

When you add a new system that mutates a per-body component:

1. **Identify the category**: stock, colony, pop, atmosphere, orbit,
   body, fleet, or some combination. Look at the analogous entry in the
   table above.
2. **Choose / add a `DirtyReason` variant**. Existing variants cover
   most cases; only add a new one if no existing variant captures the
   semantics.
3. **Wire `dirty.mark(...)` (or `dirty.mark_stockpile(...)`) into the
   system**. The mark should fire on every iteration that actually
   mutated state — not on read-only inspections.
4. **Add a `match` arm in `extract_bodies`**
   (`src/persistence/state_store_extract.rs`) so the new reason populates
   the matching divergence field. If the field doesn't exist yet (e.g.
   `body_override`), add it to `BodyDivergence` with
   `#[serde(default)]` so older saves still load.
5. **Update the catalog table above** with the new system.
6. **Add a regression test** in `tests/state_store_v2_e2e.rs` that
   exercises the mark + extract path end-to-end. Pin the contract
   that the body appears in divergences with the expected override
   fields populated.
7. **Wire the test harness**: if the system is unit-tested via
   `App::new()`, add `app.init_resource::<DirtyBodies>()` to the test
   setup — see `src/economy/auto_freight.rs::tests::init_econ_resources`
   for the canonical pattern.

#### Regen-chain vs player-driven

Some entities are produced by the regen chain on every fresh world (the
`Day-One Constellation`, the `Mars Flyby Probe`, the debug `Earth→Jupiter`
fleet). These carry the `RegenChainFleet` marker component and the extract
path skips them. **Adding a new regen-chain-spawned entity MUST also
tag it with `RegenChainFleet`** — otherwise every Save → Load cycle
duplicates it. See `src/fleets/systems.rs::RegenChainFleet` for the
canonical pattern.

### Game Systems Overview

#### Colony Management
- **47 building types** across 8 categories (Infrastructure, Industry, Logistics, Power, Population, Research, Financial, Military)
- Each building has district-scale output: Housing = 25M residents, Farm = 1,000 Mt/yr food (~10M people), HabitatDome = 50M, Farm/Greenhouse/Aquaculture scale ×10 vs old values
- Each new building is a perceptible improvement; Earth starts with ~335 Housing Complexes (not 33,500) — queuing one adds ~0.3% capacity
- Construction queue system with resource costs and workforce requirements
- Population growth mechanics with housing capacity and food requirements (food consumption: 0.0001 Mt/person/yr)
- Buildings require maintenance resources and generate various effects (see `BuildingType::effects_summary()`)
- Tech-gated buildings unlock through research progression
- Debug menu (F12) for free construction, instant build, and tech bypass
- **Outpost founding** (`EstablishOutpostRequest` in `PendingConstructionActions`): dossier panel provides "🏗 Establish Outpost" button; hard blocks for gas giants and gravity > 3 g; starter package (LifeSupport, Housing ×1, FissionReactor ×2, AgriDome ×2) queued on click; `ColonyEnvironmentCosts` attached for O₂/Water drain
- **Resource transport** (current v0.3 behaviour): construction still draws from the same-system `ContextualStockpile` pool; interstellar supply requires a Freighter fleet transfer
- **Planned logistics network (v0.4+)**: resources will be **physically located on individual bodies** (`LocalStockpile`); construction will draw from local stockpile only; building/outpost creation publishes a `ResourceRequest`; requests fulfilled by player Freighters OR AI private shipping companies; `ContextualStockpile` retained for display-only aggregation; per-colony `MinimumStockpile` thresholds auto-create replenishment requests; see `docs/design/LOGISTICS_NETWORK.md`
- **Private shipping companies (planned)**: `ShippingCompany` AI resource; companies bid on open requests, execute Hohmann transfers using same `orbital_mechanics.rs` code as player fleets, earn credits, buy more ships; see `docs/design/LOGISTICS_NETWORK.md`

#### Economy & Resources
- **37 resource types** (defined in `src/economy/types.rs` as `ResourceType` enum): Volatiles (Water, Hydrogen, Ammonia, Methane, Phosphorus), Biological (Food), Atmospheric Gases (Nitrogen, Oxygen, CarbonDioxide, Argon), Construction Materials (Iron, Aluminum, Titanium, Silicates, Nickel, Tungsten, Carbon, Chromium, Magnesium), Fusion Fuel (Helium3, Deuterium), Fissiles (Uranium, Thorium), Precious Metals (Gold, Silver, Platinum), Strategic Materials (Copper, RareEarths, Lithium, Sulfur, Cobalt, Fluorine, Polymers), Exotic Materials (Antimatter, ExoticMatter, Metamaterials, Computronium)
- Resource stockpiles with capacity limits
- Mining operations extract resources from mineral deposits
- Refining and processing buildings convert raw materials
- Energy grid with power generation (solar, fission, fusion) and consumption
- Logistics system affects mining and research efficiency (demand vs capacity)
- Global budget tracks treasury, income, and expenses

#### Research & Technology
- **15 technology categories**: Electronics, Military, SpaceTechnology, Biology, Physics, Energy, Sociology, Construction, Propulsion, Materials, Sensors, Weapons, DefensiveSystems, LifeSupport, Industry
- Technology tree with prerequisite chains
- Research points (RP) and engineering points (EP) progression
- Technologies unlock buildings, components, and modifiers
- Tech modifiers affect construction costs, productivity, and capabilities
- **Data-driven**: All technologies defined in `assets/data/technologies.ron` — add techs without touching Rust code
- Debug menu (F12) for instant research
- See [docs/RESEARCH_MODDING.md](../docs/RESEARCH_MODDING.md) for the full modding guide (modifier types, component definitions, balancing)

#### Shipbuilding (`src/shipbuilding/`)
- `ShipbuildingPlugin` manages data-driven hulls, modules, design summaries, and construction project progression
- Canonical ship data lives in `assets/data/ship_hulls.ron` and `assets/data/ship_modules.ron`
- Do **not** create or keep generated `ship_modules*.ron` snapshots in `assets/data/`; they become stale and cause ambiguity about the source of truth
- Module unlocks are coupled to the tech tree through `required_tech` in `assets/data/ship_modules.ron` and matching IDs in `assets/data/technologies.ron`
- Module IDs must be unique; the runtime loader stores modules in a `HashMap` keyed by ID, so duplicate IDs silently override earlier entries
- RON edits must preserve tuple separators exactly; malformed RON often appears only at runtime during `cargo run`, not at compile time
- `ShipModuleCategory` is a 21-variant enum: 12 consolidated (canonical, target for new entries) and 9 legacy sub-categories retained for backward compatibility with existing RON data
  - **Consolidated (12, canonical):** `FlightSystems`, `PowerThermal`, `FuelStorage`, `Weapons`, `FireControl`, `Sensors`, `ArmorDefense`, `CrewSystems`, `UtilitySupport`, `ConstructionISRU`, `ElectronicWarfare`, `SpecialScience`
  - **Legacy (9, tolerated for existing data only):** `Bridges`, `Habitats`, `Medical`, `Maintenance`, `CargoStorage`, `Magazines`, `PointDefense`, `Armor`, `Construction`
  - **Cross-file vocabulary rule:** the consolidated set is the canonical vocabulary. When new hull `slot_layout` categories or new module `category` fields are authored, use a consolidated variant. Legacy variants are kept so the loader can still deserialize existing RON but should not appear in new entries.
  - **LGD rationale (GRA-7):** `Medical` and `CrewSystems` are kept distinct — the med-bay slot is sized for sickbays / surgical / triage and is not interchangeable with general crew quarters. `ConstructionISRU` unifies mining heads, regolith processors, gantries, and habitat modules under a single industrial / in-situ umbrella.
- **Five propulsion eras** drive ship progression: **Chemical → Fission / NTR → Gas-Core / Early Fusion → Fusion Torch → Antimatter**. Each era unlocks a coordinated set of hulls, drives, reactors, and slot families. The flagship drive tech for the era owns the era's engineering target via `unlocks_engineering`, and every module in the era's families should point at that shared target. Hull-construction techs (e.g. `chemical_spaceframes`, `orbital_assembly_heavy`, `carbon_nanotube_frames`, `fusion_superstructures`, `antimatter_containment_structures`) gate the *spaceframe*, not the propulsion.
- **Module-family gating is two-key.** Every ship module must set **both** `required_tech` (visibility) and `required_component_design` (engineering project). The runtime loader is not tolerant of a missing `required_component_design`, and the data-rule enforcement in `docs/SHIPBUILDING.md` and `.github/agents/shipbuilding-data.md` makes this a hard author-time check. All 84 modules in the current RON set both fields; new entries must follow the same pattern.
- The Shipbuilding menu now uses a single native backend:
  - `src/ui/shipbuilding_workspace.rs` = native Bevy UI shipbuilding workspace with blueprint canvas, module library, construction/archive tabs, and analytics panel
- `src/ui/shipbuilding_state.rs` holds the shared shipbuilding UI state; avoid duplicating selection, preview, or hull state in backend-local resources unless there is a strong reason
- The native blueprint currently uses **heuristic slot placement** based on slot IDs/categories when `position` is not authored in `assets/data/ship_hulls.ron`; authored `position` data should be preferred for long-term layout quality
- Ship module progression is now **engineering-first**: `required_tech` controls visibility, `required_component_design` groups module families behind a shared engineering project, and the relevant technology should advertise that family through `unlocks_engineering`
- See [docs/SHIPBUILDING.md](../docs/SHIPBUILDING.md) for the current shipbuilding workflow and data authoring rules

#### Fleet Management & Orbital Mechanics (`src/fleets/`)
- **`FleetPlugin`** manages fleet spawning, transfer planning, and ECS lifecycle
- **7 ship classes** (`ShipClass`): Courier, Frigate, Destroyer, Cruiser, ResearchVessel, Freighter, Station
  - Each class has a default dry mass (500 t – 100 000 t) and fuel fraction
- **6 propulsion types** (`PropulsionType`): Chemical (450 s), NuclearThermal (900 s), IonDrive (5 000 s), NuclearPulse (10 000 s), FusionTorch (50 000 s), AntimatterDrive (1 000 000 s)
  - Tsiolkovsky rocket equation used throughout: Δv = Isp × g₀ × ln(m_wet / m_dry)
- **`Fleet`** component: named collection of `ShipInfo` structs; Δv capacity is limited by the weakest ship
- **`FleetOrbit`** component: stable circular parking orbit around a body; visual angle advances at 1 rev/40 s real time (freezes when paused)
- **`ActiveManeuver`** component: Keplerian transfer arc computed once and propagated analytically each frame by `update_fleet_maneuver_positions`; removed by `complete_fleet_maneuvers` when `arrival_time` is reached
- **`PendingFleetActions`** resource: thread-safe action queue (spawn, start-transfer, cancel-maneuver, refuel) consumed once per Update tick by `process_fleet_actions`
- **Transfer planning** (`orbital_mechanics.rs`):
  - `hohmann_transfer()` — minimum-energy co-planar transfer
  - `calculate_transfer_options()` — 3 options (Efficient / Moderate / Fast) with different energy multipliers
  - `calculate_transfer_options_phased()` — same but after a player-chosen departure delay
  - `compute_transfer_window()` — live synodic-period countdown, phase-angle error, and phase-rate (rad/s)
  - `GravityAssistOption` — flyby bodies near the transfer arc that reduce total Δv
- **`FleetUiState`** resource (defined in `src/ui/fleets_panel.rs`, re-exported as `crate::ui::FleetUiState`; transfer planner rendering lives in `src/ui/transfer_planner.rs`): per-frame state for the Fleet panel
  - `selected_fleet`, `target_body`, `target_lagrange`, `target_fleet`
  - `departure_offset_days` slider for phased departure timing
  - `computed_options` / `planned_transfer` / `show_transfer_popup`
  - `gravity_assist_candidates` — surfaced automatically for heliocentric transfers
- **Lagrange points**: `LagrangeTarget` struct holds the primary, secondary, and Lagrange index (1–5); computing L4/L5 for any planet or Sun-Earth L1/L2/L3 is supported
- **Visualisation** (gizmo-based, no persistent meshes): trajectory arcs, orbit rings, selection reticules, transfer-preview arcs, gravity-assist preview, starmap icons
- **Important**: fleet visual orbit rate uses `Time<Real>` (real-time), but maneuver position uses `SimulationTime` — never mix them
- An initial Earth-orbit frigate fleet is spawned in `PostStartup` by `spawn_initial_fleet`

#### Background Music (`src/plugins/music.rs`)
- `MusicPlugin` plays a sequential looping playlist of ambient tracks during gameplay
- Tracks use `PlaybackMode::Despawn` — Bevy auto-despawns the entity when a track ends; the `advance_playlist` system detects this and starts the next track
- A non-interactive egui overlay in the bottom-right corner shows the current track title and CC-BY attribution (required by the Scott Buckley license)
- **Current playlist** (all CC-BY 4.0, Scott Buckley — www.scottbuckley.com.au):
  - `audio/music/starfire.mp3` — 'Starfire'
  - `audio/music/adrift-among-infinite-stars.mp3` — 'Adrift Among Infinite Stars'
  - `audio/music/passage-of-time.mp3` — 'Passage Of Time'
- **Adding a track**: push a new `TrackInfo { path, title }` into the `Vec` in `MusicPlaylist::default()`. No other code changes needed.
- **License requirement**: every track MUST include a `title` string matching the official Scott Buckley attribution so the overlay stays correct.

#### Window / Taskbar Icon (`src/plugins/window_icon.rs`)
- `WindowIconPlugin` sets the OS window + taskbar icon at startup by applying a multi-resolution set of RGBA bitmaps via `winit::Window::set_window_icon` through `bevy_winit::WINIT_WINDOWS`.
- Bevy 0.18's `Window` struct does NOT expose an `icon` field — the high-level `Window` descriptor carries no icon. The icon has to be applied post-creation via the winit handle stored in `bevy_winit::WINIT_WINDOWS` (a `!Send` thread-local). The plugin runs in `Startup` after the primary window entity is spawned by `WindowPlugin` and the splash window entity by `SplashPlugin`.
- Asset strategy: **prefer the pre-rendered per-size PNGs** in `assets/icons/icon_<N>.png` (16, 32, 48, 64, 128, 256). These are generated by [`scripts/build_icons.py`](../scripts/build_icons.py) from `assets/logo/icon.png` (the canonical 667×677 square crop of the artwork) using Lanczos downscale.
- **Runtime fallback** when a pre-rendered PNG is missing: Lanczos3-resize `assets/logo/icon.png` (preferred) or `assets/logo/logo_large.png` (last resort) so a fresh checkout without `assets/icons/` still gets a working icon.
- A pre-built `assets/icons/icon.ico` (PNG-in-ICO, multi-res 16/32/48/64/128/256) is **also produced** by the script for downstream packaging tools (`cargo-bundle`, Windows installers); it is NOT read by this plugin — `winit::Icon::from_rgba` consumes raw RGBA only.
- **Regenerating the icon set**: run `python scripts/build_icons.py`. Idempotent — re-running after editing `assets/logo/icon.png` updates all sizes. macOS `.icns` is intentionally NOT produced (the runtime path doesn't need it; `cargo-bundle` + `iconutil` handle macOS at packaging time).
- Sizes use `ICON_SIZES = &[32, 48, 64, 128, 256]` — Windows picks the bitmap whose dimensions are closest to the OS request (taskbar = 32×32, Explorer thumbnail = 48 or 256). The pre-rendered 16×16 PNG is shipped in `assets/icons/` for tooling but not handed to winit (winit's smallest meaningful size is 32).
- Plugin reads the source PNGs from disk at startup (not `include_bytes!`) so re-exporting the logo doesn't require a rebuild — but it also means the assets MUST be present at runtime. CI must run `scripts/build_icons.py` before `cargo run`.

#### Atmospheric Scattering (`src/plugins/atmosphere.rs`)
- `AtmospherePlugin` registers `MaterialPlugin::<AtmosphereMaterial>` and a global `AtmosphereSettings` resource (enabled, quality, intensity)
- Each body with an atmosphere gets a child sphere mesh at 1.05× visual radius with `AtmosphereMaterial` (`AtmosphereShell` marker component)
- Shader (`assets/shaders/atmosphere_scattering.wgsl`) uses `@group(3)` bindings; planet centre is derived analytically from fragment geometry so it stays correct as planets orbit
- Scattering parameters auto-derived from `AtmosphereComposition` via `derive_scattering_params()` on `AtmosphereComposition`:
  - Scale height from surface gravity + mean molecular weight
  - Rayleigh tint from dominant gas (CO2→warm, H2-dominant→blue-white for ice giants, CH4 in N2→amber for Titan, N2/O2→classic blue)
  - Haze colour uses `H2 > 50%` branch first so Uranus/Neptune get pale blue-white, not orange
- RON overrides in `solar_system.ron` (optional fields): `scale_height_km`, `rayleigh_rgb`, `rayleigh_strength`, `mie_strength`, `mie_g`, `haze_color`, `atmosphere_intensity`, `scattering_replaces_clouds`
- `scattering_replaces_clouds: true` on Venus skips spawning the `venus_atmosphere_2k.jpg` texture layer to avoid double-atmosphere artefact
- Layer ordering (surface outward): surface (1.0×) → night lights (1.002×) → cloud deck (1.015×) → scattering shell (1.05×)
- Quality presets in `AtmosphereSettings`: `Low` (4 ray-march samples), `Medium` (8), `High` (16)

#### Celestial Body & Texture Modding
- All solar system data defined in `assets/data/solar_system.ron`
- Add custom textures by setting the `texture` field in the RON file — no code changes needed
- See [docs/MODDING.md](../docs/MODDING.md) for the full texture and body modding guide

#### Celestial Bodies & Astronomy
- All astronomical data is based on real NASA/IAU sources
- Orbital mechanics use simplified Keplerian elements
- Time acceleration is supported for simulation speed (up to 1 year/second)
- Bodies are organized hierarchically (Sun -> Planets -> Moons)

### Simulation Time (IMPORTANT)
- **Never use `Time<Virtual>`** for game-world calculations. Bevy's virtual time has a hard `max_delta` cap (~250ms) that silently limits effective speed to ~15×.
- Use `SimulationTime` (defined in `src/ui/time.rs`, re-exported as `crate::ui::SimulationTime`) for all game-world elapsed time. It reads `Time<Real>` delta and scales it by `TimeScale`, with **no cap**, enabling speeds up to 1 year/second.
- Access via `Res<SimulationTime>` — call `.elapsed_seconds()` to get total simulation time in f64.
- All time-dependent game systems (orbits, rotation, economy ticks, research, production) **must** use `SimulationTime`, not `Time`, `Time<Virtual>`, or `time.delta_seconds()`.
- `Time` / `Time<Real>` should only be used for UI animations, camera movement, and other real-time visual effects that should not scale with game speed.
- All positional/rotational calculations must be **analytical** (compute state from total elapsed time), not **incremental** (accumulate deltas). This ensures correctness at any speed.

#### Custom game start dates & ephemeris (how to implement)
- Use the ephemeris utility in `src/astronomy/ephemeris.rs` to compute mean anomalies for a chosen Unix timestamp: `calculate_positions_at_timestamp(start_timestamp)`.
- Create `SimulationTime` with `SimulationTime::with_start_timestamp(start_timestamp)` so that the UI and systems display the correct date and time.
- When spawning bodies at game start (e.g., in `src/plugins/solar_system.rs::setup_solar_system`), override loaded `initial_angle` / set `KeplerOrbit.mean_anomaly_epoch` from the values returned by `calculate_positions_at_timestamp` (convert degrees to radians as necessary).
- Ensure this initialization runs before any systems that propagate or render orbits, so all bodies begin at the correct positions for the start date.
- Add tests to validate a few canonical dates (e.g., Jan 1, 2026) to ensure the ephemeris integration remains correct.

### Orbit Rendering
- Orbit trails sample uniformly in **true anomaly** for even point density
- Highly eccentric orbits (e > 0.6) automatically get more segments
- Trail fades from the body's current position backwards around the orbit

### Camera System
- Free-flight camera with WASD panning (W/S=up/down, A/D=left/right), right-click drag rotation, mouse wheel zoom
- Home key to recenter view after panning
- Camera focuses on selected celestial bodies via anchor (double-click to anchor)

### Ambient Lighting
- Use `GlobalAmbientLight` (resource) for the default ambient light for the entire world
- Use `AmbientLight` (component) on a `Camera` entity to override the global ambient for that camera
- In Bevy 0.18, the old `AmbientLight` resource was split into `GlobalAmbientLight` (resource) and `AmbientLight` (component)

### State Transitions (Bevy 0.18)
- `NextState::set()` now **always triggers** `OnEnter`/`OnExit` transitions, even if setting the same state
- Use `NextState::set_if_neq()` if you want the previous behavior of skipping same-state transitions

### Entity API (Bevy 0.18)
- Entity terminology changed from "row" to "index": `Entity::row()` → `Entity::index()`, `EntityRow` → `EntityIndex`
- Many Entity interaction errors changed from `EntityDoesNotExistError` to `EntityNotSpawnedError`

### Custom Materials & Shaders
- Material bind groups use `@group(3)` in WGSL shaders (shifted from `@group(2)` in 0.17)
- Custom materials use `#[derive(AsBindGroup)]` — the derive auto-generates the `label()` method
- `MaterialPlugin` no longer has `prepass_enabled` / `shadows_enabled` fields; override `Material::enable_prepass()` / `Material::enable_shadows()` methods instead
- Camera HDR is controlled via the `Hdr` marker component (not a field on `Camera`)
- Bloom is at `bevy::post_process::bloom::Bloom`
- `ShaderRef` is at `bevy::shader::ShaderRef`

### Automatic Aabb Updates (Bevy 0.18)
- Bevy now auto-updates `Aabb` when meshes/sprites change — no need to manually remove and re-add `Aabb`
- Use `NoAutoAabb` component to opt out of automatic Aabb creation/update

## Security Considerations

- Validate all user inputs
- Use safe Rust practices, avoid `unsafe` unless necessary
- Be careful with deserialization from untrusted sources
- Follow Rust's memory safety guarantees

## Performance Guidelines

- Profile before optimizing
- Use Bevy's built-in diagnostics for frame timing
- Batch operations where possible
- Use Bevy's parallel system execution
- Consider using `bevy_rapier` for physics if needed

## Documentation

### Documentation Principles (CRITICAL)
- **NO PR Summaries**: NEVER create "SUMMARY.md", "IMPLEMENTATION_SUMMARY.md", "FIXES.md" or similar PR-specific documents. These become stale documentation clutter.
- **Update, Don't Create**: Before creating new documentation, search for existing docs that can be updated instead.
- **One Source of Truth**: Each topic should have ONE authoritative document. Avoid multiple documents covering the same subject.
- **Archive Old Content**: Move historical/completed work summaries to `docs/archive/` immediately after PR merge.
- **Lean Documentation**: Every document must serve an ongoing reference purpose. If it's just a progress report, it doesn't belong in main docs.

### Documentation Guidelines
- **Cleanliness**: Maintain a clean project root. Move detailed docs to `docs/`, `docs/design/`, or `docs/archive/`.
- **Maintenance**: ALWAYS prefer updating existing documents over creating new ones. Consolidate related information.
- **Synchronization**: Ensure every code change is reflected in the relevant documentation immediately.
- **Canonical Data Files**: For shipbuilding and tech-tree work, keep a single source of truth in the main `assets/data/*.ron` files and delete one-off generated variants after consolidation.
- **Review**: Regularly scan detailed documentation (`docs/`) to ensure it matches the current codebase state.
- **Reference Material Only**: Documentation in `docs/` should be reference material (guides, architecture, APIs), not progress reports or PR summaries.
- Document all public APIs with `///` doc comments
- Include examples in doc comments
- Keep README.md up to date
- Update ARCHITECTURE.md for significant changes

### Egui System Scheduling
- All egui-using systems must be placed in the `EguiPrimaryContextPass` schedule (from `bevy_egui`), **not** `Update`
- This is required because `bevy_egui` 0.36+ runs its context setup in a separate pass; calling `available_rect()` before the context is ready causes a panic
- Example: `.add_systems(EguiPrimaryContextPass, my_egui_system)`
- This applies to **egui systems only**. Native Bevy UI systems such as `src/ui/shipbuilding_workspace.rs` stay in normal Bevy schedules like `Startup` or `Update`

### Bevy 0.18 Feature Collections
- Bevy 0.18 introduced high-level cargo feature collections: `2d`, `3d`, `ui`
- Consider using these instead of listing individual sub-crate features for simpler `Cargo.toml` maintenance
- Our project currently uses individual features for fine-grained control

### Worktree & Build-Artifact Safety (recap)
- See **Multi-Agent Worktree Safety** at the top of this document. The TL;DR: never `git stash`, never `cargo clean`, never `git reset --hard`, never `rm -rf target/` in a shared worktree. Use a fresh worktree (`git worktree add`) for hermetic experiments, and re-run failing tests on a clean `main` worktree to confirm pre-existence rather than stashing. This rule applies to **every agent** working in this repo.

## Getting Help

- Check the [Bevy documentation](https://bevyengine.org/)
- Review the [Rust Book](https://doc.rust-lang.org/book/)
- See existing code patterns in the plugins/ directory
- Use the specialized chat modes in `.github/agents/`
