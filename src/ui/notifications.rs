//! Toast notification panel — rendered in the top-right corner.
//!
//! Shows notifications categorized by kind with auto-dismiss for info,
//! click-to-pan camera behavior, and acknowledgement for warning/critical.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::game_events::{Notification, NotificationCategory, NotificationKind, NotificationQueue};
use crate::plugins::camera::{CameraAnchor, GameCamera, OrbitCamera};
use crate::plugins::starmap::StarSystemIcon;
use crate::ui::theme;

/// Returns a Unicode icon for each notification category.
fn category_icon(category: NotificationCategory) -> &'static str {
    match category {
        NotificationCategory::Combat => "\u{2694}",       // ⚔ crossed swords
        NotificationCategory::Construction => "\u{1F3D7}", // 🏗 building
        NotificationCategory::Discovery => "\u{1F30D}",    // 🌍 globe
        NotificationCategory::Research => "\u{1F4DA}",     // 📚 books
        NotificationCategory::Resource => "\u{1F4B0}",     // 💰 money bag
        NotificationCategory::Fleet => "\u{2708}",         // ✈ airplane
        NotificationCategory::Diplomacy => "\u{1F54A}",    // 🕊 dove
        NotificationCategory::System => "\u{2699}",         // ⚙ gear
        NotificationCategory::Tutorial => "\u{2139}",      // ℹ info
    }
}

/// Maximum number of visible notifications.
const MAX_VISIBLE: usize = 8;

/// Height of each notification tile.
const TILE_HEIGHT: f32 = 52.0;

/// Corner radius for notification tiles.
const TILE_RADIUS: f32 = 4.0;

/// Duration (seconds) before an info notification auto-dismisses.
const INFO_AUTO_DISMISS_SECS: f64 = 30.0;

pub struct NotificationPanelState {
    /// Id of the notification currently hovered (for highlighting).
    pub hovered_id: Option<u64>,
}

impl Default for NotificationPanelState {
    fn default() -> Self {
        Self { hovered_id: None }
    }
}

