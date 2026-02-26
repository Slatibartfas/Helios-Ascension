use super::*;
use super::transfer_planner::render_transfer_planner;

pub(super) fn ui_fleets_panel(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    fleet_query: Query<(
        Entity,
        &Fleet,
        Option<&FleetOrbit>,
        Option<&ActiveManeuver>,
        &SpaceCoordinates,
    )>,
    body_query: Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    colony_query: Query<(Entity, &Colony)>,
    mut pending_actions: ResMut<PendingFleetActions>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    sim_time: Res<SimulationTime>,
) {
    if active_menu.current != GameMenu::Fleets {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let elapsed = sim_time.elapsed_seconds();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Fleets");
        ui.separator();

        // ── Top summary bar ──────────────────────────────────────────────────
        let fleet_count = fleet_query.iter().count();
        let in_transit = fleet_query.iter().filter(|(_, _, _, m, _)| m.is_some()).count();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("🚀 Total Fleets: {fleet_count}"))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(200, 220, 255)),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(format!("✈ In Transit: {in_transit}"))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(100, 200, 255)),
            );
        });
        ui.separator();

        // ── Main two-column layout ───────────────────────────────────────────
        let available = ui.available_size();
        let left_width = (available.x * 0.42).max(380.0);

        ui.horizontal_top(|ui| {
            // ── Left column: fleet list ──────────────────────────────────────
            ui.allocate_ui_with_layout(
                egui::Vec2::new(left_width, available.y - 80.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                egui::Frame::default()
                    .inner_margin(egui::Margin::same(6i8))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 80, 120)))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Fleet List")
                                .strong()
                                .size(14.0)
                                .color(egui::Color32::from_rgb(180, 210, 255)),
                        );
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt("fleet_list_scroll")
                            .max_height(available.y - 140.0)
                            .show(ui, |ui| {
                                render_fleet_list(
                                    ui,
                                    &fleet_query,
                                    &body_query,
                                    &mut fleet_ui_state,
                                    &mut pending_actions,
                                    elapsed,
                                );
                            });

                        ui.separator();
                        // ── Create Fleet section ─────────────────────────────
                        {
                            // Build sorted colony list grouped by star system
                            let mut colony_entries: Vec<(Entity, String, String)> = colony_query
                                .iter()
                                .filter_map(|(e, _colony)| {
                                    body_query.get(e).ok().map(|(_, body, _, _, _)| {
                                        let star = find_body_star_name(e, &body_query);
                                        (e, body.name.clone(), star)
                                    })
                                })
                                .collect();
                            colony_entries.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));

                            // Keep selection valid; default to selected fleet location → Earth → first colony
                            let selection_valid = fleet_ui_state.spawn_location_body
                                .map(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e))
                                .unwrap_or(false);
                            if !selection_valid {
                                let fallback = fleet_ui_state.selected_fleet
                                    .and_then(|sel| fleet_query.get(sel).ok().and_then(|(_, _, mo, _, _)| mo.map(|o| o.body)))
                                    .and_then(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e).then_some(e))
                                    .or_else(|| body_query.iter().find(|(_, b, _, _, _)| b.name == "Earth").map(|(e, _, _, _, _)| e)
                                        .and_then(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e).then_some(e)))
                                    .or_else(|| colony_entries.first().map(|(e, _, _)| *e));
                                fleet_ui_state.spawn_location_body = fallback;
                            }

                            let selected_label = fleet_ui_state.spawn_location_body
                                .and_then(|e| colony_entries.iter().find(|(ce, _, _)| *ce == e))
                                .map(|(_, name, star)| format!("{name} ({star})"))
                                .unwrap_or_else(|| "— No colony —".to_string());

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Location:")
                                        .size(12.0)
                                        .color(egui::Color32::GRAY),
                                );
                                if colony_entries.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No colonies yet")
                                            .size(12.0)
                                            .italics()
                                            .color(egui::Color32::DARK_GRAY),
                                    );
                                } else {
                                    egui::ComboBox::from_id_salt("create_fleet_location")
                                        .selected_text(egui::RichText::new(&selected_label).size(12.0))
                                        .width(210.0)
                                        .show_ui(ui, |ui| {
                                            let mut current_star = String::new();
                                            for (e, body_name, star_name) in &colony_entries {
                                                if *star_name != current_star {
                                                    current_star = star_name.clone();
                                                    ui.add_space(2.0);
                                                    ui.label(
                                                        egui::RichText::new(format!("★ {star_name}"))
                                                            .size(11.0)
                                                            .strong()
                                                            .color(egui::Color32::from_rgb(255, 220, 100)),
                                                    );
                                                }
                                                let is_sel = fleet_ui_state.spawn_location_body == Some(*e);
                                                if ui.selectable_label(
                                                    is_sel,
                                                    egui::RichText::new(format!("  {body_name}")).size(12.0),
                                                ).clicked() {
                                                    fleet_ui_state.spawn_location_body = Some(*e);
                                                }
                                            }
                                        });
                                }
                            });

                            if ui
                                .button(egui::RichText::new("＋ Create Fleet").size(13.0))
                                .clicked()
                            {
                                let spawn_body = fleet_ui_state.spawn_location_body.or_else(|| {
                                    body_query
                                        .iter()
                                        .find(|(_, b, _, _, _)| b.name == "Earth")
                                        .map(|(e, _, _, _, _)| e)
                                });
                                if let Some(body_entity) = spawn_body {
                                    let orbit_radius_au = 6_771.0_f64 * 1_000.0 / AU_IN_METERS;
                                    pending_actions.spawn_fleets.push(
                                        crate::fleets::components::SpawnFleetAction {
                                            name: format!("New Fleet {}", fleet_count + 1),
                                            ships: Vec::new(),
                                            orbit_body: body_entity,
                                            orbit_radius_au,
                                        },
                                    );
                                }
                            }
                        }
                    });
            });

            ui.add_space(8.0);

            // ── Right column: selected fleet details + transfer planner ──────
            let remaining = ui.available_width().min(480.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(remaining, available.y - 80.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                egui::Frame::default()
                    .inner_margin(egui::Margin::same(6i8))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 80, 120)))
                    .show(ui, |ui| {
                if let Some(selected) = fleet_ui_state.selected_fleet {
                    if let Ok((_, fleet, maybe_orbit, maybe_maneuver, _)) =
                        fleet_query.get(selected)
                    {
                        egui::ScrollArea::vertical()
                            .id_salt("fleet_detail_scroll")
                            .show(ui, |ui| {
                                render_fleet_detail(
                                    ui,
                                    selected,
                                    fleet,
                                    maybe_orbit,
                                    maybe_maneuver,
                                    &body_query,
                                    &mut fleet_ui_state,
                                    &mut pending_actions,
                                    elapsed,
                                );
                            });
                    } else {
                        // Selected entity no longer exists
                        fleet_ui_state.selected_fleet = None;
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(
                            egui::RichText::new("Select a fleet from the list to view details.")
                                .size(14.0)
                                .italics()
                                .color(egui::Color32::GRAY),
                        );
                    });
                }
                    });
            });
        });
    });

    // ── Disband confirmation popup ────────────────────────────────────────────
    if let Some(fleet_to_disband) = fleet_ui_state.disband_confirm_fleet {
        let fleet_info = fleet_query.get(fleet_to_disband).ok().map(|(_, f, _, _, _)| (f.name.clone(), f.ships.len()));
        if let Some((fleet_name, ship_count)) = fleet_info {
            let mut do_disband = false;
            let mut cancel = false;
            egui::Window::new("⚠ Confirm Disband")
                .id(egui::Id::new("fleet_disband_confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("⚠").size(36.0).color(egui::Color32::from_rgb(255, 180, 40)));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("Disband \"{}\"?", fleet_name))
                            .strong()
                            .size(15.0)
                            .color(egui::Color32::from_rgb(255, 220, 120)),
                    );
                    if ship_count > 0 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "This will permanently destroy {} ship(s).\nThis action cannot be undone.",
                                ship_count
                            ))
                            .size(13.0)
                            .color(egui::Color32::from_rgb(220, 120, 100)),
                        );
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("Cancel").size(13.0)).clicked() {
                            cancel = true;
                        }
                        ui.add_space(12.0);
                        if ui
                            .button(
                                egui::RichText::new("🗑 Disband")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(230, 80, 60)),
                            )
                            .clicked()
                        {
                            do_disband = true;
                        }
                    });
                    ui.add_space(4.0);
                });
            if do_disband {
                pending_actions.disband_fleets.push(fleet_to_disband);
                if fleet_ui_state.selected_fleet == Some(fleet_to_disband) {
                    fleet_ui_state.selected_fleet = None;
                }
                fleet_ui_state.selected_fleets.retain(|&e| e != fleet_to_disband);
                fleet_ui_state.disband_confirm_fleet = None;
            }
            if cancel {
                fleet_ui_state.disband_confirm_fleet = None;
            }
        } else {
            // Fleet no longer exists
            fleet_ui_state.disband_confirm_fleet = None;
        }
    }
}

