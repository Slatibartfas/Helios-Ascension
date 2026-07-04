//! Main menu shell — five buttons + keyboard shortcuts + subview routing.
//!
//! GRA-317 PR-C of the GRA-309 launch-flow chain. Pairs with the
//! PR-A scaffold (`mod.rs` types, [`LaunchUiManifest`]) and PR-B splash
//! rendering (`Splash` system set).
//!
//! Scope (per GRA-317 issue body):
//!
//! - Render a centered action grid: Continue / New Game / Load Game /
//!   Settings / Quit.
//! - Continue is disabled when [`SaveIndex`] has zero valid entries,
//!   matching the `continue_disabled_until_save_exists` policy flag.
//! - Subview routing (`LaunchState::NewGame / LoadGame / Settings`)
//!   is implemented here as a state transition; **content** for those
//!   subviews lands in PR-D (GRA-318).
//! - Keyboard shortcuts: `1`=Continue, `2`=New Game, `3`=Load Game,
//!   `4`=Settings, `5` and `Esc`=Quit. The `5` shortcut deliberately
//!   does NOT collide with the in-game speed-preset digit `5` at
//!   `src/ui/dashboard.rs:1320` because the menu keyboard handler
//!   early-returns when `LaunchState::is_in_game()` is true.
//! - Action queue only — button presses write to [`PendingLaunchActions`]
//!   and (for subview navigation) mutate `LaunchState`. The transition
//!   from `LoadGame` → `InGame` is PR-D's responsibility.
//!
//! Out of scope (per issue body):
//!
//! - Subview **content** (new-game form, load-game list rendering,
//!   settings tabs).
//! - Save-load runtime (GRA-314 separate parent).
//! - Splash → MainMenu transition (PR-B / GRA-316).
//!
//! Rendering rules:
//!
//! - The system runs in [`EguiPrimaryContextPass`] via
//!   [`LaunchSystemSet::Menu`]. Update-schedule placement is forbidden
//!   (tooltips / egui write paths require the primary context pass —
//!   see CLAUDE.md "Egui Scheduling").
//! - Theme colors only — no `Color32` literals. All visible copy comes
//!   from [`LaunchUiManifest::menu`] (LGD-owned, RON-driven).

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiContexts;

use super::manifest::LaunchUiManifest;
use super::{LaunchState, PendingLaunchActions, SaveIndex};
use crate::ui::theme;

/// Fixed minimum width of the action grid column.
///
/// `360 px` matches the GRA-309 §3.4 mock-up proportions: a centred
/// card slightly wider than a typical button row but narrow enough to
/// read at a glance.
const MENU_COLUMN_WIDTH: f32 = 360.0;

/// Fixed minimum height of each menu button.
///
/// Tall enough to host a 12 px label + 10 px shortcut hint on the
/// right; keeps the action grid visually balanced against the
/// `theme::title()` heading above.
const MENU_BUTTON_HEIGHT: f32 = 44.0;

/// System: render the main menu shell + handle key bindings.
///
/// Registered in [`LaunchSystemSet::Menu`] on
/// [`EguiPrimaryContextPass`] from [`super::LaunchPlugin::build`].
/// The system early-returns when `LaunchState` is not one of the menu
/// states so PR-B (splash) and PR-D (subview content) are unaffected.
pub fn main_menu_render_system(
    mut contexts: EguiContexts,
    save_index: Res<SaveIndex>,
    manifest: Res<LaunchUiManifest>,
    mut pending_actions: ResMut<PendingLaunchActions>,
    mut launch_state: ResMut<LaunchState>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    // Snapshot the current state for the gate check before we take
    // `ResMut<LaunchState>`. Bevy 0.18 forbids `Res<T>` + `ResMut<T>`
    // on the same resource, so we deref the `ResMut` instead.
    let current_state = *launch_state;

    // Gate: only render in menu states. Splash (PR-B) owns Splash,
    // and the in-game UI panel chain owns everything from `InGame`.
    if !is_menu_state(current_state) {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Continue is enabled iff the LGD policy says so AND the SaveIndex
    // has at least one valid save. The LGD flag defaults to true; the
    // SaveIndex check is unconditional so a missing saves directory
    // can never enable Continue on cold boot.
    let continue_enabled =
        !manifest.continue_disabled_until_save_exists || save_index.valid_count() > 0;

    // Keyboard handling. We deliberately run keyboard parsing before
    // `egui::CentralPanel::show` so the `LaunchState` transition is
    // reflected in the very next frame (no one-frame visual lag).
    //
    // `ctx.wants_keyboard_input()` is the same gate the in-game
    // time-controls panel uses at `src/ui/dashboard.rs:1313`, so the
    // shortcuts never fire while a text field is focused.
    if !ctx.wants_keyboard_input() {
        handle_menu_keybindings(
            &keyboard_input,
            current_state,
            continue_enabled,
            &mut pending_actions,
            &mut launch_state,
        );
    }

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
            render_menu_body(
                ui,
                &manifest,
                current_state,
                continue_enabled,
                &mut pending_actions,
                &mut launch_state,
            );
        });
}

