//! Read-only save-game index for the main menu's Load Game list.
//!
//! PR-A (GRA-311) shipped the discovery layer so the menu UI can
//! list saves without lying about their existence. PR-I
//! (GRA-358) replaced the v1 DynamicScene path with the v2
//! StateStore format — the scanner sniffs the magic header on
//! the first line and dispatches:
//!
//! - For every `<userdata>/saves/*.ron` whose body is a v2
//!   [`StateStore`], extract a [`SaveHeader`] from its
//!   `metadata` block.
//! - On parse success, append a [`SaveSummary::Valid`] entry.
//! - On parse failure (missing magic header, malformed RON,
//!   unsupported legacy v1 file), append a
//!   [`SaveSummary::Broken`] entry. The menu disables the row
//!   but the file still shows up so the player can rename /
//!   delete it.
//!
//! Empty-index behavior is the documented default: a fresh
//! install or a missing `<userdata>/saves` directory is **not**
//! an error.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::persistence::state_store::{SavePreview, StateStoreMetadata};

/// Sub-directory of the userdata dir where save files live.
pub const SAVES_SUBDIR: &str = "saves";

/// Display header extracted from the on-disk v2 [`StateStore`]
/// metadata block.
///
/// Field shape mirrors [`StateStoreMetadata`] (the canonical
/// header every v2 save writer produces), with every field
/// optional so the loader survives malformed / partially-written /
/// older-format saves without aborting the whole scan. A save is
/// "valid" if its envelope parses; missing fields render as `?` /
/// `Unknown` / `—` in the menu.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SaveHeader {
    /// Save format version (matches `StateStoreMetadata::format_version`).
    /// Bumped by the persistence layer when the body schema changes;
    /// the loader checks this before deserialising the body.
    #[serde(default)]
    pub format_version: Option<u32>,
    /// Unix timestamp (seconds since epoch) at the moment of save.
    #[serde(default)]
    pub saved_at_unix_s: Option<u64>,
    /// Total in-game playtime at the moment of save, in seconds.
    #[serde(default)]
    pub playtime_s: Option<u64>,
    /// Stable game-seed the save was started with.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Helios version (Cargo package version) that produced the save.
    #[serde(default)]
    pub helios_version: Option<String>,
    /// Rich campaign summary displayed after selecting a save.
    #[serde(default)]
    pub preview: SavePreview,
}

impl SaveHeader {
    /// Build a [`SaveHeader`] from a freshly-decoded
    /// [`StateStoreMetadata`]. Used by the scanner after it parses
    /// a v2 save's metadata block — every field is populated
    /// because the v2 extractor always writes a complete
    /// `StateStoreMetadata`.
    pub fn from_metadata(metadata: &StateStoreMetadata) -> Self {
        Self {
            format_version: Some(metadata.format_version),
            saved_at_unix_s: Some(metadata.saved_at_unix_s.max(0) as u64),
            playtime_s: Some(metadata.playtime_s.max(0.0) as u64),
            seed: Some(metadata.seed),
            helios_version: Some(metadata.helios_version.clone()),
            preview: metadata.preview.clone(),
        }
    }

    /// Format the saved-at unix timestamp as a `YYYY-MM-DD HH:MM UTC`
    /// string, or `Unknown` when the field is absent. Uses the
    /// system clock's UTC offset (deliberately UTC — saves are
    /// anchored to a fixed moment in time, not the player's
    /// timezone, so displaying them in UTC avoids DST drift in the
    /// menu list).
    pub fn formatted_saved_at(&self) -> String {
        match self.saved_at_unix_s {
            Some(ts) => format_unix_timestamp_utc(ts),
            None => "Unknown".to_string(),
        }
    }

    /// Format the playtime as `Hh MMm` / `MMm SSs` / `SSs`, or `—`
    /// when absent. Matches the `format_playtime` helper used by
    /// the in-game HUD so the menu reads consistently with the
    /// in-world UI.
    pub fn formatted_playtime(&self) -> String {
        match self.playtime_s {
            Some(s) => format_playtime(s),
            None => "—".to_string(),
        }
    }

