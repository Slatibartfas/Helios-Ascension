# Atmosphere UI Feature Documentation

## Overview
This document describes the new atmosphere display feature added to the Selection Panel on the right side of the game window.

## Location
The atmosphere information is displayed in the **right side panel** when a celestial body with an atmosphere is selected.

## UI Layout

```
┌─────────────────────────────────────────────────┐
│ Selected Body                                   │
├─────────────────────────────────────────────────┤
│ Earth                                           │
│                                                 │
│ ┌─────────────────────────────────────────┐   │
│ │ Position                                │   │
│ │ Distance from Sun: 1.000 AU             │   │
│ │ Radius: 6371.0 km                       │   │
│ │ Mass: 5.97e24 kg                        │   │
│ └─────────────────────────────────────────┘   │
│                                                 │
│ ┌─────────────────────────────────────────┐   │
│ │ Orbital Elements                        │   │
│ │ Semi-major axis: 1.000 AU               │   │
│ │ Eccentricity: 0.0167                    │   │
│ │ Inclination: 0.00°                      │   │
│ │ Period: 1.00 years                      │   │
│ └─────────────────────────────────────────┘   │
│                                                 │
│ ┌─────────────────────────────────────────┐   │
│ │ ▼ 🌍 Atmosphere                         │   │  <- COLLAPSIBLE HEADER (NEW!)
│ │                                         │   │
│ │   Pressure: 1.01 bar                   │   │  <- Displays in bar or mbar
│ │   Temperature: 15.0°C                  │   │
│ │   Breathable: ✓ Yes                    │   │  <- Green if breathable
│ │   Colony Cost: 0/8                     │   │  <- Color coded: Green/Yellow/Orange/Red
│ │                                         │   │
│ │   ▶ Gas Composition                    │   │  <- SUB-COLLAPSIBLE (NEW!)
│ │                                         │   │
│ └─────────────────────────────────────────┘   │
│                                                 │
│ ┌─────────────────────────────────────────┐   │
│ │ Resources                               │   │
│ │ ...                                     │   │
│ └─────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

## Gas Composition (When Expanded)

```
│ ▼ 🌍 Atmosphere                             │
│                                             │
│   Pressure: 1.01 bar                       │
│   Temperature: 15.0°C                      │
│   Breathable: ✓ Yes                        │
│   Colony Cost: 0/8                         │
│                                             │
│   ▼ Gas Composition                        │  <- EXPANDED
│     N2: 78.00%                             │
│     O2: 21.00%                             │
│     Ar: 0.93%                              │
│     CO2: 0.04%                             │
│                                             │
```

## Features

### 1. **Collapsible Atmosphere Section**
- Main atmosphere section can be collapsed/expanded
- Opens by default when a body with atmosphere is selected
- Uses egui's `CollapsingState` for persistent state

### 2. **Basic Atmosphere Information**
- **Pressure**: Displays in appropriate units
  - Bar format for pressure >= 1.0 bar (e.g., "1.01 bar", "92.00 bar")
  - Millibar format for pressure < 1.0 bar (e.g., "6 mbar", "500 mbar")
- **Temperature**: Always in Celsius (e.g., "15.0°C", "-63.0°C", "465.0°C")
- **Breathability**: Visual indicator
  - ✓ Yes (Green) - Atmosphere is breathable (100-300 mbar O₂)
  - ✗ No (Red) - Not breathable
- **Colony Cost**: Color-coded rating (0-8 scale)
  - 0: Green (Earth-like, perfect for colonization)
  - 1-3: Yellow (Challenging but manageable)
  - 4-6: Orange (Difficult, requires significant infrastructure)
  - 7-8: Red (Extreme, very hostile environment)

### 3. **Gas Composition Sub-Section**
- Nested collapsible section under atmosphere
- Closed by default to save space
- Lists all atmospheric gases with percentages
- Format: "GAS_NAME: XX.XX%"
- Examples:
  - N₂: 78.00%
  - O₂: 21.00%
  - H₂: 90.00%
  - CO₂: 96.50%

## Example Displays

### Earth
```
🌍 Atmosphere
  Pressure: 1.01 bar
  Temperature: 15.0°C
  Breathable: ✓ Yes (Green)
  Colony Cost: 0/8 (Green)
  
  Gas Composition:
    N2: 78.00%
    O2: 21.00%
    Ar: 0.93%
    CO2: 0.04%
```

### Mars
```
🌍 Atmosphere
  Pressure: 6 mbar
  Temperature: -63.0°C
  Breathable: ✗ No (Red)
  Colony Cost: 7/8 (Red)
  
  Gas Composition:
    CO2: 95.00%
    N2: 2.70%
    Ar: 1.60%
    O2: 0.13%
```

### Venus
```
🌍 Atmosphere
  Pressure: 92.00 bar
  Temperature: 465.0°C
  Breathable: ✗ No (Red)
  Colony Cost: 8/8 (Red)
  
  Gas Composition:
    CO2: 96.50%
    N2: 3.50%
```

### Jupiter
```
🌍 Atmosphere
  Pressure: 1.00 bar (at reference level)
  Temperature: -145.0°C
  Breathable: ✗ No (Red)
  Colony Cost: 8/8 (Red)
  
  Gas Composition:
    H2: 90.00%
    He: 10.00%
```

## Implementation Details

### Code Changes
1. **Import Addition**: Added `AtmosphereComposition` to UI module imports
2. **Query Update**: Added `Option<&AtmosphereComposition>` to body query
3. **UI Rendering**: Added atmosphere display section with:
   - Main collapsible header with 🌍 icon
   - Pressure display with smart unit selection
   - Temperature display
   - Breathability indicator with color
   - Colony cost with color coding
   - Nested gas composition collapsible section

### Persistent State
- Both the main atmosphere section and gas composition sub-section use persistent IDs
- State is saved in egui context between frames
- User preferences for expanded/collapsed state are preserved

### Future Extensions
The UI is designed to accommodate future features:
- **Terraforming Progress**: Can add progress bars for ongoing atmosphere changes
- **Recent Changes**: Can add a "Recent Changes" section to show atmosphere modifications
- **Composition Changes**: Can add delta indicators (↑↓) next to gas percentages
- **Target Values**: Can add target composition settings for terraforming

## Testing
- All atmosphere UI logic is tested in `tests/atmosphere_ui_tests.rs`
- Tests verify:
  - Data accessibility for UI display
  - Formatting logic (pressure units, colony cost ranges)
  - Color category mapping
  - Gas composition display
