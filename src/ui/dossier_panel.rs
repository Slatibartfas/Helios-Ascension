//! Planet Dossier Panel — "Tactical OS" style body information display.
//!
//! Replaces the old text-based right `SidePanel` from `dashboard.rs` with a
//! visually rich panel featuring:
//! - Stacked atmosphere composition bar
//! - Habitability radar chart (5 colony-cost axes)
//! - Resource periodic grid with depth-fill tiles
//! - Dark tactical palette (#0A0F1E + #00F2FF accents)

use super::*;
use super::dashboard::format_mass;
use super::resources_bar::format_population;
use super::theme::{
    self, BG, ACCENT, ACCENT_DIM, BORDER, TEXT_DIM, TEXT_VALUE,
    SURFACE, AMBER, RED, GREEN,
};
use crate::astronomy::components::{
    calculate_colony_cost_details, AtmosphericGas, OceanType, SurfaceTemperature,
    OceanProperties,
};
use std::f32::consts::TAU;

// ─── Tactical Palette (re-exports from theme) ───────────────────────────

/// Deep navy background at 85% alpha
const BG_FILL: egui::Color32 = BG;
/// Panel background for resource tiles and sub-sections
const TILE_BG: egui::Color32 = SURFACE;
/// Negative / red accent
const RED_ACCENT: egui::Color32 = RED;
/// Positive / green accent
const GREEN_ACCENT: egui::Color32 = GREEN;

/// Section header font
fn heading_font() -> egui::FontId { theme::heading() }

/// Body name font
fn title_font() -> egui::FontId { theme::title() }

/// Monospace value font
fn mono_font(size: f32) -> egui::FontId { theme::mono(size) }

// ─── Main System ─────────────────────────────────────────────────────────

/// Renders the right-side "Celestial Body Dossier" panel when a body is
/// selected and no full-screen menu is active.
#[allow(clippy::too_many_arguments)]
pub(super) fn ui_planet_dossier(
    mut commands: Commands,
    mut contexts: EguiContexts,
    selection: Res<Selection>,
    active_menu: Res<ActiveMenu>,
    mut body_query: Query<(
        &CelestialBody,
        Option<&SpaceCoordinates>,
        Option<&KeplerOrbit>,
        Option<&PlanetResources>,
        Option<&AtmosphereComposition>,
        Option<&crate::plugins::starmap::PlanetCategory>,
        Option<&mut SurveyLevel>,
        Option<&Population>,
        Option<&SurfaceTemperature>,
        Option<&LogicalParent>,
        Option<&OceanProperties>,
    )>,
    parent_coords_query: Query<&SpaceCoordinates>,
    all_bodies_query: Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&crate::astronomy::components::SystemId>,
    )>,
    star_system_query: Query<(
        Entity,
        &StarSystemIcon,
        Option<&SelectedStarSystem>,
    )>,
) {
    // Don't show when full-screen menus are active
    if matches!(
        active_menu.current,
        GameMenu::Research | GameMenu::Construction | GameMenu::Economy | GameMenu::Fleets
    ) {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // If a star system is selected (starmap view), don't show body dossier
    let has_selected_star = star_system_query.iter().any(|(_, _, sel)| sel.is_some());
    if has_selected_star {
        return;
    }

    if !selection.has_selection() {
        return;
    }

    let entity = match selection.get() {
        Some(e) => e,
        None => return,
    };

    let Ok((
        body,
        opt_coords,
        orbit,
        resources,
        atmosphere,
        category_opt,
        mut survey_level,
        population,
        surface_temp,
        logical_parent,
        ocean_props,
    )) = body_query.get_mut(entity)
    else {
        return;
    };

    let panel_frame = egui::Frame::NONE
        .fill(BG_FILL)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(10));

    egui::SidePanel::right("selection_panel")
        .min_width(340.0)
        .max_width(420.0)
        .frame(panel_frame)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("dossier_scroll")
                .show(ui, |ui| {
                    // ── Header ──────────────────────────────────────
                    draw_dossier_header(
                        ui,
                        body,
                        category_opt,
                        population,
                        opt_coords,
                        logical_parent,
                        &parent_coords_query,
                        &all_bodies_query,
                    );

                    // ── Orbital Elements ────────────────────────────
                    if let Some(orbit) = orbit {
                        draw_orbital_stats(ui, orbit);
                    }

                    // ── Ring body special case ──────────────────────
                    if body.body_type == BodyType::Ring {
                        section_divider(ui);
                        ui.colored_label(
                            AMBER,
                            egui::RichText::new("\u{26A0} ORBITAL MINING ONLY")
                                .font(heading_font()),
                        );
                        ui.add_space(4.0);
                        ui.colored_label(
                            TEXT_DIM,
                            "Free-floating ice and dust. Cannot be colonised.",
                        );
                    } else {
                        // ── Habitability Radar ──────────────────────
                        section_divider(ui);
                        let scores = compute_habitability_scores(
                            body,
                            surface_temp,
                            atmosphere,
                            ocean_props,
                        );
                        draw_habitability_section(
                            ui,
                            entity,
                            body,
                            &scores,
                            surface_temp,
                            atmosphere,
                        );

                        // ── Atmosphere Bar ──────────────────────────
                        if let Some(atmo) = atmosphere {
                            section_divider(ui);
                            draw_atmosphere_section(ui, entity, atmo);
                        }

                        // ── Ocean ───────────────────────────────────
                        if let Some(ocean) = ocean_props {
                            section_divider(ui);
                            draw_ocean_section(ui, ocean);
                        }
                    }

                    // ── Resource Grid ───────────────────────────────
                    if let Some(res) = resources {
                        section_divider(ui);
                        draw_resource_section(
                            ui,
                            entity,
                            res,
                            survey_level.as_deref_mut(),
                            &mut commands,
                        );
                    }
                });
        });
}

