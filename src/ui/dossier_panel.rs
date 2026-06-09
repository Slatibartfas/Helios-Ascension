//! Planet Dossier Panel — "Tactical OS" style body information display.
//!
//! Replaces the old text-based right `SidePanel` from `dashboard.rs` with a
//! visually rich panel featuring:
//! - Stacked atmosphere composition bar
//! - Habitability radar chart (5 colony-cost axes)
//! - Resource periodic grid with depth-fill tiles
//! - Dark tactical palette (#0A0F1E + #00F2FF accents)

use super::dashboard::{format_mass, format_mass_compact, format_rate_monthly};
use super::resources_bar::format_population;
use super::theme::{
    self, ACCENT, ACCENT_DIM, AMBER, BG, BORDER, GREEN, RED, SURFACE, TEXT_DIM, TEXT_VALUE,
};
use super::*;
use crate::astronomy::components::{
    AtmosphericGas, OceanProperties, OceanType, StellarProperties, SurfaceTemperature,
};
use crate::astronomy::nearby_stars::NearbyStarsData;
use crate::economy::components::{SpectralClass, StarSystem};
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};
use std::f32::consts::TAU;

/// Format asteroid class for display with description
fn format_asteroid_class(class: AsteroidClass) -> String {
    match class {
        AsteroidClass::SType => "S-Type Asteroid (Silicaceous - rocky, metal-rich)".to_string(),
        AsteroidClass::CType => "C-Type Asteroid (Carbonaceous - dark, volatile-rich)".to_string(),
        AsteroidClass::MType => "M-Type Asteroid (Metallic - iron-nickel core)".to_string(),
        AsteroidClass::VType => "V-Type Asteroid (Vestoid - basaltic crust)".to_string(),
        AsteroidClass::DType => "D-Type Asteroid (Dark - outer belt, organic-rich)".to_string(),
        AsteroidClass::PType => "P-Type Asteroid (Primitive - icy, carbon-rich)".to_string(),
        AsteroidClass::Unknown => "Asteroid (Unknown type)".to_string(),
    }
}

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
fn heading_font() -> egui::FontId {
    theme::heading()
}

/// Body name font
fn title_font() -> egui::FontId {
    theme::title()
}

/// Monospace value font
fn mono_font(size: f32) -> egui::FontId {
    theme::mono(size)
}

// ─── Main System ─────────────────────────────────────────────────────────

/// Renders the right-side "Celestial Body Dossier" panel when a body is
/// selected and no full-screen menu is active.
#[allow(clippy::too_many_arguments)]
pub(super) fn ui_planet_dossier(
    mut commands: Commands,
    mut contexts: EguiContexts,
    selection: Res<Selection>,
    active_menu: Res<ActiveMenu>,
    nearby_stars: Res<NearbyStarsData>,
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
        Option<&StellarProperties>,
        Option<&crate::astronomy::components::SystemId>,
        Option<&StarSystem>,
        Option<&LogicalParent>,
        Option<&OceanProperties>,
        Option<&Colony>,
    )>,
    parent_coords_query: Query<&SpaceCoordinates>,
    all_bodies_query: Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&crate::astronomy::components::SystemId>,
    )>,
    star_system_query: Query<(Entity, &StarSystemIcon, Option<&SelectedStarSystem>)>,
    rate_tracker: Res<ResourceRateTracker>,
    mut pending_actions: ResMut<PendingConstructionActions>,
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
        stellar_props,
        system_id,
        star_system,
        logical_parent,
        ocean_props,
        existing_colony,
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
                        orbit,
                        population,
                        opt_coords,
                        logical_parent,
                        &parent_coords_query,
                        &all_bodies_query,
                    );

                    if body.body_type == BodyType::Star {
                        section_divider(ui);
                        draw_star_properties_section(
                            ui,
                            body,
                            stellar_props,
                            system_id,
                            star_system,
                            &nearby_stars,
                        );
                        return;
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
                            ocean_props,
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
                            &rate_tracker,
                        );
                    }

                    // ── Outpost / Colony ───────────────────────────
                    // Only show for colonisable body types (not stars, rings, gas giants)
                    let can_colonise = !matches!(
                        body.body_type,
                        BodyType::Star | BodyType::Ring | BodyType::GasGiant
                    );
                    if can_colonise {
                        section_divider(ui);
                        draw_colony_section(
                            ui,
                            entity,
                            body,
                            existing_colony,
                            atmosphere,
                            surface_temp,
                            ocean_props,
                            &mut pending_actions,
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
    orbit: Option<&KeplerOrbit>,
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

    // Category caption - with special formatting for asteroid spectral types
    if let Some(cat) = category {
        let label = if body.body_type == BodyType::Asteroid {
            // Show detailed asteroid type from the body's asteroid_class field
            if let Some(class) = body.asteroid_class {
                format_asteroid_class(class)
            } else {
                // Fallback to category string
                title_case_words(&cat.0)
            }
        } else {
            title_case_words(&cat.0)
        };
        ui.label(egui::RichText::new(label).small().color(TEXT_DIM));
    }

    ui.add_space(6.0);

    ui.horizontal_top(|ui| {
        // Left: core physical stats
        egui::Grid::new("header_stats_physical")
            .num_columns(2)
            .spacing([16.0, 2.0])
            .show(ui, |ui| {
                // Distance: for moons show orbital distance to parent; for others show distance to star
                if !matches!(body.body_type, BodyType::Star) {
                    if body.body_type == BodyType::Moon {
                        // Use the KeplerOrbit semi-major axis which is always the orbital
                        // distance from the parent body, regardless of coordinate system.
                        if let Some(orb) = orbit {
                            const AU_KM: f64 = 149_597_870.7;
                            let sma_au = orb.semi_major_axis;
                            let (value_str, tooltip) = if sma_au < 0.01 {
                                (
                                    format!("{:.0} km", sma_au * AU_KM),
                                    "Semi-major axis: average orbital distance from the parent body.",
                                )
                            } else {
                                (
                                    format!("{sma_au:.4} AU"),
                                    "Semi-major axis: average orbital distance from the parent body.",
                                )
                            };
                            stat_row_with_tooltip(ui, "DISTANCE", &value_str, tooltip);
                        }
                    } else if let Some(c) = coords {
                        let star_pos = find_star_position(
                            logical_parent,
                            parent_coords_query,
                            all_bodies_query,
                        );
                        let distance_au = (c.position - star_pos).length();
                        stat_row_with_tooltip(
                            ui,
                            "DISTANCE",
                            &format!("{distance_au:.3} AU"),
                            "Current distance from the system's primary star.",
                        );
                    }
                }

                stat_row_with_tooltip(
                    ui,
                    "RADIUS",
                    &format!("{:.1} km", body.radius),
                    "Mean body radius in kilometers.",
                );
                stat_row_with_tooltip(
                    ui,
                    "MASS",
                    &format!("{:.2e} kg", body.mass),
                    "Total mass of the body in kilograms.",
                );
                stat_row_with_tooltip(
                    ui,
                    "GRAVITY",
                    &format!("{:.2} g", body.surface_gravity()),
                    "Surface gravity relative to Earth gravity (1.0 g).",
                );

                if let Some(pop) = population {
                    if pop.count > 0.0 {
                        stat_row_with_tooltip(
                            ui,
                            "POP",
                            &format_population(pop.count),
                            "Estimated resident population.",
                        );
                    }
                }
            });

        // Right: orbital elements
        if let Some(orbit) = orbit {
            ui.add_space(10.0);
            // Draw a fixed-height vertical divider instead of ui.separator(),
            // which would expand to fill all available panel height.
            let (_, divider_rect) = ui.allocate_space(egui::Vec2::new(1.0, 60.0));
            ui.painter().vline(
                divider_rect.center().x,
                divider_rect.y_range(),
                egui::Stroke::new(1.0, BORDER),
            );
            ui.add_space(10.0);

            ui.vertical(|ui| {
                egui::Grid::new("header_stats_orbital")
                    .num_columns(2)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        stat_row_with_tooltip(
                            ui,
                            "SMA",
                            &format!("{:.4} AU", orbit.semi_major_axis),
                            "Semi-major axis: average orbital distance from the parent body.",
                        );
                        stat_row_with_tooltip(
                            ui,
                            "ECC",
                            &format!("{:.5}", orbit.eccentricity),
                            "Eccentricity: 0 is circular; higher values are more elongated.",
                        );
                        stat_row_with_tooltip(
                            ui,
                            "INC",
                            &format!("{:.2}\u{00B0}", orbit.inclination.to_degrees()),
                            "Inclination: orbital tilt relative to the reference plane.",
                        );

                        let period_s = crate::astronomy::KeplerOrbit::period_from_mean_motion(
                            orbit.mean_motion,
                        );
                        let period_d = period_s / 86400.0;
                        if period_d < 365.0 {
                            stat_row_with_tooltip(
                                ui,
                                "PERIOD",
                                &format!("{period_d:.1} d"),
                                "Time required to complete one full orbit.",
                            );
                        } else {
                            stat_row_with_tooltip(
                                ui,
                                "PERIOD",
                                &format!("{:.2} yr", period_d / 365.25),
                                "Time required to complete one full orbit.",
                            );
                        }
                    });
            });
        }
    });

    ui.add_space(4.0);
}

