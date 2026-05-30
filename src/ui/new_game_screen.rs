//! New Game screen — galaxy configuration UI.
//!
//! Displays a full-panel overlay for:
//! - Galaxy settings (stars, AI factions, starting resources)
//! - Player faction customization (name, color, personality)
//! - Tutorial vs sandbox start choice
//! - Galaxy generation trigger

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::ai::components::{AIDifficulty, AIPersonality};
use crate::game_state::{FactionConfig, GalaxyConfig, GamePhase};
use crate::ui::theme::{self, ep_amber, ep_teal, ACCENT, TEXT, TEXT_DIM};

/// Default faction presets for quick selection.
const FACTION_PRESETS: &[(&str, [u8; 3], AIPersonality)] = &[
    ("Federal Union", [100, 140, 200], AIPersonality::Balanced),
    ("Terra Dominion", [220, 80, 80], AIPersonality::Militarist),
    ("Scientific Coalition", [80, 200, 140], AIPersonality::Scientific),
    ("Mercantile Guild", [200, 180, 60], AIPersonality::Economic),
];

/// Color options for player faction.
const FACTION_COLORS: [[u8; 3]; 8] = [
    [100, 180, 255], // Blue
    [255, 100, 100], // Red
    [100, 200, 100], // Green
    [255, 200, 100], // Gold
    [180, 100, 255], // Purple
    [100, 255, 200], // Cyan
    [255, 150, 100], // Orange
    [200, 200, 200], // Silver
];

/// System that renders the New Game configuration screen.
pub fn ui_new_game_screen(
    mut contexts: EguiContexts,
    mut game_phase: ResMut<GamePhase>,
    mut galaxy_config: ResMut<GalaxyConfig>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Only show when in NewGame phase
    if *game_phase != GamePhase::NewGame {
        return;
    }

    // Full-screen centered panel
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(egui::Color32::from_rgba_unmultiplied(8, 12, 24, 240)))
        .show(ctx, |ui| {
            let available = ui.available_rect();
            let panel_width = 600.0.min(available.width() - 40.0);
            let panel_height = 580.0.min(available.height() - 40.0);

            let panel_rect = egui::Rect::from_center_size(
                ui.center().unwrap(),
                egui::vec2(panel_width, panel_height),
            );

            egui::Area::new("new_game_panel".into())
                .fixed_rect(panel_rect)
                .show(ui.ctx(), |ui| {
                    ui.set_width(panel_width);
                    ui.set_height(panel_height);

                    render_new_game_content(ui, &mut game_phase, &mut galaxy_config);
                });
        });
}

