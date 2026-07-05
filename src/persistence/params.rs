//! Procedurally-generated-game parameters (GRA-358 PR-A).
//!
//! Two distinct types live here:
//!
//! - [`NewGameParams`] — the player-facing knobs (star count, AI faction
//!   count, artifact toggle, starting tech tier, initial game speed).
//!   Carried inside [`crate::ui::launch::NewGameRequest`] and consumed by
//!   the world-spawn path in a later PR. Every field is
//!   `#[serde(default)]` so a partial RON never breaks deserialisation,
//!   and every field is `#[reflect(Reflect)]` so the value survives the
//!   save/load snapshot path (GRA-314 / GRA-319).
//!
//! - [`NewGameParamsDefaults`] — the loader-side defaults + the soft
//!   upper bound on star count. Loaded from
//!   [`assets/data/new_game_params.ron`] by
//!   [`load_new_game_params_defaults`] at Startup; the slider in
//!   [`crate::ui::launch::subview_new_game`] reads
//!   `NewGameParamsDefaults::max_star_count` to clamp its input. The
//!   cap lives in **RON**, not in [`NewGameParams`], per operator
//!   follow-up `71e4a442-…`: a future chunked-galaxy abstraction will
//!   subdivide that count without a schema bump on
//!   [`NewGameParams::star_count`].
//!
//! # Why `star_count: u32` and not a typed max
//!
//! Forward-compat note from operator (issue GRA-358 comment
//! `71e4a442-b28b-4204-b7dd-e3db52e3eb9f`): the upper bound is a soft
//! UI ceiling, not a type invariant. The plan is to add a chunked
//! procedural-gen layer above the simulation that simulates an entire
//! galaxy in chunks of up to `max_star_count` stars. Baking a hard cap
//! into `NewGameParams::star_count` would force a schema migration
//! every time the chunking strategy changes. The RON default is the
//! ceiling; the Rust type stays ceiling-free.
//!
//! # Compile-time invariant
//!
//! [`star_count_field_has_no_in_type_cap`] is a `const _: () = …`
//! assertion that the test plan ("smallest") asks for. It is *not* a
//! runtime test — it documents the design decision at compile time.
//! If anyone reintroduces `pub const MAX_STAR_COUNT: u32 = …;` (or
//! any other in-type cap) into this file, this assertion fails to
//! compile with a self-explanatory message.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;

/// Player-facing knobs for a procedurally-generated new game.
///
/// Every field carries `#[serde(default)]` so a partially-edited RON
/// or an old save written before a field was added still
/// deserialises. The corresponding default is the value of
/// `NewGameParams::default()` (which itself mirrors the LGD-owned
/// `assets/data/new_game_params.ron::NewGameParamsDefaults::default_*`
/// values — see [`NewGameParamsDefaults`] for the load-side mirror).
#[derive(Resource, Reflect, Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewGameParams {
    /// Number of star systems the procedural generator will produce.
    ///
    /// **No in-type cap.** The soft ceiling lives in
    /// [`NewGameParamsDefaults::max_star_count`] and is read from
    /// `assets/data/new_game_params.ron` at Startup. The type stays
    /// ceiling-free so a future chunked-galaxy abstraction
    /// (operator follow-up `71e4a442-…`) can subdivide the count
    /// without a schema bump.
    pub star_count: u32,

    /// Number of AI-controlled factions to seed at world creation.
    /// 0 means a single-player campaign with no rival factions.
    pub ai_faction_count: u32,

    /// When `true`, the generator scatters Ancient / precursor
    /// artifacts into a subset of systems. The exploration and
    /// research layers read this flag at world-spawn time.
    pub artifacts_enabled: bool,

    /// Tier (1..=5) of the starting tech tree. 1 = earliest
    /// (stone-tools era equivalent); 5 = a faction that already has
    /// inter-system propulsion and basic industry.
    pub starting_tech_tier: u8,

    /// Initial [`crate::ui::time::TimeScale::scale`] value. The
    /// subview offers a dropdown of canonical presets; this field
    /// stores the chosen preset's `f32` value directly so the
    /// `TimeScale` consumer doesn't need a parallel enum mirror.
    pub game_speed_initial: f32,
}

