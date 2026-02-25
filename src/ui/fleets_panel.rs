use super::*;
use super::time::format_timestamp_date_time;

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
        render_active_maneuver_status(ui, fleet_entity, maneuver, fleet, body_query, pending_actions, elapsed);
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

/// Transfer planning sub-panel: choose a destination and transfer option.
#[allow(clippy::too_many_arguments)]
fn render_transfer_planner(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    current_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    all_fleets_query: &Query<(Entity, &Fleet, &SpaceCoordinates, Option<&FleetOrbit>, Option<&ActiveManeuver>), Without<CelestialBody>>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    current_system_id: usize,
    body_system_ids: &Query<&SystemId>,
    elapsed: f64,
    nearby_stars: &NearbyStarsData,
    current_timestamp: i64,
) {
    let is_course_correction = current_maneuver.is_some();

    if is_course_correction {
        ui.label(
            egui::RichText::new("🔄 Course Correction")
                .strong()
                .size(15.0)
                .color(egui::Color32::from_rgb(255, 200, 80)),
        );
        ui.label(
            egui::RichText::new("Redirecting mid-transit burns additional fuel for the abort maneuver.")
                .size(11.0)
                .italics()
                .color(egui::Color32::GRAY),
        );
    } else {
        ui.label(
            egui::RichText::new("📡 Orbital Transfer Planner")
                .strong()
                .size(15.0)
                .color(egui::Color32::from_rgb(200, 220, 255)),
        );
    }
    ui.separator();

    // ── Hierarchical destination selector ────────────────────────────────────
    // DestEntry variants:
    //   Header — non-clickable category label; separator drawn BEFORE it (but not the very first)
    //   Body   — selectable destination
    //   Ring   — selectable ring destination (no KeplerOrbit; radius from body.radius field)
    //   Lagrange — one of the 5 L-points of a planet-star system
    //   FleetTarget — another fleet (for intercept course)
    //   StarSystem — interstellar target (another star system)
    #[derive(Clone)]
    enum DestEntry {
        Header(String),
        Body { entity: Entity, name: String },
        // Rings are treated like regular bodies for selection; the extra
        // parent/radius information used to be stored here but never read.
        Ring { entity: Entity, name: String },
        // TODO(lagrange-transfers): variant kept so the match arm compiles; re-enable construction when ready.
        #[allow(dead_code)]
        Lagrange { lp: LagrangeTarget },
        FleetTarget { entity: Entity, name: String, in_transit: bool },
        StarSystem { system_id: usize, name: String, distance_ly: f32 },
    }

    let mut dest_entries: Vec<DestEntry> = Vec::new();

    // Collect all valid candidate bodies (exclude Star, include Ring)
    // For Rings: sma = None (no KeplerOrbit); radius stored via body.radius field separately.
    let candidates: Vec<(Entity, String, BodyType, Option<f64>, Option<Entity>)> = body_query
        .iter()
        .filter_map(|(e, body, _, maybe_ko, maybe_lp)| {
            if e == orbit.body { return None; }
            if body.body_type == BodyType::Star { return None; }
            if !body_system_ids.get(e).ok().map(|s| s.0 == current_system_id).unwrap_or(false) {
                return None;
            }
            let sma = maybe_ko.map(|ko| ko.semi_major_axis);
            let parent = maybe_lp.map(|lp| lp.0);
            Some((e, body.name.clone(), body.body_type, sma, parent))
        })
        .collect();

    // Separate ring bodies out; they lack KeplerOrbits so need special handling
    let ring_candidates: Vec<(Entity, String, Option<Entity>, f64)> = body_query
        .iter()
        .filter_map(|(e, body, _, _, maybe_lp)| {
            if body.body_type != BodyType::Ring { return None; }
            if !body_system_ids.get(e).ok().map(|s| s.0 == current_system_id).unwrap_or(false) {
                return None;
            }
            let parent = maybe_lp.map(|lp| lp.0)?;
            // Use body.radius (km) as the representative ring orbit distance from planet centre
            let radius_au = (body.radius as f64 * 1_000.0) / AU_IN_METERS;
            Some((e, body.name.clone(), Some(parent), radius_au))
        })
        .collect();

    // ── Group 1: bodies that directly orbit the fleet's current body ──────────
    {
        let orbit_body_name = body_query.get(orbit.body)
            .map(|(_, b, _, _, _)| b.name.clone()).unwrap_or_default();
        let mut local: Vec<(Entity, String, f64)> = candidates.iter()
            .filter(|(_, _, btype, _, parent)| {
                *parent == Some(orbit.body) && *btype != BodyType::Ring
            })
            .filter_map(|(e, name, _, sma, _)| sma.map(|s| (*e, name.clone(), s)))
            .collect();
        // Rings around the current orbit body
        let mut local_rings: Vec<(Entity, String, Option<Entity>, f64)> = ring_candidates.iter()
            .filter(|(_, _, parent, _)| *parent == Some(orbit.body))
            .cloned().collect();
        if !local.is_empty() || !local_rings.is_empty() {
            local.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            local_rings.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            dest_entries.push(DestEntry::Header(format!("{orbit_body_name} System")));
            for (e, name, _) in &local {
                dest_entries.push(DestEntry::Body { entity: *e, name: name.clone() });
            }
            for (e, name, parent, _radius_au) in local_rings {
                if parent.is_some() {
                    dest_entries.push(DestEntry::Ring { entity: e, name });
                }
            }
        }

        // TODO(lagrange-transfers): Re-enable Sun-Planet and Planet-Moon Lagrange
        // point entries in this dropdown once transfer planning is working.
        // Search for TODO(lagrange-transfers) to find all related disabled code.
    }

    // ── Groups 2+: planet systems (moons/rings orbiting a planet that isn't fleet's body) ──
    let mut planet_map: std::collections::BTreeMap<String, (Entity, f64, Vec<(Entity, String, f64, bool)>)> =
        std::collections::BTreeMap::new();

    // Regular moons / small bodies orbiting a planet
    for (e, name, btype, sma, parent) in &candidates {
        if *btype == BodyType::Ring { continue; }
        let parent_e = match parent { Some(p) => *p, None => continue };
        if parent_e == orbit.body { continue; }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star { continue; }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            if let Some(s) = sma {
                planet_map.entry(pb.name.clone())
                    .or_insert_with(|| (parent_e, parent_sma, vec![]))
                    .2.push((*e, name.clone(), *s, false)); // false = not a ring
            }
        }
    }
    // Rings orbiting a planet that isn't the fleet's body
    for (e, name, parent_opt, radius_au) in &ring_candidates {
        let parent_e = match parent_opt { Some(p) => *p, None => continue };
        if parent_e == orbit.body { continue; }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star { continue; }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            planet_map.entry(pb.name.clone())
                .or_insert_with(|| (parent_e, parent_sma, vec![]))
                .2.push((*e, name.clone(), *radius_au, true)); // true = ring
        }
    }

    let mut sorted_planet_systems: Vec<_> = planet_map.into_iter().collect();
    sorted_planet_systems.sort_by(|a, b| a.1.1.partial_cmp(&b.1.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut planets_shown = std::collections::HashSet::<Entity>::new();
    for (planet_name, (parent_e, _parent_sma, mut children)) in sorted_planet_systems {
        planets_shown.insert(parent_e);
        children.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header(format!("{planet_name} System")));
        if orbit.body != parent_e {
            dest_entries.push(DestEntry::Body { entity: parent_e, name: planet_name.clone() });
        }
        for (e, name, _sma, is_ring) in &children {
            if *is_ring {
                dest_entries.push(DestEntry::Ring {
                    entity: *e,
                    name: name.clone(),
                });
            } else {
                dest_entries.push(DestEntry::Body { entity: *e, name: name.clone() });
            }
        }
        // TODO(lagrange-transfers): Re-enable planet and moon Lagrange point
        // sub-groups in this dropdown once transfer planning is working.
    }

    // ── Group: Planets/GasGiants not yet shown (no children found in data) ───
    let already_listed: std::collections::HashSet<Entity> = dest_entries.iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();

    let mut standalone: Vec<(Entity, String, f64)> = candidates.iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet)
                && sma.is_some()
                && !planets_shown.contains(e)
                && !already_listed.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !standalone.is_empty() {
        standalone.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header("Planets".to_string()));
        for (e, name, _) in standalone {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Small bodies ─────────────────────────────────────────────────
    let already_listed2: std::collections::HashSet<Entity> = dest_entries.iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();
    let mut small_bodies: Vec<(Entity, String, f64)> = candidates.iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Asteroid | BodyType::Comet)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !small_bodies.is_empty() {
        small_bodies.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let sb_label = if small_bodies.len() > 5 {
            format!("Small Bodies ({} total)", small_bodies.len())
        } else {
            "Small Bodies".to_string()
        };
        dest_entries.push(DestEntry::Header(sb_label));
        for (e, name, _) in small_bodies {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Solar Approach ────────────────────────────────────────────────
    // Always offer a direct solar-approach destination so the player can plot
    // an inward heliocentric transfer toward the star.  Filter by current_system_id
    // to find Sol, not Alpha Centauri or another star from a different system.
    let star_entity = body_query.iter()
        .find(|(e, b, _, _, _)| {
            b.body_type == BodyType::Star
                && body_system_ids.get(*e).ok().map(|s| s.0 == current_system_id).unwrap_or(false)
        })
        .map(|(e, _, _, _, _)| e);
    if let Some(star_e) = star_entity {
        dest_entries.push(DestEntry::Header("Solar".to_string()));
        dest_entries.push(DestEntry::Body {
            entity: star_e,
            name: "☀ Solar Approach (0.3 AU)".to_string(),
        });
    }

    // ── Group: Interstellar ──────────────────────────────────────────────────
    // List every other star system from NearbyStarsData as an interstellar target.
    // The current system is identified by its numeric id; Sol = id 0 by convention.
    {
        let mut interstellar_entries: Vec<DestEntry> = nearby_stars.systems
            .iter()
            .filter(|sys| {
                // Exclude the current system (id comparison via name match is a fallback)
                // NearbyStarsData systems use 0-based index ordering; system_id 0 = Sol.
                // We exclude any system whose name matches current system's star name.
                let this_star_name = body_query.iter()
                    .find(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids.get(*e).ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| b.name.as_str())
                    .unwrap_or("Sol");
                // Each StarSystemData has stars[0].name; compare to current star
                !sys.stars.iter().any(|s| s.name == this_star_name)
                    && sys.distance_ly > 0.0
            })
            .enumerate()
            .map(|(idx, sys)| {
                let display = format!("✨ {} ({:.2} ly)", sys.system_name, sys.distance_ly);
                // Use index+1 as system_id (0 reserved for Sol in current system)
                DestEntry::StarSystem {
                    system_id: idx + 1,
                    name: display,
                    distance_ly: sys.distance_ly,
                }
            })
            .collect();

        if !interstellar_entries.is_empty() {
            interstellar_entries.sort_by(|a, b| {
                let da = if let DestEntry::StarSystem { distance_ly, .. } = a { *distance_ly } else { 0.0 };
                let db = if let DestEntry::StarSystem { distance_ly, .. } = b { *distance_ly } else { 0.0 };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            dest_entries.push(DestEntry::Header("Interstellar".to_string()));
            dest_entries.extend(interstellar_entries);
        }
    }

    // ── Build hierarchical categories from dest_entries ─────────────────────
    // Top-level headers ("…System", "Small Bodies", "Heliocentric") become
    // category names in the first-level picker. Lagrange sub-headers are kept
    // as visual separators inside each category group.
    #[derive(Clone)]
    struct DestGroup {
        name: String,
        entries: Vec<DestEntry>,
    }

    let mut groups: Vec<DestGroup> = Vec::new();
    for entry in dest_entries {
        let is_top_header = match &entry {
            DestEntry::Header(label) => {
                label.ends_with(" System")
                    || label == "Planets"
                    || label == "Solar"
                    || label == "Interstellar"
                    || label.starts_with("Small Bodies")
            }
            _ => false,
        };
        if is_top_header {
            let name = match &entry {
                DestEntry::Header(label) => {
                    label.strip_suffix(" System").unwrap_or(label).to_string()
                }
                _ => unreachable!(),
            };
            groups.push(DestGroup { name, entries: Vec::new() });
        } else if let Some(g) = groups.last_mut() {
            g.entries.push(entry);
        }
    }

    // ── Fleet intercept category ─────────────────────────────────────────────
    {
        let other_fleets: Vec<(Entity, String, bool)> = all_fleets_query
            .iter()
            .filter(|(e, _, _, _, _)| *e != fleet_entity)
            .map(|(e, f, _, _, maybe_ma)| (e, f.name.clone(), maybe_ma.is_some()))
            .collect();
        if !other_fleets.is_empty() {
            let mut fleet_group = DestGroup { name: "Fleets".to_string(), entries: Vec::new() };
            // In-orbit fleets first
            for (e, name, in_transit) in &other_fleets {
                fleet_group.entries.push(DestEntry::FleetTarget {
                    entity: *e,
                    name: name.clone(),
                    in_transit: *in_transit,
                });
            }
            groups.push(fleet_group);
        }
    }

    // ── Auto-select category if a target is selected ─────────────────────────
    let mut correct_category = None;
    if let Some(target) = fleet_ui_state.target_body {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => *entity == target,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(ref lp) = fleet_ui_state.target_lagrange {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Lagrange { lp: entry_lp } => entry_lp.point == lp.point && entry_lp.planet_entity == lp.planet_entity,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::FleetTarget { entity, .. } => *entity == tf,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some((tss_id, _, _)) = fleet_ui_state.target_star_system {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::StarSystem { system_id, .. } => *system_id == tss_id,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    }

    if let Some(cat) = correct_category {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        if sel != Some(&cat) && !(sel == Some("Small Bodies") && cat.starts_with("Small Bodies")) {
            fleet_ui_state.selected_dest_category = Some(cat);
        }
    }

    // ── Render the two-level selector ────────────────────────────────────────
    // Step 1: category (planet system / small bodies / fleets)
    let cat_label = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    }).map(|g| g.name.clone()).unwrap_or_else(|| fleet_ui_state.selected_dest_category.clone().unwrap_or_else(|| "— System —".to_owned()));

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("System:").size(13.0));
        egui::ComboBox::from_id_salt("fleet_dest_category")
            .selected_text(&cat_label)
            .width(200.0)
            .show_ui(ui, |ui| {
                for group in &groups {
                    let sel = fleet_ui_state.selected_dest_category.as_deref();
                    let cat_is_sel = sel == Some(&group.name) || (sel == Some("Small Bodies") && group.name.starts_with("Small Bodies"));
                    if ui.selectable_label(
                        cat_is_sel,
                        egui::RichText::new(&group.name).size(13.0),
                    ).clicked() && !cat_is_sel {
                        fleet_ui_state.selected_dest_category = Some(group.name.clone());
                        // Clear the specific target so the second step is re-selected
                        fleet_ui_state.target_body = None;
                        fleet_ui_state.target_lagrange = None;
                        fleet_ui_state.target_fleet = None;
                        fleet_ui_state.target_star_system = None;
                        fleet_ui_state.computed_options.clear();
                        fleet_ui_state.planned_transfer = None;
                        fleet_ui_state.selected_option = 0;
                        fleet_ui_state.selected_gravity_assist = None;
                    }
                }
            });
    });

    // Step 2: specific target within selected category
    let active_group = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    });

    let target_label = if let Some(ref lp) = fleet_ui_state.target_lagrange {
        format!("L{} {} — {}", lp.point, lp.planet_name, lp.qualifier())
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        all_fleets_query.get(tf)
            .map(|(_, f, _, _, ma)| {
                let status = if ma.is_some() { "✈" } else { "🛰" };
                format!("{status} {}", f.name)
            })
            .unwrap_or_else(|_| "— Target —".to_owned())
    } else if let Some((_, ref name, _)) = fleet_ui_state.target_star_system {
        name.clone()
    } else {
        fleet_ui_state.target_body
            .and_then(|e| body_query.get(e).ok())
            .map(|(_, b, _, _, _)| {
                if b.body_type == BodyType::Ring {
                    format!("{} 💍", b.name)
                } else {
                    b.name.clone()
                }
            })
            .unwrap_or_else(|| "— Target —".to_owned())
    };

    if active_group.is_some() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Target:").size(13.0));
            egui::ComboBox::from_id_salt("fleet_target_body")
                .selected_text(&target_label)
                .width(280.0)
                .show_ui(ui, |ui| {
                    if let Some(group) = active_group {
                        let mut first_sub = true;
                        for entry in &group.entries {
                            match entry {
                                DestEntry::Header(label) => {
                                    if !first_sub { ui.add_space(4.0); }
                                    first_sub = false;
                                    ui.label(
                                        egui::RichText::new(label.as_str())
                                            .strong()
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(180, 180, 100)),
                                    );
                                }
                                DestEntry::Body { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui.selectable_label(
                                        selected,
                                        egui::RichText::new(format!("  {name}")).size(12.0),
                                    ).clicked() && !selected {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::Ring { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui.selectable_label(
                                        selected,
                                        egui::RichText::new(format!("  {name} 💍")).size(12.0),
                                    ).clicked() && !selected {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::Lagrange { lp: _ } => {
                                    // TODO(lagrange-transfers): Lagrange-point transfers are
                                    // temporarily disabled. The LP markers are still rendered
                                    // and selectable for viewing, but cannot be chosen as a
                                    // fleet transfer destination until the transfer planner
                                    // for L-points is fully working. Re-enable by restoring
                                    // the DestEntry::Lagrange branch here and in
                                    // ui_lp_click_handler / astronomy::systems::hover_lagrange_points.
                                }
                                DestEntry::FleetTarget { entity, name, in_transit } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_fleet == Some(*entity);
                                    let icon = if *in_transit { "✈" } else { "🛰" };
                                    let status = if *in_transit { "In transit" } else { "In orbit" };
                                    let label = format!("  {icon} {name}  ({status})");
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(label)
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(100, 210, 240)),
                                    ).clicked() && !is_sel {
                                        fleet_ui_state.target_fleet = Some(*entity);
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_star_system = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::StarSystem { system_id, name, distance_ly } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_star_system
                                        .as_ref().map(|(id, _, _)| *id == *system_id)
                                        .unwrap_or(false);
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(format!("  {name}"))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(200, 180, 255)),
                                    ).clicked() && !is_sel {
                                        fleet_ui_state.target_star_system = Some((*system_id, name.clone(), *distance_ly));
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                            }
                        }
                    }
                });
        });
    }

    // ── Intercept parameters (shown only when a fleet is targeted) ────────────
    if fleet_ui_state.target_fleet.is_some() {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("⚔ Intercept Parameters")
                    .strong()
                    .size(13.0)
                    .color(egui::Color32::from_rgb(220, 160, 80)),
            );
            ui.add_space(4.0);

            // Passing distance slider: 0 = rendezvous / dock, up to 1 000 km = fast flyby
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Passing distance:").size(12.0));
                let mut pd = fleet_ui_state.intercept_passing_km as f32;
                if ui.add(
                    egui::Slider::new(&mut pd, 0.0_f32..=1_000.0_f32)
                        .suffix(" km")
                        .text("0 = rendezvous")
                        .step_by(10.0),
                ).changed() {
                    fleet_ui_state.intercept_passing_km = pd as f64;
                    fleet_ui_state.computed_options.clear();
                }
            });

            // Encounter speed: 0 = match velocity (boarding), up to 30 km/s = high-speed pass
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Encounter speed:").size(12.0));
                let mut spd_kms = (fleet_ui_state.intercept_speed_ms / 1_000.0) as f32;
                if ui.add(
                    egui::Slider::new(&mut spd_kms, 0.0_f32..=30.0_f32)
                        .suffix(" km/s")
                        .text("0 = match velocity")
                        .step_by(0.5),
                ).changed() {
                    fleet_ui_state.intercept_speed_ms = spd_kms as f64 * 1_000.0;
                    fleet_ui_state.computed_options.clear();
                }
            });

            ui.label(
                egui::RichText::new(
                    if fleet_ui_state.intercept_passing_km < 1.0 && fleet_ui_state.intercept_speed_ms < 100.0 {
                        "Mode: Rendezvous / docking approach"
                    } else if fleet_ui_state.intercept_passing_km > 100.0 || fleet_ui_state.intercept_speed_ms > 5_000.0 {
                        "Mode: High-speed flyby (combat pass)"
                    } else {
                        "Mode: Close approach (boarding range)"
                    }
                )
                .size(11.0)
                .italics()
                .color(egui::Color32::from_rgb(160, 200, 160)),
            );
        });
    }

    // ── Compute transfer options when a target is selected ───────────────────
    let fleet_target_snap = fleet_ui_state.target_fleet;
    let star_system_snap = fleet_ui_state.target_star_system.clone();
    let any_target = fleet_ui_state.target_body.is_some()
        || fleet_ui_state.target_lagrange.is_some()
        || fleet_target_snap.is_some()
        || star_system_snap.is_some();
    // Snapshot lagrange so we can use it immutably while also mut-borrowing fleet_ui_state below
    let lp_target_snap = fleet_ui_state.target_lagrange.clone();
    let body_target_snap = fleet_ui_state.target_body;

    // Transfer window info computed this frame (Some only for body-target transfers).
    // Kept as a local so the window UI section can read it without re-computing.
    let mut window_this_frame: Option<TransferWindowInfo> = None;
    let mut window_max_slider_days: f64 = 730.0;

    if any_target {
        // Recompute every frame — body angles (SpaceCoordinates) update with the simulation clock,
        // so the phase error and launch-window countdown change live.

        // ── Fleet intercept computation ──────────────────────────────────────
        if let Some(target_fleet_entity) = fleet_target_snap {
            // Use the target fleet's current heliocentric position as the intercept radius.
            // r2 = distance from origin (0,0,0) to target fleet position in AU.
            let target_sc = all_fleets_query.get(target_fleet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(bevy::math::DVec3::ZERO);
            let r2_au = target_sc.length().max(0.001);

            // r1: heliocentric distance of the departing fleet
            let r1_au = {
                let own_ko = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let origin_parent = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);
                if own_ko.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_ko)
                        .unwrap_or(1.0)
                } else {
                    own_ko.unwrap_or(1.0)
                }
            };
            fleet_ui_state.computed_options = calculate_transfer_options(r1_au, r2_au, GM_SUN, 0.0);
            // Post-process: fill burn_time_s and flag thrust-limited options.
            apply_thrust_limits(
                &mut fleet_ui_state.computed_options,
                fleet.min_accel_ms2(),
                fleet.average_isp_s(),
            );
            // Add kinematic options for high-thrust fleets intercepting other fleets.
            let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
            let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
            let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
            let d = (r2_au - r1_au).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
            let mut kinematics = kinematic_transfer_options(
                d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                hohmann_dv, sma_h, ecc_h, false
            );
            fleet_ui_state.computed_options.append(&mut kinematics);
        } else if let Some(target_entity) = body_target_snap {
            //   - Ring transfer (dest has no KeplerOrbit; use body.radius as r2):
            //       r1 = fleet orbit radius or origin SMA, r2 = ring.radius_au, GM = parent mass * G
            //   - Local transfer (dest orbits fleet's body, e.g. Earth→Moon):
            //       r1 = fleet's parking orbit radius, r2 = dest SMA, GM = parent mass * G
            //   - Moon-to-moon (both orbit the same planet):
            //       r1 = origin moon SMA, r2 = dest moon SMA, GM = shared planet mass * G
            //   - Solar approach (dest is a star):
            //       r1 = fleet's heliocentric SMA, r2 = 0.3 AU, GM = GM_SUN
            //   - Heliocentric transfer (both in heliocentric orbits):
            //       r1 = origin body heliocentric SMA, r2 = dest heliocentric SMA, GM_SUN
            let dest_body_type = body_query.get(target_entity).ok()
                .map(|(_, b, _, _, _)| b.body_type);
            let dest_has_orbit = body_query.get(target_entity).ok()
                .and_then(|(_, _, _, ko, _)| ko).is_some();
            let dest_parent = body_query.get(target_entity).ok()
                .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);
            let origin_parent = body_query.get(orbit.body).ok()
                .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);

            // Target solar approach orbit (AU from star).  Inside Mercury's orbit so the
            // transfer is always clearly "inward".  Requires advanced propulsion (~10–20 km/s).
            const SOLAR_APPROACH_AU: f64 = 0.3;

            let (r1, r2, gm) = if dest_body_type == Some(BodyType::Star) {
                // Heliocentric inward transfer: plot a Hohmann from the fleet's heliocentric
                // distance to SOLAR_APPROACH_AU using GM_SUN as the central-body parameter.
                // Walk up the parent chain to find the fleet's heliocentric SMA.
                let own_sma = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r1_au = if own_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Fleet is parked at a moon/sub-body; use its planet's heliocentric SMA.
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                } else {
                    own_sma.unwrap_or(1.0)
                };
                // Ensure r2 is strictly less than r1 (always an inward transfer).
                let r2_au = SOLAR_APPROACH_AU.min(r1_au * 0.5);
                (r1_au, r2_au, GM_SUN)
            } else if !dest_has_orbit && dest_parent == Some(orbit.body) {
                // Ring around current orbit body
                let parent_mass = body_query.get(orbit.body).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if !dest_has_orbit && dest_parent.is_some() && dest_parent == origin_parent {
                // Ring around another planet (dest_parent is a planet, not fleet's body)
                let shared = dest_parent.unwrap();
                let parent_mass = body_query.get(shared).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (r1, r2, G_CONST * parent_mass)
            } else if dest_parent == Some(orbit.body) {
                // Local: destination orbits the fleet's current body
                let parent_mass = body_query.get(orbit.body).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r2 = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if dest_parent.is_some() && dest_parent == origin_parent {
                // Both orbit the same central body (moon-to-moon, OR interplanetary e.g. Earth→Mars)
                let shared = dest_parent.unwrap();
                // NOTE: The Sun lacks SpaceCoordinates, so body_query.get(Sun) fails.
                // Fall back to GM_SUN so interplanetary transfers compute correctly.
                let gm = body_query.get(shared).ok()
                    .map(|(_, b, _, _, _)| {
                        if b.body_type == BodyType::Star { GM_SUN } else { G_CONST * b.mass }
                    })
                    .unwrap_or(GM_SUN);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                let r2 = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                (r1, r2, gm)
            } else if Some(target_entity) == origin_parent {
                // Downward transfer: fleet is at a moon, destination is the parent planet.
                // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
                let parent_mass = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                // Park at ~3× destination body surface radius (low orbit).
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 3_000.0) / AU_IN_METERS)
                    .unwrap_or(4.26e-5);
                (r1, r2.min(r1 * 0.5), G_CONST * parent_mass)
            } else {
                // Heliocentric: fleet is at a body that is not in the same parent chain as dest.
                // If fleet is parked at a moon, its KeplerOrbit SMA is Earth-relative, NOT
                // heliocentric. Walk up one level to get the heliocentric SMA.
                let own_sma = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r1 = if own_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Small SMA → likely a moon; use its parent's heliocentric SMA
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                } else {
                    own_sma.unwrap_or(1.0)
                };
                let dest_sma = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r2 = if dest_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Small SMA → likely a moon; use its parent's heliocentric SMA
                    dest_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(dest_sma)
                        .unwrap_or(1.5)
                } else {
                    dest_sma.unwrap_or(1.5)
                };
                (r1, r2, GM_SUN)
            };
            fleet_ui_state.computed_options = {
                // Extract angles of origin and destination bodies in the correct coordinate system.
                let is_heliocentric = (gm - GM_SUN).abs() < 1e10;
                // Moon → parent-planet case: target IS the body that origin orbits around.
                let is_moon_to_parent = Some(target_entity) == origin_parent;

                let get_heliocentric_pos = |entity: Entity| -> Option<bevy::math::DVec3> {
                    let entry = body_query.get(entity).ok()?;
                    let is_moon = entry.1.body_type == BodyType::Moon;
                    if is_moon {
                        let parent_entity = entry.4?.0;
                        let parent_entry = body_query.get(parent_entity).ok()?;
                        Some(parent_entry.2.position)
                    } else {
                        Some(entry.2.position)
                    }
                };

                let get_local_pos = |entity: Entity, central_body: Entity| -> Option<bevy::math::DVec3> {
                    if entity == central_body {
                        Some(bevy::math::DVec3::ZERO)
                    } else {
                        let entry = body_query.get(entity).ok()?;
                        Some(entry.2.position)
                    }
                };

                let (pos1, pos2) = if is_moon_to_parent {
                    // Moon→parent: use Moon's position relative to the parent planet.
                    // The parent planet is at the centre of the local frame.
                    let moon_helio = body_query.get(orbit.body).ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    let planet_helio = body_query.get(target_entity).ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    (Some(moon_helio - planet_helio), Some(bevy::math::DVec3::ZERO))
                } else if is_heliocentric {
                    (get_heliocentric_pos(orbit.body), get_heliocentric_pos(target_entity))
                } else {
                    let central_body = dest_parent.unwrap_or(orbit.body);
                    (get_local_pos(orbit.body, central_body), get_local_pos(target_entity, central_body))
                };

                let theta1 = pos1.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);
                let theta2 = pos2.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);

                // Compute transfer window from live positions
                let window = compute_transfer_window(r1, r2, gm, theta1, theta2);
                window_max_slider_days = if window.synodic_period_s.is_finite() {
                    (window.synodic_period_s / 86_400.0 * 1.5).max(1.0)
                } else {
                    730.0
                };
                // Consume the "auto-set to next window" signal (departure_offset_days == -1.0)
                // that is set when the player first right-clicks a target body.  We resolve it
                // here — after the window is computed but before departure_s is used — so the
                // slider, quality indicator, and phased options all start at the optimal position.
                if fleet_ui_state.departure_offset_days < 0.0 {
                    fleet_ui_state.departure_offset_days =
                        (window.time_to_window_s / 86_400.0).max(0.0);
                }
                // Compute orbital-plane difference between origin and destination.
                // Mirrors the (r1, r2, gm) case logic above so the right pair of
                // KeplerOrbits is diffed in the correct reference frame.
                let delta_i: f64 = {
                    let origin_ko = body_query.get(orbit.body).ok().and_then(|(_, _, _, ko, _)| ko);
                    let dest_ko   = body_query.get(target_entity).ok().and_then(|(_, _, _, ko, _)| ko);

                    if dest_body_type == Some(BodyType::Star) || Some(target_entity) == origin_parent {
                        // Inward heliocentric or moon→parent: report inclination of the
                        // departure body's orbit (fleet is already in that plane).
                        // Plane change equals what is needed to depart the current orbital plane.
                        origin_ko.map(|ko| ko.inclination).unwrap_or(0.0)
                    } else if dest_parent == Some(orbit.body) {
                        // Fleet at planet, going to one of its moons.
                        dest_ko.map(|ko| ko.inclination).unwrap_or(0.0)
                    } else if dest_parent.is_some() && dest_parent == origin_parent {
                        // Both share a parent (moon-to-moon, OR interplanetary Earth→Mars).
                        match (origin_ko, dest_ko) {
                            (Some(o), Some(d)) => plane_change_angle(
                                o.inclination, o.longitude_ascending_node,
                                d.inclination, d.longitude_ascending_node,
                            ),
                            _ => 0.0,
                        }
                    } else {
                        // Heliocentric: walk up from moons to their heliocentric parents.
                        let helio_origin_ko = if origin_ko.map(|ko| ko.semi_major_axis < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                            origin_parent.and_then(|pe| body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko))
                        } else {
                            origin_ko
                        };
                        let helio_dest_ko = if dest_ko.map(|ko| ko.semi_major_axis < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                            dest_parent.and_then(|pe| body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko))
                        } else {
                            dest_ko
                        };
                        match (helio_origin_ko, helio_dest_ko) {
                            (Some(o), Some(d)) => plane_change_angle(
                                o.inclination, o.longitude_ascending_node,
                                d.inclination, d.longitude_ascending_node,
                            ),
                            _ => 0.0,
                        }
                    }
                };

                let departure_s = fleet_ui_state.departure_offset_days * 86_400.0;
                let opts = calculate_transfer_options_phased(r1, r2, gm, departure_s, &window, delta_i);
                window_this_frame = Some(window);
                opts
            };
            // Post-process: fill burn_time_s, flag thrust-limited options,
            // and add kinematic options for high-thrust fleets.
            {
                let accel = fleet.min_accel_ms2();
                let isp = fleet.average_isp_s();
                apply_thrust_limits(&mut fleet_ui_state.computed_options, accel, isp);
                
                let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                let d = (r2 - r1).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d, accel, fleet.max_delta_v_ms(),
                    hohmann_dv, sma_h, ecc_h, false
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            }
            // ── Gravity assist candidates (heliocentric transfers only) ─────────
            // Collect planets between r1 and r2, compute two-leg patched-conic options.
            // Only meaningful when GM ≈ GM_SUN (genuinely heliocentric transfer).
            if (gm - GM_SUN).abs() < 1e10 && !is_course_correction {
                let ga_bodies: Vec<(String, f64, f64, f64)> = body_query
                    .iter()
                    .filter_map(|(e, body, _, maybe_ko, _)| {
                        if !matches!(body.body_type,
                            BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet)
                        { return None; }
                        // Exclude the fleet's current body and the chosen destination
                        if e == orbit.body || Some(e) == body_target_snap { return None; }
                        // Only consider planets/bodies in the current star system
                        if body_system_ids.get(e).map(|s| s.0).unwrap_or(0) != current_system_id {
                            return None;
                        }
                        let sma = maybe_ko?.semi_major_axis;
                        if sma < MIN_HELIOCENTRIC_SMA_AU { return None; }
                        let gm_p = G_CONST * body.mass;
                        // Safe flyby periapsis: 3 × body radius (km → m → AU)
                        let min_peri = (body.radius as f64 * 3_000.0) / AU_IN_METERS;
                        Some((body.name.clone(), sma, gm_p, min_peri.max(1e-6)))
                    })
                    .collect();

                let new_candidates: Vec<GravityAssistEntry> =
                    find_gravity_assist_options(r1, r2, gm, &ga_bodies)
                    .into_iter()
                    .filter_map(|opt| {
                        // Resolve each candidate to its ECS entity by name
                        let entity = body_query
                            .iter()
                            .find(|(_, b, _, _, _)| b.name == opt.body_name)
                            .map(|(e, _, _, _, _)| e)?;
                        Some(GravityAssistEntry { option: opt, flyby_entity: entity })
                    })
                    .collect();

                fleet_ui_state.gravity_assist_candidates = new_candidates;

                // Validate selected index is still in-range (target may have changed)
                if fleet_ui_state.selected_gravity_assist
                    .map(|i| i >= fleet_ui_state.gravity_assist_candidates.len())
                    .unwrap_or(false)
                {
                    fleet_ui_state.selected_gravity_assist = None;
                }
            } else {
                fleet_ui_state.gravity_assist_candidates.clear();
                fleet_ui_state.selected_gravity_assist = None;
            }

            // If a gravity assist is selected, prepend it as option 0 so the
            // regular execute/select logic treats it uniformly.
            if let Some(sel_ga) = fleet_ui_state.selected_gravity_assist {
                let ga_data = fleet_ui_state.gravity_assist_candidates.get(sel_ga)
                    .map(|e| (
                        e.option.total_dv_ms,
                        e.option.total_time_s,
                        e.option.flyby_radius_au,
                        e.option.dv_depart_ms + e.option.dv_mid_ms, // departure + mid-course
                        e.option.dv_arrive_ms,
                    ));
                if let Some((total_dv, total_time, fly_r, dv1, dv2)) = ga_data {
                    // Use Leg-2 Hohmann parameters for the transfer-orbit visualization
                    // (the arc the fleet actually flies after the flyby).
                    let (_, _, _, ga_sma, ga_ecc) = hohmann_transfer(fly_r, r2, gm);
                    let burn_t = compute_burn_time_s(total_dv, fleet.min_accel_ms2(), fleet.average_isp_s());
                    // Gravity-assist options use multi-leg patched-conic timing; the burn
                    // is spread across two legs so we apply the thrust-limit check here.
                    let (ga_transfer_time, ga_thrust_limited) = if burn_t > 0.0 && burn_t > total_time {
                        (burn_t, true)
                    } else {
                        (total_time, false)
                    };
                    let ga_option = TransferOption {
                        label: "Gravity Assist",
                        total_delta_v_ms: total_dv,
                        delta_v1_ms: dv1,   // actual departure + any mid-course burn
                        delta_v2_ms: dv2,   // actual arrival circularisation
                        plane_change_dv_ms: 0.0, // gravity-assist paths are heliocentric (ecliptic)
                        transfer_time_s: ga_transfer_time,
                        sma_au: ga_sma,     // Leg-2 ellipse SMA for arc rendering
                        eccentricity: ga_ecc,
                        energy_multiplier: 1.0,
                        burn_time_s: burn_t,
                        is_thrust_limited: ga_thrust_limited,
                    };
                    fleet_ui_state.computed_options.insert(0, ga_option);
                }
            }
            } else if let Some(ref lp) = lp_target_snap {
                // Lagrange-point transfer.
                // Determine the fleet's current heliocentric SMA, walking up to
                // the planet's SMA when the fleet is parked at a moon/sub-body.
                // When orbiting the star directly (e.g. after a previous LP transfer),
                // use the fleet's parking radius if available, otherwise the LP planet's SMA.
                let r1_lp = body_query.get(orbit.body).ok()
                    .and_then(|(_, body, _, ko, _)| {
                        if body.body_type == BodyType::Star {
                            // Fleet parked around the star — use its parking orbit radius
                            // or fall back to the target LP's planet SMA.
                            if orbit.radius_au > 0.01 {
                                Some(orbit.radius_au)
                            } else {
                                Some(lp.planet_sma_au)
                            }
                        } else {
                            ko.map(|ko| ko.semi_major_axis)
                        }
                    })
                    .or_else(|| {
                        body_query.get(orbit.body).ok()
                            .and_then(|(_, _, _, _, parent)| parent)
                            .and_then(|lpp| body_query.get(lpp.0).ok()
                                .and_then(|(_, _, _, ko, _)| ko)
                                .map(|ko| ko.semi_major_axis))
                    })
                    .unwrap_or(lp.planet_sma_au);

                // L3/L4/L5 are co-orbital with the planet (same heliocentric radius,
                // different phase angle).  A Hohmann gives 0 Delta-V in this case.
                // Use a phasing-orbit maneuver instead: lower into a shorter-period
                // orbit and drift the 60 deg (L4/L5) or 180 deg (L3) phase gap in N laps.
                let co_orbital = matches!(lp.point, 3 | 4 | 5)
                    && (r1_lp - lp.planet_sma_au).abs() < 0.02;

                if co_orbital {
                    let delta_phi = if lp.point == 3 {
                        std::f64::consts::PI           // L3: 180 deg opposition
                    } else {
                        std::f64::consts::FRAC_PI_3    // L4/L5: 60 deg
                    };
                    fleet_ui_state.computed_options =
                        co_orbital_phasing_options(lp.planet_sma_au, lp.gm, delta_phi);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    // Kinematic options: arc-length of the phase drift as proxy distance.
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(r1_lp);
                    let d = lp.planet_sma_au * delta_phi * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, 0.0, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                } else if matches!(lp.point, 1 | 2) {
                    // L1/L2: small radial offset from planet (~r_hill ≈ 0.01 AU).
                    // Use a direct manifold-like trajectory (realistic ~1–3 month travel
                    // time) instead of a Hohmann half-orbit that takes 6 months and arrives
                    // 180° away from the LP.
                    fleet_ui_state.computed_options =
                        direct_lp_transfer_options(r1_lp, lp.radius_au, lp.gm);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                } else {
                    // L3/L4/L5 cross-orbit (fleet NOT co-orbital with the planet):
                    // standard Hohmann Keplerian transfer to the planet's SMA.
                    fleet_ui_state.computed_options =
                        calculate_transfer_options(r1_lp, lp.radius_au, lp.gm, 0.0);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                }
            }

        // ── Interstellar transfer computation ───────────────────────────────
        if let Some((_, _, distance_ly)) = star_system_snap {
            use crate::fleets::orbital_mechanics::{AU_IN_METERS, TransferOption};
            // 1 ly = 63 241.077 AU
            const AU_PER_LY: f64 = 63_241.077;
            let distance_m  = distance_ly as f64 * AU_PER_LY * AU_IN_METERS;
            let accel       = fleet.min_accel_ms2();
            let max_dv      = fleet.max_delta_v_ms();

            fleet_ui_state.computed_options.clear();

            let mut kinematics = kinematic_transfer_options(
                distance_m, accel, max_dv,
                0.0, 0.0, 0.0, true
            );
            fleet_ui_state.computed_options.append(&mut kinematics);

            if fleet_ui_state.computed_options.is_empty() {
                // Fleet lacks the minimum thrust for interstellar travel
                fleet_ui_state.computed_options.push(TransferOption {
                    label: "Insufficient thrust",
                    total_delta_v_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    plane_change_dv_ms: 0.0,
                    transfer_time_s: f64::INFINITY,
                    sma_au: 0.0,
                    eccentricity: 0.0,
                    energy_multiplier: 0.0,
                    burn_time_s: 0.0,
                    is_thrust_limited: true,
                });
            }
        }

        // ── Transfer Window info + departure slider ─────────────────────────
        // Show a co-orbital / L-point info section for Lagrange targets.
        if window_this_frame.is_none() && lp_target_snap.is_some() {
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                // Left: Lagrange transfer info
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        let lp = lp_target_snap.as_ref().unwrap();
                        // Determine actual transfer type — same logic as the computation
                        // section above.  L3/L4/L5 are co-orbital only when the fleet is
                        // already near the planet's SMA (within 0.02 AU).
                        let r1_info = body_query.get(orbit.body).ok()
                            .and_then(|(_, body, _, ko, _)| {
                                if body.body_type == BodyType::Star {
                                    if orbit.radius_au > 0.01 { Some(orbit.radius_au) }
                                    else { Some(lp.planet_sma_au) }
                                } else { ko.map(|k| k.semi_major_axis) }
                            })
                            .or_else(|| {
                                body_query.get(orbit.body).ok()
                                    .and_then(|(_, _, _, _, parent)| parent)
                                    .and_then(|lpp| body_query.get(lpp.0).ok()
                                        .and_then(|(_, _, _, ko, _)| ko)
                                        .map(|ko| ko.semi_major_axis))
                            })
                            .unwrap_or(lp.planet_sma_au);
                        let is_co_orbital = matches!(lp.point, 3 | 4 | 5)
                            && (r1_info - lp.planet_sma_au).abs() < 0.02;
                        let is_l12_direct = matches!(lp.point, 1 | 2);
                        if is_co_orbital {
                            ui.label(
                                egui::RichText::new("⟳ Co-orbital Phasing")
                                    .strong().size(12.0)
                                    .color(egui::Color32::from_rgb(160, 210, 255)),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new("Depart any time")
                                    .size(12.0).strong()
                                    .color(egui::Color32::from_rgb(80, 220, 80)),
                            );
                            ui.label(
                                egui::RichText::new("Fleet drifts in a slightly\nlower orbit to cover the\nphase gap over N laps.")
                                    .size(10.0).color(egui::Color32::GRAY),
                            );
                        } else if is_l12_direct {
                            ui.label(
                                egui::RichText::new("🎯 Direct LP Transfer")
                                    .strong().size(12.0)
                                    .color(egui::Color32::from_rgb(160, 210, 255)),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(egui::Color32::from_rgb(200, 200, 200)),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(egui::Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new("Low-energy manifold trajectory\nto the Lagrange equilibrium.")
                                    .size(10.0).color(egui::Color32::GRAY),
                            );
                        } else {
                            // L3/L4/L5 cross-orbit (fleet not co-orbital): Hohmann
                            ui.label(
                                egui::RichText::new("⬆ Hohmann Transfer")
                                    .strong().size(12.0)
                                    .color(egui::Color32::from_rgb(160, 210, 255)),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(egui::Color32::from_rgb(200, 200, 200)),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(egui::Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new("Keplerian transfer arc,\nthen phase into the LP.")
                                    .size(10.0).color(egui::Color32::GRAY),
                            );
                        }
                    });
                });
                // Fleet stats infobox (same as body-target section)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("\u{1f680} Fleet")
                                .strong().size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );
                        ui.add_space(3.0);
                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_g = fleet.min_accel_ms2() / 9.80665;
                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0).strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0).strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(format!("{:.3} g", accel_g))
                                .size(11.0).strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                    });
                });
            });
        }
        if let Some(ref window) = window_this_frame {
            let syn_days = if window.synodic_period_s.is_finite() {
                window.synodic_period_s / 86_400.0
            } else {
                f64::INFINITY
            };
            let window_days = window.time_to_window_s / 86_400.0;

            ui.add_space(6.0);

            let max_days = window_max_slider_days.min(1_825.0); // cap at 5 years
            let step_size = if max_days <= 1.0 {
                0.01 // ~14 mins
            } else if max_days <= 10.0 {
                0.05 // ~1.2 hours
            } else if max_days <= 50.0 {
                0.1 // ~2.4 hours
            } else if max_days <= 200.0 {
                0.5 // 12 hours
            } else {
                1.0 // 1 day
            };

            // ── Transfer Window (left) + Planned Departure (right) side by side ──
            ui.horizontal_top(|ui| {
                // Left: Transfer Window box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("⏱ Transfer Window")
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );
                        ui.add_space(3.0);

                        egui::Grid::new("window_info_grid")
                            .num_columns(2)
                            .spacing([8.0, 3.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Next window:").size(12.0));
                                if window_days < 1.0 {
                                    ui.label(
                                        egui::RichText::new("NOW  ✓")
                                            .size(12.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(80, 220, 80)),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(format!("{}", format_duration(window.time_to_window_s)))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(200, 200, 200)),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Synodic period:").size(12.0));
                                let syn_str = if syn_days.is_finite() {
                                    format_duration(window.synodic_period_s)
                                } else {
                                    "∞ (same orbit)".to_owned()
                                };
                                ui.label(egui::RichText::new(syn_str).size(12.0).color(egui::Color32::GRAY));
                                ui.end_row();
                            });
                    });
                });

                // Right: Planned Departure box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        // Row 1: label
                        ui.label(
                            egui::RichText::new("🕐 Planned Departure")
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );

                        // Row 2: slider
                        let mut offset_days = fleet_ui_state.departure_offset_days as f32;
                        let slider = egui::Slider::new(&mut offset_days, 0.0_f32..=max_days as f32)
                            .step_by(step_size as f64)
                            .custom_formatter(|v, _| {
                                if v < 0.01 {
                                    "Now".to_owned()
                                } else {
                                    format_duration(v as f64 * 86_400.0)
                                }
                            });
                        if ui.add(slider).changed() {
                            fleet_ui_state.departure_offset_days = offset_days as f64;
                        }

                        // Row 3: alignment indicator (below the slider)
                        let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                        let phase_at = {
                            let raw = window.phase_error_now_rad + window.phase_rate_rad_s * dep_s;
                            ((raw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)) - std::f64::consts::PI
                        };
                        let factor = crate::fleets::orbital_mechanics::phase_dv_factor(phase_at.abs());
                        let (quality_str, quality_color) = if factor < 1.05 {
                            ("● Optimal", egui::Color32::from_rgb(80, 220, 80))
                        } else if factor < 1.40 {
                            ("◑ Good", egui::Color32::from_rgb(180, 220, 80))
                        } else if factor < 1.80 {
                            ("◔ Fair", egui::Color32::from_rgb(220, 180, 60))
                        } else {
                            ("○ Poor", egui::Color32::from_rgb(220, 80, 60))
                        };
                        ui.label(egui::RichText::new(quality_str).size(11.0).color(quality_color))
                            .on_hover_text("Indicates how well the planets are aligned for a transfer at the planned departure time. Poor alignment requires significantly more ΔV.");

                        // Next Window button on its own row
                        if window_days > 0.5 {
                            ui.add_space(2.0);
                            if ui.small_button(format!("🎯 Next Window (+{:.0} d)", window_days)).clicked() {
                                fleet_ui_state.departure_offset_days = window_days;
                            }
                        }
                    });
                });

                // Fleet stats infobox (narrow, right-most)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("🚀 Fleet")
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );
                        ui.add_space(3.0);

                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_ms2 = fleet.min_accel_ms2();
                        let accel_g = accel_ms2 / 9.80665;
                        let accel_str = format!("{:.3} g", accel_g);

                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(accel_str)
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                    });
                });
            });
        }

        if !fleet_ui_state.computed_options.is_empty() {
            ui.add_space(6.0);

            let fleet_max_dv = fleet.max_delta_v_ms();

            // Ensure selected_option is within bounds
            if fleet_ui_state.selected_option >= fleet_ui_state.computed_options.len() {
                fleet_ui_state.selected_option = fleet_ui_state.computed_options.len() - 1;
            }

            // Pre-compute execute button state
            let sel_option = fleet_ui_state.computed_options[fleet_ui_state.selected_option].clone();
            let abort_cost_t: f32 = if let Some(maneuver) = current_maneuver {
                let progress = maneuver.progress(elapsed) as f32;
                let abort_factor = 4.0 * progress * (1.0 - progress);
                maneuver.fuel_used_t * abort_factor * 0.6
            } else {
                0.0
            };
            let dv_after_abort = if abort_cost_t > 0.0 {
                fleet.min_delta_v_after_abort(abort_cost_t)
            } else {
                fleet_max_dv
            };
            let sel_affordable_with_abort = sel_option.total_delta_v_ms <= dv_after_abort;

            // Interstellar note
            let is_interstellar = star_system_snap.is_some();
            if is_interstellar {
                if let Some((_, ref sys_name, dist_ly)) = star_system_snap {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new(format!("\u{1F30C} Interstellar Mission: {}", sys_name))
                                .strong().size(13.0).color(egui::Color32::from_rgb(200, 180, 255)),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Distance: {:.2} ly = {:.0} AU",
                                dist_ly,
                                dist_ly as f64 * 63_241.077
                            )).size(11.0).color(egui::Color32::GRAY),
                        );
                        ui.label(
                            egui::RichText::new(
                                "\u{26A0} Interstellar navigation is point-and-burn. \
                                 Transfer windows do not apply. \
                                 Ensure adequate \u{394}V and life-support reserves."
                            ).size(11.0).italics().color(egui::Color32::from_rgb(220, 180, 80)),
                        );
                    });
                    ui.add_space(4.0);
                }
            }

            let btn_label = if is_interstellar {
                "\u{1F680} Commit Interstellar Course".to_string()
            } else if is_course_correction {
                if abort_cost_t > 0.01 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!("\u{1F504} Execute Course Correction (+{:.2} km/s abort burn)", abort_dv_kms)
                } else {
                    "\u{1F504} Execute Course Correction".to_string()
                }
            } else {
                "\u{1F680} Execute Transfer".to_string()
            };

            // For fleet intercepts note the encounter speed penalty
            if fleet_target_snap.is_some() && fleet_ui_state.intercept_speed_ms > 100.0 {
                let extra_dv_kms = fleet_ui_state.intercept_speed_ms / 1_000.0;
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26A0} +{:.1} km/s added for encounter speed (not included in \u{394}V below)",
                        extra_dv_kms
                    ))
                    .size(11.0)
                    .italics()
                    .color(egui::Color32::from_rgb(220, 160, 60)),
                );
            }

            // Execute Transfer button with ETA on the same row
            ui.horizontal(|ui| {
                let insufficient = sel_option.is_thrust_limited && is_interstellar && sel_option.total_delta_v_ms == 0.0;
                let btn = egui::Button::new(
                    egui::RichText::new(&btn_label).size(13.0).strong(),
                );
                let resp = ui.add_enabled(!insufficient && (sel_affordable_with_abort || is_interstellar), btn);
                if resp.clicked() {
                    if is_interstellar {
                        // Interstellar travel: no ECS destination body; log mission intent.
                        // Full multi-system navigation will be implemented in a future session.
                        if let Some((sys_id, ref sys_name, dist_ly)) = star_system_snap {
                            info!(
                                "Fleet '{}' committed to interstellar course: {} ({:.2} ly, system_id {}). \
                                 \u{394}V required: {:.1} km/s, travel time: {:.1} years. \
                                 Multi-system navigation NYI.",
                                fleet.name, sys_name, dist_ly, sys_id,
                                sel_option.total_delta_v_ms / 1_000.0,
                                sel_option.transfer_time_s / (365.25 * 86_400.0),
                            );
                        }
                    } else {
                        let maybe_transfer = if let Some(ref lp) = lp_target_snap {
                            build_planned_transfer_lp(fleet_entity, fleet, orbit, lp, body_query, &sel_option)
                        } else if let Some(tfe) = fleet_target_snap {
                            all_fleets_query.get(tfe).ok()
                                .and_then(|(_, _, _, maybe_fo, _)| maybe_fo)
                                .and_then(|fo| {
                                    build_planned_transfer(fleet_entity, fleet, orbit, fo.body, body_query, &sel_option)
                                })
                        } else if let Some(te) = body_target_snap {
                            build_planned_transfer(fleet_entity, fleet, orbit, te, body_query, &sel_option)
                        } else {
                            None
                        };
                        if let Some(transfer) = maybe_transfer {
                            pending_actions.start_transfers.push(StartTransferAction {
                                fleet: fleet_entity,
                                transfer,
                                abort_cost_t,
                                departure_offset_s: fleet_ui_state.departure_offset_days * 86_400.0,
                            });
                        }
                    }
                }
                if !is_interstellar {
                    let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                    let total_eta_s = dep_s + sel_option.transfer_time_s;
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("ETA  {}", format_duration(total_eta_s)))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(160, 220, 160)),
                    );
                }
            });
            if !is_interstellar {
                let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                let total_eta_s = dep_s + sel_option.transfer_time_s;
                let arrival_ts = current_timestamp + total_eta_s as i64;
                ui.label(
                    egui::RichText::new(format!(
                        "Arrives  {}",
                        format_timestamp_date_time(arrival_ts)
                    ))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(130, 190, 220)),
                );
            }
            if !is_interstellar && !sel_affordable_with_abort {
                ui.label(
                    egui::RichText::new(
                        if abort_cost_t > 0.0 {
                            "Insufficient \u{394}V remaining after abort burn."
                        } else {
                            "Selected option requires more \u{394}V than this fleet can provide."
                        },
                    )
                    .size(11.0)
                    .italics()
                    .color(egui::Color32::from_rgb(200, 100, 60)),
                );
            }
        }

        // ── Gravity Assists panel ─────────────────────────────────────────────
        // Shown whenever there are heliocentric flyby candidates for this route.
        if !fleet_ui_state.gravity_assist_candidates.is_empty() {
            ui.add_space(6.0);
            let num_ga = fleet_ui_state.gravity_assist_candidates.len();
            let header_text = format!("⚡ Gravity Assists ({num_ga} available)");
            egui::CollapsingHeader::new(
                egui::RichText::new(header_text)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(120, 220, 255)),
            )
            .default_open(true)
            .show(ui, |ui| {
                // Snapshot data before mut-borrowing fleet_ui_state below
                let snapped: Vec<(usize, String, f64, f64, f64, f64)> =
                    fleet_ui_state.gravity_assist_candidates
                        .iter()
                        .enumerate()
                        .map(|(i, e)| (
                            i,
                            e.option.body_name.clone(),
                            e.option.dv_savings_ms,
                            e.option.extra_time_s,
                            e.option.window_period_s,
                            e.option.v_inf_ms,
                        ))
                        .collect();

                for (idx, body_name, savings, extra_t, win_period, v_inf) in snapped {
                    let is_sel = fleet_ui_state.selected_gravity_assist == Some(idx);
                    let beneficial = savings > 100.0;
                    let header_color = if is_sel {
                        egui::Color32::from_rgb(80, 255, 180)
                    } else if beneficial {
                        egui::Color32::from_rgb(160, 255, 100)
                    } else {
                        egui::Color32::GRAY
                    };

                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(format!("⚡ via {body_name}"))
                                .size(12.0)
                                .strong()
                                .color(header_color),
                        );
                        egui::Grid::new(format!("ga_grid_{idx}"))
                            .num_columns(2)
                            .spacing([8.0, 2.0])
                            .show(ui, |ui| {
                                if beneficial {
                                    ui.label(egui::RichText::new("ΔV saved:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(format_delta_v(savings))
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(80, 220, 80)),
                                    );
                                } else {
                                    ui.label(egui::RichText::new("Extra ΔV:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(format_delta_v(-savings))
                                            .size(11.0)
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Extra time:").size(11.0));
                                let sign = if extra_t >= 0.0 { "+" } else { "" };
                                ui.label(
                                    egui::RichText::new(
                                        format!("{sign}{}", format_duration(extra_t.abs()))
                                    )
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.end_row();

                                ui.label(egui::RichText::new("Window every:").size(11.0));
                                let win_str = if win_period.is_finite() {
                                    format_duration(win_period)
                                } else {
                                    "∞".to_owned()
                                };
                                ui.label(
                                    egui::RichText::new(win_str)
                                        .size(11.0)
                                        .color(egui::Color32::GRAY),
                                );
                                ui.end_row();

                                ui.label(egui::RichText::new("v∞:").size(11.0));
                                ui.label(
                                    egui::RichText::new(format_delta_v(v_inf))
                                        .size(11.0)
                                        .color(egui::Color32::GRAY),
                                );
                                ui.end_row();
                            });

                        ui.horizontal(|ui| {
                            if is_sel {
                                if ui.small_button("✕ Clear Assist").clicked() {
                                    fleet_ui_state.selected_gravity_assist = None;
                                    // Shift selection back to direct Efficient option
                                    fleet_ui_state.selected_option = 0;
                                    fleet_ui_state.planned_transfer = None;
                                }
                            } else {
                                let label = if beneficial { "⚡ Use Gravity Assist" } else { "Use Suboptimal Assist" };
                                if ui.small_button(label).clicked() {
                                    fleet_ui_state.selected_gravity_assist = Some(idx);
                                    fleet_ui_state.selected_option = 0; // GA is option 0
                                    fleet_ui_state.planned_transfer = None;
                                }
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
            });
        }

        if !fleet_ui_state.computed_options.is_empty() {
            let fleet_wet_mass = fleet.total_wet_mass_t();
            let fleet_max_dv = fleet.max_delta_v_ms();

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Transfer Options:").strong().size(13.0));
            ui.add_space(2.0);

            let options: Vec<_> = fleet_ui_state.computed_options.clone();
            for (idx, option) in options.iter().enumerate() {
                let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);
                let fuel_pct = if fleet_wet_mass > 0.0 {
                    (fuel_cost / fleet_wet_mass * 100.0) as u32
                } else {
                    0
                };
                let affordable = option.total_delta_v_ms <= fleet_max_dv;

                let is_selected = fleet_ui_state.selected_option == idx;
                let row_color = if !affordable {
                    egui::Color32::from_rgb(180, 80, 80)
                } else if is_selected {
                    egui::Color32::from_rgb(100, 180, 255)
                } else {
                    egui::Color32::from_rgb(200, 200, 200)
                };

                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    let resp = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(format!(
                            "{} {}",
                            if is_selected { "●" } else { "○" },
                            option.label
                        ))
                        .size(13.0)
                        .strong()
                        .color(row_color),
                    );
                    if resp.clicked() {
                        fleet_ui_state.selected_option = idx;
                        fleet_ui_state.planned_transfer = None;
                    }

                    egui::Grid::new(format!("option_{idx}"))
                        .num_columns(4)
                        .spacing([16.0, 2.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Total ΔV:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.total_delta_v_ms))
                                    .size(12.0)
                                    .strong()
                                    .color(row_color),
                            );
                            ui.label(egui::RichText::new("Travel time:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_duration(option.transfer_time_s))
                                    .size(12.0)
                                    .strong(),
                            );
                            ui.end_row();

                            ui.label(egui::RichText::new("Est. fuel:").size(12.0));
                            let fuel_color = if affordable {
                                egui::Color32::from_rgb(220, 180, 60)
                            } else {
                                egui::Color32::from_rgb(220, 80, 60)
                            };
                            ui.label(
                                egui::RichText::new(format!("{:.0} t ({fuel_pct}%)", fuel_cost))
                                    .size(12.0)
                                    .color(fuel_color),
                            );
                            ui.label(egui::RichText::new("Departure burn:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.delta_v1_ms))
                                    .size(12.0),
                            );
                            ui.end_row();

                            // Plane-change ΔV row (only shown when non-trivial)
                            if option.plane_change_dv_ms > 100.0 {
                                ui.label(egui::RichText::new("Plane change:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_delta_v(option.plane_change_dv_ms))
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(180, 200, 255)),
                                );
                                ui.label(egui::RichText::new("").size(12.0));
                                ui.label(egui::RichText::new("").size(12.0));
                                ui.end_row();
                            }

                            // Burn time row — shows how long the fleet's engines fire.
                            if option.burn_time_s > 0.0 {
                                // Classify burn profile based on burn/transfer time ratio.
                                let (profile_label, profile_color) =
                                    if option.is_thrust_limited {
                                        // Burn time >= Hohmann time: impulsive assumption invalid.
                                        ("⚠ Thrust-limited", egui::Color32::from_rgb(220, 100, 40))
                                    } else if option.label == "Full Thrust" {
                                        // Entire trip is a burn
                                        ("⚡ Full thrust", egui::Color32::from_rgb(255, 180, 60))
                                    } else {
                                        let ratio = option.burn_time_s / option.transfer_time_s.max(1.0);
                                        if option.burn_time_s < 3_600.0 {
                                            ("Impulsive", egui::Color32::from_rgb(120, 200, 120))
                                        } else if ratio < 0.05 {
                                            ("Short burn", egui::Color32::from_rgb(140, 210, 140))
                                        } else if ratio < 0.25 {
                                            ("Extended burn", egui::Color32::from_rgb(220, 200, 80))
                                        } else {
                                            ("Continuous thrust", egui::Color32::from_rgb(220, 140, 60))
                                        }
                                    };
                                ui.label(egui::RichText::new("Burn time:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_duration(option.burn_time_s))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.label(egui::RichText::new("Profile:").size(12.0));
                                ui.label(
                                    egui::RichText::new(profile_label)
                                        .size(12.0)
                                        .color(profile_color),
                                );
                                ui.end_row();

                                let accel_ms2 = fleet.min_accel_ms2();
                                let accel_g = accel_ms2 / 9.80665;
                                ui.label(egui::RichText::new("Acceleration:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format!("{:.2} g", accel_g))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.end_row();

                                // Extra warning row for thrust-limited options.
                                if option.is_thrust_limited {
                                    ui.label(
                                        egui::RichText::new("  Low-thrust spiral — travel time ≥ burn time")
                                            .size(11.0)
                                            .italics()
                                            .color(egui::Color32::from_rgb(180, 130, 80)),
                                    );
                                    ui.end_row();
                                }
                            }

                            if !affordable {
                                ui.label(
                                    egui::RichText::new("⚠ Insufficient ΔV capacity")
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(220, 80, 60)),
                                );
                            }
                        });
                });
                ui.add_space(2.0);
            }

        }
    }
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
        Some(FleetOrbit::new(maneuver.destination_body, maneuver.arrival_orbit_radius_au))
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
                    );
                });
        });

    if !open {
        fleet_ui_state.show_transfer_popup = false;
    }
}

