//! Save Panel subview (GRA-358 PR-C).
//!
//! Surfaces when [`crate::ui::launch::LaunchState::SaveGame`] is
//! active. Two action paths:
//!
//! - **Save** — writes the live world to a "current slot" file
//!   (`saves/current_save.ron`) via
//!   [`crate::persistence::write_save_atomic`] (via
//!   [`crate::persistence::write_save_to_path`]).
//! - **Save As** — a slot picker the player can use to write to
//!   any path under `<userdata>/saves/`. Existing entries are read
//!   from [`SaveIndex`].
//!
//! Both action paths go through [`crate::persistence::PendingSaveActions`]
//! — the egui render system pushes the player's selection onto
//! the queue, and a separate exclusive system
//! ([`consume_save_actions_system`]) drains the queue in the same
//! frame. The split lets the render system use only read-only
//! resources (no [`Res`] / [`ResMut`] conflict with an exclusive
//! world-mutating consumer).
//!
//! Re-scans [`SaveIndex`] after every successful write so the menu's
//! Load Game list picks up the new file without a restart.
//!
//! # In-game entry-point
//!
//! When `LaunchState` is `InGame`, the in-game dashboard's "Save"
//! entry (wired in [`crate::ui::dashboard::ui_dashboard`]) flips
//! `LaunchState` to `SaveGame`. The Back button round-trips back to
//! whichever state the player was in before opening the panel; we
//! track that via [`PendingSavePanelReturn`] so `Back` from in-game
//! Save returns to InGame, not MainMenu.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use std::path::PathBuf;

use crate::persistence::{rescan_save_index, write_save_to_path};
use crate::ui::launch::save_index::{SaveIndex, SaveIndexState, SaveSummary, SAVES_SUBDIR};
use crate::ui::launch::userdata::resolve_userdata_dir;
use crate::ui::launch::{LaunchState, LaunchSystemSet};
use crate::ui::theme;

/// Track which [`LaunchState`] the player was in before opening
/// the Save Panel from in-game. Populated when the in-game Save
/// entry triggers a transition to `SaveGame`; consumed when the
/// player presses Back.
#[derive(Resource, Debug, Default, Clone)]
pub struct PendingSavePanelReturn {
    pub previous_state: Option<LaunchState>,
}

/// One-shot queue for "the in-game dashboard's Save button was
/// clicked, open the Save Panel subview". Inserted by the render
/// system through `Commands::insert_resource(...)` (which `ui_dashboard`
/// already carries) — that path avoids growing the dashboard's
/// parameter list past Bevy 0.18's type-complexity ceiling. The
/// exclusive consumer ([`consume_in_game_save_request_system`]) reads
/// the queue, captures `LaunchState` into
/// [`PendingSavePanelReturn::previous_state`], advances `LaunchState`
/// to `SaveGame`, and removes the resource.
#[derive(Resource, Debug, Default, Clone)]
pub struct PendingInGameSaveRequest {
    pub open_panel: bool,
}

impl PendingInGameSaveRequest {
    pub fn has_any(&self) -> bool {
        self.open_panel
    }
}

/// Action queued by the Save Panel render system. Drained by
/// [`consume_save_actions_system`] in the same frame.
///
/// Two variants keep the surface narrow: writing to the current
/// slot, or writing to an explicit picked path. The consumer
/// resolves both to a [`std::path::PathBuf`].
#[derive(Debug, Default, Resource, Clone)]
pub struct PendingSaveActions {
    pub save_to_current_slot: bool,
    pub save_as_path: Option<PathBuf>,
}

impl PendingSaveActions {
    pub fn has_any(&self) -> bool {
        self.save_to_current_slot || self.save_as_path.is_some()
    }

    pub fn clear(&mut self) {
        self.save_to_current_slot = false;
        self.save_as_path = None;
    }
}

/// Default quick-save filename. Lives under `<userdata>/saves/`.
const CURRENT_SAVE_FILE: &str = "current_save.ron";

