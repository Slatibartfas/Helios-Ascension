# Helios Ascension - Development Roadmap

## Current Status: v0.4.0 - Building & Logistics Overhaul ✅ SHIPPED

The building system overhaul and localized logistics network are now live. Resources are stored per-body, freighters (player and AI) carry them, and private shipping companies automate long-haul deliveries. v0.4.x follow-ups (late-game hardening, Mega/Gigaton freighters, inter-system logistics) are tracked in a new section below.

**v0.5.0 is 🟡 IN FLIGHT** — the survey rework is mostly shipped (8-dimension model, 6 RON files, 9-mission roster, dossier SURVEY tab); the personnel system is partially shipped (data layer only, UI panel pending). See §5.1, §5.2 below for the per-item status.

## v0.3.0: Fleet & Orbital Transfer System Complete ✅

The game has a fully implemented fleet management system with realistic orbital mechanics, transfer planning, gravity-assist routing, and Lagrange-point targeting. Core gameplay loop is functional and ships fly!

---

## v0.4.0: Building System Overhaul & Localized Logistics ✅ COMPLETE

Transform the building system to offer much more variety and player choices, with resources now stored per-body rather than globally.

### 4.1 Building System Redesign ✅
- [x] Expand building types for greater variety and specialization (now **51** building types, up from 31)
- [x] Have buildings consume all resources by adding more and update the existing ones
- [x] Building tiers with upgrade paths
- [x] Unique building effects and strategic choices
- [x] Building synergies
- [x] Remove global resources — each body/ship/station has its own storage

### 4.2 Localized Resource Storage ✅
- [x] Per-body resource stockpiles (planets, moons, asteroids)
- [x] Per-ship resource storage (cargo capacity)
- [x] Per-station resource storage
- [x] Resource transfer mechanics between locations (Freighter fleets + automated shipping)
- [x] Storage capacity limits per location

### 4.3 Logistics Network

Inspired by Aurora 4X (player-directed logistics) and Distant Worlds 2 (private sector automation).

**Core mechanic:** Every solar system has its own logistics network.  Resources are physically located on individual bodies and must be carried by Freighter ships to be used elsewhere.  The UI shows aggregated system-wide stockpiles for visibility, but construction and consumption draw from **local stockpiles only**.

**Resource requests (Phase 1):** ✅
- [x] `ResourceRequest` component generated when construction needs materials not present locally
- [x] Requests generated when an outpost is founded (full starter-package materials needed at destination)
- [x] Priority tiers: Emergency → Construction → Maintenance → Trade
- [x] Requests visible in the new **Logistics** panel tab (Private Shipping overview — subpanel)

**Player-directed transport (Phase 2):** ✅
- [x] Fleet panel lists open resource requests at each body
- [x] Assign a Freighter fleet to a specific request from the Fleet panel
- [x] Fleet arrival auto-delivers cargo and closes the request
- [x] Manual control for players who prefer Aurora 4X-style micromanagement

**Private shipping companies (Phase 3):** ✅
- [x] `ShippingCompany` resource — AI-controlled freighter fleets operating autonomously
- [x] Companies scan open requests and assign their nearest available freighter
- [x] Payment: credits deducted from player treasury → credited to company
- [x] Payment formula: `base_rate × amount_mt × distance_au × priority_multiplier`
- [x] Companies reinvest profits to purchase additional ships at shipyards
- [x] Starting state: one company with 2–3 chemical freighters at Earth; new companies emerge as economy grows
- [ ] Company fleet icons visible on starmap; hover to see active routes _(deferred — see follow-ups)_
- [x] Company registry in Logistics panel (name, ship count, active routes, treasury)

**Minimum stockpile (Phase 4):** ✅
- [x] Per-colony, per-resource minimum stockpile threshold (player-configurable per resource)
- [x] When local stockpile drops below threshold → auto-create Maintenance-priority request
- [x] Freighter (player or company) dispatched to top up the colony
- [x] Default minimums for Life Support bodies: O₂ = 200 Mt, Water = 100 Mt (Uranium default deferred)
- [x] UI: minimum input field per resource row in colony dossier with ETA display
- [x] "In transit" indicator on resource bar showing amount en route

