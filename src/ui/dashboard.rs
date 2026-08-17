use super::dossier_panel::{paint_resource_tile, ResourceTileDisplay};
use super::*;
use crate::astronomy::components::FloatingOrigin;
use crate::plugins::solar_system_data::AsteroidClass;
use std::cell::RefCell;
use std::collections::HashMap;

fn render_selectable_label(ui: &mut egui::Ui, is_selected: bool, name: &str) -> egui::Response {
    let desired_size = egui::vec2(ui.available_width(), 24.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    let fill = if response.hovered() {
        theme::SURFACE_RAISED
    } else if is_selected {
        theme::SURFACE
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke = if response.hovered() {
        egui::Stroke::new(1.0_f32, theme::CYAN)
    } else if is_selected {
        egui::Stroke::new(1.0_f32, theme::ACCENT_DIM)
    } else {
        egui::Stroke::NONE
    };
    let text_color = if response.hovered() || is_selected {
        theme::CYAN
    } else {
        theme::TEXT
    };

    let row_rect = rect.shrink2(egui::vec2(0.0, 1.0));
    ui.painter()
        .rect(row_rect, 3.0, fill, stroke, egui::StrokeKind::Outside);
    ui.painter().text(
        row_rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        theme::body(14.0),
        text_color,
    );
    // Keyboard-focus ring (Tab/Shift+Tab cycles through the body ledger)
    theme::paint_focus_ring(ui.painter(), row_rect, response.has_focus());

    response
}

fn render_group_header(
    ui: &mut egui::Ui,
    label: &str,
    count: usize,
    is_open: bool,
    color: egui::Color32,
) -> egui::Response {
    let text = format!("{} ({})", label, count);
    let desired_size = egui::vec2(ui.available_width(), 22.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    let fill = if response.hovered() || is_open {
        theme::SURFACE
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke = if response.hovered() {
        egui::Stroke::new(1.0_f32, theme::ACCENT_DIM)
    } else if is_open {
        egui::Stroke::new(1.0_f32, theme::BORDER)
    } else {
        egui::Stroke::NONE
    };
    let row_rect = rect.shrink2(egui::vec2(0.0, 1.0));
    ui.painter()
        .rect(row_rect, 3.0, fill, stroke, egui::StrokeKind::Outside);
    ui.painter().text(
        row_rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        theme::heading(),
        color,
    );

    response
}

fn focus_remote_fleet_system(
    target_system_id: usize,
    navigation: &mut ParamSet<(ResMut<CurrentStarSystem>, ResMut<FloatingOrigin>)>,
    star_system_query: &Query<(Entity, &StarSystemIcon, Option<&SelectedStarSystem>)>,
    orbit_query: &mut Query<&mut OrbitCamera, With<GameCamera>>,
    commands: &mut Commands,
) {
    if target_system_id == navigation.p0().0 {
        return;
    }

    let Some((_, icon, _)) = star_system_query
        .iter()
        .find(|(_, icon, _)| icon.id == target_system_id)
    else {
        return;
    };

    navigation.p0().0 = target_system_id;
    navigation.p1().position = icon.position;

    for (entity, _, selected) in star_system_query.iter() {
        if selected.is_some() {
            commands.entity(entity).remove::<SelectedStarSystem>();
        }
    }

    if let Ok(mut orbit_camera) = orbit_query.single_mut() {
        orbit_camera.pan_offset = Vec3::ZERO;
    }
}

/// Returns a Unicode icon for each body type to distinguish entries in the ledger
fn body_type_icon(body_type: &BodyType) -> &'static str {
    match body_type {
        BodyType::Star => "\u{2605}",        // ★
        BodyType::Planet => "\u{25CF}",      // ●
        BodyType::Moon => "\u{25D1}",        // ◑
        BodyType::DwarfPlanet => "\u{25CC}", // ◌
        BodyType::Asteroid => "\u{25C6}",    // ◆
        BodyType::Comet => "\u{2604}",       // ☄
        BodyType::GasGiant => "\u{25C9}",    // ◉
        BodyType::Ring => "\u{25CB}",        // ○
    }
}

/// Format asteroid class for display (short form for ledger)
fn format_asteroid_type_short(class: AsteroidClass) -> String {
    match class {
        AsteroidClass::SType => "S-Type".to_string(),
        AsteroidClass::CType => "C-Type".to_string(),
        AsteroidClass::MType => "M-Type".to_string(),
        AsteroidClass::VType => "V-Type".to_string(),
        AsteroidClass::DType => "D-Type".to_string(),
        AsteroidClass::PType => "P-Type".to_string(),
        AsteroidClass::Unknown => "?".to_string(),
    }
}

fn render_body_row(
    ui: &mut egui::Ui,
    entity: Entity,
    body: &CelestialBody,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchored_entity: Option<Entity>,
    pending_anchor: &RefCell<Option<Entity>>,
) {
    let is_selected = selection.is_selected(entity);
    let is_anchored = anchored_entity == Some(entity);
    let type_icon = body_type_icon(&body.body_type);
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        // Show anchor indicator (⚓) when anchored - not clickable, just informational
        if is_anchored {
            ui.add(egui::Label::new(
                egui::RichText::new("⚓").color(theme::ANCHOR),
            ));
        } else {
            ui.add_space(20.0); // Keep consistent spacing
        }

        // Use a visually distinct style for selected items
        // For asteroids, include the spectral type in the name
        let display_name = if body.body_type == BodyType::Asteroid {
            if let Some(class) = body.asteroid_class {
                format!(
                    "{} {} [{}]",
                    type_icon,
                    body.name,
                    format_asteroid_type_short(class)
                )
            } else {
                format!("{} {}", type_icon, body.name)
            }
        } else {
            format!("{} {}", type_icon, body.name)
        };
        let response = render_selectable_label(ui, is_selected, &display_name);

        // Single click: select only
        if response.clicked() {
            // Fire RowSelect on every body selection (single click).
            // The PendingSfxRequests resource is drained by
            // `drain_collector_into_ui_sfx` next frame; queued from
            // the existing `commands` param so this system stays at
            // its 16-param InSet-system cap.
            commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                crate::plugins::sfx::SfxCueId::RowSelect,
            ]));
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            commands.entity(entity).insert(Selected);
            selection.select(entity);
        }

        // Double click: select AND anchor
        if response.double_clicked() {
            // Use ModalConfirm (the louder of the modal cues) to
            // signal "this body is now the anchor".
            commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                crate::plugins::sfx::SfxCueId::ModalConfirm,
            ]));
            // First select
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            commands.entity(entity).insert(Selected);
            selection.select(entity);

            // Then set pending anchor (to be applied after UI)
            *pending_anchor.borrow_mut() = Some(entity);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_grouped_children(
    ui: &mut egui::Ui,
    children: &[Entity],
    group_name: &str,
    parent_entity: Entity,
    body_map: &std::collections::HashMap<Entity, &CelestialBody>,
    hierarchy: &std::collections::HashMap<Entity, Vec<Entity>>,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchored_entity: Option<Entity>,
    pending_anchor: &RefCell<Option<Entity>>,
    expanded_groups: &mut std::collections::HashSet<(Entity, String)>,
) {
    if children.is_empty() {
        return;
    }

    // Make ID unique by including parent entity to avoid UI jumping bug
    let id = ui.make_persistent_id((group_name, parent_entity));
    let state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let is_open = state.is_open();
    // Record expansion state so orbit visibility can be driven from the ledger.
    if is_open {
        expanded_groups.insert((parent_entity, group_name.to_string()));
    }
    let mut row_clicked = false;
    let mut header = state.show_header(ui, |ui| {
        row_clicked =
            render_group_header(ui, group_name, children.len(), is_open, theme::TEXT_DIM).clicked();
    });
    if row_clicked {
        // GRA-SFX-3b: group toggle in body ledger. The expand /
        // collapse is a discrete visual transition, so the
        // PanelOpen / PanelClose pair fits. Pick based on the
        // pre-toggle state (stored at the top of this function
        // as `is_open`).
        commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![if is_open {
            crate::plugins::sfx::SfxCueId::PanelClose
        } else {
            crate::plugins::sfx::SfxCueId::PanelOpen
        }]));
        header.toggle();
    }
    header.body(|ui| {
        for &child_entity in children {
            // Use render_body_tree so bodies with children (e.g. Pluto → Charon)
            // are expanded recursively rather than shown as a flat row.
            render_body_tree(
                ui,
                child_entity,
                body_map,
                hierarchy,
                selection,
                commands,
                selected_query,
                anchored_entity,
                pending_anchor,
                expanded_groups,
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_body_tree(
    ui: &mut egui::Ui,
    entity: Entity,
    body_map: &std::collections::HashMap<Entity, &CelestialBody>,
    hierarchy: &std::collections::HashMap<Entity, Vec<Entity>>,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchored_entity: Option<Entity>,
    pending_anchor: &RefCell<Option<Entity>>,
    expanded_groups: &mut std::collections::HashSet<(Entity, String)>,
) {
    if let Some(body) = body_map.get(&entity) {
        let is_selected = selection.is_selected(entity);
        let is_anchored = anchored_entity == Some(entity);
        let id = ui.make_persistent_id(entity);

        // Group children by type
        let mut child_rings = Vec::new();
        let mut child_planets = Vec::new();
        let mut child_moons = Vec::new(); // Usually planets have moons
        let mut child_asteroids = Vec::new();
        let mut child_comets = Vec::new();
        let mut child_dwarf_planets = Vec::new();
        let mut child_others = Vec::new();

        let has_children = if let Some(children) = hierarchy.get(&entity) {
            for &child in children {
                if let Some(child_body) = body_map.get(&child) {
                    match child_body.body_type {
                        BodyType::Ring => child_rings.push(child),
                        BodyType::Planet | BodyType::GasGiant => child_planets.push(child),
                        BodyType::Moon => child_moons.push(child),
                        BodyType::Asteroid => child_asteroids.push(child),
                        BodyType::Comet => child_comets.push(child),
                        BodyType::DwarfPlanet => child_dwarf_planets.push(child),
                        _ => child_others.push(child),
                    }
                }
            }
            true
        } else {
            false
        };

        if has_children {
            let mut row_clicked = false;
            let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                body.body_type == BodyType::Star,
            )
            .show_header(ui, |ui| {
                // Show anchor indicator (⚓) when anchored - not clickable, just informational
                if is_anchored {
                    ui.add(egui::Label::new(
                        egui::RichText::new("⚓").color(theme::ANCHOR),
                    ));
                } else {
                    ui.add_space(20.0); // Keep consistent spacing
                }

                // Use a visually distinct style for selected items
                // For asteroids, include the spectral type in the name
                let type_icon = body_type_icon(&body.body_type);
                let display_name = if body.body_type == BodyType::Asteroid {
                    if let Some(class) = body.asteroid_class {
                        format!(
                            "{} {} [{}]",
                            type_icon,
                            body.name,
                            format_asteroid_type_short(class)
                        )
                    } else {
                        format!("{} {}", type_icon, body.name)
                    }
                } else {
                    format!("{} {}", type_icon, body.name)
                };
                let response = render_selectable_label(ui, is_selected, &display_name);
                row_clicked = response.clicked();

                // Single click: select only
                if response.clicked() {
                    // GRA-SFX-3b: row selection in nested body tree.
                    // Same RowSelect cue as the flat render_body_row
                    // sibling for consistency.
                    commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                        crate::plugins::sfx::SfxCueId::RowSelect,
                    ]));
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);
                }

                // Double click: select AND anchor
                if response.double_clicked() {
                    // GRA-SFX-3b: anchor switch from nested-tree
                    // double-click. Same ModalConfirm cue as the
                    // flat-row path for player consistency.
                    commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                        crate::plugins::sfx::SfxCueId::ModalConfirm,
                    ]));
                    // First select
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);

                    // Then set pending anchor (to be applied after UI)
                    *pending_anchor.borrow_mut() = Some(entity);
                }
            });
            if row_clicked {
                header.toggle();
            }
            header.body(|ui| {
                // 0. Rings — shown first so they are never buried under 46+ moons
                for child in child_rings {
                    render_body_tree(
                        ui,
                        child,
                        body_map,
                        hierarchy,
                        selection,
                        commands,
                        selected_query,
                        anchored_entity,
                        pending_anchor,
                        expanded_groups,
                    );
                }
                // 1. Planets (Recursive)
                for child in child_planets {
                    render_body_tree(
                        ui,
                        child,
                        body_map,
                        hierarchy,
                        selection,
                        commands,
                        selected_query,
                        anchored_entity,
                        pending_anchor,
                        expanded_groups,
                    );
                }
                // 2. Dwarf Planets (Grouped, recursive so moons like Charon are shown)
                render_grouped_children(
                    ui,
                    &child_dwarf_planets,
                    "Dwarf Planets",
                    entity,
                    body_map,
                    hierarchy,
                    selection,
                    commands,
                    selected_query,
                    anchored_entity,
                    pending_anchor,
                    expanded_groups,
                );
                // 3. Moons — listed directly under the parent (no collapsible group)
                for child in child_moons {
                    render_body_tree(
                        ui,
                        child,
                        body_map,
                        hierarchy,
                        selection,
                        commands,
                        selected_query,
                        anchored_entity,
                        pending_anchor,
                        expanded_groups,
                    );
                }
                // 4. Asteroids
                render_grouped_children(
                    ui,
                    &child_asteroids,
                    "Asteroids",
                    entity,
                    body_map,
                    hierarchy,
                    selection,
                    commands,
                    selected_query,
                    anchored_entity,
                    pending_anchor,
                    expanded_groups,
                );
                // 5. Comets
                render_grouped_children(
                    ui,
                    &child_comets,
                    "Comets",
                    entity,
                    body_map,
                    hierarchy,
                    selection,
                    commands,
                    selected_query,
                    anchored_entity,
                    pending_anchor,
                    expanded_groups,
                );
                // 6. Others
                for child in child_others {
                    render_body_tree(
                        ui,
                        child,
                        body_map,
                        hierarchy,
                        selection,
                        commands,
                        selected_query,
                        anchored_entity,
                        pending_anchor,
                        expanded_groups,
                    );
                }
            });
        } else {
            render_body_row(
                ui,
                entity,
                body,
                selection,
                commands,
                selected_query,
                anchored_entity,
                pending_anchor,
            );
        }
    }
}