// ─── Header ──────────────────────────────────────────────────────────────

fn draw_dossier_header(
    ui: &mut egui::Ui,
    body: &CelestialBody,
    category: Option<&crate::plugins::starmap::PlanetCategory>,
    population: Option<&Population>,
    coords: Option<&SpaceCoordinates>,
    logical_parent: Option<&LogicalParent>,
    parent_coords_query: &Query<&SpaceCoordinates>,
    all_bodies_query: &Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&crate::astronomy::components::SystemId>,
    )>,
) {
    // Body name
    ui.label(
        egui::RichText::new(&body.name)
            .font(title_font())
            .color(ACCENT),
    );

    // Category caption
    if let Some(cat) = category {
        let mut label = cat.0.clone();
        if let Some(first) = label.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        ui.label(egui::RichText::new(label).small().color(TEXT_DIM));
    }

    ui.add_space(6.0);

    // Key stats grid
    egui::Grid::new("header_stats")
        .num_columns(2)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            // Distance from star
            if !matches!(body.body_type, BodyType::Star) {
                if let Some(c) = coords {
                    let star_pos = find_star_position(
                        logical_parent,
                        parent_coords_query,
                        all_bodies_query,
                    );
                    let distance_au = (c.position - star_pos).length();
                    stat_row(ui, "DISTANCE", &format!("{distance_au:.3} AU"));
                }
            }

            stat_row(ui, "RADIUS", &format!("{:.1} km", body.radius));
            stat_row(ui, "MASS", &format!("{:.2e} kg", body.mass));
            stat_row(ui, "GRAVITY", &format!("{:.2} g", body.surface_gravity()));

            if let Some(pop) = population {
                if pop.count > 0.0 {
                    stat_row(ui, "POP", &format_population(pop.count));
                }
            }
        });

    ui.add_space(4.0);
}

/// Walk up the LogicalParent chain to find the system star position.
fn find_star_position(
    logical_parent: Option<&LogicalParent>,
    parent_coords_query: &Query<&SpaceCoordinates>,
    all_bodies_query: &Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&crate::astronomy::components::SystemId>,
    )>,
) -> bevy::math::DVec3 {
    let mut current = logical_parent.map(|lp| lp.0);
    while let Some(parent_entity) = current {
        if let Ok((_, parent_body, grandparent, _, _)) = all_bodies_query.get(parent_entity) {
            if matches!(parent_body.body_type, BodyType::Star) {
                if let Ok(star_coords) = parent_coords_query.get(parent_entity) {
                    return star_coords.position;
                }
                break;
            }
            current = grandparent.map(|gp| gp.0);
        } else {
            break;
        }
    }
    bevy::math::DVec3::ZERO
}

/// Render a dim-label + mono-value row in the stats grid.
fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(mono_font(10.0))
            .color(TEXT_DIM),
    );
    ui.label(
        egui::RichText::new(value)
            .font(mono_font(12.0))
            .color(TEXT_VALUE),
    );
    ui.end_row();
}

/// Thin horizontal tactical divider.
fn section_divider(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter()
        .hline(rect.left()..=rect.right(), y, egui::Stroke::new(1.0, BORDER));
    ui.add_space(8.0);
}

// ─── Orbital Stats ───────────────────────────────────────────────────────

fn draw_orbital_stats(ui: &mut egui::Ui, orbit: &KeplerOrbit) {
    section_divider(ui);
    ui.label(
        egui::RichText::new("ORBITAL ELEMENTS")
            .font(heading_font())
            .color(TEXT_DIM),
    );
    ui.add_space(4.0);

    egui::Grid::new("orbital_stats")
        .num_columns(2)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            stat_row(ui, "SMA", &format!("{:.4} AU", orbit.semi_major_axis));
            stat_row(ui, "ECC", &format!("{:.5}", orbit.eccentricity));
            stat_row(
                ui,
                "INC",
                &format!("{:.2}\u{00B0}", orbit.inclination.to_degrees()),
            );

            let period_s =
                crate::astronomy::KeplerOrbit::period_from_mean_motion(orbit.mean_motion);
            let period_d = period_s / 86400.0;
            if period_d < 365.0 {
                stat_row(ui, "PERIOD", &format!("{period_d:.1} d"));
            } else {
                stat_row(ui, "PERIOD", &format!("{:.2} yr", period_d / 365.25));
            }
        });
}