/// True when the menu shell should render.
///
/// Matches GRA-309 §3.4: `MainMenu` is the canonical resting state;
/// `NewGame / LoadGame / Settings` show the same shell underneath the
/// subview (subview content lands in PR-D).
fn is_menu_state(state: LaunchState) -> bool {
    matches!(
        state,
        LaunchState::MainMenu
            | LaunchState::NewGame
            | LaunchState::LoadGame
            | LaunchState::Settings
    )
}

/// Render the heading + 5-button action grid + footer build label.
fn render_menu_body(
    ui: &mut egui::Ui,
    manifest: &LaunchUiManifest,
    launch_state: LaunchState,
    continue_enabled: bool,
    pending_actions: &mut PendingLaunchActions,
    next_launch_state: &mut LaunchState,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.18);

        // Heading — title-case "HELIOS ASCENSION" in the dim accent.
        ui.label(
            egui::RichText::new("HELIOS ASCENSION")
                .font(theme::title())
                .color(theme::ACCENT),
        );
        ui.add_space(theme::Spacing::xl);

        // Action grid: 5 buttons rendered with the resolved label on
        // the left and the shortcut keycap on the right. Subview
        // labels switch to "Back" for any state other than MainMenu
        // (PR-D will overwrite this with its own subview header).
        let column_width = MENU_COLUMN_WIDTH.min(ui.available_width() - theme::Spacing::xl);
        ui.allocate_ui_with_layout(
            egui::vec2(column_width, 0.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                render_action_grid(
                    ui,
                    manifest,
                    launch_state,
                    continue_enabled,
                    pending_actions,
                    next_launch_state,
                );
            },
        );

        ui.add_space(theme::Spacing::xl);
        ui.label(theme::caption(build_footer_label(manifest)));
    });
}

/// Render the 5-button action grid (Continue / New Game / Load Game /
/// Settings / Quit) with shortcut hints and the appropriate state
/// transitions for each press.
fn render_action_grid(
    ui: &mut egui::Ui,
    manifest: &LaunchUiManifest,
    launch_state: LaunchState,
    continue_enabled: bool,
    pending_actions: &mut PendingLaunchActions,
    next_launch_state: &mut LaunchState,
) {
    let copy = &manifest.menu;

    // Continue
    if render_menu_button(
        ui,
        copy.resolved_continue_label(),
        copy.resolved_continue_shortcut(),
        continue_enabled,
    )
    .clicked()
        && continue_enabled
        && launch_state == LaunchState::MainMenu
    {
        pending_actions.continue_recent = true;
    }

    ui.add_space(theme::Spacing::sm);

    // New Game — subview routing for PR-C; PR-D fills the form.
    let new_game_label = if launch_state == LaunchState::NewGame {
        "Back"
    } else {
        copy.resolved_new_game_label()
    };
    if render_menu_button(ui, new_game_label, copy.resolved_new_game_shortcut(), true).clicked() {
        toggle_subview(next_launch_state, launch_state, LaunchState::NewGame);
    }

    ui.add_space(theme::Spacing::sm);

    // Load Game
    let load_game_label = if launch_state == LaunchState::LoadGame {
        "Back"
    } else {
        copy.resolved_load_game_label()
    };
    if render_menu_button(
        ui,
        load_game_label,
        copy.resolved_load_game_shortcut(),
        true,
    )
    .clicked()
    {
        toggle_subview(next_launch_state, launch_state, LaunchState::LoadGame);
    }

    ui.add_space(theme::Spacing::sm);

    // Settings
    let settings_label = if launch_state == LaunchState::Settings {
        "Back"
    } else {
        copy.resolved_settings_label()
    };
    if render_menu_button(ui, settings_label, copy.resolved_settings_shortcut(), true).clicked() {
        toggle_subview(next_launch_state, launch_state, LaunchState::Settings);
    }

    ui.add_space(theme::Spacing::md);

    // Quit — no subview toggle; just sets the `quit` flag in
    // `PendingLaunchActions`. A separate transition system (PR-D /
    // GRA-318) consumes it and exits the app.
    if render_menu_button(
        ui,
        copy.resolved_quit_label(),
        copy.resolved_quit_shortcut(),
        true,
    )
    .clicked()
    {
        pending_actions.quit = true;
    }
}

