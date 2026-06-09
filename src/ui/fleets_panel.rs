use super::transfer_planner::render_transfer_planner;
use super::*;
use crate::astronomy::components::SpaceCoordinates;
use crate::economy::hohmann_round_trip_seconds;
use crate::economy::LocalStockpile;
use crate::economy::PendingResourceRequests;
use crate::economy::RequestPriority;
use crate::economy::RequestState;
use crate::economy::ResourceRequest;
use crate::fleets::components::ShipInfo;

/// Filter applied to the fleet list when navigating from the Private
/// Shipping overview panel (GRA-37.e).  `None` shows every fleet
/// (default); `Some(idx)` restricts the list to fleets currently bound
/// to `ShippingCompanies.companies[idx]` via an `Assigned` / `InTransit`
/// `ResourceRequest` (i.e. the freighter fleets the company is using).
///
/// The filter is set by clicking a row in the overview panel and cleared
/// by the player via the "× Clear filter" chip in the fleets header.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ShippingCompanyFilter(pub Option<usize>);

const SHIP_MANIFEST_ACTIONS_WIDTH: f32 = 122.0;
const SHIP_MANIFEST_ROW_HEIGHT: f32 = 24.0;
const SHIP_MANIFEST_INNER_PADDING_X: f32 = 8.0;
const SHIP_MANIFEST_COLUMN_SPACING: f32 = 10.0;
const SHIP_MANIFEST_MAX_DRAG_WIDTH: f32 = 900.0;
const SHIP_MANIFEST_COLUMN_WEIGHTS: [f32; 7] = [1.9, 1.3, 0.95, 0.8, 1.5, 1.0, 1.15];

fn draw_menu_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(theme::title())
            .color(theme::ACCENT),
    );
    ui.label(
        egui::RichText::new(subtitle)
            .font(theme::body(11.5))
            .color(theme::TEXT_DIM),
    );
    theme::divider(ui);
}

fn draw_status_chip(ui: &mut egui::Ui, label: &str, value: String, color: egui::Color32) {
    ui.vertical(|ui| {
        ui.set_min_width(132.0);
        ui.label(
            egui::RichText::new(label)
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(value)
                .font(theme::heading())
                .color(color),
        );
    });
}

fn ship_manifest_drag_width(available_width: f32) -> f32 {
    (available_width - SHIP_MANIFEST_ACTIONS_WIDTH - 2.0).clamp(220.0, SHIP_MANIFEST_MAX_DRAG_WIDTH)
}

fn ship_manifest_text_column(
    painter: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    align: egui::Align2,
    color: egui::Color32,
    _strong: bool,
) {
    let clipped = painter.with_clip_rect(rect);
    let anchor = match align {
        egui::Align2::LEFT_CENTER => rect.left_center(),
        egui::Align2::CENTER_CENTER => rect.center(),
        egui::Align2::RIGHT_CENTER => rect.right_center(),
        _ => rect.left_center(),
    };
    clipped.text(anchor, align, text, egui::FontId::proportional(12.0), color);
}

fn ship_manifest_column_rects(row_rect: egui::Rect) -> [egui::Rect; 7] {
    let content_rect = row_rect.shrink2(egui::vec2(SHIP_MANIFEST_INNER_PADDING_X, 0.0));
    let total_spacing =
        SHIP_MANIFEST_COLUMN_SPACING * (SHIP_MANIFEST_COLUMN_WEIGHTS.len() - 1) as f32;
    let total_weight: f32 = SHIP_MANIFEST_COLUMN_WEIGHTS.iter().sum();
    let usable_width = (content_rect.width() - total_spacing).max(70.0);
    let mut left = content_rect.left();

    std::array::from_fn(|idx| {
        let width = usable_width * SHIP_MANIFEST_COLUMN_WEIGHTS[idx] / total_weight;
        let rect = egui::Rect::from_min_size(
            egui::pos2(left, content_rect.top()),
            egui::vec2(width, content_rect.height()),
        );
        left += width + SHIP_MANIFEST_COLUMN_SPACING;
        rect
    })
}

fn paint_ship_manifest_header(ui: &mut egui::Ui, drag_width: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(drag_width, SHIP_MANIFEST_ROW_HEIGHT),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        let columns = ship_manifest_column_rects(rect);
        let headers = [
            ("Name", egui::Align2::LEFT_CENTER),
            ("Class", egui::Align2::LEFT_CENTER),
            ("Dry (t)", egui::Align2::RIGHT_CENTER),
            ("Fuel", egui::Align2::CENTER_CENTER),
            ("Drive", egui::Align2::LEFT_CENTER),
            ("Thrust", egui::Align2::RIGHT_CENTER),
            ("Max ΔV", egui::Align2::RIGHT_CENTER),
        ];
        for (idx, (label, align)) in headers.into_iter().enumerate() {
            ship_manifest_text_column(painter, columns[idx], label, align, theme::TEXT_VALUE, true);
        }

        ui.allocate_ui_with_layout(
            egui::vec2(SHIP_MANIFEST_ACTIONS_WIDTH, SHIP_MANIFEST_ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new("Actions").strong().size(12.0));
            },
        );
    });
}