// ─── Habitability Radar ──────────────────────────────────────────────────

/// Scores: [gravity, temperature, pressure, breathability, hydrosphere] all 0..=1.
fn compute_habitability_scores(
    body: &CelestialBody,
    temp: Option<&SurfaceTemperature>,
    atmo: Option<&AtmosphereComposition>,
    ocean: Option<&OceanProperties>,
) -> [f32; 5] {
    let gravity_g = body.surface_gravity();

    // Gravity: 1.0 at Earth, drops linearly toward 0 and 2g
    let gravity_score = (1.0 - (gravity_g - 1.0).abs().min(2.0) / 2.0).clamp(0.0, 1.0);

    // Temperature: use colony cost details
    let (min_t, max_t) = temp
        .map(|t| (t.min_celsius, t.max_celsius))
        .or_else(|| {
            atmo.map(|a| (a.surface_temperature_celsius, a.surface_temperature_celsius))
        })
        .unwrap_or((-273.15, -273.15));

    let cost = calculate_colony_cost_details(gravity_g, min_t, max_t, atmo);
    let temp_score =
        (1.0 - (cost.cold_cost + cost.heat_cost).min(10.0) / 10.0).clamp(0.0, 1.0);

    // Pressure
    let pressure_score = if atmo.is_none() {
        0.15 // vacuum penalty
    } else {
        (1.0 - cost.pressure_cost.min(4.0) / 4.0).clamp(0.0, 1.0)
    };

    // Breathability
    let breath_score = match atmo {
        Some(a) if a.breathable => 1.0,
        Some(_) => 0.3,
        None => 0.0,
    };

    // Hydrosphere
    let hydro_score = ocean
        .map(|o| {
            if o.is_subsurface {
                0.3 // subsurface oceans contribute partial score
            } else if o.ocean_type == OceanType::Water {
                o.surface_fraction.clamp(0.0, 1.0)
            } else {
                0.15 // exotic liquids
            }
        })
        .unwrap_or(0.0);

    [
        gravity_score,
        temp_score,
        pressure_score,
        breath_score,
        hydro_score,
    ]
}

/// Draw the habitability section: radar chart + colony cost summary.
#[allow(clippy::too_many_arguments)]
fn draw_habitability_section(
    ui: &mut egui::Ui,
    entity: Entity,
    body: &CelestialBody,
    scores: &[f32; 5],
    surface_temp: Option<&SurfaceTemperature>,
    atmosphere: Option<&AtmosphereComposition>,
) {
    ui.label(
        egui::RichText::new("HABITABILITY")
            .font(heading_font())
            .color(TEXT_DIM),
    );
    ui.add_space(4.0);

    // Radar chart
    draw_radar_chart(ui, scores);

    ui.add_space(6.0);

    // Colony cost summary
    let gravity_g = body.surface_gravity();
    let (min_t, max_t) = surface_temp
        .map(|t| (t.min_celsius, t.max_celsius))
        .or_else(|| {
            atmosphere.map(|a| (a.surface_temperature_celsius, a.surface_temperature_celsius))
        })
        .unwrap_or((-273.15, -273.15));

    let details = calculate_colony_cost_details(gravity_g, min_t, max_t, atmosphere);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("COLONY COST")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        let (color, text) = if details.heavy_gravity_limit_exceeded {
            (RED_ACCENT, "UNINHABITABLE".to_string())
        } else if details.total_cost <= 0.0 {
            (GREEN_ACCENT, "0.00 \u{2014} IDEAL".to_string())
        } else if details.total_cost <= 2.0 {
            (
                egui::Color32::from_rgb(200, 200, 50),
                format!("{:.2}", details.total_cost),
            )
        } else if details.total_cost <= 5.0 {
            (AMBER, format!("{:.2}", details.total_cost))
        } else {
            (RED_ACCENT, format!("{:.2}", details.total_cost))
        };
        ui.label(
            egui::RichText::new(text)
                .font(mono_font(13.0))
                .color(color),
        );
    });

    // Temperature line
    if let Some(temp) = surface_temp {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("TEMP")
                    .font(mono_font(10.0))
                    .color(TEXT_DIM),
            );
            let temp_color = if temp.average_celsius > -10.0 && temp.average_celsius < 45.0 {
                GREEN_ACCENT
            } else if temp.average_celsius > -60.0 && temp.average_celsius < 80.0 {
                AMBER
            } else {
                RED_ACCENT
            };
            ui.label(
                egui::RichText::new(format!(
                    "{:.0}\u{00B0}C  (min {:.0} / max {:.0})",
                    temp.average_celsius, temp.min_celsius, temp.max_celsius
                ))
                .font(mono_font(11.0))
                .color(temp_color),
            );
        });
    }

    // Colony cost breakdown (collapsible)
    let cost_factors_id = ui.make_persistent_id(("cost_factors", entity));
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        cost_factors_id,
        false,
    )
    .show_header(ui, |ui| {
        ui.label(
            egui::RichText::new("Cost Breakdown")
                .small()
                .color(TEXT_DIM),
        );
    })
    .body(|ui| {
        if details.heavy_gravity_limit_exceeded {
            ui.colored_label(RED_ACCENT, "Gravity exceeds 1.7g \u{2014} uninhabitable");
        } else {
            if details.base_cost > 0.0 {
                ui.colored_label(
                    AMBER,
                    format!("  Unbreathable +{:.2}", details.base_cost),
                );
            }
            if details.cold_cost > 0.0 {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 180, 255),
                    format!("  Cold penalty +{:.2}", details.cold_cost),
                );
            }
            if details.heat_cost > 0.0 {
                ui.colored_label(
                    RED_ACCENT,
                    format!("  Heat penalty +{:.2}", details.heat_cost),
                );
            }
            if details.pressure_cost > 0.0 {
                ui.colored_label(
                    AMBER,
                    format!("  Pressure +{:.2}", details.pressure_cost),
                );
            }
            if details.low_gravity_penalty > 0.0 {
                ui.colored_label(
                    AMBER,
                    format!("  Low gravity +{:.2}", details.low_gravity_penalty),
                );
            }
        }
    });
}

