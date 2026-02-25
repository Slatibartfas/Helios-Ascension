use super::*;
use super::time::format_time_rate;
use super::resources_bar::format_population;

fn render_selectable_label(ui: &mut egui::Ui, is_selected: bool, name: &str) -> egui::Response {
    if is_selected {
        ui.selectable_label(is_selected, name).highlight()
    } else {
        ui.selectable_label(is_selected, name)
    }
}

/// Returns a Unicode icon for each body type to distinguish entries in the ledger
fn body_type_icon(body_type: &BodyType) -> &'static str {
    match body_type {
        BodyType::Star => "\u{2605}",       // ★
        BodyType::Planet => "\u{25CF}",     // ●
        BodyType::Moon => "\u{25D1}",       // ◑
        BodyType::DwarfPlanet => "\u{25CC}", // ◌
        BodyType::Asteroid => "\u{25C6}",   // ◆
        BodyType::Comet => "\u{2604}",      // ☄
        BodyType::GasGiant => "\u{25C9}",   // ◉
        BodyType::Ring => "\u{25CB}",       // ○
    }
}

fn render_body_row(
    ui: &mut egui::Ui,
    entity: Entity,
    body: &CelestialBody,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    let is_selected = selection.is_selected(entity);
    let type_icon = body_type_icon(&body.body_type);
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        if ui
            .small_button("⚓")
            .on_hover_text("Anchor Camera")
            .clicked()
        {
            // Select the body when anchoring
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            commands.entity(entity).insert(Selected);
            selection.select(entity);

            // Anchor the camera
            if let Ok(mut anchor) = anchor_query.single_mut() {
                anchor.0 = Some(entity);
            }
        }

        // Use a visually distinct style for selected items
        let display_name = format!("{} {}", type_icon, body.name);
        if render_selectable_label(ui, is_selected, &display_name).clicked() {
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            commands.entity(entity).insert(Selected);
            selection.select(entity);
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
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    if children.is_empty() {
        return;
    }

    // Make ID unique by including parent entity to avoid UI jumping bug
    let id = ui.make_persistent_id((group_name, parent_entity));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            ui.label(format!("{} ({})", group_name, children.len()));
        })
        .body(|ui| {
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
                    anchor_query,
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
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    if let Some(body) = body_map.get(&entity) {
        let is_selected = selection.is_selected(entity);
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
                        BodyType::Planet => child_planets.push(child),
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
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                body.body_type == BodyType::Star,
            )
            .show_header(ui, |ui| {
                if ui
                    .small_button("⚓")
                    .on_hover_text("Anchor Camera")
                    .clicked()
                {
                    // Select the body when anchoring
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);

                    // Anchor the camera
                    if let Ok(mut anchor) = anchor_query.single_mut() {
                        anchor.0 = Some(entity);
                    }
                }

                // Use a visually distinct style for selected items
                let type_icon = body_type_icon(&body.body_type);
                let display_name = format!("{} {}", type_icon, body.name);
                if render_selectable_label(ui, is_selected, &display_name).clicked() {
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);
                }
            })
            .body(|ui| {
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
                        anchor_query,
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
                        anchor_query,
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
                    anchor_query,
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
                        anchor_query,
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
                    anchor_query,
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
                    anchor_query,
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
                        anchor_query,
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
                anchor_query,
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
    body_map: &std::collections::HashMap<Entity, &CelestialBody>,
    fleet_ui_state: &mut FleetUiState,
    selected_query: &Query<Entity, With<Selected>>,
    commands: &mut Commands,
    selection: &mut Selection,
    elapsed: f64,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
    sim_time: &SimulationTime,
) {
    let mut fleets: Vec<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)> =
        fleet_query.iter().map(|(e, f, o, m)| (e, f, o, m)).collect();
    fleets.sort_by(|a, b| a.1.name.cmp(&b.1.name));

    if fleets.is_empty() {
        ui.label(
            egui::RichText::new("  No fleets deployed")
                .size(12.0)
                .color(egui::Color32::GRAY)
                .italics(),
        );
        return;
    }

    for (entity, fleet, maybe_orbit, maybe_maneuver) in fleets {
        let is_selected = fleet_ui_state.selected_fleet == Some(entity);

        let status_icon = if maybe_maneuver.is_some() { "✈" } else { "🛰" };
        let display_name = format!("{} {}", status_icon, fleet.name);

        let row_color = if is_selected {
            egui::Color32::from_rgb(100, 220, 100)
        } else {
            egui::Color32::from_rgb(170, 200, 170)
        };

        let sub_status = if let Some(maneuver) = maybe_maneuver {
            if elapsed < maneuver.departure_time {
                "⏳ Waiting to depart".to_string()
            } else {
                "↗ In transit".to_string()
            }
        } else if let Some(orbit) = maybe_orbit {
            let body = body_map.get(&orbit.body);
            let body_name = body.map(|b| b.name.as_str()).unwrap_or("?");
            // Show a distinct label for heliocentric Lagrange-point orbits.
            if body.map(|b| b.body_type) == Some(BodyType::Star) {
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
            if fleet.ships.len() == 1 { "ship" } else { "ships" }
        );

        let row_response = ui.selectable_label(
            is_selected,
            egui::RichText::new(&display_name).color(row_color).size(13.0),
        );

        if row_response.clicked() {
            if is_selected {
                fleet_ui_state.selected_fleet = None;
            } else {
                // Clear body selection
                for e in selected_query.iter() {
                    commands.entity(e).remove::<Selected>();
                }
                selection.clear();
                fleet_ui_state.selected_fleet = Some(entity);
                fleet_ui_state.clear_target();
            }
        }

        if row_response.double_clicked() {
            // Select the fleet
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            selection.clear();
            fleet_ui_state.selected_fleet = Some(entity);
            fleet_ui_state.clear_target();

            // Camera anchor behaviour:
            // - Orbiting  → anchor to the body being orbited.
            // - In transit → anchor to the fleet dot itself so the camera follows
            //   the ship along its arc.  A separate system (switch_anchor_on_arrival)
            //   will redirect the anchor to the destination body once the maneuver
            //   completes.
            if let Ok(mut anchor) = anchor_query.single_mut() {
                if let Some(orbit) = maybe_orbit {
                    anchor.0 = Some(orbit.body);
                } else if maybe_maneuver.is_some() {
                    // Follow the moving fleet dot
                    anchor.0 = Some(entity);
                }
            }
        }

        // Sub-status line
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new(format!("{sub_status}  {ships_txt}"))
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );
        });

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
                            .color(egui::Color32::from_rgb(100, 180, 255)),
                    );
                });
            }
        }
    }
}

