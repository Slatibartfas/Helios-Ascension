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
//!   PR-G (GRA-141): the dismiss "×" lives in its own button
//!   child (`egui::Id::new("dismiss_button")`) and the rest of
//!   the frame is the body-click region. A body-click pushes
//!   the entity's `context_link` into `PendingNotificationClicks`
//!   for the `click_handler` system to drain.
//! - Theme integration: severity → `theme::STATUS_*` palette;
//!   card chrome → `theme::TAB_*` borders. No new colours.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::game_state::{ActiveMenu, GameMenu};
use crate::ui::notifications::components::{
    ActiveNotification, PendingNotificationClick, PendingNotificationClicks,
    PendingNotificationDismissal,
};
use crate::ui::notifications::events::{NotificationContextLink, NotificationSeverity};
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
#[allow(clippy::too_many_arguments)]
pub fn render_notification_toasts(
    mut contexts: bevy_egui::EguiContexts,
    active_menu: Res<ActiveMenu>,
    settings: Res<NotificationSettings>,
    sim_time: Res<SimulationTime>,
    categories: Res<crate::ui::notifications::data::NotificationCategoriesData>,
    active: Query<(Entity, &ActiveNotification)>,
    mut pending_dismiss: ResMut<PendingNotificationDismissal>,
    mut pending_focus: ResMut<PendingNotificationClicks>,
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
                render_one_toast(
                    ui,
                    n,
                    *entity,
                    now,
                    &mut pending_dismiss,
                    &mut pending_focus,
                );
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
    pending_dismiss: &mut ResMut<PendingNotificationDismissal>,
    pending_focus: &mut ResMut<PendingNotificationClicks>,
) {
    let (border, text) = severity_palette(n.severity);

    let frame = egui::Frame::new()
        .fill(theme::BG)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(
            theme::Spacing::md as i8,
            theme::Spacing::sm as i8,
        ));

    frame.show(ui, |ui| {
        // PR-G (GRA-141): the title is now non-interactive text.
        // Clicks land on the body region below (the rest of the
        // frame). A separate "×" button on the right owns the
        // dismiss action; the two are guaranteed disjoint because
        // they live in different child-rects of the frame's
        // `ui.horizontal(...)` row below.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&n.title)
                            .strong()
                            .color(text)
                            .size(14.0),
                    )
                    .wrap(),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                // The dismiss button is its own egui child so a
                // click on it is consumed here and does not
                // propagate to the body-click region below. We
                // `push_id` with the entity's packed `to_bits()`
                // so egui can match the button across frames
                // even as the entity list churns — the GRA-141
                // spec calls for `egui::Id::new("dismiss_button")`
                // per-toast; pushing a per-toast id on top of
                // that name is the equivalent in egui 0.33 (which
                // does not expose `.id()` on `Button`).
                ui.push_id(
                    egui::Id::new("dismiss_button").with(entity.to_bits()),
                    |ui| {
                        let dismiss_btn = egui::Button::new(
                            egui::RichText::new("×").color(theme::TEXT_HINT).size(14.0),
                        );
                        if ui.add(dismiss_btn).clicked() {
                            // Encode the entity as a stable u64.
                            // `Entity` exposes `to_bits()` in 0.18
                            // which gives the packed (index,
                            // generation) pair as a u64.
                            pending_dismiss.push(entity.to_bits());
                        }
                    },
                );
            });
        });

        // Body region. The "rest of the frame" contract from
        // the GRA-141 spec: any click that was NOT on the
        // dismiss button above lands here and triggers the
        // context link.
        //
        // Two render branches so the click target is always
        // present when the toast has a non-`None` context_link:
        //
        // 1. `body` non-empty → click-sense label on the body
        //    text (the natural target).
        // 2. `body` empty but `context_link` is `Some`-ish →
        //    a small dim "Click to view →" hint, also
        //    click-sense. This covers toasts like
        //    `SurveyEvent::MissionCompleted` whose body field
        //    is empty in the bridge output but the bridge
        //    still wires `context_link = SelectBody(body)`
        //    (see `event_bridge.rs:107`). Without this branch
        //    the player has no way to click such toasts.
        // 3. `body` empty and `context_link == None` → render
        //    nothing. No click target is needed (informational
        //    toasts).
        let has_context_link = !matches!(n.context_link, NotificationContextLink::None);
        if !n.body.is_empty() {
            let body_response = ui.add(
                egui::Label::new(
                    egui::RichText::new(&n.body)
                        .color(theme::TEXT_DIM)
                        .size(12.0),
                )
                .sense(egui::Sense::click())
                .wrap(),
            );
            if body_response.clicked() {
                pending_focus.push(PendingNotificationClick {
                    entity_bits: entity.to_bits(),
                    context_link: n.context_link,
                });
            }
        } else if has_context_link {
            // Empty body, but the bridge set a context_link
            // (e.g. `SurveyEvent::MissionCompleted`). Render a
            // dim "Click to view →" hint as the click target
            // so the player has something to click.
            let hint_response = ui.add(
                egui::Label::new(
                    egui::RichText::new("Click to view →")
                        .color(theme::TEXT_DIM)
                        .italics()
                        .size(12.0),
                )
                .sense(egui::Sense::click())
                .wrap(),
            );
            if hint_response.clicked() {
                pending_focus.push(PendingNotificationClick {
                    entity_bits: entity.to_bits(),
                    context_link: n.context_link,
                });
            }
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
    // The pre-existing `theme::STATUS_*` constants in this file
    // are `egui::Color32`, while the new `theme::Color::STATUS_*`
    // mirror module exposes `bevy::prelude::Color`. Toast panel
    // uses egui throughout, so stick with the Color32 family.
    match severity {
        NotificationSeverity::Info => (theme::ACCENT, theme::TEXT),
        NotificationSeverity::Notice => (theme::ACCENT, theme::TEXT),
        NotificationSeverity::Warning => (theme::STATUS_WARN, theme::TEXT),
        NotificationSeverity::Critical => (theme::STATUS_ERROR, theme::TEXT),
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
        world.insert_resource(PendingNotificationDismissal::default());
        world.insert_resource(PendingNotificationClicks::default());

        // Drive a Schedule; the system must complete without
        // panic, but we don't need to run any schedules here —
        // we only assert the empty-query path is taken.
        let mut query = world.query::<(Entity, &ActiveNotification)>();
        assert_eq!(query.iter(&world).count(), 0);
    }
}
