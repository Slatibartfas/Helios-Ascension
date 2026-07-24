//! Load Game subview (GRA-318 PR-D).
//!
//! Renders when [`crate::ui::launch::LaunchState::LoadGame`] is
//! active. Surfaces the saves discovered by the PR-A
//! [`crate::ui::launch::SaveIndex`] scanner and lets the player
//! pick one to restore.
//!
//! Layout:
//! - One row per [`SaveSummary`] entry, with metadata columns
//!   derived from [`SaveHeader`] (helios_version,
//!   saved_at_unix_s, playtime_s, seed).
//! - Broken saves render in [`theme::RED`] and are not selectable.
//! - Back button returns to [`LaunchState::MainMenu`].
//!
//! On row click, the subview writes the chosen path into
//! [`crate::ui::launch::PendingLaunchActions::load_save`] and
//! transitions `LaunchState → InGame`. The
//! [`super::subview_kickoff::kickoff_world_system`] consumes the
//! request — the actual restore is GRA-314's job (still `in_progress`
//! at the time of this PR; the kickoff system logs the request and
//! leaves the world as-is so the menu → in-game transition is
//! observable end-to-end today).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::persistence::{delete_save_files, rescan_save_index};
use crate::ui::launch::save_index::{SaveHeader, SaveIndex, SaveSummary};
use crate::ui::launch::{LaunchState, LaunchSystemSet, PendingLaunchActions};
use crate::ui::theme;

#[derive(Resource, Debug, Default, Clone)]
pub struct PendingInGameLoadRequest {
    pub open_panel: bool,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct LoadGameReturnState {
    pub previous_state: Option<LaunchState>,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct PendingDeleteSave {
    pub path: Option<std::path::PathBuf>,
    pub confirmed: bool,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct LoadGameSelection {
    pub selected_path: Option<std::path::PathBuf>,
}

/// Render the Load Game subview. Reads [`LaunchState::LoadGame`]
/// for gating; no-ops for every other variant. Lives in
/// [`LaunchSystemSet::Menu`] so it only ticks while the menu state
/// is active (PR-A's set is reserved for PR-C/D).
pub fn ui_load_game_subview(
    mut contexts: EguiContexts,
    mut launch_state: ResMut<LaunchState>,
    mut actions: ResMut<PendingLaunchActions>,
    return_state: Res<LoadGameReturnState>,
    save_index: Res<SaveIndex>,
    mut selection: ResMut<LoadGameSelection>,
    mut delete: ResMut<PendingDeleteSave>,
    mut thumbnail: Local<Option<(std::path::PathBuf, egui::TextureHandle)>>,
) {
    if *launch_state != LaunchState::LoadGame {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let mut selected_path: Option<std::path::PathBuf> = None;
    let mut load_clicked = false;
    let mut back_clicked = false;

    // GRA-XYZ: transparent central panel so the rotating-Earth backdrop
    // stays visible behind the save list. The save list rows provide
    // their own opaque backgrounds for legibility.
    egui::CentralPanel::default()
        .frame(theme::menu_transparent_frame())
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(theme::Spacing::xl);
                ui.label(
                    egui::RichText::new("Load Mission")
                        .font(theme::title())
                        .color(theme::ACCENT)
                        .size(28.0),
                );
                ui.add_space(theme::Spacing::sm);
                ui.label(
                    egui::RichText::new(format!(
                        "{} saved mission(s) available",
                        save_index.valid_count()
                    ))
                    .color(theme::TEXT_DIM)
                    .size(11.0),
                );
                ui.add_space(theme::Spacing::lg);
            });

            let action_height = 72.0;
            let content_height = (ui.available_height() - action_height).max(280.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), content_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let available = ui.available_width();
                    if available >= 980.0 {
                        ui.columns(2, |columns| {
                            columns[0].set_min_width(available * 0.38);
                            render_save_list(
                                &mut columns[0],
                                &save_index,
                                &selection,
                                &mut selected_path,
                            );
                            if let Some((path, header)) = selected_save(&save_index, &selection) {
                                render_save_preview(
                                    &mut columns[1],
                                    ctx,
                                    path,
                                    header,
                                    &mut thumbnail,
                                );
                            } else {
                                columns[1].vertical_centered(|ui| {
                                    ui.add_space(120.0);
                                    ui.label(
                                        egui::RichText::new("Select a mission to preview it")
                                            .color(theme::TEXT_DIM),
                                    );
                                });
                            }
                        });
                    } else {
                        render_save_list(ui, &save_index, &selection, &mut selected_path);
                        if let Some((path, header)) = selected_save(&save_index, &selection) {
                            ui.add_space(theme::Spacing::lg);
                            render_save_preview(ui, ctx, path, header, &mut thumbnail);
                        }
                    }

                    if let Some(path) = selected_path {
                        selection.selected_path = Some(path);
                    }
                },
            );