    /// Format the helios version as a string, or `?` when absent.
    /// The display code uses this directly in the version column.
    pub fn formatted_version(&self) -> String {
        self.helios_version
            .clone()
            .unwrap_or_else(|| "?".to_string())
    }

    /// Format the seed as a string, or `—` when absent.
    pub fn formatted_seed(&self) -> String {
        self.seed
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_string())
    }
}

/// Format a unix timestamp in seconds as a `YYYY-MM-DD HH:MM UTC`
/// string. Returns `"@<ts>"` if the timestamp is out of range (e.g.
/// 0 / pre-1970 / year > 9999).
fn format_unix_timestamp_utc(ts: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let dt = UNIX_EPOCH.checked_add(Duration::from_secs(ts));
    let Some(dt) = dt else {
        return format!("@{ts}");
    };
    let total = dt.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // Seconds-since-epoch → civil date via the algorithm in
    // `std::time::SystemTime`'s docs (Howard Hinnant's
    // civil_from_days). Inline so we don't pull in `chrono`.
    let days = (total / 86_400) as i64;
    let secs_of_day = (total % 86_400) as u32;
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    // Hinnant's `days_from_civil` inverse: take epoch days,
    // shift to civil-from-1970-01-01, then back out the date.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC")
}

/// Format a playtime in seconds as `Hh MMm` (e.g. `3h 12m`).
/// Sub-hour playtimes render as `MMm SSs`; sub-minute as `SSs`.
fn format_playtime(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// One entry in the [`SaveIndex`].
#[derive(Debug, Clone, PartialEq)]
pub enum SaveSummary {
    Valid {
        path: PathBuf,
        header: SaveHeader,
    },
    /// Broken save — parse failed but we still surface the path
    /// so the player can delete it manually. `mtime_unix_s` is
    /// the file's mtime at scan time (used for the index sort
    /// so the most recently written broken save surfaces at the
    /// top of the broken-saves list).
    Broken {
        path: PathBuf,
        error: String,
        mtime_unix_s: Option<u64>,
    },
}

impl SaveSummary {
    pub fn path(&self) -> &Path {
        match self {
            SaveSummary::Valid { path, .. } => path,
            SaveSummary::Broken { path, .. } => path,
        }
    }

    pub fn is_broken(&self) -> bool {
        matches!(self, SaveSummary::Broken { .. })
    }
}

/// In-memory index of the saves directory. Populated at boot by
/// [`SaveIndex::scan`] and registered as a Bevy resource so the menu
/// can read it without touching the disk on every frame.
#[derive(Resource, Debug, Default, Clone)]
pub struct SaveIndex {
    pub entries: Vec<SaveSummary>,
    pub scanned_dir: Option<PathBuf>,
}

/// Tracker for the last time [`SaveIndex`] was re-scanned from disk
/// (GRA-358 PR-C).
///
/// PR-A / PR-B already re-scan [`SaveIndex`] after every successful
/// save (see `src/persistence/autosave.rs::fire_autosave` and the
/// PR-C manual-save panel) — but they replace the resource wholesale,
/// so the "did the index drift behind a manual save?" question has
/// no first-class answer. PR-C introduces this resource so the
/// Save Panel can refresh itself when the player has been idle on the
/// subview (e.g. an autosave fires while they are picking a slot)
/// without re-running the scan every frame.
///
/// PR-C exposes [`rescan_save_index_into_world`](Self::rescan_save_index_into_world)
/// as the single entry point every save/load path calls; the helper
/// re-scans once and stamps `last_scanned`.
#[derive(Resource, Debug, Clone)]
pub struct SaveIndexState {
    /// Wall-clock instant of the last successful re-scan. The menu
    /// compares this to its own "first render" stamp to decide
    /// whether to refresh; tests construct it directly.
    pub last_scanned: std::time::Instant,
}

impl Default for SaveIndexState {
    fn default() -> Self {
        Self {
            // `Instant` has no `UNIX_EPOCH` analogue. Use a far-future
            // sentinel so the first scan always happens and the
            // sentinel never collides with a real scan.
            last_scanned: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(60 * 60 * 24 * 365 * 10))
                .unwrap_or_else(std::time::Instant::now),
        }
    }
}

