# Helios Ascension - Development Roadmap

## Current Status: v0.4.0 - Building System Overhaul & Logistics

Major refactor in progress to transform the game from simple point-to-point resource management into a deep logistics simulation with AI competition and meaningful progression.

## v0.3.0: Fleet & Orbital Transfer System Complete ✅

The game has a fully implemented fleet management system with realistic orbital mechanics, transfer planning, gravity-assist routing, and Lagrange-point targeting. Core gameplay loop is functional and ships fly!

---

## v0.4.0: Building System Overhaul & Localized Logistics (IN PROGRESS)

Transform the building system to offer much more variety and player choices, with resources now stored per-body rather than globally.

### 4.1 Building System Redesign
- [ ] Expand building types for greater variety and specialization
- [ ] Have buildings consume all resources by adding more and update the existing ones
- [ ] Building tiers with upgrade paths
- [ ] Unique building effects and strategic choices
- [ ] Building synergies
- [ ] Remove global resources — each body/ship/station has its own storage

### 4.2 Localized Resource Storage
- [ ] Per-body resource stockpiles (planets, moons, asteroids)
- [ ] Per-ship resource storage (cargo capacity)
- [ ] Per-station resource storage
- [ ] Resource transfer mechanics between locations
- [ ] Storage capacity limits per location

### 4.3 Logistics Network
- [ ] AI-controlled logistics ships (private company freighters)
- [ ] Automated cargo routes between colonies
- [ ] Logistics priority system (essential vs luxury goods)
- [ ] Shipping costs and transit times
- [ ] Supply and demand per location

### 4.4 Ship & Station Designer
- [ ] Modular ship design interface
- [ ] Component selection (hulls, engines, cargo bays, weapons)
- [ ] Station module builder
- [ ] Design cost calculation
- [ ] Ship/station naming

---

## v0.5.0: Exploration & Progression System

Sequential exploration where you send probes first, then rovers, establish stations, then bases.

### 5.1 Survey System Rework
- [ ] Remove three-tier survey system
- [ ] Progressive discovery with probes
- [ ] Survey teams with scientist personnel
- [ ] Gradually reveal resources, anomalies, landing sites
- [ ] Survey data collection and analysis

### 5.2 Personnel System
- [ ] Scientists for survey missions and research
- [ ] Generals for fleet operations
- [ ] Governors for colony management
- [ ] Personnel training and advancement
- [ ] Mission assignment interface

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

- **v0.4.0** - Building & Logistics Overhaul (IN PROGRESS)
  - Building system redesign with more variety
  - Per-body/ship/station resource storage (no more global resources)
  - AI logistics ships from private companies
  - Ship and station designer

- **v0.5.0** - Exploration & Progression ✅ NEXT
  - Survey system rework (progressive discovery)
  - Personnel system (scientists, generals, governors)
  - Sequential expansion (probes → rovers → stations → bases)
  - Notification and event system

- **v0.6.0** - AI Competition ✅
  - AI factions with distinct behaviors
  - Competition for resources and territory
  - Diplomatic system

- **v0.7.0** - Financial System ✅
  - Corporate budget management
  - Trade economy with markets
  - Taxation and revenue

- **v0.8.0** - Technology Rework ✅
  - Progression-locked tech tree
  - Exploration milestones unlock tech
  - New personnel roles

- **v0.9.0** - Balance & Polish ✅
  - Game balance (mining, research, economy)
  - UI/UX improvements
  - Audio completion
  - Performance optimization

- **v1.0.0** - Release ✅
  - Feature complete
  - Balanced gameplay
  - Full documentation
  - Save/load system

---

## Contributing

Want to help? See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on how to contribute to the project.

Priority areas for contribution:
1. Building system redesign
2. Logistics and AI freighters
3. Personnel system
4. UI/UX design
5. Game balance

---

*This roadmap is subject to change based on development priorities.*

Last Updated: 2026-03-06