fn render_ship_manifest_row(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    ship_idx: usize,
    ship: &ShipInfo,
    in_orbit_for_manifest: bool,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
) {
    let fuel_pct = (ship.fuel_fraction() * 100.0) as u32;
    let fuel_color = if fuel_pct > 50 {
        theme::GREEN
    } else if fuel_pct > 20 {
        theme::AMBER
    } else {
        theme::RED
    };
    let class_text = format!("{} {}", ship.class.icon(), ship.class.display_name());
    let dry_text = format!("{:.0}", ship.dry_mass_t);
    let fuel_text = format!("{fuel_pct}%");
    let thrust_text = format!("{:.0} kN", ship.thrust_kn);
    let delta_v_text = format_delta_v(ship.delta_v_ms());

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let drag_width = ship_manifest_drag_width(ui.available_width());
        let drag_id = egui::Id::new("drag_ship").with(fleet_entity).with(ship_idx);
        ui.dnd_drag_source(drag_id, (fleet_entity, ship_idx), |ui| {
            let (row_rect, response) = ui.allocate_exact_size(
                egui::vec2(drag_width, SHIP_MANIFEST_ROW_HEIGHT),
                egui::Sense::click_and_drag(),
            );

            let row_rounding = egui::CornerRadius::same(4);
            let is_hovered = ui.rect_contains_pointer(row_rect);
            let show_frame = is_hovered || response.dragged();
            let frame_rect = row_rect.expand2(egui::vec2(4.0, 0.0));
            let fill = if response.dragged() {
                egui::Color32::from_rgba_premultiplied(0, 140, 160, 42)
            } else {
                theme::SURFACE_RAISED
            };
            let stroke_color = if response.dragged() {
                theme::ACCENT
            } else {
                theme::ACCENT_DIM
            };

            let painter = ui.painter();
            if show_frame {
                painter.rect_filled(frame_rect, row_rounding, fill);
                painter.rect_stroke(
                    frame_rect,
                    row_rounding,
                    egui::Stroke::new(if response.dragged() { 1.2 } else { 1.0 }, stroke_color),
                    egui::StrokeKind::Inside,
                );
            }

            let columns = ship_manifest_column_rects(row_rect);
            ship_manifest_text_column(
                painter,
                columns[0],
                &ship.name,
                egui::Align2::LEFT_CENTER,
                theme::TEXT_VALUE,
                false,
            );
            ship_manifest_text_column(
                painter,
                columns[1],
                &class_text,
                egui::Align2::LEFT_CENTER,
                theme::TEXT,
                false,
            );
            ship_manifest_text_column(
                painter,
                columns[2],
                &dry_text,
                egui::Align2::RIGHT_CENTER,
                theme::TEXT,
                false,
            );
            ship_manifest_text_column(
                painter,
                columns[3],
                &fuel_text,
                egui::Align2::CENTER_CENTER,
                fuel_color,
                false,
            );
            ship_manifest_text_column(
                painter,
                columns[4],
                ship.propulsion.display_name(),
                egui::Align2::LEFT_CENTER,
                theme::TEXT,
                false,
            );
            ship_manifest_text_column(
                painter,
                columns[5],
                &thrust_text,
                egui::Align2::RIGHT_CENTER,
                theme::TEXT,
                false,
            );
            ship_manifest_text_column(
                painter,
                columns[6],
                &delta_v_text,
                egui::Align2::RIGHT_CENTER,
                theme::ACCENT,
                false,
            );

            if is_hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }

            response
        });

        let refuel_resp = ui.add_enabled(
            in_orbit_for_manifest,
            egui::Button::new(egui::RichText::new("⛽ Refuel").size(11.0))
                .min_size(egui::vec2(58.0, 18.0)),
        );
        if refuel_resp
            .on_hover_text(if in_orbit_for_manifest {
                "Refuel this ship to full capacity (free — debug)"
            } else {
                "Cannot refuel while in transit"
            })
            .clicked()
        {
            pending_actions.refuel_ships.push((fleet_entity, ship_idx));
        }

        if ui
            .add(
                egui::Button::new(egui::RichText::new("🗑 Scrap").size(11.0).color(theme::RED))
                    .min_size(egui::vec2(54.0, 18.0)),
            )
            .on_hover_text("Permanently scrap this ship")
            .clicked()
        {
            fleet_ui_state.scrap_confirm_ship = Some((fleet_entity, ship_idx));
        }
    });
}

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
    body_query: Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    colony_query: Query<(Entity, &Colony)>,
    mut pending_actions: ResMut<PendingFleetActions>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    mut settings: ResMut<Settings>,
    sim_time: Res<SimulationTime>,
    pending_resource_requests: Res<PendingResourceRequests>,
    stockpiles: Query<(Entity, &LocalStockpile)>,
    coords_query: Query<&SpaceCoordinates, Without<Fleet>>,
    shipping_companies: Res<crate::economy::ShippingCompanies>,
    mut shipping_company_filter: ResMut<ShippingCompanyFilter>,
) {
    if active_menu.current != GameMenu::Fleets {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let elapsed = sim_time.elapsed_seconds();

    // Build the set of fleet entities that match the company filter
    // (GRA-37.e click-through from the Private Shipping overview).  A
    // fleet matches if any non-terminal `ResourceRequest` links it to the
    // filtered company.  When the filter is `None`, the set is empty and
    // every fleet passes through.
    let company_filter_set: std::collections::HashSet<Entity> = match shipping_company_filter.0 {
        Some(company_idx) => pending_resource_requests
            .requests
            .iter()
            .filter(|r| {
                r.assigned_company_idx == Some(company_idx)
                    && matches!(r.state, RequestState::Assigned | RequestState::InTransit)
            })
            .filter_map(|r| r.assignee_fleet_id)
            .collect(),
        None => std::collections::HashSet::new(),
    };

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
            draw_menu_header(
                ui,
                "FLEETS",
                "Force composition, orbital posture, and transfer planning.",
            );

            // ── Company-filter chip (GRA-37.e) ──────────────────────────────
            if let Some(company_idx) = shipping_company_filter.0 {
                let company_name = shipping_companies
                    .companies
                    .get(company_idx)
                    .map(|c| c.name.as_str())
                    .unwrap_or("(unknown)");
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "🔍 Filtered to company: {}  ({} fleets)",
                            company_name,
                            company_filter_set.len()
                        ))
                        .color(theme::ACCENT)
                        .size(12.0),
                    );
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("× Clear").size(11.0).color(theme::RED),
                            )
                            .min_size(egui::vec2(64.0, 20.0)),
                        )
                        .clicked()
                    {
                        shipping_company_filter.0 = None;
                    }
                });
                theme::divider(ui);
            }

            // ── Top summary bar ──────────────────────────────────────────────────
            let fleet_count = fleet_query.iter().count();
            let in_transit = fleet_query
                .iter()
                .filter(|(_, _, _, m, _)| m.is_some())
                .count();
            ui.horizontal_wrapped(|ui| {
                draw_status_chip(
                    ui,
                    "TOTAL FLEETS",
                    fleet_count.to_string(),
                    theme::TEXT_VALUE,
                );
                ui.separator();
                draw_status_chip(ui, "IN TRANSIT", in_transit.to_string(), theme::RP_BLUE);
                ui.separator();
                ui.checkbox(
                    &mut settings.show_freighters_in_transit,
                    "Show freighters in transit",
                )
                .on_hover_text(
                    "When off, freighter fleets are hidden from this list and the system-map \
                     transit arcs.  Useful when auto-freight (GRA-37.a) generates a lot of \
                     moving traffic.",
                );
            });

            theme::divider(ui);

            // ── Main two-column layout ───────────────────────────────────────────
            let available = ui.available_size();
            let left_width = (available.x * 0.40).clamp(340.0, 560.0);

            ui.horizontal_top(|ui| {
                // ── Left column: fleet list ──────────────────────────────────────
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(left_width, available.y - 80.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        theme::elevated_frame().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("FLEET LIST")
                                    .font(theme::heading())
                                    .color(theme::ACCENT),
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
                                        &settings,
                                        &company_filter_set,
                                    );
                                });

                            ui.separator();
                            // ── Create Fleet section ─────────────────────────────
                            {
                                // Build sorted colony list grouped by star system
                                let mut colony_entries: Vec<(Entity, String, String)> =
                                    colony_query
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
                                let selection_valid = fleet_ui_state
                                    .spawn_location_body
                                    .map(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e))
                                    .unwrap_or(false);
                                if !selection_valid {
                                    let fallback = fleet_ui_state
                                        .selected_fleet
                                        .and_then(|sel| {
                                            fleet_query
                                                .get(sel)
                                                .ok()
                                                .and_then(|(_, _, mo, _, _)| mo.map(|o| o.body))
                                        })
                                        .and_then(|e| {
                                            colony_entries
                                                .iter()
                                                .any(|(ce, _, _)| *ce == e)
                                                .then_some(e)
                                        })
                                        .or_else(|| {
                                            body_query
                                                .iter()
                                                .find(|(_, b, _, _, _)| b.name == "Earth")
                                                .map(|(e, _, _, _, _)| e)
                                                .and_then(|e| {
                                                    colony_entries
                                                        .iter()
                                                        .any(|(ce, _, _)| *ce == e)
                                                        .then_some(e)
                                                })
                                        })
                                        .or_else(|| colony_entries.first().map(|(e, _, _)| *e));
                                    fleet_ui_state.spawn_location_body = fallback;
                                }

                                let selected_label = fleet_ui_state
                                    .spawn_location_body
                                    .and_then(|e| colony_entries.iter().find(|(ce, _, _)| *ce == e))
                                    .map(|(_, name, star)| format!("{name} ({star})"))
                                    .unwrap_or_else(|| "— No colony —".to_string());

                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Location:")
                                            .size(12.0)
                                            .color(theme::TEXT_DIM),
                                    );
                                    if colony_entries.is_empty() {
                                        ui.label(
                                            egui::RichText::new("No colonies yet")
                                                .size(12.0)
                                                .italics()
                                                .color(theme::TEXT_HINT),
                                        );
                                    } else {
                                        egui::ComboBox::from_id_salt("create_fleet_location")
                                            .selected_text(
                                                egui::RichText::new(&selected_label).size(12.0),
                                            )
                                            .width(210.0)
                                            .show_ui(ui, |ui| {
                                                let mut current_star = String::new();
                                                for (e, body_name, star_name) in &colony_entries {
                                                    if *star_name != current_star {
                                                        current_star = star_name.clone();
                                                        ui.add_space(2.0);
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "★ {star_name}"
                                                            ))
                                                            .size(11.0)
                                                            .strong()
                                                            .color(theme::STAR_GOLD),
                                                        );
                                                    }
                                                    let is_sel = fleet_ui_state.spawn_location_body
                                                        == Some(*e);
                                                    if ui
                                                        .selectable_label(
                                                            is_sel,
                                                            egui::RichText::new(format!(
                                                                "  {body_name}"
                                                            ))
                                                            .size(12.0),
                                                        )
                                                        .clicked()
                                                    {
                                                        fleet_ui_state.spawn_location_body =
                                                            Some(*e);
                                                    }
                                                }
                                            });
                                    }
                                });

                                if ui
                                    .button(egui::RichText::new("＋ Create Fleet").size(13.0))
                                    .clicked()
                                {
                                    let spawn_body =
                                        fleet_ui_state.spawn_location_body.or_else(|| {
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
                                                stationary: false,
                                            },
                                        );
                                    }
                                }
                            }
                        });
                    },
                );

                ui.add_space(theme::Spacing::sm);

                // ── Right column: selected fleet details + transfer planner ──────
                let remaining = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(remaining, available.y - 80.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        theme::elevated_frame().show(ui, |ui| {
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
                                                &pending_resource_requests,
                                                &stockpiles,
                                                &coords_query,
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
                                        egui::RichText::new(
                                            "Select a fleet from the list to view details.",
                                        )
                                        .font(theme::heading())
                                        .italics()
                                        .color(theme::TEXT_DIM),
                                    );
                                });
                            }
                        });
                    },
                );
            });
        });

    // ── Disband confirmation popup ────────────────────────────────────────────
    if let Some(fleet_to_disband) = fleet_ui_state.disband_confirm_fleet {
        let fleet_info = fleet_query
            .get(fleet_to_disband)
            .ok()
            .map(|(_, f, _, _, _)| (f.name.clone(), f.ships.len()));
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
                        ui.label(egui::RichText::new("⚠").size(36.0).color(theme::AMBER));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("Disband \"{}\"?", fleet_name))
                            .strong()
                            .size(15.0)
                            .color(theme::AMBER),
                    );
                    if ship_count > 0 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "This will permanently destroy {} ship(s).\nThis action cannot be undone.",
                                ship_count
                            ))
                            .size(13.0)
                            .color(theme::RED),
                        );
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("Cancel").size(13.0)).clicked() {
                            cancel = true;
                        }
                        ui.add_space(theme::Spacing::lg);
                        if ui
                            .button(
                                egui::RichText::new("🗑 Disband")
                                    .size(13.0)
                                    .color(theme::RED),
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
                fleet_ui_state
                    .selected_fleets
                    .retain(|&e| e != fleet_to_disband);
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

    // ── Ship scrap confirmation popup ────────────────────────────────────────
    if let Some((fleet_entity, ship_idx)) = fleet_ui_state.scrap_confirm_ship {
        let ship_info = fleet_query
            .get(fleet_entity)
            .ok()
            .and_then(|(_, fleet, _, _, _)| {
                fleet.ships.get(ship_idx).map(|ship| {
                    (
                        fleet.name.clone(),
                        fleet.ships.len(),
                        ship.name.clone(),
                        format!("{} {}", ship.class.icon(), ship.class.display_name()),
                    )
                })
            });
        if let Some((fleet_name, fleet_ship_count, ship_name, ship_class)) = ship_info {
            let mut do_scrap = false;
            let mut cancel = false;
            egui::Window::new("⚠ Confirm Scrap")
                .id(egui::Id::new("fleet_ship_scrap_confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(380.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("⚠").size(36.0).color(theme::AMBER));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("Scrap \"{}\"?", ship_name))
                            .strong()
                            .size(15.0)
                            .color(theme::AMBER),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} in fleet \"{}\" will be permanently destroyed.",
                            ship_class, fleet_name
                        ))
                        .size(13.0)
                        .color(theme::TEXT_VALUE),
                    );
                    if fleet_ship_count == 1 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "This is the last ship in the fleet, so the fleet will also be removed.",
                            )
                            .size(13.0)
                            .color(theme::RED),
                        );
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("Cancel").size(13.0)).clicked() {
                            cancel = true;
                        }
                        ui.add_space(theme::Spacing::lg);
                        if ui
                            .button(
                                egui::RichText::new("🗑 Scrap")
                                    .size(13.0)
                                    .color(theme::RED),
                            )
                            .clicked()
                        {
                            do_scrap = true;
                        }
                    });
                    ui.add_space(4.0);
                });
            if do_scrap {
                pending_actions.scrap_ships.push((fleet_entity, ship_idx));
                fleet_ui_state.scrap_confirm_ship = None;
            }
            if cancel {
                fleet_ui_state.scrap_confirm_ship = None;
            }
        } else {
            fleet_ui_state.scrap_confirm_ship = None;
        }
    }
}

