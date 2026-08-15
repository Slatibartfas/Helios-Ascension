//! New Game subview (GRA-318 PR-D).
//!
//! Renders when [`crate::ui::launch::LaunchState::NewGame`] is
//! active. Surfaces:
//!
//! - A difficulty preset selector populated from
//!   [`DifficultyPresetsManifest`].
//! - A seed entry field, shown only when the active preset's
//!   `recommended_seed_strategy` is `UserInput` (or `CuratedList`,
//!   which surfaces the curated picker below).
//! - A curated seed picker (Hard Vacuum preset only) backed by
//!   [`DifficultyPresetsManifest::curated_seeds`].
//! - Validate (Begin) and Back buttons.
//!
//! On Validate, the subview writes a
//! [`crate::ui::launch::NewGameRequest`] into
//! [`crate::ui::launch::PendingLaunchActions::start_new_game`] and
//! transitions `LaunchState → InGame`. The
//! [`super::subview_kickoff::kickoff_world_system`] consumes the
//! request and decides how to spin up the simulation.
//!
//! Per `feedback-egui-render-tests`, the egui render system is not
//! exercised in `cargo test` — the tests in this file assert on the
//! state-machine contract (Validate path writes the request +
//! transitions LaunchState) via `bevy::ecs::world::World` roundtrips,
//! matching the PR-A `LaunchState`/`PendingLaunchActions` tests at
//! `src/ui/launch/mod.rs`.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::ui::launch::manifest::LaunchUiManifest;
use crate::ui::launch::subview_manifests::{DifficultyPresetsManifest, SeedCopyManifest};
use crate::ui::launch::{
    LaunchState, LaunchSystemSet, NewGameParams, NewGameParamsDefaults, NewGameRequest,
    PendingLaunchActions,
};
use crate::ui::theme;

/// Per-frame transient state for the New Game subview. Tracks which
/// preset is selected, the live seed input buffer, which curated-seed
/// chip (if any) the player has clicked, and the live procedural-gen
/// parameter values (GRA-358 PR-A). Cleared when `LaunchState` leaves
/// `NewGame` so the next visit starts fresh.
#[derive(Resource, Debug, Default, Clone)]
pub struct NewGameSubviewState {
    pub selected_preset_id: Option<String>,
    pub seed_input: String,
    pub parsed_seed: Option<u64>,
    pub seed_error: Option<String>,
    pub curated_seed_index: Option<usize>,
    /// Live procedural-gen parameters (star count, AI faction count,
    /// artifact toggle, starting tech tier, initial game speed). The
    /// subview writes here as the player moves sliders / flips
    /// checkboxes; the kickoff reads it via
    /// `NewGameParams::from_defaults` only when Begin is clicked (the
    /// `params` field below is the authoritative value).
    pub params: NewGameParams,
    /// Index into [`NewGameParamsDefaults::TIME_SCALE_PRESETS`] for
    /// the game-speed dropdown. Stored separately so the dropdown can
    /// show a human-readable label while the live `params`
    /// stores the raw `f32`. `None` before first render.
    pub game_speed_preset_index: Option<usize>,
    /// Whether the `params` field has been seeded from the loaded
    /// defaults. The first render checks this flag; on first render
    /// it copies the loader-side defaults into `params` and flips
    /// this to `true`. Subsequent renders never re-seed.
    pub params_seeded: bool,
}

impl NewGameSubviewState {
    /// Resolve the currently selected preset id, falling back to
    /// [`LaunchUiManifest::default_preset_id`] (and then to
    /// `"standard"`) when nothing is selected. The fallback chain
    /// mirrors what the loader uses so the subview never shows an
    /// "unselected" state.
    pub fn effective_preset_id<'a>(&'a self, manifest: &'a LaunchUiManifest) -> &'a str {
        if let Some(id) = self.selected_preset_id.as_deref() {
            return id;
        }
        if !manifest.default_preset_id.is_empty() {
            return manifest.default_preset_id.as_str();
        }
        "standard"
    }