/// Render the Save Panel subview.
///
/// Gated on `LaunchState == SaveGame` — every other state is a
/// no-op so the subview can coexist with the main-menu shell render
/// in the same [`LaunchSystemSet::Menu`] pass. The system uses only
/// read-only resource handles and pushes to [`PendingSaveActions`];
/// the actual disk write happens in [`consume_save_actions_system`]
/// which takes `&mut World`.
pub fn ui_save_panel_subview(
    mut contexts: EguiContexts,
    launch_state: Res<LaunchState>,
    save_index: Res<SaveIndex>,
    save_index_state: Res<SaveIndexState>,
    return_state: Res<PendingSavePanelReturn>,
    mut pending: ResMut<PendingSaveActions>,
    mut commands: Commands,
) {
    if *launch_state != LaunchState::SaveGame {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let mut back_clicked = false;
    let mut save_clicked = false;
    let mut save_as_target: Option<PathBuf> = None;

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(theme::Spacing::xl);
            ui.label(
                egui::RichText::new("Save Game")
                    .font(theme::title())
                    .color(theme::ACCENT)
                    .size(28.0),
            );
            ui.add_space(theme::Spacing::sm);
            ui.label(
                egui::RichText::new("Write your campaign to disk for later")
                    .color(theme::TEXT_DIM)
                    .size(11.0),
            );
            ui.add_space(theme::Spacing::lg);
        });

        // ── Action row ────────────────────────────────────────
        ui.vertical_centered(|ui| {
            let save_label = if save_index.valid_count() == 0 {
                "Save (no saves yet)"
            } else {
                "Save"
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(save_label).color(theme::ACCENT))
                        .frame(false),
                )
                .clicked()
            {
                save_clicked = true;
            }
            ui.add_space(theme::Spacing::sm);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Save As…").color(theme::TEXT))
                        .frame(false),
                )
                .clicked()
            {
                save_as_target = Some(current_slot_path());
            }
        });

        ui.add_space(theme::Spacing::lg);

        // ── SaveAs slot picker ────────────────────────────────
        ui.label(
            egui::RichText::new("Existing saves")
                .color(theme::TEXT_HINT)
                .size(11.0),
        );
        if save_index.entries.is_empty() {
            ui.label(
                egui::RichText::new("No saves yet — Save As… to create one.")
                    .color(theme::TEXT_DIM)
                    .size(11.0),
            );
        } else {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(theme::Spacing::md as i8))
                .show(ui, |ui| {
                    for entry in save_index.entries.iter() {
                        match entry {
                            SaveSummary::Valid { path, header } => {
                                let label = format!(
                                    "{}  ·  {}",
                                    header.saved_at.clone().unwrap_or_else(|| "?".to_string()),
                                    path.file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_default(),
                                );
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(label).color(theme::ACCENT),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    save_as_target = Some(path.clone());
                                }
                            }
                            SaveSummary::Broken { path, error } => {
                                ui.colored_label(
                                    theme::RED,
                                    format!(
                                        "Broken: {} ({})",
                                        path.file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or_default(),
                                        error,
                                    ),
                                );
                            }
                        }
                    }
                });
        }

        ui.add_space(theme::Spacing::lg);

        ui.label(
            egui::RichText::new(format!(
                "Index last scanned: {}",
                format_instant(save_index_state.last_scanned)
            ))
            .color(theme::TEXT_HINT)
            .size(10.0),
        );

        ui.add_space(theme::Spacing::lg);

        // ── Back row ──────────────────────────────────────────
        ui.horizontal(|ui| {
            if ui
                .button(egui::RichText::new("Back").color(theme::TEXT_DIM))
                .clicked()
            {
                back_clicked = true;
            }
        });
    });

    // ── Post-click state writes ─────────────────────────────
    // Mutate outside the egui closure to avoid `ResMut` borrows held
    // across the closure.

    if save_clicked {
        pending.save_to_current_slot = true;
    }
    if let Some(path) = save_as_target {
        pending.save_as_path = Some(path);
    }
    if back_clicked {
        let next = return_state.previous_state.unwrap_or(LaunchState::MainMenu);
        commands.insert_resource(NextLaunchState::set(next));
    }
}

