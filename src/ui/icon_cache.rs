//! Hash-validated icon cache (`<userdata>/cache/resource_icons/`).
//!
//! ## Problem
//!
//! Every launch re-decodes and re-processes ~49 resource/category/
//! energy icons from 1024×1024 source PNGs (`assets/textures/ui/
//! resources/*.png`). Each icon needs a luminance key (category
//! badges, energy) or an RGB→white pass (pre-baked resource icons)
//! plus a Lanczos3 1024→64-ish downscale — ~4.2M pixels per icon.
//! The batch was a 20 s first-frame stall before the per-frame
//! budget landed (GRA regression, fixed 2026-08-05); the budget
//! removed the *stall* but every launch still redoes all the work
//! across ~24 frames.
//!
//! ## Approach
//!
//! Bake each icon **once** per source-file change, at all 7 DPI
//! texture sizes (32/48/64/96/128/192/256) + the fixed 64 px bevy_ui
//! size, into `<userdata>/cache/resource_icons/`. Write a JSON
//! manifest recording each logical key's source path, source stat
//! (`len`, `mtime_ns`), and a content hash. On the next launch,
//! validate by **stat first** (cheap) and only fall back to content
//! hashing when the stat matches but correctness matters.
//!
//! This kills three birds:
//! 1. **Cold-start processing** — second launch reads tiny cached
//!    PNGs (each 64 px ≈ a few hundred bytes) instead of reprocessing
//!    1024×1024 sources.
//! 2. **DPI-rebake stalls** — all sizes are pre-baked; switching
//!    monitors no longer triggers a full reprocess (the old code
//!    cleared every egui cache and re-decoded from 1024×1024).
//! 3. **Missing icon files** — the manifest records which logical
//!    keys have no source, so the runtime skips the disk read instead
//!    of failing every frame.
//!
//! ## Validation strategy
//!
//! `validate` is a two-tier check:
//! - **Tier 1 (stat)**: compare `(len, mtime_ns)` of every source
//!   path against the manifest. Mismatch ⇒ stale → full re-bake.
//! - **Tier 2 (content hash)**: when every stat matches, hash each
//!   source once and compare to the manifest. Catches in-place edits
//!   that preserve len+mtime (rare; editors usually bump mtime).
//!
//! The cache is keyed on `(logical_key, size_px)`. Logical keys are
//! stable strings (`"resource:Water"`, `"category:Atmospheric Gases"`,
//! `"energy"`), so a `ResourceType` enum change doesn't invalidate the
//! whole cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ui::launch::userdata::resolve_userdata_dir;

/// Relative directory (under userdata) where baked icons live.
pub const CACHE_SUBDIR: &str = "cache/resource_icons";

/// All baked texture edges. The egui side picks the nearest size ≥ the
/// DPI-derived `icon_texture_size`; the bevy_ui side uses 64.
pub const ICON_CACHE_SIZES: &[u32] = &[32, 48, 64, 96, 128, 192, 256];

/// Logical key for the energy icon (not a `ResourceType`).
pub const ENERGY_KEY: &str = "energy";

/// File name of the manifest inside the cache dir.
const MANIFEST_FILE: &str = "manifest.json";

/// Errors from the icon cache. Every variant is user-facing-safe:
/// none of them should crash the game — the caller falls back to
/// the old inline processing path.
#[derive(Debug)]
pub enum IconCacheError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for IconCacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IconCacheError::Io(e) => write!(f, "io error: {e}"),
            IconCacheError::Json(e) => write!(f, "json error: {e}"),
        }
    }
}

impl std::error::Error for IconCacheError {}

impl From<std::io::Error> for IconCacheError {
    fn from(e: std::io::Error) -> Self {
        IconCacheError::Io(e)
    }
}

impl From<serde_json::Error> for IconCacheError {
    fn from(e: serde_json::Error) -> Self {
        IconCacheError::Json(e)
    }
}

/// File stat (len + mtime) for a source path. Used as the cheap
/// invalidation signal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceStat {
    pub len: u64,
    /// Modification time in nanoseconds since UNIX_EPOCH.
    pub mtime_ns: u128,
}

impl SourceStat {
    /// Stat a path. Returns `None` when the file is missing (the
    /// caller treats a missing source as "no icon for this key").
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let mtime_ns = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some(Self {
            len: meta.len(),
            mtime_ns,
        })
    }
}

