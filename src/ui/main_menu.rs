//! Main menu screen — title screen with New Game, Load, Settings buttons.
//!
//! Displayed when `GamePhase::MainMenu` is active.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::game_state::GamePhase;
use crate::ui::theme::{ep_amber, ACCENT, SURFACE, TEXT_DIM};

/// Local UI state for the main menu.
#[derive(Resource, Default)]
pub struct MainMenuState {
    /// Selected save slot for load-game UI (not yet implemented).
    pub selected_save_slot: Option<usize>,
}

/// System that renders the main menu screen.
pub fn ui_main_menu(
    mut contexts: EguiContexts,
    mut game_phase: ResMut<GamePhase>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Only show when in MainMenu phase
    if *game_phase != GamePhase::MainMenu {
        return;
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(egui::Color32::from_rgba_unmultiplied(8, 12, 24, 240)))
        .show(ctx, |ui| {
            let available = ui.available_rect();
            let panel_width = 500.0.min(available.width() - 40.0);
            let panel_height = 440.0.min(available.height() - 40.0);

            let panel_rect = egui::Rect::from_center_size(
                ui.center().unwrap(),
                egui::vec2(panel_width, panel_height),
            );

            egui::Area::new("main_menu_panel".into())
                .fixed_rect(panel_rect)
                .show(ui.ctx(), |ui| {
                    ui.set_width(panel_width);
                    ui.set_height(panel_height);

                    render_main_menu_content(ui, &mut game_phase);
                });
        });
}

/// Renders the main menu content with buttons.
fn render_main_menu_content(ui: &mut egui::Ui, game_phase: &mut ResMut<GamePhase>) {
    // Title area
    ui.add_space(20.0);

    // Game title — "HELIOS ASCENSION" in large bold amber
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("HELIOS")
                .size(52.0)
                .strong()
                .color(ep_amber()),
        );
        ui.label(
            egui::RichText::new("ASCENSION")
                .size(52.0)
                .strong()
                .color(ep_amber()),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("A 4X Space Strategy Game")
                .size(14.0)
                .color(TEXT_DIM),
        );
    });

    ui.add_space(30.0);

    // Menu buttons
    let button_width = 220.0;
    let button_height = 42.0;

    // New Game button — accent fill, transitions to NewGame phase
    let new_game_btn = egui::Button::new("New Game")
        .fill(ACCENT)
        .frame(false)
        .corner_radius(4.0);
    ui.vertical_centered(|ui| {
        if ui
            .add_sized(egui::vec2(button_width, button_height), new_game_btn)
            .clicked()
        {
            *game_phase = GamePhase::NewGame;
        }
    });

    ui.add_space(12.0);

    // Load Game button — surface color, placeholder log
    let load_btn = egui::Button::new("Load Game")
        .fill(SURFACE)
        .frame(false)
        .corner_radius(4.0);
    ui.vertical_centered(|ui| {
        if ui
            .add_sized(egui::vec2(button_width, button_height), load_btn)
            .clicked()
        {
            info!("Load game not yet implemented");
        }
    });

    ui.add_space(12.0);

    // Settings button — surface color, placeholder log
    let settings_btn = egui::Button::new("Settings")
        .fill(SURFACE)
        .frame(false)
        .corner_radius(4.0);
    ui.vertical_centered(|ui| {
        if ui
            .add_sized(egui::vec2(button_width, button_height), settings_btn)
            .clicked()
        {
            info!("Settings not yet implemented");
        }
    });

    ui.add_space(20.0);

    // Version/credits footer
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("v0.1.0 — Helios Ascension")
                .size(10.0)
                .color(TEXT_DIM),
        );
    });
}