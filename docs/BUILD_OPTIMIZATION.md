# Build and Test Performance Optimizations

This document describes the optimizations made to improve build and test performance in the Helios Ascension project.

## Summary of Improvements

The following optimizations have been implemented to significantly reduce build and test times:

### 1. Fast Linker (LLD)
**Impact**: 2-5x faster linking times

The project now uses LLVM's LLD linker instead of the default GNU ld linker. This dramatically reduces the time spent in the linking phase, especially for large projects with many dependencies like Bevy.

- **Configuration**: `.cargo/config.toml`
- **How it works**: LLD is a modern linker designed for performance with parallel linking and better memory usage
- **Requirement**: `clang` and `lld` must be installed

### 2. Increased Build Parallelism
**Impact**: Faster compilation by utilizing all CPU cores

Build parallelism has been increased from 4 to 8 jobs, allowing the compiler to fully utilize all available CPU cores during compilation.

- **Configuration**: `.cargo/config.toml` - `jobs = 8`
- **How it works**: More parallel compilation units means faster overall build times
- **Note**: This can be adjusted based on your system's CPU count

### 3. Incremental Compilation
**Impact**: Significantly faster rebuilds

Incremental compilation is now explicitly enabled, allowing the compiler to reuse work from previous builds.

- **Configuration**: `Cargo.toml` - `incremental = true` in dev and test profiles
- **How it works**: The compiler only recompiles what has changed, not the entire codebase
- **Benefit**: Subsequent builds after making small changes are much faster

### 4. Optimized Test Profile
**Impact**: Faster test compilation and execution

A dedicated test profile has been added with balanced optimization levels:
- Project code: opt-level 1 (faster compilation)
- Dependencies: opt-level 2 (better runtime performance for tests)

- **Configuration**: `Cargo.toml` - `[profile.test]`
- **How it works**: Balances compilation speed with test execution speed
- **Benefit**: Tests compile faster while still running efficiently

### 5. Cargo Nextest Integration
**Impact**: Parallel test execution for faster test runs

Cargo nextest is a next-generation test runner that executes tests in parallel and provides better output.

- **Configuration**: `.config/nextest.toml`
- **How it works**: Runs tests in parallel using all CPU cores
- **Usage**: `cargo nextest run`
- **Features**:
  - Parallel execution on all CPU cores
  - Cleaner output (only shows failures)
  - Automatic retry of flaky tests (2 retries)
  - Per-test timeouts

### 6. Optional Window System Features
**Impact**: Ability to skip heavy window system dependencies for testing

Window system features (X11 and Wayland) have been made optional through Cargo features:
- Default: includes windowing support
- `--no-default-features`: builds without windowing for headless environments

- **Configuration**: `Cargo.toml` - `[features]`
- **Use case**: CI/CD pipelines, headless testing, or when developing non-graphics code
- **Note**: Currently requires window system libraries even with this flag due to Bevy dependencies

## Expected Performance Improvements

**Note**: The "Before" times are estimates based on typical performance without these optimizations, as actual baseline measurements weren't captured. Actual performance improvements will vary based on hardware configuration and specific workload.

### First-time Build
- **Before (estimated)**: 10-15 minutes (with default linker and limited parallelism)
- **After (measured)**: 3-6 minutes (with LLD and full parallelism)
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
# Install LLD linker (Ubuntu/Debian)
sudo apt-get install lld

# Install cargo-nextest (one-time setup)
cargo install cargo-nextest
```

### Build Commands
```bash
# Regular debug build (with all optimizations)
cargo build

# Run tests with cargo-nextest
cargo nextest run

# Fast profile for rapid iteration
cargo build --profile fast
```

### Verify Optimizations
You can verify that the optimizations are working by:

1. **Check linker**: Look for "Linking" messages using `lld` in build output
2. **Check parallelism**: Monitor CPU usage during builds (should use all cores)
3. **Time builds**: Use `time cargo build` to measure build times

## Troubleshooting

### LLD Not Found
If you get an error about `lld` not being found:
```bash
# Install LLD
sudo apt-get install lld

# Or temporarily disable it by removing .cargo/config.toml linker configuration
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
