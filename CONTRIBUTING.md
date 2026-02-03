# Contributing to Helios Ascension

Thank you for your interest in contributing to Helios Ascension! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites
- Rust 1.70+ (latest stable recommended)
- System dependencies (see README.md)
- Basic understanding of Bevy ECS architecture

### Setting Up Development Environment

1. Clone the repository
```bash
git clone https://github.com/Slatibartfas/Helios-Ascension.git
cd Helios-Ascension
```

2. Install dependencies
```bash
# Linux
sudo apt-get install libwayland-dev libxkbcommon-dev libvulkan-dev libasound2-dev libudev-dev
```

3. Build the project
```bash
cargo build
```

4. Run tests
```bash
cargo test
```

## Development Workflow

### Code Style
- Follow Rust standard style guidelines (use `rustfmt`)
- Run `cargo fmt` before committing
- Run `cargo clippy` to catch common issues
- Keep line length under 100 characters where possible

### Testing
- Write tests for new functionality
- Ensure all tests pass before submitting PR
- Add integration tests for new plugins
- Test both debug and release builds

### Commit Messages
- Use clear, descriptive commit messages
- Start with a verb (Add, Fix, Update, Remove, etc.)
- Reference issue numbers when applicable
- Example: "Add resource management plugin (#123)"

## Architecture Guidelines

### Adding New Plugins

When adding a new plugin:

1. Create a new file in `src/plugins/`
2. Implement the `Plugin` trait
3. Add appropriate components and systems
4. Export from `src/plugins/mod.rs`
5. Register in `main.rs`
6. Add documentation and tests

Example structure:
```rust
use bevy::prelude::*;

pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
           .add_systems(Update, update_system);
    }
}

#[derive(Component)]
pub struct MyComponent {
    // fields
}

fn setup(/* parameters */) {
    // initialization logic
}

fn update_system(/* parameters */) {
    // update logic
}
```

### Components
- Keep components focused and single-purpose
- Use `#[derive(Component)]` for component types
- Document component fields and their purpose
- Mark intentionally unused fields with `#[allow(dead_code)]`

### Systems
- Keep systems small and focused
- Use queries efficiently
- Minimize resource access
- Consider parallelization opportunities
- Add appropriate system ordering when needed

### Resources
- Use resources for global state only
- Prefer components over resources when possible
- Document resource structure and usage

## Performance Guidelines

- Profile before optimizing
- Use Bevy's built-in optimization features
- Avoid unnecessary allocations
- Use `&` queries when read-only access is sufficient
- Batch operations when possible
- Consider using events for one-time communications

## Documentation

### Code Documentation
- Document all public APIs
- Use Rust doc comments (`///` and `//!`)
- Include examples in documentation
- Document complex algorithms and logic

### User Documentation
- Update README.md for user-facing changes
- Update ARCHITECTURE.md for structural changes
- Keep documentation in sync with code

## Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Format code (`cargo fmt`)
6. Check with clippy (`cargo clippy`)
7. Commit changes
8. Push to your fork
9. Open a Pull Request

### PR Guidelines
- Provide a clear description of changes
- Reference related issues
- Include screenshots for visual changes
- Ensure CI passes
- Be responsive to review feedback

## Code Review

All submissions require review. We look for:
- Code quality and style
- Test coverage
- Documentation
- Performance considerations
- Architecture alignment

## Questions?

Feel free to open an issue for:
- Questions about architecture
- Feature proposals
- Bug reports
- Documentation improvements

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