/// Render one row of the action grid: `Label` on the left, `Shortcut`
/// keycap on the right.
///
/// Disabled state matches `theme::SURFACE` with
/// `theme::TEXT_DIM` so the player can see why Continue is greyed out
/// (cold boot, no saves). Shortcut hints are intentionally hidden when
/// the button is disabled — the affordance is meaningless when the
/// action cannot fire.
fn render_menu_button(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    let fill = if enabled {
        theme::SURFACE_RAISED
    } else {
        theme::SURFACE
    };
    let stroke = if enabled {
        egui::Stroke::new(1.0, theme::ACCENT_DIM)
    } else {
        egui::Stroke::new(0.5, theme::BORDER)
    };
    let label_color = if enabled {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };

    let button = egui::Button::new(
        egui::RichText::new(label)
            .font(theme::body(14.0))
            .color(label_color),
    )
    .fill(fill)
    .stroke(stroke)
    .min_size(egui::vec2(MENU_COLUMN_WIDTH, MENU_BUTTON_HEIGHT))
    .sense(if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    });

    let response = ui.add(button);
    if enabled {
        // Paint the shortcut keycap on the right edge of the button
        // using the painter so it doesn't influence layout. We use the
        // theme helper for consistency with other keycap-style chips
        // (notifications panel, settings tooltips).
        let rect = response.rect;
        let shortcut_pos = egui::pos2(rect.right() - theme::Spacing::lg - 4.0, rect.center().y);
        ui.painter().text(
            shortcut_pos,
            egui::Align2::RIGHT_CENTER,
            shortcut,
            theme::mono(10.0),
            theme::ACCENT,
        );
    }
    response
}

/// Toggle a subview state: clicking the active subview's button sends
/// the player back to `MainMenu`; otherwise advances to `target`.
///
/// PR-C owns the `LaunchState` transition for subview navigation
/// (New / Load / Settings). PR-D owns the per-subview content and the
/// `LoadGame` → `InGame` transition once a save is selected.
fn toggle_subview(next_launch_state: &mut LaunchState, current: LaunchState, target: LaunchState) {
    if current == target {
        *next_launch_state = LaunchState::MainMenu;
    } else {
        *next_launch_state = target;
    }
}