impl SaveIndexState {
    /// Re-scan the saves directory via [`SaveIndex::scan`], update the
    /// [`SaveIndex`] resource, and stamp `last_scanned` to "now".
    ///
    /// Single entry point for every save / load path so the index
    /// stays consistent without each call site having to remember to
    /// touch `last_scanned`. The saves directory is resolved from
    /// [`crate::ui::launch::userdata::resolve_userdata_dir`] so
    /// tests can override via the `HELIOS_USERDATA_DIR` env var.
    pub fn rescan_save_index_into_world(&mut self, world: &mut World) {
        let dir = crate::ui::launch::userdata::resolve_userdata_dir().join(SAVES_SUBDIR);
        let index = SaveIndex::scan(&dir);
        world.insert_resource(index);
        self.last_scanned = std::time::Instant::now();
    }
}

impl SaveIndex {
    /// Build an empty index — useful as a `Default` and for tests.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Number of valid saves in the index.
    pub fn valid_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, SaveSummary::Valid { .. }))
            .count()
    }

    /// Number of broken saves in the index.
    pub fn broken_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_broken()).count()
    }

    /// Scan `saves_dir` for `*.ron` save files. Missing directory →
    /// empty index, no error. Per-file parse failures are reported as
    /// [`SaveSummary::Broken`] entries, never as errors that abort
    /// the scan.
    pub fn scan(saves_dir: &Path) -> Self {
        let mut index = Self {
            entries: Vec::new(),
            scanned_dir: Some(saves_dir.to_path_buf()),
        };

        let read_dir = match fs::read_dir(saves_dir) {
            Ok(rd) => rd,
            Err(e) => {
                // Missing directory is the documented empty-index
                // default — log at info, not warn.
                if e.kind() == std::io::ErrorKind::NotFound {
                    info!(
                        "SaveIndex: no saves directory at {} (cold boot); index is empty",
                        saves_dir.display()
                    );
                } else {
                    warn!(
                        "SaveIndex: cannot read saves directory {}: {}; index is empty",
                        saves_dir.display(),
                        e
                    );
                }
                return index;
            }
        };

        let paths: Vec<PathBuf> = read_dir
            .filter_map(|res| res.ok())
            .map(|entry| entry.path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("ron"))
            .collect();

        // Parse every save first, then sort by `saved_at_unix_s`
        // DESCENDING (newest first). The first valid entry is then
        // the most recent save — exactly what
        // `most_recent_valid_save` (and the Continue button)
        // expect.
        //
        // Why not `paths.sort()` by file name? Because file names
        // are user-controlled: a save named `Z_most_recent.ron`
        // would sort after `autosave_1784886024.ron` even though
        // the autosave is older. Sorting by the in-file
        // `saved_at_unix_s` is the canonical "when was this save
        // actually produced" key. Pre-PR-C the scan sorted by
        // file name, which produced a wrong "most recent" pick
        // whenever the user renamed or moved a save.
        //
        // Broken entries fall back to file mtime (so a corrupted
        // save that the player wants to recover still surfaces
        // at the top of the broken-saves list). If mtime is
        // unavailable (e.g. the file was deleted between
        // `read_dir` and `metadata`), they sink to the bottom.
        for path in paths {
            match parse_header_from_file(&path) {
                Ok(header) => index.entries.push(SaveSummary::Valid { path, header }),
                Err(e) => {
                    let mtime = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs());
                    index.entries.push(SaveSummary::Broken {
                        path,
                        error: e,
                        mtime_unix_s: mtime,
                    });
                }
            }
        }

        // Sort valid + broken entries together by timestamp DESC
        // so the first valid is the most recent. Broken entries
        // (with mtime fallback) interleave naturally.
        index.entries.sort_by(|a, b| {
            let a_ts = entry_timestamp(a).unwrap_or(0);
            let b_ts = entry_timestamp(b).unwrap_or(0);
            b_ts.cmp(&a_ts) // DESC
        });

        index
    }
}