/// Renders the content of the new game configuration panel.
fn render_new_game_content(
    ui: &mut egui::Ui,
    game_phase: &mut ResMut<GamePhase>,
    galaxy_config: &mut ResMut<GalaxyConfig>,
) {
    // Title
    ui.add_space(20.0);
    ui.heading("⚙ New Game");
    ui.separator();

    // ── Galaxy Settings section ────────────────────────────────────────────
    ui.add_space(10.0);
    ui.label(egui::RichText::new("GALAXY SETTINGS").size(14.0).color(TEXT_DIM));
    ui.add_space(5.0);

    // Stars slider
    ui.horizontal(|ui| {
        ui.label("Stars:");
        ui.add_space(10.0);
        let mut stars = galaxy_config.num_stars as i32;
        ui.add(egui::Slider::new(&mut stars, 1..=10).text(""));
        galaxy_config.num_stars = stars as u32;
        ui.label(format!("{}", galaxy_config.num_stars));
    });

    // AI Factions slider
    ui.horizontal(|ui| {
        ui.label("AI Factions:");
        ui.add_space(10.0);
        let mut factions = galaxy_config.num_ai_factions as i32;
        ui.add(egui::Slider::new(&mut factions, 0..=5).text(""));
        galaxy_config.num_ai_factions = factions as u32;
        ui.label(format!("{}", galaxy_config.num_ai_factions));
    });

    // Starting resources slider
    ui.horizontal(|ui| {
        ui.label("Starting Resources:");
        ui.add_space(10.0);
        let mut resources = (galaxy_config.starting_resource_multiplier * 100.0) as i32;
        ui.add(egui::Slider::new(&mut resources, 50..=200).text(""));
        galaxy_config.starting_resource_multiplier = (resources as f32) / 100.0;
        ui.label(format!("{:.1}×", galaxy_config.starting_resource_multiplier));
    });

    // ── Faction Selection section ──────────────────────────────────────────
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new("PLAYER FACTION").size(14.0).color(TEXT_DIM));
    ui.add_space(5.0);

    // Faction name input
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.add_space(10.0);
        ui.add(egui::TextEdit::singleline(&mut galaxy_config.player_faction.name)
            .hint_text("Enter faction name")
            .desired_width(200.0));
    });

    // Color picker
    ui.horizontal(|ui| {
        ui.label("Color:");
        ui.add_space(10.0);
        for (idx, color) in FACTION_COLORS.iter().enumerate() {
            let color32 = egui::Color32::from_rgb(color[0], color[1], color[2]);
            let is_selected = galaxy_config.player_faction.color == *color;
            let rect = ui.allocate(egui::vec2(28.0, 28.0), egui::Sense::click()).rect;
            let painter = ui.painter();

            // Draw color swatch
            painter.rect_filled(rect, 4.0, color32);
            if is_selected {
                painter.rect_stroke(rect, 2.0, egui::Stroke::new(2.0, ACCENT), egui::StrokeKind::Outside);
            }

            if rect.contains(ui.ctx().pointer_interact_pos().unwrap_or(egui::Pos2::new(-1.0, -1.0))) && ui.ctx().input(|i| i.pointer.any_click()) {
                galaxy_config.player_faction.color = *color;
            }

            if idx < FACTION_COLORS.len() - 1 {
                ui.add_space(4.0);
            }
        }
    });

    // Faction presets quick-select
    ui.horizontal(|ui| {
        ui.label("Presets:");
        ui.add_space(10.0);
        for (name, color, personality) in FACTION_PRESETS {
            let btn = egui::Button::new(*name)
                .small()
                .fill(if galaxy_config.player_faction.name == *name {
                    theme::SURFACE_RAISED
                } else {
                    theme::SURFACE
                });
            if ui.add(btn).clicked() {
                galaxy_config.player_faction.name = name.to_string();
                galaxy_config.player_faction.color = *color;
                galaxy_config.player_faction.personality = *personality;
            }
            ui.add_space(5.0);
        }
    });

    // ── Start Options section ─────────────────────────────────────────────
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new("START OPTIONS").size(14.0).color(TEXT_DIM));
    ui.add_space(5.0);

    ui.radio_value(&mut galaxy_config.tutorial_enabled, true, "Guided Tutorial");
    ui.add_space(5.0);
    ui.radio_value(&mut galaxy_config.tutorial_enabled, false, "Sandbox (free play)");

    // ── Action buttons ────────────────────────────────────────────────────
    ui.add_space(20.0);
    ui.separator();
    ui.add_space(15.0);

    ui.horizontal(|ui| {
        let back_btn = egui::Button::new("← Back to Menu")
            .fill(theme::SURFACE)
            .frame(true);
        if ui.add(back_btn).clicked() {
            *game_phase = GamePhase::MainMenu;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let generate_btn = egui::Button::new("Generate Galaxy →")
                .fill(ACCENT)
                .frame(false)
                .small();
            if ui.add(generate_btn).clicked() {
                // Validate faction name
                if galaxy_config.player_faction.name.trim().is_empty() {
                    galaxy_config.player_faction.name = "Human".to_string();
                }
                *game_phase = GamePhase::Generating;
            }
        });
    });
}