fn title_case_words(value: &str) -> String {
    value
        .split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    chars.as_str().to_ascii_lowercase()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

fn stat_row_with_tooltip(ui: &mut egui::Ui, label: &str, value: &str, tooltip: &str) {
    let label_response = ui.label(
        egui::RichText::new(label)
            .font(mono_font(10.0))
            .color(TEXT_DIM),
    );
    label_response.on_hover_text(tooltip);
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
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        egui::Stroke::new(1.0, BORDER),
    );
    ui.add_space(8.0);
}

fn draw_star_properties_section(
    ui: &mut egui::Ui,
    body: &CelestialBody,
    stellar_props: Option<&StellarProperties>,
    system_id: Option<&crate::astronomy::components::SystemId>,
    star_system: Option<&StarSystem>,
    nearby_stars: &NearbyStarsData,
) {
    let star_data = system_id
        .and_then(|id| nearby_stars.get_by_id(id.0))
        .and_then(|system| system.stars.iter().find(|star| star.name == body.name));

    let luminosity_sol = star_data
        .map(|star| star.luminosity_sol)
        .or_else(|| stellar_props.map(|props| props.luminosity_sol))
        .unwrap_or(1.0);
    let temperature_kelvin = star_data
        .map(|star| star.temp_k)
        .or_else(|| stellar_props.map(|props| props.temperature_kelvin))
        .unwrap_or(5778.0);
    let mass_sol = star_data
        .map(|star| star.mass_sol)
        .unwrap_or((body.mass / 1.989e30) as f32);
    let radius_sol = star_data
        .map(|star| star.radius_sol)
        .unwrap_or(body.radius / 695_700.0);
    let metallicity = star_data
        .and_then(|star| star.metallicity)
        .or_else(|| star_system.map(|system| system.metallicity));
    let spectral_type = star_data
        .map(|star| star.spectral_type.clone())
        .unwrap_or_else(|| fallback_spectral_type(body, star_system));
    let frost_line_au = star_system
        .map(|system| system.frost_line_au)
        .unwrap_or_else(|| 4.85 * (luminosity_sol as f64).sqrt());
    let habitable_zone = (
        0.75 * (luminosity_sol as f64).sqrt(),
        1.77 * (luminosity_sol as f64).sqrt(),
    );

    ui.label(
        egui::RichText::new("STAR PROPERTIES")
            .font(heading_font())
            .color(ACCENT),
    );
    ui.add_space(6.0);

    egui::Grid::new("star_properties_grid")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            stat_row(ui, "TYPE", &spectral_type);
            stat_row(ui, "LUMINOSITY", &format!("{luminosity_sol:.3} L☉"));
            stat_row(ui, "TEMP", &format!("{temperature_kelvin:.0} K"));
            stat_row(ui, "MASS", &format!("{mass_sol:.2} M☉"));
            stat_row(ui, "RADIUS", &format!("{radius_sol:.2} R☉"));
            stat_row(ui, "FROST LINE", &format!("{frost_line_au:.2} AU"));
            stat_row(
                ui,
                "HZ",
                &format!("{:.2}-{:.2} AU", habitable_zone.0, habitable_zone.1),
            );
            if let Some(metallicity) = metallicity {
                stat_row(ui, "METALLICITY", &format!("[Fe/H] {metallicity:+.2}"));
            }
        });
}

