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
use super::{LaunchState, NewGameParams, NewGameRequest, PendingLaunchActions, SaveIndex};
use crate::plugins::sfx::bridges::UiSfxRequest;
use crate::plugins::sfx::SfxCueId;
use crate::ui::theme;

/// Shared width of every menu button in the bottom row.
///
/// Sized so that 5 buttons + 4 gaps comfortably fit a 1280 px viewport
/// (5 × 168 + 4 × 10 = 880 px) with room on either side for the
/// rotating-Earth backdrop to breathe.
const MENU_BUTTON_WIDTH: f32 = 168.0;

/// Height of every menu button. Tall enough to give the glass effect
/// visual presence without crowding the 720 px minimum-window height.
const MENU_BUTTON_HEIGHT: f32 = 56.0;

/// Gap between adjacent menu buttons in the bottom row.
const MENU_BUTTON_GAP: f32 = theme::Spacing::md;

/// Distance from the screen bottom to the bottom edge of the button row.
const MENU_ROW_BOTTOM_MARGIN: f32 = 56.0;

/// Corner radius of the glass buttons. Larger than egui's default 4 px
/// gives the buttons their rounded, liquid-glass silhouette.
const MENU_GLASS_CORNER_RADIUS: f32 = 12.0;

/// How far the button expands on hover. 1.04× reads as a subtle "pop"
/// without jarring the row layout.
const MENU_HOVER_SCALE: f32 = 0.04;

/// Time constant for the hover scale animation (seconds). Small enough
/// to feel snappy.
const MENU_HOVER_ANIM_S: f32 = 0.12;

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
    mut sfx_ui: MessageWriter<UiSfxRequest>,
) {
    // Snapshot the current state for the gate check before we take
    // `ResMut<LaunchState>`. Bevy 0.18 forbids `Res<T>` + `ResMut<T>`
    // on the same resource, so we deref the `ResMut` instead.
    let current_state = *launch_state;

    // Gate: only render the shell in `MainMenu`. Subviews
    // (`NewGame`, `LoadGame`, `Settings`, `SaveGame`) own the full
    // central-panel content for their state — drawing the menu
    // shell behind them causes `egui::CentralPanel::default()` ID
    // collisions because both panels share `Id::NULL` as their
    // parent, which manifests as duplicate widget IDs and (most
    // importantly for Save Panel) the hidden Quit button getting
    // activated when the player presses Back. Splash (PR-B) owns
    // Splash, and the in-game UI chain owns everything from
    // `InGame`.
    if current_state != LaunchState::MainMenu {
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
        .frame(theme::menu_transparent_frame())
        .show(ctx, |ui| {
            render_menu_body(
                ui,
                &manifest,
                current_state,
                continue_enabled,
                &mut pending_actions,
                &mut launch_state,
                &mut sfx_ui,
            );
        });
}

/// True when the launch flow is *not* past the menu — i.e. the
/// player is still on a shell (menu or splash) or in a subview that
/// the menu render system should keep its hands off.
///
/// Used by the keyboard-shortcut handler and the in-game chrome gate
/// to decide whether digit-row bindings + ESC are safe to consume.
/// NOTE: this is **not** used as the render-system gate any more —
/// `main_menu_render_system` early-returns strictly on `MainMenu`
/// (subviews own their full central-panel content; rendering the
/// shell behind a subview causes egui widget-ID collisions that
/// trigger spurious Quit presses).
fn is_menu_state(state: LaunchState) -> bool {
    matches!(
        state,
        LaunchState::MainMenu
            | LaunchState::NewGame
            | LaunchState::LoadGame
            | LaunchState::Settings
            | LaunchState::SaveGame
    )
}

/// Render the heading + 5-button action grid + footer build label.
///
/// Layout (GRA-XYZ redesign):
/// - Top:    `HELIOS ASCENSION` title in the cyan accent, centered.
/// - Top:    subtitle "EARTH · SECTOR SOL" in dim caption text.
/// - Middle: empty (subview content renders here when active; the
///   rotating-Earth backdrop is visible behind it).
/// - Bottom: 5 glass-style buttons in a horizontal row, centered,
///   with the hover-state pop animation and the three-layer
///   painter-driven bloom.
/// - Footer: build label, centered under the title.
fn render_menu_body(
    ui: &mut egui::Ui,
    manifest: &LaunchUiManifest,
    launch_state: LaunchState,
    continue_enabled: bool,
    pending_actions: &mut PendingLaunchActions,
    next_launch_state: &mut LaunchState,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) {
    // We use `egui::Area` for absolute positioning so the button row
    // and footer land at exact screen-relative coordinates regardless
    // of how egui's parent layout has sliced the available rect. The
    // title uses the same trick for the top-centered placement.
    let available = ui.ctx().available_rect();

    // ── Top: title + subtitle ───────────────────────────────────────
    // Default Area anchor is `Align2::LEFT_TOP`, so `fixed_pos` sets
    // the top-left corner. We compute the top-left position by
    // estimating the title's width and offsetting from `center()`.
    egui::Area::new("menu_title".into())
        .fixed_pos(egui::pos2(
            available.center().x - 100.0,
            available.top() + theme::Spacing::xl * 2.0,
        ))
        .show(ui.ctx(), |ui| {
            ui.set_min_width(200.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("HELIOS ASCENSION")
                        .font(theme::title())
                        .color(theme::CYAN),
                );
                ui.add_space(theme::Spacing::xs);
                ui.label(theme::caption("EARTH · SECTOR SOL"));
            });
        });

    // ── Bottom: 5-button glass row, anchored to the bottom margin ──
    // Row width: 5 × 168 + 4 × 10 = 880. Compute top-left from center.
    let row_width = MENU_BUTTON_WIDTH * 5.0 + MENU_BUTTON_GAP * 4.0;
    egui::Area::new("menu_button_row".into())
        .fixed_pos(egui::pos2(
            available.center().x - row_width * 0.5,
            available.bottom() - MENU_ROW_BOTTOM_MARGIN - MENU_BUTTON_HEIGHT,
        ))
        .show(ui.ctx(), |ui| {
            ui.set_min_width(row_width);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = MENU_BUTTON_GAP;
                render_action_grid(
                    ui,
                    manifest,
                    launch_state,
                    continue_enabled,
                    pending_actions,
                    next_launch_state,
                    sfx_ui,
                );
            });
        });

    // ── Footer: build label at the very bottom ──────────────────────
    egui::Area::new("menu_footer".into())
        .fixed_pos(egui::pos2(
            available.center().x - 100.0,
            available.bottom() - theme::Spacing::sm,
        ))
        .show(ui.ctx(), |ui| {
            ui.set_min_width(200.0);
            ui.label(theme::caption(build_footer_label(manifest)));
        });
}