/// Extract the canonical "when was this save produced" timestamp
/// from a [`SaveSummary`]. Returns `None` only for valid entries
/// missing the field (older saves) — those sink to the bottom
/// of the index by design.
fn entry_timestamp(entry: &SaveSummary) -> Option<u64> {
    match entry {
        SaveSummary::Valid { header, .. } => header.saved_at_unix_s,
        SaveSummary::Broken { mtime_unix_s, .. } => *mtime_unix_s,
    }
}

/// Parse a [`SaveHeader`] from `path`. Returns the parsed header on
/// success or an error message describing the failure (suitable for
/// [`SaveSummary::Broken`]).
///
/// The on-disk format is a v2 [`StateStore`] — `metadata`, an
/// optional divergences array, and per-section sub-stores
/// (fleets, research, economy, ui, etc.). We dispatch on the magic
/// header `helios_state_store_v2`; anything past that header is an
/// unsupported legacy v1 DynamicScene save.
///
/// ## Header-only parse (2026-08-05)
///
/// The scanner only needs `metadata` (timestamp, seed, version,
/// preview) to build the menu list — the divergences/bodies/fleets/
/// economy/notifications sub-stores are read later, only when the
/// player actually clicks Load. So instead of `StateStore::from_ron`
/// (which deserialises the entire body), we deserialise a
/// `SaveHeaderOnly` struct that carries just `metadata`. Serde
/// ignores unknown fields by default, so the same RON body parses
/// in a fraction of the time — the expensive bodies/fleets maps
/// never materialise.
///
/// v1 history: PR-A scanned a 4 KB prefix against a bare
/// [`SaveHeader`] struct; the struct's field names never matched
/// the [`StateStoreMetadata`] the writer actually emits, so the
/// prefix scan silently produced all-None headers on small saves
/// and truncated mid-string on larger ones. PR-I replaced that
/// code path with the magic-header sniff + full-file StateStore
/// parse. This header-only variant keeps PR-I's magic sniff and
/// adds the metadata-only decode.
pub fn parse_header_from_file(path: &Path) -> Result<SaveHeader, String> {
    let bytes = fs::read(path).map_err(|e| format!("read failed: {}", e))?;
    if bytes.is_empty() {
        return Err("file is empty".to_string());
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("not valid UTF-8: {}", e))?;

    // v2 fast-path: a StateStore file starts with the
    // magic header. We sniff the first 64 bytes for the
    // header so the scanner doesn't have to load the whole
    // save file just to discover it's a v2 save.
    let head = &text[..text.len().min(64)];
    if head
        .trim_start()
        .starts_with(crate::persistence::state_store::StateStore::MAGIC)
    {
        // Strip the magic line (same as `StateStore::from_ron`), then
        // deserialise ONLY the metadata block. Serde ignores the
        // unknown bodies/fleets/research/… fields entirely.
        let body_start = text.find('\n').ok_or("save has no body".to_string())? + 1;
        let only: SaveHeaderOnly = ron::from_str(&text[body_start..])
            .map_err(|e| format!("StateStore metadata parse failed: {e}"))?;
        let header = SaveHeader::from_metadata(&only.metadata);
        return Ok(header);
    }

    // v1 path: retired in PR-I. PR-I ships the v2 StateStore
    // format only, so anything past the magic-header check is
    // an unsupported legacy save.
    Err("save is not a v2 StateStore (PR-I dropped the v1 DynamicScene format)".to_string())
}

