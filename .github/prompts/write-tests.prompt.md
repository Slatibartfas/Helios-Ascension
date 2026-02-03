---
agent: 'agent'
model: Claude Sonnet 4
tools: ['edit', 'codebase', 'search']
description: 'Generate comprehensive tests for Rust code in Helios Ascension'
---

# Write Tests for Helios Ascension

You are helping to write tests for Helios Ascension, a Bevy-based game written in Rust.

## Information to Gather

Ask for the following if not provided:

1. **Target**: What code needs testing? (file, function, system, plugin)
2. **Test Type**: Unit tests or integration tests?
3. **Coverage**: What scenarios should be tested?

## Test Organization

### Unit Tests
- Place in the same file as the code being tested
- Use `#[cfg(test)]` module
- Import with `use super::*;`
- Test individual functions and components

### Integration Tests
- Place in `tests/` directory
- Test plugin interactions
- Test complete workflows
- Test serialization/deserialization

## Test Structure

Follow the [testing standards](../.github/instructions/testing.instructions.md).

### Basic Test Pattern
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_describes_what_is_tested() {
        // Arrange: Set up test data
        let input = create_test_data();
        
        // Act: Execute the code being tested
        let result = function_under_test(input);
        
        // Assert: Verify expected behavior
        assert_eq!(result, expected);
    }
}
```

### Bevy System Testing
```rust
#[test]
fn test_system_behavior() {
    // Create test app
    let mut app = App::new();
    
    // Add minimal plugins needed
    app.add_plugins(MinimalPlugins);
    
    // Set up test state
    app.world_mut().spawn(TestComponent::default());
    
    // Add system under test
    app.add_systems(Update, system_under_test);
    
    // Run one update
    app.update();
    
    // Query and verify results
    let query = app.world().query::<&TestComponent>();
    // Assertions here
}
```

## Test Categories

### Component Tests
- Test component creation and initialization
- Test component data validity
- Test component derive traits

### System Tests
- Test system logic with mock data
- Test entity queries and filters
- Test resource access and mutation
- Test event handling

### Plugin Tests
- Test plugin registration
- Test system ordering
- Test resource initialization
- Test plugin interactions

### Data Tests
- Test RON deserialization
- Test data validation
- Test data transformations
- Test file loading

### Error Tests
- Test error handling paths
- Test invalid input handling
- Use `#[should_panic]` for expected panics
- Test edge cases

## Test Best Practices

- One test per behavior
- Use descriptive test names
- Keep tests simple and readable
- Test edge cases and error conditions
- Make tests independent of each other
- Use test fixtures for complex setup
- Add comments for complex test logic
- Run tests frequently during development

## Assertions

- Use `assert_eq!` for equality checks
- Use `assert_ne!` for inequality checks
- Use `assert!` for boolean conditions
- Provide custom messages for clarity
- Test both success and failure cases

## Test Data

- Use realistic test data
- Create helper functions for common test data
- Keep test data minimal but sufficient
- Use constants for magic numbers

## Common Patterns

### Testing Entity Spawning
```rust
#[test]
fn test_entity_spawn() {
    let mut app = App::new();
    app.add_systems(Startup, spawn_test_entity);
    app.update();
    
    let count = app.world().query::<&TestComponent>().iter(app.world()).count();
    assert_eq!(count, 1);
}
```

### Testing Celestial Body Data
```rust
#[test]
fn test_celestial_body_valid_data() {
    let body = CelestialBody::new("Earth", 5.972e24, 6.371e6);
    assert_eq!(body.name, "Earth");
    assert!(body.mass > 0.0);
    assert!(body.radius > 0.0);
}
```

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests in a file
cargo test --test file_name

# Run with output
cargo test -- --nocapture

# Run parallel tests
cargo nextest run
```

## After Writing Tests

1. Run `cargo test` to verify all pass
2. Run `cargo fmt` to format test code
3. Run `cargo clippy` to check for issues
4. Ensure tests are well-documented
5. Consider test coverage for critical paths