/// Walk up the `LogicalParent` chain to find the star name for a body.
fn find_body_star_name(
    mut body_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
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
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
    settings: &Settings,
    company_filter_set: &std::collections::HashSet<Entity>,
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
        .filter(|(entity, fleet, _, maybe_maneuver, _)| {
            // GRA-37.e: when the company filter is active, keep only the
            // fleets that the filtered company is currently using.
            if !company_filter_set.is_empty() && !company_filter_set.contains(entity) {
                return false;
            }
            // GRA-41: hide in-transit freighter fleets when the player
            // has toggled the visibility off.  Combat / survey fleets
            // (no `ShipClass::Freighter` ship) are unaffected.
            if settings.show_freighters_in_transit {
                return true;
            }
            let in_transit = maybe_maneuver.is_some();
            !(in_transit && fleet.has_freighter_ship())
        })
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
                    .color(theme::STAR_GOLD),
            );
        }

        let is_primary = fleet_ui_state.selected_fleet == Some(entry.entity);
        let is_checked = fleet_ui_state.selected_fleets.contains(&entry.entity);
        let row_text = format!(
            "{} {} — {} ship(s)",
            entry.role_icon, entry.name, entry.ship_count
        );
        // Selected: bright accent so text is readable on the teal selection background.
        // Unselected in-transit: cool blue.  Unselected orbiting: bright teal-green.
        let row_color = if is_primary {
            theme::ACCENT
        } else if entry.in_transit {
            theme::RP_BLUE
        } else {
            theme::EP_TEAL
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
                    fleet_ui_state
                        .selected_fleets
                        .retain(|&e| e != entry.entity);
                }
            }

            let drop_result = ui.dnd_drop_zone::<(Entity, usize), _>(egui::Frame::NONE, |ui| {
                // Measure the full text to decide whether to marquee-scroll.
                let font_id = egui::FontId::proportional(13.0);
                let available_w = ui.available_width().max(1.0);
                let row_height = ui.text_style_height(&egui::TextStyle::Body);
                let full_text_width = ui
                    .painter()
                    .layout_no_wrap(row_text.clone(), font_id.clone(), row_color)
                    .size()
                    .x;

                let (rect, resp) = ui.allocate_exact_size(
                    egui::Vec2::new(available_w, row_height),
                    egui::Sense::click_and_drag(),
                );

                // Selection / hover background — themed to match the detail panel palette.
                let row_rounding = egui::CornerRadius::same(3);
                if is_primary {
                    // Dark teal fill so the bright ACCENT text is easy to read.
                    ui.painter().rect_filled(
                        rect.expand(1.0),
                        row_rounding,
                        theme::BUTTON_ACTIVE_BG,
                    );
                    ui.painter().rect_stroke(
                        rect.expand(1.0),
                        row_rounding,
                        egui::Stroke::new(1.0, theme::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                } else if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect.expand(1.0), row_rounding, theme::SURFACE_RAISED);
                    ui.painter().rect_stroke(
                        rect.expand(1.0),
                        row_rounding,
                        egui::Stroke::new(1.0, theme::BORDER),
                        egui::StrokeKind::Inside,
                    );
                }

                // Narrow the draw region slightly so text doesn't touch the edge.
                let text_rect = rect.shrink2(egui::Vec2::new(4.0, 0.0));
                if full_text_width <= text_rect.width() {
                    // Text fits — draw as-is.
                    ui.painter().text(
                        text_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        &row_text,
                        font_id,
                        row_color,
                    );
                } else {
                    // Continuous marquee: two copies separated by a gap loop seamlessly.
                    let gap = 48.0_f32;
                    let cycle = full_text_width + gap;
                    let speed = 40.0_f64;
                    let t = ui.ctx().input(|i| i.time);
                    let offset_x = ((t * speed) % cycle as f64) as f32;
                    let painter = ui.painter().with_clip_rect(text_rect);
                    let galley =
                        painter.layout_no_wrap(row_text.clone(), font_id.clone(), row_color);
                    let y = text_rect.top() + (text_rect.height() - galley.size().y) * 0.5;
                    let x0 = text_rect.left() - offset_x;
                    painter.galley(egui::pos2(x0, y), galley.clone(), row_color);
                    let x1 = x0 + cycle;
                    if x1 < text_rect.right() + full_text_width {
                        painter.galley(egui::pos2(x1, y), galley, row_color);
                    }
                    ui.ctx().request_repaint();
                }

                if resp.clicked() {
                    let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                    let shift = ui.input(|i| i.modifiers.shift);
                    if ctrl {
                        if fleet_ui_state.selected_fleets.contains(&entry.entity) {
                            fleet_ui_state
                                .selected_fleets
                                .retain(|&e| e != entry.entity);
                        } else {
                            fleet_ui_state.selected_fleets.push(entry.entity);
                        }
                    } else if shift {
                        let anchor = fleet_ui_state.last_single_selected.unwrap_or(entry.entity);
                        let ai = sorted_entities
                            .iter()
                            .position(|&e| e == anchor)
                            .unwrap_or(0);
                        let ci = sorted_entities
                            .iter()
                            .position(|&e| e == entry.entity)
                            .unwrap_or(0);
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

        // ── Sub-status line — marquee-scrolls when the text is too wide ───────
        let (sub_text, sub_color): (String, egui::Color32) =
            if let Some(wait_str) = &entry.waiting_depart {
                (format!("    Waiting — T-minus {wait_str}"), theme::AMBER)
            } else if let Some((prog, rem)) = &entry.transit_progress {
                (
                    format!(
                        "    ✈ {} — {}% done, {} left",
                        entry.location_text, prog, rem
                    ),
                    theme::TEXT_VALUE,
                )
            } else {
                (
                    format!("    {} — fuel {}%", entry.location_text, entry.fuel_pct),
                    theme::TEXT_DIM,
                )
            };
        render_marquee_line(ui, &sub_text, sub_color, egui::FontId::proportional(11.0));
    }

    // ── Multi-select action bar ───────────────────────────────────────────────
    let n = fleet_ui_state.selected_fleets.len();
    if n >= 2 {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{n} selected"))
                    .size(12.0)
                    .color(theme::TEXT_VALUE),
            );
            // All selected fleets must be in orbit at the same body (not in transit).
            let merge_bodies: Vec<Option<Entity>> = fleet_ui_state
                .selected_fleets
                .iter()
                .map(|&e| {
                    fleet_query.get(e).ok().and_then(|(_, _, mo, ma, _)| {
                        if ma.is_some() {
                            None
                        } else {
                            mo.map(|o| o.body)
                        }
                    })
                })
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
                .add_enabled(
                    all_same_location,
                    egui::Button::new(egui::RichText::new("⊕ Merge").size(13.0)),
                )
                .on_hover_text(merge_tooltip)
                .clicked()
            {
                let target_fleet =
                    fleet_ui_state
                        .selected_fleets
                        .iter()
                        .copied()
                        .max_by_key(|&e| {
                            fleet_query
                                .get(e)
                                .map(|(_, f, _, _, _)| f.ships.len())
                                .unwrap_or(0)
                        });
                if let Some(target_fleet) = target_fleet {
                    let source_fleets = fleet_ui_state
                        .selected_fleets
                        .iter()
                        .copied()
                        .filter(|&e| e != target_fleet)
                        .collect();
                    pending_actions.merge_fleets.push(MergeFleetAction {
                        source_fleets,
                        target_fleet,
                    });
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
        egui::RichText::new(
            "💡 Drag a ship row → fleet to transfer it  ·  Ctrl/⌘+click or Shift+click to multi-select",
        )
        .size(10.0)
        .italics()
        .color(theme::TEXT_DIM),
    );
}

/// Render a single-line label that marquee-scrolls (pause → slide left → pause) when the
/// text is wider than the available width.  Clips the text to a clean rectangle so adjacent
/// rows are never disturbed.  Uses real-time `ui.input(|i| i.time)` so the animation runs
/// regardless of simulation speed.
pub(super) fn render_marquee_line(
    ui: &mut egui::Ui,
    text: &str,
    color: egui::Color32,
    font_id: egui::FontId,
) {
    let available_w = ui.available_width().max(1.0);
    // Calculate text height from the font so the allocated rect is exactly right.
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font_id.clone(), color);
    let text_size = galley.size();
    let row_h = text_size.y.max(14.0);

    let (rect, _) =
        ui.allocate_exact_size(egui::Vec2::new(available_w, row_h), egui::Sense::hover());
    // Narrow slightly so the text never clips hard against the panel border.
    let clip = rect.shrink2(egui::Vec2::new(4.0, 0.0));
    let painter = ui.painter().with_clip_rect(clip);

    if text_size.x <= clip.width() {
        // Fits — draw statically.
        painter.text(
            clip.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            font_id,
            color,
        );
    } else {
        // Continuous marquee: two copies separated by a gap scroll left seamlessly.
        let gap = 48.0_f32;
        let cycle = text_size.x + gap;
        let speed = 40.0_f64; // px / real-second
        let t = ui.ctx().input(|i| i.time);
        let offset_x = ((t * speed) % cycle as f64) as f32;
        let galley = painter.layout_no_wrap(text.to_string(), font_id.clone(), color);
        let y = clip.top() + (clip.height() - text_size.y) * 0.5;
        let x0 = clip.left() - offset_x;
        painter.galley(egui::pos2(x0, y), galley.clone(), color);
        let x1 = x0 + cycle;
        if x1 < clip.right() + text_size.x {
            painter.galley(egui::pos2(x1, y), galley, color);
        }
        ui.ctx().request_repaint();
    }
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
    let name_color = theme::TEXT_VALUE;

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
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(max_width, widget_height), egui::Sense::hover());
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

/// Pick the best representative source body for a request's ETA display.
///
/// When the request has an explicit `source_body` we use that, otherwise we
/// fall back to the body with the largest current `LocalStockpile` of the
/// requested resource.  If no matching stockpile is found, the destination
/// body itself is returned so the ETA computation has a sensible coordinate.
fn pick_eta_source_for_request(
    request: &ResourceRequest,
    stockpiles: &Query<(Entity, &LocalStockpile)>,
) -> Entity {
    if let Some(src) = request.source_body {
        return src;
    }
    let mut best: Option<(Entity, f64)> = None;
    for (entity, ls) in stockpiles.iter() {
        let amt = ls.get(&request.resource);
        if amt > 0.0 && best.is_none_or(|(_, b)| amt > b) {
            // Strict greater-than — ties keep the first (smaller-entity) body.
            best = Some((entity, amt));
        }
    }
    best.map(|(e, _)| e).unwrap_or(request.destination_body)
}

/// Render the **Logistics** section of the fleet panel.
///
/// Lists open `ResourceRequest`s at the fleet's current body and provides an
/// **Assign** button per request that queues an
/// `AssignLogisticsRequestAction`.  Each row shows the resource, amount,
/// priority, source body, and the fleet's projected Hohmann round-trip ETA.
///
/// Section is a no-op for fleets that are currently in transfer
/// (`maybe_orbit == None`) — there's no destination to assign against.
#[allow(clippy::too_many_arguments)]
fn render_logistics_section(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    maybe_orbit: Option<&FleetOrbit>,
    pending_resource_requests: &PendingResourceRequests,
    stockpiles: &Query<(Entity, &LocalStockpile)>,
    coords_query: &Query<&SpaceCoordinates, Without<Fleet>>,
    pending_actions: &mut PendingFleetActions,
    _body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("📦 LOGISTICS")
                    .strong()
                    .size(13.0)
                    .color(theme::RP_BLUE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Open delivery requests at this body")
                        .italics()
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                );
            });
        });
        theme::divider(ui);

        let Some(orbit) = maybe_orbit else {
            ui.label(
                egui::RichText::new("Fleet is in transit — no destination to assign against.")
                    .italics()
                    .color(theme::TEXT_DIM),
            );
            return;
        };

        // Collect Pending requests for the fleet's destination body.
        let mut at_body: Vec<&ResourceRequest> = pending_resource_requests
            .requests
            .iter()
            .filter(|r| {
                r.destination_body == orbit.body
                    && r.state == RequestState::Pending
                    && r.assignee_fleet_id.is_none()
            })
            .collect();
        // Stable order: higher priority first, then oldest id first within tier.
        at_body.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));

        if at_body.is_empty() {
            ui.label(
                egui::RichText::new("No open requests at this body.")
                    .italics()
                    .color(theme::TEXT_DIM),
            );
            return;
        }

        egui::Grid::new("logistics_request_grid")
            .num_columns(4)
            .spacing([12.0, 4.0])
            .striped(true)
            .min_col_width(40.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Resource").strong().size(11.0));
                ui.label(egui::RichText::new("Amount").strong().size(11.0));
                ui.label(egui::RichText::new("ETA").strong().size(11.0));
                ui.label(egui::RichText::new("").strong());
                ui.end_row();

                for request in at_body {
                    let priority_color = match request.priority {
                        RequestPriority::Emergency => theme::RED,
                        RequestPriority::Construction => theme::RP_BLUE,
                        RequestPriority::Maintenance => theme::TEXT_VALUE,
                        RequestPriority::Trade => theme::TEXT_DIM,
                    };

                    ui.horizontal(|ui| {
                        ui.colored_label(
                            priority_color,
                            egui::RichText::new(format!("{:?}", request.priority)).size(10.0),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:?}", request.resource))
                                .strong()
                                .size(12.0),
                        );
                    });
                    ui.label(
                        egui::RichText::new(format!("{:.1} Mt", request.amount_mt)).size(12.0),
                    );

                    let eta_source = pick_eta_source_for_request(request, stockpiles);
                    let transit_s =
                        hohmann_round_trip_seconds(orbit.body, eta_source, coords_query);
                    let transit_days = (transit_s / 86_400.0).round() as i64;
                    ui.label(
                        egui::RichText::new(format!("~{transit_days} d"))
                            .size(11.0)
                            .color(theme::TEXT_DIM),
                    );

                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Assign").size(11.0))
                                .min_size(egui::Vec2::new(64.0, 22.0)),
                        )
                        .on_hover_text(format!(
                            "Assign this fleet to deliver {:.1} Mt of {:?} to {} (ETA ~{} days).",
                            request.amount_mt,
                            request.resource,
                            request.destination_name,
                            transit_days
                        ))
                        .clicked()
                    {
                        pending_actions.assign_logistics_requests.push(
                            crate::fleets::components::AssignLogisticsRequestAction {
                                fleet: fleet_entity,
                                request_id: request.id,
                            },
                        );
                    }
                    ui.end_row();
                }
            });
    });
}