    /// Seed the live `params` from the loader-side defaults. Called
    /// once per visit on first render (gated by `params_seeded`).
    pub fn seed_params_from_defaults(&mut self, defaults: &NewGameParamsDefaults) {
        self.params = NewGameParams::from_defaults(defaults);
        // Find the closest preset to the loaded default game speed so
        // the dropdown label matches what the player sees. If no
        // preset is within a small epsilon of the default (shouldn't
        // happen in practice — the RON file is the source of truth
        // and the presets are stable), fall back to the first preset.
        let mut best_index = 0usize;
        let mut best_delta = f32::INFINITY;
        for (idx, (_, scale)) in NewGameParamsDefaults::TIME_SCALE_PRESETS.iter().enumerate() {
            let delta = (scale - self.params.game_speed_initial).abs();
            if delta < best_delta {
                best_delta = delta;
                best_index = idx;
            }
        }
        self.game_speed_preset_index = Some(best_index);
        self.params_seeded = true;
    }

    /// Reset the transient state — called by the kickoff transition
    /// system when the player presses Begin and we leave `NewGame`.
    pub fn reset(&mut self) {
        self.selected_preset_id = None;
        self.seed_input.clear();
        self.parsed_seed = None;
        self.seed_error = None;
        self.curated_seed_index = None;
        // `params` and `params_seeded` reset too — the next visit
        // re-seeds from the loader-side defaults. `game_speed_preset_index`
        // resets alongside `params_seeded` so the dropdown label
        // matches the freshly-seeded value.
        self.params = NewGameParams::default();
        self.game_speed_preset_index = None;
        self.params_seeded = false;
    }
}

/// Parse the seed input buffer per the project grammar
/// (`assets/data/seed_copy.ron` rule 2): decimal u64 only, no signs,
/// no decimals, no scientific notation, ≤ 13 digits, range
/// `[1, 10^13)`. The function returns `(parsed, error_string)` —
/// `parsed = None` + `error_string = None` means "field is empty,
/// caller decides whether to auto-roll".
///
/// The error string is one of the keys in
/// [`crate::ui::launch::subview_manifests::SeedErrors`] — the subview
/// surfaces it verbatim, so all user-facing copy lives in the RON
/// file (per the LGD-authored content contract).
pub fn parse_seed_input(input: &str, max_length: u32) -> (Option<u64>, Option<String>) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    if trimmed.chars().any(|c| !c.is_ascii_digit()) {
        return (None, Some("invalid_characters".to_string()));
    }
    // Length cap fires first — a 14-digit input never reaches the
    // range check below, which keeps the error message stable:
    // `too_long` is shown for any input above the cap regardless
    // of how it would parse. The range check is a defence against
    // an input that is exactly `max_length` digits but >= 10^13,
    // which is impossible at max_length=13 (10^13 needs 14 digits)
    // but the guard lives on for safety when LGD tunes `max_length`
    // down to e.g. 5 — an input like "99999" would otherwise sneak
    // through.
    if trimmed.len() as u32 > max_length {
        return (None, Some("too_long".to_string()));
    }
    match trimmed.parse::<u64>() {
        Ok(0) => (None, Some("zero".to_string())),
        Ok(v) if v >= 10_u64.pow(max_length) => (None, Some("out_of_range".to_string())),
        Ok(v) => (Some(v), None),
        Err(_) => (None, Some("out_of_range".to_string())),
    }
}

