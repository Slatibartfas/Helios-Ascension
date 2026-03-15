# Helios Ascension - User Interface Guide

## Overview

Helios Ascension features a comprehensive UI system built with egui, providing intuitive access to all game systems through a modern dashboard interface.

## Main Dashboard

The dashboard is visible at the top of the screen and provides access to all major game panels:

### Navigation Tabs
- **Survey** - Explore celestial bodies and star systems
- **Construction** - Build and manage colony infrastructure
- **Research** - Navigate the technology tree
- **Economy** - Track resources and budget
- **Fleets** - Manage spacecraft (coming soon)
- **Shipbuilding** - Design and construct vessels (coming soon)

### Time Controls

Located in the dashboard header:

- **Pause/Play Button**: Pause or resume simulation
- **Speed Selection**: Choose simulation speed
  - 1 hr/s (3,600× real-time)
  - 1 day/s (86,400× real-time)
  - 1 week/s (604,800× real-time)
  - 1 month/s (~2.6M× real-time)
  - 1 year/s (31.5M× real-time)
- **Date Display**: Current in-game date and time

## Survey Panel

View and select celestial bodies or star systems.

### System View (Zoomed In)
When viewing the solar system or a specific star system:

- **Body List**: Scrollable list of all bodies in current system
- **Body Selection**: Click a body to select it
- **Body Information** (right panel when selected):
  - Name and body type
  - Physical properties (mass, radius, gravity)
  - Orbital parameters
  - Surface conditions (temperature, atmosphere)
  - Mineral deposits (if surveyed)
  - Colony information (if colonized)
  - Population and buildings

### Starmap View (Zoomed Out)
When zoomed out beyond ~100 AU:

- **Star Icons**: Visual representation of nearby star systems
- **Hover Tooltips**: Hover over stars to see:
  - Star system name
  - Distance from Sol (light years)
  - Number of bodies in system
- **Star Selection**: Double-click a star to view detailed information
- **Star System Panel** (right panel when selected):
  - System information (distance, ID)
  - Star properties (spectral type, mass, luminosity, temperature, metallicity)
  - Body counts by type
  - Total surveyed resources across all bodies
  - Population statistics (coming soon)

## Construction Panel

Manage colony buildings and construction projects.

### Features
- **Colony Selection**: Dropdown to choose which colony to manage
- **Build Multiplier**: Queue ×1, ×5, or ×10 copies in one click
- **Building Categories** (47 total):
  - Infrastructure (Housing, Habitat Dome, Underground Habitat, Life Support, Water Treatment, Desalination, Recycling)
  - Industry (Mines, Refineries, Factories, Atmospheric Processors, Chemical Plants, Drills, Semiconductor Fabs, Pharma Plants)
  - Logistics (Mass Drivers, Orbital Lifts, Cargo Terminals, Warehouses)
  - Power (Solar, Wind, Hydro, Geothermal, Coal, Gas, Fission, Fusion)
  - Population (Agri Domes, Farms, Greenhouses, Aquaculture Facilities, Medical Centers)
  - Research (Labs, Engineering Bays, AI Clusters, Data Centers)
  - Financial (Commercial Hubs, Financial Centers, Trade Ports)
  - Military (Shipyards, Missile Silos, Launch Sites, Space Ports, Defense Batteries)

### Building Card Layout

Each building card shows (top to bottom):

1. **Icon + Name** — identifying header
2. **Description** — what it is (one line)
3. **Separator**
4. **Stats row** — `BP cost` | `👷 workforce` | `⚡ power demand`
5. **Build time** — estimated years or months based on current Factory BP output
6. **▸ Effect lines** (green) — the actual numeric impact per building, e.g.:
   - `+25M housing capacity`
   - `+1,000 Mt/yr food (feeds ~10M ppl)`
   - `+20 GW power output`
   - `+15% mining efficiency`
7. **Resource costs** — 2 per row, coloured green (affordable) or red (insufficient)
8. **Queue button** — disabled in red if resources are insufficient

### Construction Queue
- Appears at the bottom of the panel
- Shows active projects with a progress bar and estimated completion
- Cancel any project to refund queued resources

### Debug Controls (F12)
- **Free Construction**: Build without resource costs
- **Instant Build**: Complete construction immediately
- **Bypass Tech**: Show and queue all buildings regardless of tech prerequisites

> For a complete building reference with per-building outputs, capacities, and tech requirements see `docs/COLONIES.md`.

## Research Panel

Browse and select technologies to research.

