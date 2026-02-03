---
agent: 'agent'
model: Claude Sonnet 4
tools: ['edit', 'codebase', 'search']
description: 'Create a new Bevy plugin or system for Helios Ascension'
---

# Create a New Bevy Component/Plugin/System

You are helping to create new components, plugins, or systems for Helios Ascension, a Bevy-based 4X space strategy game.

## Information to Gather

Ask for the following if not provided:

1. **Type**: What are you creating? (Component, System, Plugin, Resource, Event)
2. **Name**: What should it be called?
3. **Purpose**: What does it do?
4. **Dependencies**: What other components/systems does it interact with?
5. **Location**: Where should it go? (existing plugin or new plugin)

## Component Creation

For Components:
- Create a simple data structure
- Derive `Component`, `Debug`, and other appropriate traits
- Add documentation explaining the component's purpose
- Consider default values if applicable
- Place in appropriate plugin module

## System Creation

For Systems:
- Define as a function that takes Bevy parameters
- Use queries to access components
- Use resources for global state
- Document the system's behavior
- Add to appropriate plugin's system list
- Consider system ordering and run conditions

## Plugin Creation

For Plugins:
- Create in `src/plugins/` directory
- Implement the `Plugin` trait
- Register resources, components, and systems in `build()`
- Add module export in `src/plugins/mod.rs`
- Document plugin's purpose and what it provides
- Follow existing plugin patterns in the codebase

## Resource Creation

For Resources:
- Create a struct with needed state
- Derive `Resource` and appropriate traits
- Provide initialization logic
- Document the resource's purpose
- Register in appropriate plugin

## Event Creation

For Events:
- Define as a simple struct or enum
- Derive `Event` and appropriate traits
- Document when the event is fired
- Add event registration to plugin
- Create systems that write/read the event

## Best Practices

- Follow the [Rust coding standards](../.github/instructions/rust.instructions.md)
- Keep components as pure data
- Keep systems focused and single-purpose
- Use Bevy's parallel execution by default
- Document public APIs
- Add unit tests for logic
- Consider performance implications

## Example Structure

```rust
// Component example
#[derive(Component, Debug, Clone)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// System example
pub fn update_positions(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &Velocity)>,
) {
    for (mut transform, velocity) in &mut query {
        // Update logic here
    }
}

// Plugin example
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_positions);
    }
}
```

## Integration

After creating the new component/system/plugin:

1. Add to appropriate module in `src/plugins/`
2. Export in `mod.rs` if needed
3. Register plugin in `main.rs` if new plugin
4. Add tests in `tests/` directory
5. Update documentation if significant feature
6. Run `cargo fmt` and `cargo clippy`
7. Run tests to ensure nothing broke