fn fallback_spectral_type(body: &CelestialBody, star_system: Option<&StarSystem>) -> String {
    if body.name.eq_ignore_ascii_case("sun") || body.name.eq_ignore_ascii_case("sol") {
        return "G2V".to_string();
    }

    star_system
        .map(|system| match system.spectral_class {
            SpectralClass::O => "O-type",
            SpectralClass::B => "B-type",
            SpectralClass::A => "A-type",
            SpectralClass::F => "F-type",
            SpectralClass::G => "G-type",
            SpectralClass::K => "K-type",
            SpectralClass::M => "M-type",
        })
        .unwrap_or("Star")
        .to_string()
}

// ─── Orbital Stats ───────────────────────────────────────────────────────

// ─── Habitability Radar ──────────────────────────────────────────────────

/// Scores: [gravity, temperature, pressure, air_quality, hydrosphere] all 0..=1.
///
/// Each axis mirrors the colony-cost formula so the radar chart is a
/// direct visual inverse of the terraforming effort.  A perfect pentagon
/// (all 100%) means Earth-like conditions.
fn compute_habitability_scores(
    body: &CelestialBody,
    temp: Option<&SurfaceTemperature>,
    atmo: Option<&AtmosphereComposition>,
    ocean: Option<&OceanProperties>,
) -> [f32; 5] {
    // Gas giants — no solid surface, completely uninhabitable.
    let is_gas_giant =
        body.body_type == BodyType::GasGiant || atmo.is_some_and(|a| a.is_reference_pressure);
    if is_gas_giant {
        return [0.0; 5];
    }

    let gravity_g = body.surface_gravity();

    // ── Gravity (Earth-centered, smooth) ───────────────────────────────
    // Gaussian around 1.0 g so Venus (0.90 g) reads near 95% instead of
    // snapping to 100% across a wide flat band.
    let gravity_delta = gravity_g - 1.0;
    let gravity_score = (-(gravity_delta * gravity_delta) / (2.0 * 0.22 * 0.22)).exp();

    // ── Temperature (ideal 0–30 °C mean) ──────────────────────────────
    let mean_t = temp
        .map(|t| t.average_celsius)
        .or_else(|| atmo.map(|a| a.surface_temperature_celsius))
        .unwrap_or(-273.15);
    let temp_dev = if mean_t < 0.0 {
        -mean_t
    } else if mean_t > 30.0 {
        mean_t - 30.0
    } else {
        0.0
    };
    // Smooth exponential decay — mirrors the cost curve.
    let temp_score = (-temp_dev / 120.0).exp();

    // ── Pressure (ideal 0.5–2.0 bar) ─────────────────────────────────
    let pressure_score = match atmo {
        None => 0.0,
        Some(a) => {
            let p = a.surface_pressure_mbar / 1000.0;
            if p < 0.0001 {
                0.0
            } else if (0.5..=2.0).contains(&p) {
                1.0
            } else {
                let log_dev = if p < 0.5 {
                    (0.5 / p).log10()
                } else {
                    (p / 2.0).log10()
                };
                (1.0 - log_dev / 2.5).clamp(0.0, 1.0)
            }
        }
    };

    // ── Air quality / breathability ───────────────────────────────────
    let air_score = match atmo {
        Some(a) if a.breathable => 1.0,
        Some(a) => {
            let has_o2 = a.get_gas_percentage("O2").unwrap_or(0.0) > 0.1;
            let has_n2 = a.get_gas_percentage("N2").unwrap_or(0.0) > 1.0;
            let has_co2 = a.get_gas_percentage("CO2").unwrap_or(0.0) > 1.0;
            if has_o2 {
                0.6 // some O₂ present
            } else if has_n2 || has_co2 {
                0.3 // useful feedstock
            } else {
                0.15 // exotic/inert
            }
        }
        None => 0.0,
    };

    // ── Hydrosphere ───────────────────────────────────────────────────
    let hydro_score = ocean
        .map(|o| {
            if o.is_subsurface {
                0.3
            } else if o.ocean_type == OceanType::Water {
                (o.surface_fraction / 0.3).clamp(0.0, 1.0)
            } else {
                0.15
            }
        })
        .unwrap_or(0.0);

    [
        gravity_score,
        temp_score,
        pressure_score,
        air_score,
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
    ocean: Option<&OceanProperties>,
) {
    ui.label(
        egui::RichText::new("HABITABILITY")
            .font(heading_font())
            .color(TEXT_DIM),
    );
    ui.add_space(4.0);

    // Habitability row: radar (left, ~60 %) | vertical divider | terraforming placeholder (~40 %)
    ui.horizontal(|ui| {
        // Left column — radar chart, left-aligned, no centering padding
        ui.vertical(|ui| {
            draw_radar_chart(ui, scores);
        });

        // Vertical divider line between the two columns
        ui.separator();

        // Right column — terraforming placeholder (system not yet implemented)
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("TERRAFORMING")
                    .font(heading_font())
                    .color(TEXT_DIM),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Process: \u{2014}")
                    .font(mono_font(10.0))
                    .color(TEXT_DIM),
            );
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new("ETA: \u{2014}")
                    .font(mono_font(10.0))
                    .color(TEXT_DIM),
            );
            ui.add_space(8.0);
            // Button is disabled until the terraforming system is implemented
            ui.add_enabled(
                false,
                egui::Button::new(egui::RichText::new("Open Menu").font(mono_font(10.0))),
            );
        });
    });

    ui.add_space(6.0);

    // Colony cost summary
    let gravity_g = body.surface_gravity();
    let is_gas_giant =
        body.body_type == BodyType::GasGiant || atmosphere.is_some_and(|a| a.is_reference_pressure);
    let (min_t, max_t) = surface_temp
        .map(|t| (t.min_celsius, t.max_celsius))
        .or_else(|| {
            atmosphere.map(|a| (a.surface_temperature_celsius, a.surface_temperature_celsius))
        })
        .unwrap_or((-273.15, -273.15));

    // Compute water bonus from ocean data
    let water_bonus = ocean
        .map(|o| {
            if o.is_subsurface {
                -0.2
            } else if o.ocean_type == OceanType::Water {
                -0.5 * (o.surface_fraction / 0.3).clamp(0.0, 1.0)
            } else {
                -0.1
            }
        })
        .unwrap_or(0.0_f32);

    let details = crate::astronomy::components::calculate_colony_cost_with_water(
        gravity_g,
        min_t,
        max_t,
        atmosphere,
        is_gas_giant,
        water_bonus,
    );

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("COLONY COST")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        let (color, label, text) = if details.is_gas_giant || details.heavy_gravity_limit_exceeded {
            (RED_ACCENT, "", "UNINHABITABLE".to_string())
        } else if details.total_cost <= 0.01 {
            (GREEN_ACCENT, "", "IDEAL".to_string())
        } else if details.total_cost <= 2.0 {
            (
                GREEN_ACCENT,
                "Easy ",
                format!("{:.1}/10", details.total_cost),
            )
        } else if details.total_cost <= 4.0 {
            (
                theme::DIFFICULTY_MODERATE,
                "Moderate ",
                format!("{:.1}/10", details.total_cost),
            )
        } else if details.total_cost <= 7.0 {
            (AMBER, "Hard ", format!("{:.1}/10", details.total_cost))
        } else {
            (
                RED_ACCENT,
                "Extreme ",
                format!("{:.1}/10", details.total_cost),
            )
        };
        if !label.is_empty() {
            ui.label(
                egui::RichText::new(label)
                    .font(mono_font(10.0))
                    .color(color),
            );
        }
        ui.label(egui::RichText::new(text).font(mono_font(13.0)).color(color));
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
        if details.is_gas_giant {
            ui.colored_label(RED_ACCENT, "Gas Giant \u{2014} uninhabitable (no surface)");
        } else if details.heavy_gravity_limit_exceeded {
            ui.colored_label(RED_ACCENT, "Gravity > 3g \u{2014} uninhabitable");
        } else {
            let line = |ui: &mut egui::Ui, color: egui::Color32, name: &str, val: f32, max: f32| {
                if val > 0.005 {
                    ui.colored_label(color, format!("  {} +{:.1} / {:.0}", name, val, max));
                }
            };
            line(ui, theme::BODY_TERRESTRIAL, "Cold", details.cold_cost, 3.0);
            line(ui, RED_ACCENT, "Heat", details.heat_cost, 3.0);
            line(ui, AMBER, "Atmosphere", details.atmosphere_cost, 3.0);
            line(ui, AMBER, "Pressure", details.pressure_cost, 2.0);
            line(ui, AMBER, "Gravity", details.gravity_cost, 1.5);
            if details.water_bonus < -0.005 {
                ui.colored_label(GREEN_ACCENT, format!("  Water {:.1}", details.water_bonus));
            }
        }
    });
}