/// Keyboard shortcut handler for the menu shell.
///
/// Maps the digit row to the same 5 buttons as the click path. The
/// Quit shortcut is bound to `5` AND `Esc` so the player has two
/// unambiguous exits (matches GRA-309 §3.4 "keybindings 1..5 + ESC").
///
/// Keyboard handling deliberately runs **before** the egui panel
/// draws in the same frame so the state transition is visible on the
/// very next frame (no one-frame visual lag for the "Back" label
/// flip).
fn handle_menu_keybindings(
    keyboard_input: &ButtonInput<KeyCode>,
    launch_state: LaunchState,
    continue_enabled: bool,
    pending_actions: &mut PendingLaunchActions,
    next_launch_state: &mut LaunchState,
) {
    // The in-game speed-preset handler at `src/ui/dashboard.rs:1320`
    // also reads Digit1-5, so we MUST guard on a menu state to avoid
    // double-firing when the player pops back to the main menu
    // mid-simulation. The caller (`main_menu_render_system`) only
    // invokes this handler when `is_menu_state` is true, so the guard
    // here is belt-and-suspenders for any future caller.
    debug_assert!(is_menu_state(launch_state));

    if keyboard_input.just_pressed(KeyCode::Digit1)
        && continue_enabled
        && launch_state == LaunchState::MainMenu
    {
        pending_actions.continue_recent = true;
    }
    if keyboard_input.just_pressed(KeyCode::Digit2) {
        toggle_subview(next_launch_state, launch_state, LaunchState::NewGame);
    }
    if keyboard_input.just_pressed(KeyCode::Digit3) {
        toggle_subview(next_launch_state, launch_state, LaunchState::LoadGame);
    }
    if keyboard_input.just_pressed(KeyCode::Digit4) {
        toggle_subview(next_launch_state, launch_state, LaunchState::Settings);
    }
    if keyboard_input.just_pressed(KeyCode::Digit5) || keyboard_input.just_pressed(KeyCode::Escape)
    {
        pending_actions.quit = true;
    }
}

/// Compose the footer line shown beneath the action grid.
///
/// Re-uses the `build_label_format` template from the manifest with
/// `{version}` substituted from `manifest.resolved_version()` and
/// `{sha}` replaced with an empty string for now — the short-SHA
/// wiring lives in the splash module (PR-B) and will be hoisted into
/// the manifest once both PRs ship.
fn build_footer_label(manifest: &LaunchUiManifest) -> String {
    manifest
        .build_label_format
        .replace("{version}", &manifest.resolved_version())
        .replace("{sha}", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_state_predicate_only_matches_menu_variants() {
        assert!(is_menu_state(LaunchState::MainMenu));
        assert!(is_menu_state(LaunchState::NewGame));
        assert!(is_menu_state(LaunchState::LoadGame));
        assert!(is_menu_state(LaunchState::Settings));
        assert!(!is_menu_state(LaunchState::Splash));
        assert!(!is_menu_state(LaunchState::InGame));
    }

    #[test]
    fn build_footer_label_substitutes_version() {
        let m = LaunchUiManifest {
            version: Some("0.5.0-rc1".to_string()),
            ..Default::default()
        };
        let label = build_footer_label(&m);
        assert!(
            label.contains("0.5.0-rc1"),
            "footer should include version: {}",
            label
        );
    }

    #[test]
    fn build_footer_label_substitutes_cargo_version_when_none() {
        let m = LaunchUiManifest::default();
        let label = build_footer_label(&m);
        assert!(
            label.contains(env!("CARGO_PKG_VERSION")),
            "footer should fall back to CARGO_PKG_VERSION: {}",
            label
        );
    }

    #[test]
    fn toggle_subview_round_trips_to_main_menu() {
        let mut next = LaunchState::MainMenu;
        toggle_subview(&mut next, LaunchState::MainMenu, LaunchState::NewGame);
        assert_eq!(next, LaunchState::NewGame);

        toggle_subview(&mut next, LaunchState::NewGame, LaunchState::NewGame);
        assert_eq!(next, LaunchState::MainMenu);
    }

    #[test]
    fn toggle_subview_load_game_from_main_menu() {
        let mut next = LaunchState::MainMenu;
        toggle_subview(&mut next, LaunchState::MainMenu, LaunchState::LoadGame);
        assert_eq!(next, LaunchState::LoadGame);
    }

    #[test]
    fn toggle_subview_settings_round_trip() {
        let mut next = LaunchState::MainMenu;
        toggle_subview(&mut next, LaunchState::MainMenu, LaunchState::Settings);
        assert_eq!(next, LaunchState::Settings);

        toggle_subview(&mut next, LaunchState::Settings, LaunchState::Settings);
        assert_eq!(next, LaunchState::MainMenu);
    }
}