/// Render fleet rows inside the left ledger "Fleets" collapsible section.
///
/// Each row shows status icon, fleet name, and current location.
/// Clicking a row selects/deselects the fleet and clears any body selection.
#[allow(clippy::too_many_arguments)]
fn render_fleet_ledger_tree(
    ui: &mut egui::Ui,
    fleet_query: &Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    body_lookup: &std::collections::HashMap<Entity, (&CelestialBody, usize)>,
    fleet_ui_state: &mut FleetUiState,
    selected_query: &Query<Entity, With<Selected>>,
    commands: &mut Commands,
    selection: &mut Selection,
    elapsed: f64,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
    orbit_query: &mut Query<&mut OrbitCamera, With<GameCamera>>,
    navigation: &mut ParamSet<(ResMut<CurrentStarSystem>, ResMut<FloatingOrigin>)>,
    star_system_query: &Query<(Entity, &StarSystemIcon, Option<&SelectedStarSystem>)>,
    sim_time: &SimulationTime,
) {
    let mut fleets: Vec<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)> =
        fleet_query.iter().collect();
    fleets.sort_by(|a, b| a.1.name.cmp(&b.1.name));

    if fleets.is_empty() {
        ui.label(
            egui::RichText::new("  No fleets deployed")
                .size(12.0)
                .color(theme::TEXT_DIM)
                .italics(),
        );
        return;
    }

    for (entity, fleet, maybe_orbit, maybe_maneuver) in fleets {
        let is_selected = fleet_ui_state.selected_fleet == Some(entity);
        let in_transit = maybe_maneuver.is_some();

        let status_icon = if in_transit { "✈" } else { "🛰" };
        let display_name = format!("{} {}", status_icon, fleet.name);

        // Colour scheme matching the fleet-menu panel list.
        let row_color = if is_selected {
            theme::CYAN
        } else if in_transit {
            theme::RP_BLUE
        } else {
            theme::EP_TEAL
        };

        let sub_status = if let Some(maneuver) = maybe_maneuver {
            if elapsed < maneuver.departure_time {
                "⏳ Waiting to depart".to_string()
            } else {
                "↗ In transit".to_string()
            }
        } else if let Some(orbit) = maybe_orbit {
            let body = body_lookup.get(&orbit.body).copied();
            let body_name = body.map(|(b, _)| b.name.as_str()).unwrap_or("?");
            // Show a distinct label for heliocentric Lagrange-point orbits.
            if body.map(|(b, _)| b.body_type) == Some(BodyType::Star) {
                format!("✦ Lagrange Orbit ({body_name})")
            } else {
                format!("⊙ Orbiting {body_name}")
            }
        } else {
            "Location unknown".to_string()
        };

        let ships_txt = format!(
            "{} {}",
            fleet.ships.len(),
            if fleet.ships.len() == 1 {
                "ship"
            } else {
                "ships"
            }
        );

        // ── Clickable fleet-name row with themed background + marquee ─────
        {
            let font_id = egui::FontId::proportional(13.0);
            let available_w = ui.available_width().max(1.0);
            let row_height = ui.text_style_height(&egui::TextStyle::Body);
            let full_text_w = ui
                .painter()
                .layout_no_wrap(display_name.clone(), font_id.clone(), row_color)
                .size()
                .x;

            let (rect, resp) = ui.allocate_exact_size(
                egui::Vec2::new(available_w, row_height),
                egui::Sense::click(),
            );

            // Selection / hover background — identical to fleet-panel list rows.
            let rounding = egui::CornerRadius::same(3);
            if is_selected {
                ui.painter()
                    .rect_filled(rect.expand(1.0), rounding, theme::BUTTON_ACTIVE_BG);
                ui.painter().rect_stroke(
                    rect.expand(1.0),
                    rounding,
                    egui::Stroke::new(1.0_f32, theme::CYAN),
                    egui::StrokeKind::Inside,
                );
            } else if resp.hovered() {
                ui.painter()
                    .rect_filled(rect.expand(1.0), rounding, theme::SURFACE_RAISED);
                ui.painter().rect_stroke(
                    rect.expand(1.0),
                    rounding,
                    egui::Stroke::new(1.0_f32, theme::BORDER),
                    egui::StrokeKind::Inside,
                );
            }

            // Draw text (marquee when too wide).
            let text_rect = rect.shrink2(egui::Vec2::new(4.0, 0.0));
            if full_text_w <= text_rect.width() {
                ui.painter().text(
                    text_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    &display_name,
                    font_id,
                    row_color,
                );
            } else {
                // Continuous marquee: two copies loop seamlessly.
                let gap = 48.0_f32;
                let cycle = full_text_w + gap;
                let speed = 40.0_f64;
                let t = ui.ctx().input(|i| i.time);
                let offset_x = ((t * speed) % cycle as f64) as f32;
                let painter = ui.painter().with_clip_rect(text_rect);
                let galley =
                    painter.layout_no_wrap(display_name.clone(), font_id.clone(), row_color);
                let y = text_rect.top() + (text_rect.height() - galley.size().y) * 0.5;
                let x0 = text_rect.left() - offset_x;
                painter.galley(egui::pos2(x0, y), galley.clone(), row_color);
                let x1 = x0 + cycle;
                if x1 < text_rect.right() + full_text_w {
                    painter.galley(egui::pos2(x1, y), galley, row_color);
                }
                ui.ctx().request_repaint();
            }

            // Click handling.
            if resp.clicked() {
                // GRA-SFX-3b: fleet-row click selects/deselects a
                // fleet. Use RowSelect because the action is
                // parallel to a body-row click — both are
                // "select this object in the ledger".
                commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                    crate::plugins::sfx::SfxCueId::RowSelect,
                ]));
                if is_selected {
                    fleet_ui_state.selected_fleet = None;
                } else {
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    selection.clear();
                    fleet_ui_state.selected_fleet = Some(entity);
                    fleet_ui_state.clear_target();
                }
            }

            if resp.double_clicked() {
                for e in selected_query.iter() {
                    commands.entity(e).remove::<Selected>();
                }
                selection.clear();
                fleet_ui_state.selected_fleet = Some(entity);
                fleet_ui_state.clear_target();

                let target = if let Some(orbit) = maybe_orbit {
                    body_lookup
                        .get(&orbit.body)
                        .map(|(_, system_id)| (orbit.body, *system_id))
                } else if let Some(maneuver) = maybe_maneuver {
                    let focus_body = if elapsed < maneuver.departure_time {
                        maneuver.origin_body
                    } else {
                        maneuver.destination_body
                    };
                    body_lookup
                        .get(&focus_body)
                        .map(|(_, system_id)| (entity, *system_id))
                } else {
                    None
                };

                if let Some((anchor_target, target_system_id)) = target {
                    focus_remote_fleet_system(
                        target_system_id,
                        navigation,
                        star_system_query,
                        orbit_query,
                        commands,
                    );

                    if let Ok(mut anchor) = anchor_query.single_mut() {
                        anchor.0 = Some(anchor_target);
                    }
                }
            }
        }

        // Sub-status line — marquee-scrolled when too narrow.
        {
            let sub_full = format!("  {sub_status}  {ships_txt}");
            let sub_color = if in_transit {
                theme::RP_BLUE
            } else {
                theme::TEXT_DIM
            };
            super::fleets_panel::render_marquee_line(
                ui,
                &sub_full,
                sub_color,
                egui::FontId::proportional(10.0),
            );
        }

        // Progress bar + ETA for actively-transiting fleets
        if let Some(maneuver) = maybe_maneuver {
            if elapsed >= maneuver.departure_time {
                let progress = maneuver.progress(elapsed) as f32;
                let eta_str = sim_time.format_arrival_date(maneuver.arrival_time);
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .text(format!("{:.0}%", progress * 100.0))
                            .desired_width(110.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("ETA {eta_str}"))
                            .size(10.0)
                            .color(theme::RP_BLUE),
                    );
                });
            }
        }
    }
}

