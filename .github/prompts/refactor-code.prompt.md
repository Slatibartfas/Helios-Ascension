---
agent: 'agent'
model: Claude Sonnet 4
tools: ['edit', 'codebase', 'search']
description: 'Refactor code to improve quality, performance, or maintainability'
---

# Refactor Code in Helios Ascension

You are helping to refactor code in Helios Ascension to improve quality, performance, or maintainability.

## Refactoring Goals

Ask for the following if not provided:

1. **Target**: What code needs refactoring?
2. **Goal**: Why refactor? (readability, performance, maintainability, testing)
3. **Scope**: How extensive should the refactoring be?

## Common Refactoring Patterns

### Extract Function
Break down large functions into smaller, focused functions.

**When to use:**
- Function is too long (>50 lines)
- Logic can be grouped into meaningful units
- Code is duplicated
- Function does multiple things

### Extract Component
Move data into separate Bevy components for better ECS design.

**When to use:**
- Component has too many fields
- Data is logically separate
- Need better query performance
- Want to enable optional features

### Extract System
Split large systems into multiple focused systems.

**When to use:**
- System does multiple things
- Logic is independent
- Can improve parallelism
- Need different run conditions

### Simplify Conditionals
Reduce nested if statements and complex boolean logic.

**When to use:**
- Deep nesting (>3 levels)
- Complex boolean expressions
- Duplicated conditions
- Hard to understand logic

### Use Iterators
Replace index-based loops with iterator chains.

**When to use:**
- Index loops are used
- Multiple passes over data
- Transforming collections
- Filtering data

### Remove Duplication
Extract common code into reusable functions or traits.

**When to use:**
- Same code in multiple places
- Similar but not identical logic
- Copy-paste code
- Can generalize behavior

## Bevy-Specific Refactoring

### Split Large Plugins
Break plugins into smaller, focused plugins.

**Example:**
```rust
// Before: One large plugin
pub struct GamePlugin;

// After: Multiple focused plugins
pub struct PhysicsPlugin;
pub struct RenderPlugin;
pub struct AIPlugin;
```

### Improve Query Efficiency
Use query filters to reduce iteration overhead.

**Example:**
```rust
// Before: Check condition in loop
for entity in query.iter() {
    if has_some_property { ... }
}

// After: Use query filter
for entity in query.iter().filter(|e| e.has_property()) { ... }
```

### Use Change Detection
Only process changed entities.

**Example:**
```rust
// Before: Process all entities every frame
fn system(query: Query<&Transform>) { ... }

// After: Only process changed transforms
fn system(query: Query<&Transform, Changed<Transform>>) { ... }
```

### Extract Bundles
Group commonly-used components into bundles.

**Example:**
```rust
#[derive(Bundle)]
pub struct CelestialBodyBundle {
    pub body: CelestialBody,
    pub transform: Transform,
    pub velocity: Velocity,
    pub name: Name,
}
```

## Refactoring Process

1. **Understand Current Code**
   - Read and understand existing implementation
   - Identify pain points
   - Consider implications of changes

2. **Plan Refactoring**
   - Define clear goals
   - Identify tests to verify behavior
   - Plan incremental steps
   - Consider breaking changes

3. **Ensure Tests Exist**
   - Write tests if missing
   - Verify tests pass before refactoring
   - Tests protect against regressions

4. **Refactor Incrementally**
   - Make small, focused changes
   - Test after each change
   - Commit working states
   - Don't change behavior and refactor simultaneously

5. **Verify Results**
   - Run all tests
   - Run `cargo clippy`
   - Run `cargo fmt`
   - Check performance if relevant
   - Review the changes

## Best Practices

- Refactor for readability first
- Keep changes small and focused
- Maintain or improve test coverage
- Don't change behavior unless intended
- Update documentation
- Run tests frequently
- Use version control effectively
- Consider performance implications
- Follow [Rust standards](../.github/instructions/rust.instructions.md)

## Warning Signs

Stop or reconsider if:
- Tests start failing
- Changes become too large
- You're changing behavior unintentionally
- Complexity is increasing
- You're unsure of the impact

## After Refactoring

1. Run full test suite
2. Run `cargo clippy` and fix warnings
3. Run `cargo fmt`
4. Update documentation if needed
5. Review the diff
6. Consider performance testing
7. Get code review feedback
