# Helios Ascension - Development Roadmap

## Current Status: v0.3.0 - Fleet & Orbital Transfer System Complete ✅

The game has a fully implemented fleet management system with realistic orbital mechanics, transfer planning, gravity-assist routing, and Lagrange-point targeting. Core gameplay loop is functional and ships fly!

## Phase 1: Core Mechanics ✅ COMPLETE

### 1.1 Resource System ✅
- [x] Define resource types (20 types: minerals, energy, volatiles, gases, fissiles, specialty materials)
- [x] Resource extraction mechanics (mining buildings)
- [x] Resource storage and management (stockpiles with capacity limits)
- [x] Resource transport and logistics (logistics buildings and efficiency system)

### 1.2 Research System ✅
- [x] Technology tree structure (15 categories)
- [x] Research mechanics (time, resources, research points)
- [x] Technology prerequisites (dependency chains)
- [x] Technology effects on gameplay (tech modifiers, building unlocks)

### 1.3 Time System ✅
- [x] Game time management (SimulationTime resource)
- [x] Time acceleration controls (1 day/s to 1 year/s)
- [x] Pause/resume functionality
- [x] Event scheduling (construction, research progression)

## Phase 2: Space Infrastructure ✅ COMPLETE

### 2.1 Colony Buildings ✅
- [x] Building construction mechanics (29 building types)
- [x] Building categories and specializations (8 categories)
- [x] Construction queue system
- [x] Building functionality (mining, research, habitation, industry, logistics, power, financial, military)

### 2.2 Spacecraft ✅
- [x] Ship design system foundation
- [x] **7 ship classes**: Courier, Frigate, Destroyer, Cruiser, Research Vessel, Freighter, Station
- [x] **6 propulsion types**: Chemical, Nuclear Thermal, Ion Drive, Nuclear Pulse, Fusion Torch, Antimatter Drive
- [x] Tsiolkovsky rocket-equation Δv and fuel calculations
- [x] Fleet spawning from launch sites (Shipyard, Launch Site buildings)
- [x] Fleet movement — orbital transfer arcs with Keplerian propagation
- [x] **3 transfer options** per route (efficient Hohmann, moderate, fast)
- [x] Transfer window planner with synodic-period countdown and phase-angle display
- [x] Phased departure planning (departure-time slider)
- [x] Gravity-assist flyby candidate computation
- [x] Lagrange-point targeting (planetary L4/L5, Earth-Sun L1/L2/L3)
- [x] Fleet intercept planning
- [x] Mid-transit course correction and abort burns
- [x] Refuelling from planetary stockpiles
- [x] Trajectory visualisation (arcs, orbit rings, selection reticules, starmap icons)

### 2.3 Mining Operations ✅
- [x] Mineral deposits on celestial bodies
- [x] Mining buildings (Mine, Deep Drill, Laser Drill, Strip Mine)
- [x] Mining efficiency mechanics (workforce, logistics)
- [x] Resource processing (Refinery, Atmospheric Processor)

## Phase 3: Planetary Systems (Partially Complete)

### 3.1 Colonies ✅
- [x] Colony establishment (starting colony on Earth)
- [x] Population simulation (growth, workforce allocation)
- [x] Colony infrastructure (29 building types)
- [x] Colony growth mechanics (housing, food, logistics)

### 3.2 Planetary Bodies ✅
- [x] Moon systems (148 moons across all planets)
- [x] Asteroid belts (145 asteroids catalogued)
- [x] Comets and other objects (20 comets)
- [x] Realistic orbital mechanics (Keplerian orbits with analytical propagation)

### 3.3 Terraforming (Planned)
- [ ] Terraforming mechanics
- [ ] Atmospheric manipulation
- [ ] Temperature control
- [ ] Biosphere development

## Phase 4: Expansion & Exploration (Partially Complete)

### 4.1 Star Systems ✅
- [x] Multiple star system generation (60 nearest star systems)
- [x] Procedural planet generation for nearby systems
- [x] System discovery mechanics (starmap view)
- [ ] Interstellar travel (planned)

### 4.2 Exploration (Planned)
- [x] Survey mechanics (mineral deposit discovery)
- [ ] Anomaly detection
- [ ] Resource prospecting
- [ ] System mapping

### 4.3 Kardashev Scale Progress (Planned)
- [ ] Type I: Planetary civilization milestones
- [ ] Type II: Dyson sphere/swarm mechanics
- [ ] Type III: Galactic-scale projects
- [ ] Victory conditions