/// Compact mass format for tight UI tiles — no space before unit, single decimal.
/// Input is in MEGATONS and preserves the long-standing UI semantics used
/// across colony, dossier, and economy panels.
pub(super) fn format_mass_compact(megatons: f64) -> String {
    let abs_val = megatons.abs();
    if abs_val < 1e-9 {
        return "0".to_string();
    }
    if abs_val < 0.001 {
        let tonnes = megatons * 1_000_000.0;
        if tonnes.abs() < 10.0 {
            format!("{:.1}t", tonnes)
        } else {
            format!("{:.0}t", tonnes)
        }
    } else if abs_val < 1.0 {
        let kt = megatons * 1000.0;
        if kt.abs() < 10.0 {
            format!("{:.1}kt", kt)
        } else {
            format!("{:.0}kt", kt)
        }
    } else if abs_val < 1000.0 {
        if abs_val < 10.0 {
            format!("{:.1}Mt", megatons)
        } else {
            format!("{:.0}Mt", megatons)
        }
    } else if abs_val < 1_000_000.0 {
        let gt = megatons / 1000.0;
        if gt.abs() < 10.0 {
            format!("{:.1}Gt", gt)
        } else {
            format!("{:.0}Gt", gt)
        }
    } else if abs_val < 1_000_000_000.0 {
        let tt = megatons / 1_000_000.0;
        if tt.abs() < 10.0 {
            format!("{:.1}Tt", tt)
        } else {
            format!("{:.0}Tt", tt)
        }
    } else if abs_val < 1_000_000_000_000.0 {
        let pt = megatons / 1_000_000_000.0;
        if pt.abs() < 10.0 {
            format!("{:.1}Pt", pt)
        } else {
            format!("{:.0}Pt", pt)
        }
    } else {
        let et = megatons / 1_000_000_000_000.0;
        if et.abs() < 10.0 {
            format!("{:.1}Et", et)
        } else {
            format!("{:.0}Et", et)
        }
    }
}

pub(super) fn format_mass_compact_tonnes(tonnes: f64) -> String {
    let abs_val = tonnes.abs();
    if abs_val < 1e-9 {
        return "0".to_string();
    }
    if abs_val < 0.001 {
        let g = tonnes * 1_000_000.0;
        if g.abs() < 10.0 {
            format!("{:.1}g", g)
        } else {
            format!("{:.0}g", g)
        }
    } else if abs_val < 1.0 {
        let kg = tonnes * 1000.0;
        if kg.abs() < 10.0 {
            format!("{:.1}kg", kg)
        } else {
            format!("{:.0}kg", kg)
        }
    } else if abs_val < 1000.0 {
        if abs_val < 10.0 {
            format!("{:.1}t", abs_val)
        } else {
            format!("{:.0}t", abs_val)
        }
    } else if abs_val < 1_000_000.0 {
        let kt = abs_val / 1000.0;
        if kt < 10.0 {
            format!("{:.1}kt", kt)
        } else {
            format!("{:.0}kt", kt)
        }
    } else if abs_val < 1_000_000_000.0 {
        let mt = abs_val / 1_000_000.0;
        if mt < 10.0 {
            format!("{:.1}Mt", mt)
        } else {
            format!("{:.0}Mt", mt)
        }
    } else if abs_val < 1_000_000_000_000.0 {
        let gt = abs_val / 1_000_000_000.0;
        if gt < 10.0 {
            format!("{:.1}Gt", gt)
        } else {
            format!("{:.0}Gt", gt)
        }
    } else {
        let tt = abs_val / 1_000_000_000_000.0;
        if tt < 10.0 {
            format!("{:.1}Tt", tt)
        } else {
            format!("{:.0}Tt", tt)
        }
    }
}

pub(super) fn format_mass(megatons: f64) -> String {
    let abs_val = megatons.abs();

    // Near-zero
    if abs_val < 1e-9 {
        return "0 t".to_string();
    }

    // Tons: 1 Mt = 1,000,000 t  (for values < 1 kt = 0.001 Mt)
    if abs_val < 0.001 {
        return format!("{:.1} t", megatons * 1_000_000.0);
    }

    // Kilotons: 1 Mt = 1000 kt  (for values < 1 Mt)
    if abs_val < 1.0 {
        return format!("{:.1} kt", megatons * 1000.0);
    }

    // Megatons (for values < 1 Gt = 1000 Mt)
    if abs_val < 1000.0 {
        return format!("{:.1} Mt", megatons);
    }

    // Gigatons (Gt) - 1 Gt = 1000 Mt
    if abs_val < 1_000_000.0 {
        return format!("{:.1} Gt", megatons / 1000.0);
    }

    // Teratons (Tt) - 1 Tt = 1000 Gt = 1,000,000 Mt
    if abs_val < 1_000_000_000.0 {
        return format!("{:.1} Tt", megatons / 1_000_000.0);
    }

    // Petatons (Pt) - 1 Pt = 1000 Tt = 1,000,000,000 Mt
    if abs_val < 1_000_000_000_000.0 {
        return format!("{:.1} Pt", megatons / 1_000_000_000.0);
    }

    // Exatons (Et) and beyond
    format!("{:.1} Et", megatons / 1_000_000_000_000.0)
}

/// Format a monthly rate value with sign and appropriate color.
/// Returns (formatted_string, color).
pub(super) fn format_rate_monthly(value: f64) -> (String, egui::Color32) {
    if value.abs() < 1e-9 {
        return ("+0/mo".to_string(), theme::TEXT_DIM);
    }
    // v3.8.6: use Unicode minus for negative rates (the ASCII
    // '-' was being clobbered in the format_mass path; this
    // makes the sign consistent with the annual rate display).
    if value > 0.0 {
        (format!("+{}/mo", format_mass(value)), theme::GREEN)
    } else {
        (format!("−{}/mo", format_mass(-value)), theme::RED)
    }
}