/// 5-point radar / pentagon chart rendered with `egui::Painter`.
fn draw_radar_chart(ui: &mut egui::Ui, scores: &[f32; 5]) {
    const LABELS: [&str; 5] = ["Gravity", "Temp", "Pressure", "Air", "Water"];
    const SIZE: f32 = 170.0;
    const MAX_R: f32 = 65.0;
    const LABEL_R: f32 = 80.0;

    let (response, painter) =
        ui.allocate_painter(egui::Vec2::splat(SIZE), egui::Sense::hover());
    let center = response.rect.center();

    // Axis angles — start top (-PI/2), go clockwise
    let angles: Vec<f32> = (0..5)
        .map(|i| -std::f32::consts::FRAC_PI_2 + (i as f32) * TAU / 5.0)
        .collect();

    // Reference rings at 33%, 66%, 100%
    for &frac in &[0.33_f32, 0.66, 1.0] {
        let r = MAX_R * frac;
        let ring_points: Vec<egui::Pos2> = angles
            .iter()
            .map(|&a| center + egui::Vec2::new(a.cos() * r, a.sin() * r))
            .collect();
        painter.add(egui::Shape::closed_line(
            ring_points,
            egui::Stroke::new(0.5, BORDER),
        ));
    }

    // Axis lines
    for &a in &angles {
        let tip = center + egui::Vec2::new(a.cos() * MAX_R, a.sin() * MAX_R);
        painter.line_segment([center, tip], egui::Stroke::new(0.5, BORDER));
    }

    // Earth reference pentagon (all 1.0) — thin cyan outline
    let earth_pts: Vec<egui::Pos2> = angles
        .iter()
        .map(|&a| center + egui::Vec2::new(a.cos() * MAX_R, a.sin() * MAX_R))
        .collect();
    painter.add(egui::Shape::closed_line(
        earth_pts,
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_premultiplied(0, 242, 255, 60),
        ),
    ));

    // Player polygon
    let player_pts: Vec<egui::Pos2> = scores
        .iter()
        .zip(angles.iter())
        .map(|(&s, &a)| {
            let r = MAX_R * s.clamp(0.0, 1.0);
            center + egui::Vec2::new(a.cos() * r, a.sin() * r)
        })
        .collect();

    // Filled polygon
    painter.add(egui::Shape::convex_polygon(
        player_pts.clone(),
        egui::Color32::from_rgba_premultiplied(0, 242, 255, 40),
        egui::Stroke::new(1.5, ACCENT),
    ));

    // Vertex dots + score labels
    for (i, (&s, &a)) in scores.iter().zip(angles.iter()).enumerate() {
        let pt = player_pts[i];
        painter.circle_filled(pt, 3.0, ACCENT);

        // Score text near vertex (avoid overlap with label)
        if s > 0.05 && s < 0.88 {
            let score_text = format!("{:.0}%", s * 100.0);
            let score_pos = center
                + egui::Vec2::new(
                    a.cos() * (MAX_R * s.clamp(0.0, 1.0) + 10.0),
                    a.sin() * (MAX_R * s.clamp(0.0, 1.0) + 10.0),
                );
            painter.text(
                score_pos,
                egui::Align2::CENTER_CENTER,
                &score_text,
                mono_font(8.0),
                ACCENT_DIM,
            );
        }

        // Axis label
        let label_pos = center + egui::Vec2::new(a.cos() * LABEL_R, a.sin() * LABEL_R);
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            LABELS[i],
            mono_font(9.0),
            TEXT_DIM,
        );
    }
}