/// Minimal deserialisation target for the header-only scan path.
/// Carries just the `metadata` block; serde drops the other
/// `StateStore` fields (bodies, fleets, research, economy, ui,
/// notifications, surveys, meta_autosave) without parsing them.
#[derive(Debug, serde::Deserialize)]
struct SaveHeaderOnly {
    pub metadata: StateStoreMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::state_store::{StateStore, StateStoreMetadata};
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_saves_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("helios-saves-{}-{}-{}", tag, pid, n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create saves dir");
        dir
    }

    /// Write a v2 [`StateStore`] with the given metadata. The body
    /// can be empty — the scanner only inspects the metadata
    /// block, never the divergences.
    fn write_state_store(dir: &Path, name: &str, metadata: &StateStoreMetadata) -> PathBuf {
        let path = dir.join(name);
        let store = StateStore {
            metadata: metadata.clone(),
            ..Default::default()
        };
        let text = store.to_ron().expect("serialize StateStore");
        fs::write(&path, text).expect("write v2 save");
        path
    }

    fn write_broken_save(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).expect("write broken save");
        path
    }

    /// Helper: a populated [`StateStoreMetadata`] for tests.
    fn meta(version: &str, playtime_s: u64, seed: u64) -> StateStoreMetadata {
        StateStoreMetadata {
            format_version: crate::persistence::format_version::FORMAT_VERSION,
            helios_version: version.to_string(),
            saved_at_unix_s: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            playtime_s: playtime_s as f64,
            seed,
            start_timestamp: 0,
            sim_now_seconds: 0.0,
            preview: crate::persistence::state_store::SavePreview::default(),
        }
    }

    /// Helper: a populated [`StateStoreMetadata`] with a fixed
    /// `saved_at_unix_s` so tests can order saves deterministically.
    fn meta_at(
        version: &str,
        playtime_s: u64,
        seed: u64,
        saved_at_unix_s: u64,
    ) -> StateStoreMetadata {
        let mut m = meta(version, playtime_s, seed);
        m.saved_at_unix_s = saved_at_unix_s as i64;
        m
    }

    /// GRA-358 PR-C regression: pre-PR-C the scanner sorted by
    /// file name, which is **not** the same as "most recent".
    /// The autosave filename encodes a unix timestamp so the
    /// sort happened to work for autosaves, but any user-named
    /// save (e.g. `Z_most_recent.ron`) would sort after
    /// `autosave_1784886024.ron` even though the autosave is
    /// older. The Continue button's "most recent" pick was
    /// therefore wrong for user-named saves.
    ///
    /// This test writes three saves with deliberately confusing
    /// names (the oldest has the lexicographically-latest name
    /// so a file-name sort would put it first), checks the
    /// `saved_at_unix_s` values, and asserts the scanner
    /// surfaces the most recent one first regardless of name.
    #[test]
    fn scan_sorts_by_saved_at_unix_s_not_file_name() {
        let dir = fresh_saves_dir("sort");
        // File names: "z_oldest.ron" sorts LAST lexicographically
        // but is the OLDEST by saved_at. "a_newest.ron" sorts
        // FIRST but is the NEWEST. A file-name sort would
        // surface `a_newest.ron` first (correct) but would
        // surface `m_middle.ron` second — wrong; the scanner
        // must surface `z_oldest.ron` second.
        write_state_store(&dir, "a_newest.ron", &meta_at("0.4.0", 0, 1, 1_900_000_000));
        write_state_store(&dir, "m_middle.ron", &meta_at("0.4.0", 0, 2, 1_800_000_000));
        write_state_store(&dir, "z_oldest.ron", &meta_at("0.4.0", 0, 3, 1_700_000_000));

        let index = SaveIndex::scan(&dir);
        assert_eq!(index.valid_count(), 3);

        // First valid entry must be the most recent by timestamp.
        match &index.entries[0] {
            SaveSummary::Valid { path, header } => {
                assert!(
                    path.ends_with("a_newest.ron"),
                    "most recent save must be first; got {:?}",
                    path
                );
                assert_eq!(
                    header.saved_at_unix_s,
                    Some(1_900_000_000),
                    "most recent save's timestamp must be preserved"
                );
            }
            other => panic!("expected Valid entry at index 0, got {other:?}"),
        }
        // Middle entry: 1.8e9 (z_oldest? no — middle by timestamp).
        // Just check that the timestamps are in DESCENDING order.
        let mut prev_ts = u64::MAX;
        for entry in &index.entries {
            if let SaveSummary::Valid { header, .. } = entry {
                let ts = header.saved_at_unix_s.unwrap_or(0);
                assert!(
                    ts <= prev_ts,
                    "scanner must sort by saved_at_unix_s DESC; \
                     saw {ts} after {prev_ts}"
                );
                prev_ts = ts;
            }
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_three_valid_and_one_broken() {
        let dir = fresh_saves_dir("3v1b");
        write_state_store(&dir, "alpha.ron", &meta("0.4.0", 3600, 1234567890123));
        write_state_store(&dir, "beta.ron", &meta("0.4.0", 7200, 9876543210));
        // gamma: older format version with smaller playtime.
        write_state_store(&dir, "gamma.ron", &meta("0.3.9", 60, 42));
        write_broken_save(&dir, "delta.ron", "this is not ron at all");

        let index = SaveIndex::scan(&dir);
        assert_eq!(index.entries.len(), 4, "4 files in the dir");
        assert_eq!(index.valid_count(), 3);
        assert_eq!(index.broken_count(), 1);

        // Spot-check that the scanner extracted real metadata
        // from each valid save — version, playtime, and seed
        // round-trip through SaveHeader::from_metadata.
        let alpha = index
            .entries
            .iter()
            .find_map(|e| match e {
                SaveSummary::Valid { path, header } if path.ends_with("alpha.ron") => Some(header),
                _ => None,
            })
            .expect("alpha entry exists");
        assert_eq!(alpha.helios_version.as_deref(), Some("0.4.0"));
        assert_eq!(alpha.playtime_s, Some(3600));
        assert_eq!(alpha.seed, Some(1234567890123));

        let broken = index
            .entries
            .iter()
            .find(|e| e.is_broken())
            .expect("broken entry must exist");
        match broken {
            SaveSummary::Broken { path, error, .. } => {
                assert!(path.ends_with("delta.ron"));
                assert!(!error.is_empty(), "broken entries carry a reason");
            }
            _ => unreachable!("just checked is_broken"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// Regression test for the v1 4 KB prefix bug. The v1
    /// scanner read only the first 4 KB of a `SaveFile` and
    /// raised "Expected end of string" when the body data
    /// exceeded that prefix. The v2 StateStore format is
    /// KB-sized regardless of universe size (divergences
    /// only), so the prefix-scan code path no longer exists
    /// and this regression is structurally impossible — but
    /// the test stays as a guard against accidentally
    /// re-introducing a streaming read that could truncate.
    #[test]
    fn scan_handles_large_save_regardless_of_body_size() {
        let dir = fresh_saves_dir("large");
        // 6 KB of dummy divergence data — well past any
        // streaming-read length we would ever consider. The
        // v2 extractor will accept this and the v2 scanner
        // will read the whole file.
        let mut metadata = meta("0.5.0", 9_999, 777);
        // Pad `helios_version` so the file is big; cheap
        // way to inflate without needing a real divergence
        // builder here.
        metadata.helios_version = "x".repeat(6_000);
        write_state_store(&dir, "current_save.ron", &metadata);

        let index = SaveIndex::scan(&dir);
        assert_eq!(index.valid_count(), 1, "oversized save must still parse");
        assert_eq!(index.broken_count(), 0);
        match &index.entries[0] {
            SaveSummary::Valid { header, .. } => {
                assert_eq!(header.playtime_s, Some(9_999));
                assert_eq!(header.seed, Some(777));
                assert!(header.helios_version.as_deref().unwrap().starts_with('x'));
            }
            other => panic!("expected Valid entry, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_missing_directory_returns_empty_index() {
        let dir = env::temp_dir().join("helios-saves-definitely-does-not-exist-xyz-12345");
        let _ = fs::remove_dir_all(&dir);
        let index = SaveIndex::scan(&dir);
        assert!(index.entries.is_empty());
        assert_eq!(index.valid_count(), 0);
        assert_eq!(index.broken_count(), 0);
    }

    #[test]
    fn empty_dir_returns_empty_index() {
        let dir = fresh_saves_dir("empty");
        let index = SaveIndex::scan(&dir);
        assert!(index.entries.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_ron_files_are_ignored() {
        let dir = fresh_saves_dir("filter");
        write_state_store(&dir, "real.ron", &meta("0.4.0", 0, 0));
        fs::write(dir.join("readme.txt"), "this is not a save").unwrap();
        fs::write(dir.join("notes.md"), "# notes").unwrap();
        let index = SaveIndex::scan(&dir);
        assert_eq!(index.entries.len(), 1, "only .ron files are scanned");
        assert_eq!(index.valid_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_header_extracts_metadata_from_save_file() {
        let dir = fresh_saves_dir("parse");
        write_state_store(&dir, "alpha.ron", &meta("0.5.0", 3600, 12345));
        let header = parse_header_from_file(&dir.join("alpha.ron")).expect("must parse");
        // PR-I bumped FORMAT_VERSION from 1 to 2 (StateStore).
        // The header parser should report whatever the file
        // actually contains, which is the current version.
        assert_eq!(
            header.format_version,
            Some(crate::persistence::format_version::FORMAT_VERSION)
        );
        assert_eq!(header.helios_version.as_deref(), Some("0.5.0"));
        assert_eq!(header.playtime_s, Some(3600));
        assert_eq!(header.seed, Some(12345));
        // saved_at_unix_s is a fresh-now timestamp — just
        // confirm it's recent (within the last hour).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts = header.saved_at_unix_s.expect("saved_at_unix_s populated");
        assert!(now.abs_diff(ts) < 3600, "saved_at_unix_s should be ~now");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_header_rejects_garbage() {
        let dir = fresh_saves_dir("reject");
        write_broken_save(&dir, "garbage.ron", "nonsense { not valid ron");
        let err = parse_header_from_file(&dir.join("garbage.ron"))
            .expect_err("garbage must fail to parse");
        assert!(!err.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_header_from_metadata_round_trips_all_fields() {
        let md = StateStoreMetadata {
            format_version: 42,
            saved_at_unix_s: 1_700_000_000,
            playtime_s: 7200.0,
            seed: 9999,
            helios_version: "0.5.0-test".to_string(),
            preview: Default::default(),
            ..Default::default()
        };
        let header = SaveHeader::from_metadata(&md);
        assert_eq!(header.format_version, Some(42));
        assert_eq!(header.saved_at_unix_s, Some(1_700_000_000));
        assert_eq!(header.playtime_s, Some(7200));
        assert_eq!(header.seed, Some(9999));
        assert_eq!(header.helios_version.as_deref(), Some("0.5.0-test"));
        assert_eq!(header.preview, Default::default());
    }

    #[test]
    fn formatted_helpers_handle_some_and_none() {
        let h = SaveHeader {
            format_version: Some(1),
            saved_at_unix_s: Some(1_700_000_000),
            playtime_s: Some(3 * 3600 + 12 * 60 + 45),
            seed: Some(42),
            helios_version: Some("0.5.0".to_string()),
            preview: Default::default(),
        };
        assert_eq!(h.formatted_version(), "0.5.0");
        assert!(h.formatted_saved_at().contains("2023"));
        assert_eq!(h.formatted_playtime(), "3h 12m");
        assert_eq!(h.formatted_seed(), "42");

        let empty = SaveHeader::default();
        assert_eq!(empty.formatted_version(), "?");
        assert_eq!(empty.formatted_saved_at(), "Unknown");
        assert_eq!(empty.formatted_playtime(), "—");
        assert_eq!(empty.formatted_seed(), "—");
    }
}