/// v3.8.11 (2026-08-07): build a hover tooltip that shows the rate
/// calculation in the form "Production (X) − per-cap (Y) − maintenance (Z)
/// − synthesis input (W) = net" so the player can see why a rate is what
/// it is, especially for resources that are also consumed as inputs to
/// industrial processes (e.g. Methane → PolymerSynthesis, which was
/// silently adding to the displayed rate due to a v3.8.0-v3.8.10 sign
/// bug — see `economy::mining::update_resource_rates` for the fix).
///
/// The body fills are passed in so the tooltip can flag when the rate
/// is being throttled by the storage cap or by the survey reserve. The
/// cap note is sized to the most-binding body in the category — same
/// semantics as the existing cap-lock icon.
pub fn rate_tooltip(
    resource: &crate::economy::ResourceType,
    rate_tracker: &crate::economy::ResourceRateTracker,
    cap: f64,
    body_fills: &[(String, f64)],
) -> String {
    let prod = rate_tracker
        .gross_production_rates
        .get(resource)
        .copied()
        .unwrap_or(0.0);
    let pop = rate_tracker
        .population_consumption
        .get(resource)
        .copied()
        .unwrap_or(0.0);
    let synth = rate_tracker
        .synthesis_input
        .get(resource)
        .copied()
        .unwrap_or(0.0);
    let total_cons = rate_tracker
        .gross_consumption_rates
        .get(resource)
        .copied()
        .unwrap_or(0.0);
    // Maintenance = total consumption - per-cap - synthesis input.
    // (Could be slightly off by floating point; clamp to >=0.)
    let maint = (total_cons - pop - synth).max(0.0);
    let net = rate_tracker
        .resource_rates
        .get(resource)
        .copied()
        .unwrap_or(0.0);

    // The same `format_mass` used by the rate label, so the tooltip
    // numbers line up visually with what the player is reading.
    let f = format_mass;

    let mut s = format!(
        "{} per month:\n\
         \n\
         ┌─ production         {:>8}\n\
         ├─ per-capita         {:>8}  (8.2B × rate)\n\
         ├─ maintenance        {:>8}  (yield-scaled)\n\
         ├─ synthesis input    {:>8}  (industrial processes)\n\
         │\n\
         └─ net rate           {:>+8}\n",
        resource.display_name(),
        f(prod),
        f(-pop),
        f(-maint),
        f(-synth),
        f(net),
    );

    // v3.8.11: cap / reserve notes. Reuse the same body-fill band
    // logic as the cap-lock icon: any body with fill ≥ 1.0 is "at
    // the cap" (production is throttled to the consumption floor).
    if cap > 0.0 && cap < f64::MAX {
        if let Some((body_name, fill)) = body_fills
            .iter()
            .find(|(_, f)| *f >= 1.0 - 1e-9)
            .map(|(n, f)| (n.clone(), *f))
        {
            s.push_str(&format!(
                "\n\
                 ⚠ Capped on {body_name} ({:.0}% full): production is\n\
                 throttled to the consumption floor. Net will not\n\
                 improve until you build Warehouses or send the\n\
                 surplus off-world.",
                fill * 100.0
            ));
        }
    }

    // Industrial-process special note: if the resource is consumed
    // as a synthesis input, flag it so the player doesn't think
    // "I have plenty of Methane, why is it decreasing?" — the
    // answer is "PolymerSynthesis is eating ~860 Mt/yr of it".
    if synth > 0.01 {
        s.push_str(&format!(
            "\n\
             \n\
             Note: this resource is consumed as an input by industrial\n\
             processes. {}/mo flows into factories even if you have\n\
             no per-capita draw on it.",
            f(synth)
        ));
    }

    s
}