/// Render the 5 glass buttons in a single horizontal row, centered in
/// the available rect. Each button is a [`render_glass_button`] call;
/// click routing stays byte-for-byte identical to the previous design.
fn render_action_grid(
    ui: &mut egui::Ui,
    manifest: &LaunchUiManifest,
    launch_state: LaunchState,
    continue_enabled: bool,
    pending_actions: &mut PendingLaunchActions,
    next_launch_state: &mut LaunchState,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) {
    let copy = &manifest.menu;

    // Continue
    if render_glass_button(
        ui,
        copy.resolved_continue_label(),
        copy.resolved_continue_shortcut(),
        continue_enabled,
    )
    .clicked()
        && continue_enabled
        && launch_state == LaunchState::MainMenu
    {
        sfx_ui.write(UiSfxRequest(SfxCueId::ButtonClick));
        pending_actions.continue_recent = true;
    }

    ui.add_space(MENU_BUTTON_GAP);

    // New Game
    let new_game_label = if launch_state == LaunchState::NewGame {
        "Back"
    } else {
        copy.resolved_new_game_label()
    };
    if render_glass_button(ui, new_game_label, copy.resolved_new_game_shortcut(), true).clicked() {
        sfx_ui.write(UiSfxRequest(SfxCueId::PanelOpen));
        if launch_state == LaunchState::MainMenu {
            pending_actions.start_new_game = Some(NewGameRequest {
                params: NewGameParams::default(),
                seed: 0,
                preset: "standard".to_string(),
            });
        } else {
            toggle_subview(next_launch_state, launch_state, LaunchState::NewGame);
        }
    }

    ui.add_space(MENU_BUTTON_GAP);

    // Load Game
    let load_game_label = if launch_state == LaunchState::LoadGame {
        "Back"
    } else {
        copy.resolved_load_game_label()
    };
    if render_glass_button(
        ui,
        load_game_label,
        copy.resolved_load_game_shortcut(),
        true,
    )
    .clicked()
    {
        sfx_ui.write(UiSfxRequest(SfxCueId::PanelOpen));
        toggle_subview(next_launch_state, launch_state, LaunchState::LoadGame);
    }

    ui.add_space(MENU_BUTTON_GAP);

    // Settings
    let settings_label = if launch_state == LaunchState::Settings {
        "Back"
    } else {
        copy.resolved_settings_label()
    };
    if render_glass_button(ui, settings_label, copy.resolved_settings_shortcut(), true).clicked() {
        sfx_ui.write(UiSfxRequest(SfxCueId::PanelOpen));
        toggle_subview(next_launch_state, launch_state, LaunchState::Settings);
    }

    ui.add_space(MENU_BUTTON_GAP);

    // Quit
    if render_glass_button(
        ui,
        copy.resolved_quit_label(),
        copy.resolved_quit_shortcut(),
        true,
    )
    .clicked()
    {
        sfx_ui.write(UiSfxRequest(SfxCueId::ButtonClick));
        pending_actions.quit = true;
    }
}

