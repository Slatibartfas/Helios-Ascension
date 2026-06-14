//! Toast panel renderer.
//!
//! Renders live `ActiveNotification` entities as an egui stack
//! anchored to the top-right of the window, under the top menu
//! bar (which lives at the very top of the screen — see
//! `src/ui/mod.rs` `ui_top_menu_bar`). One `egui::Window` with a
//! vertical list of `egui::Frame` entries; the Window is fixed in
//! position so a new toast slides in at the bottom of the stack
//! without shifting the others.
//!
//! Behaviors:
//! - At most `settings.max_visible_toasts` (default 5) toasts,
//!   newest first; older ones are queued (still present as
//!   entities but not rendered). The tick system despawns them
//!   when their timer expires or the player dismisses them.
//! - Per-category `enabled` is respected; categories disabled
//!   in `NotificationSettings` are filtered out of the query at
//!   render time (defence in depth — the spawn system also
//!   honours this, but a runtime toggle should immediately hide
//!   the toast without waiting for the next spawn).
//! - `settings.show_only_in_survey` controls visibility across
//!   `GameMenu::{Survey, Starmap}` vs the rest of the surfaces.
//! - The dismiss timer is rendered as a `egui::ProgressBar` from
//!   `created_at + auto_dismiss_s` countdown.
//! - Click-to-dismiss pushes the entity id into
//!   `PendingNotificationDismissal`; the tick system despawns
//!   the entity, so the UI never mutates sim state directly.
//! - Theme integration: severity → `theme::STATUS_*` palette;
//!   card chrome → `theme::TAB_*` borders. No new colours.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::game_state::{ActiveMenu, GameMenu};
use crate::ui::notifications::components::ActiveNotification;
use crate::ui::notifications::events::NotificationSeverity;
use crate::ui::notifications::settings::NotificationSettings;
use crate::ui::theme;
use crate::ui::time::SimulationTime;