/// 5-point radar / pentagon chart rendered with `egui::Painter`.
fn draw_radar_chart(ui: &mut egui::Ui, scores: &[f32; 5]) {
    const LABELS: [&str; 5] = ["Gravity", "Temp", "Pressure", "Air", "Water"];
    const SIZE: f32 = 210.0;
    // These are fallbacks; actual radii are computed from the painter rect
    const FALLBACK_MAX_R: f32 = 65.0;

    let (response, painter) = ui.allocate_painter(egui::Vec2::splat(SIZE), egui::Sense::hover());
    let center = response.rect.center();
    // Compute a safe maximum radius based on the actual painter rect so the
    // radar never draws outside its bounds even when available space is small.
    let half_w = response.rect.width() / 2.0;
    let half_h = response.rect.height() / 2.0;
    let max_possible = half_w.min(half_h);
    let max_r = (max_possible - 16.0).clamp(10.0, FALLBACK_MAX_R);
    let label_r = (max_r + 14.0).min(82.0);

    // Axis angles — start top (-PI/2), go clockwise
    let angles: Vec<f32> = (0..5)
        .map(|i| -std::f32::consts::FRAC_PI_2 + (i as f32) * TAU / 5.0)
        .collect();

    // Reference rings at 33%, 66%, 100%
    for &frac in &[0.33_f32, 0.66, 1.0] {
        let r = max_r * frac;
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
        let tip = center + egui::Vec2::new(a.cos() * max_r, a.sin() * max_r);
        painter.line_segment([center, tip], egui::Stroke::new(0.5, BORDER));
    }

    // Earth reference pentagon (all 1.0) — thin cyan outline
    let earth_pts: Vec<egui::Pos2> = angles
        .iter()
        .map(|&a| center + egui::Vec2::new(a.cos() * max_r, a.sin() * max_r))
        .collect();
    painter.add(egui::Shape::closed_line(
        earth_pts,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(0, 242, 255, 60)),
    ));

    // Player polygon — compute vertex positions from scores.
    // Use a small minimum radius (3px) so even all-zero scores produce a
    // faintly visible mini-polygon rather than an invisible centre point.
    let player_pts: Vec<egui::Pos2> = scores
        .iter()
        .zip(angles.iter())
        .map(|(&s, &a)| {
            // Cap well inside the outer ring so anti-aliased stroke never bleeds outside.
            let r = (max_r * s.clamp(0.0, 1.0)).min(max_r - 3.0).max(3.0);
            center + egui::Vec2::new(a.cos() * r, a.sin() * r)
        })
        .collect();

    // Filled polygon — triangle fan from the centre so fill is correct
    // even when the radar shape is concave.
    let fill_color = egui::Color32::from_rgba_premultiplied(0, 242, 255, 18);
    for i in 0..5 {
        let j = (i + 1) % 5;
        painter.add(egui::Shape::convex_polygon(
            vec![center, player_pts[i], player_pts[j]],
            fill_color,
            egui::Stroke::NONE,
        ));
    }
    // Outline
    painter.add(egui::Shape::closed_line(
        player_pts.clone(),
        egui::Stroke::new(1.0, ACCENT),
    ));

    // Axis labels + % score beneath each label.
    // Score % is no longer rendered inside the polygon; it always appears
    // directly under the axis name so it is never occluded by the fill.
    for (i, (&s, &a)) in scores.iter().zip(angles.iter()).enumerate() {
        // Axis label — anchor based on position relative to centre so text
        // never extends beyond the widget boundary.
        let label_pos = center + egui::Vec2::new(a.cos() * label_r, a.sin() * label_r);
        let align = if label_pos.x < center.x - 5.0 {
            egui::Align2::RIGHT_CENTER
        } else if label_pos.x > center.x + 5.0 {
            egui::Align2::LEFT_CENTER
        } else if label_pos.y < center.y {
            egui::Align2::CENTER_BOTTOM
        } else {
            egui::Align2::CENTER_TOP
        };
        painter.text(label_pos, align, LABELS[i], mono_font(9.0), TEXT_DIM);

        // % value always shown directly below the axis name.
        // For a top-anchored label (CENTER_BOTTOM) the text sits above
        // label_pos, so the % anchor is placed at label_pos + a small gap.
        // For all other alignments we drop 11 px below the anchor point.
        let (pct_pos, pct_align) = match align {
            egui::Align2::CENTER_BOTTOM => (
                label_pos + egui::Vec2::new(0.0, 2.0),
                egui::Align2::CENTER_TOP,
            ),
            egui::Align2::CENTER_TOP => (
                label_pos - egui::Vec2::new(0.0, 2.0),
                egui::Align2::CENTER_BOTTOM,
            ),
            _ => (label_pos + egui::Vec2::new(0.0, 11.0), align),
        };
        painter.text(
            pct_pos,
            pct_align,
            format!("{:.0}%", s * 100.0),
            mono_font(8.0),
            ACCENT_DIM,
        );
    }
}