/// Walk up the `LogicalParent` chain to find the star name for a body.
fn find_body_star_name(
    mut body_entity: Entity,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
) -> String {
    for _ in 0..10 {
        match body_query.get(body_entity) {
            Ok((_, body, _, _, maybe_parent)) => {
                if body.body_type == BodyType::Star {
                    return body.name.clone();
                }
                if let Some(LogicalParent(parent)) = maybe_parent {
                    body_entity = *parent;
                } else {
                    return body.name.clone();
                }
            }
            Err(_) => break,
        }
    }
    "Unknown System".to_string()
}

/// Render the scrollable list of fleets on the left side, grouped by star system.
fn render_fleet_list(
    ui: &mut egui::Ui,
    fleet_query: &Query<(
        Entity,
        &Fleet,
        Option<&FleetOrbit>,
        Option<&ActiveManeuver>,
        &SpaceCoordinates,
    )>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
) {
    struct FEntry {
        entity: Entity,
        ship_count: usize,
        role_icon: &'static str,
        name: String,
        in_transit: bool,
        star_name: String,
        location_text: String,
        fuel_pct: u32,
        transit_progress: Option<(u32, String)>,
        waiting_depart: Option<String>,
    }

    let mut entries: Vec<FEntry> = fleet_query
        .iter()
        .map(|(entity, fleet, maybe_orbit, maybe_maneuver, _)| {
            let in_transit = maybe_maneuver.is_some();
            let (location_text, star_name) = if let Some(orbit) = maybe_orbit {
                let body_name = body_query
                    .get(orbit.body)
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_default();
                let star = find_body_star_name(orbit.body, body_query);
                (format!("📍 {body_name}"), star)
            } else if let Some(man) = maybe_maneuver {
                let src = body_query
                    .get(man.origin_body)
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_default();
                let dst = body_query
                    .get(man.destination_body)
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_default();
                let star = find_body_star_name(man.destination_body, body_query);
                (format!("{src} → {dst}"), star)
            } else {
                ("Unknown".to_string(), "Unknown System".to_string())
            };

            let transit_progress = maybe_maneuver.and_then(|man| {
                if elapsed >= man.departure_time {
                    let prog = (man.progress(elapsed) * 100.0) as u32;
                    let rem = format_duration(man.time_remaining_s(elapsed));
                    Some((prog, rem))
                } else {
                    None
                }
            });
            let waiting_depart = maybe_maneuver.and_then(|man| {
                if elapsed < man.departure_time {
                    Some(format_duration(man.departure_time - elapsed))
                } else {
                    None
                }
            });

            FEntry {
                entity,
                ship_count: fleet.ships.len(),
                role_icon: if in_transit { "✈" } else { fleet.role.icon() },
                name: fleet.name.clone(),
                in_transit,
                star_name,
                location_text,
                fuel_pct: (fleet.fuel_fraction() * 100.0) as u32,
                transit_progress,
                waiting_depart,
            }
        })
        .collect();

    entries.sort_by(|a, b| a.star_name.cmp(&b.star_name).then(a.name.cmp(&b.name)));

    // Ordered list of entities (same order as display) for shift-range select.
    let sorted_entities: Vec<Entity> = entries.iter().map(|e| e.entity).collect();

    let mut current_system = String::new();
    for entry in &entries {
        // ── System header ─────────────────────────────────────────────────────
        if entry.star_name != current_system {
            current_system = entry.star_name.clone();
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("★  {current_system}"))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 220, 100)),
            );
        }

        let is_primary = fleet_ui_state.selected_fleet == Some(entry.entity);
        let is_checked = fleet_ui_state.selected_fleets.contains(&entry.entity);
        let row_text = format!("{} {} — {} ship(s)", entry.role_icon, entry.name, entry.ship_count);
        let row_color = if entry.in_transit {
            egui::Color32::from_rgb(100, 180, 255)
        } else {
            egui::Color32::from_rgb(100, 220, 100)
        };

        // ── Row: [checkbox] [drop-zone selectable] ────────────────────────────
        ui.horizontal(|ui| {
            let mut checked = is_checked;
            if ui.checkbox(&mut checked, "").changed() {
                if checked {
                    if !fleet_ui_state.selected_fleets.contains(&entry.entity) {
                        fleet_ui_state.selected_fleets.push(entry.entity);
                    }
                } else {
                    fleet_ui_state.selected_fleets.retain(|&e| e != entry.entity);
                }
            }

            let drop_result = ui.dnd_drop_zone::<(Entity, usize), _>(egui::Frame::NONE, |ui| {
                let resp = ui.selectable_label(
                    is_primary,
                    egui::RichText::new(&row_text).size(13.0).color(row_color),
                );
                if resp.clicked() {
                    let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                    let shift = ui.input(|i| i.modifiers.shift);
                    if ctrl {
                        if fleet_ui_state.selected_fleets.contains(&entry.entity) {
                            fleet_ui_state.selected_fleets.retain(|&e| e != entry.entity);
                        } else {
                            fleet_ui_state.selected_fleets.push(entry.entity);
                        }
                    } else if shift {
                        let anchor = fleet_ui_state.last_single_selected.unwrap_or(entry.entity);
                        let ai = sorted_entities.iter().position(|&e| e == anchor).unwrap_or(0);
                        let ci = sorted_entities.iter().position(|&e| e == entry.entity).unwrap_or(0);
                        let (lo, hi) = if ai <= ci { (ai, ci) } else { (ci, ai) };
                        for &e in &sorted_entities[lo..=hi] {
                            if !fleet_ui_state.selected_fleets.contains(&e) {
                                fleet_ui_state.selected_fleets.push(e);
                            }
                        }
                    } else {
                        if fleet_ui_state.selected_fleet == Some(entry.entity) {
                            fleet_ui_state.selected_fleet = None;
                        } else {
                            fleet_ui_state.selected_fleet = Some(entry.entity);
                            fleet_ui_state.clear_target();
                            fleet_ui_state.last_single_selected = Some(entry.entity);
                        }
                        fleet_ui_state.selected_fleets.clear();
                    }
                }
                resp
            });

            if let Some(payload) = drop_result.1 {
                let (source_fleet, ship_idx) = *payload;
                if source_fleet != entry.entity {
                    pending_actions.transfer_ships.push(
                        crate::fleets::components::TransferShipsAction {
                            source_fleet,
                            destination_fleet: entry.entity,
                            ship_indices: vec![ship_idx],
                        },
                    );
                }
            }
        });

        // ── Sub-status line ───────────────────────────────────────────────────
        let sub = if let Some(wait_str) = &entry.waiting_depart {
            egui::RichText::new(format!("    Waiting — T-minus {wait_str}"))
                .size(11.0)
                .color(egui::Color32::from_rgb(255, 200, 100))
        } else if let Some((prog, rem)) = &entry.transit_progress {
            egui::RichText::new(format!(
                "    ✈ {} — {}% done, {} left",
                entry.location_text, prog, rem
            ))
            .size(11.0)
            .color(egui::Color32::from_rgb(160, 190, 230))
        } else {
            egui::RichText::new(format!("    {} — fuel {}%", entry.location_text, entry.fuel_pct))
                .size(11.0)
                .color(egui::Color32::GRAY)
        };
        ui.label(sub);
    }

    // ── Multi-select action bar ───────────────────────────────────────────────
    let n = fleet_ui_state.selected_fleets.len();
    if n >= 2 {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{n} selected"))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(200, 220, 255)),
            );
            // All selected fleets must be in orbit at the same body (not in transit).
            let merge_bodies: Vec<Option<Entity>> = fleet_ui_state
                .selected_fleets
                .iter()
                .map(|&e| fleet_query.get(e).ok().and_then(|(_, _, mo, ma, _)| {
                    if ma.is_some() { None } else { mo.map(|o| o.body) }
                }))
                .collect();
            let all_same_location = !merge_bodies.is_empty()
                && merge_bodies[0].is_some()
                && merge_bodies.iter().all(|&b| b == merge_bodies[0]);
            let merge_tooltip = if all_same_location {
                "Merge into one fleet — the largest fleet keeps its name".to_string()
            } else {
                "Cannot merge: all fleets must be in orbit at the same location".to_string()
            };
            if ui
                .add_enabled(all_same_location, egui::Button::new(egui::RichText::new("⊕ Merge").size(13.0)))
                .on_hover_text(merge_tooltip)
                .clicked()
            {
                let target_fleet = fleet_ui_state
                    .selected_fleets
                    .iter()
                    .copied()
                    .max_by_key(|&e| fleet_query.get(e).map(|(_, f, _, _, _)| f.ships.len()).unwrap_or(0));
                if let Some(target_fleet) = target_fleet {
                    let source_fleets = fleet_ui_state
                        .selected_fleets
                        .iter()
                        .copied()
                        .filter(|&e| e != target_fleet)
                        .collect();
                    pending_actions.merge_fleets.push(MergeFleetAction { source_fleets, target_fleet });
                    fleet_ui_state.selected_fleet = Some(target_fleet);
                    fleet_ui_state.clear_multi_selection();
                    fleet_ui_state.clear_target();
                }
            }
            if ui
                .button(egui::RichText::new("✕ Clear").size(12.0))
                .clicked()
            {
                fleet_ui_state.clear_multi_selection();
            }
        });
    }

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("💡 Drag ship name → fleet to transfer  ·  Ctrl/⌘+click or Shift+click to multi-select")
            .size(10.0)
            .italics()
            .color(egui::Color32::from_rgb(120, 140, 170)),
    );
}