/// Render the right panel: fleet details (ship manifest, stats, status) and transfer planner.
#[allow(clippy::too_many_arguments)]
fn render_fleet_detail(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    maybe_orbit: Option<&FleetOrbit>,
    maybe_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
    pending_resource_requests: &PendingResourceRequests,
    stockpiles: &Query<(Entity, &LocalStockpile)>,
    coords_query: &Query<&SpaceCoordinates, Without<Fleet>>,
) {
    // ── Fleet header ─────────────────────────────────────────────────────────
    // Row 1: fleet name + ✏ button (full width, no competing right-side controls)
    ui.horizontal(|ui| {
        let name_area_width = (ui.available_width() - 32.0).max(60.0);

        let is_editing_this = fleet_ui_state
            .editing_fleet_name
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
                    let cancelled =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let committed = response.lost_focus() && !cancelled;
                    if !committed && !cancelled {
                        response.request_focus();
                    }
                    (
                        if committed {
                            Some(current_name.clone())
                        } else {
                            None
                        },
                        cancelled,
                    )
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
            render_fleet_name_marquee(
                ui,
                fleet,
                fleet_entity,
                name_area_width,
                &mut fleet_ui_state.editing_fleet_name,
            );
        }
    });

    // Row 2: Role selector + Disband (right-aligned)
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui::RichText::new("🗑 Disband").color(theme::RED))
                .on_hover_text(if fleet.ships.is_empty() {
                    "Disband this fleet"
                } else {
                    "Disband fleet (destroys all ships)"
                })
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
                        if ui
                            .selectable_label(
                                fleet.role == role,
                                format!("{} {}", role.icon(), role.display_name()),
                            )
                            .clicked()
                        {
                            pending_actions
                                .change_fleet_roles
                                .push((fleet_entity, role));
                        }
                    }
                });
            ui.label("Role:");
        });
    });
    ui.separator();

    // ── Current status ────────────────────────────────────────────────────────
    if let Some(maneuver) = maybe_maneuver {
        render_active_maneuver_status(
            ui,
            fleet_entity,
            maneuver,
            fleet,
            body_query,
            pending_actions,
            elapsed,
            fleet_ui_state.waiting_orbit_count,
        );
    } else if let Some(orbit) = maybe_orbit {
        render_orbit_status(ui, orbit, fleet, body_query);
    }

    ui.separator();

    // ── Ship manifest ─────────────────────────────────────────────────────────
    ui.label(egui::RichText::new("Ship Manifest").strong().size(14.0));
    let in_orbit_for_manifest = maybe_orbit.is_some();
    let manifest_drag_width = ship_manifest_drag_width(ui.available_width());
    paint_ship_manifest_header(ui, manifest_drag_width);
    ui.add_space(2.0);
    for (idx, ship) in fleet.ships.iter().enumerate() {
        render_ship_manifest_row(
            ui,
            fleet_entity,
            idx,
            ship,
            in_orbit_for_manifest,
            fleet_ui_state,
            pending_actions,
        );
        ui.add_space(2.0);
    }

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
                egui::RichText::new(format!("{:.0} t ({fuel_pct}%)", fleet.total_fuel_t()))
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
                    .color(theme::ACCENT),
            );
            ui.end_row();
        });

    // ── Transfer Planner shortcut ─────────────────────────────────────────
    // The planner now lives in a floating popup; show a button to open it.
    let can_plan = maybe_orbit.is_some() || maybe_maneuver.is_some();
    if can_plan {
        ui.separator();
        if ui
            .add(
                egui::Button::new(egui::RichText::new("📡 Open Transfer Planner ↗").size(13.0))
                    .min_size(egui::Vec2::new(200.0, 32.0)),
            )
            .on_hover_text("Open the orbital transfer planner in a floating window")
            .clicked()
        {
            fleet_ui_state.show_transfer_popup = true;
        }
    }

    // ── Logistics section (GRA-33 / PR-B) ─────────────────────────────────
    // Only meaningful for fleets that contain a `ShipClass::Freighter` and
    // are currently in orbit (i.e. not on a transfer).  Lists open
    // `ResourceRequest`s at the fleet's current body and exposes an
    // **Assign** button per request that queues an `AssignLogisticsRequestAction`.
    if fleet.has_freighter_ship() {
        ui.separator();
        render_logistics_section(
            ui,
            fleet_entity,
            maybe_orbit,
            pending_resource_requests,
            stockpiles,
            coords_query,
            pending_actions,
            body_query,
        );
    }
}

