//! Read-only save-game index for the main menu's Load Game list.
//!
//! Full save/load is a separate ticket (GRA-NNNN per GRA-309 §1.4 /
//! §8). PR-A (GRA-311) ships only the discovery layer so the menu UI
//! can list saves without lying about their existence:
//!
//! - For every `<userdata>/saves/*.ron`, read the first 4 KB and try
//!   to parse a [`SaveHeader`] struct out of it.
//! - On parse success, append a [`SaveSummary::Valid`] entry.
//! - On parse failure, append a [`SaveSummary::Broken`] entry. The
//!   menu will disable the row but the file still shows up so the
//!   player can rename / delete it.
//!
//! Empty-index behavior is the documented default: a fresh install
//! or a missing `<userdata>/saves` directory is **not** an error.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// `SAVE_HEADER_PREFIX_BYTES` — bytes to read for header parsing.
///
/// 4 KB is enough for the prefix-only schema described in GRA-309
/// §3.10 (version, saved_at, playtime_s, seed) with comfortable head-
/// room for future fields. Larger files are still scanned; we just
/// ignore anything past this offset.
pub const SAVE_HEADER_PREFIX_BYTES: usize = 4096;

/// Sub-directory of the userdata dir where save files live.
pub const SAVES_SUBDIR: &str = "saves";

/// Header prefix parsed from the first 4 KB of a save file.
///
/// Field shape is intentionally minimal — this is the **discovery**
/// header that lives at the top of every save. The full save body
/// (world state, sim snapshot, etc.) lands in a future ticket.
///
/// All fields are optional so the loader survives older saves that
/// predate the addition of a given field. A save is "valid" if its
/// header parses without error, even when fields are missing — the
/// menu just shows `Unknown` in the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveHeader {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub saved_at: Option<String>,
    #[serde(default)]
    pub playtime_s: Option<f64>,
    #[serde(default)]
    pub seed: Option<u64>,
}

/// One entry in the [`SaveIndex`].
#[derive(Debug, Clone, PartialEq)]
pub enum SaveSummary {
    Valid { path: PathBuf, header: SaveHeader },
    Broken { path: PathBuf, error: String },
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

        let mut paths: Vec<PathBuf> = read_dir
            .filter_map(|res| res.ok())
            .map(|entry| entry.path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("ron"))
            .collect();
        paths.sort();

        for path in paths {
            match parse_header_from_file(&path) {
                Ok(header) => index.entries.push(SaveSummary::Valid { path, header }),
                Err(e) => index.entries.push(SaveSummary::Broken { path, error: e }),
            }
        }

        index
    }
}

/// Parse a [`SaveHeader`] from the first 4 KB of `path`. Returns the
/// parsed header on success or an error message describing the
/// failure (suitable for [`SaveSummary::Broken`]).
pub fn parse_header_from_file(path: &Path) -> Result<SaveHeader, String> {
    let bytes = fs::read(path).map_err(|e| format!("read failed: {}", e))?;
    if bytes.is_empty() {
        return Err("file is empty".to_string());
    }
    let prefix_len = bytes.len().min(SAVE_HEADER_PREFIX_BYTES);
    let prefix = &bytes[..prefix_len];
    let text = std::str::from_utf8(prefix).map_err(|e| format!("not valid UTF-8: {}", e))?;
    ron::from_str::<SaveHeader>(text).map_err(|e| format!("RON parse failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn write_valid_save(dir: &Path, name: &str, header: &SaveHeader) -> PathBuf {
        let path = dir.join(name);
        let text = ron::ser::to_string(header).expect("serialize header");
        fs::write(&path, text).expect("write save");
        path
    }

    fn write_broken_save(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).expect("write broken save");
        path
    }

    #[test]
    fn scan_with_three_valid_and_one_broken() {
        let dir = fresh_saves_dir("3v1b");
        write_valid_save(
            &dir,
            "alpha.ron",
            &SaveHeader {
                version: Some("0.4.0".to_string()),
                saved_at: Some("2026-07-03T20:00:00Z".to_string()),
                playtime_s: Some(3600.0),
                seed: Some(1234567890123),
            },
        );
        write_valid_save(
            &dir,
            "beta.ron",
            &SaveHeader {
                version: Some("0.4.0".to_string()),
                saved_at: Some("2026-07-04T10:30:00Z".to_string()),
                playtime_s: Some(7200.0),
                seed: Some(9876543210),
            },
        );
        write_valid_save(
            &dir,
            "gamma.ron",
            &SaveHeader {
                version: Some("0.3.9".to_string()),
                saved_at: None,
                playtime_s: Some(60.0),
                seed: None,
            },
        );
        write_broken_save(&dir, "delta.ron", "this is not ron at all");

        let index = SaveIndex::scan(&dir);
        assert_eq!(index.entries.len(), 4, "4 files in the dir");
        assert_eq!(index.valid_count(), 3);
        assert_eq!(index.broken_count(), 1);

        let broken = index
            .entries
            .iter()
            .find(|e| e.is_broken())
            .expect("broken entry must exist");
        match broken {
            SaveSummary::Broken { path, error } => {
                assert!(path.ends_with("delta.ron"));
                assert!(!error.is_empty(), "broken entries carry a reason");
            }
            _ => unreachable!("just checked is_broken"),
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
        write_valid_save(
            &dir,
            "real.ron",
            &SaveHeader {
                version: Some("0.4.0".to_string()),
                saved_at: None,
                playtime_s: Some(0.0),
                seed: None,
            },
        );
        fs::write(dir.join("readme.txt"), "this is not a save").unwrap();
        fs::write(dir.join("notes.md"), "# notes").unwrap();
        let index = SaveIndex::scan(&dir);
        assert_eq!(index.entries.len(), 1, "only .ron files are scanned");
        assert_eq!(index.valid_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_header_accepts_optional_fields() {
        let dir = fresh_saves_dir("parse");
        // Empty struct — all fields None.
        write_valid_save(&dir, "empty.ron", &SaveHeader::default());
        let header = parse_header_from_file(&dir.join("empty.ron")).expect("must parse");
        assert_eq!(header, SaveHeader::default());
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
}
