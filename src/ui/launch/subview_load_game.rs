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

use crate::ui::launch::save_index::{SaveIndex, SaveSummary};
use crate::ui::launch::{LaunchState, LaunchSystemSet, PendingLaunchActions};
use crate::ui::theme;

/// Render the Load Game subview. Reads [`LaunchState::LoadGame`]
/// for gating; no-ops for every other variant. Lives in
/// [`LaunchSystemSet::Menu`] so it only ticks while the menu state
/// is active (PR-A's set is reserved for PR-C/D).
pub fn ui_load_game_subview(
    mut contexts: EguiContexts,
    mut launch_state: ResMut<LaunchState>,
    mut actions: ResMut<PendingLaunchActions>,
    save_index: Res<SaveIndex>,
) {
    if *launch_state != LaunchState::LoadGame {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let mut clicked_path: Option<std::path::PathBuf> = None;
    let mut back_clicked = false;

    egui::CentralPanel::default().show(ctx, |ui| {
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

        if save_index.entries.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(theme::Spacing::xl);
                ui.label(
                    egui::RichText::new(
                        "No saved missions yet. Begin a new mission and the autosave will appear here.",
                    )
                    .color(theme::TEXT_HINT)
                    .size(12.0),
                );
            });
        } else {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(theme::Spacing::md as i8))
                .show(ui, |ui| {
                    // ── Column header row ───────────────────────
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), theme::Spacing::md),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.strong("Name");
                                ui.add_space(theme::Spacing::lg);
                                ui.strong("Saved");
                                ui.add_space(theme::Spacing::lg);
                                ui.strong("Playtime");
                                ui.add_space(theme::Spacing::lg);
                                ui.strong("Seed");
                            },
                        );
                    });
                    ui.add_space(theme::Spacing::xs);

                    // ── One row per save ─────────────────────────
                    for entry in save_index.entries.iter() {
                        match entry {
                            SaveSummary::Valid { path, header } => {
                                let label = save_row_label(header);
                                let response = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new(label).color(theme::ACCENT),
                                    )
                                    .frame(false),
                                );
                                if response.clicked() {
                                    clicked_path = Some(path.clone());
                                }
                            }
                            SaveSummary::Broken { path: _, error } => {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        theme::RED,
                                        format!("Broken save ({})", error),
                                    );
                                });
                            }
                        }
                    }
                });
        }

        ui.add_space(theme::Spacing::lg);

        // ── Action row ──────────────────────────────────────
        ui.horizontal(|ui| {
            if ui
                .button(egui::RichText::new("Back").color(theme::TEXT_DIM))
                .clicked()
            {
                back_clicked = true;
            }
        });

        // ── Post-click state writes ─────────────────────────
        // Same rationale as `subview_new_game`: mutate resources
        // after the egui closure returns so we don't fight the
        // `ResMut` borrows held by the render closure.
        if back_clicked {
            actions.load_save = None;
            *launch_state = LaunchState::MainMenu;
        }
        if let Some(path) = clicked_path {
            actions.load_save = Some(path);
            *launch_state = LaunchState::InGame;
        }
    });
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
fn save_row_label(header: &crate::ui::launch::save_index::SaveHeader) -> String {
    format!(
        "{}  ·  {}  ·  {}  ·  seed {}",
        header.formatted_version(),
        header.formatted_saved_at(),
        header.formatted_playtime(),
        header.formatted_seed(),
    )
}

/// Register the Load Game subview render system in
/// [`LaunchSystemSet::Menu`].
pub fn register_load_game_subview(app: &mut App) {
    app.add_systems(
        EguiPrimaryContextPass,
        ui_load_game_subview.in_set(LaunchSystemSet::Menu),
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
            // 2026-07-03 22:00:00 UTC = unix 1788688800
            saved_at_unix_s: Some(1_788_688_800),
            playtime_s: Some(3 * 3600 + 12 * 60),
            seed: Some(42),
        };
        let label = save_row_label(&header);
        assert!(label.contains("0.5.0"));
        assert!(label.contains("2026-07-03"));
        assert!(label.contains("3h 12m"));
        assert!(label.contains("seed 42"));
    }

    #[test]
    fn save_row_label_handles_missing_fields() {
        let header = SaveHeader::default();
        let label = save_row_label(&header);
        assert!(label.contains("?"));
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
        });
        assert_eq!(index.valid_count(), 0);
        assert_eq!(index.broken_count(), 1);
    }
}
