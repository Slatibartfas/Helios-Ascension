//! Save-format version migrations.
//!
//! Migrations translate the on-disk representation of a save from one
//! [`FORMAT_VERSION`] to the next, applied in order until the save matches
//! the binary's current version. The migrators are pure functions over the
//! parsed save body — they do not touch the [`World`].
//!
//! PR-A ships only the v0→v1 placeholder; future version bumps append a new
//! `migrate_v{n}_to_v{n+1}` function and wire it into
//! [`run_migrations`]. The migrator chain is deliberately small at this
//! point because PR-A itself defines the v1 schema; there is nothing yet to
//! migrate *from*.
//!
//! When adding migrations:
//!
//! 1. Add `migrate_v{n}_to_v{n+1}` with a unit test that round-trips a
//!    hand-crafted v{n} save.
//! 2. Bump [`FORMAT_VERSION`].
//! 3. Extend `run_migrations` with the new step.

use crate::persistence::format_version::FORMAT_VERSION;

/// Top-level error returned when a save cannot be migrated forward.
///
/// Plain `String` rather than a `thiserror` enum because PR-A's migrator
/// is a thin wrapper; future PRs (PR-C autosave, etc.) may swap to a
/// typed enum once the failure modes are real.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateError {
    /// The save's `format_version` is below
    /// [`MIN_SUPPORTED_VERSION`](crate::persistence::format_version::MIN_SUPPORTED_VERSION)
    /// and we don't know how to handle it.
    TooOld { found: u32, min: u32 },

    /// The save's `format_version` is newer than this binary's
    /// [`FORMAT_VERSION`]. Likely a save produced by a newer build.
    TooNew { found: u32, current: u32 },

    /// A migrator step failed (malformed field, panic in user data, etc.).
    Step { from: u32, to: u32, reason: String },
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrateError::TooOld { found, min } => write!(
                f,
                "save format version {found} is older than minimum supported {min}"
            ),
            MigrateError::TooNew { found, current } => write!(
                f,
                "save format version {found} is newer than binary's {current}"
            ),
            MigrateError::Step { from, to, reason } => {
                write!(f, "migration from v{from} to v{to} failed: {reason}")
            }
        }
    }
}

impl std::error::Error for MigrateError {}

/// Run the migrator chain over a save whose declared
/// `format_version` is `from_version`, returning the migrated body and
/// the resulting version (always equal to [`FORMAT_VERSION`] on
/// success).
///
/// `from_version` MUST be the integer written into the save header; do
/// not infer it from the body shape.
pub fn run_migrations(from_version: u32, body: Body) -> Result<(Body, u32), MigrateError> {
    use crate::persistence::format_version::MIN_SUPPORTED_VERSION;

    if from_version < MIN_SUPPORTED_VERSION {
        return Err(MigrateError::TooOld {
            found: from_version,
            min: MIN_SUPPORTED_VERSION,
        });
    }
    if from_version > FORMAT_VERSION {
        return Err(MigrateError::TooNew {
            found: from_version,
            current: FORMAT_VERSION,
        });
    }

    let mut current = from_version;
    let mut current_body = body;

    // v0 -> v1: PR-A defines the v1 schema, so this step is a no-op
    // when migrating a hypothetical v0 save forward. No real v0 save
    // exists yet, so the migrator itself returns an error.
    if current == 0 {
        current_body = migrate_v0_to_v1(current_body)?;
        current = 1;
    }

    debug_assert_eq!(current, FORMAT_VERSION);
    Ok((current_body, current))
}

/// Stub v0 -> v1 migrator. PR-A defines the v1 schema as "the whole
/// world serialised via Bevy's `DynamicScene`", so a v0 save would be
/// whatever pre-PR-A persistence existed (`settings.ron` only). For
/// now, no v0 save format exists, so this function is unreachable in
/// practice. The function is kept to wire the migrator chain correctly
/// for the first real version bump.
fn migrate_v0_to_v1(_body: Body) -> Result<Body, MigrateError> {
    Err(MigrateError::Step {
        from: 0,
        to: 1,
        reason: "no v0 save format exists in PR-A — this migrator is unreachable".to_string(),
    })
}

/// Opaque save-body wrapper used by [`run_migrations`].
///
/// PR-A wraps a single RON-encoded Bevy [`DynamicScene`](bevy_scene::DynamicScene)
/// string in `Body::data`. Future versions may add fields (e.g.
/// `manifest_hash` for tamper-detection).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Body {
    /// The body schema version, distinct from the format version.
    /// Allows the migrator to recognise a v1 save whose body is a raw
    /// RON blob vs a future save whose body is a structured map.
    pub schema: SchemaKind,
    /// The RON-serialised payload. Stored as a string to keep the
    /// migrator independent of Bevy's internal scene format.
    pub data: String,
}

/// Marker enum describing how [`Body::data`] should be interpreted.
/// PR-A only knows [`SchemaKind::SceneRon`] — a v1 save body is a
/// RON-serialised Bevy `DynamicScene`. Future versions may add
/// structured variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SchemaKind {
    /// `data` is a RON-serialised Bevy `DynamicScene` (v1 format).
    SceneRon,
}

#[cfg(test)]
mod tests {
    use super::{Body, MigrateError, SchemaKind};

    fn dummy_body() -> Body {
        Body {
            schema: SchemaKind::SceneRon,
            data: "test".to_string(),
        }
    }

    #[test]
    fn migration_rejects_too_old() {
        let err = super::run_migrations(0, dummy_body()).expect_err("v0 must be rejected");
        assert!(matches!(err, MigrateError::TooOld { .. }));
    }

    #[test]
    fn migration_rejects_too_new() {
        let err =
            super::run_migrations(99, dummy_body()).expect_err("future version must be rejected");
        assert!(matches!(err, MigrateError::TooNew { .. }));
    }

    #[test]
    fn migration_passthrough_for_current_version() {
        let body = dummy_body();
        let (out, version) =
            super::run_migrations(super::FORMAT_VERSION, body.clone()).expect("passthrough");
        assert_eq!(version, super::FORMAT_VERSION);
        assert_eq!(out, body);
    }

    #[test]
    fn body_round_trips_via_serde_ron() {
        let body = dummy_body();
        let ron = ron::to_string(&body).expect("serialize body");
        let back: Body = ron::from_str(&ron).expect("deserialize body");
        assert_eq!(back, body);
    }
}