/// Render the fleet name with a marquee (ticker) scroll effect when the name is
/// too wide to fit in `max_width` pixels.  Uses `ui.input(|i| i.time)` for
/// real-time animation so it runs regardless of simulation speed.
/// Appends the ✏ rename button and writes into `editing_fleet_name` on click.
fn render_fleet_name_marquee(
    ui: &mut egui::Ui,
    fleet: &Fleet,
    fleet_entity: Entity,
    max_width: f32,
    editing_fleet_name: &mut Option<(Entity, String)>,
) {
    let name_text = format!("{} {}", fleet.role.icon(), fleet.name);
    let font_id = egui::FontId::proportional(18.0);
    let name_color = egui::Color32::from_rgb(200, 230, 255);

    // Measure the full text width at the desired font size.
    let full_width = ui
        .painter()
        .layout_no_wrap(name_text.clone(), font_id.clone(), name_color)
        .size()
        .x;

    if full_width <= max_width {
        // Fits entirely — plain label.
        ui.label(
            egui::RichText::new(&name_text)
                .strong()
                .size(18.0)
                .color(name_color),
        );
    } else {
        // Continuous marquee: scroll left at constant speed, loop seamlessly.
        // Two copies of the text are painted side-by-side separated by a gap;
        // the offset cycles over (full_width + gap) so the join is invisible.
        let gap = 72.0_f32;
        let cycle = (full_width + gap) as f64;
        let speed = 50.0_f64; // px / real-second
        let t = ui.input(|i| i.time);
        let offset_x = ((t * speed) % cycle) as f32;

        let text_height = ui
            .painter()
            .layout_no_wrap(name_text.clone(), font_id.clone(), name_color)
            .size()
            .y;
        let widget_height = text_height.max(24.0);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(max_width, widget_height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect); // clips to rect automatically
        let y = rect.top() + (rect.height() - text_height) * 0.5;

        // Re-layout for painting (layout_no_wrap needs Arc<Galley>)
        let galley2 = painter.layout_no_wrap(name_text.clone(), font_id.clone(), name_color);
        let x0 = rect.left() - offset_x;
        painter.galley(egui::pos2(x0, y), galley2.clone(), name_color);
        let x1 = x0 + (full_width + gap);
        if x1 < rect.right() + full_width {
            painter.galley(egui::pos2(x1, y), galley2, name_color);
        }

        ui.ctx().request_repaint();
    }

    if ui.button("✏").on_hover_text("Rename Fleet").clicked() {
        *editing_fleet_name = Some((fleet_entity, fleet.name.clone()));
    }
}

