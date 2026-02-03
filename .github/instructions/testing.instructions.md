---
description: 'Testing standards and best practices for Helios Ascension'
applyTo: '**/*test*.rs,tests/**/*.rs'
---

# Testing Standards and Best Practices

Write comprehensive tests for Helios Ascension to ensure reliability and maintainability.

## Testing Philosophy

- Write tests that verify behavior, not implementation
- Test edge cases and error conditions
- Keep tests simple, readable, and maintainable
- Tests should be fast and run reliably
- Each test should test one thing and have a clear purpose

## Test Organization

### Unit Tests
Place unit tests in the same file as the code being tested:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_creation() {
        // Test code here
    }
}
```

### Integration Tests
Place integration tests in the `tests/` directory for testing multiple modules together.

## Bevy Testing Patterns

### Testing Systems
- Use Bevy's `App` for testing systems in isolation
- Set up minimal world state needed for the test
- Use `App::update()` to run systems
- Query for expected changes after system execution

### Testing Components
- Test component creation and initialization
- Verify component data integrity
- Test component interactions through systems

### Testing Resources
- Test resource initialization
- Verify resource state changes
- Test resource access patterns

## Test Naming

- Use descriptive names that explain what is being tested
- Follow the pattern: `test_<what>_<condition>_<expected_result>`
- Examples:
  - `test_camera_movement_with_valid_input_updates_position`
  - `test_celestial_body_spawn_with_valid_data_creates_entity`
  - `test_orbital_calculation_with_invalid_params_returns_error`

## Assertions

- Use `assert_eq!` and `assert_ne!` for clear failure messages
- Use `assert!` for boolean conditions
- Provide custom messages for complex assertions
- Use `#[should_panic]` for tests that expect panics

## Test Data

- Use realistic test data that matches production scenarios
- Create test fixtures for complex data structures
- Keep test data minimal but sufficient
- Use constants for magic numbers in tests

## Mocking and Test Doubles

- Keep dependencies testable through trait abstractions
- Use builder patterns for test setup
- Create test-specific implementations when needed
- Avoid over-mocking - prefer testing with real implementations when practical

## Performance Testing

- Use `#[ignore]` for slow tests
- Run performance tests separately from unit tests
- Profile performance-critical code paths
- Use benchmarks for micro-optimizations (consider `criterion` crate)

## Best Practices

- Each test should be independent and not rely on other tests
- Clean up resources after tests (Bevy handles this automatically)
- Use `cargo test` for standard testing
- Consider `cargo nextest` for faster parallel execution
- Run tests before committing code
- Keep test coverage high for critical systems
- Write tests before fixing bugs to prevent regression

## Common Patterns

### Testing Entity Spawning
Verify entities are created with correct components.

### Testing System Behavior
Set up world state, run system, verify expected changes.

### Testing Data Loading
Verify RON deserialization and data integrity.

### Testing Error Handling
Verify errors are returned for invalid inputs.

## Test Coverage Goals

- Aim for high coverage on core game systems
- All public APIs should have tests
- Error paths should be tested
- Edge cases should be covered
- Focus on critical game systems first