// ─── Atmosphere Bar ──────────────────────────────────────────────────────

/// Gas name -> colour mapping.
fn gas_color(name: &str) -> egui::Color32 {
    let lower = name.to_lowercase();
    if lower.starts_with("n2") || lower.starts_with("nitrogen") {
        egui::Color32::from_rgb(26, 82, 118) // Deep blue
    } else if lower.starts_with("o2") || lower.starts_with("oxygen") {
        egui::Color32::from_rgb(0, 200, 220) // Cyan
    } else if lower.starts_with("co2") || lower.starts_with("carbon d") {
        egui::Color32::from_rgb(230, 126, 34) // Amber
    } else if lower.starts_with("ar") || lower.starts_with("argon") {
        egui::Color32::from_rgb(86, 101, 115) // Slate
    } else if lower.starts_with("ch4") || lower.starts_with("methane") {
        egui::Color32::from_rgb(243, 156, 18) // Gold
    } else if lower.starts_with("h2") || lower.starts_with("hydrogen") {
        egui::Color32::from_rgb(174, 214, 241) // Pale blue
    } else if lower.starts_with("he") || lower.starts_with("helium") {
        egui::Color32::from_rgb(213, 245, 227) // Light mint
    } else if lower.starts_with("so2") || lower.starts_with("sulfur") {
        egui::Color32::from_rgb(180, 160, 30) // Olive yellow
    } else if lower.starts_with("ne") || lower.starts_with("neon") {
        egui::Color32::from_rgb(255, 100, 100) // Neon red
    } else {
        egui::Color32::from_rgb(80, 80, 100) // Generic grey-blue
    }
}

fn draw_atmosphere_section(
    ui: &mut egui::Ui,
    entity: Entity,
    atmo: &AtmosphereComposition,
) {
    ui.label(
        egui::RichText::new("ATMOSPHERE")
            .font(heading_font())
            .color(TEXT_DIM),
    );
    ui.add_space(4.0);

    // Pressure + breathability header row
    ui.horizontal(|ui| {
        let pressure_bar = atmo.surface_pressure_mbar / 1000.0;
        let pressure_label = if atmo.is_reference_pressure {
            "REF"
        } else {
            "SRF"
        };
        let pressure_text = if pressure_bar >= 1.0 {
            format!("{pressure_bar:.2} bar")
        } else {
            format!("{:.0} mbar", atmo.surface_pressure_mbar)
        };
        ui.label(
            egui::RichText::new(format!("{pressure_label} {pressure_text}"))
                .font(mono_font(11.0))
                .color(TEXT_VALUE),
        );

        ui.add_space(8.0);

        // Breathability indicator
        let (breath_icon, breath_color, breath_tip) = if atmo.breathable {
            (
                "\u{25CF}  BREATHABLE",
                ACCENT,
                "O\u{2082} levels and pressure are safe for humans",
            )
        } else {
            let has_some_o2 = atmo.gases.iter().any(|g| {
                (g.name.starts_with("O2") || g.name.starts_with("Oxygen")) && g.percentage > 1.0
            });
            if has_some_o2 {
                (
                    "\u{25D0}  MARGINAL",
                    AMBER,
                    "Some oxygen present but not in safe range",
                )
            } else {
                ("\u{25CB}  UNBREATHABLE", RED_ACCENT, "No usable oxygen")
            }
        };
        ui.label(
            egui::RichText::new(breath_icon)
                .font(mono_font(10.0))
                .color(breath_color),
        )
        .on_hover_text(breath_tip);
    });

    // Gas giant harvest info
    if atmo.is_reference_pressure && atmo.harvest_altitude_bar > 0.0 {
        let yield_mult = atmo.harvest_yield_multiplier();
        ui.label(
            egui::RichText::new(format!(
                "HARVEST {:.1} bar ({yield_mult:.1}\u{00D7} yield)  MAX {:.0} bar",
                atmo.harvest_altitude_bar, atmo.max_harvest_altitude_bar
            ))
            .font(mono_font(9.0))
            .color(TEXT_DIM),
        );
    }

    ui.add_space(4.0);

    // ── Stacked horizontal bar ──
    draw_atmosphere_bar(ui, &atmo.gases);

    // ── Composition detail (collapsible) ──
    let gas_detail_id = ui.make_persistent_id(("gas_detail", entity));
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        gas_detail_id,
        false,
    )
    .show_header(ui, |ui| {
        ui.label(
            egui::RichText::new("Composition Detail")
                .small()
                .color(TEXT_DIM),
        );
    })
    .body(|ui| {
        for gas in &atmo.gases {
            ui.horizontal(|ui| {
                let color = gas_color(&gas.name);
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::new(10.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);
                ui.label(
                    egui::RichText::new(format!("{}: {:.2}%", gas.name, gas.percentage))
                        .font(mono_font(10.0))
                        .color(TEXT_VALUE),
                );
            });
        }
    });
}