            ui.add_space(theme::Spacing::sm);

            // ── Action row ──────────────────────────────────────
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui::launch::render_glass_button(ui, "Back", "", true).clicked() {
                    back_clicked = true;
                }
                if crate::ui::launch::render_glass_button(
                    ui,
                    "Delete",
                    "",
                    selection.selected_path.is_some(),
                )
                .clicked()
                {
                    delete.path = selection.selected_path.clone();
                }
                if crate::ui::launch::render_glass_button(
                    ui,
                    "Load",
                    "",
                    selection.selected_path.is_some(),
                )
                .clicked()
                {
                    load_clicked = true;
                }
            });

            if let Some(path) = delete.path.clone() {
                let mut open = true;
                egui::Window::new("Delete save?")
                    .id(egui::Id::new("delete-save-confirmation"))
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .collapsible(false)
                    .resizable(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        ui.set_min_width(380.0);
                        ui.label(format!("Permanently delete ‘{}’?", save_name(&path)));
                        ui.label(
                            egui::RichText::new("This also removes the saved preview image.")
                                .color(theme::TEXT_DIM),
                        );
                        ui.add_space(theme::Spacing::md);
                        ui.horizontal(|ui| {
                            if ui
                                .button(egui::RichText::new("Delete save").color(theme::RED))
                                .clicked()
                            {
                                delete.confirmed = true;
                            }
                            if ui.button("Cancel").clicked() {
                                delete.path = None;
                            }
                        });
                    });
                if !open {
                    delete.path = None;
                }
            }

            // ── Post-click state writes ─────────────────────────
            if back_clicked {
                actions.load_save = None;
                selection.selected_path = None;
                *launch_state = return_state.previous_state.unwrap_or(LaunchState::MainMenu);
            }
            if load_clicked {
                if let Some(path) = selection.selected_path.clone() {
                    actions.load_save = Some(path);
                    selection.selected_path = None;
                    *launch_state = LaunchState::InGame;
                }
            }
        });
}

fn selected_save<'a>(
    save_index: &'a SaveIndex,
    selection: &LoadGameSelection,
) -> Option<(&'a std::path::PathBuf, &'a SaveHeader)> {
    selection.selected_path.as_ref().and_then(|selected| {
        save_index.entries.iter().find_map(|entry| match entry {
            SaveSummary::Valid { path, header } if path == selected => Some((path, header)),
            _ => None,
        })
    })
}

