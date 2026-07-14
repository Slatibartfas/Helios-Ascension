# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Helios Ascension is a 4X grand strategy game built with Rust and Bevy 0.18, featuring realistic orbital mechanics, colony management, fleet operations, and a technology tree. The game simulates 377+ solar-system bodies plus 5 000+ confirmed exoplanets across 60+ nearby star systems with accurate astronomical data.

The current release is **v0.5.0 (Exploration & Progression, 🟡 IN FLIGHT)** — the building & logistics overhaul is shipped (52 building types, per-body resource stockpiles, private shipping companies), the survey rework is shipped (eight-dimension model, 9-mission roster, anomaly confidence, recovery missions), the notification/event system is shipped (toast panel, settings, event bridges, click-to-focus), the transfer planner is hardened (porkchop plot, Lagrange routing, star-approach parking-radius picker), and the personnel data layer is shipped (scientists with specialty & seniority). The remaining v0.5 surface is the Personnel Roster UI panel. See `ROADMAP.md` for the per-item status.

## Commands

```bash
# Build
cargo build              # Debug (faster compilation)
cargo build --release   # Optimized
cargo build --profile fast  # Quick iteration

# Run
cargo run
cargo run --release
cargo run --profile fast

# Test
cargo test                    # Run all tests
cargo test test_name          # Run specific test
cargo test -- --nocapture     # Show output

# Parallel testing (faster)
cargo nextest run

# Code quality
cargo fmt
cargo clippy
cargo doc --open
```

## Architecture

The project uses a **plugin-based architecture** with Bevy's ECS:

```
src/
├── astronomy/       # Orbital mechanics, Kepler orbits, ephemeris, exoplanets, nearby stars, Lagrange helpers
├── colony/          # Buildings, construction, population growth
├── economy/         # Resources, mining, budget, energy grid, logistics, shipping-company AI
├── fleets/          # Ships, maneuvers, Hohmann transfers, porkchop, Lagrange transfers
├── personnel/       # Scientists (v0.5.0 data layer; UI pending)
├── research/        # Technology tree, engineering, unlock catalogs
├── shipbuilding/    # Hull/module data, construction projects, refit, slipways
├── ships/           # Hull templates, migration shims (legacy `standard_freighter`)
├── survey/          # v0.5.0 survey rework: 8-dimension state, missions, anomalies, instruments
├── plugins/         # Camera, music, solar_system, atmosphere, visual effects
├── ui/              # All UI panels (dossier, construction, research, economy, fleets, shipbuilding, notifications, transfer_planner, porkchop)
└── render/          # Skybox, backdrop
```

### Key Systems

- **Astronomy**: KeplerOrbit propagation, comet tails, Lagrange points, starmap, **exoplanet data model staged (CSV loader deferred to v0.6, see `assets/data/README.md`)**, **JPL-epoch mean-anomaly computation**
- **Colony**: **52 building types** across 8 categories, construction queue, population
- **Economy**: **38 resource types**, mining operations, energy grid, per-body stockpiles, **localized logistics**, **shipping-company AI**
- **Fleets**: 7 ship classes, 6 propulsion types, Hohmann transfers, **porkchop plot planner**, **Lagrange routing (L1–L5)**, **star-approach parking-radius picker**, gravity assists
- **Research**: 15 technology categories, prerequisite chains, modifiers, **9 v0.5.0 survey / personnel / geology techs**, **tier-1 paid research_cost rebalance**
- **Survey (v0.5.0)**: 8-dimension model, 17 instruments, 9-mission roster, **6 RON data files**, anomaly confidence, failure modes, recovery missions, **continuous orbital survey station**
- **Personnel (v0.5.0)**: **Scientists with 8 specialties, 3 seniority tiers, hire & promotion** (data layer shipped; Personnel Roster UI pending)
- **Notifications (v0.5.0)**: toast panel, **per-category settings**, **event bridges**, **2-second coalesce**, **click-to-focus**, **pause-on-event**
- **UI**: Egui-based panels with SimulationTime-driven updates, **theme tokens (CI-linted)**, **layout primitives (Tab, focus rings, spacing scale)**

