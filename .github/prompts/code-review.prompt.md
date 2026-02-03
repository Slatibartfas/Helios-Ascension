---
agent: 'agent'
model: Claude Sonnet 4
tools: ['codebase', 'search', 'problems']
description: 'Perform thorough code review for Helios Ascension changes'
---

# Code Review for Helios Ascension

You are performing a code review for changes to Helios Ascension, a Bevy-based Rust game.

## Review Checklist

Follow the [code review standards](../.github/instructions/code-review.instructions.md).

### 1. Correctness
- [ ] Does the code achieve its intended purpose?
- [ ] Are edge cases handled?
- [ ] Is error handling appropriate?
- [ ] Are there potential panics or unwraps in library code?
- [ ] Does it follow Rust best practices?

### 2. Code Quality
- [ ] Is the code clear and readable?
- [ ] Are function sizes reasonable?
- [ ] Are names descriptive and follow conventions?
- [ ] Is there unnecessary code duplication?
- [ ] Is the code well-organized?
- [ ] Does it follow the [Rust standards](../.github/instructions/rust.instructions.md)?

### 3. Architecture
- [ ] Does it fit with the existing plugin architecture?
- [ ] Are Bevy ECS patterns followed correctly?
- [ ] Is separation of concerns maintained?
- [ ] Are components pure data structures?
- [ ] Are systems focused and single-purpose?

### 4. Performance
- [ ] Are there obvious performance issues?
- [ ] Are queries optimized with filters?
- [ ] Is change detection used appropriately?
- [ ] Are allocations minimized?
- [ ] Follow the [performance guidelines](../.github/instructions/performance.instructions.md)

### 5. Testing
- [ ] Are there adequate tests?
- [ ] Do tests cover edge cases?
- [ ] Are tests clear and maintainable?
- [ ] Do all tests pass?
- [ ] Follow the [testing standards](../.github/instructions/testing.instructions.md)

### 6. Documentation
- [ ] Are public APIs documented with `///`?
- [ ] Are complex algorithms explained?
- [ ] Is documentation accurate?
- [ ] Are examples provided where helpful?
- [ ] Follow the [documentation standards](../.github/instructions/documentation.instructions.md)

### 7. Security
- [ ] Is input validated?
- [ ] Is unsafe code justified and documented?
- [ ] Are errors handled without exposing sensitive info?
- [ ] Follow the [security standards](../.github/instructions/security.instructions.md)

### 8. Rust-Specific
- [ ] No compiler warnings?
- [ ] `cargo clippy` passes without warnings?
- [ ] `cargo fmt` has been run?
- [ ] Ownership and borrowing is correct?
- [ ] Error propagation uses `?` instead of unwrap?
- [ ] Appropriate traits are derived?

### 9. Bevy-Specific
- [ ] Components have `#[derive(Component)]`?
- [ ] Resources have `#[derive(Resource)]`?
- [ ] Events have `#[derive(Event)]`?
- [ ] Systems use proper query patterns?
- [ ] Plugin registration is complete?
- [ ] System ordering is considered?

## Review Process

1. **Understand the Change**
   - Read the PR description
   - Understand the problem being solved
   - Review related issues or discussions

2. **Review the Code**
   - Go through each file systematically
   - Check against the checklist above
   - Look for potential issues
   - Consider edge cases

3. **Check Testing**
   - Review test coverage
   - Verify tests are meaningful
   - Run tests locally if needed
   - Check for test quality

4. **Verify Build**
   - Ensure code compiles
   - Run `cargo clippy`
   - Run `cargo test`
   - Check for warnings

5. **Provide Feedback**
   - Be specific and constructive
   - Explain why something is an issue
   - Suggest concrete improvements
   - Praise good solutions
   - Ask questions for understanding

## Feedback Guidelines

### Providing Feedback
- Start with positive observations
- Be specific about issues
- Explain the "why" behind suggestions
- Distinguish requirements from suggestions
- Offer alternatives or examples
- Be respectful and professional

### Categorize Comments
- **Required**: Must be fixed before merge
- **Suggestion**: Nice to have but not blocking
- **Question**: Seeking clarification
- **Praise**: Acknowledging good work

## Common Issues to Watch For

### Rust Issues
- Unnecessary `clone()` calls
- Use of `unwrap()` or `expect()` without justification
- Missing error handling
- Unsafe code without documentation
- Ignoring compiler warnings
- Not using iterators effectively

### Bevy Issues
- Mutable resource access preventing parallelism
- Missing system ordering dependencies
- Not using change detection
- Inefficient queries
- Components with behavior
- Systems doing too much

### General Issues
- Magic numbers without explanation
- Deeply nested logic
- Functions that are too long
- Poor naming
- Missing documentation
- Incomplete testing

## After Review

- Wait for author response
- Review updated code
- Approve when satisfied
- Merge when ready
