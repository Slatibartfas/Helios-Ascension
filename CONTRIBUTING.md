# Contributing to Helios Ascension

Thank you for your interest in contributing to Helios Ascension! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites
- Rust 1.94.0 (pinned in `rust-toolchain.toml`; rustup auto-fetches)
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

### Coding rules

The full rule set lives in `AGENTS.md` and `.github/copilot-instructions.md`; the
critical Bevy-specific rules are:

- **B0001 dual-Query rule**: a system function MUST NOT declare two `Query<…>`
  parameters that both access the same component (Bevy 0.18 runtime panic that
  `cargo build` / `cargo test` don't catch). Fold queries into one, use a
  `ParamSet`, or apply disjoint `With`/`Without` filters. Audit:
  `python3 scripts/audit_b0001.py src`.
- **SimulationTime**: never use `Time<Virtual>` for game-world calculations
  (capped at ~15×); use `SimulationTime` from `src/ui/time.rs` instead.
- **Egui scheduling**: egui-using systems must run in `EguiPrimaryContextPass`,
  not `Update`.
- **Events**: in Bevy 0.17+ use `MessageReader<T>` / `MessageWriter<T>`.
- **UI colour tokens**: raw `Color32::from_*` and `bevy::Color::*` literals are
  CI-gated — `src/ui/theme.rs` is the only authorised home. Add new tokens
  there or the `audit_color32_literals.py` / `audit_bevy_color_literals.py`
  audits will fail the build.

### Worktree safety

This repo is frequently worked on by **multiple agents in parallel worktrees**
(Copilot sessions, Claude Code, subagents, CI bots), often sharing `target/`
and `Cargo.lock`. Naive worktree mutations have repeatedly destroyed
concurrent work. See `AGENTS.md` → "Multi-Agent Worktree Safety" for the
full decision table. The TL;DR:

- **Never** run `git stash` (any form), `git checkout -- <path>`,
  `git reset --hard`, `git clean -fdx`, `git switch` / `git checkout`
  to another branch, `cargo clean`, `rm -rf target/`, or `rm Cargo.lock`
  in a shared worktree.
- Use a fresh worktree (`git worktree add ../<branch>-worktree <branch>`)
  for hermetic experiments.
- Re-run failing tests on a fresh `main` worktree to confirm pre-existence
  rather than stashing.
- Use `cargo clean -p <specific-package>` instead of full `cargo clean`.

### Code Style
- Follow Rust standard style guidelines (use `rustfmt`)
- Run `cargo fmt --all -- --check` before committing
- Run `cargo clippy --all-targets --all-features -- -D warnings` to catch common issues
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

### Merge gates (must be green before PR)

Every PR must pass the following locally before opening it:

```bash
# Format
cargo fmt --all -- --check

# Lints (the authoritative gate)
cargo clippy --all-targets --all-features -- -D warnings

# UI colour-token audits (raw Color32 / bevy::Color literals → src/ui/theme.rs)
python3 scripts/audit_color32_literals.py --strict --baseline scripts/audit_color32_literals_baseline.txt src
python3 scripts/audit_bevy_color_literals.py --strict --baseline scripts/audit_bevy_color_literals_baseline.txt src

# SFX manifest + coverage (each SFX must be wired to ≥1 trigger site)
python3 scripts/audit_sfx_manifest.py
python3 scripts/audit_sfx_coverage.py

# B0001 dual-Query advisory (print-only by default; pass --strict to fail)
python3 scripts/audit_b0001.py src

# Tests (best-effort — the full Bevy test target can hit the GHA 60-min ceiling)
cargo test --all
```

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
2. Branch from `main`; never push to it directly.
   Create a feature branch (`git checkout -b feature/amazing-feature`).
3. Make your changes
4. Run tests (`cargo test`)
5. Format code (`cargo fmt`)
6. Check with clippy (`cargo clippy`)
7. Commit changes
8. Push to your fork
9. Open a Pull Request

Commit messages are verb-led, imperative, and reference the GitHub issue
or Linear key. Example: `Add resource management plugin (#123)`.

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
