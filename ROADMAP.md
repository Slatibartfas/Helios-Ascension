# Helios Ascension — Development Roadmap

This roadmap tracks the path from the current 0.5.0 release (a working single-system 4X with deep logistics, eight-dimension survey, notifications, porkchop planner, and Lagrange-point transfers) toward **v1.0** — a hard-sci-fi 4X grand-strategy game with realistic orbital mechanics, Kardashev-scale progression, and deep interstellar simulation.

Each section below lists the goal, the per-item status (✅ shipped · 🟡 in flight · ⬜ planned · 🔒 deferred), and pointers to the design spec / implementation file. Milestones roll up into the long-horizon vision at the bottom of the document.

---

## Current Status: v0.5.0 — Exploration & Progression 🟡 IN FLIGHT

The 0.4.0 building & logistics overhaul is fully shipped. v0.5.0 is mostly shipped as a series of PRs landed between May and June 2026:

- ✅ Survey rework (eight-dimension model, 9-mission roster, anomaly confidence, recovery missions, continuous orbital survey station, dossier `SURVEY` ledger) — GRA-79 → GRA-114
- ✅ Notification / event system (toast panel, settings modal, event bridges, coalesce, click-to-focus, pause-on-event) — GRA-135 → GRA-142
- ✅ Transfer planner hardening (porkchop plot, Lagrange routing L1–L5, star-approach parking-radius picker) — GRA-149 → GRA-162
- 🟡 Personnel system (data layer shipped: `Scientist` component, 8 specialties, 3 seniority tiers, hire & promotion systems; `PersonnelRoster` UI panel pending — design contract in `docs/UI.md` §8.3)
- 🟡 Progressive expansion (5.3) — outpost founding flow shipped; probes → rovers → stations → bases fleet is partially modelled; fuel depots and asteroid mining UI still pending
- ✅ 5-ship Day-1 constellation + 4 historical probes at the 2026-01-01 JPL epoch — GRA-128, GRA-131
- ✅ 9 new survey / personnel / geology techs (GRA-106) + tier-1 paid `research_cost` rebalance (GRA-127)

---

## Shipped: v0.4.x — Late-Game Logistics Follow-ups

The 0.4.0 release shipped a workable end-to-end logistics loop, but late-game players were hitting scaling, capacity, and missing-mechanic limits. Items land as 0.4.x patches (small Rust delta, mostly RON + UI) and roll into 0.5:

| Item | Status | Notes |
|------|:------:|-------|
| Per-trip freight cap (fleet cargo splits remainder across trips) | ✅ | GRA-119 |
| Auto-creation of `ResourceRequest` at outpost founding | ✅ | GRA-31 PR-A |
| Game-start & outpost `MinimumStockpile` defaults aligned with life-support scale | ✅ | GRA-31 PR-C |
| Private shipping overview top-level Logistics panel | ✅ | GRA-43 |
| Company freighter construction (companies reinvest profits into hull orders) | ✅ | GRA-39 |
| `predict_build_effect` cost-model rework | ✅ | (LGD) |
| Inter-system logistics | 🔒 | Design deferred to 0.5.x follow-ups; design spec at `docs/design/LOGISTICS_LATE_GAME.md` |
| Shipping-capacity market (auction, dynamic pricing, capacity reservation) | 🔒 | Deferred; one-company scenario doesn't yet exercise competing bidders |
| Mega / Gigaton freighter hulls (`SlotSize::XLarge` variant) | 🔒 | Design accepted at `docs/design/MEGA_GIGATON_FREIGHTER_TIERS.md`; parked for 0.5.1+ |
| Logistics network congestion / routing (traffic awareness, lane capacity, priority preemption) | 🔒 | Deferred; needs a real congestion model before priority preemption pays off |
| Uranium default minimum for fission colonies | ✅ | Shipped with GRA-31 PR-C |
| Company fleet icons on starmap | 🔒 | Deferred; surface after starmap icon system is generalised (0.6+) |
| Save / load | 🔒 | Deferred to v1.0 |

### Open questions for LGD (logistics depth vs game pacing)

These surfaced during the 0.4 review and need design input before they land in code:

1. Should private shipping companies be **opt-in** (default off) or **opt-out** (default on, balanced by treasury pressure)?
2. At what **empire size** (colony count or annual resource throughput) should the **Mega / Gigaton** hulls become available?
3. Do we want **bidding wars** between companies, or is **first-come-first-served** good enough for 0.5.x?
4. Is **inter-system logistics** a 0.5.x patch or a 0.6+ feature that informs the Exploration milestone?

See `helios-lgd/docs/design/` for the working design notes.

---

## v0.5.0 — Exploration & Progression 🟡 IN FLIGHT

Sequential exploration where you send probes first, then rovers, establish stations, then bases. The survey rework (5.1) and notification system (5.4) are **shipped**; the personnel system (5.2) is **data layer shipped / UI pending**; 5.3 is **partially shipped**.

### 5.1 Survey System Rework ✅ MOSTLY SHIPPED
- ✅ Remove three-tier survey system — replaced by the 8-dimension tiered model (PR #140 / GRA-98, 2026-06-10)
- ✅ Progressive discovery with probes — 9-mission roster in `missions.ron`, dispatched from the dossier SURVEY tab (PR #137 / GRA-80, PR #135 / GRA-82, 2026-06-08)
- ✅ Survey teams with scientist personnel — `Scientist` component + specialty + seniority enums live (PR #137)
- ✅ Gradually reveal resources, anomalies, landing sites — anomaly confidence model live (PR #136 / GRA-81); resource estimate tier display in Economy panel (PR #138 / GRA-84, 2026-06-08)
- ✅ Survey data collection and analysis — 6 RON files (dimensions, instruments, anomalies, tiers, mining efficiency, missions) on `main`
- ✅ 9 new techs from `SURVEY_REWORK.md` §[Tech Tree Integration] landed in `technologies.ron` (GRA-106, PR #151)
- 🟡 §10/§11 reconciliation in `docs/SURVEY.md` once the failure-mode and landing-site sections are finalised (PR-D, PR-G)
- ✅ Continuous orbital survey station with mining-yield bonus (GRA-83, PR #145)
- ✅ Failure modes & recovery missions (probe loss, rover stuck, drill bit stuck, solar storm, crew injury) — GRA-85, PR #148
- ✅ Mining-efficiency curve rebalance (GRA-117, PR #162)

### 5.2 Personnel System 🟡 PARTIAL
- ✅ Scientists for survey missions and research — `Scientist` component, 8 specialties, 3 seniority tiers in `src/personnel/`
- ✅ Personnel training and advancement — `seniority_promotion` system live; driven by completed survey missions
- ✅ `hire_scientists` system produces scientists from `University` buildings
- 🟡 Personnel Roster UI panel — `GameMenu::Personnel` stub in `dashboard.rs:1249`; design contract in `docs/UI.md` §8.3; implementation in `src/ui/personnel_panel.rs` not yet started
- ⬜ Generals for fleet operations (v0.6 candidate) — design contract TBD
- ⬜ Governors for colony management (v0.6 candidate) — design contract TBD
- ⬜ Auto-assign heuristics for scientists → analysis jobs

### 5.3 Progressive Expansion 🟡 PARTIAL
- ✅ Outpost founding flow (`EstablishOutpostRequest`) with habitability gate, atmosphere requirements, starter-package buildings, and per-person running costs
- ✅ Per-body resource delivery at outpost founding (resource requests auto-created and filled by player fleet or shipping company)
- ⬜ Probe deployment (cheap, expendable) — small_probe_frame, micro_probe_frame already in `ship_hulls.ron`; gameplay loop needs to drive player demand
- ⬜ Rover surface missions — rover_frame in `ship_hulls.ron`; rover-only missions in `missions.ron` (PR-D) need surface-loiter plumbing
- ⬜ Orbital stations around moons/planets — `Station` ship class scaffolded; full orbital-station construction pipeline pending
- ⬜ Surface bases (Mars, Moon, asteroids) — colony slots on top of mining operations
- 🟡 Asteroid mining operations — `Mine`/`Refinery`/`StripMine` buildings are mineral-class agnostic; per-body deposit gating by `Mineral deposits` tier needs additional UI
- ⬜ Fuel depots and refuelling points — needs a `FuelDepot` building variant and a fuel-only `ResourceRequest` priority

### 5.4 Notification & Event System ✅ SHIPPED
- ✅ In-game notification toast panel (PR-A scaffold, PR-B render + tick, GRA-136)
- ✅ Story events and milestones — Survey / Construction / Research event bridges (GRA-137)
- ✅ Random events (discoveries, disasters, opportunities) — anomaly triggers, recovery-mission outcomes, mining-yield shocks
- ✅ Event log and history — toast log + dismiss queue
- ✅ Event-triggered missions — recovery missions auto-spawned from failure modes (GRA-85)
- ✅ Coalesce / group with 2 s default window (GRA-138)
- ✅ Per-category settings panel (GRA-139)
- ✅ Pause-on-event (GRA-140)
- ✅ Click-to-focus context_link dispatcher (GRA-141)
- ✅ End-to-end integration tests (GRA-142)

### 5.5 Transfer Planner Hardening ✅ SHIPPED
- ✅ Porkchop plot RON loader + math + UI (GRA-152, PR #182)
- ✅ Parking radius wiring (GRA-149, PR #181)
- ✅ Interactive star-approach parking-radius picker (GRA-161, PR #189)
- ✅ Lagrange-point transfers L1 / L2 / L3 / L4 / L5 (GRA-154 → GRA-156, PR #183 / #185)
- ✅ L4 fallback to single Hohmann (GRA-154, PR #185)
- ✅ Destination state-mutation contract (GRA-160, PR #188)
- ✅ HIGH/MEDIUM-severity transfer-planner fixes (GRA-153, PR #186)
- ✅ Deferred porkchop grid build for non-click entries (PR #187)
- ✅ Tooltip visibility enhancements (PR #186 / #188)

---

## v0.5.x — Personnel UI & Progressive-Expansion Close-Out 🟡 NEXT

The personnel system is the only v0.5 surface still missing UI. The progressive-expansion close-out fills the 5.3 gaps above. Both are small Rust deltas; mostly RON, UI wiring, and existing-system hooks.

### 5.x.1 Personnel UI
- ⬜ `src/ui/personnel_panel.rs` — scientist roster (filter by specialty, seniority, current assignment)
- ⬜ Analysis-queue sub-panel: assign / unassign scientists to analysis jobs
- ⬜ `University` building production view (junior / senior / principal pipeline)
- ⬜ Personnel cap visualisation (soft-cap penalty indicator)
- ⬜ Promotion history view (per-scientist career timeline)

### 5.x.2 Progressive Expansion Close-Out
- ⬜ Probe deployment UI (build & launch probe from Shipbuilding workspace; auto-attached to a survey mission)
- ⬜ Rover surface-loiter plumbing (rover as a per-body `RoverEntity`; auto-assigned to a survey site)
- ⬜ Fuel depot building + fuel-only request priority
- ⬜ Asteroid mining UI (per-body resource grid already shipped; mining-side UX needs an asteroid-specific path)
- ⬜ Orbital station construction pipeline (Station class → in-orbit spawn → docking hooks)
- ⬜ Surface-base settlement model (colony slot + surface facilities on top of mining)

---

## v0.6.0 — Interstellar Travel & Starmap

The interstellar hand-off is the next major arc. The `interstellar_probe` tech (tier 5) and the exoplanet ingestion (`src/astronomy/exoplanets.rs`, 5 000+ confirmed planets) are pre-requisites and already on `main`. The 0.6 milestone closes the loop on cross-system play.

### 6.1 Interstellar Propulsion & Transfer
- ⬜ Alcubierre / warp-drive speculative propulsion (requires `ExoticMatter` & `Metamaterials` fuels)
- ⬜ Generation ship class (multi-decade cruise; `AntimatterDrive` core + closed-cycle life support)
- ⬜ Project Orion / nuclear-pulse fleets for fast outer-system transits
- ⬜ Starmap routing graph (one edge per system pair, Δv-weighted)
- ⬜ Cross-system Hohmann transfers (departure window when bodies are within a configurable phase angle)

### 6.2 Multi-System Simulation
- ⬜ Active-system isolation (per-system resource pools; cross-system pooled only via freighters)
- ⬜ Starmap view overhaul: real planet positions within visited systems (current procedural fallback → confirmed `ConfirmedPlanet` entities)
- ⬜ `CurrentStarSystem` resource extended for system-switching
- ⬜ Inter-system logistics baseline (Mega/Gigaton freighter hulls unlocked here)

### 6.3 First Contact & SETI Hooks
- ⬜ SETI listening post (radio-astronomy instrument + signal-detection roll)
- ⬜ Signal anomaly events (decoded message fragments; unlocks research projects)
- ⬜ First-contact scenarios (neutral / hostile / indifferent AI factions)

---

## v0.7.0 — AI Factions, Diplomacy & Combat

AI-controlled factions competing for resources, territory, and ideological advantage.

### 7.1 AI Factions
- ⬜ Multiple AI factions with distinct behaviours (Expansionist / Isolationist / Technologist / Spiritualist / Militarist)
- ⬜ Per-faction agenda & goal priority (resource hoarding, territorial control, scientific primacy, military buildup)
- ⬜ Faction reputation & relationship matrix
- ⬜ AI diplomacy (treaties, trade pacts, non-aggression, embargo, war)

### 7.2 Competition Mechanics
- ⬜ Territory influence system (per-body influence projection; contested vs controlled)
- ⬜ Resource competition (faction-vs-faction bids on asteroids, mining sites, anomaly sites)
- ⬜ Strategic location control (L4 / L5 Lagrange points, chokepoints, fuel depots)
- ⬜ Victory conditions (Terraformer, Supremacist, Diplomat, Scientist, Conqueror)
- ⬜ End-game scoring (Kardashev level + colony count + tech milestones + diplomatic standing)

### 7.3 Combat (Hard-Sci-Fi Constraint Set)
- ⬜ Ship-to-ship combat at high relativistic speeds (engagement ranges 10⁵ – 10⁸ km)
- ⬜ Kinetic-kill vehicles, laser weapons, plasma cannons, particle beams
- ⬜ Point-defense & electronic warfare
- ⬜ Missile magazines & deep-space intercepts
- ⬜ Combat is **not** a primary 4X loop in v1.0; combat-first factions are a player choice. Combat resolution at planetary distances is decoupled from RTS-style micro
- ⬜ `GroundDefenseBattery` and `MissileSilo` buildings become functional (currently scaffolding only)

---

## v0.8.0 — Terraforming & Planetary Engineering

Long-term planetary modification as a late-game 4X pillar. The `OrbitalSurveyStation` continuous-yield bonus and the `BuildingType::category()` hook for atmospheric buildings are the pre-requisites already on `main`.

### 8.1 Terraforming Pipeline
- ⬜ Per-body terraforming index (atmosphere, hydrosphere, surface temperature, biosphere)
- ⬜ Atmospheric processors as terraforming engines (thicken CO₂, add O₂, scrub pollutants)
- ⬜ Orbital shade / sunshade mirrors (modulate insolation)
- ⬜ Comet redirect (impactor missions that deliver volatiles to airless worlds)
- ⬜ Biosphere engineering (closed-loop ecology; introduces `closed_loop_ecology` tech effect)

### 8.2 Megastructures (Late-Game)
- ⬜ Orbital ring habitats (Earth-orbit scale ringworld slice; closed-cycle life support)
- ⬜ Dyson swarm (partial sphere of solar collectors; harvest stellar output at scale)
- ⬜ Space elevator (Earth-orbit + Mars-orbit + Moon-orbit variants)
- ⬜ Lagrange-point colonies (L4 / L5 permanent habitats; multi-billion population)
- ⬜ Stellar lifting (`Dyson` extension; mining the Sun itself via `Metamaterials` collectors)

---

## v0.9.0 — Kardashev Progression & End-Game Loop

The Kardashev scale is the spine of the long game. Progression from K0.7 (current day) to K2 (stellar) and beyond is gated by tech, megastructure completion, and empire-scale resource throughput.

### 9.1 Kardashev Tiers (Civilisation Score Anchors)
- ⬜ **K0.7** — current start (2026 civilisation; ~18 TW planetary)
- ⬜ **K1.0** — full planetary mastery (~10¹⁶ W; one Earth, fully utilised)
- ⬜ **K1.5** — system mastery (~10²¹ W; full Sol-system industrial output)
- ⬜ **K2.0** — stellar mastery (~10²⁶ W; partial Dyson swarm)
- ⬜ **K2.5+** — multi-stellar / speculative (closed timelike curves, computronium substrates)

### 9.2 Late-Game Resources
- ⬜ `Antimatter` production (particle accelerator farms; requires `Metamaterials` collectors)
- ⬜ `ExoticMatter` and `Metamaterials` (K2-tier crafting chains)
- ⬜ `Computronium` (post-singularity computation substrate; late-game research engine)

### 9.3 Economy at Scale
- ⬜ Realistic energy grid (per-body W generated, distributed, and consumed; GW / TW / PW units)
- ⬜ Trade economy with markets (supply / demand, price elasticity, scarcity rent)
- ⬜ Corporate finances (per-colony / per-sector budgets; loans, credit, dividends)
- ⬜ Taxation & revenue (per-colony tax rates, tariffs, stockpile-value fees)
- ⬜ Mega-engineering projects (year-to-decade builds; mid-save suspension & resume)

---

## v1.0.0 — Release

- ⬜ Feature complete against the 4X hard-sci-fi vision
- ⬜ Balanced gameplay (mining, research, economy, expansion)
- ⬜ **Save / load** (the persistence layer; gates the post-release community)
- ⬜ Full documentation (README, ROADMAP, ARCHITECTURE, all `docs/`)
- ⬜ Tutorial system (mission-guided intro to survey → construction → expansion)
- ⬜ Bug fixing & community feedback integration
- ⬜ Modding release: documented surface for the 6 survey RON files, `buildings.ron`, `technologies.ron`, `ship_hulls.ron`, `ship_modules.ron`, `solar_system.ron`, `freighter_templates.ron`, `notifications.ron`, `porkchop_config.ron`

---

## Milestones — Shipped

### v0.1.0 — Foundation ✅
- Bevy engine integration
- Plugin architecture
- Solar system simulation with 377 bodies
- Debug UI with inspector

### v0.2.0 — Core Mechanics ✅
- 20 → 38 resource types (now extends to specialty / fusion / late-game)
- Research tree (15 tech categories)
- Colony management (29 → 52 building types)
- Economy and budget tracking
- Time management with variable speeds
- Comprehensive UI panels
- Starmap with 60 nearby star systems

### v0.3.0 — Fleet & Orbital Transfer ✅
- Fleet management system (7 ship classes, 6 propulsion types)
- Keplerian transfer arc propagation
- 3 transfer options per route (Hohmann / moderate / fast)
- Transfer window planner with synodic-period countdown
- Phased departure planning
- Gravity-assist flyby candidates
- Lagrange-point targeting (L4 / L5, L1 / L2 / L3)
- Fleet intercept planning
- Mid-transit course correction and abort burns
- Refuelling from planetary stockpiles
- Full trajectory visualisation
- Background music playlist (CC-BY, Scott Buckley)
- Atmospheric scattering shader (Rayleigh + Mie)

### v0.4.0 — Building & Logistics Overhaul ✅
- **52 building types** (was 31) with 4–6-resource maintenance, tiers, synergies, atmosphere availability
- Per-body resource stockpiles (planets, moons, asteroids, ships, stations)
- Resource request system (Emergency → Construction → Maintenance → Trade priorities)
- Player-directed freight (assign a Freighter fleet to a request from the Fleet panel)
- Private shipping companies — AI freighters, treasury → company credit, auto-reinvest
- Per-colony minimum stockpile editor (default O₂ = 200 Mt, Water = 100 Mt for Life Support bodies)
- In-transit resource indicator on the resource bar
- Atmosphere-availability field on cross-atmosphere buildings
- Construction panel rebuild — yield chip, depletion timeline, redesigned cards
- Native Bevy shipbuilding workspace (hulls, modules, engineering-linked components, construction queueing, archive)
- Freighter template system (light / mid / heavy) + legacy `standard_freighter` migration shim
- Company freighter construction (companies build their own freighters at shipyards)
- Private shipping overview subpanel + in-transit freighter filter in Fleet panel
- Per-trip freight cap (fleet cargo splits remainder across trips) — GRA-119
- Game-start & outpost `MinimumStockpile` defaults aligned with life-support scale — GRA-31 PR-C

### v0.5.0 — Exploration & Progression (🟡 IN FLIGHT — most shipped)
- 8-dimension survey rework (6 RON files, 9-mission roster, anomaly confidence model, dossier SURVEY tab) — shipped 2026-06-08
- Scientist data layer (specialty, seniority, hiring, promotion) — shipped
- 9 new survey / personnel / geology techs (GRA-106) — shipped
- Notification / event system (toast, settings, bridges, coalesce, click-to-focus, pause-on-event) — shipped
- Porkchop plot planner (GRA-152 → GRA-162) — shipped
- Lagrange-point transfers L1 / L2 / L3 / L4 / L5 (GRA-154 → GRA-156) — shipped
- Star-approach parking-radius picker (GRA-161) — shipped
- 5-ship Day-1 constellation + 4 historical probes at the 2026-01-01 JPL epoch — GRA-128, GRA-131
- Generals / Governors / Personnel Roster UI — **pending**

### Upcoming (next)
- v0.5.x — Personnel UI, progressive-expansion close-out (probe/rover/asteroid/fuel-depot/station)
- v0.6.0 — Interstellar travel & multi-system simulation
- v0.7.0 — AI factions, diplomacy, combat
- v0.8.0 — Terraforming & megastructures
- v0.9.0 — Kardashev progression & end-game loop
- v1.0.0 — Release (save/load, tutorial, full docs)

---

## The Long-Horizon Vision — 4X Hard-Sci-Fi

Helios Ascension aims to be the **most realistic 4X hard-sci-fi game** playable today. The pillars of that vision:

1. **Realistic orbital mechanics.** Every fleet, probe, station, and Lagrange transfer is computed against Keplerian orbits with analytic time-propagation. Porkchop plots, synodic-period windows, gravity assists, and Lagrange-point routing are first-class; nothing is fudged.
2. **Realistic logistics.** Resources are physical, per-body, and must be carried by ships. Construction draws only from local stockpiles. Private shipping companies and player freighters compete for delivery contracts. Mega/Gigaton freighters move civilisation-scale tonnage.
3. **Realistic survey.** Eight independent dimensions, 17 instruments, 9 mission archetypes, failure modes, recovery missions, and continuous orbital survey stations. Survey is the analysis engine that drives both mining and anomaly discovery.
4. **Realistic personnel.** Scientists, generals, and governors with specialties, seniority, and promotion ladders. Personnel caps are tech-gated. The PersonnelRoster panel is the human-side of the analysis engine.
5. **Realistic progression.** Kardashev scale anchors the end-game. K1.0 is planetary mastery, K2.0 is stellar mastery via Dyson swarm, K2.5+ is post-singularity computation substrate.
6. **Realistic combat (where it appears).** Combat at planetary distances is decoupled from RTS-style micro; high-relativistic engagement with kinetic-kill, laser, and plasma weapons at ranges of 10⁵ – 10⁸ km.
7. **Realistic megastructures.** Orbital rings, space elevators, Lagrange colonies, Dyson swarms — gated by `Metamaterials` and `Computronium` resource chains.
8. **Realistic modding.** All gameplay surfaces — buildings, technologies, ship hulls, ship modules, solar system, freighter templates, notifications, porkchop config, survey dimensions, instruments, anomalies, missions, recovery missions, mining efficiency — are RON-driven. A modder can ship an entire new campaign without touching Rust.

The roadmap above is the path to that vision. v1.0 is not the end of the project; it is the first stable hand-off to the community. Post-v1.0 work continues toward K2.0+ milestones and beyond.

---

## Contributing

Want to help? See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Priority areas for contribution:
1. **Personnel UI** (v0.5.x) — `src/ui/personnel_panel.rs` from the `docs/UI.md` §8.3 design contract
2. **Progressive expansion close-out** (v0.5.x) — probe / rover / fuel-depot / station pipelines
3. **Save / load** (v1.0) — the persistence layer is the largest single blocker to v1.0
4. **Mega / Gigaton freighter hulls** — design accepted at `docs/design/MEGA_GIGATON_FREIGHTER_TIERS.md`
5. **Modding surface** — additional RON-driven surfaces to reduce the Rust-mod surface
6. **Game balance** — mining yield curves, research pacing, economy dynamics

---

*This roadmap is subject to change based on development priorities.*

Last Updated: 2026-06-28