// ─── Atmosphere Bar ──────────────────────────────────────────────────────

/// Gas name -> colour mapping. Delegates to `theme::gas_color` so the
/// atmospheric palette is shared with any other surface that wants to
/// colour-code gases consistently.
fn gas_color(name: &str) -> egui::Color32 {
    theme::gas_color(name)
}

fn draw_atmosphere_section(ui: &mut egui::Ui, entity: Entity, atmo: &AtmosphereComposition) {
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
/// Returns a high-contrast label colour (dark or white) for the given background.
/// Uses the WCAG relative-luminance formula so we always stay readable.
fn label_color_for_bg(bg: egui::Color32) -> egui::Color32 {
    // Convert sRGB bytes to linear light
    let linearize = |c: u8| -> f32 {
        let s = c as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = linearize(bg.r());
    let g = linearize(bg.g());
    let b = linearize(bg.b());
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    // Dark text on bright backgrounds, white on dark ones
    if lum > 0.35 {
        egui::Color32::from_rgba_premultiplied(20, 20, 30, 230)
    } else {
        egui::Color32::from_rgba_premultiplied(255, 255, 255, 220)
    }
}

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
            let text_color = label_color_for_bg(color);
            let center = seg_rect.center();
            // Shadow pass – offset by 1 px, semi-transparent opposite of text colour
            let shadow_color = if text_color.r() < 128 {
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 60)
            } else {
                egui::Color32::from_rgba_premultiplied(0, 0, 0, 80)
            };
            painter.text(
                center + egui::Vec2::new(1.0, 1.0),
                egui::Align2::CENTER_CENTER,
                &label_text,
                mono_font(9.0),
                shadow_color,
            );
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                label_text,
                mono_font(9.0),
                text_color,
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
                        bar_response
                            .clone()
                            .on_hover_text(format!("{}: {:.3}%", gas.name, gas.percentage));
                    }
                    break;
                }
            }
        }
    }

    // Rounded border overlay
    painter.rect_stroke(
        bar_rect,
        3.0,
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );
}

// ─── Ocean Section ───────────────────────────────────────────────────────