/// One manifest entry: the source identity + every baked size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconCacheEntry {
    /// Source path (absolute, as resolved at bake time).
    pub source_path: String,
    pub source_stat: SourceStat,
    /// Content hash of the source (hex, lower-case). Computed when
    /// the entry is baked; used only on the tier-2 validation path.
    pub source_hash: String,
    /// Baked output files, keyed by size px. The value is the file
    /// name relative to the cache dir.
    pub outputs: HashMap<u32, String>,
    /// True when this key had no source file at bake time (records
    /// the absence so the runtime skips the disk read every frame).
    pub missing: bool,
}

/// The on-disk manifest. Written atomically (tmp+rename) after a bake.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IconCacheManifest {
    /// Version of the cache schema. Bump to invalidate all caches
    /// (e.g. when the processing recipe changes).
    pub version: u32,
    pub entries: HashMap<String, IconCacheEntry>,
}

/// Result of validating the cache against the current sources.
#[derive(Debug, PartialEq, Eq)]
pub enum CacheValidation {
    /// Cache is fresh; no re-bake needed.
    Fresh,
    /// Sources changed or manifest missing; `needs_bake` lists the
    /// logical keys that must be re-baked. Empty when the cache dir
    /// is missing entirely (cold boot) — the caller bakes everything.
    Stale { needs_bake: Vec<String> },
}

/// Version of the cache schema. Bump when the processing recipe
/// changes (e.g. the luminance-key constants move). `pub(crate)` so
/// the bake path (`bake_keys` in `resource_icons.rs`) can stamp it
/// on freshly-written manifests — a manifest written with `Default`
/// (`0`) would otherwise fail every `validate` (the "49 stale every
/// launch" bug).
///
/// v0.5.2 PR-A.7 final: bumped to `2` after the bake recipe changed
/// (LANCZOS 1024→64 pre-bake + lo/hi 0.40/0.70 luminance key — see
/// `post_process_category_rgba` doc in `resource_icons.rs`).
/// Anything baked under `1` is stale and will be re-baked on the
/// next launch.
pub(crate) const CACHE_VERSION: u32 = 2;

/// Resolve the absolute cache directory path.
pub fn cache_dir() -> PathBuf {
    resolve_userdata_dir().join(CACHE_SUBDIR)
}

