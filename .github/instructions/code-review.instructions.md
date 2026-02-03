---
description: 'Code review standards for Helios Ascension'
applyTo: '**/*.rs'
---

# Code Review Standards

Code reviews ensure quality, maintainability, and knowledge sharing in Helios Ascension.

## Review Goals

- Catch bugs and logic errors
- Ensure code quality and consistency
- Verify adherence to project standards
- Share knowledge across the team
- Improve code clarity and documentation

## What to Review

### Correctness
- Does the code do what it's supposed to do?
- Are edge cases handled properly?
- Is error handling appropriate?
- Are there potential panics or unwraps?
- Does it follow Rust best practices?

### Code Quality
- Is the code clear and readable?
- Are functions appropriately sized?
- Are names descriptive and consistent?
- Is there unnecessary duplication?
- Is the code well-organized?

### Architecture
- Does it fit with existing architecture?
- Are abstractions appropriate?
- Is separation of concerns maintained?
- Are plugin boundaries respected?
- Does it follow ECS principles?

### Performance
- Are there obvious performance issues?
- Are collections sized appropriately?
- Are queries optimized?
- Is change detection used where appropriate?
- Are there unnecessary allocations?

### Testing
- Are there adequate tests?
- Do tests cover edge cases?
- Are tests clear and maintainable?
- Do all tests pass?
- Is test coverage sufficient?

### Documentation
- Are public APIs documented?
- Are complex algorithms explained?
- Is documentation accurate and up to date?
- Are examples provided where helpful?
- Are design decisions documented?

### Security
- Is input validated?
- Is unsafe code justified and documented?
- Are dependencies trustworthy?
- Are resources managed safely?
- Are error messages appropriate?

## Review Process

1. Review the code thoroughly before commenting
2. Be constructive and respectful in feedback
3. Ask questions to understand intent
4. Suggest improvements rather than just pointing out problems
5. Approve when standards are met
6. Follow up on requested changes

## Providing Feedback

- Be specific about issues and suggestions
- Explain why something is a problem
- Suggest concrete alternatives
- Distinguish between requirements and suggestions
- Praise good code and clever solutions
- Keep feedback professional and focused on the code

## Responding to Reviews

- Be open to feedback
- Ask for clarification when needed
- Explain your reasoning for design decisions
- Make requested changes or discuss alternatives
- Thank reviewers for their time and insights
- Update the PR based on feedback

## Common Review Items

- Check for compiler warnings
- Verify `cargo clippy` passes
- Ensure `cargo fmt` has been run
- Confirm tests pass
- Review commit messages
- Check for appropriate documentation
- Verify no temporary debug code remains
