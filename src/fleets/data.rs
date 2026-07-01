//! RON data loaders for fleet resources.
//!
//! Mirrors the pattern in `src/colony/data.rs` (`load_buildings`):
//! read from `assets/data/*.ron` at Startup, parse via `ron::from_str`,
//! insert the resulting `*Data` resource.  Failures log a warning and
//! fall back to a `Default::default()` resource so debug builds still run
//! — the strict pass/fail gate is the unit test in `data::tests`.

use super::components::PorkchopConfig;
use bevy::prelude::*;
use std::fs;

/// Load the porkchop plot configuration from `assets/data/porkchop_config.ron`.
///
/// On parse failure or missing file, falls back to `PorkchopConfig::default()`
/// and emits a `warn!` log.  The unit test in `data::tests` is the strict
/// pass/fail gate — production code never panics on a bad RON.
pub fn load_porkchop_config(mut commands: Commands) {
    let path = "assets/data/porkchop_config.ron";
    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<PorkchopConfig>(&contents) {
            Ok(cfg) => {
                if let Err(violations) = cfg.validate() {
                    for v in &violations {
                        warn!("porkchop_config.ron validation: {}", v);
                    }
                    warn!(
                        "porkchop_config.ron: {} validation violation(s); loader is using the file anyway",
                        violations.len()
                    );
                } else {
                    info!(
                        "porkchop_config.ron: {} category override(s), {} colormap stop(s)",
                        cfg.category_overrides.len(),
                        cfg.colormap.len()
                    );
                }
                commands.insert_resource(cfg);
            }
            Err(e) => {
                error!("Failed to parse porkchop_config.ron: {}", e);
                commands.insert_resource(PorkchopConfig::default());
            }
        },
        Err(e) => {
            warn!(
                "porkchop_config.ron not found at {}: {}. Using defaults.",
                path, e
            );
            commands.insert_resource(PorkchopConfig::default());
        }
    }
}

impl PorkchopConfig {
    /// Validate the loaded RON.  Returns `Ok(())` on success, or a list of
    /// human-readable violations on failure.  Non-fatal: the loader still
    /// inserts the resource even when violations are present.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if self.colormap.len() < 2 {
            violations.push(format!(
                "colormap must contain ≥ 2 stops (got {})",
                self.colormap.len()
            ));
        }
        if let Some(first) = self.colormap.first() {
            if first.delta_v_km_s != 0.0 {
                violations.push(format!(
                    "colormap first stop must be ΔV = 0.0 (got {})",
                    first.delta_v_km_s
                ));
            }
        }
        if self.defaults.resolution_t_dep * self.defaults.resolution_tof > 5000 {
            violations.push(format!(
                "defaults.resolution_t_dep * defaults.resolution_tof must be ≤ 5000 (got {} × {} = {})",
                self.defaults.resolution_t_dep,
                self.defaults.resolution_tof,
                self.defaults.resolution_t_dep * self.defaults.resolution_tof
            ));
        }
        for (i, ov) in self.category_overrides.iter().enumerate() {
            if ov.resolution_t_dep * ov.resolution_tof > 5000 {
                violations.push(format!(
                    "category_overrides[{}] ({}): resolution product {} × {} = {} > 5000",
                    i,
                    ov.match_key,
                    ov.resolution_t_dep,
                    ov.resolution_tof,
                    ov.resolution_t_dep * ov.resolution_tof
                ));
            }
            if ov.tof_min_hohmann_factor >= ov.tof_max_hohmann_factor {
                violations.push(format!(
                    "category_overrides[{}] ({}): tof_min ({}) must be < tof_max ({})",
                    i, ov.match_key, ov.tof_min_hohmann_factor, ov.tof_max_hohmann_factor
                ));
            }
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porkchop_config_default_validates() {
        let cfg = PorkchopConfig::default();
        assert!(
            cfg.validate().is_ok(),
            "default PorkchopConfig should validate"
        );
    }

    #[test]
    fn porkchop_config_resolve_unknown_falls_through_to_defaults() {
        let cfg = PorkchopConfig::default();
        let resolved = cfg.resolve("nonexistent_match_key");
        assert_eq!(resolved.t_dep_window_days, cfg.defaults.t_dep_window_days);
        assert_eq!(resolved.resolution_t_dep, cfg.defaults.resolution_t_dep);
        assert_eq!(resolved.resolution_tof, cfg.defaults.resolution_tof);
    }

    #[test]
    fn porkchop_config_resolve_known_uses_override() {
        let cfg = PorkchopConfig {
            category_overrides: vec![super::super::components::PorkchopCategoryOverride {
                match_key: "interplanetary".to_string(),
                t_dep_window_days: 7.0,
                tof_min_hohmann_factor: 0.5,
                tof_max_hohmann_factor: 1.5,
                tof_floor_days: 1.0,
                tof_ceiling_years: 1.0,
                resolution_t_dep: 10,
                resolution_tof: 8,
                c3_ceiling_km2_s2: 100.0,
            }],
            ..PorkchopConfig::default()
        };
        let resolved = cfg.resolve("interplanetary");
        assert_eq!(resolved.t_dep_window_days, 7.0);
        assert_eq!(resolved.resolution_t_dep, 10);
        assert_eq!(resolved.resolution_tof, 8);
    }

    #[test]
    fn porkchop_config_resolution_over_5000_fails_validation() {
        let mut cfg = PorkchopConfig::default();
        cfg.defaults.resolution_t_dep = 100;
        cfg.defaults.resolution_tof = 100;
        let v = cfg.validate().unwrap_err();
        assert!(v.iter().any(|s| s.contains("5000")));
    }

    /// Strict load of the actual `assets/data/porkchop_config.ron` file.
    /// Catches RON-side regressions (missing fields, typos, unknown
    /// values) at `cargo test` time so they don't slip into runtime as
    /// a startup-only `error!` log and a silent fall-through to
    /// `PorkchopConfig::default()`.  The loader itself stays tolerant
    /// (debug builds shouldn't panic on a bad RON), but this test is
    /// the hard gate: a broken file fails the test suite.
    #[test]
    fn porkchop_config_ron_loads_cleanly() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/data/porkchop_config.ron");
        let contents =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let cfg: PorkchopConfig = ron::from_str(&contents)
            .unwrap_or_else(|e| panic!("porkchop_config.ron failed to parse: {e}"));
        // Validation must also pass — covers the case where RON deserializes
        // but a per-override invariant (e.g. tof_min > tof_max, resolution
        // product > 5000) is violated.
        if let Err(violations) = cfg.validate() {
            panic!("porkchop_config.ron failed validation: {violations:#?}");
        }
        // Sanity: every override must define the fields the planner reads
        // at runtime.  This is implicit in the struct's serde contract
        // (no `#[serde(default)]` on those fields), but we re-assert here
        // so a future struct change that adds a default would not silently
        // hide a missing RON field — the file should always be
        // self-contained.
        for ov in &cfg.category_overrides {
            assert!(
                ov.tof_floor_days.is_finite() && ov.tof_floor_days >= 0.0,
                "override `{}` must define a finite tof_floor_days",
                ov.match_key
            );
            assert!(
                ov.tof_ceiling_years.is_finite() && ov.tof_ceiling_years > 0.0,
                "override `{}` must define a positive tof_ceiling_years",
                ov.match_key
            );
            assert!(
                ov.c3_ceiling_km2_s2.is_finite() && ov.c3_ceiling_km2_s2 > 0.0,
                "override `{}` must define a positive c3_ceiling_km2_s2",
                ov.match_key
            );
        }
    }
}
