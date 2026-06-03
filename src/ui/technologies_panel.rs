//! First egui panel: the Technologies screen.
//!
//! Renders every entry in `Res<TechnologiesData>` as a single row, proving the
//! RON data pipeline (see `DATA_LOADER.md`) is reaching the UI layer. The
//! panel is intentionally minimal — no progression logic, no queue, no
//! tooltips. The LGD will replace it with the real research screen once the
//! 5-era propulsion tree lands.
//!
//! Follows the architecture baseline (see `ARCHITECTURE_BASELINE.md` §2):
//! this system runs in the `EguiPrimaryContextPass` and reads the primary
//! egui context via `EguiContexts::ctx_mut()`. It does not create a second
//! `egui::Context` instance.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use super::theme;
use crate::research::TechnologiesData;

/// Render the Technologies screen.
///
/// Renders a floating egui `Window` listing every technology loaded by the
/// RON data loader. Always visible at app startup so the CTO and the LGD can
/// confirm the data pipeline end-to-end.
pub fn technologies_panel_system(
    mut contexts: EguiContexts,
    tech_data: Res<TechnologiesData>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let total = tech_data.technologies.len();

    egui::Window::new("Technologies")
        .id(egui::Id::new("technologies_panel"))
        .default_pos([960.0, 60.0])
        .default_size([420.0, 720.0])
        .resizable(true)
        .collapsible(true)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.style_mut().interaction.selectable_labels = false;

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Technologies")
                        .font(theme::title())
                        .color(theme::ACCENT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} loaded", total))
                            .font(theme::body(11.0))
                            .color(theme::TEXT_DIM),
                    );
                });
            });
            ui.label(
                egui::RichText::new(
                    "Read-only view of the RON-loaded tech tree. LGD will replace this.",
                )
                .font(theme::body(10.0))
                .color(theme::TEXT_HINT),
            );

            theme::divider(ui);

            if total == 0 {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new("No technologies loaded.")
                            .font(theme::body(12.0))
                            .color(theme::AMBER),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Check assets/data/technologies.ron and the data loader logs.",
                        )
                        .font(theme::body(10.0))
                        .color(theme::TEXT_HINT),
                    );
                });
                return;
            }

            // The data is a HashMap — sort by tier, then id, for a stable
            // display order that doesn't depend on the loader's iteration.
            let mut techs: Vec<&crate::research::Technology> =
                tech_data.technologies.values().collect();
            techs.sort_by(|a, b| a.tier.cmp(&b.tier).then_with(|| a.id.cmp(&b.id)));

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("technologies_grid")
                        .num_columns(5)
                        .striped(true)
                        .spacing([12.0, 4.0])
                        .min_col_width(40.0)
                        .show(ui, |ui| {
                            ui.strong(egui::RichText::new("Era").color(theme::ACCENT));
                            ui.strong(egui::RichText::new("ID").color(theme::ACCENT));
                            ui.strong(egui::RichText::new("Name").color(theme::ACCENT));
                            ui.strong(egui::RichText::new("Category").color(theme::ACCENT));
                            ui.strong(
                                egui::RichText::new("Cost").color(theme::ACCENT),
                            );
                            ui.end_row();

                            for tech in techs {
                                ui.label(
                                    egui::RichText::new(format!("{}", tech.tier))
                                        .font(theme::mono(11.0))
                                        .color(theme::TEXT_VALUE),
                                );
                                ui.label(
                                    egui::RichText::new(&tech.id)
                                        .font(theme::mono(11.0))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.label(
                                    egui::RichText::new(&tech.name)
                                        .font(theme::body(12.0))
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    egui::RichText::new(tech.category.display_name())
                                        .font(theme::body(11.0))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.label(
                                    egui::RichText::new(format_cost(tech.research_cost))
                                        .font(theme::mono(11.0))
                                        .color(theme::RP_BLUE),
                                );
                                ui.end_row();
                            }
                        });
                });
        });
}

/// Format a research cost in RP. Whole numbers get no decimal; fractional
/// values are shown with one decimal place.
fn format_cost(cost: f64) -> String {
    if (cost - cost.round()).abs() < f64::EPSILON {
        format!("{}", cost as i64)
    } else {
        format!("{:.1}", cost)
    }
}

#[cfg(test)]
mod tests {
    use super::format_cost;

    #[test]
    fn format_cost_whole_number() {
        assert_eq!(format_cost(0.0), "0");
        assert_eq!(format_cost(1000.0), "1000");
    }

    #[test]
    fn format_cost_fractional() {
        assert_eq!(format_cost(100.5), "100.5");
    }
}