See `docs/design/LOGISTICS_NETWORK.md` for the full design specification and per-phase shipped status.

### 4.4 Ship & Station Designer 🟡 PARTIAL
- [x] Modular ship design interface (native Bevy workspace, engineering-linked component selection)
- [x] Component selection (hulls, engines, cargo bays, weapons, sensors, life support)
- [ ] Station module builder _(see v0.4.x follow-ups)_
- [x] Design cost calculation
- [x] Ship/station naming
- [x] Freighter template system: light / mid / heavy freighter hulls with cargo-bay-derived capacity
- [x] Legacy `standard_freighter` migration shim to the new template system

---

## v0.4.x — Late-Game Logistics Follow-ups 🆕

The 0.4.0 release shipped a workable end-to-end logistics loop, but it is **MVP for early-to-mid game**. Late-game players will hit scaling, capacity, and missing-mechanic limits. Tracked as 0.4.x patches (small Rust delta, mostly RON + UI), to be sequenced into 0.5:

- [ ] **Inter-system logistics** — current design is intra-system only; no market, no shared pool, no convoy routes between stars
- [ ] **Shipping-capacity market** — no auction, dynamic pricing, or capacity reservation; one-company scenario doesn't exercise competing bidders
- [ ] **Mega / Gigaton freighter hulls** — design accepted (`docs/design/MEGA_GIGATON_FREIGHTER_TIERS.md`); Rust delta = one additive `SlotSize::XLarge` variant, parked for 0.4.2+
- [ ] **Logistics network congestion / routing** — no traffic awareness, no lane capacity, no priority preemption
- [ ] **Outpost `ResourceRequest` auto-generation at founding** — review whether the current implementation is fully wired (the spec says yes, but the per-building `ResourceRequest` trigger path is not yet end-to-end tested)
- [ ] **Uranium default minimum for fission colonies** — deferred from 0.4 Phase 4
- [ ] **Company fleet icons on starmap** — deferred from 0.4 Phase 3
- [ ] **Save/load** — not yet implemented; would expose logistics state to the persistence layer
- [ ] **Logistics panel as a top-level tab** — currently surfaced as a subpanel of the Economy panel
- [ ] **`predict_build_effect` cost model rework** — hardcoded food rates caught at the 0.4 boundary; separate LGD schema refactor

### Open questions for LGD (logistics depth vs game pacing)

These were surfaced during the 0.4 review and need design input before they land in code:

1. Should private shipping companies be **opt-in** (default off) or **opt-out** (default on, balanced by treasury pressure)?
2. At what **empire size** (colony count or annual resource throughput) should the **Mega / Gigaton** hulls become available?
3. Do we want **bidding wars** between companies, or is **first-come-first-served** good enough for 0.4.x?
4. Is **inter-system logistics** a 0.4.x patch or a 0.5+ feature that informs the Exploration milestone?

See `helios-lgd/docs/design/` for the working design notes.

---

## v0.5.0: Exploration & Progression System 🟡 IN FLIGHT

Sequential exploration where you send probes first, then rovers, establish stations, then bases. The survey rework (5.1) is **mostly shipped** as a series of PRs; the personnel system (5.2) is **partially shipped** (data layer only, UI pending); 5.3 and 5.4 are still open.