/// Render the notification toast panel in the top-right corner.
pub fn ui_notification_panel(
    mut contexts: EguiContexts,
    mut queue: ResMut<NotificationQueue>,
    mut camera_query: Query<&mut CameraAnchor, With<GameCamera>>,
    _orbit_query: Query<&mut OrbitCamera, With<GameCamera>>,
    _star_system_query: Query<(Entity, &StarSystemIcon), With<crate::astronomy::components::Selected>>,
    _bodies_query: Query<&crate::plugins::solar_system::CelestialBody>,
    _panel_state: Local<NotificationPanelState>,
    sim_time: Res<crate::ui::time::SimulationTime>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Don't compete with full-screen panels for attention.
    // Still show notifications on top of the 3D world.
    let available = ctx.available_rect();

    // Panel: top-right anchored, max MAX_VISIBLE entries.
    // Each tile is TILE_HEIGHT tall, plus header.
    let panel_height = (MAX_VISIBLE as f32 * TILE_HEIGHT).min(400.0);

    let panel_rect = egui::Rect::from_min_max(
        egui::pos2(available.max.x - 300.0, available.min.y),
        egui::pos2(available.max.x, available.min.y + panel_height),
    );

    // Collect visible notifications (sorted newest-first, truncated to MAX_VISIBLE).
    let visible: Vec<&Notification> = queue
        .items
        .iter()
        .rev()
        .take(MAX_VISIBLE)
        .collect();

    if visible.is_empty() {
        return;
    }

    // Close button for the panel.
    let header_height = 28.0;

    egui::Area::new("notification_panel".into())
        .fixed_pos(egui::pos2(panel_rect.min.x, panel_rect.min.y))
        .order(egui::Order::TopLayer)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_min_size(egui::vec2(280.0, panel_height));

            // Dark semi-transparent background.
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_premultiplied(8, 13, 26, 230))
                .show(ui, |ui| {
                    // ── Header ───────────────────────────────────────────
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        let label = egui::RichText::new("NOTIFICATIONS")
                            .font(theme::heading())
                            .color(theme::TEXT_DIM);
                        ui.label(label);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let count = queue.items.len();
                            if count > 0 {
                                let badge = egui::RichText::new(format!("{}", count))
                                    .color(theme::ACCENT)
                                    .size(12.0);
                                ui.label(badge);
                            }
                            if ui
                                .button("✕")
                                .on_hover_text("Dismiss all")
                                .clicked()
                            {
                                queue.items.clear();
                            }
                        });
                    });

                    ui.add_space(4.0);
                    theme::divider(ui);

                    // ── Notification tiles ───────────────────────────────
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .id_salt("notification_scroll")
                        .show(ui, |ui| {
                            for notification in visible {
                                let notif_id = notification.id;
                                let kind = notification.kind;
                                let tint = kind.tint();
                                let is_info = matches!(kind, NotificationKind::Info);

                                // Age display for info notifications.
                                let age_suffix = if is_info {
                                    let age = notification.age_seconds();
                                    let remaining = (INFO_AUTO_DISMISS_SECS - age).max(0.0) as i32;
                                    if remaining <= 0 {
                                        continue; // skip rendering expired ones
                                    }
                                    format!(" · {}s", remaining)
                                } else {
                                    String::new()
                                };

                                let tile_height = TILE_HEIGHT;
                                let size = egui::vec2(ui.available_width(), tile_height);
                                let tile_resp = ui.allocate_exact_size(size, egui::Sense::click());
                                let tile_rect = tile_resp.rect;

                                // Background: flash unacknowledged warning/critical.
                                let fill = if !notification.acknowledged && !is_info {
                                    let blink = (sim_time.elapsed_seconds() * 2.0).sin() * 0.5 + 0.5;
                                    let (r, g, b) = (
                                        (13.0 + 40.0 * blink) as u8,
                                        (17.0 + 10.0 * blink) as u8,
                                        (23.0 + 15.0 * blink) as u8,
                                    );
                                    egui::Color32::from_rgb(r, g, b)
                                } else {
                                    theme::SURFACE
                                };

                                // Tint strip on left edge.
                                let strip_rect = egui::Rect::from_min_max(
                                    tile_rect.min,
                                    egui::pos2(tile_rect.min.x + 4.0, tile_rect.max.y),
                                );

                                ui.painter().rect_filled(
                                    tile_rect.expand(1.0),
                                    TILE_RADIUS,
                                    fill,
                                );
                                ui.painter().rect_stroke(
                                    tile_rect.expand(1.0),
                                    TILE_RADIUS,
                                    egui::Stroke::new(1.0, tint.linear_multiply(0.4)),
                                    egui::StrokeKind::Outside,
                                );
                                ui.painter().rect_filled(strip_rect, TILE_RADIUS, tint.linear_multiply(0.6));

                                // Title with category icon.
                                let icon = category_icon(notification.category);
                                let title_text = egui::RichText::new(format!("{} {}", icon, notification.title))
                                    .size(13.0)
                                    .color(theme::TEXT)
                                    .strong();
                                let body_text = egui::RichText::new(format!(
                                    "{}{}",
                                    notification.body, age_suffix
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM);

                                let text_rect = tile_rect.shrink2(egui::vec2(14.0, 4.0));
                                let title_str = format!("{} {}", icon, notification.title);
                                ui.painter().text(
                                    egui::pos2(text_rect.min.x, text_rect.min.y + 2.0),
                                    egui::Align2::LEFT_TOP,
                                    title_str.as_str(),
                                    egui::FontId::proportional(13.0),
                                    tint,
                                );
                                ui.painter().text(
                                    egui::pos2(text_rect.min.x, text_rect.min.y + 18.0),
                                    egui::Align2::LEFT_TOP,
                                    format!("{}{}", notification.body, age_suffix).as_str(),
                                    egui::FontId::proportional(11.0),
                                    theme::TEXT_DIM,
                                );

                                // Kind badge.
                                let badge_rect = egui::Rect::from_min_max(
                                    egui::pos2(tile_rect.max.x - 70.0, tile_rect.min.y + 4.0),
                                    egui::pos2(tile_rect.max.x - 6.0, tile_rect.min.y + 18.0),
                                );
                                ui.painter().rect_filled(
                                    badge_rect,
                                    2.0,
                                    tint.linear_multiply(0.2),
                                );
                                ui.painter().text(
                                    badge_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    kind.label(),
                                    egui::FontId::proportional(9.0),
                                    tint,
                                );

                                // Click handling.
                                if tile_resp.response.clicked() {
                                    // Acknowledge.
                                    queue.acknowledge(notif_id);

                                    // Pan camera to entity if present.
                                    if let Some(target_entity) = notification.entity {
                                        // Find what system the entity is in.
                                        // For now, just set anchor to the entity directly.
                                        if let Ok(mut anchor) = camera_query.single_mut() {
                                            anchor.0 = Some(target_entity);
                                        }
                                        // Exit any starmap mode to system view.
                                        // TODO: use ViewMode if available
                                    }
                                }

                                // Acknowledge button (✕ per tile).
                                let ack_btn_rect = egui::Rect::from_min_max(
                                    egui::pos2(tile_rect.max.x - 22.0, tile_rect.max.y - 18.0),
                                    egui::pos2(tile_rect.max.x - 6.0, tile_rect.max.y - 6.0),
                                );
                                let ack_btn = egui::Button::new(
                                    egui::RichText::new("✕").size(10.0).color(theme::TEXT_HINT),
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE);

                                if ui
                                    .put(ack_btn_rect, ack_btn)
                                    .on_hover_text("Dismiss")
                                    .clicked()
                                {
                                    queue.remove(notif_id);
                                }
                            }
                        });
                });
        });
}