/// Render one liquid-glass button: `Label` on the left, `Shortcut`
/// keycap on the right.
///
/// Implementation note: we use `ui.allocate_response` + a custom painter
/// instead of `egui::Button` because the global `apply_global_visuals`
/// paints an opaque `SURFACE` background under every button — that
/// visual's bg_fill takes precedence over `Button::fill(...)` in some
/// egui versions, which is why the previous attempt produced solid
/// dark rectangles. Drawing on the foreground layer via `ctx.layer_painter`
/// with the response's id guarantees our paint composites on top of any
/// visual background.
///
/// Three visual states:
/// - **Rest:** translucent `MENU_GLASS_FILL` + soft cyan `MENU_GLASS_STROKE` border.
/// - **Hover:** brighter fill + accent border + three concentric painter
///   strokes that approximate a bloom (outer 6 px, middle 3 px, inner
///   1.5 px) + slight 1.04× expansion via egui's hover animator.
/// - **Active/pressed:** the rect expands more (1.08×) while pressed.
///
/// Disabled state uses opaque `theme::SURFACE` + `theme::BORDER` +
/// `theme::TEXT_DIM` so the player can see why Continue is greyed out
/// (cold boot, no saves). The shortcut keycap is hidden when disabled.
pub fn render_glass_button(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    // Allocate the button's rect via egui's response system (handles
    // hover, focus, click, and layout). We don't put an egui::Button
    // inside because that would also paint a visual-default background.
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );

    // ── Hover + active animation ────────────────────────────────────
    // `animate_bool_with_time` returns a 0..1 lerp for hover; we read
    // `is_pointer_button_down_on()` for the active (pressed) state.
    let hover_t = if enabled {
        ui.ctx()
            .animate_bool_with_time(response.id, response.hovered(), MENU_HOVER_ANIM_S)
    } else {
        0.0
    };
    let pressed_t = if enabled {
        ui.ctx().animate_bool_with_time(
            response.id.with("pressed"),
            response.is_pointer_button_down_on(),
            0.08,
        )
    } else {
        0.0
    };

    // Expansion combines hover pop + pressed dip.
    let expansion =
        (hover_t * MENU_HOVER_SCALE + pressed_t * MENU_HOVER_SCALE * 0.5) * rect.height() * 0.5;
    let painted_rect = rect.expand(expansion);

    // ── Painter on the foreground layer ─────────────────────────────
    // The response id gives us a stable, widget-scoped layer; painting
    // on `Order::Foreground` ensures the glass surface sits above any
    // visual-default background painted by egui.
    let layer_id = egui::LayerId::new(egui::Order::Foreground, response.id);
    let painter = ui.ctx().layer_painter(layer_id);

    if enabled {
        // ── Fill (lerp rest → hover colour) ─────────────────────────
        let fill = if hover_t > 0.0 {
            lerp_color(
                theme::MENU_GLASS_FILL,
                theme::MENU_GLASS_HOVER_FILL,
                hover_t,
            )
        } else {
            theme::MENU_GLASS_FILL
        };
        painter.rect_filled(painted_rect, MENU_GLASS_CORNER_RADIUS, fill);

        // ── Three-layer hover glow (outer → inner) ──────────────────
        if hover_t > 0.0 {
            let glow_color = |base: egui::Color32| -> egui::Color32 {
                let a = (base.a() as f32 * hover_t) as u8;
                egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), a)
            };
            // Outer halo (broadest, faintest)
            painter.rect_stroke(
                painted_rect.expand(6.0),
                MENU_GLASS_CORNER_RADIUS + 6.0,
                egui::Stroke::new(6.0_f32, glow_color(theme::MENU_GLASS_GLOW_OUTER)),
                egui::StrokeKind::Outside,
            );
            // Mid halo
            painter.rect_stroke(
                painted_rect.expand(3.0),
                MENU_GLASS_CORNER_RADIUS + 3.0,
                egui::Stroke::new(3.0_f32, glow_color(theme::MENU_GLASS_GLOW_MID)),
                egui::StrokeKind::Outside,
            );
            // Inner accent line — solid cyan, brightest
            painter.rect_stroke(
                painted_rect,
                MENU_GLASS_CORNER_RADIUS,
                egui::Stroke::new(1.5_f32, theme::CYAN),
                egui::StrokeKind::Inside,
            );
        } else {
            // Rest state: subtle border
            painter.rect_stroke(
                painted_rect,
                MENU_GLASS_CORNER_RADIUS,
                egui::Stroke::new(1.0_f32, theme::MENU_GLASS_STROKE),
                egui::StrokeKind::Inside,
            );
        }

        // ── Label (centre-left) ─────────────────────────────────────
        let label_color = if hover_t > 0.5 {
            theme::CYAN
        } else {
            theme::TEXT
        };
        let label_pos = egui::pos2(
            painted_rect.left() + theme::Spacing::lg + 4.0,
            painted_rect.center().y,
        );
        painter.text(
            label_pos,
            egui::Align2::LEFT_CENTER,
            label,
            theme::body(15.0),
            label_color,
        );

        // ── Shortcut keycap (right edge) ─────────────────────────────
        let shortcut_pos = egui::pos2(
            painted_rect.right() - theme::Spacing::lg - 4.0,
            painted_rect.center().y,
        );
        painter.text(
            shortcut_pos,
            egui::Align2::RIGHT_CENTER,
            shortcut,
            theme::mono(10.0),
            theme::CYAN,
        );
    } else {
        // Disabled: opaque dim surface so the player sees why Continue
        // is greyed out (cold boot, no saves).
        painter.rect_filled(painted_rect, MENU_GLASS_CORNER_RADIUS, theme::SURFACE);
        painter.rect_stroke(
            painted_rect,
            MENU_GLASS_CORNER_RADIUS,
            egui::Stroke::new(0.5_f32, theme::BORDER),
            egui::StrokeKind::Inside,
        );

        // Label + shortcut still rendered, in dim text + grey keycap.
        let label_pos = egui::pos2(
            painted_rect.left() + theme::Spacing::lg + 4.0,
            painted_rect.center().y,
        );
        painter.text(
            label_pos,
            egui::Align2::LEFT_CENTER,
            label,
            theme::body(15.0),
            theme::TEXT_DIM,
        );
    }

    response
}