/// Stacked horizontal bar: each gas gets a proportional segment with its colour.
fn draw_atmosphere_bar(ui: &mut egui::Ui, gases: &[AtmosphericGas]) {
    let bar_height = 18.0;
    let available_width = ui.available_width().max(100.0);

    let (bar_response, painter) = ui.allocate_painter(
        egui::Vec2::new(available_width, bar_height),
        egui::Sense::hover(),
    );
    let bar_rect = bar_response.rect;

    // Background
    painter.rect_filled(bar_rect, 3.0, TILE_BG);

    // Total percentage (should be ~100)
    let total: f32 = gases.iter().map(|g| g.percentage).sum();
    if total <= 0.0 {
        return;
    }

    let mut x = bar_rect.left();
    let mut segment_rects: Vec<(egui::Rect, String)> = Vec::new();

    for gas in gases {
        let frac = gas.percentage / total;
        let w = (available_width * frac).max(1.0);
        let seg_rect = egui::Rect::from_min_size(
            egui::Pos2::new(x, bar_rect.top()),
            egui::Vec2::new(w, bar_height),
        )
        .intersect(bar_rect);

        let color = gas_color(&gas.name);
        painter.rect_filled(seg_rect, 0.0, color);

        // Label inside bar if segment is wide enough
        if w > 28.0 {
            let label_text = if w > 55.0 {
                format!("{} {:.1}%", &gas.name, gas.percentage)
            } else {
                gas.name.clone()
            };
            painter.text(
                seg_rect.center(),
                egui::Align2::CENTER_CENTER,
                label_text,
                mono_font(9.0),
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 200),
            );
        }

        segment_rects.push((seg_rect, gas.name.clone()));
        x += w;
    }

    // Hover tooltips for bar segments
    if bar_response.hovered() {
        if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
            for (rect, name) in &segment_rects {
                if rect.contains(pointer) {
                    if let Some(gas) = gases.iter().find(|g| &g.name == name) {
                        bar_response.clone().on_hover_text(format!(
                            "{}: {:.3}%",
                            gas.name, gas.percentage
                        ));
                    }
                    break;
                }
            }
        }
    }

    // Rounded border overlay
    painter.rect_stroke(bar_rect, 3.0, egui::Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);
}

// ─── Ocean Section ───────────────────────────────────────────────────────

fn draw_ocean_section(ui: &mut egui::Ui, ocean: &OceanProperties) {
    let (icon, label, color) = if ocean.is_subsurface {
        (
            "\u{25C8}",
            "SUBSURFACE OCEAN",
            egui::Color32::from_rgb(100, 180, 220),
        )
    } else {
        match ocean.ocean_type {
            OceanType::Water => (
                "\u{25C9}",
                "SURFACE OCEAN (WATER)",
                egui::Color32::from_rgb(64, 164, 223),
            ),
            OceanType::Methane => (
                "\u{25C9}",
                "METHANE LAKES",
                egui::Color32::from_rgb(180, 140, 60),
            ),
            OceanType::Hydrocarbon => (
                "\u{25C9}",
                "HYDROCARBON LAKES",
                egui::Color32::from_rgb(180, 140, 60),
            ),
            OceanType::Ammonia => (
                "\u{25C9}",
                "AMMONIA OCEAN",
                egui::Color32::from_rgb(160, 120, 200),
            ),
            OceanType::Subsurface => (
                "\u{25C8}",
                "SUBSURFACE OCEAN",
                egui::Color32::from_rgb(100, 180, 220),
            ),
        }
    };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).color(color));
        ui.label(
            egui::RichText::new(label)
                .font(heading_font())
                .color(color),
        );
    });

    ui.add_space(4.0);

    egui::Grid::new("ocean_stats")
        .num_columns(2)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            if ocean.is_subsurface {
                stat_row(ui, "LOCATION", "Beneath ice crust");
            } else {
                stat_row(
                    ui,
                    "COVERAGE",
                    &format!("{:.0}%", ocean.surface_fraction * 100.0),
                );
            }

            let depth_text = if ocean.mean_depth_km >= 1.0 {
                format!("{:.1} km", ocean.mean_depth_km)
            } else {
                format!("{:.0} m", ocean.mean_depth_km * 1000.0)
            };
            stat_row(ui, "DEPTH", &depth_text);
        });

    // Habitability modifier
    let hab = ocean.habitability_modifier();
    let (hab_color, hab_text) = if hab > 1.2 {
        (
            GREEN_ACCENT,
            format!("+{:.0}% growth", (hab - 1.0) * 100.0),
        )
    } else if hab > 1.0 {
        (
            egui::Color32::from_rgb(100, 220, 100),
            format!("+{:.0}% growth", (hab - 1.0) * 100.0),
        )
    } else if hab < 1.0 {
        (
            AMBER,
            format!("-{:.0}% penalty", (1.0 - hab) * 100.0),
        )
    } else {
        (TEXT_DIM, "Neutral".to_string())
    };
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("GROWTH")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(hab_text)
                .font(mono_font(11.0))
                .color(hab_color),
        );
    });
}