/// Render the right panel: fleet details (ship manifest, stats, status) and transfer planner.
#[allow(clippy::too_many_arguments)]
fn render_fleet_detail(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    maybe_orbit: Option<&FleetOrbit>,
    maybe_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
) {
    // ── Fleet header ─────────────────────────────────────────────────────────
    // Row 1: fleet name + ✏ button (full width, no competing right-side controls)
    ui.horizontal(|ui| {
        let name_area_width = (ui.available_width() - 32.0).max(60.0);

        let is_editing_this = fleet_ui_state.editing_fleet_name
            .as_ref()
            .map(|(e, _)| *e == fleet_entity)
            .unwrap_or(false);

        if is_editing_this {
            let (committed_name, should_cancel) = {
                if let Some((_, ref mut current_name)) = fleet_ui_state.editing_fleet_name {
                    let response = ui.add_sized(
                        [name_area_width, 24.0],
                        egui::TextEdit::singleline(current_name),
                    );
                    let cancelled = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let committed = response.lost_focus() && !cancelled;
                    if !committed && !cancelled {
                        response.request_focus();
                    }
                    (if committed { Some(current_name.clone()) } else { None }, cancelled)
                } else {
                    (None, false)
                }
            };
            if let Some(name) = committed_name {
                pending_actions.rename_fleets.push((fleet_entity, name));
                fleet_ui_state.editing_fleet_name = None;
            } else if should_cancel {
                fleet_ui_state.editing_fleet_name = None;
            }
        } else {
            render_fleet_name_marquee(ui, fleet, fleet_entity, name_area_width, &mut fleet_ui_state.editing_fleet_name);
        }
    });

    // Row 2: Role selector + Disband (right-aligned)
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(egui::RichText::new("🗑 Disband").color(egui::Color32::from_rgb(220, 80, 60)))
                .on_hover_text(if fleet.ships.is_empty() { "Disband this fleet" } else { "Disband fleet (destroys all ships)" })
                .clicked()
            {
                fleet_ui_state.disband_confirm_fleet = Some(fleet_entity);
            }
            egui::ComboBox::from_id_salt("fleet_role_combo")
                .selected_text(fleet.role.display_name())
                .show_ui(ui, |ui| {
                    use crate::fleets::types::FleetRole;
                    let roles = [
                        FleetRole::Unassigned,
                        FleetRole::Attack,
                        FleetRole::Defend,
                        FleetRole::Survey,
                        FleetRole::Transport,
                        FleetRole::Explore,
                    ];
                    for role in roles {
                        if ui.selectable_label(
                            fleet.role == role,
                            format!("{} {}", role.icon(), role.display_name()),
                        ).clicked() {
                            pending_actions.change_fleet_roles.push((fleet_entity, role));
                        }
                    }
                });
            ui.label("Role:");
        });
    });
    ui.separator();

    // ── Current status ────────────────────────────────────────────────────────
    if let Some(maneuver) = maybe_maneuver {
        render_active_maneuver_status(ui, fleet_entity, maneuver, fleet, body_query, pending_actions, elapsed, fleet_ui_state.waiting_orbit_count);
    } else if let Some(orbit) = maybe_orbit {
        render_orbit_status(ui, orbit, fleet, body_query);
    }

    ui.separator();

    // ── Ship manifest ─────────────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Ship Manifest")
            .strong()
            .size(14.0),
    );
    let in_orbit_for_manifest = maybe_orbit.is_some();
    egui::Grid::new("ship_manifest")
        .num_columns(8)
        .spacing([12.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            // Header row
            ui.label(egui::RichText::new("Name").strong().size(12.0));
            ui.label(egui::RichText::new("Class").strong().size(12.0));
            ui.label(egui::RichText::new("Dry (t)").strong().size(12.0));
            ui.label(egui::RichText::new("Fuel").strong().size(12.0));
            ui.label(egui::RichText::new("Drive").strong().size(12.0));
            ui.label(egui::RichText::new("Thrust").strong().size(12.0));
            ui.label(egui::RichText::new("Max ΔV").strong().size(12.0));
            ui.label(egui::RichText::new("Actions").strong().size(12.0));
            ui.end_row();

            for (idx, ship) in fleet.ships.iter().enumerate() {
                let drag_id = egui::Id::new("drag_ship").with(fleet_entity).with(idx);
                ui.dnd_drag_source(drag_id, (fleet_entity, idx), |ui| {
                    ui.label(egui::RichText::new(&ship.name).size(12.0));
                });
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        ship.class.icon(),
                        ship.class.display_name()
                    ))
                    .size(12.0),
                );
                ui.label(
                    egui::RichText::new(format!("{:.0}", ship.dry_mass_t)).size(12.0),
                );
                let fuel_pct = (ship.fuel_fraction() * 100.0) as u32;
                let fuel_color = if fuel_pct > 50 {
                    egui::Color32::from_rgb(100, 220, 100)
                } else if fuel_pct > 20 {
                    egui::Color32::from_rgb(220, 180, 60)
                } else {
                    egui::Color32::from_rgb(220, 80, 60)
                };
                ui.label(
                    egui::RichText::new(format!("{fuel_pct}%"))
                        .size(12.0)
                        .color(fuel_color),
                );
                ui.label(
                    egui::RichText::new(ship.propulsion.display_name()).size(12.0),
                );
                ui.label(
                    egui::RichText::new(format!("{:.0} kN", ship.thrust_kn)).size(12.0),
                );
                ui.label(
                    egui::RichText::new(format_delta_v(ship.delta_v_ms())).size(12.0),
                );
                // Refuel button — enabled only while in a stable orbit.
                // Currently fills to max for free (debug). In future will
                // draw propellant from the orbited body's stockpile.
                let refuel_resp = ui.add_enabled(
                    in_orbit_for_manifest,
                    egui::Button::new(egui::RichText::new("⛽ Refuel").size(11.0))
                        .min_size(egui::Vec2::new(60.0, 18.0)),
                );
                if refuel_resp
                    .on_hover_text(if in_orbit_for_manifest {
                        "Refuel this ship to full capacity (free — debug)"
                    } else {
                        "Cannot refuel while in transit"
                    })
                    .clicked()
                {
                    pending_actions.refuel_fleets.push(fleet_entity);
                }
                ui.end_row();
            }
        });

    // ── Fleet aggregate stats ─────────────────────────────────────────────────
    ui.add_space(6.0);
    egui::Grid::new("fleet_stats")
        .num_columns(4)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Dry mass:").size(12.0));
            ui.label(
                egui::RichText::new(format!("{:.0} t", fleet.total_dry_mass_t()))
                    .size(12.0)
                    .strong(),
            );
            ui.label(egui::RichText::new("Fuel:").size(12.0));
            let fuel_pct = (fleet.fuel_fraction() * 100.0) as u32;
            ui.label(
                egui::RichText::new(format!(
                    "{:.0} t ({fuel_pct}%)",
                    fleet.total_fuel_t()
                ))
                .size(12.0)
                .strong(),
            );
            ui.end_row();

            ui.label(egui::RichText::new("Min thrust:").size(12.0));
            ui.label(
                egui::RichText::new(format!("{:.0} kN", fleet.min_thrust_kn()))
                    .size(12.0)
                    .strong(),
            );
            ui.label(egui::RichText::new("Max ΔV:").size(12.0));
            ui.label(
                egui::RichText::new(format_delta_v(fleet.max_delta_v_ms()))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 220, 255)),
            );
            ui.end_row();
        });

    // ── Transfer Planner shortcut ─────────────────────────────────────────
    // The planner now lives in a floating popup; show a button to open it.
    let can_plan = maybe_orbit.is_some()
        || maybe_maneuver.is_some();
    if can_plan {
        ui.separator();
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📡 Open Transfer Planner ↗").size(13.0),
                )
                .min_size(egui::Vec2::new(200.0, 32.0)),
            )
            .on_hover_text("Open the orbital transfer planner in a floating window")
            .clicked()
        {
            fleet_ui_state.show_transfer_popup = true;
        }
    }
}

