# Usability Features Guide

This document describes the new usability features added to Helios Ascension.

## Hover Effects

When you move your mouse cursor over a celestial body, the following happens:

1. **Visual Ring**: A glowing cyan ring appears around the body with a subtle pulsing effect
   - Main ring with high opacity (0.8)
   - Outer glow ring with medium opacity (0.4)
   - Inner highlight ring with medium-high opacity (0.6)

2. **Hover Tooltip**: A tooltip appears in the top-left corner of the screen displaying:
   - Body name in large, bold text
   - Body type (Planet, Moon, Star, etc.)
   - Styled with a dark background and cyan border to match the ring

The hover system uses ray casting to detect which body is closest to the mouse cursor, ensuring accurate selection even when bodies overlap in the view.

## Selection Highlight Ring

When a body is selected in the ledger (left sidebar), a glowing cyan ring appears around it in the 3D view:

1. **Visual Ring**: The same glowing effect as the hover ring
   - Makes it easy to locate the selected body in the 3D solar system view
   - Remains visible as long as the body is selected
   - Uses the same three-layer effect (main ring, outer glow, inner highlight)

2. **Combined Effects**: When hovering over the selected body:
   - Only one ring is shown (the hover effect takes precedence)
   - This prevents visual clutter from overlapping rings

## Ledger Highlighting

The ledger (left sidebar) now provides enhanced visual feedback:

1. **Selection Highlighting**: Selected bodies are highlighted in the ledger with a distinct visual style
   - The label uses egui's `.highlight()` modifier for emphasis
   - Makes it easy to see which body is currently selected in the UI

2. **3D View Synchronization**: When selecting a body in the ledger:
   - A glowing ring appears around it in the 3D view
   - Helps you quickly locate the body in the solar system

3. **Anchor Integration**: Clicking the anchor button (⚓) now:
   - Automatically selects the body
   - Anchors the camera to follow it
   - Triggers the zoom-to-fit behavior

## Camera Lock and Follow

The camera system has been enhanced with automatic zoom and following:

1. **Camera Following**: When anchored to a body, the camera automatically follows it as it moves through its orbit
   - The camera maintains its relative position while the body moves
   - Smooth tracking ensures a stable view

2. **Smart Zoom-to-Fit**:
   - **Regular bodies**: Zooms to make the body fill approximately 10% of the screen
     - Calculates based on the body's visual radius
     - Clamps between 50 and 10,000 Bevy units for reasonable viewing
   
   - **The Sun**: Special handling to show the entire solar system
     - Zooms out to approximately 40 AU distance
     - Displays planets from Mercury to Neptune in view

3. **Automatic Triggering**: Zoom-to-fit triggers when:
   - You click the anchor button (⚓) in the ledger
   - A body is selected and anchored via the UI

## Technical Implementation

### Components
- `Hovered`: Marker component for bodies currently under the mouse cursor
- `Selected`: Marker component for the currently selected body (existing)
- `CameraAnchor`: Tracks which entity the camera is following (existing)

### Systems
- `handle_body_hover`: Detects hover via ray casting
- `draw_hover_effects`: Renders the glowing ring effect using Bevy's gizmos
- `ui_hover_tooltip`: Displays the egui tooltip for hovered bodies
- `zoom_camera_to_anchored_body`: Automatically adjusts camera zoom when anchoring

### Performance Considerations
- Hover detection runs every frame but uses efficient ray casting
- Only draws effects for the single hovered body
- Change detection ensures zoom only triggers when selection changes
- UI tooltip only renders when a body is actively hovered

## Usage Tips

1. **Exploring the Solar System**:
   - Hover over bodies to quickly identify them without selecting
   - Use the anchor button to lock onto and follow planets
   - The system automatically zooms to show the body at a comfortable size

2. **Navigating to the Sun**:
   - Click the anchor button next to "Sol" in the ledger
   - The camera zooms out to show the entire inner solar system
   - Perfect for getting an overview before focusing on specific bodies

3. **Tracking Moving Bodies**:
   - Anchor to a planet to watch it orbit the sun
   - Anchor to a moon to watch it orbit its planet
   - The camera follows smoothly as time progresses

## Future Enhancements

Potential improvements for future versions:
- 3D text labels in world space for hovered bodies
- Customizable zoom levels in the settings
- Smooth camera transitions when anchoring (interpolation)
- Multiple camera bookmarks for quick navigation
- Distance indicators in the hover tooltip