// (Transfer Planner is now a floating popup — see ui_transfer_planner_popup.)

/// Show current maneuver status with a progress bar.
fn render_active_maneuver_status(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    maneuver: &ActiveManeuver,
    fleet: &Fleet,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
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
                        .color(theme::AMBER),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("T-minus {}", wait_str))
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            });

            // Orbit-wait counter from the trajectory gizmo.
            if waiting_orbit_count > 1 {
                ui.label(
                    egui::RichText::new(format!(
                        "× {} orbits until departure angle",
                        waiting_orbit_count
                    ))
                    .size(11.0)
                    .color(theme::GRAVITY_ASSIST),
                );
            }

            ui.add_space(4.0);
            if ui
                .button(egui::RichText::new("🛑 Abort Mission").size(12.0))
                .clicked()
            {
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
                    .color(theme::RP_BLUE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} remaining", remaining))
                        .size(12.0)
                        .color(theme::TEXT_DIM),
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
                    egui::RichText::new(maneuver.option_label)
                        .size(12.0)
                        .strong(),
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
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
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
                    .color(theme::GREEN),
            );
        });
        egui::Grid::new("orbit_info")
            .num_columns(4)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new(altitude_label).size(12.0));
                ui.label(egui::RichText::new(altitude_value).size(12.0).strong());
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
    all_fleets_query: Query<
        (
            Entity,
            &Fleet,
            &SpaceCoordinates,
            Option<&FleetOrbit>,
            Option<&ActiveManeuver>,
        ),
        Without<CelestialBody>,
    >,
    body_query: Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
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
    } else {
        // Use origin_body (the departure body) rather than destination_body.
        // If we used destination_body and the user re-targets the same destination,
        // r1 == r2 and the planner shows "Same orbit, 0 m/s" with zero travel time.
        maybe_maneuver
            .map(|maneuver| FleetOrbit::new(maneuver.origin_body, maneuver.arrival_orbit_radius_au))
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
        GameMenu::Fleets | GameMenu::Research | GameMenu::Construction | GameMenu::Economy
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
                ui.add_space(theme::Spacing::sm);

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
                            .color(theme::GREEN),
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
                                        .color(theme::RP_BLUE),
                                );
                            });
                        }
                    }
                });

                ui.separator();

                // Transfer Planner — always available
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("🗺 Transfer Planner").size(13.0))
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

                ui.add_space(theme::Spacing::sm);
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
                            egui::RichText::new("⚔ Attack").size(13.0).color(theme::RED),
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
                                .color(theme::RED),
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
                                .color(theme::RED),
                        )
                        .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text("Land troops to take control of the colony")
                    .clicked()
                {
                    info!("Invade requested for {:?}", selected_entity);
                }

                ui.add_space(theme::Spacing::sm);
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
                    ui.add_space(theme::Spacing::sm);
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("⛔ Abort Transfer")
                                    .size(13.0)
                                    .color(theme::RED),
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
        anchor.0 = fleet_query.get(fleet_entity).ok().map(|orbit| orbit.body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_scale_creation() {
        let time_scale = TimeScale::new();
        assert_eq!(time_scale.scale, 3_600.0); // default is 1 hr/s
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