// (Transfer Planner is now a floating popup — see ui_transfer_planner_popup.)

/// Show current maneuver status with a progress bar.
fn render_active_maneuver_status(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    maneuver: &ActiveManeuver,
    fleet: &Fleet,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
    waiting_orbit_count: u32,
) {
    let dest_name = body_query
        .get(maneuver.destination_body)
        .map(|(_, b, _, _, _)| b.name.as_str())
        .unwrap_or("Unknown");

    if elapsed < maneuver.departure_time {
        let wait_time = maneuver.departure_time - elapsed;
        let wait_str = format_duration(wait_time);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("⏳ Waiting to depart for {dest_name}"))
                        .strong()
                        .size(14.0)
                        .color(egui::Color32::from_rgb(255, 200, 100)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("T-minus {}", wait_str))
                            .size(12.0)
                            .color(egui::Color32::GRAY),
                    );
                });
            });

            // Orbit-wait counter from the trajectory gizmo.
            if waiting_orbit_count > 1 {
                ui.label(
                    egui::RichText::new(format!("× {} orbits until departure angle", waiting_orbit_count))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(160, 80, 220)),
                );
            }

            ui.add_space(4.0);
            if ui.button(egui::RichText::new("🛑 Abort Mission").size(12.0)).clicked() {
                pending_actions.cancel_maneuvers.push(fleet_entity);
            }
        });
        return;
    }

    let progress = maneuver.progress(elapsed) as f32;
    let remaining = format_duration(maneuver.time_remaining_s(elapsed));

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("✈ En Route → {dest_name}"))
                    .strong()
                    .size(14.0)
                    .color(egui::Color32::from_rgb(100, 200, 255)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} remaining", remaining))
                        .size(12.0)
                        .color(egui::Color32::GRAY),
                );
            });
        });

        ui.add(
            egui::ProgressBar::new(progress)
                .text(format!("{:.1}%", progress * 100.0))
                .desired_width(ui.available_width()),
        );

        egui::Grid::new("maneuver_info")
            .num_columns(4)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Option:").size(12.0));
                ui.label(
                    egui::RichText::new(maneuver.option_label).size(12.0).strong(),
                );
                ui.label(egui::RichText::new("Arrival ΔV:").size(12.0));
                ui.label(
                    egui::RichText::new(format_delta_v(maneuver.arrival_delta_v_ms))
                        .size(12.0)
                        .strong(),
                );
                ui.end_row();

                ui.label(egui::RichText::new("Fuel used:").size(12.0));
                ui.label(
                    egui::RichText::new(format!("{:.0} t", maneuver.fuel_used_t))
                        .size(12.0)
                        .strong(),
                );
                let fuel_pct = (fleet.fuel_fraction() * 100.0) as u32;
                ui.label(egui::RichText::new("Remaining fuel:").size(12.0));
                ui.label(
                    egui::RichText::new(format!("{fuel_pct}%"))
                        .size(12.0)
                        .strong(),
                );
                ui.end_row();
            });
    });
}

