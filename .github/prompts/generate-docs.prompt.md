---
agent: 'agent'
model: Claude Sonnet 4
tools: ['edit', 'codebase', 'search']
description: 'Generate comprehensive documentation for Helios Ascension code'
---

# Generate Documentation for Helios Ascension

You are helping to create documentation for Helios Ascension, a Bevy-based space strategy game.

## Information to Gather

Ask for the following if not provided:

1. **Target**: What needs documentation? (API, module, feature, architecture)
2. **Audience**: Who is the documentation for? (developers, users, contributors)
3. **Type**: What type of documentation? (API docs, README, guide, architecture)

## Documentation Types

### API Documentation
Document public APIs in code using `///` comments.

**Structure:**
```rust
/// Brief description of what this does.
///
/// More detailed explanation of behavior, if needed.
///
/// # Arguments
/// * `param1` - Description of parameter
/// * `param2` - Description of parameter
///
/// # Returns
/// Description of return value
///
/// # Examples
/// ```
/// let result = function_name(arg1, arg2);
/// assert_eq!(result, expected);
/// ```
///
/// # Panics
/// Describe when this panics, if applicable
///
/// # Errors
/// Describe error conditions, if applicable
pub fn function_name() { }
```

### Module Documentation
Document modules using `//!` at the top of the file.

**Structure:**
```rust
//! Brief description of the module.
//!
//! Detailed explanation of what this module provides,
//! its role in the architecture, and how to use it.
//!
//! # Examples
//! ```
//! use helios_ascension::module_name::*;
//! // Usage example
//! ```
```

### README Files
Update README.md with project information.

**Include:**
- Project overview and goals
- Features
- Installation instructions
- System requirements
- Usage examples
- Building and running
- Testing
- Contributing guidelines
- License

### Architecture Documentation
Document design decisions and system architecture.

**Include:**
- System overview
- Component interactions
- Data flow
- Plugin architecture
- Design decisions and rationale
- Diagrams (when helpful)

## Documentation Standards

Follow the [documentation standards](../.github/instructions/documentation.instructions.md).

### Writing Style
- Use clear, concise language
- Write in present tense
- Start with a verb (e.g., "Creates", "Returns")
- Be specific and accurate
- Avoid jargon when possible
- Include examples for complex APIs

### Code Examples
- Provide runnable examples
- Show common use cases
- Include error handling in examples
- Keep examples simple and focused
- Test examples to ensure they work

### Links and References
- Link to related documentation
- Reference relevant types and functions
- Link to external resources (Bevy docs, Rust book)
- Use relative paths for internal links

## Bevy-Specific Documentation

### Components
```rust
/// Represents the velocity of a celestial body in 3D space.
///
/// This component is used in conjunction with `Transform` to update
/// entity positions over time.
///
/// # Examples
/// ```
/// let velocity = Velocity::new(1.0, 0.0, 0.0);
/// commands.spawn((velocity, Transform::default()));
/// ```
#[derive(Component, Debug, Clone)]
pub struct Velocity {
    /// X-axis velocity in units per second
    pub x: f32,
    /// Y-axis velocity in units per second
    pub y: f32,
    /// Z-axis velocity in units per second
    pub z: f32,
}
```

### Systems
```rust
/// Updates entity positions based on their velocity.
///
/// This system runs every frame and moves entities according to
/// their `Velocity` component, taking into account delta time.
///
/// # Query Parameters
/// * `Transform` - The entity's position (mutable)
/// * `Velocity` - The entity's velocity (immutable)
pub fn update_positions(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &Velocity)>,
) {
    // Implementation
}
```

### Plugins
```rust
/// Physics simulation plugin for celestial bodies.
///
/// This plugin provides:
/// * Velocity-based movement
/// * Gravitational calculations
/// * Collision detection
///
/// # Systems
/// * `update_positions` - Updates entity positions
/// * `apply_gravity` - Applies gravitational forces
///
/// # Usage
/// ```
/// App::new()
///     .add_plugins(PhysicsPlugin)
///     .run();
/// ```
pub struct PhysicsPlugin;
```

## What to Document

### Must Document
- All public APIs
- Complex algorithms
- Non-obvious design decisions
- Plugin purposes and usage
- Public components, resources, and events
- Error conditions and panics

### Optional Documentation
- Private implementation details (if complex)
- Performance characteristics
- Alternative approaches considered
- Known limitations

### Don't Document
- Obvious functionality
- Self-explanatory code
- Temporary or experimental code
- Implementation that may change

## Process

1. **Understand the Code**
   - Read and understand implementation
   - Identify key concepts
   - Note important details

2. **Write Documentation**
   - Start with brief description
   - Add detailed explanation
   - Include examples
   - Document parameters and returns
   - Note panics and errors

3. **Review and Refine**
   - Check for clarity
   - Verify accuracy
   - Test examples
   - Fix typos and grammar

4. **Generate and Review**
   - Run `cargo doc --open` to view
   - Check formatting and links
   - Verify examples compile

## Tools

```bash
# Generate and view documentation
cargo doc --open

# Generate documentation for dependencies too
cargo doc --open --document-private-items

# Check documentation warnings
cargo doc --no-deps

# Test code examples in documentation
cargo test --doc
```

## After Documentation

1. Review generated documentation
2. Test any code examples
3. Check links work
4. Ensure formatting is correct
5. Update related documentation
6. Consider adding to examples/ directory