/// Build a `PlannedTransfer` from the selected transfer option and fleet/body state.
fn build_planned_transfer(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    target_entity: Entity,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::{AU_IN_METERS, G_CONST, GM_SUN};

    let (_, origin_body, origin_sc, origin_ko, origin_lp) = body_query.get(orbit.body).ok()?;
    let (_, dest_body, _dest_sc, dest_ko, dest_lp) = body_query.get(target_entity).ok()?;

    let dest_parent = dest_lp.map(|lp| lp.0);
    let origin_parent = origin_lp.map(|lp| lp.0);
    let dest_is_star = dest_body.body_type == BodyType::Star;
    let dest_is_ring = dest_body.body_type == BodyType::Ring;

    // Determine: (origin_sma, dest_sma, gm, orbit_center, actual destination body for FleetOrbit)
    // For Rings: redirect the FleetOrbit destination to the ring's parent planet.
    // For Stars: Fleet will orbit the star at the planet SOI boundary; orbit_center = star entity.
    let (origin_sma_au, dest_sma_au, gm, orbit_center, actual_dest_body) = if dest_is_star {
        // Heliocentric escape: orbit body = current body's parent star
        let parent_mass = origin_body.mass;
        let planet_sma_au = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0);
        let soi_au = planet_sma_au * (parent_mass / 1.989e30_f64).powf(0.4);
        (orbit.radius_au, soi_au.max(orbit.radius_au * 50.0), G_CONST * parent_mass, target_entity, target_entity)
    } else if dest_is_ring {
        // Ring: resolve to orbiting the ring's parent planet at ring.radius altitude
        let ring_parent = dest_parent.unwrap_or(orbit.body);
        let parent_mass = body_query.get(ring_parent).ok()
            .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
        let ring_radius_au = (dest_body.radius as f64 * 1_000.0) / AU_IN_METERS;
        let r1 = if ring_parent == orbit.body {
            orbit.radius_au
        } else {
            origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.01)
        };
        (r1, ring_radius_au, G_CONST * parent_mass, ring_parent, ring_parent)
    } else if dest_parent == Some(orbit.body) {
        // Local (e.g., Earth → Moon)
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        (orbit.radius_au, r2, G_CONST * origin_body.mass, orbit.body, target_entity)
    } else if dest_parent.is_some() && dest_parent == origin_parent {
        // Both orbit the same central body (moon-to-moon OR interplanetary, e.g. Earth→Mars).
        // NOTE: The Sun lacks SpaceCoordinates so body_query.get(Sun) fails — fall back to GM_SUN.
        let shared = dest_parent.unwrap();
        let gm = body_query.get(shared).ok()
            .map(|(_, b, _, _, _)| if b.body_type == BodyType::Star { GM_SUN } else { G_CONST * b.mass })
            .unwrap_or(GM_SUN);
        let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        (r1, r2, gm, shared, target_entity)
    } else if Some(target_entity) == origin_parent {
        // Downward transfer: fleet is at a moon, destination is the parent planet.
        // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
        let parent_mass = dest_body.mass;
        let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        let r2 = (dest_body.radius as f64 * 3_000.0) / AU_IN_METERS;
        (r1, r2.min(r1 * 0.5), G_CONST * parent_mass, target_entity, target_entity)
    } else {
        // Heliocentric: if fleet is at a moon, its own SMA is Earth-relative — use parent's SMA.
        let r1 = if origin_ko.map(|ko| ko.semi_major_axis < 0.05).unwrap_or(true) {
            origin_parent
                .and_then(|pe| body_query.get(pe).ok())
                .and_then(|(_, _, _, ko, _)| ko)
                .map(|ko| ko.semi_major_axis)
                .or_else(|| origin_ko.map(|ko| ko.semi_major_axis))
                .unwrap_or(1.0)
        } else {
            origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0)
        };
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.5);
        let star = body_query.iter()
            .find(|(_, b, _, ko, _)| ko.is_none() && b.body_type == BodyType::Star)
            .map(|(e, _, _, _, _)| e)
            .unwrap_or(orbit.body);
        (r1, r2, GM_SUN, star, target_entity)
    };

    let outward = dest_sma_au >= origin_sma_au;
    let center_pos = body_query.get(orbit_center).map(|(_, _, sc, _, _)| sc.position).unwrap_or(bevy::math::DVec3::ZERO);
    let rel_pos = origin_sc.position - center_pos;

    // Derive the transfer-orbit plane from the 3D departure and arrival position
    // vectors relative to the central body (r1 × r2 gives the plane normal).
    // This keeps inclination, LAN, and argument_of_periapsis mutually consistent
    // so the propagated green-dot position and the displayed preview arc match.
    let dest_sc_pos = body_query.get(target_entity).ok()
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(bevy::math::DVec3::ZERO);
    let dest_rel = dest_sc_pos - center_pos;

    let plane_normal = rel_pos.cross(dest_rel);
    let plane_normal_len = plane_normal.length();

    let (transfer_inclination, transfer_lan, argument_of_periapsis) = if plane_normal_len > 1e-20 {
        let n = plane_normal / plane_normal_len;
        // i = angle between plane normal and ecliptic north (Ẑ).
        let incl = n.z.clamp(-1.0, 1.0).acos();
        // Ascending node: N = Ẑ × n = (-ny, nx, 0).
        let node_xy = bevy::math::DVec3::new(-n.y, n.x, 0.0);
        let node_len = node_xy.length();
        let lan = if node_len > 1e-20 {
            let node = node_xy / node_len;
            node.y.atan2(node.x)
        } else {
            0.0
        };
        // ω: angle from ascending node to periapsis (departure point for outward,
        // arrival for inward), measured in the orbital plane.
        let aop = if node_len > 1e-20 {
            let node = node_xy / node_len;
            let peri_dir = rel_pos.normalize_or_zero();
            let cos_w = node.dot(peri_dir);
            let sin_w = n.dot(node.cross(peri_dir));
            let omega = sin_w.atan2(cos_w);
            if outward { omega } else { omega + std::f64::consts::PI }
        } else {
            let departure_angle = rel_pos.y.atan2(rel_pos.x);
            if outward { departure_angle } else { departure_angle - std::f64::consts::PI }
        };
        (incl, lan, aop)
    } else {
        // Degenerate (origin and destination collinear with center): ecliptic-flat.
        let departure_angle = rel_pos.y.atan2(rel_pos.x);
        let aop = if outward { departure_angle } else { departure_angle - std::f64::consts::PI };
        (0.0, 0.0, aop)
    };

    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
    let sma_m = option.sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: option.sma_au,
        eccentricity: option.eccentricity,
        inclination: transfer_inclination,
        longitude_ascending_node: transfer_lan,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    // Arrival orbit radius: for rings use the ring radius, otherwise reuse fleet parking radius
    let arrival_orbit_radius_au = if dest_is_ring {
        dest_sma_au
    } else if dest_is_star {
        dest_sma_au // park at SOI boundary initially
    } else {
        orbit.radius_au
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body: actual_dest_body,
        orbit_center,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label: option.label,
        start_position_au: None,
        end_position_au: None,
    })
}

