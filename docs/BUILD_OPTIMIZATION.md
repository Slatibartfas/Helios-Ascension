# Build and Test Performance Optimizations

This document describes the optimizations made to improve build and test performance in the Helios Ascension project.

## Summary of Improvements

The following optimizations have been implemented to significantly reduce build and test times:

### 1. Fast Linker (LLD)
**Impact**: 2-5x faster linking times (Linux only)

The project uses LLVM's LLD linker instead of the default GNU ld linker on Linux. This dramatically reduces the time spent in the linking phase, especially for large projects with many dependencies like Bevy.

- **Platform**: Linux (x86_64-unknown-linux-gnu) only
- **Configuration**: `.cargo/config.toml`
- **How it works**: LLD is a modern linker designed for performance with parallel linking and better memory usage
- **Requirement**: `clang` and `lld` must be installed on Linux
- **Note**: macOS and Windows use their default system linkers which are already optimized

### 2. Automatic Build Parallelism
**Impact**: Faster compilation by utilizing available CPU cores

Cargo automatically detects and uses an appropriate number of parallel jobs based on your system's CPU cores.

- **Configuration**: Uses Cargo's default behavior (auto-detection)
- **How it works**: Cargo parallelizes compilation across available CPU cores
- **Note**: No manual configuration needed - Cargo optimizes based on your system

### 3. Optimized Test Profile
**Impact**: Faster test compilation and execution

A dedicated test profile has been added with balanced optimization levels:
- Project code: opt-level 1 (faster compilation)
- Dependencies: opt-level 2 (better runtime performance for tests)

- **Configuration**: `Cargo.toml` - `[profile.test]`
- **How it works**: Balances compilation speed with test execution speed
- **Benefit**: Tests compile faster while still running efficiently

### 4. Cargo Nextest Integration
**Impact**: Parallel test execution for faster test runs

Cargo nextest is a next-generation test runner that executes tests in parallel and provides better output.

- **Configuration**: `.config/nextest.toml`
- **How it works**: Runs tests in parallel using all CPU cores
- **Usage**: `cargo nextest run`
- **Features**:
  - Parallel execution on all CPU cores
  - Cleaner output (only shows failures)
  - Per-test timeouts

## Expected Performance Improvements

**Note**: The "Before" times are estimates based on typical performance without these optimizations, as actual baseline measurements weren't captured. Actual performance improvements will vary based on hardware configuration and specific workload.

### First-time Build
- **Before (estimated)**: 10-15 minutes (with default linker and limited parallelism)
- **After (measured)**: ~6 minutes (measured: 5m57s; may vary by system)
- **Improvement**: ~40-60% faster

### Incremental Rebuild (small changes)
- **Before (estimated)**: 30-60 seconds (typical without optimizations)
- **After (estimated)**: 10-20 seconds (with incremental compilation)
- **Improvement**: ~60-70% faster

### Test Execution
- **Before (measured)**: ~2 seconds (sequential execution)
- **After (measured)**: <1 second (parallel execution with nextest: 0.621s)
- **Improvement**: ~70% faster

## How to Use

### Install Dependencies
```bash
# Linux (Ubuntu/Debian) - LLD linker is REQUIRED
sudo apt-get install lld clang

# Linux (Fedora/RHEL)
sudo dnf install lld clang

# Linux (Arch)
sudo pacman -S lld clang

# Linux (openSUSE)
sudo zypper install lld clang

# macOS / Windows
# No additional dependencies needed - uses default system linker

# Install cargo-nextest (optional but recommended, all platforms)
cargo install cargo-nextest
```

### Build Commands
```bash
# Regular debug build (with all optimizations)
cargo build

# Run tests with cargo-nextest
cargo nextest run

# Use the existing 'fast' profile for rapid iteration (pre-configured in Cargo.toml)
cargo build --profile fast
```

### Verify Optimizations
You can verify that the optimizations are working by:

1. **Check linker** (Linux only): Look for "Linking" messages using `lld` in build output
2. **Check parallelism**: Monitor CPU usage during builds (should use available cores)
3. **Time builds**: Use `time cargo build` to measure build times

## Troubleshooting

### LLD Not Found (Linux)
If you get an error about `lld` not being found on Linux:
```bash
# Install LLD and clang
sudo apt-get install lld clang  # Ubuntu/Debian
# Or see installation instructions above for other distributions

# To temporarily disable LLD and use default linker:
# Remove or comment out the [target.x86_64-unknown-linux-gnu] section in .cargo/config.toml
```

### Cargo Nextest Not Found
```bash
cargo install cargo-nextest
```

### High Memory Usage
If you experience high memory usage during builds:
- Reduce `jobs` in `.cargo/config.toml` from 8 to 4 or lower
- Close other applications during builds

## Future Improvements

Potential future optimizations:
1. **Sccache/ccache**: Shared compilation cache across projects
2. **Cranelift backend**: Alternative code generation backend for faster debug builds
3. **Workspace splitting**: Break project into smaller crates for better incremental compilation
4. **Dependency pruning**: Remove unused dependencies and features
5. **Nightly features**: Use nightly Rust features for faster compilation

## References

- [Cargo Book - Build Configuration](https://doc.rust-lang.org/cargo/reference/config.html)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Cargo Nextest](https://nexte.st/)
- [LLD Linker](https://lld.llvm.org/)
- [Bevy Fast Compiles Guide](https://bevyengine.org/learn/book/getting-started/setup/#enable-fast-compiles-optional)