/// Show stable orbit information.
fn render_orbit_status(
    ui: &mut egui::Ui,
    orbit: &FleetOrbit,
    fleet: &Fleet,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
) {
    let (body_name, body_type) = body_query
        .get(orbit.body)
        .map(|(_, b, _, _, _)| (b.name.as_str(), b.body_type))
        .unwrap_or(("Unknown", BodyType::Planet));

    let fuel_pct = (fleet.fuel_fraction() * 100.0) as u32;

    // For star-orbiting fleets (Lagrange points), display heliocentric orbital
    // radius in AU rather than an altitude in km (which would be nonsensical
    // at interplanetary scales).
    let altitude_label;
    let altitude_value;
    if body_type == BodyType::Star {
        altitude_label = "Orbital radius:";
        altitude_value = format!("{:.4} AU", orbit.radius_au);
    } else {
        let radius_km = orbit.radius_au * AU_IN_METERS / 1_000.0;
        altitude_label = "Altitude:";
        altitude_value = format!("{:.0} km", radius_km);
    };

    // Label: for star-orbiting fleets say "at Lagrange point" to make it clear.
    let status_label = if body_type == BodyType::Star {
        format!("🛰 Lagrange Orbit ({body_name})")
    } else {
        format!("🛰 Orbiting {body_name}")
    };

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(status_label)
                    .strong()
                    .size(14.0)
                    .color(egui::Color32::from_rgb(100, 220, 100)),
            );
        });
        egui::Grid::new("orbit_info")
            .num_columns(4)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new(altitude_label).size(12.0));
                ui.label(
                    egui::RichText::new(altitude_value)
                        .size(12.0)
                        .strong(),
                );
                ui.label(egui::RichText::new("Fuel:").size(12.0));
                ui.label(
                    egui::RichText::new(format!("{fuel_pct}%"))
                        .size(12.0)
                        .strong(),
                );
                ui.end_row();
            });
    });
}