fn draw_ocean_section(ui: &mut egui::Ui, ocean: &OceanProperties) {
    let (icon, label, color) = if ocean.is_subsurface {
        ("\u{25C8}", "SUBSURFACE OCEAN", theme::OCEAN_SUBSURFACE)
    } else {
        match ocean.ocean_type {
            OceanType::Water => ("\u{25C9}", "SURFACE OCEAN (WATER)", theme::OCEAN_WATER),
            OceanType::Methane => ("\u{25C9}", "METHANE LAKES", theme::OCEAN_METHANE),
            OceanType::Hydrocarbon => ("\u{25C9}", "HYDROCARBON LAKES", theme::OCEAN_HYDROCARBON),
            OceanType::Ammonia => ("\u{25C9}", "AMMONIA OCEAN", theme::OCEAN_AMMONIA),
            OceanType::Subsurface => ("\u{25C8}", "SUBSURFACE OCEAN", theme::OCEAN_SUBSURFACE),
        }
    };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).color(color));
        ui.label(egui::RichText::new(label).font(heading_font()).color(color));
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
        (GREEN_ACCENT, format!("+{:.0}% growth", (hab - 1.0) * 100.0))
    } else if hab > 1.0 {
        (
            theme::STATUS_SUCCESS_DIM,
            format!("+{:.0}% growth", (hab - 1.0) * 100.0),
        )
    } else if hab < 1.0 {
        (AMBER, format!("-{:.0}% penalty", (1.0 - hab) * 100.0))
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
    rate_tracker: &ResourceRateTracker,
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
            if *survey != SurveyLevel::CoreSample
                && ui
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
    draw_resource_grid(ui, resources, current_level, rate_tracker, entity);

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

/// Determine deposit magnitude tier (0–5) from total mass in megatons.
/// Each tier spans ~3 orders of magnitude.
fn magnitude_tier(megatons: f64) -> (u8, &'static str) {
    if megatons < 1.0 {
        (0, "Trace") // < 1 Mt
    } else if megatons < 1_000.0 {
        (1, "Minor") // Mt-range
    } else if megatons < 1_000_000.0 {
        (2, "Moderate") // Gt-range
    } else if megatons < 1_000_000_000.0 {
        (3, "Rich") // Tt-range
    } else if megatons < 1_000_000_000_000.0 {
        (4, "Vast") // Pt-range
    } else {
        (5, "Planetary") // Et-range and beyond
    }
}

pub(super) enum ResourceTileDisplay {
    Unknown,
    None,
    Deposit {
        discovered_megatons: f64,
        concentration: Option<f32>,
    },
}

pub(super) fn paint_resource_tile(
    ui: &mut egui::Ui,
    resource: ResourceType,
    display: ResourceTileDisplay,
    size: f32,
    cat_color: egui::Color32,
) -> egui::Response {
    let (response, painter) = ui.allocate_painter(egui::Vec2::splat(size), egui::Sense::hover());
    let rect = response.rect;

    painter.rect_filled(rect, 3.0, TILE_BG);

    match display {
        ResourceTileDisplay::Deposit {
            discovered_megatons,
            concentration,
        } => {
            let (tier, _tier_label) = magnitude_tier(discovered_megatons);

            let brightness = concentration
                .map(|conc| {
                    let conc = conc.clamp(1e-10, 1.0) as f64;
                    let conc_norm = ((conc.log10() + 10.0) / 10.0).clamp(0.0, 1.0) as f32;
                    0.3 + 0.7 * conc_norm
                })
                .unwrap_or(0.8);

            let fill_frac = match tier {
                0 => 0.08,
                1 => 0.25,
                2 => 0.42,
                3 => 0.60,
                4 => 0.80,
                _ => 1.00,
            };
            let fill_height = rect.height() * fill_frac;
            let fill_rect = egui::Rect::from_min_max(
                egui::Pos2::new(rect.left(), rect.bottom() - fill_height),
                rect.max,
            );

            let fill_alpha = (40.0 + 120.0 * brightness) as u8;
            let fill_color = egui::Color32::from_rgba_premultiplied(
                (cat_color.r() as f32 * brightness * 0.5) as u8,
                (cat_color.g() as f32 * brightness * 0.5) as u8,
                (cat_color.b() as f32 * brightness * 0.5) as u8,
                fill_alpha,
            );
            painter.rect_filled(fill_rect, 0.0, fill_color);

            painter.text(
                egui::Pos2::new(rect.center().x, rect.top() + size * 0.32),
                egui::Align2::CENTER_CENTER,
                resource.symbol(),
                mono_font(11.0),
                egui::Color32::WHITE,
            );

            let compact = format_mass_compact(discovered_megatons);
            painter.text(
                egui::Pos2::new(rect.center().x, rect.top() + size * 0.58),
                egui::Align2::CENTER_CENTER,
                &compact,
                mono_font(7.0),
                egui::Color32::from_rgba_premultiplied(220, 230, 240, (180.0 * brightness) as u8),
            );

            let pip_y = rect.bottom() - 5.0;
            let pip_r = 2.0_f32;
            let pip_spacing = 6.0_f32;
            let total_pip_w = 5.0 * pip_spacing - pip_spacing + pip_r * 2.0;
            let pip_start_x = rect.center().x - total_pip_w * 0.5 + pip_r;
            for i in 0..5u8 {
                let cx = pip_start_x + i as f32 * pip_spacing;
                if i < tier {
                    painter.circle_filled(egui::Pos2::new(cx, pip_y), pip_r, cat_color);
                } else {
                    painter.circle_stroke(
                        egui::Pos2::new(cx, pip_y),
                        pip_r,
                        egui::Stroke::new(0.5, theme::BORDER_DIM),
                    );
                }
            }
        }
        ResourceTileDisplay::None => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                resource.symbol(),
                mono_font(10.0),
                theme::SURFACE_RAISED_2,
            );
        }
        ResourceTileDisplay::Unknown => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                mono_font(13.0),
                TEXT_DIM,
            );
        }
    }

    painter.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );

    response
}

/// Resource periodic grid: tiles laid out by category, each showing a chemical
/// symbol with magnitude pips and compact value.
fn draw_resource_grid(
    ui: &mut egui::Ui,
    resources: &PlanetResources,
    survey_level: SurveyLevel,
    rate_tracker: &ResourceRateTracker,
    entity: Entity,
) {
    let tile_size = 44.0_f32;
    let tile_spacing = 3.0_f32;

    for (category_name, category_resources) in ResourceType::by_category() {
        // Only show resources that can be mined from natural deposits
        let mineable: Vec<ResourceType> = category_resources
            .into_iter()
            .filter(|r| r.is_mineable())
            .collect();
        if mineable.is_empty() {
            continue;
        }

        let cat_color = theme::category_color(category_name);

        // Category header with coloured label
        ui.label(
            egui::RichText::new(category_name)
                .font(mono_font(9.0))
                .color(cat_color),
        );

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(tile_spacing);

            for resource_type in &mineable {
                let deposit = resources.get_deposit(resource_type);
                draw_resource_tile(
                    ui,
                    *resource_type,
                    deposit,
                    survey_level,
                    tile_size,
                    cat_color,
                    rate_tracker,
                    entity,
                );
            }
        });

        ui.add_space(2.0);
    }
}