/// Sanitize a logical key into a cross-platform filename fragment.
///
/// The logical keys are `resource:Water`, `category:Atmospheric
/// Gases`, `energy`. On Windows, `:` is illegal in a filename (and
/// worse — it is the NTFS Alternate-Data-Stream separator, so
/// `File::create("resource:Water_32.png")` silently creates a file
/// named `resource` with a `Water_32.png` ADS instead of erroring).
/// The 2026-08-05 smoke test hit exactly that: the "bake" produced
/// two junk files (`resource`, `category`) and no PNGs. Replace
/// every character that is illegal in a Windows filename with `_`.
///
/// The manifest keeps the LOGICAL key; only the on-disk name is
/// sanitized (`resource_Water_32.png`).
pub fn sanitize_filename(key: &str) -> String {
    key.chars()
        .map(|c| match c {
            ':' | '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

/// Load the manifest from the cache dir, if present and parseable.
pub fn load_manifest(cache_dir: &Path) -> Result<Option<IconCacheManifest>, IconCacheError> {
    let path = cache_dir.join(MANIFEST_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(IconCacheError::Io(e)),
    };
    let manifest = serde_json::from_slice(&bytes)?;
    Ok(Some(manifest))
}

/// Save the manifest atomically (tmp+rename, same pattern as
/// `write_save_atomic` in `src/persistence/io.rs`).
pub fn save_manifest(cache_dir: &Path, manifest: &IconCacheManifest) -> Result<(), IconCacheError> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(MANIFEST_FILE);
    let tmp_path = cache_dir.join(format!("{MANIFEST_FILE}.tmp"));
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(&tmp_path, json.as_bytes())?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Validate the cache against the current sources.
///
/// Returns `Fresh` when every entry's stat (and, on stat-match, hash)
/// matches the source. Returns `Stale { needs_bake }` when the
/// manifest is missing/version-mismatched (bake everything) or when
/// specific sources changed (bake only those). Missing sources are
/// recorded in the manifest as `missing: true` and never re-baked.
///
/// `cache_dir` is currently unused — the stat/hash checks only need
/// the source map + manifest. It is kept in the signature so the
/// caller's intent (validate *this* cache dir) is explicit and a
/// future tier that checks output presence can use it.
pub fn validate(
    _cache_dir: &Path,
    manifest: &Option<IconCacheManifest>,
    sources: &HashMap<String, PathBuf>,
    content_hash: impl Fn(&Path) -> Option<String>,
) -> CacheValidation {
    let Some(manifest) = manifest.as_ref() else {
        return CacheValidation::Stale {
            needs_bake: sources.keys().cloned().collect(),
        };
    };
    if manifest.version != CACHE_VERSION {
        return CacheValidation::Stale {
            needs_bake: sources.keys().cloned().collect(),
        };
    }

    let mut needs_bake = Vec::new();
    for (key, source_path) in sources {
        match manifest.entries.get(key) {
            None => needs_bake.push(key.clone()),
            Some(entry) => {
                // Tier 1: stat compare.
                let Some(stat) = SourceStat::of(source_path) else {
                    // Source gone; if the manifest says missing, that's
                    // consistent — otherwise the source was deleted, so
                    // drop the entry.
                    if !entry.missing {
                        needs_bake.push(key.clone());
                    }
                    continue;
                };
                if entry.missing {
                    // Source appeared after a prior "missing" bake.
                    needs_bake.push(key.clone());
                    continue;
                }
                if entry.source_stat.len != stat.len || entry.source_stat.mtime_ns != stat.mtime_ns {
                    needs_bake.push(key.clone());
                    continue;
                }
                // Tier 2: content hash (only when stat matched — the
                // rare in-place edit with preserved mtime).
                if let Some(hash) = content_hash(source_path) {
                    if hash != entry.source_hash {
                        needs_bake.push(key.clone());
                    }
                }
            }
        }
    }
    if needs_bake.is_empty() {
        CacheValidation::Fresh
    } else {
        CacheValidation::Stale { needs_bake }
    }
}

/// Look up a baked output file for `key` at `size`, from a loaded
/// manifest. Returns the absolute path when the cache has it.
pub fn cached_output(
    manifest: &Option<IconCacheManifest>,
    cache_dir: &Path,
    key: &str,
    size: u32,
) -> Option<PathBuf> {
    let manifest = manifest.as_ref()?;
    let entry = manifest.entries.get(key)?;
    if entry.missing {
        return None;
    }
    // Nearest size >= requested (or the largest available).
    let chosen = entry
        .outputs
        .keys()
        .copied()
        .filter(|&s| s >= size)
        .min()
        .or_else(|| entry.outputs.keys().copied().max())?;
    Some(cache_dir.join(&entry.outputs[&chosen]))
}

/// Simple content hash — FNV-1a over the raw bytes, hex-encoded.
/// Deliberately not cryptographic: this is a cache-invalidation
/// signal, not a security boundary.
pub fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("helios-icon-cache-{tag}-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn validate_is_fresh_when_all_stats_match() {
        let dir = temp_dir("fresh");
        let src = dir.join("src.png");
        std::fs::write(&src, b"hello").unwrap();
        let stat = SourceStat::of(&src).unwrap();
        let mut manifest = IconCacheManifest {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        manifest.entries.insert(
            "resource:Water".into(),
            IconCacheEntry {
                source_path: src.display().to_string(),
                source_stat: stat.clone(),
                source_hash: fnv1a_hex(b"hello"),
                outputs: HashMap::new(),
                missing: false,
            },
        );
        let sources = HashMap::from([("resource:Water".to_string(), src.clone())]);
        let result = validate(&dir, &Some(manifest), &sources, |p| {
            std::fs::read(p).ok().map(|b| fnv1a_hex(&b))
        });
        assert_eq!(result, CacheValidation::Fresh);
    }

    #[test]
    fn validate_detects_mtime_change() {
        let dir = temp_dir("mtime");
        let src = dir.join("src.png");
        std::fs::write(&src, b"hello").unwrap();
        let stat = SourceStat::of(&src).unwrap();
        // Pretend an older stat (len differs) — stale.
        let stale_stat = SourceStat {
            len: stat.len - 1,
            mtime_ns: stat.mtime_ns - 1_000,
        };
        let mut manifest = IconCacheManifest {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        manifest.entries.insert(
            "resource:Water".into(),
            IconCacheEntry {
                source_path: src.display().to_string(),
                source_stat: stale_stat,
                source_hash: fnv1a_hex(b"hello"),
                outputs: HashMap::new(),
                missing: false,
            },
        );
        let sources = HashMap::from([("resource:Water".to_string(), src)]);
        let result = validate(&dir, &Some(manifest), &sources, |_| None);
        assert_eq!(
            result,
            CacheValidation::Stale {
                needs_bake: vec!["resource:Water".to_string()]
            }
        );
    }

    #[test]
    fn validate_detects_content_hash_change_with_same_stat() {
        let dir = temp_dir("hash");
        let src = dir.join("src.png");
        std::fs::write(&src, b"hello").unwrap();
        let stat = SourceStat::of(&src).unwrap();
        let mut manifest = IconCacheManifest {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        manifest.entries.insert(
            "resource:Water".into(),
            IconCacheEntry {
                source_path: src.display().to_string(),
                source_stat: stat,
                source_hash: fnv1a_hex(b"different-content"),
                outputs: HashMap::new(),
                missing: false,
            },
        );
        let sources = HashMap::from([("resource:Water".to_string(), src)]);
        let result = validate(&dir, &Some(manifest), &sources, |p| {
            std::fs::read(p).ok().map(|b| fnv1a_hex(&b))
        });
        assert_eq!(
            result,
            CacheValidation::Stale {
                needs_bake: vec!["resource:Water".to_string()]
            }
        );
    }

    #[test]
    fn validate_returns_all_keys_when_manifest_missing() {
        let dir = temp_dir("missing-manifest");
        let sources = HashMap::from([
            ("resource:Water".to_string(), PathBuf::from("/tmp/a.png")),
            ("energy".to_string(), PathBuf::from("/tmp/e.png")),
        ]);
        let result = validate(&dir, &None, &sources, |_| None);
        match result {
            CacheValidation::Stale { needs_bake } => {
                let mut sorted = needs_bake.clone();
                sorted.sort();
                assert_eq!(
                    sorted,
                    vec!["energy".to_string(), "resource:Water".to_string()]
                );
            }
            other => panic!("expected Stale, got {other:?}"),
        }
    }

    #[test]
    fn cached_output_picks_nearest_size_at_or_above() {
        let dir = temp_dir("nearest");
        let mut outputs = HashMap::new();
        outputs.insert(32u32, "water_32.png".to_string());
        outputs.insert(64u32, "water_64.png".to_string());
        let mut manifest = IconCacheManifest {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        manifest.entries.insert(
            "resource:Water".into(),
            IconCacheEntry {
                source_path: "/tmp/a.png".into(),
                source_stat: SourceStat { len: 10, mtime_ns: 1 },
                source_hash: "x".into(),
                outputs,
                missing: false,
            },
        );
        let got = cached_output(&Some(manifest.clone()), &dir, "resource:Water", 48);
        assert_eq!(got, Some(dir.join("water_64.png")));
        // Requesting exactly 64 → 64.
        let got = cached_output(&Some(manifest), &dir, "resource:Water", 64);
        assert_eq!(got, Some(dir.join("water_64.png")));
    }

    #[test]
    fn missing_entry_returns_none() {
        let dir = temp_dir("missing");
        let mut manifest = IconCacheManifest {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        manifest.entries.insert(
            "energy".into(),
            IconCacheEntry {
                source_path: "/tmp/e.png".into(),
                source_stat: SourceStat { len: 10, mtime_ns: 1 },
                source_hash: "x".into(),
                outputs: HashMap::new(),
                missing: true,
            },
        );
        let got = cached_output(&Some(manifest), &dir, "energy", 32);
        assert_eq!(got, None);
    }

    #[test]
    fn save_and_load_manifest_roundtrip() {
        let dir = temp_dir("roundtrip");
        let mut manifest = IconCacheManifest {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        manifest.entries.insert(
            "energy".into(),
            IconCacheEntry {
                source_path: "/tmp/e.png".into(),
                source_stat: SourceStat { len: 10, mtime_ns: 1 },
                source_hash: "abc".into(),
                outputs: HashMap::from([(32u32, "energy_32.png".to_string())]),
                missing: false,
            },
        );
        save_manifest(&dir, &manifest).unwrap();
        let loaded = load_manifest(&dir).unwrap().unwrap();
        assert_eq!(loaded.version, manifest.version);
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries["energy"].source_hash, "abc");
    }

    #[test]
    fn fnv1a_is_stable() {
        assert_eq!(fnv1a_hex(b"hello"), fnv1a_hex(b"hello"));
        assert_ne!(fnv1a_hex(b"hello"), fnv1a_hex(b"world"));
    }

    #[test]
    fn sanitize_filename_replaces_illegal_windows_chars() {
        // `:` is illegal on Windows AND the NTFS ADS separator — the
        // 2026-08-05 smoke test hit this (the bake silently wrote
        // `resource` + a `Water_32.png` stream instead of a file).
        assert_eq!(sanitize_filename("resource:Water"), "resource_Water");
        assert_eq!(
            sanitize_filename("category:Atmospheric Gases"),
            "category_Atmospheric Gases"
        );
        assert_eq!(sanitize_filename("energy"), "energy");
        // Round-trip safety: sanitized names never contain a colon.
        let f = sanitize_filename("resource:Water");
        assert!(!f.contains(':'));
    }
}