/// Internal marker: when set, the next consume pass sets
/// [`LaunchState`] to the contained value. Avoids holding a
/// `ResMut<LaunchState>` across the consumer system.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NextLaunchState(pub LaunchState);
impl NextLaunchState {
    pub fn set(s: LaunchState) -> Self {
        Self(s)
    }
}

/// Drain [`PendingSaveActions`], write to disk, and re-scan
/// [`SaveIndex`]. Runs after [`ui_save_panel_subview`] in the same
/// pass so the player's click is committed in-frame.
///
/// Registered in [`register_save_panel_subview`] as an exclusive
/// system (`&mut World`) — Bevy 0.18 forbids holding a `ResMut` and
/// `&mut World` in the same system, so we split the work between
/// the read-only render system and this exclusive consumer.
pub fn consume_save_actions_system(world: &mut World) {
    let has_any = world.resource::<PendingSaveActions>().has_any();
    if !has_any && !world.contains_resource::<NextLaunchState>() {
        return;
    }

    // Drain Save actions first.
    let action: Option<(PendingSaveActionKind, PathBuf)> = {
        let mut pending = world.resource_mut::<PendingSaveActions>();
        if pending.save_as_path.is_some() {
            let path = pending.save_as_path.take().unwrap();
            pending.clear();
            Some((PendingSaveActionKind::SaveAs, path))
        } else if pending.save_to_current_slot {
            let path = current_slot_path();
            pending.clear();
            Some((PendingSaveActionKind::Save, path))
        } else {
            pending.clear();
            None
        }
    };

    if let Some((kind, path)) = action {
        match write_save_to_path(world, &path) {
            Ok(()) => {
                info!("SavePanel: {kind:?} → {p}", p = path.display());
                rescan_save_index(world);
            }
            Err(e) => {
                warn!("SavePanel: write to {p} failed: {e}", p = path.display());
            }
        }
    }

    // Apply any pending LaunchState transition.
    if let Some(next) = world.get_resource::<NextLaunchState>().copied() {
        *world.resource_mut::<LaunchState>() = next.0;
        world.remove_resource::<NextLaunchState>();
    }
}

#[derive(Debug, Clone, Copy)]
enum PendingSaveActionKind {
    Save,
    SaveAs,
}

/// Default slot path for the "Save" (non-As) action.
fn current_slot_path() -> PathBuf {
    resolve_userdata_dir()
        .join(SAVES_SUBDIR)
        .join(CURRENT_SAVE_FILE)
}

/// Format an [`std::time::Instant`] as a short human-readable label.
/// Wall-clock-relative — we render "just now" because the UI's only
/// job is to confirm the index was re-scanned during this session.
fn format_instant(_t: std::time::Instant) -> String {
    "just now".to_string()
}

/// Drain [`PendingInGameSaveRequest`]: capture the current
/// [`LaunchState`] into [`PendingSavePanelReturn::previous_state`]
/// so the Save Panel's Back button returns to where the player was,
/// then advance `LaunchState` to `SaveGame` so the subview renders
/// on the next frame.
///
/// The queue resource is **inserted on demand** by
/// [`crate::ui::dashboard::ui_dashboard`] via
/// `Commands::insert_resource(...)` — so the system here first
/// checks for its presence (`world.get_resource`) before consuming.
/// Removing the resource afterwards is the "consume"; we don't
/// keep a persistent `pending` state on a default resource because
/// that would cost a render-time branch for a write that only
/// happens on click.
///
/// Order: runs *after* [`crate::ui::dashboard::ui_dashboard`]
/// (which writes the queue) and *before*
/// [`ui_save_panel_subview`] (which observes the new
/// `LaunchState`). Exclusive system (`&mut World`) for the same
/// reason as [`consume_save_actions_system`]: writes to two
/// resources without `ResMut` conflict.
pub fn consume_in_game_save_request_system(world: &mut World) {
    let open = world
        .get_resource::<PendingInGameSaveRequest>()
        .map(|r| r.open_panel)
        .unwrap_or(false);
    if !open {
        return;
    }
    let current = *world.resource::<LaunchState>();
    if current != LaunchState::InGame {
        world.remove_resource::<PendingInGameSaveRequest>();
        return;
    }
    world
        .resource_mut::<PendingSavePanelReturn>()
        .previous_state = Some(current);
    *world.resource_mut::<LaunchState>() = LaunchState::SaveGame;
    info!("SavePanel: in-game open request → LaunchState=SaveGame");
    world.remove_resource::<PendingInGameSaveRequest>();
}

