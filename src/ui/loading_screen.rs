//! Loading screen shown during galaxy generation.
//!
//! Displays a modal overlay with a progress bar and current phase text.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::game_state::GamePhase;
use crate::ui::theme::{self, ACCENT, TEXT, TEXT_DIM};

/// Progress text for each generation phase.
#[derive(Resource, Default)]
pub struct GenerationProgressText(pub String);

/// System that renders the loading screen during galaxy generation.
pub fn ui_generation_screen(
    mut contexts: EguiContexts,
    game_phase: Res<GamePhase>,
    progress_text: Res<GenerationProgressText>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Only show during Generating phase
    if *game_phase != GamePhase::Generating {
        return;
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(egui::Color32::from_rgba_unmultiplied(8, 12, 24, 240)))
        .show(ctx, |ui| {
            let available = ui.available_rect();
            let panel_width = 400.0.min(available.width() - 40.0);
            let panel_height = 150.0.min(available.height() - 40.0);

            let panel_rect = egui::Rect::from_center_size(
                ui.center().unwrap(),
                egui::vec2(panel_width, panel_height),
            );

            egui::Area::new("generation_screen".into())
                .fixed_rect(panel_rect)
                .show(ui.ctx(), |ui| {
                    ui.set_width(panel_width);
                    ui.set_height(panel_height);

                    render_loading_content(ui, &progress_text.0);
                });
        });
}

/// Renders the loading screen content.
fn render_loading_content(ui: &mut egui::Ui, progress_text: &str) {
    ui.add_space(20.0);
    ui.heading("Generating Galaxy");
    ui.separator();
    ui.add_space(15.0);

    // Progress indicator (animated dots)
    let dots = match (ui.ctx().input(|i| i.time) as f64 % 2.0) as u32 {
        0 => ".",
        1 => "..",
        _ => "...",
    };

    ui.label(
        egui::RichText::new(format!("{}{}", progress_text, dots))
            .size(16.0)
            .color(TEXT),
    );

    ui.add_space(20.0);

    // Progress bar
    let progress = match (ui.ctx().input(|i| i.time) as f64 * 0.5) % 1.0 {
        p if p < 0.25 => p * 4.0,
        p if p < 0.5 => 0.25 + (p - 0.25) * 2.0,
        p if p < 0.75 => 0.5 + (p - 0.5) * 2.0,
        p => 0.75 + (p - 0.75) * 4.0,
    };

    ui.add(egui::ProgressBar::new(progress as f32).show_percentage());

    ui.add_space(15.0);
    ui.label(
        egui::RichText::new("Please wait...")
            .size(12.0)
            .color(TEXT_DIM)
            .italics(),
    );
}