// ─── Resource Grid ───────────────────────────────────────────────────────

fn draw_resource_section(
    ui: &mut egui::Ui,
    entity: Entity,
    resources: &PlanetResources,
    survey_level: Option<&mut SurveyLevel>,
    commands: &mut Commands,
) {
    ui.label(
        egui::RichText::new("RESOURCES")
            .font(heading_font())
            .color(TEXT_DIM),
    );
    ui.add_space(2.0);

    // Survey status
    let current_level = survey_level
        .as_deref()
        .copied()
        .unwrap_or(SurveyLevel::Unsurveyed);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SURVEY")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        let (level_color, level_text) = match current_level {
            SurveyLevel::Unsurveyed => (TEXT_DIM, "UNSURVEYED"),
            SurveyLevel::OrbitalScan => (egui::Color32::LIGHT_BLUE, "ORBITAL"),
            SurveyLevel::SeismicSurvey => (egui::Color32::YELLOW, "SEISMIC"),
            SurveyLevel::CoreSample => (GREEN_ACCENT, "CORE SAMPLE"),
        };
        ui.label(
            egui::RichText::new(level_text)
                .font(mono_font(11.0))
                .color(level_color),
        );

        // Upgrade button
        if let Some(survey) = survey_level {
            if *survey != SurveyLevel::CoreSample {
                if ui
                    .small_button(
                        egui::RichText::new("\u{25B2} UPGRADE")
                            .font(mono_font(9.0))
                            .color(ACCENT),
                    )
                    .clicked()
                {
                    *survey = match *survey {
                        SurveyLevel::Unsurveyed => SurveyLevel::OrbitalScan,
                        SurveyLevel::OrbitalScan => SurveyLevel::SeismicSurvey,
                        SurveyLevel::SeismicSurvey => SurveyLevel::CoreSample,
                        _ => SurveyLevel::CoreSample,
                    };
                }
            }
        } else if ui
            .small_button(
                egui::RichText::new("\u{25CE} INIT SCAN")
                    .font(mono_font(9.0))
                    .color(ACCENT),
            )
            .clicked()
        {
            commands.entity(entity).insert(SurveyLevel::OrbitalScan);
        }
    });

    ui.add_space(6.0);

    if current_level == SurveyLevel::Unsurveyed {
        ui.colored_label(TEXT_DIM, "Perform orbital scan to detect resources.");
        return;
    }

    // Resource periodic grid
    draw_resource_grid(ui, resources, current_level);

    // Summary line
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!(
            "VIABLE {}  \u{2502}  VALUE {:.1}",
            resources.viable_count(),
            resources.total_value()
        ))
        .font(mono_font(9.0))
        .color(TEXT_DIM),
    );
}

/// Resource periodic grid: tiles laid out by category, each showing a chemical
/// symbol with a fill level representing concentration.
fn draw_resource_grid(ui: &mut egui::Ui, resources: &PlanetResources, survey_level: SurveyLevel) {
    let tile_size = 42.0_f32;
    let tile_spacing = 3.0_f32;

    for (category_name, category_resources) in ResourceType::by_category() {
        // Category header
        ui.label(
            egui::RichText::new(category_name)
                .font(mono_font(9.0))
                .color(ACCENT_DIM),
        );

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(tile_spacing);

            for resource_type in &category_resources {
                let deposit = resources.get_deposit(resource_type);
                draw_resource_tile(ui, *resource_type, deposit, survey_level, tile_size);
            }
        });

        ui.add_space(2.0);
    }
}