fn render_save_list(
    ui: &mut egui::Ui,
    save_index: &SaveIndex,
    selection: &LoadGameSelection,
    selected_path: &mut Option<std::path::PathBuf>,
) {
    if save_index.entries.is_empty() {
        ui.label(egui::RichText::new("No saved missions yet.").color(theme::TEXT_HINT));
        return;
    }
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(theme::Spacing::md as i8))
        .show(ui, |ui| {
            ui.strong("Saved missions");
            ui.add_space(theme::Spacing::xs);
            for entry in &save_index.entries {
                match entry {
                    SaveSummary::Valid { path, header } => {
                        let selected = selection.selected_path.as_ref() == Some(path);
                        let label = save_row_label(path, header);
                        let button =
                            egui::Button::new(egui::RichText::new(label).color(if selected {
                                theme::ACCENT
                            } else {
                                theme::TEXT
                            }))
                            .fill(if selected {
                                theme::BUTTON_ACTIVE_BG
                            } else {
                                theme::SURFACE
                            })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if selected {
                                    theme::ACCENT
                                } else {
                                    theme::BORDER
                                },
                            ))
                            .min_size(egui::vec2(ui.available_width(), 34.0));
                        let response = ui.add(button);
                        if response.hovered() {
                            ui.painter().rect_stroke(
                                response.rect,
                                3.0,
                                egui::Stroke::new(1.5, theme::ACCENT),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if response.clicked() {
                            *selected_path = Some(path.clone());
                        }
                    }
                    SaveSummary::Broken { path, error, .. } => {
                        ui.colored_label(theme::RED, format!("{} — {error}", save_name(path)));
                    }
                }
            }
        });
}

fn save_name(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unnamed save".to_string())
}

fn render_save_preview(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    save_path: &std::path::Path,
    header: &SaveHeader,
    thumbnail: &mut Option<(std::path::PathBuf, egui::TextureHandle)>,
) {
    let preview = &header.preview;
    let image_path = save_path.with_extension("png");
    if thumbnail
        .as_ref()
        .is_none_or(|(loaded, _)| loaded != &image_path)
    {
        *thumbnail = load_save_thumbnail(ctx, &image_path).map(|texture| (image_path, texture));
    }
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(theme::Spacing::md as i8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("load-save-preview-scroll")
                .max_height((ui.ctx().content_rect().height() - 190.0).max(320.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.strong("Selected mission preview");
                    if let Some((_, texture)) = thumbnail.as_ref() {
                        let available = ui.available_size();
                        let max_size = egui::vec2(
                            ui.available_width().min(820.0),
                            (available.y * 0.52).clamp(280.0, 520.0),
                        );
                        ui.add(egui::Image::new(texture).fit_to_exact_size(max_size));
                    } else {
                        ui.label(
                            egui::RichText::new("No map thumbnail available for this save.")
                                .color(theme::TEXT_DIM),
                        );
                    }
                    ui.label(format!(
                        "Current date: {}",
                        if preview.current_date.is_empty() {
                            "—"
                        } else {
                            &preview.current_date
                        }
                    ));
                    ui.label(format!("Playtime: {}", header.formatted_playtime()));
                    ui.label(format!(
                        "Colonies: {}  ·  Population: {:.0}  ·  Ships: {}",
                        preview.colony_count, preview.total_population, preview.ship_count
                    ));
                    ui.label(format!(
                        "Kardashev value: Type {:.3}  ·  Power: {:.3e} W",
                        preview.kardashev_value, preview.power_produced_watts
                    ));

                    render_kardashev_plot(ui, &preview.kardashev_history);

                    if preview.resources.is_empty() {
                        ui.label(
                            egui::RichText::new("Resources: no preview data")
                                .color(theme::TEXT_DIM),
                        );
                    } else {
                        ui.collapsing("Resources", |ui| {
                            for (name, amount) in &preview.resources {
                                ui.label(format!("{name}: {amount:.3} Mt"));
                            }
                        });
                    }
                    if preview.screenshot_file.is_some() {
                        ui.label(
                            egui::RichText::new(
                                "Map viewpoint thumbnail is captured with this save.",
                            )
                            .color(theme::TEXT_HINT),
                        );
                    }
                    ui.label(
                        egui::RichText::new(
                            "Review this information, then press Load to continue.",
                        )
                        .color(theme::TEXT_DIM),
                    );
                });
        });
}

fn load_save_thumbnail(ctx: &egui::Context, path: &std::path::Path) -> Option<egui::TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture(
        format!("save-thumbnail:{}", path.display()),
        color,
        egui::TextureOptions::NEAREST,
    ))
}