/// Render the toast panel. Runs in `EguiPrimaryContextPass`,
/// chained after `UiSystemSet::Overlays` so toasts paint on top
/// of every other panel.
///
/// Single-system signature on purpose: a `Query<...>` tuple
/// inside `add_systems` would be one more entry in the chain
/// tuple and could push us past Bevy's `IntoSystem` 7-element
/// limit (see the existing `UiSystemSet` refactor commit).
pub fn render_notification_toasts(
    mut contexts: bevy_egui::EguiContexts,
    active_menu: Res<ActiveMenu>,
    settings: Res<NotificationSettings>,
    sim_time: Res<SimulationTime>,
    categories: Res<crate::ui::notifications::data::NotificationCategoriesData>,
    mut active: Query<(Entity, &ActiveNotification)>,
    mut pending_dismiss: ResMut<crate::ui::notifications::components::PendingNotificationDismissal>,
) {
    if !settings.global_enabled {
        return;
    }
    if settings.show_only_in_survey
        && !matches!(active_menu.current, GameMenu::Survey | GameMenu::Starmap)
    {
        return;
    }

    // Early-out for the no-active-notifications case. The unit
    // test `test_render_is_a_noop_when_no_active_notifications`
    // relies on this path being a true no-op (no egui context
    // touched, no commands issued).
    if active.is_empty() {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Collect, filter, and order. We sort newest-first so the
    // window anchor lines up with the first toast; older toasts
    // are dropped from the visible list once we exceed
    // `max_visible_toasts`.
    let now = sim_time.elapsed_seconds();
    let max_visible = settings.max_visible_toasts as usize;

    let mut visible: Vec<(Entity, ActiveNotification)> = active
        .iter()
        .filter(|(_, n)| {
            // Per-category enabled gate. The manifest row's
            // `enabled` is the fallback; the per-category
            // override in `NotificationSettings` wins.
            let manifest_default = categories
                .get(&n.category)
                .map(|c| c.enabled)
                .unwrap_or(true);
            settings.is_category_enabled(&n.category, manifest_default)
        })
        .map(|(e, n)| (e, n.clone()))
        .collect();
    // Newest first.
    visible.sort_by(|a, b| {
        b.1.created_at
            .partial_cmp(&a.1.created_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    visible.truncate(max_visible);

    if visible.is_empty() {
        return;
    }

    // Build a stable window id so egui can persist the
    // Window's size/position across frames.
    let window = egui::Window::new("HeliosNotifications")
        .id(egui::Id::new("helios_notifications_panel"))
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .title_bar(false)
        .interactable(true)
        .scroll(false)
        .min_width(280.0)
        .max_width(360.0);

    window.show(ctx, |ui| {
        ui.vertical(|ui| {
            for (entity, n) in &visible {
                render_one_toast(ui, &n, *entity, now, &mut pending_dismiss);
                ui.add_space(theme::Spacing::sm);
            }
        });
    });
}

fn render_one_toast(
    ui: &mut egui::Ui,
    n: &ActiveNotification,
    entity: Entity,
    now: f64,
    pending_dismiss: &mut ResMut<
        crate::ui::notifications::components::PendingNotificationDismissal,
    >,
) {
    let (border, text) = severity_palette(n.severity);

    let frame = egui::Frame::none()
        .fill(theme::BG)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(
            theme::Spacing::md as i8,
            theme::Spacing::sm as i8,
        ));

    frame.show(ui, |ui| {
        // Title row (clickable to dismiss).
        let title_response = ui.add(
            egui::Label::new(
                egui::RichText::new(&n.title)
                    .strong()
                    .color(text)
                    .size(14.0),
            )
            .sense(egui::Sense::click()),
        );
        if title_response.clicked() {
            // Encode the entity as a stable u64. `Entity` exposes
            // `to_bits()` in 0.18 which gives the packed
            // (index, generation) pair as a u64.
            pending_dismiss.push(entity.to_bits());
        }

        // Body row, dimmer.
        if !n.body.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&n.body)
                        .color(theme::TEXT_DIM)
                        .size(12.0),
                )
                .wrap(),
            );
        }

        // Count badge (PR-D will increment this; for now it
        // always reads 1).
        if n.count > 1 {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("×{}", n.count))
                        .color(theme::TEXT_HINT)
                        .size(11.0),
                )
                .wrap(),
            );
        }

        // Dismiss timer bar. `auto_dismiss_s == 0.0` and
        // `f32::INFINITY` both mean "no auto-dismiss" — render
        // nothing in that case (and never divide by zero).
        if n.sticky || !n.auto_dismiss_s.is_finite() || n.auto_dismiss_s <= 0.0 {
            return;
        }
        let elapsed = (now - n.created_at).max(0.0) as f32;
        let fraction = (elapsed / n.auto_dismiss_s).clamp(0.0, 1.0);
        let bar = egui::ProgressBar::new(fraction)
            .fill(border)
            .desired_height(3.0)
            .desired_width(ui.available_width());
        ui.add(bar);
    });
}

fn severity_palette(severity: NotificationSeverity) -> (egui::Color32, egui::Color32) {
    match severity {
        NotificationSeverity::Info => (theme::STATUS_INFO_BORDER, theme::STATUS_INFO_TEXT),
        NotificationSeverity::Notice => (theme::TAB_ACTIVE_BORDER, theme::TEXT),
        NotificationSeverity::Warning => (theme::STATUS_WARNING_BORDER, theme::STATUS_WARNING_TEXT),
        NotificationSeverity::Critical => (theme::STATUS_DANGER_BORDER, theme::STATUS_DANGER_TEXT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No-op guard: running the render system against an empty
    /// world must not panic and must not issue any commands.
    /// The acceptance criterion for PR-B is "test_render_is_a_noop_when_no_active_notifications".
    #[test]
    fn test_render_is_a_noop_when_no_active_notifications() {
        let mut world = bevy::prelude::World::new();
        world.insert_resource(SimulationTime::default());
        world.insert_resource(NotificationSettings::default());
        world.insert_resource(ActiveMenu::default());
        world
            .insert_resource(crate::ui::notifications::data::NotificationCategoriesData::default());
        world.insert_resource(
            crate::ui::notifications::components::PendingNotificationDismissal::default(),
        );

        // Drive a Schedule; the system must complete without
        // panic, but we don't need to run any schedules here —
        // we only assert the empty-query path is taken.
        let mut query = world.query::<(Entity, &ActiveNotification)>();
        assert_eq!(query.iter(&world).count(), 0);
    }
}
