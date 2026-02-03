---
agent: 'agent'
model: Claude Sonnet 4
tools: ['codebase', 'search', 'problems', 'runCommands']
description: 'Debug issues and help resolve bugs in Helios Ascension'
---

# Debug Issues in Helios Ascension

You are helping to debug issues in Helios Ascension, a Bevy-based Rust game.

## Information to Gather

Ask for the following if not provided:

1. **Problem**: What is the issue or bug?
2. **Expected Behavior**: What should happen?
3. **Actual Behavior**: What actually happens?
4. **Reproduction Steps**: How to reproduce the issue?
5. **Error Messages**: Any error messages or stack traces?

## Debugging Process

### 1. Understand the Problem
- Read error messages carefully
- Identify affected components/systems
- Understand expected vs actual behavior
- Check if issue is reproducible

### 2. Locate the Issue
- Search for relevant code
- Trace execution path
- Check recent changes
- Review related code

### 3. Analyze the Code
- Look for logic errors
- Check for edge cases
- Verify data flow
- Check resource access patterns

### 4. Form Hypothesis
- Develop theories about the cause
- Consider multiple possibilities
- Prioritize most likely causes
- Plan debugging approach

### 5. Test Hypothesis
- Add logging or debug output
- Use debugger if needed
- Test with minimal reproduction
- Verify assumptions

### 6. Fix the Issue
- Implement fix
- Test the fix
- Verify no new issues
- Add test to prevent regression

## Common Issues

### Rust Compilation Errors
**Symptoms:** Code doesn't compile
**Check:**
- Error message and location
- Type mismatches
- Lifetime issues
- Missing trait implementations
- Unused variables or imports

### Runtime Panics
**Symptoms:** Program crashes with panic message
**Check:**
- Unwrap/expect calls
- Array bounds
- Option/Result handling
- Assertion failures
- Resource access

### Bevy System Errors
**Symptoms:** System doesn't run or behaves incorrectly
**Check:**
- System registration
- Query parameters
- Component existence
- System ordering
- Run conditions
- Resource conflicts

### Logic Errors
**Symptoms:** Wrong behavior, incorrect calculations
**Check:**
- Algorithm implementation
- Edge cases
- Boundary conditions
- Mathematical operations
- State management

### Performance Issues
**Symptoms:** Slow execution, low frame rate
**Check:**
- Hot loops
- Unnecessary allocations
- Inefficient queries
- Missing change detection
- System parallelism

### Data Issues
**Symptoms:** Wrong data, deserialization errors
**Check:**
- File formats
- Data validation
- Serialization/deserialization
- Default values
- Data transformations

## Debugging Tools

### Compiler Diagnostics
```bash
# Build with full error information
cargo build

# Check for additional issues
cargo clippy

# Check for undefined behavior
cargo miri test
```

### Logging
Add debug output to trace execution:
```rust
println!("Debug: value = {:?}", value);
dbg!(&variable);
eprintln!("Error: {}", error);
```

### Bevy Inspector
Use bevy-inspector-egui to inspect entity state at runtime.

### Rust Debugger
```bash
# Use rust-gdb or rust-lldb
rust-gdb target/debug/helios_ascension
```

### Testing
Write failing test that reproduces the issue:
```rust
#[test]
fn test_reproduces_bug() {
    // Minimal reproduction of the issue
    // This should fail before the fix
}
```

## Debugging Strategies

### Binary Search
- Comment out half the code
- Determine which half has the issue
- Repeat until issue is isolated

### Print Debugging
- Add print statements
- Trace execution flow
- Inspect variable values
- Verify assumptions

### Minimal Reproduction
- Create smallest code that shows the issue
- Remove unrelated code
- Simplify inputs
- Isolate the problem

### Compare Working Code
- Find similar working code
- Compare differences
- Identify what changed
- Learn from working examples

## Bevy-Specific Debugging

### System Execution Order
```rust
// Debug system ordering
println!("System A running");
```

### Query Results
```rust
// Debug query matches
for entity in query.iter() {
    println!("Entity: {:?}", entity);
}
```

### Component State
```rust
// Debug component values
if let Ok(component) = query.get(entity) {
    dbg!(component);
}
```

### Resource State
```rust
// Debug resource values
fn system(res: Res<MyResource>) {
    dbg!(&*res);
}
```

## Common Rust Pitfalls

### Borrowing Issues
- Multiple mutable borrows
- Borrow checker errors
- Lifetime problems

### Ownership Issues
- Use after move
- Trying to modify borrowed data
- Incorrect lifetime annotations

### Type Issues
- Type inference failures
- Trait bounds not satisfied
- Missing trait implementations

## After Debugging

1. **Fix the Issue**
   - Implement the fix
   - Keep fix minimal and focused
   - Don't change unrelated code

2. **Test the Fix**
   - Verify issue is resolved
   - Run related tests
   - Test edge cases
   - Run full test suite

3. **Add Regression Test**
   - Write test that would fail without fix
   - Test the specific bug scenario
   - Prevent future regressions

4. **Clean Up**
   - Remove debug code
   - Clean up test code
   - Update documentation if needed

5. **Code Quality**
   - Run `cargo fmt`
   - Run `cargo clippy`
   - Verify no warnings

## Prevention

- Write tests first (TDD)
- Use type system for safety
- Handle errors explicitly
- Validate inputs
- Add assertions for invariants
- Use Rust's safety features
- Follow best practices
- Review code carefully