/// Build a `PlannedTransfer` targeting a Lagrange point (no dedicated ECS entity).
fn build_planned_transfer_lp(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    lp: &LagrangeTarget,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::AU_IN_METERS;

    // LP transfers are heliocentric – find the star as orbit center
    let star_entity = body_query.iter()
        .find(|(_, b, _, ko, _)| ko.is_none() && b.body_type == BodyType::Star)
        .map(|(e, _, _, _, _)| e)
        .unwrap_or(orbit.body);

    // Determine departure position.  For fleets orbiting the star directly
    // (e.g. after a previous LP transfer), `orbit.body` is the star whose
    // SpaceCoordinates are at the heliocentric origin → rel_pos would be
    // (0,0,0) and departure_angle 0.  In that case use the L-point's parent
    // planet position instead so the orbit geometry is meaningful.
    let center_pos = body_query.get(star_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(bevy::math::DVec3::ZERO);

    let origin_pos = {
        let (_, body_data, origin_sc, _, _) = body_query.get(orbit.body).ok()?;
        if body_data.body_type == BodyType::Star {
            // Fleet is parked around the star — use the planet's current position
            // as the departure reference instead.
            body_query.get(lp.planet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        }
    };

    let rel_pos = origin_pos - center_pos;
    let departure_angle = rel_pos.y.atan2(rel_pos.x);

    // ALL LP transfers are kinematic (direct Bezier arc from origin to LP position).
    // This prevents co-orbital phasing options from rendering as multi-lap Keplerian
    // rings around the Sun (which previously looked like "multiple orbit rings").
    let option_label: &'static str = match option.label {
        "Efficient" => "Direct Efficient",
        "Moderate"  => "Direct Moderate",
        "Fast"      => "Direct Fast",
        other       => other, // kinematic labels (Full Thrust, Coast, Max Speed, Direct *) pass through
    };

    // Pre-compute the heliocentric LP position for kinematic arc rendering.
    // Every LP transfer sets start/end positions so the fleet flies to the correct
    // Lagrange-point location rather than the star origin (0,0,0).
    let planet_pos = body_query.get(lp.planet_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(origin_pos);
    let planet_rel = planet_pos - center_pos;
    let planet_angle = planet_rel.y.atan2(planet_rel.x);
    let lp_angle = match lp.point {
        3 => planet_angle + std::f64::consts::PI,
        4 => planet_angle + std::f64::consts::FRAC_PI_3,
        5 => planet_angle - std::f64::consts::FRAC_PI_3,
        _ => planet_angle, // L1/L2: on the Sun-planet radial
    };
    let lp_pos_au = center_pos + bevy::math::DVec3::new(
        lp.radius_au * lp_angle.cos(),
        lp.radius_au * lp_angle.sin(),
        0.0,
    );
    let start_pos = Some(origin_pos);
    let end_pos   = Some(lp_pos_au);

    // L1/L2: the LP is physically near the planet (±r_hill from the planet's
    // heliocentric position).  Park the fleet around the planet at r_hill so it
    // co-orbits with the planet rather than orbiting the Sun at 1 AU.
    //
    // L3/L4/L5: heliocentric co-orbital positions.  Park the fleet around the
    // star at the planet's SMA; `complete_fleet_maneuvers` will set direction=0.0
    // (frozen) because `is_kinematic()` + star destination → LP-stationed sentinel.
    let (destination_body, arrival_orbit_radius_au) = if matches!(lp.point, 1 | 2) {
        let r_hill = (lp.radius_au - lp.planet_sma_au).abs().max(0.001);
        (lp.planet_entity, r_hill)
    } else {
        (star_entity, lp.planet_sma_au)
    };

    let gm = lp.gm;
    let sma_m = option.sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let outward = lp.radius_au >= lp.planet_sma_au;
    let argument_of_periapsis = if outward {
        departure_angle
    } else {
        departure_angle - std::f64::consts::PI
    };
    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: option.sma_au,
        eccentricity: option.eccentricity,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body,
        orbit_center: star_entity,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label,
        start_position_au: start_pos,
        end_position_au: end_pos,
    })
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