### Simulation Time (CRITICAL)

Never use `Time<Virtual>` for game-world calculations. Use `SimulationTime` (in `src/ui/time.rs`) instead—it has no speed cap (enables up to 1 year/second), while Bevy's virtual time caps at ~15×.

```rust
// All game systems MUST use SimulationTime
fn system(time: Res<SimulationTime>) {
    let elapsed = time.elapsed_seconds(); // f64, no cap
}
```

Positional/rotational calculations must be **analytical** (compute from total elapsed time), not incremental.

### Bevy 0.18 Specifics

- Entity API: `Entity::index()` (not `row()`)
- Ambient lighting: `GlobalAmbientLight` (resource) or `AmbientLight` (component on Camera)
- State transitions: `NextState::set()` always triggers transitions; use `set_if_neq()` to skip
- Materials: bind groups use `@group(3)` in WGSL shaders
- Bloom: `bevy::post_process::bloom::Bloom`
- **B0001 (dual-Query rule):** a system function MUST NOT declare two
  separate `Query<...>` system parameters that both yield access to the
  same component (e.g. one `Query<(Entity, &LocalStockpile)>` and one
  `Query<&mut LocalStockpile>`). Bevy 0.18 rejects this with **error B0001**
  on the first schedule tick — `cargo build` and `cargo test` do not catch
  it, only `cargo run` does. The canonical fix is to fold the two queries
  into a single `Query<(Entity, &mut T)>` and call `iter()` then
  `get_mut(entity)` in sequence (see `process_company_ai`,
  `auto_freight_loop`, `process_fleet_logistics_assignments`).
  Acceptable alternatives:
  - `ParamSet<(Query<...>, Query<...>, ...)>` when you need both at once.
  - Filters that are statically disjoint (`With<A>` vs `Without<A>`) — see
    `propagate_orbits` for a `ParamSet` example with disjoint reads.
  Audit helper: `python3 scripts/audit_b0001.py src` (runs in CI).

### Egui Scheduling

All egui systems must run in `EguiPrimaryContextPass`, not `Update`:
```rust
.add_systems(EguiPrimaryContextPass, my_egui_system)
```

### Localized Resources

Resources in Helios are **physical** — they live on a specific body and have to be moved by ship to be used elsewhere. There are three layers:

- **`LocalStockpile`** is a `Component` on every body that produces, stores, or consumes resources. It is a `HashMap<ResourceType, f64>` in megatonnes; production (mining, atmospheric harvesting, food) deposits here, and consumption (maintenance, food, construction materials) deducts here. Construction draws **only** from the destination body's `LocalStockpile` — there is no system-pool fallback. When local materials are short, the construction system publishes a `ResourceRequest` and the building waits for delivery. See `src/economy/components.rs` (`LocalStockpile`) and `src/colony/systems.rs` (`process_construction_actions`).
- **`ContextualStockpile`** is a view-scoped `Resource` that aggregates `LocalStockpile`s for the player UI. The `update_contextual_stockpile` system reads `ViewMode` and `CurrentStarSystem` and sums: in **System view**, every body in the active star system; in **Starmap view**, every body across all systems. The label (`"Sol System"` vs `"All Systems"`) is set on the resource and surfaced in the top resource bar marquee. Construction does **not** read this — it is display-only. See `src/economy/budget.rs` (`ContextualStockpile`, `update_contextual_stockpile`).
- **Request / delivery flow.** When a body needs materials it cannot produce locally, a `ResourceRequest` is created in `PendingResourceRequests` (an ECS `Resource`). Requests have a `RequestPriority` (`Emergency` > `Construction` > `Maintenance` > `Trade`) and a `RequestState` (`Pending` → `Assigned` → `InTransit` → `Delivered`, or `Expired`). Triggers: construction that exceeds local stock, `MinimumStockpile` thresholds falling below their configured level, and life-support shortfalls (Emergency). Delivery is done by either a private `ShippingCompany` AI (see `src/economy/company.rs`) or a player-assigned Freighter fleet; `complete_deliveries` credits the destination `LocalStockpile` and unblocks the linked `ConstructionProject` when all linked requests are delivered. See `src/economy/logistics.rs`.