/// Main UI dashboard system
#[allow(clippy::too_many_arguments)]
pub(crate) fn ui_dashboard(
    mut commands: Commands,
    mut contexts: EguiContexts,
    // budget: Res<GlobalBudget>, // Moved to ui_resources_bar
    mut selection: ResMut<Selection>,
    mut navigation: ParamSet<(ResMut<CurrentStarSystem>, ResMut<FloatingOrigin>)>,
    nearby_stars: Res<NearbyStarsData>,
    active_menu: Res<ActiveMenu>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    // Resource query for system survey/resource summaries
    resource_query: Query<(
        &SystemId,
        &CelestialBody,
        &PlanetResources,
        Option<&SurveyLevel>,
        Option<&crate::survey::SurveyState>,
    )>,
    // Ledger queries
    all_bodies_query: Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&SystemId>,
    )>,
    selected_query: Query<Entity, With<Selected>>,
    // Starmap queries
    star_system_query: Query<(Entity, &StarSystemIcon, Option<&SelectedStarSystem>)>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    sim_time: Res<SimulationTime>,
    mut orbit_query: Query<&mut OrbitCamera, With<GameCamera>>,
    mut expanded_groups: ResMut<crate::ui::ExpandedLedgerGroups>,
    // GRA-SFX-3b: SFX wiring cannot use a 17th parameter here
    // (`IntoSystem` fn-item cap is 16 in Bevy 0.18). Callsites
    // that need to fire a cue route through the same Commands
    // escape-hatch the Quit / Save / Load buttons use (see
    // `src/ui/dashboard.rs` ~line 1357): insert a one-shot
    // `PendingSfxRequests` resource via `commands`, then the
    // dedicated `drain_pending_sfx_requests` system (registered
    // alongside `ui_dashboard` in `src/ui/mod.rs`) writes one
    // `UiSfxRequest` per cue and removes the resource.
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    if active_menu.current == GameMenu::Research
        || active_menu.current == GameMenu::Construction
        || active_menu.current == GameMenu::Economy
        || active_menu.current == GameMenu::Fleets
        || active_menu.current == GameMenu::Shipbuilding
        || active_menu.current == GameMenu::Personnel
    {
        return;
    }

    // Ledger Panel (Left)
    // Clear expanded group tracking so it is repopulated fresh this frame.
    expanded_groups.groups.clear();

    egui::SidePanel::left("ledger_panel")
        .min_width(220.0)
        .default_width(245.0)
        .max_width(320.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            match active_menu.current {
                GameMenu::Starmap => {
                    // Starmap view: show list of star systems
                    ui.label(
                        egui::RichText::new("NAVIGATION LEDGER")
                            .font(theme::heading())
                            .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new("Star Systems")
                            .font(theme::title())
                            .color(theme::CYAN),
                    );
                    theme::divider(ui);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .id_salt("starmap_ledger_scroll")
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            let mut star_systems: Vec<_> = star_system_query.iter().collect();
                            star_systems.sort_by(|a, b| {
                                let dist_a = a.1.position.length();
                                let dist_b = b.1.position.length();
                                dist_a
                                    .partial_cmp(&dist_b)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                                    .then_with(|| a.1.name.cmp(&b.1.name))
                            });

                            for (entity, icon, is_selected) in star_systems {
                                let response =
                                    render_selectable_label(ui, is_selected.is_some(), &icon.name);

                                if response.clicked() {
                                    // GRA-SFX-3b: starmap row click selects
                                    // + anchors a star system. Anchor is the
                                    // heavier action (single click does two
                                    // things), so use ModalConfirm rather than
                                    // RowSelect.
                                    commands.insert_resource(
                                        crate::plugins::sfx::PendingSfxRequests(vec![
                                            crate::plugins::sfx::SfxCueId::ModalConfirm,
                                        ]),
                                    );
                                    // Single click: select the star system and anchor camera
                                    // Clear previous selections first
                                    for (e, _, sel) in star_system_query.iter() {
                                        if sel.is_some() {
                                            commands.entity(e).remove::<SelectedStarSystem>();
                                        }
                                    }
                                    commands.entity(entity).insert(SelectedStarSystem);

                                    // Anchor camera to this system
                                    if let Ok(mut anchor) = anchor_query.single_mut() {
                                        anchor.0 = Some(entity);
                                        info!("Selected and anchored to {}", icon.name);
                                    }
                                }
                            }
                        });
                }
                GameMenu::Survey => {
                    // System view: show celestial body hierarchy
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("SYSTEM LEDGER")
                                    .font(theme::heading())
                                    .color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("Celestial Objects")
                                    .font(theme::title())
                                    .color(theme::CYAN),
                            );
                        });
                        ui.add_space(10.0);
                        if ui
                            .button("⟲")
                            .on_hover_text("Recenter Camera (also: Home key)")
                            .clicked()
                        {
                            // GRA-SFX-3b: Recenter Camera is a
                            // Modifier-style action. Use ButtonClick.
                            commands.insert_resource(
                                crate::plugins::sfx::PendingSfxRequests(vec![
                                    crate::plugins::sfx::SfxCueId::ButtonClick,
                                ]),
                            );
                            if let Ok(mut orbit) = orbit_query.single_mut() {
                                orbit.pan_offset = Vec3::ZERO;
                            }
                        }
                    });
                    theme::divider(ui);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .id_salt("ledger_scroll")
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            let mut hierarchy: std::collections::HashMap<Entity, Vec<Entity>> =
                                std::collections::HashMap::new();
                            let mut roots: Vec<Entity> = Vec::new();
                            let mut body_map: std::collections::HashMap<Entity, &CelestialBody> =
                                std::collections::HashMap::new();
                            let mut fleet_body_lookup: std::collections::HashMap<
                                Entity,
                                (&CelestialBody, usize),
                            > = std::collections::HashMap::new();
                            let mut orbit_map: std::collections::HashMap<Entity, f64> =
                                std::collections::HashMap::new();
                            let current_system_id = navigation.p0().0;

                            for (entity, body, logical_parent, orbit, system_id) in
                                all_bodies_query.iter()
                            {
                                let sys_id = system_id.map(|s| s.0).unwrap_or(0);
                                fleet_body_lookup.insert(entity, (body, sys_id));

                                // Filter by current star system
                                if sys_id != current_system_id {
                                    continue;
                                }

                                body_map.insert(entity, body);
                                if let Some(orbit) = orbit {
                                    orbit_map.insert(entity, orbit.semi_major_axis);
                                }

                                if let Some(logical_parent) = logical_parent {
                                    hierarchy.entry(logical_parent.0).or_default().push(entity);
                                } else {
                                    roots.push(entity);
                                }
                            }

                            // Helper closure to sort entities
                            let sort_entities = |entities: &mut Vec<Entity>| {
                                entities.sort_by(|a, b| {
                                    let name_a = &body_map.get(a).unwrap().name;
                                    let name_b = &body_map.get(b).unwrap().name;

                                    // Always keep Sol at the top
                                    if name_a == "Sol" {
                                        return std::cmp::Ordering::Less;
                                    }
                                    if name_b == "Sol" {
                                        return std::cmp::Ordering::Greater;
                                    }

                                    // Sort by orbit distance (semi-major axis)
                                    let dist_a = orbit_map.get(a).unwrap_or(&0.0);
                                    let dist_b = orbit_map.get(b).unwrap_or(&0.0);

                                    match dist_a.partial_cmp(dist_b) {
                                        Some(std::cmp::Ordering::Equal) | None => {
                                            name_a.cmp(name_b)
                                        } // Fallback to name
                                        Some(ord) => ord,
                                    }
                                });
                            };

                            // Sort roots
                            sort_entities(&mut roots);

                            // Sort all children lists in the hierarchy
                            for children in hierarchy.values_mut() {
                                sort_entities(children);
                            }

                            // Get anchor entity for display only
                            let anchored_entity = anchor_query
                                .single()
                                .ok()
                                .and_then(|a| a.0);

                            // Use a RefCell to allow interior mutability for anchor setting
                            use std::cell::RefCell;
                            thread_local! {
                                static PENDING_ANCHOR: RefCell<Option<Entity>> = const { RefCell::new(None) };
                            }

                            // Store pending anchor to set after UI is done
                            let pending_anchor = RefCell::new(None);

                            for root in roots {
                                render_body_tree(
                                    ui,
                                    root,
                                    &body_map,
                                    &hierarchy,
                                    &mut selection,
                                    &mut commands,
                                    &selected_query,
                                    anchored_entity,
                                    &pending_anchor,
                                    &mut expanded_groups.groups,
                                );
                            }

                            // Apply pending anchor after UI is done
                            if let Some(entity) = *pending_anchor.borrow() {
                                if let Ok(mut anchor) = anchor_query.single_mut() {
                                    anchor.0 = Some(entity);
                                }
                            }

                            // ── Fleet section ─────────────────────────────
                            ui.add_space(4.0);
                            theme::divider(ui);

                            let fleet_id = ui.make_persistent_id("survey_fleet_tree");
                            let mut fleet_header_clicked = false;
                            let mut fleet_header = egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                fleet_id,
                                true,
                            )
                            .show_header(ui, |ui| {
                                let n = fleet_query.iter().count();
                                fleet_header_clicked =
                                    render_group_header(ui, "🚀 Fleets", n, true, theme::EP_TEAL)
                                        .clicked();
                            });
                            if fleet_header_clicked {
                                fleet_header.toggle();
                            }
                            fleet_header.body(|ui| {
                                render_fleet_ledger_tree(
                                    ui,
                                    &fleet_query,
                                    &fleet_body_lookup,
                                    &mut fleet_ui_state,
                                    &selected_query,
                                    &mut commands,
                                    &mut selection,
                                    sim_time.elapsed_seconds(),
                                    &mut anchor_query,
                                    &mut orbit_query,
                                    &mut navigation,
                                    &star_system_query,
                                    &sim_time,
                                );
                            });
                        });
                }
                _ => {
                    // Placeholder for other menus
                    ui.heading(active_menu.current.name());
                    ui.separator();

                    ui.label(
                        egui::RichText::new("Coming Soon")
                            .size(16.0)
                            .color(theme::TEXT_DIM)
                    );

                    ui.add_space(10.0);

                    match active_menu.current {
                        GameMenu::Main => {
                            ui.label("Main menu options:");
                            if ui.button("🚪 Quit Game").clicked() {
                                // Fire Bevy's `AppExit` message so the
                                // runtime picks up the quit request the
                                // same way the main-menu Quit button does
                                // (see `src/ui/launch/menu.rs`). Routed
                                // through `Commands` rather than a
                                // dedicated `MessageWriter` parameter so
                                // the function stays under Bevy 0.18's
                                // `IntoSystem` parameter cap (~16).
                                //
                                // GRA-SFX-3b: same `Commands` channel
                                // is the documented escape hatch for
                                // firing SFX from saturated systems —
                                // insert a one-shot PendingSfxRequests
                                // resource that `drain_collector_into_ui_sfx`
                                // picks up next frame.
                                info!("Quit clicked — sending AppExit");
                                commands.insert_resource(
                                    crate::plugins::sfx::PendingSfxRequests(vec![
                                        crate::plugins::sfx::SfxCueId::ButtonClick,
                                    ]),
                                );
                                commands.write_message(bevy::app::AppExit::Success);
                            }
                            // GRA-358 PR-C: Save Game routes through
                            // the Save Panel subview. We capture the
                            // current `LaunchState` so the panel's
                            // Back button returns here (to InGame,
                            // not MainMenu). The egui render system
                            // already has a `Commands` param so we
                            // use `insert_resource` to flag the
                            // request without growing the parameter
                            // list. `consume_in_game_save_request_system`
                            // runs after `ui_dashboard` in
                            // `EguiPrimaryContextPass` and does the
                            // actual state flip.
                            if ui.button("💾 Save Game").clicked() {
                                info!("Save clicked — opening Save Panel subview");
                                commands.insert_resource(
                                    crate::ui::launch::PendingInGameSaveRequest { open_panel: true },
                                );
                                commands.insert_resource(
                                    crate::plugins::sfx::PendingSfxRequests(vec![
                                        crate::plugins::sfx::SfxCueId::PanelOpen,
                                    ]),
                                );
                            }
                            if ui.button("📂 Load Game").clicked() {
                                info!("Load clicked — opening Load Game subview");
                                commands.insert_resource(
                                    crate::ui::launch::PendingInGameLoadRequest { open_panel: true },
                                );
                                commands.insert_resource(
                                    crate::plugins::sfx::PendingSfxRequests(vec![
                                        crate::plugins::sfx::SfxCueId::PanelOpen,
                                    ]),
                                );
                            }
                            // GRA-358 PR-F: "🏠 Main Menu" returns the
                            // player to the launch main menu without
                            // quitting the application. Distinct from
                            // "🚪 Quit Game" (which fires `AppExit`
                            // and exits the process). The consumer
                            // (`consume_in_game_return_to_menu_system`)
                            // lives in `EguiPrimaryContextPass` and
                            // transitions `LaunchState` from `InGame`
                            // to `MainMenu` while tearing down the
                            // world-swap markers so a subsequent
                            // New Game / Continue / Load Save
                            // kickoff re-runs the boot-init chain.
                            if ui.button("🏠 Main Menu").clicked() {
                                info!("Main Menu clicked — requesting return to launch main menu");
                                commands.insert_resource(
                                    crate::ui::launch::PendingReturnToMenu { requested: true },
                                );
                                commands.insert_resource(
                                    crate::plugins::sfx::PendingSfxRequests(vec![
                                        crate::plugins::sfx::SfxCueId::PanelClose,
                                    ]),
                                );
                            }
                            if ui.button("⚙ Options").clicked() {
                                info!("Options clicked");
                                commands.insert_resource(
                                    crate::plugins::sfx::PendingSfxRequests(vec![
                                        crate::plugins::sfx::SfxCueId::ButtonClick,
                                    ]),
                                );
                            }
                        }
                        GameMenu::Construction => {
                            // The Construction panel is now a bevy_ui
                            // menu (`src/ui/construction.rs`); the F4
                            // key opens it. No egui system here.
                            ui.label("Switch to full Construction view for details.");
                        }
                        GameMenu::Research => {
                            ui.label("Research UI requires loading...");
                            ui.label("Switch to Research view to see tech tree.");
                        }
                        GameMenu::Fleets => {
                            ui.label("Fleet panel is open in the main view.");
                        }
                        GameMenu::Shipbuilding => {
                            ui.label("Ship design and construction queue will be shown here.");
                        }
                        GameMenu::Economy => {
                            ui.label("Economic overview and private sector management will be shown here.");
                        }
                        GameMenu::Personnel => {
                            // Handled by ui_personnel_panel system
                            ui.label("Personnel panel is open in the main view.");
                        }
                        GameMenu::Intel => {
                            // Handled by `ui_intel_panel` system — see
                            // GRA-787-followup. The dedicated system owns
                            // the submenu picker, the early-game
                            // milestone checklist, and the placeholder
                            // placeholders for Faction Intel and
                            // Anomalies.
                            ui.label("Intel panel is open in the main view.");
                        }
                        GameMenu::Diplomacy => {
                            ui.label("Diplomatic relations and treaties will be shown here.");
                        }
                        GameMenu::Starmap | GameMenu::Survey => {
                            // Already handled above
                        }
                    }
                }
            }
        });

    // Right side panel - show either selected star system or selected body
    let selected_star_system = star_system_query
        .iter()
        .find(|(_, _, selected)| selected.is_some());

    if active_menu.current == GameMenu::Starmap {
        if let Some((_star_entity, star_icon, _)) = selected_star_system {
            // Show star system details
            render_star_system_panel(
                ctx,
                star_icon,
                &all_bodies_query,
                &resource_query,
                &nearby_stars,
            );
        }
    }
    // Body dossier panel is now rendered by `dossier_panel::ui_planet_dossier`
}

