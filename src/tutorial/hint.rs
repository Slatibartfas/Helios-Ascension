//! Hint system for contextual tips when player is idle
//!
//! Displays a toast notification at the bottom of the screen when the player
//! is on the same screen for >3 minutes, with a 5-minute cooldown per screen.
//! Hint auto-dismisses after 10 seconds.
//!
//! Design from DELA-39 section 2.4.

use bevy_egui::egui;
use bevy_egui::EguiContexts;

use crate::game_state::ActiveMenu;
use crate::tutorial::{HintInfo, TUTORIAL_STEPS, TutorialState};
use crate::ui::time::SimulationTime;

/// Idle time threshold before showing hint (3 minutes in seconds)
const IDLE_THRESHOLD_SECONDS: f64 = 180.0;
/// Cooldown between hints on the same screen (5 minutes in seconds)
const HINT_COOLDOWN_SECONDS: f64 = 300.0;
/// How long the hint toast stays visible (10 seconds)
const HINT_DISPLAY_SECONDS: f64 = 10.0;

/// Update the hint system based on idle time per screen.
///
/// This system:
/// 1. Tracks idle time per screen (GameMenu)
/// 2. Shows a hint toast when idle > 3 min with expired cooldown
/// 3. Auto-dismisses hints after 10 seconds
pub fn update_hint_system(
    time: Res<SimulationTime>,
    active_menu: Res<ActiveMenu>,
    mut tutorial_state: ResMut<TutorialState>,
) {
    let current_menu = active_menu.current;
    let elapsed = time.elapsed;

    // Update idle timer for current screen
    let idle_times = &mut tutorial_state.idle_times;
    let last_activity = idle_times.entry(current_menu).or_insert(elapsed);
    let idle_duration = elapsed - *last_activity;

    // Check if hint should be dismissed (10 second auto-dismiss)
    if let Some(ref mut hint) = tutorial_state.active_hint {
        if elapsed >= hint.show_until {
            tutorial_state.active_hint = None;
        }
        // Don't update idle time while hint is showing
        return;
    }

    // Update last activity time (player is actively using this screen)
    *last_activity = elapsed;

    // Skip if tutorials disabled or all steps complete
    if tutorial_state.disabled || tutorial_state.current_step >= TUTORIAL_STEPS.len() {
        return;
    }

    // Check if on a hintable screen with expired cooldown
    let cooldown_key = current_menu;
    let last_hint_time = tutorial_state.hint_cooldowns.get(&cooldown_key).copied();
    let cooldown_ok = last_hint_time.map(|t| elapsed - t > HINT_COOLDOWN_SECONDS).unwrap_or(true);

    // Show hint if: idle > 3 min AND cooldown expired
    if idle_duration > IDLE_THRESHOLD_SECONDS && cooldown_ok {
        if let Some(step) = TUTORIAL_STEPS.get(tutorial_state.current_step) {
            tutorial_state.active_hint = Some(HintInfo {
                message: step.hint_text.to_string(),
                screen: current_menu,
                show_until: elapsed + HINT_DISPLAY_SECONDS,
            });
            tutorial_state.hint_cooldowns.insert(cooldown_key, elapsed);
        }
    }
}

/// Render the hint toast at the bottom of the screen.
///
/// Runs in `EguiPrimaryContextPass` as an overlay.
pub fn ui_hint_toast(
    mut contexts: EguiContexts,
    tutorial_state: Res<TutorialState>,
) {
    let Some(hint) = &tutorial_state.active_hint else {
        return;
    };

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Hint toast at bottom center
    let toast_width = 480.0;
    let toast_height = 60.0;

    // Position at bottom center with some margin
    let screen_height = ctx.available_rect().max.y;
    let pos = egui::pos2(
        (ctx.available_rect().max.x - toast_width) / 2.0,
        screen_height - toast_height - 20.0,
    );

    egui::Area::new("hint_toast")
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_width(toast_width);
            ui.set_height(toast_height);

            egui::Frame {
                fill: egui::Color32::from_rgba_unmultiplied(20, 25, 45, 240),
                stroke: egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 180, 100)),
                corner_radius: 6.0,
                ..default()
            }
            .show(ui, |ui| {
                ui.set_width(toast_width);
                ui.set_height(toast_height);

                ui.horizontal_wrapped(|ui| {
                    // Hint icon
                    ui.label(
                        egui::RichText::new("\u{1f4a1}")
                            .size(18.0)
                            .color(egui::Color32::from_rgb(255, 220, 100)),
                    );

                    // Hint text
                    ui.label(
                        egui::RichText::new(&hint.message)
                            .size(13.0)
                            .color(egui::Color32::from_rgb(220, 225, 240)),
                    );
                });

                // Small screen indicator
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Hint for: {}", hint.screen.name()))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(140, 150, 170)),
                );
            });
        });
}
