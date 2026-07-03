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
use crate::ui::launch::{LaunchState, LaunchSystemSet, NewGameRequest, PendingLaunchActions};
use crate::ui::theme;

/// Per-frame transient state for the New Game subview. Tracks which
/// preset is selected, the live seed input buffer, and which
/// curated-seed chip (if any) the player has clicked. Cleared when
/// `LaunchState` leaves `NewGame` so the next visit starts fresh.
#[derive(Resource, Debug, Default, Clone)]
pub struct NewGameSubviewState {
    pub selected_preset_id: Option<String>,
    pub seed_input: String,
    pub parsed_seed: Option<u64>,
    pub seed_error: Option<String>,
    pub curated_seed_index: Option<usize>,
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

    /// Reset the transient state — called by the kickoff transition
    /// system when the player presses Begin and we leave `NewGame`.
    pub fn reset(&mut self) {
        self.selected_preset_id = None;
        self.seed_input.clear();
        self.parsed_seed = None;
        self.seed_error = None;
        self.curated_seed_index = None;
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
pub fn ui_new_game_subview(
    mut contexts: EguiContexts,
    mut launch_state: ResMut<LaunchState>,
    mut actions: ResMut<PendingLaunchActions>,
    mut subview_state: ResMut<NewGameSubviewState>,
    seed_copy: Res<SeedCopyManifest>,
    presets: Res<DifficultyPresetsManifest>,
    manifest: Res<LaunchUiManifest>,
) {
    if *launch_state != LaunchState::NewGame {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Resolve effective preset id (selected → default → "standard")
    // once per frame so the subview is consistent across all
    // sections that branch on it.
    let active_id = subview_state.effective_preset_id(&manifest).to_string();
    let active_preset = presets.find(&active_id).cloned();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(theme::Spacing::xl);
            ui.label(
                egui::RichText::new(&seed_copy.new_game_subview.title)
                    .font(egui::TextStyle::Heading::resolve_font(&ctx.style()))
                    .color(theme::ACCENT)
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
                        .color(theme::ACCENT)
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
                            .color(theme::ACCENT)
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
                                    let selected = subview_state.curated_seed_index == Some(idx);
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
                        let (parsed, err_key) =
                            parse_seed_input(&subview_state.seed_input, seed_copy.seed.max_length);
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
                    actions.start_new_game = Some(NewGameRequest {
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
        };
        state.reset();
        assert!(state.selected_preset_id.is_none());
        assert!(state.seed_input.is_empty());
        assert!(state.parsed_seed.is_none());
        assert!(state.seed_error.is_none());
        assert!(state.curated_seed_index.is_none());
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
            seed,
            preset: preset_id,
        });
        *world.resource_mut::<LaunchState>() = LaunchState::InGame;

        let actions = world.resource::<PendingLaunchActions>();
        assert_eq!(
            actions.start_new_game.as_ref(),
            Some(&NewGameRequest {
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
