//! Tutorial popup UI system
//!
//! Renders the tutorial step popup in the center of the screen when triggered.
//! Uses the design from DELA-39: centered window with title, body text,
//! progress indicator, Skip and Next buttons.

use bevy_egui::egui;
use bevy_egui::EguiContexts;

use crate::game_state::ActiveMenu;
use crate::tutorial::{trigger_condition_met, TUTORIAL_STEPS, TutorialState};
use crate::ui::time::SimulationTime;

/// Tutorial popup UI system.
///
/// Runs in `EguiPrimaryContextPass` to render overlay on top of game world.
pub fn ui_tutorial_popup(
    mut contexts: EguiContexts,
    mut tutorial_state: ResMut<TutorialState>,
    time: Res<SimulationTime>,
    active_menu: Res<ActiveMenu>,
) {
    // Skip if tutorials disabled or all steps complete
    if tutorial_state.disabled || tutorial_state.current_step >= TUTORIAL_STEPS.len() {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Get current step
    let step = match TUTORIAL_STEPS.get(tutorial_state.current_step) {
        Some(s) => s,
        None => return,
    };

    // Check trigger condition
    if !trigger_condition_met(&step.trigger_condition, &tutorial_state, &active_menu.current) {
        return;
    }

    // Window dimensions
    let window_width = 520.0;
    let window_height = 340.0;

    egui::Window::new("tutorial_popup_window")
        .id(egui::Id::new("tutorial_popup"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .frame(egui::Frame {
            fill: egui::Color32::from_rgba_unmultiplied(16, 20, 36, 250),
            stroke: egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 255)),
            corner_radius: 8.0,
            ..default()
        })
        .show(ctx, |ui| {
            ui.set_width(window_width);
            ui.set_height(window_height);

            // Custom title bar with icon
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("\u{1f4da}")
                        .size(24.0)
                        .color(egui::Color32::from_rgb(100, 200, 255)),
                );
                ui.label(
                    egui::RichText::new(&step.title)
                        .size(18.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 230, 255)),
                );
            });

            ui.add_space(4.0);

            // Progress indicator: "Step X of Y"
            ui.label(
                egui::RichText::new(format!(
                    "Step {} of {}",
                    tutorial_state.current_step + 1,
                    TUTORIAL_STEPS.len()
                ))
                .size(12.0)
                .color(egui::Color32::from_rgb(150, 180, 220)),
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(12.0);

            // Step body text
            ui.label(
                egui::RichText::new(step.body_text)
                    .size(14.0)
                    .color(egui::Color32::from_rgb(220, 225, 235)),
            );

            ui.add_space(16.0);

            // Target screen hint
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Target: ")
                        .size(12.0)
                        .color(egui::Color32::from_rgb(140, 160, 200)),
                );
                ui.label(
                    egui::RichText::new(step.target_screen.name())
                        .size(12.0)
                        .color(egui::Color32::from_rgb(180, 220, 255)),
                );
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);

            // Buttons row
            ui.horizontal(|ui| {
                // Skip Tutorial button
                if ui
                    .button(
                        egui::RichText::new("Skip Tutorial")
                            .size(14.0)
                            .color(egui::Color32::from_rgb(180, 180, 200)),
                    )
                    .clicked()
                {
                    tutorial_state.disabled = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Next button
                    let is_last_step = tutorial_state.current_step >= TUTORIAL_STEPS.len() - 1;
                    let next_text = if is_last_step {
                        "Finish"
                    } else {
                        "Next \u{2192}"
                    };

                    let next_button = egui::Button::new(
                        egui::RichText::new(next_text)
                            .size(14.0)
                            .color(egui::Color32::from_rgb(20, 30, 50)),
                    )
                    .fill(egui::Color32::from_rgb(80, 180, 255));

                    if ui.add(next_button).clicked() {
                        tutorial_state.advance_step();
                    }
                });
            });
        });
}