### Data-Driven Design

The modding surface is intentionally broad. All gameplay surfaces are RON-driven unless noted:

- Buildings: `assets/data/buildings.ron`
- Technologies: `assets/data/technologies.ron`
- Ship hulls: `assets/data/ship_hulls.ron`
- Ship modules: `assets/data/ship_modules.ron`
- Solar system: `assets/data/solar_system.ron`
- Stars: `assets/data/nearest_stars_raw.json`
- Exoplanets: `assets/data/Exoplanets_NASA.csv` (untracked; planned loader in `src/astronomy/exoplanets.rs`, ships with v0.6 — see `assets/data/README.md`)
- Freighter templates: `assets/data/freighter_templates.ron`
- Notifications: `assets/data/notifications.ron`
- Porkchop config: `assets/data/porkchop_config.ron`
- Survey (v0.5.0): `assets/data/survey/{dimensions,instruments,anomalies,tiers,mining_efficiency,missions,recovery_missions}.ron`

See `docs/MODDING.md` and `docs/RESEARCH_MODDING.md` for the full surface.

### Per-Trip Freight Cap (GRA-119)

A fleet's cargo capacity caps a single delivery. When a `ResourceRequest` exceeds the fleet's capacity, the request is split across multiple trips: the fleet picks up `min(request.amount, fleet.cargo)`, delivers it, then loops for the remainder. This prevents a single overloaded freighter from "consuming" a request in one go and lets players see the in-transit progress. See `src/economy/auto_freight.rs`.

### Shipbuilding Progression

- `src/ui/shipbuilding_workspace.rs` is the only shipbuilding UI path
- Ship module families should use `required_component_design` to share one engineering project instead of creating parallel unlock paths
- The corresponding technology should expose that family via `unlocks_engineering` so the tech tree and engineering list stay synchronized
- Validate ship data changes with both `cargo build` and `cargo run`; malformed RON and bad IDs are still runtime failures

## Fleet Mechanics

The fleet system uses:
- `FleetOrbit`: Circular parking orbit, angle advances at 1 rev/40s real time
- `ActiveManeuver`: Keplerian transfer arc, propagated analytically each frame
- `PendingFleetActions`: Thread-safe action queue for spawn/transfer/cancel
- Transfer planning: Hohmann transfers, gravity assists, phased departures, **porkchop plot planner (GRA-152 → GRA-162)**, **Lagrange-point transfers (L1–L5, GRA-154 → GRA-156)**, **star-approach parking-radius picker (GRA-161)**

Delta-v calculation uses Tsiolkovsky rocket equation: Δv = Isp × g₀ × ln(m_wet / m_dry)

## Notifications & Event Bus (v0.5.0, GRA-135 → GRA-142)

Notifications are the player-facing event bus. Survey / Construction / Research bridges write `NotificationEvent` messages; the spawn system attaches an `ActiveNotification` component; the tick system auto-dismisses and triggers `pause_on_event` (wired to `TimeScale::pause()`); the coalesce system deduplicates within a 2-second window; the click_handler dispatches a `context_link` (body / fleet / project) to the relevant panel.

The flow is:

```text
Bridge system  →  Messages<NotificationEvent>  →  spawn system  →  ActiveNotification
                                                                       ↓
                                                          tick / coalesce / click_handler
                                                                       ↓
                                                            render (top-right toast panel)
```

Per-category overrides live in `NotificationSettings` (resource) and the dedicated `ui_notifications_settings_panel` modal. See `src/ui/notifications/` and the `assets/data/notifications.ron` data file.

## Development Notes

- F12 toggles debug settings (free construction, instant build, tech bypass)
- Use `bevy-inspector-egui` in debug builds for runtime inspection
- Camera: WASD panning, right-click drag rotation, mouse wheel zoom, Home to recenter
- Starmap view activates at ~100 AU zoom distance
- Background music: CC-BY 4.0 (Scott Buckley), attribution overlay required