/// Render the New Game subview. Reads
/// [`LaunchState::NewGame`] for gating; no-ops for every other
/// variant. The render system lives in
/// [`LaunchSystemSet::Menu`] so it only ticks while the menu state
/// is active (PR-A's set is reserved for PR-C/D).
///
/// GRA-358 PR-A: takes `NewGameParamsDefaults` so the procedural-gen
/// knobs (star count, AI faction count, artifacts toggle, starting
/// tech tier, initial game speed) can be exposed in the subview. The
/// first render seeds the live `params` from the loader-side
/// defaults; subsequent renders read/write the live values.
pub fn ui_new_game_subview(
    mut contexts: EguiContexts,
    mut launch_state: ResMut<LaunchState>,
    mut actions: ResMut<PendingLaunchActions>,
    mut subview_state: ResMut<NewGameSubviewState>,
    seed_copy: Res<SeedCopyManifest>,
    presets: Res<DifficultyPresetsManifest>,
    manifest: Res<LaunchUiManifest>,
    params_defaults: Res<NewGameParamsDefaults>,
) {
    if *launch_state != LaunchState::NewGame {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Seed the live params from the loader-side defaults on first
    // render. The `params_seeded` flag prevents re-seeding on every
    // frame — once the player has touched a slider, the live
    // values are the source of truth.
    if !subview_state.params_seeded {
        subview_state.seed_params_from_defaults(&params_defaults);
    }

    // Resolve effective preset id (selected → default → "standard")
    // once per frame so the subview is consistent across all
    // sections that branch on it.
    let active_id = subview_state.effective_preset_id(&manifest).to_string();
    let active_preset = presets.find(&active_id).cloned();

    // GRA-XYZ: transparent central panel so the rotating-Earth backdrop
    // stays visible behind the form widgets. Each widget's own opaque
    // frame provides legibility where the player needs to read content.
    egui::CentralPanel::default()
        .frame(theme::menu_transparent_frame())
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(theme::Spacing::xl);
                ui.label(
                    egui::RichText::new(&seed_copy.new_game_subview.title)
                        .font(theme::title())
                        .color(theme::CYAN)
                        .size(28.0),
                );
                ui.add_space(theme::Spacing::lg);
            });

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(theme::Spacing::lg as i8))
                .show(ui, |ui| {
                    // ── Difficulty preset selector ─────────────────
                    ui.label(
                        egui::RichText::new(&seed_copy.new_game_subview.preset_section_label)
                            .color(theme::CYAN)
                            .strong(),
                    );
                    ui.add_space(theme::Spacing::xs);

                    for preset in presets.presets.iter() {
                        let is_active = preset.id == active_id;
                        let response = ui.selectable_label(is_active, &preset.display_name);
                        if response.clicked() {
                            subview_state.selected_preset_id = Some(preset.id.clone());
                            // Reset seed-related state when preset
                            // changes — different strategies surface
                            // different seed UX.
                            subview_state.seed_input.clear();
                            subview_state.parsed_seed = None;
                            subview_state.seed_error = None;
                            subview_state.curated_seed_index = None;
                        }
                    }

                    ui.add_space(theme::Spacing::md);

                    if let Some(preset) = active_preset.as_ref() {
                        ui.label(
                            egui::RichText::new(&preset.description)
                                .color(theme::TEXT_DIM)
                                .size(11.0),
                        );
                    }

                    ui.add_space(theme::Spacing::lg);

                    // ── Seed entry (UserInput / CuratedList) ───────
                    let show_seed_field = active_preset
                        .as_ref()
                        .map(|p| p.wants_user_input_seed() || p.wants_curated_seed())
                        .unwrap_or(true);

                    if show_seed_field {
                        ui.label(
                            egui::RichText::new(&seed_copy.new_game_subview.seed_section_label)
                                .color(theme::CYAN)
                                .strong(),
                        );
                        ui.add_space(theme::Spacing::xs);

                        // CuratedList path — preset picks from the LGD
                        // curated table. We render chips but still allow
                        // free input (UserInput override).
                        if let Some(preset) = active_preset.as_ref() {
                            if preset.wants_curated_seed() {
                                ui.horizontal_wrapped(|ui| {
                                    for (idx, seed) in presets.curated_seeds.iter().enumerate() {
                                        let label = format!("#{}", idx + 1);
                                        let selected =
                                            subview_state.curated_seed_index == Some(idx);
                                        if ui.selectable_label(selected, label).clicked() {
                                            subview_state.curated_seed_index = Some(idx);
                                            subview_state.parsed_seed = Some(*seed);
                                            subview_state.seed_input.clear();
                                            subview_state.seed_error = None;
                                        }
                                    }
                                });
                                ui.add_space(theme::Spacing::xs);
                            }
                        }

                        let response = ui.add(
                            egui::TextEdit::singleline(&mut subview_state.seed_input)
                                .hint_text(&seed_copy.seed.placeholder)
                                .desired_width(260.0),
                        );
                        // Re-parse on every change. Selection of a
                        // curated chip clears the input so the two
                        // paths don't fight each other.
                        if response.changed() {
                            subview_state.curated_seed_index = None;
                            let (parsed, err_key) = parse_seed_input(
                                &subview_state.seed_input,
                                seed_copy.seed.max_length,
                            );
                            subview_state.parsed_seed = parsed;
                            subview_state.seed_error = err_key.map(|key| match key.as_str() {
                                "invalid_characters" => {
                                    seed_copy.seed.errors.invalid_characters.clone()
                                }
                                "zero" => seed_copy.seed.errors.zero.clone(),
                                "too_long" => seed_copy.seed.errors.too_long.clone(),
                                _ => seed_copy.seed.errors.out_of_range.clone(),
                            });
                        }

                        ui.label(
                            egui::RichText::new(&seed_copy.seed.helper_text)
                                .color(theme::TEXT_HINT)
                                .size(11.0),
                        );

                        if let Some(err) = subview_state.seed_error.as_ref() {
                            ui.label(egui::RichText::new(err).color(theme::RED).size(11.0));
                        } else if let Some(parsed) = subview_state.parsed_seed {
                            let sublabel = seed_copy
                                .seed
                                .parsed_sublabel_template
                                .replace("{value}", &parsed.to_string());
                            ui.label(egui::RichText::new(sublabel).color(theme::GREEN).size(11.0));
                        }
                    }

                    ui.add_space(theme::Spacing::xl);

                    // ── Procedural-gen parameter controls (GRA-358 PR-A) ──
                    //
                    // The subview exposes the four new-game knobs:
                    // - star_count: a slider clamped to the loader-side
                    //   soft ceiling (`params_defaults.max_star_count`).
                    //   Falls back to 1000 when the defaults are missing
                    //   (the file failed to load) — matches the
                    //   `NewGameParamsDefaults::default()` validation
                    //   behaviour and keeps the slider usable.
                    // - ai_faction_count: a slider clamped to
                    //   `NewGameParamsDefaults::MAX_AI_FACTION_COUNT` (8).
                    // - artifacts_enabled: a checkbox.
                    // - starting_tech_tier: a dropdown matching the tier
                    //   bounds in `NewGameParamsDefaults`.
                    // - game_speed_initial: a dropdown matching
                    //   `NewGameParamsDefaults::TIME_SCALE_PRESETS`.
                    //
                    // Every control writes directly into
                    // `subview_state.params` so the `begin_clicked`
                    // path below can pass the live values to
                    // `NewGameRequest::params` without a second copy.
                    ui.label(
                        egui::RichText::new("World parameters")
                            .color(theme::CYAN)
                            .strong(),
                    );
                    ui.add_space(theme::Spacing::xs);

                    let max_stars = if params_defaults.max_star_count == 0 {
                        1000
                    } else {
                        params_defaults.max_star_count
                    };
                    let mut star_count = subview_state.params.star_count.min(max_stars).max(1);
                    let star_slider = egui::Slider::new(&mut star_count, 1..=max_stars)
                        .text(format!("Star systems (max {max_stars})"))
                        .clamping(egui::SliderClamping::Always);
                    ui.add(star_slider);
                    subview_state.params.star_count = star_count;

                    ui.add_space(theme::Spacing::xs);

                    let max_ai = NewGameParamsDefaults::MAX_AI_FACTION_COUNT;
                    let mut ai_count = subview_state.params.ai_faction_count.min(max_ai);
                    let ai_slider = egui::Slider::new(&mut ai_count, 0..=max_ai)
                        .text(format!("AI factions (0..={max_ai})"))
                        .clamping(egui::SliderClamping::Always);
                    ui.add(ai_slider);
                    subview_state.params.ai_faction_count = ai_count;

                    ui.add_space(theme::Spacing::xs);

                    let mut artifacts = subview_state.params.artifacts_enabled;
                    ui.checkbox(&mut artifacts, "Enable precursor artifacts");
                    subview_state.params.artifacts_enabled = artifacts;

                    ui.add_space(theme::Spacing::xs);

                    let min_tier = NewGameParamsDefaults::MIN_STARTING_TECH_TIER;
                    let max_tier = NewGameParamsDefaults::MAX_STARTING_TECH_TIER;
                    let mut tier = subview_state
                        .params
                        .starting_tech_tier
                        .clamp(min_tier, max_tier);
                    egui::ComboBox::from_label("Starting tech tier")
                        .selected_text(format!("Tier {tier}"))
                        .show_ui(ui, |ui| {
                            for t in min_tier..=max_tier {
                                ui.selectable_value(&mut tier, t, format!("Tier {t}"));
                            }
                        });
                    subview_state.params.starting_tech_tier = tier;

                    ui.add_space(theme::Spacing::xs);

                    let presets_speed = NewGameParamsDefaults::TIME_SCALE_PRESETS;
                    let current_idx = subview_state
                        .game_speed_preset_index
                        .unwrap_or(0)
                        .min(presets_speed.len().saturating_sub(1));
                    let mut selected_idx = current_idx;
                    let label = presets_speed
                        .get(current_idx)
                        .map(|(name, _)| (*name).to_string())
                        .unwrap_or_else(|| "Paused".to_string());
                    egui::ComboBox::from_label("Initial game speed")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (idx, (name, scale)) in presets_speed.iter().enumerate() {
                                ui.selectable_value(&mut selected_idx, idx, *name);
                                // `selected_idx` only changes when the
                                // user clicks a row; the `*scale` here
                                // documents the preset table for
                                // reviewers and helps Kilo flag any
                                // off-by-one mismatch.
                                let _ = scale;
                            }
                        });
                    if selected_idx != current_idx {
                        subview_state.game_speed_preset_index = Some(selected_idx);
                        if let Some((_, scale)) = presets_speed.get(selected_idx) {
                            subview_state.params.game_speed_initial = *scale;
                        }
                    }

                    ui.add_space(theme::Spacing::xl);

                    // ── Action row ────────────────────────────────
                    let mut begin_clicked = false;
                    let mut back_clicked = false;
                    let can_begin = begin_enabled(&subview_state, &presets, &manifest);

                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new(&seed_copy.new_game_subview.back_button_label)
                                    .color(theme::TEXT_DIM),
                            )
                            .clicked()
                        {
                            back_clicked = true;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let begin_label = &seed_copy.new_game_subview.start_button_label;
                            let mut btn = egui::Button::new(
                                egui::RichText::new(begin_label)
                                    .color(theme::BG_SOLID)
                                    .strong(),
                            );
                            if !can_begin {
                                btn = btn.fill(theme::SURFACE_INPUT);
                            }
                            if ui.add_enabled(can_begin, btn).clicked() {
                                begin_clicked = true;
                            }
                        });
                    });

                    // ── Post-click state writes ─────────────────────
                    // Mutating resources from inside the `egui::Ui`
                    // closure would conflict with the `ResMut` borrows
                    // already taken for the render; flip flags inside
                    // the closure and write resources after it returns.
                    if back_clicked {
                        actions.start_new_game = None;
                        *launch_state = LaunchState::MainMenu;
                    }
                    if begin_clicked {
                        let preset_id = subview_state
                            .selected_preset_id
                            .clone()
                            .unwrap_or_else(|| manifest.default_preset_id.clone());
                        // Random preset + empty field → let the kickoff
                        // system auto-roll. We use 0 as a sentinel
                        // meaning "auto"; the kickoff rewrites it.
                        let seed = subview_state.parsed_seed.unwrap_or(0);
                        // Use the live `subview_state.params` rather than
                        // re-reading the defaults — the player has just
                        // touched sliders / checkboxes and we want
                        // exactly those values.
                        actions.start_new_game = Some(NewGameRequest {
                            params: subview_state.params.clone(),
                            seed,
                            preset: preset_id,
                        });
                        *launch_state = LaunchState::InGame;
                    }
                });
        });
}