/// System that displays a tooltip for hovered celestial bodies or Lagrange points

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
        return ("+0/mo".to_string(), egui::Color32::GRAY);
    }
    if value > 0.0 {
        (format!("+{}/mo", format_mass(value)), egui::Color32::from_rgb(100, 255, 100))
    } else {
        (format!("{}/mo", format_mass(value)), egui::Color32::from_rgb(255, 100, 100))
    }
}

/// Format a monthly rate for points (integer display).
pub(super) fn format_points_rate_monthly(value: f64) -> (String, egui::Color32) {
    if value > 0.0 {
        (format!("+{:.0}/mo", value), egui::Color32::from_rgb(100, 255, 100))
    } else if value < 0.0 {
        (format!("{:.0}/mo", value), egui::Color32::from_rgb(255, 100, 100))
    } else {
        ("+0/mo".to_string(), egui::Color32::GRAY)
    }
}

/// Main UI dashboard system
#[allow(clippy::too_many_arguments)]
pub(super) fn ui_dashboard(
    mut commands: Commands,
    mut contexts: EguiContexts,
    // budget: Res<GlobalBudget>, // Moved to ui_resources_bar
    mut selection: ResMut<Selection>,
    current_system: Res<CurrentStarSystem>,
    nearby_stars: Res<NearbyStarsData>,
    active_menu: Res<ActiveMenu>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    // Query for selected body information
    mut body_query: Query<(
        &CelestialBody,
        Option<&SpaceCoordinates>,
        Option<&KeplerOrbit>,
        Option<&PlanetResources>,
        Option<&AtmosphereComposition>,
        Option<&mut SurveyLevel>,
        Option<&Population>,
        Option<&crate::astronomy::SurfaceTemperature>,
        Option<&LogicalParent>,
    )>,
    // Read-only lookup for parent body coordinates
    parent_coords_query: Query<&SpaceCoordinates>,
    // Resource query for system totals
    resource_query: Query<(&SystemId, &PlanetResources)>,
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
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    if active_menu.current == GameMenu::Research
        || active_menu.current == GameMenu::Construction
        || active_menu.current == GameMenu::Economy
        || active_menu.current == GameMenu::Fleets
    {
        return;
    }

    // Ledger Panel (Left)
    egui::SidePanel::left("ledger_panel")
        .min_width(200.0)
        .default_width(230.0)
        .show(ctx, |ui| {
            match active_menu.current {
                GameMenu::Starmap => {
                    // Starmap view: show list of star systems
                    ui.heading("Star Systems");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_salt("starmap_ledger_scroll")
                        .show(ui, |ui| {
                            for (entity, icon, is_selected) in star_system_query.iter() {
                                let response =
                                    render_selectable_label(ui, is_selected.is_some(), &icon.name);

                                if response.clicked() {
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
                    ui.heading("Celestial Objects");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_salt("ledger_scroll")
                        .show(ui, |ui| {
                            let mut hierarchy: std::collections::HashMap<Entity, Vec<Entity>> =
                                std::collections::HashMap::new();
                            let mut roots: Vec<Entity> = Vec::new();
                            let mut body_map: std::collections::HashMap<Entity, &CelestialBody> =
                                std::collections::HashMap::new();
                            let mut orbit_map: std::collections::HashMap<Entity, f64> =
                                std::collections::HashMap::new();

                            for (entity, body, logical_parent, orbit, system_id) in
                                all_bodies_query.iter()
                            {
                                // Filter by current star system
                                let sys_id = system_id.map(|s| s.0).unwrap_or(0);
                                if sys_id != current_system.0 {
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

                            for root in roots {
                                render_body_tree(
                                    ui,
                                    root,
                                    &body_map,
                                    &hierarchy,
                                    &mut selection,
                                    &mut commands,
                                    &selected_query,
                                    &mut anchor_query,
                                );
                            }

                            // ── Fleet section ─────────────────────────────
                            ui.add_space(4.0);
                            ui.separator();

                            let fleet_id = ui.make_persistent_id("survey_fleet_tree");
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                fleet_id,
                                true,
                            )
                            .show_header(ui, |ui| {
                                let n = fleet_query.iter().count();
                                ui.label(
                                    egui::RichText::new(format!("🚀 Fleets ({n})"))
                                        .strong()
                                        .color(egui::Color32::from_rgb(120, 210, 140)),
                                );
                            })
                            .body(|ui| {
                                render_fleet_ledger_tree(
                                    ui,
                                    &fleet_query,
                                    &body_map,
                                    &mut fleet_ui_state,
                                    &selected_query,
                                    &mut commands,
                                    &mut selection,
                                    sim_time.elapsed_seconds(),
                                    &mut anchor_query,
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
                            .color(egui::Color32::from_rgb(180, 180, 180))
                    );
                    
                    ui.add_space(10.0);
                    
                    match active_menu.current {
                        GameMenu::Main => {
                            ui.label("Main menu options:");
                            if ui.button("🚪 Quit Game").clicked() {
                                // TODO: Implement quit
                                info!("Quit clicked");
                            }
                            if ui.button("💾 Save Game").clicked() {
                                info!("Save clicked");
                            }
                            if ui.button("📂 Load Game").clicked() {
                                info!("Load clicked");
                            }
                            if ui.button("⚙ Options").clicked() {
                                info!("Options clicked");
                            }
                        }
                        GameMenu::Construction => {
                            // Handled by ui_construction_panels system
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
                            ui.label("Officers, managers, and personnel assignments will be shown here.");
                        }
                        GameMenu::Intel => {
                            ui.label("Intelligence reports on enemy factions will be shown here.");
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

    if let Some((_star_entity, star_icon, _)) = selected_star_system {
        // Show star system details
        render_star_system_panel(
            ctx,
            star_icon,
            &all_bodies_query,
            &resource_query,
            &nearby_stars,
        );
    } else if selection.has_selection() {
        // Show selected celestial body details
        egui::SidePanel::right("selection_panel")
            .min_width(300.0)
            .max_width(400.0)
            .show(ctx, |ui| {
                ui.heading("Selected Body");
                ui.separator();

                if let Some(entity) = selection.get() {
                    if let Ok((body, opt_coords, orbit, resources, atmosphere, mut survey_level, population, surface_temp, logical_parent)) = body_query.get_mut(entity) {
                        // Body name and basic info
                        ui.label(egui::RichText::new(&body.name).size(18.0).strong());
                        ui.add_space(10.0);

                        // Position information
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Position").strong());
                            // For non-star bodies, compute distance relative to
                            // the system primary (star) using the absolute position.
                            if !matches!(body.body_type, crate::plugins::solar_system_data::BodyType::Star) {
                                if let Some(coords) = opt_coords {
                                    // Walk up the LogicalParent chain to find the system star,
                                    // then subtract its absolute universe position so we get the
                                    // true orbital distance regardless of the star's distance from Sol.
                                    let star_pos = {
                                        let mut current = logical_parent.map(|lp| lp.0);
                                        let mut found = bevy::math::DVec3::ZERO;
                                        while let Some(parent_entity) = current {
                                            if let Ok((_, parent_body, grandparent, _, _)) = all_bodies_query.get(parent_entity) {
                                                if matches!(parent_body.body_type, crate::plugins::solar_system_data::BodyType::Star) {
                                                    if let Ok(star_coords) = parent_coords_query.get(parent_entity) {
                                                        found = star_coords.position;
                                                    }
                                                    break;
                                                }
                                                current = grandparent.map(|gp| gp.0);
                                            } else {
                                                break;
                                            }
                                        }
                                        found
                                    };
                                    let distance = (coords.position - star_pos).length();
                                    ui.label(format!("Distance from Star: {:.3} AU", distance));
                                } else {
                                    ui.label("Position: co-orbiting parent body");
                                }
                            }
                            ui.label(format!("Radius: {:.1} km", body.radius));
                            ui.label(format!("Mass: {:.2e} kg", body.mass));
                            ui.label(format!("Gravity: {:.2} g", body.surface_gravity()));
                            if let Some(pop) = population {
                                if pop.count > 0.0 {
                                    ui.label(format!("Population: {}", format_population(pop.count)));
                                }
                            }
                        });

                        ui.add_space(10.0);

                        // Orbital data if available
                        if let Some(orbit) = orbit {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("Orbital Elements").strong());
                                ui.label(format!("Semi-major axis: {:.3} AU", orbit.semi_major_axis));
                                ui.label(format!("Eccentricity: {:.4}", orbit.eccentricity));
                                ui.label(format!("Inclination: {:.2}°", orbit.inclination.to_degrees()));
                                
                                // Calculate and show orbital period
                                let period_seconds = crate::astronomy::KeplerOrbit::period_from_mean_motion(orbit.mean_motion);
                                let period_days = period_seconds / 86400.0;
                                if period_days < 365.0 {
                                    ui.label(format!("Period: {:.1} days", period_days));
                                } else {
                                    ui.label(format!("Period: {:.2} years", period_days / 365.25));
                                }
                            });

                            ui.add_space(10.0);
                        }

                        // Rings cannot be colonised or have buildings — show a concise note
                        // instead of the full habitability / colony-cost section.
                        if body.body_type == crate::plugins::solar_system_data::BodyType::Ring {
                            ui.group(|ui| {
                                ui.label(
                                    egui::RichText::new("⚠ Orbital Mining Only")
                                        .strong()
                                        .color(egui::Color32::from_rgb(255, 165, 0)),
                                );
                                ui.label("Ring systems consist of free-floating ice and dust.");
                                ui.label("They cannot be colonised or have buildings constructed.");
                                ui.label("Resources must be harvested by mining ships in orbit.");
                            });
                        } else {
                        // Show Colony Cost for all bodies
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Habitability").strong());
                            
                            let mut temp_c = -273.15;
                            let mut min_temp_c = -273.15;
                            let mut max_temp_c = -273.15;

                            // Try to get temperature from SurfaceTemperature component, then Atmosphere
                            if let Some(comp) = surface_temp {
                                temp_c = comp.average_celsius;
                                min_temp_c = comp.min_celsius;
                                max_temp_c = comp.max_celsius;
                            } else if let Some(atm) = atmosphere {
                                temp_c = atm.surface_temperature_celsius;
                                min_temp_c = temp_c;
                                max_temp_c = temp_c;
                            }

                            // Colony Cost
                            ui.horizontal(|ui| {
                                ui.label("Colony Cost:");
                                let gravity = body.surface_gravity();
                                let cost_details = crate::astronomy::components::calculate_colony_cost_details(
                                    gravity, 
                                    min_temp_c, 
                                    max_temp_c,
                                    atmosphere.as_deref()
                                );
                                let cost = cost_details.total_cost;
                                
                                let cost_tooltip = |ui: &mut egui::Ui| {
                                    if cost_details.heavy_gravity_limit_exceeded {
                                        ui.colored_label(egui::Color32::RED, "Uninhabitable: Gravity > 1.7g");
                                    } else {
                                        ui.label(egui::RichText::new("Colony Cost Factors").strong());
                                        ui.separator();
                                        
                                        if cost_details.base_cost > 0.0 {
                                            ui.label(format!("Base Cost (Unbreathable): +{:.2}", cost_details.base_cost));
                                        }
                                        if cost_details.cold_cost > 0.0 {
                                            ui.label(format!("Cold Penalty (Heating): +{:.2}", cost_details.cold_cost));
                                        }
                                        if cost_details.heat_cost > 0.0 {
                                            ui.label(format!("Heat Penalty (Cooling): +{:.2}", cost_details.heat_cost));
                                        }
                                        if cost_details.pressure_cost > 0.0 {
                                            ui.label(format!("High Pressure Penalty: +{:.2}", cost_details.pressure_cost));
                                        }
                                        if cost_details.low_gravity_penalty > 0.0 {
                                            ui.label(format!("Low Gravity Penalty: +{:.2}", cost_details.low_gravity_penalty));
                                        }
                                    }
                                };

                                if cost.is_infinite() {
                                    ui.colored_label(egui::Color32::RED, "Uninhabitable (Gravity)")
                                        .on_hover_ui(cost_tooltip);
                                } else {
                                    let cost_color = if cost <= 0.0 {
                                        egui::Color32::GREEN
                                    } else if cost <= 2.0 {
                                        egui::Color32::YELLOW
                                    } else if cost <= 5.0 {
                                        egui::Color32::from_rgb(255, 165, 0) // Orange
                                    } else {
                                        egui::Color32::RED
                                    };
                                    ui.colored_label(cost_color, format!("{:.2}", cost))
                                        .on_hover_ui(cost_tooltip);
                                }
                            });
                            
                            // Temperature display (moved out of Atmosphere section so it shows for everyone)
                            ui.horizontal(|ui| {
                                ui.label("Temperature:");
                                ui.label(format!("{:.1}°C", temp_c));
                            });
                        }); // end Habitability group
                        } // end else (non-ring body)
                        
                        ui.add_space(5.0);

                        // Atmosphere data if available
                        if let Some(atmosphere) = atmosphere {
                            ui.group(|ui| {
                                let id = ui.make_persistent_id(("atmosphere_header", entity));
                                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                                    .show_header(ui, |ui| {
                                        ui.label(egui::RichText::new("🌍 Atmosphere").strong());
                                    })
                                    .body(|ui| {
                                        // Basic atmosphere properties
                                        ui.horizontal(|ui| {
                                            // Display appropriate label based on whether this is reference or surface pressure
                                            if atmosphere.is_reference_pressure {
                                                ui.label("Pressure (at 1 bar ref):");
                                            } else {
                                                ui.label("Surface Pressure:");
                                            }
                                            let pressure_bar = atmosphere.surface_pressure_mbar / 1000.0;
                                            if pressure_bar >= 1.0 {
                                                ui.label(format!("{:.2} bar", pressure_bar));
                                            } else {
                                                ui.label(format!("{:.0} mbar", atmosphere.surface_pressure_mbar));
                                            }
                                        });
                                        
                                        // Show harvest altitude for gas giants
                                        if atmosphere.is_reference_pressure && atmosphere.harvest_altitude_bar > 0.0 {
                                            ui.horizontal(|ui| {
                                                ui.label("Harvest Altitude:");
                                                let yield_mult = atmosphere.harvest_yield_multiplier();
                                                ui.label(format!("{:.1} bar ({:.1}× yield)", 
                                                    atmosphere.harvest_altitude_bar, yield_mult));
                                            });
                                            
                                            ui.horizontal(|ui| {
                                                ui.label("Max Harvest Depth:");
                                                ui.label(format!("{:.1} bar (tech-limited)", 
                                                    atmosphere.max_harvest_altitude_bar));
                                            });
                                        }
                                        
                                        ui.horizontal(|ui| {
                                            ui.label("Breathable:");
                                            if atmosphere.breathable {
                                                ui.colored_label(egui::Color32::GREEN, "✓ Yes");
                                            } else {
                                                ui.colored_label(egui::Color32::RED, "✗ No");
                                            }
                                        });
                                        
                                        ui.add_space(5.0);
                                        
                                        // Gas composition in collapsible section
                                        let gas_id = ui.make_persistent_id(("gas_composition", entity));
                                        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), gas_id, false)
                                            .show_header(ui, |ui| {
                                                ui.label(egui::RichText::new("Gas Composition").size(12.0));
                                            })
                                            .body(|ui| {
                                                for gas in &atmosphere.gases {
                                                    ui.horizontal(|ui| {
                                                        ui.label(format!("  {}:", gas.name));
                                                        ui.label(format!("{:.2}%", gas.percentage));
                                                    });
                                                }
                                            });

                                    });
                            });

                            ui.add_space(10.0);
                        }

                        // Resources if available
                        if let Some(resources) = resources {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("Resources").strong());
                                ui.label(format!("Body mass: {:.2e} kg", body.mass));
                                ui.add_space(5.0);
                                
                                // Survey Controls
                                let current_level = survey_level.as_deref().copied().unwrap_or(SurveyLevel::Unsurveyed);
                                
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Survey Status:");
                                        let status_color = match current_level {
                                            SurveyLevel::Unsurveyed => egui::Color32::GRAY,
                                            SurveyLevel::OrbitalScan => egui::Color32::LIGHT_BLUE,
                                            SurveyLevel::SeismicSurvey => egui::Color32::YELLOW,
                                            SurveyLevel::CoreSample => egui::Color32::GREEN,
                                        };
                                        ui.label(egui::RichText::new(format!("{:?}", current_level)).strong().color(status_color));
                                    });
                                    
                                    if let Some(survey) = survey_level.as_deref_mut() {
                                        if *survey != SurveyLevel::CoreSample {
                                            if ui.button("Upgrade Survey").clicked() {
                                                *survey = match *survey {
                                                    SurveyLevel::Unsurveyed => SurveyLevel::OrbitalScan,
                                                    SurveyLevel::OrbitalScan => SurveyLevel::SeismicSurvey,
                                                    SurveyLevel::SeismicSurvey => SurveyLevel::CoreSample,
                                                    _ => SurveyLevel::CoreSample,
                                                };
                                            }
                                        }
                                    } else {
                                        if ui.button("Initialize Survey System").clicked() {
                                            commands.entity(entity).insert(SurveyLevel::OrbitalScan);
                                        }
                                    }
                                });
                                
                                ui.add_space(5.0);

                                if current_level != SurveyLevel::Unsurveyed {
                                    egui::ScrollArea::vertical()
                                        .max_height(400.0)
                                        .show(ui, |ui| {
                                            // Group resources by category
                                            for (category_name, category_resources) in ResourceType::by_category() {
                                                ui.label(egui::RichText::new(category_name).strong().color(egui::Color32::LIGHT_BLUE));
                                                
                                                for resource_type in &category_resources {
                                                    if let Some(deposit) = resources.get_deposit(resource_type) {
                                                        // Calculate discovered amount
                                                        let discovered_mt = current_level.discovered_amount(&deposit.reserve);
                                                        
                                                        // Skip resources with negligible amounts
                                                        // Threshold of 0.001 Mt (= 1 kt) prevents showing "0.0 kt" entries
                                                        if discovered_mt < 0.001 && !deposit.is_viable() {
                                                             continue;
                                                        }

                                                        ui.horizontal(|ui| {
                                                            ui.label(format!("  {} ({})", 
                                                                resource_type.display_name(),
                                                                resource_type.symbol()
                                                            ));
                                                        });
                                                        
                                                        // Tiered Display
                                                        ui.horizontal(|ui| {
                                                            ui.label("    Total Discovered:");
                                                            ui.label(egui::RichText::new(format_mass(discovered_mt)).strong());
                                                        });
                                                        
                                                        // Use the deposit's is_atmospheric flag to decide labels
                                                        // Atmospheric gases show: Atmospheric / Trapped-Dissolved / Chemically Bound
                                                        // Mineral deposits show: Proven Reserves / Deep Deposits / Planetary Bulk
                                                        let is_atm = deposit.is_atmospheric;
                                                        let proven_label = if is_atm { "    Atmospheric:" } else { "    Proven Reserves:" };
                                                        let deep_label = if is_atm { "    Trapped/Dissolved:" } else { "    Deep Deposits:" };
                                                        let bulk_label = if is_atm { "    Chemically Bound:" } else { "    Planetary Bulk:" };
                                                        
                                                        // Proven / Atmospheric (Always visible if Orbital+)
                                                        ui.horizontal(|ui| {
                                                            ui.label(proven_label);
                                                            if deposit.reserve.proven_crustal < 0.001 {
                                                                ui.label(egui::RichText::new("Depleted").color(egui::Color32::RED).strong());
                                                            } else {
                                                                ui.add(egui::ProgressBar::new(1.0)
                                                                    .text(format_mass(deposit.reserve.proven_crustal)));
                                                            }
                                                        });
                                                        
                                                        // Deep / Trapped
                                                        if matches!(current_level, SurveyLevel::SeismicSurvey | SurveyLevel::CoreSample) {
                                                            ui.horizontal(|ui| {
                                                                ui.label(deep_label);
                                                                if deposit.reserve.deep_deposits < 0.001 {
                                                                    ui.label(egui::RichText::new("Depleted").color(egui::Color32::RED).strong());
                                                                } else {
                                                                    ui.add(egui::ProgressBar::new(1.0)
                                                                        .text(format_mass(deposit.reserve.deep_deposits)));
                                                                }
                                                            });
                                                        } else {
                                                             ui.label(format!("    {}: ???", if is_atm { "Trapped/Dissolved" } else { "Deep Deposits" }));
                                                        }
                                                        
                                                        // Bulk / Chemically Bound
                                                        if current_level == SurveyLevel::CoreSample {
                                                            ui.horizontal(|ui| {
                                                                ui.label(bulk_label);
                                                                if deposit.reserve.planetary_bulk < 0.001 {
                                                                    ui.label(egui::RichText::new("Depleted").color(egui::Color32::RED).strong());
                                                                } else {
                                                                    ui.add(egui::ProgressBar::new(1.0)
                                                                        .text(format_mass(deposit.reserve.planetary_bulk)));
                                                                }
                                                            });
                                                        } else {
                                                            ui.label(format!("    {}: ???", if is_atm { "Chemically Bound" } else { "Planetary Bulk" }));
                                                        }
                                                        
                                                        // Only show concentration for non-atmospheric deposits
                                                        // Concentration is meaningless for gas in an atmosphere
                                                        if !is_atm {
                                                            ui.horizontal(|ui| {
                                                                ui.label("    Concentration:");
                                                                let conc = deposit.reserve.concentration;
                                                                let conc_text = if conc >= 0.01 {
                                                                    format!("{:.1}%", conc * 100.0)
                                                                } else if conc >= 0.000_01 {
                                                                    format!("{:.1} ppm", conc * 1_000_000.0)
                                                                } else if conc >= 0.000_000_01 {
                                                                    format!("{:.2} ppb", conc * 1_000_000_000.0)
                                                                } else {
                                                                    format!("{:.2e}", conc)
                                                                };
                                                                ui.add(egui::ProgressBar::new(conc.min(1.0))
                                                                    .text(conc_text));
                                                            });
                                                        }
                                                        
                                                        ui.add_space(3.0);
                                                    }
                                                }
                                                
                                                ui.add_space(8.0);
                                            }

                                            // Summary
                                            ui.separator();
                                            ui.label(format!("Total viable deposits: {}", resources.viable_count()));
                                            ui.label(format!("Total resource value estimates: {:.2}", resources.total_value()));
                                        });
                                } else {
                                    ui.label("Perform orbital scan to detect resources.");
                                }
                            });
                        } else {
                            ui.label("No resource data available");
                        }
                    } else {
                        ui.label("Selected entity not found");
                    }
                } else {
                    ui.label("No selection");
                }
            });
    }
}