## Phase 5: Strategy Layer

### 5.1 Factions
- [ ] AI factions
- [ ] Faction relations
- [ ] Diplomacy system
- [ ] Alliance mechanics

### 5.2 Economy
- [ ] Trade system
- [ ] Market mechanics
- [ ] Economic simulation
- [ ] Supply and demand

### 5.3 Conflicts
- [ ] Space combat system
- [ ] Defense mechanics
- [ ] Strategic objectives
- [ ] War and peace mechanics

## Phase 6: User Experience ✅ COMPLETE

### 6.1 UI/UX ✅
- [x] Main menu system
- [x] HUD design and implementation (comprehensive dashboard)
- [x] Information displays (tooltips, panels)
- [x] Context-sensitive UI (body selection, star system details)

### 6.2 Visualization ✅
- [x] Improved graphics (PBR materials, bloom effects)
- [x] Visual effects (comet trails, star glows)
- [x] Information overlays (orbit paths, labels)
- [x] Camera improvements (automatic view transitions)

### 6.3 Audio ✅
- [x] Music system (sequential playlist with CC-BY attribution overlay)
- [x] **3 ambient tracks** by Scott Buckley (CC-BY 4.0)
- [ ] Sound effects
- [ ] Ambient audio
- [ ] Audio settings


## Phase 7: Polish & Content

### 7.1 Game Balance
- [ ] Economic balance
- [ ] Technology pacing
- [ ] Resource availability
- [ ] Difficulty levels

### 7.2 Content
- [ ] Event system
- [ ] Mission system
- [ ] Storyline elements
- [ ] Random events

### 7.3 Quality of Life
- [ ] Save/load system
- [ ] Settings menu
- [ ] Tutorials
- [ ] Help system

## Technical Improvements

### Performance
- [ ] Multi-threading optimization
- [ ] LOD system for distant objects
- [ ] Culling optimization
- [ ] Memory usage optimization

### Architecture
- [ ] Data-driven design (JSON/RON configs)
- [ ] Mod support foundation
- [ ] Plugin hot-reloading
- [ ] Save game serialization

### Tools
- [ ] Level editor
- [ ] Debug console
- [ ] Performance profiling tools
- [ ] Content pipeline tools

## Community & Distribution

### Pre-Release
- [ ] Internal playtesting
- [ ] Bug fixing
- [ ] Performance optimization
- [ ] Documentation completion

### Alpha Release
- [ ] Public alpha testing
- [ ] Community feedback integration
- [ ] Bug tracking system
- [ ] Regular updates

### Beta Release
- [ ] Feature complete
- [ ] Balance refinement
- [ ] Polish and optimization
- [ ] Marketing materials

### Release
- [ ] Distribution platform setup
- [ ] Release version packaging
- [ ] Post-release support plan
- [ ] Update roadmap

## Long-term Vision

### Post-1.0
- [ ] Multiplayer support
- [ ] Procedural content generation
- [ ] Mod support and workshop
- [ ] Expansions and DLC
- [ ] Mobile/console ports

### Community Features
- [ ] Mod tools
- [ ] Scenario editor
- [ ] Custom campaigns
- [ ] Community content sharing

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

- **v0.3.0** - Fleet & Orbital Transfer ✅ COMPLETE (Current)
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

- **v0.4.0** - Expansion & Exploration (Next)
  - Interstellar travel with realistic transit times
  - Ship construction pipeline (Shipyard building integration)
  - Advanced exploration mechanics
  - Anomaly detection

- **v0.5.0** - Factions & Diplomacy
  - AI factions
  - Diplomacy system
  - Economy and trade
  - Conflicts

- **v0.6.0** - Polish
  - Audio system
  - Balance refinement
  - Quality of life improvements
  - Performance optimization

- **v1.0.0** - Release
  - Feature complete
  - Balanced gameplay
  - Full documentation
  - Save/load system

## Contributing

Want to help? See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on how to contribute to the project.

Priority areas for contribution:
1. UI/UX design and implementation
2. Game balance and mechanics design
3. Performance optimization
4. Documentation and tutorials
5. Testing and bug reports

## Stay Updated

- GitHub: Watch the repository for updates
- Issues: Track progress and discussions
- Pull Requests: See what's being worked on

---

*This roadmap is subject to change based on community feedback and development priorities.*

Last Updated: 2026-02-21