/// Floating popup window showing the Transfer Planner over the 3D view.
///
/// Opened by the "Transfer Planner" button in the fleet action bar or the Fleet Management
/// panel shortcut button. Closed with the window's × button or by deselecting the fleet.
pub(super) fn ui_transfer_planner_popup(
    mut contexts: EguiContexts,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    all_fleets_query: Query<(Entity, &Fleet, &SpaceCoordinates, Option<&FleetOrbit>, Option<&ActiveManeuver>), Without<CelestialBody>>,
    body_query: Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    body_system_ids: Query<&SystemId>,
    mut pending_actions: ResMut<PendingFleetActions>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    sim_time: Res<SimulationTime>,
    current_system: Res<CurrentStarSystem>,
    nearby_stars: Res<NearbyStarsData>,
) {
    if !fleet_ui_state.show_transfer_popup {
        return;
    }

    let Some(fleet_entity) = fleet_ui_state.selected_fleet else {
        fleet_ui_state.show_transfer_popup = false;
        return;
    };

    let Ok((_, fleet, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else {
        fleet_ui_state.show_transfer_popup = false;
        return;
    };

    let planner_orbit: Option<FleetOrbit> = if let Some(orbit) = maybe_orbit {
        Some(*orbit)
    } else if let Some(maneuver) = maybe_maneuver {
        // Use origin_body (the departure body) rather than destination_body.
        // If we used destination_body and the user re-targets the same destination,
        // r1 == r2 and the planner shows "Same orbit, 0 m/s" with zero travel time.
        Some(FleetOrbit::new(maneuver.origin_body, maneuver.arrival_orbit_radius_au))
    } else {
        None
    };

    // For course corrections, pass the fleet's actual current heliocentric position
    // so the planner can show accurate ΔV from the fleet's real location, not the
    // origin body's orbit.  Only set for fleets that have actually departed
    // (elapsed >= departure_time); waiting-to-depart fleets still use normal planner mode.
    let course_correction_sc = if let Some(man) = maybe_maneuver {
        if sim_time.elapsed_seconds() >= man.departure_time {
            all_fleets_query
                .get(fleet_entity)
                .ok()
                .map(|(_, _, sc, _, _)| sc.position)
        } else {
            None
        }
    } else {
        None
    };

    let Some(orbit) = planner_orbit else {
        fleet_ui_state.show_transfer_popup = false;
        return;
    };

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let elapsed = sim_time.elapsed_seconds();
    let current_system_id = current_system.0;

    // `open` is a separate local bool — `Window::open()` sets it to false when
    // the user clicks the × close button.
    let mut open = true;
    egui::Window::new(format!("📡 Transfer Planner — {}", fleet.name))
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .default_width(460.0)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 135.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(600.0)
                .show(ui, |ui| {
                    render_transfer_planner(
                        ui,
                        fleet_entity,
                        fleet,
                        &orbit,
                        maybe_maneuver,
                        &body_query,
                        &all_fleets_query,
                        &mut fleet_ui_state,
                        &mut pending_actions,
                        current_system_id,
                        &body_system_ids,
                        elapsed,
                        &nearby_stars,
                        sim_time.current_timestamp(),
                        course_correction_sc,
                    );
                });
        });

    if !open {
        fleet_ui_state.show_transfer_popup = false;
    }
}



// ── Fleet action bar (bottom overlay) ────────────────────────────────────────