/// Always-visible bottom panel for time controls.
///
/// Registered in `UiSystemSet::TopBar` so egui reserves the bottom strip
/// **before** any side panel (Research, Construction, Economy, etc.) is
/// rendered. This ensures the panel is never occluded regardless of the
/// active menu.
///
/// Keyboard shortcuts:
/// - **Space** — toggle pause / resume
/// - **1–5** — set speed (1 hr/s … 1 yr/s)
pub(super) fn ui_time_controls(
    mut contexts: EguiContexts,
    mut time_scale: ResMut<TimeScale>,
    sim_time: Res<SimulationTime>,
    view_mode: Res<ViewMode>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    mut playlist: ResMut<crate::plugins::music::MusicPlaylist>,
    mut sfx_ui: MessageWriter<crate::plugins::sfx::bridges::UiSfxRequest>,
    // `Instant` has no `Default` impl, so Local requires an
    // Option wrapper. The first tick sees `None` (always
    // pass the throttle gap) and subsequent ticks see the
    // last-tick stamp.
    mut slider_tick_gate: Local<Option<std::time::Instant>>,
    // The early-game milestone checklist used to be a `TopBottomPanel`
    // here (GRA-787). It leaked into every menu because the bottom
    // strip is always on-screen — Economy, Shipbuilding, the dossier,
    // all of them rendered an "EARLY-GAME PROGRESS" stub regardless of
    // what the player had open. The checklist is now its own submenu
    // under `GameMenu::Intel` (see `IntelSubmenu::EarlyGameProgress`),
    // so this function no longer needs the resource.
) {
    // ── Keyboard shortcuts (skip when egui is consuming input) ────────────
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    if !ctx.wants_keyboard_input() {
        if keyboard_input.just_pressed(KeyCode::Space) {
            if time_scale.is_paused() {
                time_scale.resume();
            } else {
                time_scale.pause();
            }
        }
        // Keys 1-5 set speeds (and un-pause if currently paused)
        const SPEED_PRESETS: [f32; 5] = [3_600.0, 86_400.0, 604_800.0, 2_592_000.0, 31_557_600.0];
        let speed_keys = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
        ];
        for (key, &preset) in speed_keys.iter().zip(SPEED_PRESETS.iter()) {
            if keyboard_input.just_pressed(*key) {
                time_scale.set_speed(preset);
                sfx_ui.write(crate::plugins::sfx::bridges::UiSfxRequest(
                    crate::plugins::sfx::SfxCueId::SliderTick,
                ));
                break;
            }
        }
    }

    // ── Blink factor for pause button (0.0 → 1.0, ~1.5 Hz) ──────────────
    let blink = if time_scale.is_paused() {
        (real_time.elapsed_secs() * std::f32::consts::TAU * 1.5).sin() * 0.5 + 0.5
    } else {
        0.0
    };

    // ── Helper: render a speed preset button with active highlight ────────
    let active_scale = time_scale.scale;
    let is_paused = time_scale.is_paused();

    egui::TopBottomPanel::bottom("time_controls")
        .min_height(54.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // ── Pause button ──────────────────────────────────────────
                let pause_fill = if is_paused {
                    theme::pause_button_fill(blink)
                } else {
                    theme::SURFACE
                };
                let pause_stroke = if is_paused {
                    let alpha = (120.0 + 135.0 * blink) as u8;
                    egui::Stroke::new(1.5_f32, theme::paused_overlay_fg(alpha))
                } else {
                    egui::Stroke::new(0.5_f32, theme::BORDER)
                };
                let pause_label = if is_paused {
                    egui::RichText::new("⏸ PAUSED")
                        .color(theme::paused_label_color((180.0 + 75.0 * blink) as u8))
                } else {
                    egui::RichText::new("▶ Running").color(theme::TEXT_DIM)
                };
                let pause_btn = egui::Button::new(pause_label)
                    .stroke(pause_stroke)
                    .fill(pause_fill);
                if ui.add_sized([80.0, 36.0], pause_btn).clicked() {
                    if is_paused {
                        time_scale.resume();
                    } else {
                        time_scale.pause();
                    }
                    sfx_ui.write(crate::plugins::sfx::bridges::UiSfxRequest(
                        crate::plugins::sfx::SfxCueId::ModeToggle,
                    ));
                }

                ui.separator();

                // ── Speed preset buttons [1]–[5] with active highlight ────
                const SPEED_LABELS: [&str; 5] = ["1 hr/s", "1 day/s", "1 wk/s", "1 mo/s", "1 yr/s"];
                const SPEED_VALUES: [f32; 5] =
                    [3_600.0, 86_400.0, 604_800.0, 2_592_000.0, 31_557_600.0];
                const SPEED_HOTKEYS: [&str; 5] = ["[1]", "[2]", "[3]", "[4]", "[5]"];

                for i in 0..5 {
                    let is_active = !is_paused && (active_scale - SPEED_VALUES[i]).abs() < 1.0;
                    let btn = if is_active {
                        egui::Button::new(
                            egui::RichText::new(SPEED_LABELS[i])
                                .color(theme::CYAN)
                                .strong(),
                        )
                        .stroke(egui::Stroke::new(1.5_f32, theme::CYAN))
                        .fill(theme::SURFACE_RAISED)
                    } else {
                        egui::Button::new(
                            egui::RichText::new(SPEED_LABELS[i])
                                .color(theme::TEXT)
                                .strong(),
                        )
                        .stroke(egui::Stroke::new(0.5_f32, theme::BORDER))
                        .fill(theme::SURFACE)
                    };
                    let speed_label = SPEED_LABELS[i];
                    let hotkey_key = SPEED_HOTKEYS[i];
                    if ui
                        .add_sized([60.0, 36.0], btn)
                        .on_hover_ui(|ui| {
                            theme::tooltip_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(speed_label).color(theme::TEXT));
                                    ui.label(
                                        egui::RichText::new("(hotkey ").color(theme::TEXT_DIM),
                                    );
                                    ui.label(theme::kbd_shortcut_label(hotkey_key));
                                    ui.label(egui::RichText::new(")").color(theme::TEXT_DIM));
                                });
                            });
                        })
                        .clicked()
                    {
                        time_scale.set_speed(SPEED_VALUES[i]);
                        sfx_ui.write(crate::plugins::sfx::bridges::UiSfxRequest(
                            crate::plugins::sfx::SfxCueId::SliderTick,
                        ));
                    }
                }

                ui.separator();

                // ── Status info ───────────────────────────────────────────
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Date: {}", sim_time.format_date_time()))
                            .color(theme::TEXT),
                    );
                    let (view_label, view_color) = match *view_mode {
                        ViewMode::System => ("🔭 System View", theme::RP_BLUE),
                        ViewMode::Starmap => ("🌌 Starmap View", theme::STAR_GOLD),
                    };
                    ui.colored_label(view_color, egui::RichText::new(view_label));
                });

                // ── Music controls (right-aligned, inline with time controls) ──
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // RTL: items added right-to-left, so volume → skip → pause → sep → title
                    // GRA-SFX-3b: volume slider. Fire SliderTick via
                    // the wrapper so it stays throttle-safe even
                    // during rapid drags. Use the `_opt` flavour
                    // because `Instant` lacks `Default` and we
                    // store the gate in a `Local<Option<Instant>>`.
                    super::egui_sfx::egui_sfx_slider_opt(
                        ui,
                        &mut playlist.volume,
                        0.0..=1.0,
                        "",
                        &mut slider_tick_gate,
                        &mut sfx_ui,
                    );

                    let skip_btn = egui::Button::new(
                        egui::RichText::new("⏭").size(16.0).color(theme::TEXT_DIM),
                    )
                    .stroke(egui::Stroke::new(0.5_f32, theme::BORDER))
                    .fill(theme::SURFACE);
                    if ui.add_sized([36.0, 36.0], skip_btn).clicked() {
                        // GRA-SFX-3b: music skip. Pair with ButtonClick
                        // since the speed-preset row already uses
                        // SliderTick for the same player-action
                        // surface.
                        sfx_ui.write(crate::plugins::sfx::bridges::UiSfxRequest(
                            crate::plugins::sfx::SfxCueId::ButtonClick,
                        ));
                        playlist.skip_requested = true;
                    }

                    let play_label = if playlist.paused { "▶" } else { "⏸" };
                    let play_color = if playlist.paused {
                        theme::CYAN
                    } else {
                        theme::TEXT_DIM
                    };
                    let play_stroke = if playlist.paused {
                        egui::Stroke::new(1.0_f32, theme::CYAN)
                    } else {
                        egui::Stroke::new(0.5_f32, theme::BORDER)
                    };
                    let play_btn = egui::Button::new(
                        egui::RichText::new(play_label).size(16.0).color(play_color),
                    )
                    .stroke(play_stroke)
                    .fill(theme::SURFACE);
                    if ui.add_sized([36.0, 36.0], play_btn).clicked() {
                        // GRA-SFX-3b: music play/pause toggle.
                        // ModeToggle is the canonical cue for
                        // play/pause-style state flips.
                        sfx_ui.write(crate::plugins::sfx::bridges::UiSfxRequest(
                            crate::plugins::sfx::SfxCueId::ModeToggle,
                        ));
                        playlist.paused = !playlist.paused;
                    }

                    ui.separator();

                    let track = &playlist.tracks[playlist.current_index];
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("♪ {}", track.title))
                                .font(egui::FontId::proportional(11.0))
                                .color(theme::TEXT_DIM),
                        );
                    });
                });
            });
        });
}

