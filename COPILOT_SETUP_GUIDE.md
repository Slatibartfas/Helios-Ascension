# GitHub Copilot Setup - Usage Guide

This document explains how to use the GitHub Copilot configuration that has been set up for Helios Ascension.

## What Was Created

The following structure has been added to your repository:

```
.github/
├── copilot-instructions.md          # Main project instructions
├── instructions/                     # Language and domain-specific instructions
│   ├── rust.instructions.md          # Rust coding standards
│   ├── testing.instructions.md       # Testing best practices
│   ├── documentation.instructions.md # Documentation standards
│   ├── security.instructions.md      # Security guidelines
│   ├── performance.instructions.md   # Performance optimization
│   └── code-review.instructions.md   # Code review standards
├── prompts/                          # Reusable task prompts
│   ├── setup-component.prompt.md     # Create Bevy components/plugins/systems
│   ├── write-tests.prompt.md         # Generate comprehensive tests
│   ├── code-review.prompt.md         # Perform code reviews
│   ├── refactor-code.prompt.md       # Refactor code
│   ├── generate-docs.prompt.md       # Generate documentation
│   └── debug-issue.prompt.md         # Debug and fix issues
├── agents/                           # Specialized chat modes
│   ├── architect.agent.md            # Architecture planning mode
│   ├── reviewer.agent.md             # Code review mode
│   └── debugger.agent.md             # Debugging mode
└── workflows/
    └── copilot-setup-steps.yml       # GitHub Actions workflow for Coding Agent
```

## How to Use

### 1. Automatic Instructions

GitHub Copilot will automatically use the instructions in `.github/copilot-instructions.md` and `.github/instructions/` when:
- Generating code in the editor
- Responding to chat questions
- Making code suggestions

These instructions ensure Copilot understands:
- Your Rust and Bevy coding standards
- Your testing approach
- Your documentation requirements
- Your security and performance considerations

**No action needed** - just use Copilot as normal and it will follow these guidelines!

### 2. Using Prompts

Prompts are reusable templates for common development tasks. To use a prompt:

**In VS Code:**
1. Open GitHub Copilot Chat (Ctrl+Shift+I or Cmd+Shift+I)
2. Type `@workspace /prompt-name` or select from the prompt library
3. Answer any questions the prompt asks
4. Copilot will complete the task following the prompt's guidance

**Available Prompts:**
- **setup-component**: Create a new Bevy component, system, plugin, resource, or event
- **write-tests**: Generate comprehensive unit or integration tests
- **code-review**: Get a thorough code review of your changes
- **refactor-code**: Refactor code for better quality or performance
- **generate-docs**: Create API documentation, README content, or architecture docs
- **debug-issue**: Debug and fix issues systematically

**Example:**
```
@workspace Can you create a new Bevy plugin for handling player input?
```

Copilot will use the `setup-component.prompt.md` guidance to help you create a proper plugin following Bevy best practices.

### 3. Using Chat Modes (Agents)

Chat modes are specialized assistants for specific scenarios:

**Architect Mode** (`architect.agent.md`):
- Planning new features
- Designing system architecture
- Creating implementation plans
- Analyzing trade-offs

**Example usage:**
```
@workspace /architect
I need to add an economy system to the game. Can you help me design the architecture?
```

**Reviewer Mode** (`reviewer.agent.md`):
- Performing thorough code reviews
- Checking for quality issues
- Verifying best practices
- Providing constructive feedback

**Example usage:**
```
@workspace /reviewer
Please review my changes to the physics system
```

**Debugger Mode** (`debugger.agent.md`):
- Investigating bugs
- Finding root causes
- Systematic debugging
- Suggesting fixes

**Example usage:**
```
@workspace /debugger
The camera movement is jerky when zooming. Help me debug this.
```

### 4. GitHub Actions Workflow

The `copilot-setup-steps.yml` workflow runs automatically when:
- You trigger it manually (workflow_dispatch)
- You push changes to the workflow file
- You open a PR that modifies the workflow file