/// Single resource tile: square with symbol, fill-level background, and hover tooltip.
fn draw_resource_tile(
    ui: &mut egui::Ui,
    resource: ResourceType,
    deposit: Option<&MineralDeposit>,
    survey_level: SurveyLevel,
    size: f32,
) {
    let (response, painter) =
        ui.allocate_painter(egui::Vec2::splat(size), egui::Sense::hover());
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 3.0, TILE_BG);

    let has_deposit = deposit.is_some_and(|d| d.reserve.total_mass() > 0.001);

    if has_deposit {
        let d = deposit.unwrap();

        // Fill level: blend of log-scaled concentration and log-scaled mass.
        // This ensures both ore quality AND deposit size contribute, so a
        // huge low-concentration deposit is still visually distinct from a
        // tiny high-concentration one.

        // Concentration axis:  1e-10 → 0.0  …  1.0 → 1.0
        let conc = d.reserve.concentration.clamp(1e-10, 1.0) as f64;
        let conc_norm = (conc.log10() + 10.0) / 10.0; // 0..1

        // Mass axis (Mt):  0.001 Mt (1 kt) → 0.0  …  1e6 Mt (1 Tt) → 1.0
        let mass = d.reserve.total_mass().clamp(0.001, 1e6);
        let mass_norm = (mass.log10() + 3.0) / 9.0; // log10(0.001)=-3, log10(1e6)=6 → 9 decades

        // Weighted blend: 40 % concentration, 60 % mass
        let fill_frac = (0.4 * conc_norm + 0.6 * mass_norm).clamp(0.05, 1.0) as f32;
        let fill_height = rect.height() * fill_frac;
        let fill_rect = egui::Rect::from_min_max(
            egui::Pos2::new(rect.left(), rect.bottom() - fill_height),
            rect.max,
        );

        // Fill colour: blend from dim to accent based on combined fill
        let fill_alpha = (60.0 + 140.0 * fill_frac) as u8;
        let fill_color = egui::Color32::from_rgba_premultiplied(
            0,
            (180.0 * fill_frac + 40.0) as u8,
            (200.0 * fill_frac + 50.0) as u8,
            fill_alpha,
        );
        painter.rect_filled(fill_rect, 0.0, fill_color);

        // Proven reserve mini-bar at bottom (3px)
        let proven = d.reserve.proven_crustal;
        let total = d.reserve.total_mass();
        if total > 0.0 {
            let bar_frac = (proven / total).clamp(0.0, 1.0) as f32;
            let bar_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 1.0, rect.bottom() - 3.0),
                egui::Vec2::new((rect.width() - 2.0) * bar_frac, 2.0),
            );
            painter.rect_filled(bar_rect, 0.0, ACCENT);
        }

        // Symbol text
        painter.text(
            rect.center() - egui::Vec2::new(0.0, 1.0),
            egui::Align2::CENTER_CENTER,
            resource.symbol(),
            mono_font(11.0),
            egui::Color32::WHITE,
        );
    } else {
        // No deposit — dim symbol
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            resource.symbol(),
            mono_font(10.0),
            egui::Color32::from_rgb(50, 55, 65),
        );
    }

    // Border
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);

    // Hover tooltip
    if response.hovered() && has_deposit {
        let d = deposit.unwrap();
        let discovered = survey_level.discovered_amount(&d.reserve);
        response.clone().on_hover_ui(|ui: &mut egui::Ui| {
            ui.set_min_width(180.0);
            let tip_frame = egui::Frame::NONE
                .fill(BG_FILL)
                .stroke(egui::Stroke::new(1.0, ACCENT_DIM))
                .inner_margin(egui::Margin::same(8));

            tip_frame.show(ui, |ui| {
                ui.label(
                    egui::RichText::new(resource.display_name())
                        .font(mono_font(13.0))
                        .color(ACCENT),
                );

                // Phase badge
                ui.label(
                    egui::RichText::new(format!("{}", d.phase))
                        .font(mono_font(9.0))
                        .color(TEXT_DIM),
                );
                ui.add_space(4.0);

                egui::Grid::new(format!("tooltip_{}", resource.symbol()))
                    .num_columns(2)
                    .spacing([8.0, 1.0])
                    .show(ui, |ui| {
                        tooltip_row(ui, "Discovered", &format_mass(discovered));

                        // Tiered breakdown labels
                        let is_atm = d.is_atmospheric;
                        let proven_label = if is_atm { "Atmospheric" } else { "Proven" };
                        let deep_label = if is_atm { "Trapped" } else { "Deep" };
                        let bulk_label = if is_atm { "Bound" } else { "Bulk" };

                        tooltip_row(
                            ui,
                            proven_label,
                            &format_mass(d.reserve.proven_crustal),
                        );

                        if matches!(
                            survey_level,
                            SurveyLevel::SeismicSurvey | SurveyLevel::CoreSample
                        ) {
                            tooltip_row(
                                ui,
                                deep_label,
                                &format_mass(d.reserve.deep_deposits),
                            );
                        }

                        if matches!(survey_level, SurveyLevel::CoreSample) {
                            tooltip_row(
                                ui,
                                bulk_label,
                                &format_mass(d.reserve.planetary_bulk),
                            );
                        }

                        if !is_atm {
                            let conc = d.reserve.concentration;
                            let conc_text = if conc >= 0.01 {
                                format!("{:.1}%", conc * 100.0)
                            } else if conc >= 0.000_01 {
                                format!("{:.1} ppm", conc * 1_000_000.0)
                            } else if conc >= 0.000_000_01 {
                                format!("{:.2} ppb", conc * 1_000_000_000.0)
                            } else {
                                format!("{conc:.2e}")
                            };
                            tooltip_row(ui, "Conc.", &conc_text);
                        }

                        tooltip_row(
                            ui,
                            "Access",
                            &format!("{:.0}%", d.accessibility * 100.0),
                        );
                    });
            });
        });
    }
}

/// Helper for tooltip grid rows.
fn tooltip_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(mono_font(9.0))
            .color(TEXT_DIM),
    );
    ui.label(
        egui::RichText::new(value)
            .font(mono_font(10.0))
            .color(TEXT_VALUE),
    );
    ui.end_row();
}
