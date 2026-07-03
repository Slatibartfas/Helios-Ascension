//! Save-file format version constants.
//!
//! Every save written by Helios embeds a [`FORMAT_VERSION`] constant so a
//! future binary can refuse (or migrate) older saves instead of silently
//! producing garbage on load. Versioning rules:
//!
//! - Append-only. Never mutate a past format in place — older saves must
//!   keep parsing identically.
//! - Migrations live in [`crate::persistence::migrate`]. Each version bump
//!   adds a new `v{n} -> v{n+1}` migrator.
//!
//! This file is intentionally minimal — no serde derives on `FORMAT_VERSION`
//! itself (it's a `const`, not a runtime value). The version is written into
//! every save as a literal integer inside the [`SaveMetadata`](super::snapshot::SaveMetadata)
//! struct, and the loader checks that field before deserialising the body.

/// Current save format version. Bump on any breaking change to the save body.
pub const FORMAT_VERSION: u32 = 1;

/// Oldest format version this binary can load without a migration.
///
/// `MIN_SUPPORTED_VERSION` is the floor — saves with `format_version` below
/// this are rejected with a clear error message rather than misparsed.
pub const MIN_SUPPORTED_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_constants_are_internally_consistent() {
        assert!(
            FORMAT_VERSION >= MIN_SUPPORTED_VERSION,
            "current format must satisfy the minimum-supported invariant"
        );
    }

    #[test]
    fn format_version_is_non_zero() {
        assert!(FORMAT_VERSION >= 1);
    }
}