/// Linear interpolation between two RGBA colours, channels separately.
/// `t` is clamped to `[0, 1]`. Used to animate the button fill from
/// `MENU_GLASS_FILL` to `MENU_GLASS_HOVER_FILL` on hover.
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| -> u8 {
        ((x as f32) + ((y as f32) - (x as f32)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_unmultiplied(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
        blend(a.a(), b.a()),
    )
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
    fn menu_state_predicate_matches_all_pre_in_game_states() {
        // Every menu shell / subview variant — used as the
        // keyboard-shortcut guard. `Splash` was removed when the
        // splash moved to a pre-main Bevy app
        // (`splash_standalone`); the game app boots straight
        // into `MainMenu` and never observes a Splash variant.
        assert!(is_menu_state(LaunchState::MainMenu));
        assert!(is_menu_state(LaunchState::NewGame));
        assert!(is_menu_state(LaunchState::LoadGame));
        assert!(is_menu_state(LaunchState::Settings));
        assert!(is_menu_state(LaunchState::SaveGame));
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

    /// GRA-XYZ redesign: 5 buttons + 4 gaps must fit comfortably on a
    /// 1280 px viewport so the row doesn't wrap or scroll on the
    /// minimum-supported window width (CLAUDE.md `MIN_WINDOW_WIDTH`).
    #[test]
    fn glass_button_row_fits_in_minimum_window_width() {
        let total_row_width = MENU_BUTTON_WIDTH * 5.0 + MENU_BUTTON_GAP * 4.0;
        assert!(
            total_row_width <= 1280.0,
            "row width {} exceeds 1280 px design budget",
            total_row_width
        );
    }

    /// GRA-XYZ: button row width + gap + bottom margin must leave the
    /// title + at least one screen-height of vertical room above so the
    /// Earth fills the upper two thirds of the viewport.
    #[test]
    fn glass_button_row_keeps_vertical_room_for_backdrop() {
        let row_top_offset = MENU_BUTTON_HEIGHT + MENU_ROW_BOTTOM_MARGIN;
        assert!(
            row_top_offset <= 360.0,
            "button row + margin {} leaves < 360 px for Earth + title",
            row_top_offset
        );
    }
}