### Technology Tree
- **15 Categories**: Electronics, Military, Space Technology, Biology, Physics, Energy, Sociology, Construction, Propulsion, Materials, Sensors, Weapons, Defensive Systems, Life Support, Industry
- **Tech Cards**: Show technology name, description, cost (RP), and prerequisites
- **Progress Tracking**: View research progress on active projects
- **Tech Status**: Visual indicators for:
  - Available (all prerequisites met)
  - Locked (missing prerequisites)
  - Researched (already completed)
  - Active (currently being researched)

### Technology Information
- **Research Cost**: Amount of Research Points (RP) required
- **Prerequisites**: Technologies that must be completed first
- **Unlocks**: Buildings, components, or capabilities unlocked
- **Modifiers**: Bonuses provided (cost reductions, productivity increases)

### Debug Controls (F12)
- **Instant Research**: Complete current research immediately
- **Free Research**: Unlock all technologies

## Economy Panel

Track resources, production, and budget.

### Resource Overview
- **Stockpiles**: Current amount of each resource
- **Production Rate**: Resources generated per year
- **Consumption Rate**: Resources used per year
- **Net Rate**: Net production/consumption (green/red)

### Resource Types (31 Total)
- **Volatiles**: Water, Hydrogen, Ammonia, Methane, Phosphorus
- **Atmospheric Gases**: Nitrogen, Oxygen, Carbon Dioxide, Argon
- **Construction Materials**: Iron, Aluminum, Titanium, Silicates, Nickel, Tungsten, Carbon
- **Fusion Fuel**: Helium-3, Deuterium
- **Fissiles**: Uranium, Thorium
- **Precious Metals**: Gold, Silver, Platinum
- **Strategic Materials**: Copper, Rare Earths, Lithium, Sulfur
- **Exotic Materials**: Antimatter, Exotic Matter, Metamaterials, Computronium

### Budget Information
- **Treasury**: Current monetary credits (MC)
- **Income**: Credits earned per year
- **Expenses**: Credits spent per year (building maintenance, operations)
- **Net Income**: Overall financial balance

### Energy Grid
- **Power Generation**: Total power produced by power plants
- **Power Consumption**: Total power used by buildings
- **Grid Status**: Surplus or deficit

## Tooltips & Interaction

### Body Tooltips (Blue Border)
Hover over celestial bodies in system view to see:
- Body name and type
- Distance from parent body
- Key properties

### Star Tooltips (Orange Border)
Hover over star icons in starmap view to see:
- Star system name
- Distance from Sol
- Body count

### Selection
- **Single Click**: Select bodies in system view
- **Double Click**: Select star systems in starmap view
- **Right Panel**: Detailed information appears for selected object

## Camera Controls

### Movement
- **W**: Forward
- **A**: Left
- **S**: Backward
- **D**: Right
- **Q**: Down
- **E**: Up

### View Control
- **Right-Click + Drag**: Rotate camera
- **Mouse Wheel**: Zoom in/out
- **Automatic View Switching**: System ↔ Starmap transition at ~100 AU

## Keyboard Shortcuts

- **F12**: Toggle debug settings (when in Construction or Research panels)
- **Space**: Pause/resume simulation
- **ESC**: Close current panel or deselect

## Tips & Tricks

### Efficient Navigation
1. Use starmap view to quickly locate distant star systems
2. Double-click stars to see resource totals without visiting
3. Use body list in Survey panel to quickly select specific bodies

### Resource Management
1. Check Economy panel regularly to monitor stockpiles
2. Watch for resource deficits (red production rates)
3. Build logistics buildings to improve efficiency

### Construction Planning
1. Check tech prerequisites before planning buildings
2. Ensure sufficient workforce before building industrial structures
3. Balance power generation and consumption
4. Build housing before running out of capacity

### Research Strategy
1. Prioritize technologies that unlock critical buildings
2. Research construction cost reduction techs early
3. Balance tech categories to unlock diverse capabilities

## Troubleshooting

### UI Not Responding
- Check if time is paused
- Ensure you've selected the correct colony/body
- Try clicking away and reselecting

### Missing Information
- Some data requires specific technologies to be researched
- Mineral deposits require survey operations
- Resource information needs time to update after changes

### Performance Issues
- Close unused panels
- Zoom closer when not using starmap
- Lower time acceleration if simulation is slow

## Future Enhancements

Planned UI improvements:
- Fleet management panel with ship controls
- Shipbuilding interface for vessel design
- Diplomacy panel for faction relations
- Advanced filters and sorting in all panels
- Customizable layouts and hotkeys