/// Single resource tile: symbol, compact mass value, magnitude pips, and
/// concentration-driven colour brightness.  Category colour tints the tile
/// so different resource families are visually distinct.
fn draw_resource_tile(
    ui: &mut egui::Ui,
    resource: ResourceType,
    deposit: Option<&MineralDeposit>,
    survey_level: SurveyLevel,
    size: f32,
    cat_color: egui::Color32,
    rate_tracker: &ResourceRateTracker,
    entity: Entity,
) {
    let has_deposit = deposit.is_some_and(|d| d.reserve.total_mass() > 0.001);
    let response = if let Some(d) = deposit.filter(|d| d.reserve.total_mass() > 0.001) {
        let discovered = survey_level.discovered_amount(&d.reserve);
        paint_resource_tile(
            ui,
            resource,
            ResourceTileDisplay::Deposit {
                discovered_megatons: discovered,
                concentration: Some(d.reserve.concentration),
            },
            size,
            cat_color,
        )
    } else {
        paint_resource_tile(ui, resource, ResourceTileDisplay::None, size, cat_color)
    };

    // Hover tooltip
    if response.hovered() && has_deposit {
        let d = deposit.unwrap();
        let discovered = survey_level.discovered_amount(&d.reserve);
        let tooltip_id = egui::Id::new("resource_tile_tooltip").with(resource.symbol());

        // Override the popup frame style on the ctx *before* show_tooltip is called.
        // egui::show_tooltip wraps content in Frame::popup(ctx.style()) which is
        // constructed outside our closure — visual overrides inside don't affect it.
        let prev_visuals = ui.ctx().style().visuals.clone();
        ui.ctx().set_visuals(egui::Visuals {
            window_fill: egui::Color32::TRANSPARENT,
            window_stroke: egui::Stroke::NONE,
            window_shadow: egui::Shadow::NONE,
            popup_shadow: egui::Shadow::NONE,
            ..prev_visuals.clone()
        });

        egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            tooltip_id,
            egui::PopupAnchor::Pointer,
        )
        .show(|ui| {
            let tip_frame = egui::Frame::NONE
                .fill(BG_FILL)
                .stroke(egui::Stroke::new(1.0, ACCENT_DIM))
                .inner_margin(egui::Margin::same(8));

            tip_frame.show(ui, |ui| {
                ui.set_min_width(180.0);
                ui.label(
                    egui::RichText::new(resource.display_name())
                        .font(mono_font(13.0))
                        .color(ACCENT),
                );

                // Phase badge + magnitude tier
                let tier: u8 = match discovered {
                    x if x >= 1e12 => 5,
                    x if x >= 1e9 => 4,
                    x if x >= 1e6 => 3,
                    x if x >= 1e3 => 2,
                    x if x > 0.0 => 1,
                    _ => 0,
                };
                let tier_label = match tier {
                    5 => "Massive",
                    4 => "Major",
                    3 => "Moderate",
                    2 => "Minor",
                    1 => "Trace",
                    _ => "None",
                };
                let tier_color = theme::tier_color(tier);
                let tier_dots: String =
                    "\u{25CF}".repeat(tier as usize) + &"\u{25CB}".repeat(5 - tier as usize);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{}", d.phase))
                            .font(mono_font(9.0))
                            .color(TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new(format!("{tier_dots} {tier_label}"))
                            .font(mono_font(9.0))
                            .color(tier_color),
                    );
                });
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

                        tooltip_row(ui, proven_label, &format_mass(d.reserve.proven_crustal));

                        if matches!(
                            survey_level,
                            SurveyLevel::SeismicSurvey | SurveyLevel::CoreSample
                        ) {
                            tooltip_row(ui, deep_label, &format_mass(d.reserve.deep_deposits));
                        }

                        if matches!(survey_level, SurveyLevel::CoreSample) {
                            tooltip_row(ui, bulk_label, &format_mass(d.reserve.planetary_bulk));
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
                    });

                // Balance (per-body production rate for this resource)
                ui.add_space(2.0);
                let rate = rate_tracker.get_entity_resource_rate(entity, &resource);
                let (rate_text, rate_color) = format_rate_monthly(rate);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Balance")
                            .font(mono_font(9.0))
                            .color(TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new(rate_text)
                            .font(mono_font(10.0))
                            .color(rate_color),
                    );
                });
            });
        });

        // Restore previous visuals now that the tooltip has been submitted
        ui.ctx().set_visuals(prev_visuals);
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

// ─── Outpost / Colony Section ────────────────────────────────────────────────