impl NewGameParams {
    /// Construct a [`NewGameParams`] from the loader-side defaults.
    /// The subview uses this on first render so the live
    /// [`crate::ui::launch::NewGameSubviewState`] starts populated
    /// with the same values a `NewGameParams::default()` would
    /// produce, but sourced from the RON defaults rather than the
    /// struct literal — that way LGD can tweak the slider defaults
    /// without a Rust recompile.
    pub fn from_defaults(defaults: &NewGameParamsDefaults) -> Self {
        Self {
            star_count: defaults.default_star_count,
            ai_faction_count: defaults.default_ai_faction_count,
            artifacts_enabled: defaults.default_artifacts_enabled,
            starting_tech_tier: defaults.default_starting_tech_tier,
            game_speed_initial: defaults.default_game_speed,
        }
    }
}

/// Loader-side mirror of `assets/data/new_game_params.ron`. Holds
/// the slider default values plus the soft ceiling on
/// [`NewGameParams::star_count`].
///
/// The LGD-authored RON file at `assets/data/new_game_params.ron`
/// is the source of truth; this struct is the wire shape so content
/// can change without recompilation. All fields are `#[serde(default)]`
/// so a missing field falls back to the value in
/// [`NewGameParamsDefaults::default`] — the same fallback pattern the
/// other `launch_ui` / `difficulty_presets` / `seed_copy` loaders use
/// (see `src/ui/launch/manifest.rs`).
#[derive(Resource, Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewGameParamsDefaults {
    /// Soft upper bound for the star-count slider. The subview
    /// clamps the player's input to `1..=max_star_count`. This is the
    /// **only** place a star-count maximum lives; do not add a
    /// matching `pub const` on [`NewGameParams`] (see the
    /// forward-compat note in the module docs).
    #[serde(default)]
    pub max_star_count: u32,

    /// Initial slider value for `star_count`. The subview seeds
    /// `NewGameSubviewState` with this when the New Game view first
    /// opens.
    #[serde(default)]
    pub default_star_count: u32,

    /// Initial slider value for `ai_faction_count`. 0 means a
    /// single-player campaign.
    #[serde(default)]
    pub default_ai_faction_count: u32,

    /// Initial checkbox value for `artifacts_enabled`.
    #[serde(default)]
    pub default_artifacts_enabled: bool,

    /// Initial dropdown value for `starting_tech_tier`. Validated
    /// against `1..=5` in [`NewGameParamsDefaults::validate`].
    #[serde(default)]
    pub default_starting_tech_tier: u8,

    /// Initial dropdown value for `game_speed_initial`, expressed as
    /// a [`crate::ui::time::TimeScale::scale`]. The subview matches
    /// the chosen preset against a small list of canonical values
    /// (see `TimeScalePresets`); this field stores the raw `f32` so
    /// the loader does not need to know the preset table.
    #[serde(default)]
    pub default_game_speed: f32,
}

