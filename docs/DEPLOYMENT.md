# Deployment Strategy

## Overview

Helios Ascension uses GitHub Actions for automated CI/CD. Binary builds are distributed via GitHub Releases with support for multiple platforms.

## Current Setup

### CI Pipeline (`.github/workflows/ci.yml`)

Runs on every push to `main`/`develop` and all PRs:
- **Rustfmt**: Code formatting check
- **Clippy**: Linting with strict warnings
- **Test Suite**: Unit and integration tests
- **Build (fast)**: Quick development build
- **Build (release)**: Multi-platform release builds (Linux, macOS, Windows)

### Release Pipeline (`.github/workflows/release.yml`)

Triggered on version tags (`v*.*.*`):
1. Creates GitHub Release with auto-generated changelog
2. Builds release binaries for all platforms
3. Uploads binaries as release assets

## Version Tagging

Semantic versioning is enforced by the release workflow:
- Format: `vMAJOR.MINOR.PATCH` (e.g., `v0.4.0`)
- Tags trigger full release pipeline
- Release notes auto-generated from commit history

## Platform Support

| Platform | Architecture | Binary Name |
|----------|--------------|-------------|
| Linux | x86_64 | `helios_ascension_linux_x86_64` |
| Linux | ARM64 | `helios_ascension_linux_arm64` |
| macOS | x86_64 | `helios_ascension_macos` |
| macOS | ARM64 | `helios_ascension_macos_arm64` |
| Windows | x86_64 | `helios_ascension_windows_x86_64.exe` |

## Distribution Channels

### GitHub Releases (Primary)
- Automatic on tag push
- Direct binary downloads
- Versioned releases with changelog

### itch.io Integration (Planned)
- Manual upload workflow
- Requires itch.io API key configuration
- See: [DEL-14](/b9ddb369/issues/DEL-14)

### Steam Distribution (Planned)
- Requires Steam Direct submission
- See: [DEL-15](/b9ddb369/issues/DEL-15)

## Making a Release

```bash
# Update version in Cargo.toml
# Create and push tag
git tag v0.4.0 -m "Release v0.4.0"
git push origin v0.4.0
```

The workflow will:
1. Build all platform binaries
2. Create GitHub release
3. Attach all binaries to release

## System Requirements

### Linux
- Ubuntu 20.04+ or equivalent
- glibc 2.31+
- Vulkan-capable GPU for graphics

### macOS
- macOS 11+ (Big Sur or later)
- Apple Silicon or Intel

### Windows
- Windows 10+
- DirectX 11 or Vulkan

## Deployment Verification

After release, verify:
- [ ] Binaries launch without errors
- [ ] Graphics render correctly
- [ ] Save/load functionality works
- [ ] No console errors in release build