/// Render detailed information panel for a selected star system
fn render_star_system_panel(
    ctx: &egui::Context,
    star_icon: &StarSystemIcon,
    bodies_query: &Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&SystemId>,
    )>,
    resource_query: &Query<(
        &SystemId,
        &CelestialBody,
        &PlanetResources,
        Option<&SurveyLevel>,
        Option<&crate::survey::SurveyState>,
    )>,
    nearby_stars: &Res<NearbyStarsData>,
) {
    let panel_frame = theme::panel_frame();

    egui::SidePanel::right("star_system_panel")
        .min_width(340.0)
        .max_width(420.0)
        .frame(panel_frame)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("star_system_panel_scroll")
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("SELECTED STAR SYSTEM")
                            .font(theme::heading())
                            .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new(&star_icon.name)
                            .font(theme::title())
                            .color(theme::CYAN),
                    );
                    ui.add_space(6.0);

                    let distance_ly = star_icon.position.length() / 63241.077;
                    egui::Grid::new("star_system_info_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            theme::stat_row(ui, "DISTANCE", &format!("{distance_ly:.2} ly"));
                            theme::stat_row(ui, "SYSTEM ID", &star_icon.id.to_string());
                        });

                    if let Some(system_data) = nearby_stars.get_by_id(star_icon.id) {
                        theme::divider(ui);
                        ui.label(
                            egui::RichText::new("STAR PROPERTIES")
                                .font(theme::heading())
                                .color(theme::CYAN),
                        );
                        ui.add_space(6.0);

                        for (star_idx, star_data) in system_data.stars.iter().enumerate() {
                            let star_label = if system_data.stars.len() > 1 {
                                format!("STAR {}", star_idx + 1)
                            } else {
                                "PRIMARY".to_string()
                            };

                            ui.label(
                                egui::RichText::new(star_label)
                                    .font(theme::heading())
                                    .color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new(&star_data.name)
                                    .font(theme::body(16.0))
                                    .color(theme::TEXT_VALUE),
                            );
                            ui.add_space(4.0);

                            egui::Grid::new(format!("star_properties_grid_{}", star_idx))
                                .num_columns(2)
                                .spacing([16.0, 4.0])
                                .show(ui, |ui| {
                                    theme::stat_row(ui, "TYPE", &star_data.spectral_type);
                                    theme::stat_row(
                                        ui,
                                        "MASS",
                                        &format!("{:.2} M☉", star_data.mass_sol),
                                    );
                                    theme::stat_row(
                                        ui,
                                        "RADIUS",
                                        &format!("{:.2} R☉", star_data.radius_sol),
                                    );
                                    theme::stat_row(
                                        ui,
                                        "LUMINOSITY",
                                        &format!("{:.3} L☉", star_data.luminosity_sol),
                                    );
                                    theme::stat_row(
                                        ui,
                                        "TEMP",
                                        &format!("{:.0} K", star_data.temp_k),
                                    );
                                    if let Some(metallicity) = star_data.metallicity {
                                        ui.label(
                                            egui::RichText::new("METALLICITY")
                                                .font(theme::mono(10.0))
                                                .color(theme::TEXT_DIM),
                                        );
                                        let metallicity_color = if metallicity > 0.0 {
                                            theme::STAR_GOLD
                                        } else if metallicity < 0.0 {
                                            theme::TEXT_DIM
                                        } else {
                                            theme::TEXT_VALUE
                                        };
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "[Fe/H] {metallicity:+.2}"
                                            ))
                                            .font(theme::mono(12.0))
                                            .color(metallicity_color),
                                        );
                                        ui.end_row();
                                    }
                                });

                            if star_idx + 1 < system_data.stars.len() {
                                ui.add_space(4.0);
                            }
                        }
                    }

                    let bodies: Vec<_> = bodies_query
                        .iter()
                        .filter(|(_, _, _, _, sys_id)| {
                            sys_id.map(|s| s.0 == star_icon.id).unwrap_or(false)
                        })
                        .collect();

                    theme::divider(ui);
                    ui.label(
                        egui::RichText::new("SYSTEM BODIES")
                            .font(theme::heading())
                            .color(theme::CYAN),
                    );
                    ui.add_space(6.0);

                    let stars = bodies
                        .iter()
                        .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Star))
                        .count();
                    let planets = bodies
                        .iter()
                        .filter(|(_, b, _, _, _)| {
                            matches!(b.body_type, BodyType::Planet | BodyType::GasGiant)
                        })
                        .count();
                    let dwarf_planets = bodies
                        .iter()
                        .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::DwarfPlanet))
                        .count();
                    let moons = bodies
                        .iter()
                        .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Moon))
                        .count();
                    let asteroids = bodies
                        .iter()
                        .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Asteroid))
                        .count();
                    let comets = bodies
                        .iter()
                        .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Comet))
                        .count();

                    egui::Grid::new("star_system_bodies_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            theme::stat_row(ui, "TOTAL", &bodies.len().to_string());
                            if stars > 0 {
                                theme::stat_row(ui, "STARS", &stars.to_string());
                            }
                            if planets > 0 {
                                theme::stat_row(ui, "PLANETS", &planets.to_string());
                            }
                            if dwarf_planets > 0 {
                                theme::stat_row(ui, "DWARF", &dwarf_planets.to_string());
                            }
                            if moons > 0 {
                                theme::stat_row(ui, "MOONS", &moons.to_string());
                            }
                            if asteroids > 0 {
                                theme::stat_row(ui, "ASTEROIDS", &asteroids.to_string());
                            }
                            if comets > 0 {
                                theme::stat_row(ui, "COMETS", &comets.to_string());
                            }
                        });

                    theme::divider(ui);
                    ui.label(
                        egui::RichText::new("SYSTEM RESOURCES")
                            .font(theme::heading())
                            .color(theme::CYAN),
                    );
                    ui.add_space(6.0);

                    let resource_bodies: Vec<_> = resource_query
                        .iter()
                        .filter(|(sys_id, _, _, _, _)| sys_id.0 == star_icon.id)
                        .collect();

                    let total_resource_weight: f64 = resource_bodies
                        .iter()
                        .map(|(_, _, resources, _, _)| total_resource_expectation_weight(resources))
                        .sum();
                    let discovered_resource_weight: f64 = resource_bodies
                        .iter()
                        .map(|(_, _, resources, survey_level, survey_state)| {
                            discovered_resource_expectation_weight(
                                resources,
                                survey_level.copied().unwrap_or(SurveyLevel::Unsurveyed),
                                *survey_state,
                            )
                        })
                        .sum();
                    // PR-F (GRA-84): a body counts as surveyed when
                    // it has a non-Unsurveyed legacy `SurveyLevel` OR
                    // a v0.5.0 `SurveyState` (any dimensions
                    // surveyed). Without this filter,
                    // `SurveyState`-only bodies were treated as
                    // unsurveyed in the starmap system panel.
                    let surveyed_body_count = resource_bodies
                        .iter()
                        .filter(|(_, _, _, survey_level, survey_state)| {
                            survey_level.copied().unwrap_or(SurveyLevel::Unsurveyed)
                                != SurveyLevel::Unsurveyed
                                || survey_state.is_some()
                        })
                        .count();
                    let survey_percent = if total_resource_weight > 0.0 {
                        (discovered_resource_weight / total_resource_weight * 100.0)
                            .clamp(0.0, 100.0)
                    } else {
                        0.0
                    };
                    let survey_percent = if survey_percent.abs() < 0.000_1 {
                        0.0
                    } else {
                        survey_percent
                    };
                    let fully_surveyed = total_resource_weight > 0.0
                        && (discovered_resource_weight / total_resource_weight) >= 0.999_999;

                    let mut discovered_resources: HashMap<ResourceType, f64> = HashMap::new();
                    for (_, _, resources, survey_level, survey_state) in &resource_bodies {
                        let legacy_level = survey_level.copied().unwrap_or(SurveyLevel::Unsurveyed);
                        if legacy_level == SurveyLevel::Unsurveyed && survey_state.is_none() {
                            continue;
                        }

                        let fidelity = survey_state
                            .map(|s| s.fidelity(crate::survey::SurveyDimension::MineralDeposits))
                            .unwrap_or_else(|| legacy_level.as_deposit_fidelity(0.0));

                        for (resource_type, deposit) in &resources.deposits {
                            let estimate = crate::survey::estimate_with_fidelity(deposit, fidelity);
                            if let Some(mid) = estimate.mid {
                                if mid > 0.001 {
                                    *discovered_resources.entry(*resource_type).or_insert(0.0) +=
                                        mid;
                                }
                            }
                        }
                    }

                    egui::Grid::new("star_system_resources_summary_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            theme::stat_row(ui, "SURVEY", &format!("{survey_percent:.0}%"));
                            theme::stat_row(
                                ui,
                                "SURVEYED BODIES",
                                &format!("{} / {}", surveyed_body_count, resource_bodies.len()),
                            );
                            theme::stat_row(
                                ui,
                                "REVEALED TYPES",
                                &discovered_resources.len().to_string(),
                            );
                        });

                    ui.add_space(6.0);

                    for (category_name, category_resources) in ResourceType::by_category() {
                        let mineable: Vec<ResourceType> = category_resources
                            .into_iter()
                            .filter(|resource| resource.is_mineable())
                            .collect();
                        if mineable.is_empty() {
                            continue;
                        }

                        let cat_color = theme::category_color(category_name);
                        ui.label(
                            egui::RichText::new(category_name)
                                .font(theme::mono(9.0))
                                .color(cat_color),
                        );

                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::Vec2::splat(3.0);

                            for resource_type in &mineable {
                                let display =
                                    if let Some(mid) = discovered_resources.get(resource_type) {
                                        // The starmap aggregates the
                                        // best-estimate mid across every
                                        // body in the system. Surface
                                        // that as a Precise estimate so
                                        // the tile shows the system
                                        // total, with confidence 1.0 so
                                        // the band collapses.
                                        let mid_value = *mid;
                                        // PR-F (GRA-84) refactored the
                                        // tile display to carry the raw
                                        // megaton value directly. The
                                        // legacy `DepositEstimate`
                                        // envelope was dropped in PR-F
                                        // because the system-total mid is
                                        // already a single precise number
                                        // (the starmap aggregator rounds
                                        // to the most-confident body).
                                        ResourceTileDisplay::Deposit {
                                            discovered_megatons: mid_value,
                                            concentration: None,
                                        }
                                    } else if fully_surveyed {
                                        ResourceTileDisplay::None
                                    } else {
                                        ResourceTileDisplay::Unknown
                                    };

                                paint_resource_tile(ui, *resource_type, display, 44.0, cat_color);
                            }
                        });

                        ui.add_space(2.0);
                    }

                    theme::divider(ui);
                    ui.label(
                        egui::RichText::new("POPULATION")
                            .font(theme::heading())
                            .color(theme::CYAN),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "Population management data is not yet available for starmap systems.",
                        )
                        .font(theme::body(13.0))
                        .color(theme::TEXT_DIM),
                    );
                });
        });
}

