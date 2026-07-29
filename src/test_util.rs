//! Shared test utilities.
//!
//! Always compiled, but contains nothing that affects non-test
//! builds.  Tests that mutate process-global state (env vars,
//! working directory, shared temp dirs) MUST route their
//! mutations through helpers here so the parallel test harness
//! cannot observe inconsistent state.

/// Process-wide lock serializing every test that mutates the
/// `HELIOS_USERDATA_DIR` env var.
///
/// Background: `std::env::set_var` is documented as not
/// thread-safe.  Rust's default test harness runs tests in
/// parallel across threads of one test binary; without a lock,
/// `subview_save_game::save_panel_save_writes_file_and_rescans_index`
/// races with `game_setup::restore_missing_path_emits_notification_and_returns_err`
/// and `userdata::resolve_userdata_dir_respects_override`:
/// one test sets `HELIOS_USERDATA_DIR` between another test's
/// `current_slot_path()` (which resolves the dir for the save
/// write) and its `rescan_save_index` (which re-reads the env
/// var), so the rescan looks in a different dir and finds zero
/// files.  Acquiring this lock around every `set_var`/use/restore
/// window closes the race.
///
/// Used by:
/// - `src/ui/launch/subview_save_game.rs::tests::install_userdata`
/// - `src/persistence/game_setup.rs::tests::install_userdata_dir`
/// - `src/ui/launch/userdata.rs::tests::resolve_userdata_dir_respects_override`
#[cfg(test)]
pub static USERDATA_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());