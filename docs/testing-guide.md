# Testing Guide for Usability Features

This guide explains how to manually test the new usability features added to Helios Ascension.

## Prerequisites

1. Build the project:
   ```bash
   cargo build --release
   ```

2. Run the game:
   ```bash
   cargo run --release
   ```

## Test 1: Hover Effects

### Expected Behavior
When you move your mouse cursor over any celestial body:
1. A glowing cyan ring should appear around the body
2. A tooltip should appear in the top-left corner showing:
   - Body name (large, bold cyan text)
   - Body type (smaller gray text)

### How to Test
1. Launch the game
2. Move your mouse cursor over different celestial bodies (Sun, planets, moons)
3. Verify the ring and tooltip appear for each body
4. Move away - verify effects disappear

### Things to Check
- ✓ Ring is visible and properly sized around small and large bodies
- ✓ Tooltip displays correct name and type
- ✓ Effects smoothly appear/disappear when moving cursor
- ✓ Only one body is highlighted at a time

## Test 2: Ledger Highlighting

### Expected Behavior
Selected bodies should be visually highlighted in the ledger sidebar.

### How to Test
1. Look at the left sidebar (ledger)
2. Click on "Earth" in the ledger
3. Verify it becomes highlighted
4. Click on "Mars"
5. Verify Mars is now highlighted and Earth is not

### Things to Check
- ✓ Selected body has distinct visual highlighting
- ✓ Only one body is highlighted at a time
- ✓ Highlighting persists until another body is selected

## Test 3: Anchor Button Auto-Selection

### Expected Behavior
Clicking the anchor button (⚓) next to a body should:
1. Select that body
2. Anchor the camera to it
3. Trigger automatic zoom

### How to Test
1. Find the anchor button (⚓) next to "Earth" in the ledger
2. Click it
3. Verify:
   - Earth is now selected (highlighted)
   - Camera moves to focus on Earth
   - Camera automatically zooms to show Earth at appropriate size

### Things to Check
- ✓ Body is selected after clicking anchor
- ✓ Camera is anchored (follows the body)
- ✓ Zoom-to-fit activates automatically

## Test 4: Camera Zoom-to-Fit (Regular Bodies)

### Expected Behavior
When anchoring to planets or moons, the camera should zoom so the body fills ~10% of the screen.

### How to Test
1. Anchor to Earth (⚓)
2. Note the zoom level - Earth should be clearly visible but not too large
3. Anchor to Jupiter
4. Jupiter should be visible and larger than Earth was
5. Anchor to a small moon (e.g., Phobos)
6. The moon should still be visible despite being small

### Things to Check
- ✓ Large bodies (Jupiter, Saturn) are appropriately sized
- ✓ Small bodies (moons, asteroids) are visible and not too small
- ✓ Camera distance feels natural for each body
- ✓ Zoom level is clamped (doesn't get too close or too far)

## Test 5: Camera Zoom for Sun

### Expected Behavior
When anchoring to the Sun, the camera should zoom out to show the entire solar system (~40 AU).

### How to Test
1. Expand "Sol" in the ledger if it's collapsed
2. Click the anchor button (⚓) next to "Sol"
3. Observe the camera zoom out dramatically
4. Verify you can see multiple planets in view

### Things to Check
- ✓ Camera zooms way out when Sun is selected
- ✓ Inner planets (Mercury, Venus, Earth, Mars) are visible
- ✓ You get a "solar system overview" view
- ✓ Different behavior than regular bodies

## Test 6: Camera Following During Orbit

### Expected Behavior
When anchored to a body, the camera should follow it as it moves through its orbit over time.

### How to Test
1. Anchor to Earth (⚓)
2. Speed up time using the time controls at the bottom:
   - Click "100x" button
3. Watch Earth move around the Sun
4. Verify the camera follows Earth smoothly

### Things to Check
- ✓ Camera stays focused on the anchored body
- ✓ Body remains centered as it orbits
- ✓ Smooth motion with no jittering
- ✓ Works at different time speeds (1x, 10x, 100x, 1000x)

## Test 7: Combined Features

### Comprehensive Test Scenario
1. Hover over Mars - see ring and tooltip
2. Click anchor (⚓) next to Mars
3. Verify:
   - Mars is selected and highlighted in ledger
   - Camera zooms to appropriate distance
   - Camera follows Mars as time progresses
4. While still anchored, hover over Earth
5. Verify:
   - Earth shows hover ring and tooltip
   - Mars remains selected in ledger
   - Camera stays anchored to Mars

### Things to Check
- ✓ Hover and selection work independently
- ✓ Can hover over one body while anchored to another
- ✓ Multiple features work together without conflicts

## Performance Testing

### Expected Behavior
The game should run smoothly with the new features active.

### How to Test
1. Run the game with normal time speed
2. Move cursor around rapidly over many bodies
3. Speed up time to 1000x
4. Check frame rate and responsiveness

### Things to Check
- ✓ No noticeable lag when hovering over bodies
- ✓ Smooth rendering at all time speeds
- ✓ UI remains responsive
- ✓ No memory leaks after extended play

## Known Limitations

1. **Hover Label**: Currently displays a simple tooltip in the UI corner. A future enhancement could add 3D text labels in world space.

2. **Zoom Transitions**: Zoom changes are instant. A future enhancement could add smooth interpolation.

3. **Multiple Bodies**: Currently only one body can be hovered at a time (the closest to the cursor).

## Reporting Issues

If you encounter any issues during testing:

1. Note the exact steps to reproduce
2. Record the expected vs actual behavior
3. Include information about:
   - Which body you were interacting with
   - Current time speed
   - Any error messages in the console
4. Take screenshots if possible

## Success Criteria

All features are working correctly if:
- ✓ All tests pass
- ✓ No visual glitches or artifacts
- ✓ Smooth, responsive interaction
- ✓ Features work together without conflicts
- ✓ Performance remains good
