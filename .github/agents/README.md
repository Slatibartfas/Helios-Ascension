# Claude Code Agents for Helios Ascension

This document describes specialized agents configured for use with Claude Code in this project.

## Available Agents

Claude Code provides built-in agents that can be invoked via the `Agent` tool. Below are the recommended configurations for this project:

### 1. Code Review Agent (`Explore` variant)

**Use for**: Reviewing changes, finding bugs, understanding code patterns

```bash
# Invoke via Agent tool with subagent_type: "Explore"
# Set description: "code-review"
```

**Best practices for this project**:
- Focus on ECS patterns (components vs systems separation)
- Check for proper error handling (Result types, not unwrap)
- Verify Bevy plugin conventions
- Review against conventions in `.github/copilot-instructions.md`

### 2. Bug Investigation Agent

**Use for**: Debugging issues, analyzing errors, finding root causes

```bash
# Invoke via Agent tool with subagent_type: "general-purpose"
```

**Best practices for this project**:
- Search for related error patterns in codebase
- Check recent git history for relevant changes
- Examine relevant Bevy systems and components
- Use `cargo build` to get compile errors

### 3. Documentation Agent (`Explore` variant)

**Use for**: Updating docs, verifying API changes, synchronizing code and docs

```bash
# Invoke via Agent tool with subagent_type: "Explore"
# Set description: "docs-update"
```

**Best practices for this project**:
- Key docs to check: `docs/UI.md`, `docs/RESOURCES.md`, `docs/ASTRONOMY.md`
- Verify RON schema changes in `assets/data/`
- Check modding guides match implementation

### 4. Testing Agent

**Use for**: Running tests, analyzing failures, creating new tests

```bash
# Invoke via Agent tool with subagent_type: "general-purpose"
```

**Best practices for this project**:
- Run `cargo test` for basic testing
- Run `cargo nextest run` for parallel testing
- Focus on integration tests in `tests/` directory

### 5. Planning Agent

**Use for**: Designing features, planning implementations, architectural decisions

```bash
# Invoke via Agent tool with subagent_type: "Plan"
```

**Best practices for this project**:
- Follow ECS architecture principles
- Consider Bevy plugin patterns
- Reference existing implementations in `src/plugins/`

### 6. Shipbuilding Data Agent

**Use for**: Editing hulls, ship modules, slot categories, construction data, and ship tech coupling

**Best practices for this project**:
- Treat `assets/data/ship_hulls.ron` and `assets/data/ship_modules.ron` as canonical
- Check `src/shipbuilding/data.rs` and `src/shipbuilding/types.rs` before adding new fields or enum values
- Verify module IDs stay unique and enum/resource names match Rust definitions exactly
- Validate with `cargo build` and a short `cargo run` because RON and duplicate-ID issues surface at runtime

### 7. Tech Tree Data Agent

**Use for**: Editing `assets/data/technologies.ron`, engineering components, prerequisites, and module unlock coupling

**Best practices for this project**:
- Keep technologies in the `technologies` array and engineering components in the `components` array
- Check unlock coupling between `unlocks_components`, engineering definitions, and ship module `required_tech`
- Preserve RON structure carefully during bulk edits
- Update `docs/SHIPBUILDING.md` and `.github/copilot-instructions.md` when data workflow changes

---

## How to Use These Agents

### Using the Agent Tool

```rust
// Example: Launch a code review agent
Agent {
    description: "code-review",
    subagent_type: "Explore",
    prompt: "Review the recent changes in src/fleets/ for proper ECS patterns and error handling..."
}
```

### Using the /simplify Skill

For quick code quality improvements:

```bash
/simplify
```

This invokes the built-in simplify skill which reviews changed code for reuse, quality, and efficiency.

---

## Quick Reference

| Task | Agent Type | Description |
|------|------------|-------------|
| Explore codebase | `Explore` | Fast file/keyword search |
| Research complex question | `general-purpose` | Multi-step analysis |
| Plan implementation | `Plan` | Architecture & steps |
| Debug issues | `general-purpose` | Root cause analysis |
| Update docs | `Explore` | Find related code |
| Shipbuilding data work | custom prompt | Hulls, modules, slot/category coupling |
| Tech tree data work | custom prompt | Technologies, prerequisites, engineering coupling |