fn total_resource_expectation_weight(resources: &PlanetResources) -> f64 {
    resources
        .deposits
        .values()
        .map(|deposit| deposit.reserve.total_mass())
        .sum()
}

fn discovered_resource_expectation_weight(
    resources: &PlanetResources,
    survey_level: SurveyLevel,
    survey_state: Option<&crate::survey::SurveyState>,
) -> f64 {
    let fidelity = survey_state
        .map(|s| s.fidelity(crate::survey::SurveyDimension::MineralDeposits))
        .unwrap_or_else(|| survey_level.as_deposit_fidelity(0.0));
    resources
        .deposits
        .values()
        .map(|deposit| crate::survey::estimate_with_fidelity(deposit, fidelity).mid_or_zero())
        .sum()
}

// ── GRA-787-followup: Early-game milestone Intel submenu content ──────────

/// Render the 6-row milestone checklist + "next objective" line.
///
/// This used to be a `TopBottomPanel` inside `ui_time_controls`, which
/// leaked into every menu. It now lives under
/// `GameMenu::Intel` → `IntelSubmenu::EarlyGameProgress`; the caller
/// owns the panel chrome (heading, divider, scroll area) so this
/// helper just emits the rows. Read-only by design — flag flips happen
/// only in `crate::survey::milestones` consumers.
pub(crate) fn draw_milestones_section(
    ui: &mut egui::Ui,
    milestones: &crate::survey::EarlyGameMilestones,
) {
    for (step, is_set) in milestones.progress_rows() {
        let (marker, color) = if is_set {
            ("[x]", theme::EP_TEAL)
        } else {
            ("[ ]", theme::TEXT_DIM)
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(marker)
                    .font(theme::mono(12.0))
                    .color(color),
            );
            // `RichText::strikethrough()` takes no argument in
            // egui 0.33 — apply conditionally on the builder.
            let mut step_label = egui::RichText::new(step.display_name()).color(if is_set {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            });
            if is_set {
                step_label = step_label.strikethrough();
            }
            ui.label(step_label);
        });
        ui.label(
            egui::RichText::new(step.description())
                .color(theme::TEXT_DIM)
                .small(),
        );
    }

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(milestones.next_objective())
            .color(theme::TEXT_DIM)
            .small(),
    );
}

/// Renders the Intel menu: a submenu picker on the left
/// (`IntelSubmenu::EarlyGameProgress` today, with placeholders for
/// faction / anomaly intel) and the selected view on the right.
///
/// Registered in `src/ui/mod.rs` as a `MainPanels` system gated on
/// `GameMenu::Intel` — same pattern as `ui_personnel_panel`. The
/// dedicated system keeps `ui_dashboard` under Bevy 0.18's
/// 16-parameter fn-item `IntoSystem` limit (the picker + content
/// rendering needs both `ResMut<SelectedIntelSubmenu>` and
/// `Res<EarlyGameMilestones>`, which would otherwise push `ui_dashboard`
/// past the cap).
pub(super) fn ui_intel_panel(
    mut contexts: EguiContexts,
    mut commands: Commands,
    active_menu: Res<ActiveMenu>,
    mut selected_intel: ResMut<SelectedIntelSubmenu>,
    milestones: Res<crate::survey::EarlyGameMilestones>,
) {
    if active_menu.current != GameMenu::Intel {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    egui::CentralPanel::default()
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("INTEL")
                    .font(theme::title())
                    .color(theme::CYAN),
            );
            ui.label(
                egui::RichText::new(
                    "Pick a view on the left; the content area updates on the right.",
                )
                .color(theme::TEXT_DIM)
                .small(),
            );
            theme::divider(ui);
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                // Left column — picker.
                egui::Frame::group(ui.style())
                    .fill(theme::SURFACE)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_min_width(200.0);
                        ui.label(
                            egui::RichText::new("INTEL VIEWS")
                                .font(theme::heading())
                                .color(theme::TEXT_DIM),
                        );
                        theme::divider(ui);
                        ui.add_space(4.0);
                        for sub in IntelSubmenu::all() {
                            let is_selected = selected_intel.current == *sub;
                            let response = render_selectable_label(
                                ui,
                                is_selected,
                                &format!("{} {}", sub.icon(), sub.label()),
                            );
                            if response.clicked() {
                                // GRA-SFX-3b: intel submenu switch.
                                // TabSwitch is the canonical cue for
                                // changing the visible view inside a
                                // picker.
                                commands.insert_resource(
                                    crate::plugins::sfx::PendingSfxRequests(vec![
                                        crate::plugins::sfx::SfxCueId::TabSwitch,
                                    ]),
                                );
                                selected_intel.current = *sub;
                            }
                            theme::paint_focus_ring(
                                ui.painter(),
                                response.rect,
                                response.has_focus(),
                            );
                        }
                    });

                ui.add_space(12.0);

                // Right column — content area.
                egui::Frame::group(ui.style())
                    .fill(theme::SURFACE)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_min_width(340.0);
                        match selected_intel.current {
                            IntelSubmenu::EarlyGameProgress => {
                                ui.label(
                                    egui::RichText::new("EARLY-GAME PROGRESS")
                                        .font(theme::heading())
                                        .color(theme::CYAN),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        "Milestones the early game nudges you toward. Strikes through as you clear them.",
                                    )
                                    .color(theme::TEXT_DIM)
                                    .small(),
                                );
                                theme::divider(ui);
                                ui.add_space(4.0);
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .id_salt("intel_early_game_scroll")
                                    .max_height(420.0)
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        draw_milestones_section(ui, &milestones);
                                    });
                            }
                            IntelSubmenu::Factions => {
                                ui.label(
                                    egui::RichText::new("FACTION INTEL")
                                        .font(theme::heading())
                                        .color(theme::CYAN),
                                );
                                theme::divider(ui);
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Foreign-faction dossiers land here once the diplomacy layer ships.",
                                    )
                                    .color(theme::TEXT_DIM),
                                );
                            }
                            IntelSubmenu::Anomalies => {
                                ui.label(
                                    egui::RichText::new("ANOMALIES")
                                        .font(theme::heading())
                                        .color(theme::CYAN),
                                );
                                theme::divider(ui);
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Catalogue of detected and refuted anomalies — fills in alongside the survey rework.",
                                    )
                                    .color(theme::TEXT_DIM),
                                );
                            }
                        }
                    });
            });
        });
}
