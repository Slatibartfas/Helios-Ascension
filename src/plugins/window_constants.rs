//! Shared window constants used by `main.rs` and the minimize-guard
//! plugin.
//!
//! Lifted out of `src/main.rs` so that plugin code (which lives in
//! the library and therefore cannot `use crate::main`) can share
//! the values without redefining them. Both `main.rs` and
//! `src/plugins/minimize_guard.rs` import from this module.

/// Minimum supported window width for the main game (logical px).
pub const MIN_WINDOW_WIDTH: f32 = 1280.0;

/// Minimum supported window height for the main game (logical px).
pub const MIN_WINDOW_HEIGHT: f32 = 720.0;
