# Bevy Engine Expert — Project Memory

## Helios-Ascension build performance

Workspace: `G:\Repositories\Helios-Ascension`, Rust 1.93, cargo 1.93, Bevy 0.18.

### Measured baselines (Windows MSVC)
- **Before (`opt-level=3` on all deps + `link.exe`):** 22.7 s per single-file incremental rebuild; 3 min 47 s cold (deps cached).
- **After (profile fix + `rust-lld` + Bevy feature trim):** 16.0 s per single-file incremental rebuild — stable across leaf files, 1k LoC files, and 3799-LoC `survey/systems.rs`. First run after the change cost ~5 min to re-warm dep cache (one-time).

### What is in the repo as of 2026-07-08
- `Cargo.toml`:
  - `[profile.dev]` now `opt-level = 1`, `debug = "line-tables-only"`. The `[profile.dev.package."*"] opt-level = 3` override is **removed** (largest single compile-time cost).
  - Bevy features trimmed: `x11`, `wayland`, `jpeg`, `png` dropped. `mp3`/`bevy_audio` retained because `plugins::music` plays `assets/audio/music/*.mp3`.
  - `bevy-inspector-egui` removed entirely: confirmed unused (only mentioned in two comments in `src/ui/notifications/`).
- `.cargo/config.toml`:
  - Windows MSVC: `linker = "...stable-x86_64-pc-windows-msvc/lib/rustlib/x86_64-pc-windows-msvc/bin/rust-lld.exe"`, `link-arg=-flavor=link`, `link-arg=/DEBUG:NONE`.
  - Linux: clang+lld (unchanged).
- `rust-toolchain.toml`: pins `channel = "stable"`, `components = ["rust-src", "rustfmt", "clippy"]`.
- `rustup component add llvm-tools` was required; `rust-lld.exe` lives at `<toolchain>/lib/rustlib/<target>/bin/` and is **not** on PATH.

### Gotchas for future-me
- `rustup which rust-lld` errors with "not a file" because rust-lld.exe is in the toolchain `lib/rustlib/.../bin/`, not the toolchain `bin/`. The `~/.cargo/bin` shim does not exist. Use the absolute path in `.cargo/config.toml`.
- Hard-coding the absolute path is brittle — if the user switches toolchain, `link.exe` is silently used again. Follow-up: `scripts/setup-linker.ps1` that rewrites the path, or a `build.rs` that emits `cargo:rustc-link-arg`.
- Bevy's `serialize` feature drives `ReflectSerialize` derive machinery; required by `DynamicScene` in `src/persistence/snapshot.rs`. Do not drop.
- `image` 0.25 (direct dep, separate from Bevy) pulls in jpeg/png decoders, so dropping Bevy's `jpeg`/`png` features is safe.
- 233 types `#[derive(Reflect)]` + 66 `register_type!` sites. Persistence is dense; data-only components could drop `Reflect` to shrink codegen + `TypeRegistry` startup.
- Stale 145 GB target artifacts came from a deleted `src/bin/diagnostic_porkchop.rs` whose cached deps persisted. `cargo clean -p helios_ascension` removed 10.9 GB without invalidating the dep cache.

### Next things to measure
- `RUSTC_WRAPPER=sccache` — reuse dep `.o` cache across machines; biggest CI win.
- Reflect-derive audit on data-only components.
- Workspace split — only if cold builds matter again.

### Don't change without measuring
- Don't `cargo clean` (full) — invalidates 30+ min of dep cache.
- Don't drop `[profile.fast]` — separate user choice.
- Don't replace `bevy/serialize` with anything else.