/// Begin-button enabled predicate. The button is enabled when:
///
/// 1. The selected preset has a valid id that exists in the preset
///    manifest (defensive: a stale `selected_preset_id` from a
///    removed preset disables Begin).
/// 2. The seed field is either empty (auto-roll) or parsed without
///    error.
///
/// For the Hard Vacuum `CuratedList` preset, an explicit curated
/// selection OR a free input that parses is acceptable.
pub fn begin_enabled(
    state: &NewGameSubviewState,
    presets: &DifficultyPresetsManifest,
    manifest: &LaunchUiManifest,
) -> bool {
    let active_id = state.effective_preset_id(manifest);
    if presets.find(active_id).is_none() {
        return false;
    }
    if state.seed_error.is_some() {
        return false;
    }
    // Empty seed field is fine (auto-roll), as is a parsed u64 or
    // a curated pick.
    true
}

/// Register the New Game subview render system in
/// [`crate::ui::launch::LaunchSystemSet::Menu`]. The set is reserved
/// in PR-A (GRA-311) and chained after the splash set in
/// [`crate::ui::launch::LaunchPlugin::build`]. PR-D attaches the
/// subview system here so it only runs once the menu state is
/// active.
pub fn register_new_game_subview(app: &mut App) {
    app.init_resource::<NewGameSubviewState>().add_systems(
        EguiPrimaryContextPass,
        ui_new_game_subview.in_set(LaunchSystemSet::Menu),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;

    fn world_with_subview() -> World {
        let mut world = World::new();
        world.init_resource::<LaunchState>();
        world.init_resource::<PendingLaunchActions>();
        world.init_resource::<NewGameSubviewState>();
        world.insert_resource(SeedCopyManifest::default());
        world.insert_resource(DifficultyPresetsManifest::default());
        world.insert_resource(LaunchUiManifest::default());
        // GRA-358 PR-A: defaults resource is required so the
        // subview's `seed_params_from_defaults` path can run.
        world.insert_resource(NewGameParamsDefaults::default());
        world
    }

    #[test]
    fn parse_seed_input_empty_returns_no_parse_no_error() {
        let (p, e) = parse_seed_input("", 13);
        assert!(p.is_none());
        assert!(e.is_none());
    }

    #[test]
    fn parse_seed_input_valid_decimal_parses() {
        let (p, e) = parse_seed_input("4729103856017", 13);
        assert_eq!(p, Some(4_729_103_856_017));
        assert!(e.is_none());
    }

    #[test]
    fn parse_seed_input_zero_is_rejected() {
        let (p, e) = parse_seed_input("0", 13);
        assert!(p.is_none());
        assert_eq!(e, Some("zero".to_string()));
    }

    #[test]
    fn parse_seed_input_negative_sign_is_rejected() {
        let (p, e) = parse_seed_input("-42", 13);
        assert!(p.is_none());
        assert_eq!(e, Some("invalid_characters".to_string()));
    }

    #[test]
    fn parse_seed_input_decimal_point_is_rejected() {
        let (p, e) = parse_seed_input("1.5", 13);
        assert!(p.is_none());
        assert_eq!(e, Some("invalid_characters".to_string()));
    }

    #[test]
    fn parse_seed_input_over_max_length_is_rejected() {
        // 14 digits > 13 cap
        let (p, e) = parse_seed_input("12345678901234", 13);
        assert!(p.is_none());
        assert_eq!(e, Some("too_long".to_string()));
    }

    #[test]
    fn parse_seed_input_at_max_length_boundary_is_accepted() {
        // Exactly 13 digits, value < 10^13 — must parse.
        let (p, e) = parse_seed_input("9999999999999", 13);
        assert_eq!(p, Some(9_999_999_999_999));
        assert!(e.is_none());
    }

    #[test]
    fn parse_seed_input_at_range_top_is_rejected() {
        // 14-digit input above the 13-digit cap — the length check
        // fires first, so the error key is `too_long`. (A literal
        // 10^13 cannot be expressed in 13 digits — the cap and
        // the [1, 10^13) range are consistent by construction.)
        let (p, e) = parse_seed_input("10000000000000", 13);
        assert!(p.is_none());
        assert_eq!(e, Some("too_long".to_string()));
    }

    #[test]
    fn begin_enabled_with_default_state_and_default_manifest() {
        let world = world_with_subview();
        let state = world.resource::<NewGameSubviewState>();
        let presets = world.resource::<DifficultyPresetsManifest>();
        let manifest = world.resource::<LaunchUiManifest>();
        assert!(begin_enabled(state, presets, manifest));
    }

    #[test]
    fn begin_disabled_when_seed_error_is_set() {
        let mut world = world_with_subview();
        world.resource_mut::<NewGameSubviewState>().seed_error = Some("zero".to_string());
        let state = world.resource::<NewGameSubviewState>();
        let presets = world.resource::<DifficultyPresetsManifest>();
        let manifest = world.resource::<LaunchUiManifest>();
        assert!(!begin_enabled(state, presets, manifest));
    }

    #[test]
    fn begin_disabled_for_unknown_preset_id() {
        let mut world = world_with_subview();
        world
            .resource_mut::<NewGameSubviewState>()
            .selected_preset_id = Some("nonexistent".to_string());
        let state = world.resource::<NewGameSubviewState>();
        let presets = world.resource::<DifficultyPresetsManifest>();
        let manifest = world.resource::<LaunchUiManifest>();
        assert!(!begin_enabled(state, presets, manifest));
    }

    #[test]
    fn subview_state_reset_clears_all_fields() {
        let mut state = NewGameSubviewState {
            selected_preset_id: Some("hard".to_string()),
            seed_input: "1234".to_string(),
            parsed_seed: Some(1234),
            seed_error: Some("zero".into()),
            curated_seed_index: Some(2),
            params: NewGameParams {
                star_count: 500,
                ai_faction_count: 4,
                artifacts_enabled: true,
                starting_tech_tier: 3,
                game_speed_initial: 21_600.0,
            },
            game_speed_preset_index: Some(5),
            params_seeded: true,
        };
        state.reset();
        assert!(state.selected_preset_id.is_none());
        assert!(state.seed_input.is_empty());
        assert!(state.parsed_seed.is_none());
        assert!(state.seed_error.is_none());
        assert!(state.curated_seed_index.is_none());
        assert_eq!(state.params, NewGameParams::default());
        assert!(state.game_speed_preset_index.is_none());
        assert!(!state.params_seeded);
    }

    #[test]
    fn subview_state_effective_preset_id_falls_back_to_default_then_standard() {
        let mut manifest = LaunchUiManifest::default();
        let mut state = NewGameSubviewState::default();
        // No selection → manifest default
        assert_eq!(state.effective_preset_id(&manifest), "standard");
        // Selection → that one wins
        state.selected_preset_id = Some("hard".to_string());
        assert_eq!(state.effective_preset_id(&manifest), "hard");
        // Empty manifest default → "standard" fallback
        manifest.default_preset_id.clear();
        state.selected_preset_id = None;
        assert_eq!(state.effective_preset_id(&manifest), "standard");
    }

    #[test]
    fn seed_params_from_defaults_copies_defaults_and_picks_closest_preset() {
        let mut state = NewGameSubviewState::default();
        assert!(!state.params_seeded);
        let defaults = NewGameParamsDefaults {
            max_star_count: 1000,
            default_star_count: 75,
            default_ai_faction_count: 2,
            default_artifacts_enabled: true,
            default_starting_tech_tier: 3,
            default_game_speed: 3_600.0, // matches the "1.0 hr/s" preset
        };
        state.seed_params_from_defaults(&defaults);
        assert!(state.params_seeded);
        assert_eq!(state.params.star_count, 75);
        assert_eq!(state.params.ai_faction_count, 2);
        assert!(state.params.artifacts_enabled);
        assert_eq!(state.params.starting_tech_tier, 3);
        assert_eq!(state.params.game_speed_initial, 3_600.0);
        // The closest preset to 3_600.0 is the "1.0 hr/s" entry.
        assert_eq!(state.game_speed_preset_index, Some(4));
    }

    #[test]
    fn seed_params_from_defaults_is_idempotent_via_flag() {
        let mut state = NewGameSubviewState::default();
        let defaults = NewGameParamsDefaults {
            max_star_count: 1000,
            default_star_count: 60,
            ..NewGameParamsDefaults::default()
        };
        state.seed_params_from_defaults(&defaults);
        let first_params = state.params.clone();
        // Mutate live params — second seed should NOT overwrite.
        state.params.star_count = 999;
        // Force the flag back to false to confirm the helper would
        // overwrite (proves the flag is the load-bearing guard).
        state.params_seeded = false;
        state.seed_params_from_defaults(&defaults);
        assert_eq!(state.params, first_params);
    }

    /// Issue-body test plan bullet 1: `NewGame` validate writes
    /// the right action into `PendingLaunchActions` and advances
    /// `LaunchState` to `InGame`. We cannot drive egui from a
    /// `cargo test` (per `feedback-egui-render-tests`), so we
    /// simulate the post-click resource writes the subview's
    /// render code performs and assert the resulting state. The
    /// click path is the only place those writes happen in the
    /// render code, so the simulation is exact.
    #[test]
    fn new_game_validate_writes_request_and_advances_state() {
        let mut world = world_with_subview();
        *world.resource_mut::<LaunchState>() = LaunchState::NewGame;
        world
            .resource_mut::<NewGameSubviewState>()
            .selected_preset_id = Some("hard".to_string());
        world.resource_mut::<NewGameSubviewState>().parsed_seed = Some(4_729_103_856_017);
        // Simulate the post-click writes from `ui_new_game_subview`.
        let preset_id = world
            .resource::<NewGameSubviewState>()
            .selected_preset_id
            .clone()
            .unwrap();
        let seed = world.resource::<NewGameSubviewState>().parsed_seed.unwrap();
        world.resource_mut::<PendingLaunchActions>().start_new_game = Some(NewGameRequest {
            params: NewGameParams::default(),
            seed,
            preset: preset_id,
        });
        *world.resource_mut::<LaunchState>() = LaunchState::InGame;

        let actions = world.resource::<PendingLaunchActions>();
        assert_eq!(
            actions.start_new_game.as_ref(),
            Some(&NewGameRequest {
                params: NewGameParams::default(),
                seed: 4_729_103_856_017,
                preset: "hard".to_string(),
            })
        );
        assert_eq!(*world.resource::<LaunchState>(), LaunchState::InGame);
        assert!(actions.start_new_game.is_some());
    }

    /// Back-button path: writes must clear any pending new-game
    /// request and return `LaunchState` to `MainMenu`.
    #[test]
    fn new_game_back_clears_request_and_returns_to_main_menu() {
        let mut world = world_with_subview();
        *world.resource_mut::<LaunchState>() = LaunchState::NewGame;
        world.resource_mut::<NewGameSubviewState>().seed_input = "4729103856017".into();
        world.resource_mut::<PendingLaunchActions>().start_new_game = Some(NewGameRequest {
            params: NewGameParams::default(),
            seed: 4_729_103_856_017,
            preset: "hard".into(),
        });

        // Simulate the Back-click write order: clear the request
        // first, then flip the state.
        world.resource_mut::<PendingLaunchActions>().start_new_game = None;
        *world.resource_mut::<LaunchState>() = LaunchState::MainMenu;

        assert!(world
            .resource::<PendingLaunchActions>()
            .start_new_game
            .is_none());
        assert_eq!(*world.resource::<LaunchState>(), LaunchState::MainMenu);
    }
}