/// Renders a thin action bar at the bottom of the 3D view whenever a fleet is
/// selected.  The bar is hidden in the full-screen Fleets panel, Research,
/// Construction, and Economy menus because those already fill the screen.
pub(super) fn ui_fleet_action_bar(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    mut pending_fleet_actions: ResMut<PendingFleetActions>,
    sim_time: Res<SimulationTime>,
) {
    // Only show when a fleet is selected AND we are NOT inside a full-screen panel.
    let Some(selected_entity) = fleet_ui_state.selected_fleet else {
        return;
    };

    if matches!(
        active_menu.current,
        GameMenu::Fleets
            | GameMenu::Research
            | GameMenu::Construction
            | GameMenu::Economy
    ) {
        return;
    }

    let Ok((_, fleet, maybe_orbit, maybe_maneuver)) = fleet_query.get(selected_entity) else {
        return; // Fleet was despawned
    };

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // `in_transit` was previously used by the old action bar logic but
    // the status string now derives directly from `maybe_maneuver`.
    let in_orbit = maybe_orbit.is_some();
    let ship_count = fleet.ships.len();
    // Only show abort when the fleet is waiting to depart (still has its parking orbit).
    // Canceling mid-flight is unsupported: there is no FleetOrbit to return to.
    let is_waiting_for_departure = maybe_maneuver
        .map(|m| sim_time.elapsed_seconds() < m.departure_time)
        .unwrap_or(false);

    // Determine which ship-class-dependent actions are available.
    // For now all friendly fleets can survey; combat actions are always shown
    // (hostile fleet detection comes in a future PR).
    let has_ships = ship_count > 0;

    egui::TopBottomPanel::bottom("fleet_action_bar")
        .min_height(48.0)
        .max_height(76.0)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);

                // Fleet name + status label (+ progress bar / ETA when in transit)
                let elapsed = sim_time.elapsed_seconds();
                let status_str = if let Some(maneuver) = maybe_maneuver {
                    if elapsed < maneuver.departure_time {
                        " ⏳ Waiting to depart".to_string()
                    } else {
                        " ✈ In Transit".to_string()
                    }
                } else {
                    " 🛰 In Orbit".to_string()
                };

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("🚀 {} —{status_str}", fleet.name))
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(130, 220, 130)),
                    );
                    // Progress bar + ETA date for actively transiting fleets
                    if let Some(maneuver) = maybe_maneuver {
                        if elapsed >= maneuver.departure_time {
                            let progress = maneuver.progress(elapsed) as f32;
                            let eta_str = sim_time.format_arrival_date(maneuver.arrival_time);
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::ProgressBar::new(progress)
                                        .text(format!("{:.1}%", progress * 100.0))
                                        .desired_width(160.0),
                                );
                                ui.label(
                                    egui::RichText::new(format!("ETA {eta_str}"))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(100, 180, 255)),
                                );
                            });
                        }
                    }
                });

                ui.separator();

                // Transfer Planner — always available
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("🗺 Transfer Planner").size(13.0),
                        )
                        .min_size(egui::Vec2::new(140.0, 36.0)),
                    )
                    .on_hover_text("Open the orbital transfer planner for this fleet")
                    .clicked()
                {
                    fleet_ui_state.show_transfer_popup = true;
                }

                ui.add_space(4.0);

                // Split Fleet — only useful when in orbit and has > 1 ship
                let can_split = in_orbit && ship_count > 1;
                if ui
                    .add_enabled(
                        can_split,
                        egui::Button::new(egui::RichText::new("✂ Split Fleet").size(13.0))
                            .min_size(egui::Vec2::new(110.0, 36.0)),
                    )
                    .on_hover_text("Detach selected ships into a new fleet")
                    .clicked()
                {
                    // Stub: split action will be fully implemented in a future update
                    info!("Split fleet requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Merge Fleet — only when in orbit (merging in transit is not possible)
                let can_merge = in_orbit;
                if ui
                    .add_enabled(
                        can_merge,
                        egui::Button::new(egui::RichText::new("🔗 Merge Fleet").size(13.0))
                            .min_size(egui::Vec2::new(110.0, 36.0)),
                    )
                    .on_hover_text("Merge with another fleet at the same location")
                    .clicked()
                {
                    // Stub: merge action will be fully implemented in a future update
                    info!("Merge fleet requested for {:?}", selected_entity);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Survey — available when in orbit
                if ui
                    .add_enabled(
                        in_orbit && has_ships,
                        egui::Button::new(egui::RichText::new("🔭 Survey").size(13.0))
                            .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text("Survey the body this fleet is orbiting")
                    .clicked()
                {
                    info!("Survey requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Attack — available when in transit or orbit
                if ui
                    .add_enabled(
                        has_ships,
                        egui::Button::new(
                            egui::RichText::new("⚔ Attack")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(230, 130, 100)),
                        )
                        .min_size(egui::Vec2::new(80.0, 36.0)),
                    )
                    .on_hover_text("Engage enemy vessels in combat")
                    .clicked()
                {
                    info!("Attack requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Bombard — requires orbit
                if ui
                    .add_enabled(
                        in_orbit && has_ships,
                        egui::Button::new(
                            egui::RichText::new("💣 Bombard")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(230, 130, 100)),
                        )
                        .min_size(egui::Vec2::new(90.0, 36.0)),
                    )
                    .on_hover_text("Bombard the surface of the body being orbited")
                    .clicked()
                {
                    info!("Bombard requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Invade — requires orbit
                if ui
                    .add_enabled(
                        in_orbit && has_ships,
                        egui::Button::new(
                            egui::RichText::new("👊 Invade")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(230, 130, 100)),
                        )
                        .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text("Land troops to take control of the colony")
                    .clicked()
                {
                    info!("Invade requested for {:?}", selected_entity);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Refuel — only when in stable orbit
                let needs_fuel = fleet.ships.iter().any(|s| s.fuel_mass_t < s.max_fuel_t);
                if ui
                    .add_enabled(
                        in_orbit && needs_fuel,
                        egui::Button::new(egui::RichText::new("⛽ Refuel").size(13.0))
                            .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text(if in_orbit {
                        if needs_fuel {
                            "Refuel all ships to full capacity"
                        } else {
                            "All ships are already at full fuel"
                        }
                    } else {
                        "Cannot refuel while in transit"
                    })
                    .clicked()
                {
                    pending_fleet_actions.refuel_fleets.push(selected_entity);
                }

                // Abort button — only while waiting to depart (fleet still has its parking orbit)
                if is_waiting_for_departure {
                    ui.add_space(8.0);
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("⛔ Abort Transfer")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(220, 80, 80)),
                            )
                            .min_size(egui::Vec2::new(120.0, 36.0)),
                        )
                        .on_hover_text("Abort the planned transfer and return to parking orbit")
                        .clicked()
                    {
                        pending_fleet_actions.cancel_maneuvers.push(selected_entity);
                    }
                }
            });
        });
}

/// Seamlessly re-anchors the camera from a transiting fleet entity to its
/// destination body the moment the maneuver completes.
///
/// When the player double-clicks an in-transit fleet, the camera is anchored
/// to the fleet entity itself so it follows the moving dot.  This system
/// detects `ActiveManeuver` removal (fired by `complete_fleet_maneuvers`) and,
/// if the camera anchor is still pointing at that fleet, redirects it to the
/// newly-inserted `FleetOrbit.body` — typically the destination planet or moon.
pub(super) fn switch_anchor_on_arrival(
    mut removed: RemovedComponents<ActiveManeuver>,
    fleet_query: Query<&FleetOrbit>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
) {
    let Ok(mut anchor) = anchor_query.single_mut() else {
        return;
    };

    for fleet_entity in removed.read() {
        // Only act when this fleet is the current anchor target.
        if anchor.0 != Some(fleet_entity) {
            continue;
        }
        // Redirect to the destination body; fall back to no anchor if not found.
        anchor.0 = fleet_query
            .get(fleet_entity)
            .ok()
            .map(|orbit| orbit.body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_scale_creation() {
        let time_scale = TimeScale::new();
        assert_eq!(time_scale.scale, 1.0);
        assert!(!time_scale.is_paused());
    }

    #[test]
    fn test_time_scale_pause() {
        let mut time_scale = TimeScale::new();
        time_scale.pause();

        assert!(time_scale.is_paused());
        assert_eq!(time_scale.scale, 0.0);
    }

    #[test]
    fn test_time_scale_resume() {
        let mut time_scale = TimeScale::new();
        time_scale.scale = 100.0;
        time_scale.pause();
        time_scale.resume();

        assert!(!time_scale.is_paused());
        assert_eq!(time_scale.scale, 100.0);
    }

    #[test]
    fn test_selection_basics() {
        let selection = Selection::new();
        assert!(!selection.has_selection());

        let mut selection = Selection::new();
        let entity = Entity::from_raw_u32(1).unwrap();
        selection.select(entity);

        assert!(selection.has_selection());
        assert_eq!(selection.get(), Some(entity));
    }
}