/// Draws the colony/outpost section of the dossier.
///
/// - If the body already has a `Colony` component, shows a compact status
///   summary (population, buildings, housing utilisation).
/// - If it does not, evaluates habitability and shows either:
///   - A blocked state (gas giant or gravity > 3 g) with a clear explanation.
///   - A warning + "Establish Outpost" button for extreme-but-possible bodies.
///   - A normal "Establish Outpost" button for habitable/marginal bodies.
///
/// The button enqueues an `EstablishOutpostRequest` that carries a
/// `needs_oxygen` flag (derived from `AtmosphereComposition.breathable`).
/// The processing system then attaches `ColonyEnvironmentCosts` so O₂ and
/// water are deducted from the local stockpile once population arrives.
#[allow(clippy::too_many_arguments)]
fn draw_colony_section(
    ui: &mut egui::Ui,
    entity: Entity,
    body: &CelestialBody,
    existing_colony: Option<&Colony>,
    atmosphere: Option<&AtmosphereComposition>,
    surface_temp: Option<&SurfaceTemperature>,
    ocean: Option<&OceanProperties>,
    pending_actions: &mut PendingConstructionActions,
) {
    if let Some(colony) = existing_colony {
        // ── Already colonised ──────────────────────────────────────
        ui.label(
            egui::RichText::new("COLONY")
                .font(heading_font())
                .color(TEXT_DIM),
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Name:")
                    .font(mono_font(10.0))
                    .color(TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(&colony.name)
                    .font(mono_font(11.0))
                    .color(TEXT_VALUE),
            );
        });

        let pop = colony.population;
        let housing = colony.housing_capacity();
        let util_pct = if housing > 0.0 {
            (pop / housing * 100.0).min(100.0)
        } else {
            0.0
        };
        let pop_color = if util_pct > 90.0 {
            RED_ACCENT
        } else if util_pct > 70.0 {
            AMBER
        } else {
            GREEN_ACCENT
        };

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Population:")
                    .font(mono_font(10.0))
                    .color(TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{}  ({:.0}% housing)",
                    Colony::format_population(pop),
                    util_pct
                ))
                .font(mono_font(11.0))
                .color(pop_color),
            );
        });

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Buildings:")
                    .font(mono_font(10.0))
                    .color(TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(format!("{}", colony.total_buildings()))
                    .font(mono_font(11.0))
                    .color(TEXT_VALUE),
            );
        });

        return;
    }

    // ── Not yet colonised — evaluate habitability ──────────────────────
    ui.label(
        egui::RichText::new("OUTPOST")
            .font(heading_font())
            .color(TEXT_DIM),
    );
    ui.add_space(4.0);

    // Compute cost details to gate the button
    let is_gas_giant = body.body_type == crate::plugins::solar_system_data::BodyType::GasGiant
        || atmosphere.is_some_and(|a| a.is_reference_pressure);
    let gravity_g = body.surface_gravity();
    let (min_t, max_t) = surface_temp
        .map(|t| (t.min_celsius, t.max_celsius))
        .unwrap_or((-273.15, -273.15));
    let water_bonus = ocean
        .map(|o| {
            use crate::astronomy::components::OceanType;
            if o.is_subsurface {
                -0.2_f32
            } else if o.ocean_type == OceanType::Water {
                -0.5 * (o.surface_fraction / 0.3).clamp(0.0, 1.0)
            } else {
                -0.1
            }
        })
        .unwrap_or(0.0_f32);

    let details = crate::astronomy::components::calculate_colony_cost_with_water(
        gravity_g,
        min_t,
        max_t,
        atmosphere,
        is_gas_giant,
        water_bonus,
    );

    // Hard blocks
    if details.is_gas_giant {
        ui.colored_label(
            RED_ACCENT,
            egui::RichText::new("⛔  UNINHABITABLE — Gas Giant")
                .font(mono_font(11.0))
                .strong(),
        );
        ui.label(
            egui::RichText::new(
                "Gas giants have no solid surface. Outposts cannot be established.",
            )
            .font(mono_font(10.0))
            .color(TEXT_DIM),
        );
        return;
    }
    if details.heavy_gravity_limit_exceeded {
        ui.colored_label(
            RED_ACCENT,
            egui::RichText::new(format!(
                "⛔  UNINHABITABLE — Gravity {:.1} g (> 3.0 g limit)",
                gravity_g
            ))
            .font(mono_font(11.0))
            .strong(),
        );
        ui.label(
            egui::RichText::new(
                "Surface gravity exceeds human physiological limits. Outposts cannot be established.",
            )
            .font(mono_font(10.0))
            .color(TEXT_DIM),
        );
        return;
    }

    // Determine atmosphere breathability for O₂ cost
    let breathable = atmosphere.is_some_and(|a| a.breathable);
    let needs_oxygen = !breathable;

    // Warn for extreme (but survivable) environments
    let is_extreme = details.total_cost > 7.0;

    // Info group: what the outpost includes + environmental notes
    ui.group(|ui| {
        ui.label(
            egui::RichText::new("Starter package:")
                .font(mono_font(10.0))
                .strong()
                .color(TEXT_VALUE),
        );
        ui.label(
            egui::RichText::new("• Life Support ×1")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        ui.label(
            egui::RichText::new("• Housing Complex ×1  (capacity: 25M residents)")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        ui.label(
            egui::RichText::new("• Fission Reactor ×2  (Uranium-powered)")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        ui.label(
            egui::RichText::new("• Agricultural Dome ×2  (food for ~80,000 ppl)")
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        ui.add_space(4.0);

        // ── Environmental operating costs ──────────────────────────
        ui.label(
            egui::RichText::new("Ongoing resource costs (per person / yr):")
                .font(mono_font(10.0))
                .strong()
                .color(TEXT_VALUE),
        );
        ui.label(
            egui::RichText::new("• 💧 Water: 50 t/person/yr  (recycling losses)")
                .font(mono_font(10.0))
                .color(theme::BODY_TERRESTRIAL),
        );
        if needs_oxygen {
            ui.label(
                egui::RichText::new("• 🫁 Oxygen: 100 t/person/yr  (no breathable atm.)")
                    .font(mono_font(10.0))
                    .color(AMBER),
            );
        } else {
            ui.label(
                egui::RichText::new("• 🫁 Oxygen: none  (breathable atmosphere present)")
                    .font(mono_font(10.0))
                    .color(GREEN_ACCENT),
            );
        }

        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Resources must be transported before construction begins.")
                .font(mono_font(9.0))
                .italics()
                .color(TEXT_DIM),
        );
    });

    ui.add_space(4.0);

    // Extreme environment warning
    if is_extreme {
        ui.colored_label(
            AMBER,
            egui::RichText::new(format!(
                "⚠  Extreme environment (colony cost {:.1}/10). Significant life-support required.",
                details.total_cost
            ))
            .font(mono_font(10.0)),
        );
        ui.add_space(4.0);
    }

    let btn_response = ui.add(
        egui::Button::new(
            egui::RichText::new("🏗  Establish Outpost")
                .font(mono_font(12.0))
                .color(ACCENT),
        )
        .min_size(egui::Vec2::new(200.0, 28.0)),
    );
    if btn_response.clicked() {
        pending_actions
            .establish_outpost
            .push(EstablishOutpostRequest {
                body_entity: entity,
                colony_name: body.name.clone(),
                needs_oxygen,
            });
    }
    let hover = if needs_oxygen {
        "Create an outpost colony. This body has no breathable atmosphere — oxygen will \
         be consumed from the stockpile. Starter buildings will be queued and built \
         as soon as the required resources arrive."
    } else {
        "Create an outpost colony. Starter buildings will be queued and built \
         as soon as the required resources arrive."
    };
    btn_response.on_hover_text(hover);
}