### 5.1 Survey System Rework 🟡 MOSTLY SHIPPED
- [x] Remove three-tier survey system — replaced by the 8-dimension tiered model (PR #140 / GRA-98, 2026-06-10)
- [x] Progressive discovery with probes — 9-mission roster in `missions.ron`, dispatched from the dossier SURVEY tab (PR #137 / GRA-80, PR #135 / GRA-82, 2026-06-08)
- [x] Survey teams with scientist personnel — `Scientist` component + specialty + seniority enums live (PR #137)
- [x] Gradually reveal resources, anomalies, landing sites — anomaly confidence model live (PR #136 / GRA-81); resource estimate tier display in Economy panel (PR #138 / GRA-84, 2026-06-08)
- [x] Survey data collection and analysis — 6 RON files (dimensions, instruments, anomalies, tiers, mining efficiency, missions) on `main`
- [ ] 9 new techs from `SURVEY_REWORK.md` §[Tech Tree Integration] land in `technologies.ron` (Coder-side PR; design contract documented in `docs/RESEARCH_MODDING.md` §[v0.5.0 Additions] and reconciled in [PR #154](https://github.com/Slatibartfas/Helios-Ascension/pull/154))
- [ ] §10/§11 reconciliation in `docs/SURVEY.md` once the Coder-side finalization lands

### 5.2 Personnel System 🟡 PARTIAL
- [x] Scientists for survey missions and research — `Scientist` component, 8 specialties, 3 seniority tiers in `src/personnel/`
- [ ] Generals for fleet operations — not yet specced (v0.6 candidate)
- [ ] Governors for colony management — not yet specced (v0.6 candidate)
- [x] Personnel training and advancement — `seniority_promotion` system live; driven by completed survey missions
- [ ] Mission assignment interface — `GameMenu::Personnel` wired in `dashboard.rs:1249`, panel UI is the design contract in `docs/UI.md` §8.3 (Preview; `src/ui/personnel_panel.rs` not yet implemented)

### 5.3 Progressive Expansion
- [ ] Probe deployment (cheap, expendable)
- [ ] Rover surface missions
- [ ] Orbital stations around moons/planets
- [ ] Surface bases (Mars, Moon, asteroids)
- [ ] Asteroid mining operations
- [ ] Fuel depots and refueling points

### 5.4 Notification & Event System
- [ ] In-game notification system
- [ ] Story events and milestones
- [ ] Random events (discoveries, disasters, opportunities)
- [ ] Event log and history
- [ ] Event-triggered missions

---

## v0.6.0: AI Competition & Factions

AI-controlled factions competing for resources and territory.

### 6.1 AI Factions
- [ ] Multiple AI factions with distinct behaviors
- [ ] Resource management AI
- [ ] Expansion priorities
- [ ] Research focus AI
- [ ] Military build-up AI

### 6.2 Competition Mechanics
- [ ] Territory influence system
- [ ] Resource competition
- [ ] Strategic location control
- [ ] Faction relations (alliances, rivalries)
- [ ] Victory conditions

### 6.3 Diplomacy
- [ ] Diplomatic interface
- [ ] Trade agreements
- [ ] Technology sharing
- [ ] Military pacts
- [ ] Negotiation mechanics

---

## v0.7.0: Financial System Overhaul

Complete rework of economy and finances.

### 7.1 Corporate Finances
- [ ] Budget management per colony/sector
- [ ] Revenue and expenses tracking
- [ ] Loan and credit system
- [ ] Stockpile value calculations
- [ ] Financial reports and analytics

### 7.2 Trade Economy
- [ ] Local markets per location
- [ ] Supply and demand pricing
- [ ] Trade route profitability
- [ ] Smuggling and black markets
- [ ] Economic events (booms, recessions)

### 7.3 Taxation & Revenue
- [ ] Tax rates per colony
- [ ] Trade tariffs
- [ ] Resource export fees
- [ ] Colony upkeep costs

---

## v0.8.0: Technology Tree Rework

Progression-locked tech tree aligned with exploration milestones.

### 8.1 Tech Tree Restructure
- [ ] Sequential tech unlocks tied to exploration
- [ ] Probe → Rover → Station → Base progression
- [ ] Technology prerequisites from gameplay milestones
- [ ] Tech tiers that require reaching certain bodies

### 8.2 Tech Categories
- [ ] Exploration tech (probes, sensors, communications)
- [ ] Propulsion tech (unlock better engines)
- [ ] Colony tech (habitation, life support)
- [ ] Military tech (weapons, defenses)
- [ ] Economy tech (trade, mining efficiency)

### 8.3 Tech Effects
- [ ] Unlock new building types
- [ ] Ship component unlocks
- [ ] Efficiency bonuses
- [ ] New personnel roles

---

## v0.9.0: Balance & Polish

### 9.1 Game Balance
- [ ] Mining rate balancing
- [ ] Research speed tuning
- [ ] Resource availability curves
- [ ] AI difficulty scaling
- [ ] Economy balancing

### 9.2 UI/UX Improvements
- [ ] Streamlined interfaces
- [ ] Better information displays
- [ ] Tooltip improvements
- [ ] Keyboard shortcuts
- [ ] Tutorial system

### 9.3 Audio
- [ ] Complete sound effects
- [ ] Ambient space audio
- [ ] UI sound feedback
- [ ] Music expansion
- [ ] Volume controls

### 9.4 Performance
- [ ] Optimization pass
- [ ] Memory usage reduction
- [ ] Frame rate improvements
- [ ] Load time reduction

---

## v1.0.0: Release

- Feature complete
- Balanced gameplay
- Save/load system
- Documentation
- Bug fixing
- Community feedback integration

---

## Milestones

- **v0.1.0** - Foundation ✅ COMPLETE
  - Bevy engine integration
  - Plugin architecture
  - Solar system simulation with 377 bodies
  - Debug UI with inspector

- **v0.2.0** - Core Mechanics ✅ COMPLETE
  - Resource system (20 resource types)
  - Research tree (15 tech categories)
  - Colony management (29 building types)
  - Economy and budget tracking
  - Time management with variable speeds
  - Comprehensive UI panels
  - Starmap with 60 nearby star systems

- **v0.3.0** - Fleet & Orbital Transfer ✅ COMPLETE
  - Fleet management system (7 ship classes, 5 propulsion types)
  - Keplerian transfer arc propagation
  - 3 transfer options per route (Hohmann / moderate / fast)
  - Transfer window planner with synodic-period countdown
  - Phased departure planning
  - Gravity-assist flyby candidates
  - Lagrange-point targeting (L4/L5, L1/L2/L3)
  - Fleet intercept planning
  - Mid-transit course correction and abort burns
  - Refuelling from planetary stockpiles
  - Full trajectory visualisation
  - Background music playlist (CC-BY, Scott Buckley)
  - Atmospheric scattering shader (Rayleigh + Mie)

- **v0.4.0** - Building & Logistics Overhaul ✅ COMPLETE
  - 51 building types (was 31) with 4–6-resource maintenance, tiers, and synergies
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
  - 0.4.x follow-ups (late-game logistics hardening) tracked above

- **v0.5.0** - Exploration & Progression (NEXT, 🟡 IN FLIGHT)
  - 8-dimension survey rework (6 RON files, 9-mission roster, anomaly confidence model, dossier SURVEY tab) — **shipped** 2026-06-08
  - Scientist data layer (specialty, seniority, hiring, promotion) — **shipped**; **PersonnelRoster UI panel pending** (Preview in `docs/UI.md` §8.3)
  - 9 new survey / personnel / geology techs from `SURVEY_REWORK.md` §[Tech Tree Integration] — **Coder-side PR pending**
  - Generals / Governors / Auto-Assign — not yet specced (5.2 partial)
  - Sequential expansion (probes → rovers → stations → bases) — open (5.3)
  - Notification and event system — open (5.4)

- **v0.6.0** - AI Competition
  - AI factions with distinct behaviors
  - Competition for resources and territory
  - Diplomatic system

- **v0.7.0** - Financial System
  - Corporate budget management
  - Trade economy with markets
  - Taxation and revenue

- **v0.8.0** - Technology Rework
  - Progression-locked tech tree
  - Exploration milestones unlock tech
  - New personnel roles

- **v0.9.0** - Balance & Polish
  - Game balance (mining, research, economy)
  - UI/UX improvements
  - Audio completion
  - Performance optimization

- **v1.0.0** - Release
  - Feature complete
  - Balanced gameplay
  - Full documentation
  - Save/load system

---

## Contributing

Want to help? See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on how to contribute to the project.

Priority areas for contribution:
1. Late-game logistics hardening (inter-system, market, Mega/Gigaton hulls)
2. Personnel system
3. UI/UX design
4. Game balance
5. Save/load system

---

*This roadmap is subject to change based on development priorities.*

Last Updated: 2026-06-12