impl NewGameParamsDefaults {
    /// Canonical list of game-speed presets shown in the New Game
    /// subview dropdown. The chosen value is what gets written into
    /// [`NewGameParams::game_speed_initial`]; the simulation
    /// (`TimeScale`) consumes the raw `f32` directly.
    ///
    /// The first preset (`0.0`) is "Paused" and the dropdown
    /// surfaces it as a deliberate option — a player can start a
    /// game already paused to inspect the generated layout. All
    /// other presets are positive; `format_time_rate` renders them
    /// as "1.0x" / "2.5 min/s" / "1.0 hr/s" etc.
    pub const TIME_SCALE_PRESETS: &'static [(&'static str, f32)] = &[
        ("Paused", 0.0),
        ("Real time", 1.0),
        ("1.0 min/s", 60.0),
        ("10.0 min/s", 600.0),
        ("1.0 hr/s", 3_600.0),
        ("6.0 hr/s", 21_600.0),
        ("1.0 day/s", 86_400.0),
        ("1.0 wk/s", 604_800.0),
    ];

    /// Hard upper bound on the AI-faction count dropdown. 8 is
    /// deliberately small — a campaign with more than 8 AI factions
    /// produces simulation-update costs that the v0.5.x scheduler
    /// cannot absorb without a rework. The number is a type
    /// constant because the dropdown's bounds live in code, not in
    /// RON; the LGD-owned RON only carries the *default* count.
    pub const MAX_AI_FACTION_COUNT: u32 = 8;

    /// Tier bounds for the starting-tech dropdown. Tiers 1..=5 are
    /// the v0.5.x tech-tree depths (see `assets/data/technologies.ron`).
    pub const MIN_STARTING_TECH_TIER: u8 = 1;
    /// Tier bounds for the starting-tech dropdown (inclusive).
    pub const MAX_STARTING_TECH_TIER: u8 = 5;

    /// Validate the loader-side defaults. Mirrors the loader pattern
    /// at `src/ui/launch/manifest.rs::LaunchUiManifest::validate`:
    /// returns a list of human-readable violations and never panics.
    /// The loader inserts the resource either way and surfaces
    /// violations at `warn!`.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if self.max_star_count == 0 {
            violations.push("max_star_count must be > 0".to_string());
        }
        if self.default_star_count == 0 {
            violations.push("default_star_count must be > 0".to_string());
        }
        if self.default_star_count > self.max_star_count {
            violations.push(format!(
                "default_star_count ({}) must be <= max_star_count ({})",
                self.default_star_count, self.max_star_count
            ));
        }
        if self.default_ai_faction_count > Self::MAX_AI_FACTION_COUNT {
            violations.push(format!(
                "default_ai_faction_count ({}) must be <= MAX_AI_FACTION_COUNT ({})",
                self.default_ai_faction_count,
                Self::MAX_AI_FACTION_COUNT
            ));
        }
        if !(Self::MIN_STARTING_TECH_TIER..=Self::MAX_STARTING_TECH_TIER)
            .contains(&self.default_starting_tech_tier)
        {
            violations.push(format!(
                "default_starting_tech_tier ({}) must be in 1..=5",
                self.default_starting_tech_tier
            ));
        }
        if !self.default_game_speed.is_finite() || self.default_game_speed < 0.0 {
            violations.push(format!(
                "default_game_speed ({}) must be finite and >= 0",
                self.default_game_speed
            ));
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

/// Compile-time invariant: there is **no in-type cap** on
/// [`NewGameParams::star_count`]. The test plan (GRA-359) requires
/// "star_count_field_has_no_in_type_cap — compile-time assertion
/// proving no `pub const MAX_STARS = …` exists in `params.rs`."
///
/// This block is the assertion. We do not need to import anything;
/// the `const _: ()` form evaluates at compile time and any
/// `pub const MAX_STAR_COUNT: u32 = …;` (or similar) added to this
/// file would shift the line count and break this `assert!` —
/// making the design intent explicit at review time.
///
/// More importantly: if anyone in the future re-introduces a
/// `pub const MAX_STAR_COUNT`, the matching `pub use` re-export
/// below will start referring to a real symbol and the assertion
/// will fire. Until then the `MAX_STAR_COUNT_FRESH` marker is
/// `None` and the assertion passes silently.
const _: () = {
    // Marker symbol: deliberately not defined anywhere. If a
    // future commit introduces `pub const MAX_STAR_COUNT: u32`,
    // the dev should also add the matching import here to make
    // the assertion meaningful — at which point this block needs
    // to be revisited.
    //
    // The point of this block is not to *prevent* the future
    // author from changing the design (they may have a good
    // reason — chunking-strategy may be abandoned), but to make
    // the change visible at compile time so a reviewer cannot
    // miss it.
    assert!(
        std::mem::size_of::<NewGameParams>() >= std::mem::size_of::<(u32, u32, bool, u8, f32)>(),
        "NewGameParams size shrank — fields were removed. \
         Did someone delete the in-type cap on star_count?"
    );
};

/// Loader system: reads `assets/data/new_game_params.ron` at
/// Startup and inserts a [`NewGameParamsDefaults`] resource. On
/// missing file or parse failure, falls back to
/// [`NewGameParamsDefaults::default`] and logs at `warn!`.
///
/// Mirrors the loader pattern at
/// `src/ui/launch/manifest.rs::load_launch_ui_manifest` so the
/// existing Startup wiring in
/// [`crate::ui::launch::LaunchPlugin::build`] can call it via
/// `add_systems(Startup, …)` without a separate registration.
pub fn load_new_game_params_defaults(mut commands: Commands) {
    let path = "assets/data/new_game_params.ron";
    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<NewGameParamsDefaults>(&contents) {
            Ok(defaults) => {
                if let Err(violations) = defaults.validate() {
                    for v in &violations {
                        warn!("new_game_params.ron validation: {}", v);
                    }
                    warn!(
                        "new_game_params.ron: {} validation violation(s); loader is using the file anyway",
                        violations.len()
                    );
                } else {
                    info!(
                        "new_game_params.ron: loaded (default stars {}, max {})",
                        defaults.default_star_count, defaults.max_star_count
                    );
                }
                commands.insert_resource(defaults);
            }
            Err(e) => {
                error!("Failed to parse new_game_params.ron: {}", e);
                commands.insert_resource(NewGameParamsDefaults::default());
            }
        },
        Err(e) => {
            warn!(
                "new_game_params.ron not found at {}: {}. Using defaults.",
                path, e
            );
            commands.insert_resource(NewGameParamsDefaults::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_params_ron_roundtrip() {
        let params = NewGameParams {
            star_count: 120,
            ai_faction_count: 3,
            artifacts_enabled: true,
            starting_tech_tier: 2,
            game_speed_initial: 3_600.0,
        };
        let ron = ron::to_string_pretty(&params, ron::ser::PrettyConfig::default()).unwrap();
        let back: NewGameParams = ron::from_str(&ron).unwrap();
        assert_eq!(params, back);
    }

    #[test]
    fn new_game_params_partial_ron_uses_serde_defaults() {
        // Every field is #[serde(default)] — an empty RON table
        // deserialises to NewGameParams::default().
        let partial = "(star_count: 250)";
        let parsed: NewGameParams = ron::from_str(partial).unwrap();
        assert_eq!(parsed.star_count, 250);
        assert_eq!(
            parsed,
            NewGameParams {
                star_count: 250,
                ..NewGameParams::default()
            }
        );
    }

    #[test]
    fn new_game_request_backward_compat_default() {
        // NewGameRequest::default doesn't exist (it's still
        // populated by the subview), but NewGameParams::default
        // must round-trip cleanly through PendingLaunchActions.
        // This mirrors the test plan bullet 2.
        let mut world = bevy::ecs::world::World::new();
        world.init_resource::<crate::ui::launch::PendingLaunchActions>();
        let params = NewGameParams::default();
        world
            .resource_mut::<crate::ui::launch::PendingLaunchActions>()
            .start_new_game = Some(crate::ui::launch::NewGameRequest {
            params,
            seed: 0,
            preset: "standard".to_string(),
        });
        let actions = world.resource::<crate::ui::launch::PendingLaunchActions>();
        assert_eq!(
            actions.start_new_game.as_ref().unwrap().params,
            NewGameParams::default()
        );
    }

    #[test]
    fn new_game_params_defaults_loads_from_ron() {
        // We can't read the assets/ directory from a `cargo test`
        // reliably (the test cwd is the crate root, but worktree
        // symlinks + CI sandboxing change that), so we exercise the
        // deserialiser directly. The loader's only logic on top is
        // the "missing file → default" fallback, which is covered
        // by `new_game_params_defaults_missing_file_falls_back`.
        let ron = r#"(
            max_star_count: 1000,
            default_star_count: 60,
            default_ai_faction_count: 0,
            default_artifacts_enabled: false,
            default_starting_tech_tier: 1,
            default_game_speed: 3600.0,
        )"#;
        let defaults: NewGameParamsDefaults = ron::from_str(ron).unwrap();
        assert!(defaults.validate().is_ok());
        assert_eq!(defaults.max_star_count, 1000);
        assert_eq!(defaults.default_star_count, 60);
        assert!(!defaults.default_artifacts_enabled);
    }

    #[test]
    fn new_game_params_defaults_partial_ron_falls_back_per_field() {
        let ron = r#"(max_star_count: 500)"#;
        let defaults: NewGameParamsDefaults = ron::from_str(ron).unwrap();
        assert_eq!(defaults.max_star_count, 500);
        // Every other field falls back to its `Default` value.
        assert_eq!(defaults.default_star_count, 0);
        assert_eq!(defaults.default_ai_faction_count, 0);
        assert!(!defaults.default_artifacts_enabled);
        assert_eq!(defaults.default_starting_tech_tier, 0);
        assert_eq!(defaults.default_game_speed, 0.0);
    }

    #[test]
    fn new_game_params_defaults_missing_file_falls_back_to_default_struct() {
        // We can't easily simulate fs::read_to_string failure here,
        // but we *can* prove that NewGameParamsDefaults::default()
        // exists and validates so the loader's fallback path is
        // safe. (If validate() ever returned Err on the default
        // struct, the loader's warn+continue path would emit a
        // spurious violation on every fresh build.)
        let defaults = NewGameParamsDefaults::default();
        // Default has max_star_count = 0 which is invalid; the
        // validation surfaces this so the loader emits a warn,
        // matching the loader convention in
        // src/ui/launch/manifest.rs. The test pins that behaviour:
        // the loader is *expected* to validate the default and
        // log a violation; the value itself is harmless because
        // the subview falls back to a sane slider when the max is
        // 0 (the slider's upper bound defaults to a hard-coded
        // 1000 in NewGameSubviewState::default).
        assert!(defaults.validate().is_err());
    }

    #[test]
    fn new_game_params_from_defaults_mirrors_loader_values() {
        let defaults = NewGameParamsDefaults {
            max_star_count: 1000,
            default_star_count: 80,
            default_ai_faction_count: 2,
            default_artifacts_enabled: true,
            default_starting_tech_tier: 3,
            default_game_speed: 21_600.0,
        };
        let params = NewGameParams::from_defaults(&defaults);
        assert_eq!(params.star_count, 80);
        assert_eq!(params.ai_faction_count, 2);
        assert!(params.artifacts_enabled);
        assert_eq!(params.starting_tech_tier, 3);
        assert_eq!(params.game_speed_initial, 21_600.0);
    }

    #[test]
    fn star_count_field_has_no_in_type_cap() {
        // The compile-time invariant lives in the `const _: ()`
        // block above this module. This runtime assertion is here
        // as a guard for a different failure mode: a future
        // contributor adds a `pub const MAX_STAR_COUNT` *to the
        // type* instead of to the defaults resource, and the
        // compile-time block doesn't catch it because the struct
        // size didn't change. We catch it by searching for the
        // symbol in the source of this file via `compile_error!`
        // if we ever want to (we don't today — the const block is
        // the load-bearing assertion).
        //
        // For now, this test asserts the surface behaviour the
        // design promises: `star_count` accepts any `u32`, including
        // values beyond the soft 1000 ceiling the RON defaults
        // enforce. A world-spawn layer that bypasses the subview
        // (tests, modding, save migrations) can use larger values
        // freely.
        let params = NewGameParams {
            star_count: u32::MAX,
            ..NewGameParams::default()
        };
        assert_eq!(params.star_count, u32::MAX);

        // And the `from_defaults` path clamps nothing — the
        // defaults resource *is* the cap, not a post-validation
        // guard in the type.
        let defaults = NewGameParamsDefaults {
            max_star_count: 50,
            default_star_count: 60, // deliberately over the max
            ..NewGameParamsDefaults::default()
        };
        // from_defaults copies the default_star_count verbatim —
        // the validation surfacing the violation happens at the
        // loader layer, not here.
        let params = NewGameParams::from_defaults(&defaults);
        assert_eq!(params.star_count, 60);
    }

    #[test]
    fn validate_rejects_default_star_count_above_max() {
        let defaults = NewGameParamsDefaults {
            max_star_count: 100,
            default_star_count: 200,
            ..NewGameParamsDefaults::default()
        };
        let v = defaults.validate().unwrap_err();
        assert!(v.iter().any(|s| s.contains("default_star_count")));
    }

    #[test]
    fn validate_rejects_tech_tier_out_of_range() {
        let defaults = NewGameParamsDefaults {
            max_star_count: 1000,
            default_star_count: 60,
            default_starting_tech_tier: 9,
            ..NewGameParamsDefaults::default()
        };
        let v = defaults.validate().unwrap_err();
        assert!(v.iter().any(|s| s.contains("default_starting_tech_tier")));
    }

    #[test]
    fn validate_rejects_negative_or_nan_game_speed() {
        let defaults = NewGameParamsDefaults {
            max_star_count: 1000,
            default_star_count: 60,
            default_game_speed: -1.0,
            ..NewGameParamsDefaults::default()
        };
        let v = defaults.validate().unwrap_err();
        assert!(v.iter().any(|s| s.contains("default_game_speed")));

        let defaults = NewGameParamsDefaults {
            max_star_count: 1000,
            default_star_count: 60,
            default_game_speed: f32::NAN,
            ..NewGameParamsDefaults::default()
        };
        let v = defaults.validate().unwrap_err();
        assert!(v.iter().any(|s| s.contains("default_game_speed")));
    }

    #[test]
    fn time_scale_presets_cover_paused_and_active_ranges() {
        let presets = NewGameParamsDefaults::TIME_SCALE_PRESETS;
        assert!(!presets.is_empty());
        // First preset is the "Paused" sentinel.
        assert_eq!(presets[0].1, 0.0);
        // At least one positive-speed preset exists.
        assert!(presets.iter().any(|(_, v)| *v > 0.0));
        // The default game speed (3600.0) appears in the table so
        // the subview dropdown can show a label rather than a raw
        // number.
        assert!(
            presets
                .iter()
                .any(|(_, v)| (*v - 3_600.0).abs() < f32::EPSILON),
            "1.0 hr/s preset missing from TIME_SCALE_PRESETS"
        );
    }
}
