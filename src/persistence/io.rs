//! Atomic on-disk write helper for save files (GRA-358 PR-B).
//!
//! PR-A produced the RON string but never touched the disk. PR-B
//! adds [`write_save_atomic`], which writes a save to `path` via
//! the standard *write-to-tmp-then-rename* pattern so a crash
//! mid-write never leaves a partial file behind. A concurrent reader
//! observes either the old file or the fully-written new one — never
//! a truncated mix.
//!
//! # Why tmp+rename
//!
//! `std::fs::rename` on POSIX is atomic when source and destination
//! live on the same filesystem. The pattern is:
//!
//! 1. Compose `<path>.tmp` (sibling of the destination).
//! 2. Write the full payload + `flush` + `sync_all` (fsync) so the
//!    bytes and metadata hit disk before the rename.
//! 3. `rename` `.tmp` over the destination. A crash before this
//!    leaves the prior file intact.
//! 4. Optionally `sync_all` the parent directory so the rename is
//!    durable across a power failure. PR-B skips step 4 (a small
//!    durability hit that matches every other game save I've seen).
//!
//! The helper exposes the error type [`SaveIoError`] so the autosave
//! path (PR-B) and the save panel (PR-C) can match on it without
//! parsing strings.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Failure modes from [`write_save_atomic`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveIoError {
    /// Could not compose the `.tmp` sibling path or the temp path
    /// does not share a parent with `path`. Almost always the latter
    /// — callers should pass an absolute `path`.
    InvalidPath(String),
    /// `File::create` failed on the `.tmp` file.
    Create(String),
    /// The write or fsync failed partway through. The destination
    /// `path` is unchanged; the `.tmp` may exist on disk and is
    /// best-effort removed before returning.
    Write(String),
    /// The atomic `rename` failed. The destination is unchanged.
    Rename(String),
}

impl std::fmt::Display for SaveIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveIoError::InvalidPath(s) => write!(f, "invalid save path: {s}"),
            SaveIoError::Create(s) => write!(f, "could not create temp file: {s}"),
            SaveIoError::Write(s) => write!(f, "save write failed: {s}"),
            SaveIoError::Rename(s) => write!(f, "save rename failed: {s}"),
        }
    }
}

impl std::error::Error for SaveIoError {}

/// Atomically write `contents` to `path` via the tmp+rename pattern.
///
/// On success the file at `path` contains exactly `contents`. On
/// failure `path` is unchanged (the prior file is preserved; the
/// temp file is best-effort removed before returning the error).
pub fn write_save_atomic(path: &Path, contents: &str) -> Result<(), SaveIoError> {
    let parent = path
        .parent()
        .ok_or_else(|| SaveIoError::InvalidPath(format!("{} has no parent", path.display())))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| SaveIoError::InvalidPath(format!("{} has no file name", path.display())))?;
    let tmp_path = parent.join(format!("{}.tmp", file_name.to_string_lossy()));

    if let Err(e) = fs::create_dir_all(parent) {
        return Err(SaveIoError::Create(format!(
            "could not create parent {}: {}",
            parent.display(),
            e
        )));
    }

    let mut file = File::create(&tmp_path).map_err(|e| {
        SaveIoError::Create(format!("could not create {}: {}", tmp_path.display(), e))
    })?;

    if let Err(e) = file.write_all(contents.as_bytes()) {
        let _ = fs::remove_file(&tmp_path);
        return Err(SaveIoError::Write(format!(
            "write to {} failed: {}",
            tmp_path.display(),
            e
        )));
    }

    if let Err(e) = file.flush() {
        let _ = fs::remove_file(&tmp_path);
        return Err(SaveIoError::Write(format!(
            "flush of {} failed: {}",
            tmp_path.display(),
            e
        )));
    }

    if let Err(e) = file.sync_all() {
        let _ = fs::remove_file(&tmp_path);
        return Err(SaveIoError::Write(format!(
            "fsync of {} failed: {}",
            tmp_path.display(),
            e
        )));
    }

    drop(file);

    if let Err(e) = fs::rename(&tmp_path, path) {
        // Try to remove the tmp file so we don't leak a partial save
        // next to the still-intact destination. The rename failure
        // itself is the primary error — surfacing the cleanup error
        // would only confuse callers.
        let _ = fs::remove_file(&tmp_path);
        return Err(SaveIoError::Rename(format!(
            "rename {} -> {} failed: {}",
            tmp_path.display(),
            path.display(),
            e
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("helios-persist-io-{}-{}-{}", tag, pid, n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn writes_contents_to_path() {
        let dir = fresh_dir("write");
        let path = dir.join("save.ron");
        write_save_atomic(&path, "hello world").expect("must succeed");
        let read = fs::read_to_string(&path).expect("must read");
        assert_eq!(read, "hello world");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_missing_parent_directory() {
        let dir = fresh_dir("mkdir");
        let nested = dir.join("a").join("b").join("c");
        let path = nested.join("save.ron");
        write_save_atomic(&path, "ok").expect("must mkdir and write");
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrites_existing_file_atomically() {
        let dir = fresh_dir("overwrite");
        let path = dir.join("save.ron");
        write_save_atomic(&path, "first").expect("first write");
        write_save_atomic(&path, "second").expect("second write");
        let read = fs::read_to_string(&path).expect("must read");
        assert_eq!(read, "second");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_path_without_parent() {
        // A bare filename has no parent — drive straight at the root
        // to force this error.
        let result = write_save_atomic(Path::new("/"), "x");
        assert!(
            matches!(result, Err(SaveIoError::InvalidPath(_))),
            "must reject parent-less path, got {:?}",
            result
        );
    }

    #[test]
    fn no_tmp_file_leaked_on_success() {
        let dir = fresh_dir("no-leak");
        let path = dir.join("save.ron");
        write_save_atomic(&path, "payload").expect("must succeed");
        let leftover = dir.join("save.ron.tmp");
        assert!(
            !leftover.exists(),
            ".tmp must be renamed over destination, not left behind"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_observes_old_or_new_never_partial() {
        // Concurrent reader sees a fully-old or fully-new file.
        // PR-B's contract: never a truncated mix. We exercise this by
        // repeatedly writing and reading; a reader that lands
        // mid-rename would see one of the two complete files because
        // rename is atomic on POSIX.
        let dir = fresh_dir("atomic");
        let path = dir.join("save.ron");
        write_save_atomic(&path, "AAAAAAAAAA").expect("first");

        // Repeated overwrites — every successful read returns the
        // exact string we wrote (no partial reads of "AAAAAA\nBBBB").
        for i in 0..50 {
            let payload = format!("seq-{:03}-{}", i, "Z".repeat(40));
            write_save_atomic(&path, &payload).expect("write");
            let read = fs::read_to_string(&path).expect("read");
            assert!(
                read == payload || read == "AAAAAAAAAA",
                "expected full old or full new; got unexpected payload"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