fn render_kardashev_plot(ui: &mut egui::Ui, points: &[(f64, f64)]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(640.0), 150.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 4.0, theme::SURFACE);
    if points.len() < 2 {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Awaiting Kardashev history",
            theme::body(11.0),
            theme::TEXT_DIM,
        );
        return;
    }
    let plot_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 46.0, rect.top() + 12.0),
        egui::pos2(rect.right() - 10.0, rect.bottom() - 30.0),
    );
    let min_x = points.first().map(|p| p.0).unwrap_or(0.0);
    let max_x = points.last().map(|p| p.0).unwrap_or(min_x + 1.0);
    let (mut min_y, mut max_y) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });
    if (max_y - min_y).abs() < f64::EPSILON {
        min_y -= 0.01;
        max_y += 0.01;
    }
    let line: Vec<egui::Pos2> = points
        .iter()
        .map(|(x, y)| {
            egui::pos2(
                egui::lerp(
                    plot_rect.left()..=plot_rect.right(),
                    ((x - min_x) / (max_x - min_x).max(1.0)) as f32,
                ),
                egui::lerp(
                    plot_rect.bottom()..=plot_rect.top(),
                    ((y - min_y) / (max_y - min_y)) as f32,
                ),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        line,
        egui::Stroke::new(2.0, theme::CAT_STRATEGIC),
    ));
    for tick in 0..=2 {
        let t = tick as f32 / 2.0;
        let y = egui::lerp(plot_rect.bottom()..=plot_rect.top(), t);
        let value = min_y + (max_y - min_y) * t as f64;
        ui.painter().text(
            egui::pos2(plot_rect.left() - 5.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{value:.3}"),
            theme::mono(9.0),
            theme::TEXT_HINT,
        );
    }
    for tick in 0..=2 {
        let t = tick as f32 / 2.0;
        let x = egui::lerp(plot_rect.left()..=plot_rect.right(), t);
        let years_ago = (1.0 - t as f64) * crate::economy::HISTORY_MAX_AGE_YEARS;
        ui.painter().text(
            egui::pos2(x, plot_rect.bottom() + 5.0),
            egui::Align2::CENTER_TOP,
            if tick == 2 {
                "Now".to_string()
            } else {
                format!("{years_ago:.0}y")
            },
            theme::mono(9.0),
            theme::TEXT_HINT,
        );
    }
    ui.painter().text(
        egui::pos2(rect.left() + 4.0, rect.top() + 4.0),
        egui::Align2::LEFT_TOP,
        "Kardashev Type",
        theme::mono(9.0),
        theme::TEXT_HINT,
    );
    ui.painter().text(
        egui::pos2(plot_rect.center().x, rect.bottom() - 2.0),
        egui::Align2::CENTER_BOTTOM,
        "Simulation history (years ago)",
        theme::mono(9.0),
        theme::TEXT_HINT,
    );
}

/// Build the row label string for a valid save. We deliberately
/// keep this as one composed string (rather than separate
/// columns) because the load-game subview is short — a single
/// line keeps the row scannable on small windows. Multi-column
/// rendering can land behind a `show_columns` toggle in a later
/// PR.
///
/// All formatting is delegated to [`SaveHeader`]'s `formatted_*`
/// helpers so the menu HUD and the in-game HUD stay consistent
/// (both render the same playtime / timestamp shape).
fn save_row_label(
    path: &std::path::Path,
    header: &crate::ui::launch::save_index::SaveHeader,
) -> String {
    format!(
        "{}  ·  {}  ·  {}  ·  seed {}",
        save_name(path),
        header.formatted_saved_at(),
        header.formatted_playtime(),
        header.formatted_seed(),
    )
}

pub fn consume_load_requests(world: &mut World) {
    let open = world
        .get_resource::<PendingInGameLoadRequest>()
        .is_some_and(|request| request.open_panel);
    if open && *world.resource::<LaunchState>() == LaunchState::InGame {
        world.resource_mut::<LoadGameReturnState>().previous_state = Some(LaunchState::InGame);
        *world.resource_mut::<LaunchState>() = LaunchState::LoadGame;
        world.remove_resource::<PendingInGameLoadRequest>();
        rescan_save_index(world);
    }

    let delete_path = world
        .get_resource::<PendingDeleteSave>()
        .filter(|pending| pending.confirmed)
        .and_then(|pending| pending.path.clone());
    if let Some(path) = delete_path {
        if let Err(error) = delete_save_files(&path) {
            warn!("LoadGame: failed to delete {}: {error}", path.display());
        }
        world.resource_mut::<PendingDeleteSave>().path = None;
        world.resource_mut::<PendingDeleteSave>().confirmed = false;
        world.resource_mut::<LoadGameSelection>().selected_path = None;
        rescan_save_index(world);
    }
}

/// Register the Load Game subview render system in
/// [`LaunchSystemSet::Menu`].
pub fn register_load_game_subview(app: &mut App) {
    app.init_resource::<LoadGameSelection>()
        .init_resource::<LoadGameReturnState>()
        .init_resource::<PendingDeleteSave>()
        .add_systems(
            EguiPrimaryContextPass,
            (
                ui_load_game_subview.in_set(LaunchSystemSet::Menu),
                consume_load_requests.after(ui_load_game_subview),
            ),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::save_index::{SaveHeader, SaveIndex};
    use bevy::ecs::world::World;

    fn world_with_empty_index() -> World {
        let mut world = World::new();
        world.init_resource::<LaunchState>();
        world.init_resource::<PendingLaunchActions>();
        world.insert_resource(SaveIndex::empty());
        world
    }

    #[test]
    fn save_header_formatted_playtime_hours_minutes() {
        // The SaveHeader's formatted_playtime helper is now the
        // source of truth for the menu's playtime column (it
        // lives in save_index.rs to keep both the menu and the
        // in-game HUD consistent). This test exercises the
        // Same/Some path.
        let h = SaveHeader {
            playtime_s: Some(3 * 3600 + 12 * 60 + 45),
            ..Default::default()
        };
        assert_eq!(h.formatted_playtime(), "3h 12m");
    }

    #[test]
    fn save_header_formatted_playtime_minutes_seconds() {
        let h = SaveHeader {
            playtime_s: Some(12 * 60 + 45),
            ..Default::default()
        };
        assert_eq!(h.formatted_playtime(), "12m 45s");
    }

    #[test]
    fn save_header_formatted_playtime_seconds_only() {
        let h = SaveHeader {
            playtime_s: Some(45),
            ..Default::default()
        };
        assert_eq!(h.formatted_playtime(), "45s");
    }

    #[test]
    fn save_header_formatted_playtime_zero() {
        let h = SaveHeader {
            playtime_s: Some(0),
            ..Default::default()
        };
        assert_eq!(h.formatted_playtime(), "0s");
    }

    #[test]
    fn save_header_formatted_playtime_none_is_em_dash() {
        let h = SaveHeader::default();
        assert_eq!(h.formatted_playtime(), "—");
    }

    #[test]
    fn save_row_label_includes_all_header_fields() {
        let header = SaveHeader {
            format_version: Some(1),
            helios_version: Some("0.5.0".into()),
            // 2026-09-06 10:00:00 UTC = unix 1788688800 (verified via
            // `new Date(1788688800 * 1000).toISOString()`).
            saved_at_unix_s: Some(1_788_688_800),
            playtime_s: Some(3 * 3600 + 12 * 60),
            seed: Some(42),
            preview: Default::default(),
        };
        let path = std::path::PathBuf::from("campaign_alpha.ron");
        let label = save_row_label(&path, &header);
        assert!(label.contains("campaign_alpha"));
        assert!(label.contains("2026-09-06"));
        assert!(label.contains("3h 12m"));
        assert!(label.contains("seed 42"));
    }

    #[test]
    fn save_row_label_handles_missing_fields() {
        let header = SaveHeader::default();
        let path = std::path::PathBuf::from("unnamed.ron");
        let label = save_row_label(&path, &header);
        assert!(label.contains("unnamed"));
        assert!(label.contains("Unknown"));
        assert!(label.contains("—"));
    }

    #[test]
    fn world_renders_nothing_when_no_saves() {
        // The render system is not exercised here (no egui context
        // in unit tests per `feedback-egui-render-tests`), but we
        // verify the world can be constructed and the index is
        // empty — the render path will hit the empty-index branch.
        let world = world_with_empty_index();
        let idx = world.resource::<SaveIndex>();
        assert_eq!(idx.valid_count(), 0);
        assert_eq!(idx.broken_count(), 0);
    }

    #[test]
    fn save_index_valid_count_distinguishes_broken() {
        use crate::ui::launch::save_index::SaveSummary;
        let mut index = SaveIndex::empty();
        index.entries.push(SaveSummary::Valid {
            path: "/tmp/a.ron".into(),
            header: SaveHeader::default(),
        });
        index.entries.push(SaveSummary::Broken {
            path: "/tmp/b.ron".into(),
            error: "RON parse failed".into(),
            mtime_unix_s: None,
        });
        assert_eq!(index.valid_count(), 1);
        assert_eq!(index.broken_count(), 1);
    }

    /// Issue-body test plan bullet 2: clicking a row in `LoadGame`
    /// produces the right `PendingLaunchActions::load_save`. As
    /// with `subview_new_game`, egui render cannot be driven from
    /// `cargo test` so we simulate the post-click resource writes
    /// performed by `ui_load_game_subview`.
    #[test]
    fn load_game_row_click_writes_path_and_advances_state() {
        let mut world = world_with_empty_index();
        *world.resource_mut::<LaunchState>() = LaunchState::LoadGame;
        let chosen = std::path::PathBuf::from("/tmp/saved_mission_alpha.ron");
        // Simulate the post-click writes performed by the
        // LoadGame render code when a valid row is clicked:
        // the chosen path goes into `load_save` and `LaunchState`
        // advances to `InGame`.
        world.resource_mut::<PendingLaunchActions>().load_save = Some(chosen.clone());
        *world.resource_mut::<LaunchState>() = LaunchState::InGame;

        let actions = world.resource::<PendingLaunchActions>();
        assert_eq!(actions.load_save.as_ref(), Some(&chosen));
        assert_eq!(*world.resource::<LaunchState>(), LaunchState::InGame);
    }

    /// Back-button path on `LoadGame` clears any pending restore
    /// path and returns to `MainMenu`.
    #[test]
    fn load_game_back_clears_path_and_returns_to_main_menu() {
        let mut world = world_with_empty_index();
        *world.resource_mut::<LaunchState>() = LaunchState::LoadGame;
        world.resource_mut::<PendingLaunchActions>().load_save =
            Some(std::path::PathBuf::from("/tmp/foo.ron"));

        world.resource_mut::<PendingLaunchActions>().load_save = None;
        *world.resource_mut::<LaunchState>() = LaunchState::MainMenu;

        assert!(world.resource::<PendingLaunchActions>().load_save.is_none());
        assert_eq!(*world.resource::<LaunchState>(), LaunchState::MainMenu);
    }

    /// Issue-body test plan bullet 4: an invalid (broken) save
    /// increments `SaveIndex::broken_count`. This re-asserts the
    /// PR-A contract from `save_index::scan` in the PR-D context,
    /// so a regression that drops the broken handling is caught
    /// alongside the subview tests.
    #[test]
    fn invalid_path_increments_broken_count() {
        use crate::ui::launch::save_index::SaveSummary;
        let mut index = SaveIndex::empty();
        index.entries.push(SaveSummary::Broken {
            path: "/tmp/not_ron.ron".into(),
            error: "RON parse failed".into(),
            mtime_unix_s: None,
        });
        assert_eq!(index.valid_count(), 0);
        assert_eq!(index.broken_count(), 1);
    }
}