/// Always-visible bottom panel for time controls.
///
/// Registered in `UiSystemSet::TopBar` so egui reserves the bottom strip
/// **before** any side panel (Research, Construction, Economy, etc.) is
/// rendered. This ensures the panel is never occluded regardless of the
/// active menu.
pub(super) fn ui_time_controls(
    mut contexts: EguiContexts,
    mut time_scale: ResMut<TimeScale>,
    sim_time: Res<SimulationTime>,
    view_mode: Res<ViewMode>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    egui::TopBottomPanel::bottom("time_controls")
        .min_height(80.0)
        .show(ctx, |ui| {
            ui.heading("Time Controls");
            ui.separator();

            ui.horizontal(|ui| {
                // Pause/Resume button
                if time_scale.is_paused() {
                    if ui.button("▶ Resume").clicked() {
                        time_scale.resume();
                    }
                } else if ui.button("⏸ Pause").clicked() {
                    time_scale.pause();
                }

                ui.separator();

                // Preset speed buttons with meaningful labels
                if ui.button("1 hr/s").clicked() {
                    time_scale.scale = 3_600.0;
                }
                if ui.button("1 day/s").clicked() {
                    time_scale.scale = 86_400.0;
                }
                if ui.button("1 wk/s").clicked() {
                    time_scale.scale = 604_800.0;
                }
                if ui.button("1 mo/s").clicked() {
                    time_scale.scale = 2_592_000.0;
                }
                if ui.button("1 yr/s").clicked() {
                    time_scale.scale = 31_557_600.0;
                }

                ui.separator();

                // Logarithmic slider for fine control
                ui.label("Speed:");
                ui.add(
                    egui::Slider::new(&mut time_scale.scale, 1.0..=MAX_TIME_SCALE)
                        .logarithmic(true)
                        .text("")
                        .custom_formatter(|v, _| format_time_rate(v as f32)),
                );
            });

            ui.horizontal(|ui| {
                ui.label(format!("Speed: {}", format_time_rate(time_scale.scale)));
                if time_scale.is_paused() {
                    ui.colored_label(egui::Color32::RED, "⏸ PAUSED");
                }
                ui.separator();
                ui.label(format!("Date: {}", sim_time.format_date_time()));
                ui.separator();
                let (view_label, view_color) = match *view_mode {
                    ViewMode::System => ("🔭 System View", egui::Color32::from_rgb(120, 180, 255)),
                    ViewMode::Starmap => {
                        ("🌌 Starmap View", egui::Color32::from_rgb(255, 200, 100))
                    }
                };
                ui.colored_label(view_color, view_label);
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
    resource_query: &Query<(&SystemId, &PlanetResources)>,
    nearby_stars: &Res<NearbyStarsData>,
) {
    egui::SidePanel::right("star_system_panel")
        .min_width(300.0)
        .max_width(400.0)
        .show(ctx, |ui| {
            ui.heading("Selected Star System");
            ui.separator();

            // System name
            ui.label(egui::RichText::new(&star_icon.name).size(18.0).strong());
            ui.add_space(10.0);

            // Distance from Sol
            let distance_ly = star_icon.position.length() / 63241.077;
            ui.group(|ui| {
                ui.label(egui::RichText::new("System Info").strong());
                ui.label(format!("Distance: {:.2} ly", distance_ly));
                ui.label(format!("System ID: {}", star_icon.id));
            });

            ui.add_space(10.0);

            // Try to find detailed system data
            if let Some(system_data) = nearby_stars.get_by_id(star_icon.id) {
                // Star properties
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Star Properties").strong());

                    for (star_idx, star_data) in system_data.stars.iter().enumerate() {
                        if system_data.stars.len() > 1 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Star {}: {}",
                                    star_idx + 1,
                                    &star_data.name
                                ))
                                .color(egui::Color32::from_rgb(200, 200, 255)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(&star_data.name)
                                    .color(egui::Color32::from_rgb(200, 200, 255)),
                            );
                        }

                        ui.label(format!("  Type: {}", star_data.spectral_type));
                        ui.label(format!("  Mass: {:.2} M☉", star_data.mass_sol));
                        ui.label(format!("  Radius: {:.2} R☉", star_data.radius_sol));
                        ui.label(format!("  Luminosity: {:.3} L☉", star_data.luminosity_sol));
                        ui.label(format!("  Temperature: {} K", star_data.temp_k));

                        if let Some(metallicity) = star_data.metallicity {
                            let metallicity_color = if metallicity > 0.0 {
                                egui::Color32::from_rgb(255, 220, 100)
                            } else if metallicity < 0.0 {
                                egui::Color32::from_rgb(150, 150, 200)
                            } else {
                                egui::Color32::from_rgb(200, 200, 200)
                            };

                            ui.label(
                                egui::RichText::new(format!(
                                    "  Metallicity: [Fe/H] = {:.2}",
                                    metallicity
                                ))
                                .color(metallicity_color),
                            );
                        }

                        ui.add_space(5.0);
                    }
                });

                ui.add_space(10.0);
            }

            // Count bodies in this system
            let bodies: Vec<_> = bodies_query
                .iter()
                .filter(|(_, _, _, _, sys_id)| sys_id.map(|s| s.0 == star_icon.id).unwrap_or(false))
                .collect();

            ui.group(|ui| {
                ui.label(egui::RichText::new("System Bodies").strong());
                ui.label(format!("Total bodies: {}", bodies.len()));

                // Count by type
                let stars = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Star))
                    .count();
                let planets = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Planet))
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

                if stars > 0 {
                    ui.label(format!("  Stars: {}", stars));
                }
                if planets > 0 {
                    ui.label(format!("  Planets: {}", planets));
                }
                if dwarf_planets > 0 {
                    ui.label(format!("  Dwarf Planets: {}", dwarf_planets));
                }
                if moons > 0 {
                    ui.label(format!("  Moons: {}", moons));
                }
                if asteroids > 0 {
                    ui.label(format!("  Asteroids: {}", asteroids));
                }
                if comets > 0 {
                    ui.label(format!("  Comets: {}", comets));
                }
            });

            ui.add_space(10.0);

            // Calculate total resources
            ui.group(|ui| {
                ui.label(egui::RichText::new("System Resources").strong());

                // Sum up all resources in this system
                let mut total_resources: std::collections::HashMap<ResourceType, f64> =
                    std::collections::HashMap::new();

                for (sys_id, resources) in resource_query.iter() {
                    if sys_id.0 == star_icon.id {
                        for (resource_type, deposit) in &resources.deposits {
                            let total = deposit.total_megatons();
                            *total_resources.entry(*resource_type).or_insert(0.0) += total;
                        }
                    }
                }

                if total_resources.is_empty() {
                    ui.label("No surveyed resources yet");
                } else {
                    ui.label(format!(
                        "Surveyed resource types: {}",
                        total_resources.len()
                    ));

                    // Show top 5 resources by abundance
                    let mut sorted_resources: Vec<_> = total_resources.iter().collect();
                    sorted_resources
                        .sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

                    ui.label(egui::RichText::new("Top resources:").italics());
                    for (resource_type, amount) in sorted_resources.iter().take(5) {
                        ui.label(format!(
                            "  {}: {}",
                            resource_type.display_name(),
                            format_mass(**amount)
                        ));
                    }
                }
            });

            ui.add_space(10.0);

            // Population (placeholder for future)
            ui.group(|ui| {
                ui.label(egui::RichText::new("Population").strong());
                ui.label("Coming soon: Population management");
            });
        });
}

