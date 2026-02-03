---
description: 'Documentation standards and guidelines for Helios Ascension'
applyTo: '**/*.rs,**/*.md'
---

# Documentation Standards

Write clear, comprehensive documentation to help developers understand and contribute to Helios Ascension.

## Code Documentation

### Public API Documentation
- Use `///` for public function, struct, enum, and trait documentation
- Include a brief description of what the item does
- Document parameters and return values
- Provide usage examples when helpful
- Document panics and safety requirements

### Module Documentation
- Use `//!` at the top of modules to describe their purpose
- Explain the module's role in the overall architecture
- List key types and functions
- Include examples of common usage patterns

### Examples in Documentation
Use code blocks in doc comments:
```rust
/// Spawns a celestial body entity.
///
/// # Examples
/// ```
/// let body = CelestialBody::new("Earth", 5.972e24);
/// ```
```

## Documentation Style

- Write in clear, concise English
- Use present tense ("Returns" not "Will return")
- Start with a verb ("Creates", "Updates", "Calculates")
- Keep paragraphs short and focused
- Use bullet points for lists
- Include links to related documentation

## README Files

- Keep README.md up to date with project changes
- Include setup instructions
- Document system requirements
- Provide usage examples
- List key features and architecture

## Architecture Documentation

- Update ARCHITECTURE.md for significant changes
- Document design decisions and rationale
- Include diagrams for complex systems
- Explain plugin interactions
- Document data flow and system ordering

## Markdown Documentation

- Use proper headings hierarchy (# ## ### ####)
- Use code blocks with language specification
- Include links to code and related docs
- Keep documentation close to the code it describes
- Use relative links for internal references

## What to Document

- Public APIs and their usage
- Complex algorithms and their approach
- Design decisions and trade-offs
- System interactions and dependencies
- Configuration options
- Build and deployment processes

## What Not to Document

- Obvious code behavior
- Implementation details that may change
- Code that explains itself through clear naming
- Temporary or experimental features