/// Register the Save Panel subview render system + consumer in
/// [`LaunchSystemSet::Menu`] on [`EguiPrimaryContextPass`].
pub fn register_save_panel_subview(app: &mut App) {
    app.init_resource::<PendingSavePanelReturn>()
        .init_resource::<PendingSaveActions>()
        .add_systems(
            EguiPrimaryContextPass,
            (
                ui_save_panel_subview.in_set(LaunchSystemSet::Menu),
                consume_save_actions_system.after(ui_save_panel_subview),
                // Run between `ui_dashboard` (which detects the Save
                // button click) and `ui_save_panel_subview` (which
                // observes the new `LaunchState`). We anchor on the
                // visible system handles because `UiSystemSet` itself
                // is private to the ui module and we don't want to
                // expose it just for ordering.
                consume_in_game_save_request_system
                    .after(crate::ui::dashboard::ui_dashboard)
                    .before(ui_save_panel_subview),
            ),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::save_index::SaveHeader;
    use std::env;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("helios-save-panel-{tag}-{pid}-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn install_userdata(tag: &str) -> PathBuf {
        let dir = fresh_dir(tag);
        unsafe {
            std::env::set_var("HELIOS_USERDATA_DIR", &dir);
        }
        dir
    }

    #[test]
    fn pending_save_panel_return_defaults_to_none() {
        let r = PendingSavePanelReturn::default();
        assert!(r.previous_state.is_none());
    }

    #[test]
    fn current_slot_path_is_userdata_saves_current_save_ron() {
        let dir = install_userdata("path");
        let resolved = current_slot_path();
        assert!(resolved.starts_with(&dir));
        assert!(resolved.ends_with(CURRENT_SAVE_FILE));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_panel_save_writes_file_and_rescans_index() {
        let dir = install_userdata("write-save");
        let mut world = World::new();
        world.init_resource::<LaunchState>();
        world.init_resource::<PendingSavePanelReturn>();
        world.insert_resource(SaveIndex::default());
        world.init_resource::<SaveIndexState>();
        world.init_resource::<crate::game_state::GameSeed>();
        world.insert_resource(crate::persistence::playtime::PlaytimeTracker::default());
        world.init_resource::<bevy::prelude::AppTypeRegistry>();

        // Simulate the player's "Save" click by writing directly.
        let path = current_slot_path();
        let res = crate::persistence::write_save_to_path(&world, &path);
        assert!(res.is_ok(), "save must succeed with valid resources");

        // Re-scan; expect the entry to appear.
        crate::persistence::rescan_save_index(&mut world);
        let index = world.resource::<SaveIndex>();
        assert_eq!(index.valid_count(), 1, "SaveIndex must pick up the file");
        assert!(
            index
                .entries
                .iter()
                .any(|e| matches!(e, SaveSummary::Valid { path: p, .. } if p == &path)),
            "valid entry must match the path we wrote"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_index_state_records_rescan() {
        let dir = install_userdata("rescan-stamp");
        let mut world = World::new();
        world.init_resource::<SaveIndexState>();
        world.insert_resource(SaveIndex::default());

        let before = world.resource::<SaveIndexState>().last_scanned;
        // Sleep 5ms to make the stamp actually change.
        std::thread::sleep(std::time::Duration::from_millis(5));
        crate::persistence::rescan_save_index(&mut world);
        let after = world.resource::<SaveIndexState>().last_scanned;
        assert!(after > before, "rescan must advance last_scanned");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_header_default_serialises_round_trip() {
        let header = SaveHeader::default();
        let s = ron::to_string(&header).expect("serialize");
        let back: SaveHeader = ron::from_str(&s).expect("parse");
        assert_eq!(header, back);
    }
}