**What it does:**
1. Installs Rust and required system dependencies
2. Checks code formatting with `cargo fmt`
3. Runs linting with `cargo clippy`
4. Builds the project
5. Runs all tests

This workflow helps GitHub Copilot understand your build and test process.

## VS Code Setup

To get the most out of this setup:

1. **Install GitHub Copilot Extension**
   - Install "GitHub Copilot" from the VS Code marketplace
   - Sign in with your GitHub account

2. **Enable Copilot Chat**
   - Install "GitHub Copilot Chat" extension
   - Access with Ctrl+Shift+I (Windows/Linux) or Cmd+Shift+I (Mac)

3. **Workspace Context**
   - Copilot automatically reads `.github/copilot-instructions.md`
   - Instructions in `.github/instructions/` are applied based on file types
   - Prompts are available via `@workspace` in chat

4. **Configure Settings (Optional)**
   - Open VS Code settings (Ctrl+,)
   - Search for "Copilot"
   - Customize suggestions and behavior

## Usage Examples

### Creating a New Component

```
@workspace I need to create a velocity component for celestial bodies
```

Copilot will:
- Follow the setup-component prompt
- Create a proper Component with appropriate traits
- Add documentation
- Follow Rust and Bevy best practices

### Writing Tests

```
@workspace Write tests for the orbital mechanics calculations in solar_system.rs
```

Copilot will:
- Follow testing.instructions.md guidelines
- Create comprehensive test cases
- Test edge cases
- Follow Rust testing patterns

### Getting Code Review

```
@workspace /reviewer
Review my changes to improve performance
```

Copilot will:
- Check against all instruction files
- Review for correctness, quality, performance
- Provide specific, actionable feedback
- Categorize issues (required, suggestion, question)

### Debugging

```
@workspace /debugger
My entities are not spawning correctly
```

Copilot will:
- Ask for details (expected vs actual behavior)
- Suggest debugging strategies
- Help locate the issue
- Propose fixes

### Architecture Planning

```
@workspace /architect
Design a system for managing resources and logistics between celestial bodies
```

Copilot will:
- Ask clarifying questions
- Analyze current architecture
- Propose detailed design
- Create implementation plan
- Consider alternatives

## Customization

Feel free to customize the configuration files:

### Modifying Instructions
Edit files in `.github/instructions/` to:
- Add project-specific conventions
- Update coding standards
- Add new guidelines
- Refine existing instructions

### Adding New Prompts
Create new `.prompt.md` files in `.github/prompts/` for common tasks specific to your project.

### Creating New Chat Modes
Add new `.agent.md` files in `.github/agents/` for specialized scenarios.

### Updating the Workflow
Modify `.github/workflows/copilot-setup-steps.yml` to:
- Add new build steps
- Change test commands
- Add additional checks

## Tips for Best Results

1. **Be Specific**: The more context you provide, the better Copilot can help
2. **Reference Instructions**: Mention specific guidelines when relevant
3. **Iterate**: If the first suggestion isn't perfect, refine your request
4. **Use Chat Modes**: Select the appropriate mode for your task
5. **Review Output**: Always review and test generated code

## Troubleshooting

**Copilot not following instructions?**
- Ensure you're using `@workspace` in chat
- Check that instruction files are in `.github/` directory
- Try being more specific in your request

**Prompts not showing up?**
- Verify `.prompt.md` files are in `.github/prompts/`
- Check YAML frontmatter is correct
- Restart VS Code

**Chat modes not working?**
- Verify `.agent.md` files are in `.github/agents/`
- Check YAML frontmatter is correct
- Use `@workspace /mode-name` syntax

## Additional Resources

- [GitHub Copilot Documentation](https://docs.github.com/en/copilot)
- [Bevy Documentation](https://bevyengine.org/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [Awesome Copilot Repository](https://github.com/github/awesome-copilot)

## Support

If you have questions or need help:
1. Check the documentation files in `.github/`
2. Ask Copilot using `@workspace`
3. Review examples in existing code
4. Check the Bevy and Rust documentation

---

Enjoy your enhanced GitHub Copilot experience! 🚀
