<!-- Based on: https://github.com/github/awesome-copilot/blob/main/instructions/rust.instructions.md -->
---
description: 'Rust programming language coding conventions and best practices for Helios Ascension'
applyTo: '**/*.rs'
---

# Rust Coding Conventions and Best Practices

Follow idiomatic Rust practices and community standards when writing Rust code for Helios Ascension.

These instructions are based on [The Rust Book](https://doc.rust-lang.org/book/), [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/), [RFC 430 naming conventions](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md), and the broader Rust community at [users.rust-lang.org](https://users.rust-lang.org).

## General Instructions

- Always prioritize readability, safety, and maintainability.
- Use strong typing and leverage Rust's ownership system for memory safety.
- Break down complex functions into smaller, more manageable functions.
- Write code with good maintainability practices, including comments on why certain design decisions were made.
- Handle errors gracefully using `Result<T, E>` and provide meaningful error messages.
- Use consistent naming conventions following [RFC 430](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md).
- Write idiomatic, safe, and efficient Rust code that follows the borrow checker's rules.
- Ensure code compiles without warnings.

## Bevy-Specific Rust Patterns

- Use Bevy's derive macros: `#[derive(Component)]`, `#[derive(Resource)]`, `#[derive(Event)]`
- System functions should use Bevy's query system for accessing components
- Use `Query<&Component>` for read-only access, `Query<&mut Component>` for mutable
- Use `Res<T>` for immutable resource access, `ResMut<T>` for mutable
- Prefer query filters like `With<T>`, `Without<T>` for entity filtering
- Use change detection queries: `Changed<T>`, `Added<T>`, `Removed<T>`

## Patterns to Follow

- Use modules (`mod`) and public interfaces (`pub`) to encapsulate logic.
- Handle errors properly using `?`, `match`, or `if let`.
- Use `serde` for serialization with `#[derive(Serialize, Deserialize)]`.
- Implement traits to abstract behavior and enable polymorphism.
- Structure async code using `async/await` and `tokio` or `async-std` when needed.
- Prefer enums over flags and states for type safety.
- Use builders for complex object creation.
- Split binary and library code (`main.rs` vs `lib.rs`) for testability and reuse.
- Use iterators instead of index-based loops as they're often faster and safer.
- Use `&str` instead of `String` for function parameters when you don't need ownership.
- Prefer borrowing and zero-copy operations to avoid unnecessary allocations.

### Ownership, Borrowing, and Lifetimes

- Prefer borrowing (`&T`) over cloning unless ownership transfer is necessary.
- Use `&mut T` when you need to modify borrowed data.
- Explicitly annotate lifetimes when the compiler cannot infer them.
- Use `Arc<T>` for thread-safe reference counting (Bevy systems often run in parallel).
- Use `Mutex<T>` or `RwLock<T>` for multi-threaded mutable state.

## Patterns to Avoid

- Don't use `unwrap()` or `expect()` in library code—prefer proper error handling.
- Avoid panics in library code—return `Result` instead.
- Don't rely on global mutable state—use Bevy resources or dependency injection.
- Avoid deeply nested logic—refactor with functions or combinators.
- Don't ignore warnings—treat them as errors during CI.
- Avoid `unsafe` unless required and fully documented.
- Don't overuse `clone()`, use borrowing instead of cloning unless ownership transfer is needed.
- Avoid premature `collect()`, keep iterators lazy until you actually need the collection.
- Avoid unnecessary allocations—prefer borrowing and zero-copy operations.

## Code Style and Formatting

- Follow the Rust Style Guide and use `rustfmt` for automatic formatting.
- Keep lines under 100 characters when possible.
- Place function and struct documentation immediately before the item using `///`.
- Use `cargo clippy` to catch common mistakes and enforce best practices.

## Error Handling

- Use `Result<T, E>` for recoverable errors and `panic!` only for unrecoverable errors.
- Prefer `?` operator over `unwrap()` or `expect()` for error propagation.
- Use `Option<T>` for values that may or may not exist.
- Provide meaningful error messages and context.
- Error types should implement standard traits like `Debug`, `Display`, and `std::error::Error`.
- Validate function arguments and return appropriate errors for invalid input.

## API Design Guidelines

### Common Traits Implementation
Eagerly implement common traits where appropriate:
- `Component`, `Resource`, `Event` for Bevy types
- `Copy`, `Clone`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, `Hash`, `Debug`, `Default`
- Use standard conversion traits: `From`, `Into`, `AsRef`, `AsMut`
- Note: `Send` and `Sync` are auto-implemented by the compiler when safe

### Type Safety and Predictability
- Use newtypes to provide static distinctions (e.g., `struct EntityId(u64)`)
- Arguments should convey meaning through types
- Use `Option<T>` appropriately instead of special sentinel values
- Prefer generic parameters over trait objects when performance matters

### Documentation
- Use `///` for public API documentation
- Include examples in doc comments using `/// # Examples`
- Document panics using `/// # Panics`
- Document safety requirements using `/// # Safety`
- Keep documentation up to date with code changes

## Testing

- Write unit tests alongside implementation code using `#[cfg(test)]`
- Use integration tests in the `tests/` directory for multi-module testing
- Test edge cases and error conditions
- Use descriptive test names that explain what is being tested
- Use `assert_eq!`, `assert_ne!`, and `assert!` for clear failure messages

## Performance Considerations

- Profile before optimizing with tools like `cargo flamegraph`
- Use Bevy's parallel system execution by default (systems run in parallel when possible)
- Avoid unnecessary allocations and copies
- Use iterators and combinators for efficient data processing
- Consider using query filters to reduce system overhead
- Use change detection to avoid processing unchanged entities
- Be mindful of entity spawning/despawning in hot loops

## Bevy ECS Best Practices

- **Components**: Pure data structures with no behavior
- **Systems**: Functions that operate on components via queries
- **Resources**: Global state, use sparingly
- **Events**: For communication between systems
- Use system ordering and run conditions to manage dependencies
- Bundle components together for common entity patterns
- Use marker components for entity categorization
