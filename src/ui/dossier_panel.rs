//! Planet Dossier Panel — "Tactical OS" style body information display.
//!
//! Replaces the old text-based right `SidePanel` from `dashboard.rs` with a
//! visually rich panel featuring:
//! - Stacked atmosphere composition bar
//! - Habitability radar chart (5 colony-cost axes)
//! - Resource periodic grid with depth-fill tiles
//! - Dark tactical palette (#0A0F1E + #00F2FF accents)
//!
//! PR-B (GRA-67) wires the dossier as the *demonstration panel* for the
//! new `theme::section_h1` / `section_h2` / `ledger_panel` primitives. The
//! body-name title and the "STAR PROPERTIES" header now go through
//! `section_h1` / `section_h2`; the Resources section is wrapped in
//! `ledger_panel` to demonstrate the Pattern 2 collapsible shell. The
//! remaining bespoke `RichText::new(...).font(heading_font()).color(...)`
//! calls are left for downstream PRs to sweep.

use super::dashboard::{format_mass, format_mass_compact, format_rate_monthly, rate_tooltip};
use super::resources_bar::format_population;
use super::tab::Tab;
use super::theme::{
    self, ACCENT, ACCENT_DIM, AMBER, BG, BORDER, BORDER_DIM, GREEN, RED, SURFACE, TEXT_DIM,
    TEXT_VALUE,
};
use super::*;
use crate::astronomy::components::{
    AtmosphericGas, OceanProperties, OceanType, StellarProperties, SurfaceTemperature,
};
use crate::astronomy::nearby_stars::NearbyStarsData;
use crate::economy::components::SurveyLevel;
use crate::economy::components::{SpectralClass, StarSystem};
use crate::economy::mining::MiningOperation;
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};
use crate::survey::components::{
    ActiveSurveyMission, ContinuousStationBonus, ContinuousSurveyStation, FailedMissionRecord,
    SurveyState,
};
use crate::survey::data::{
    ReasonTag, ScientistSummary, SurveyMissionTemplate, SurveyMissionTemplates,
};
use crate::survey::events::{
    AbortSurveyMission, DismissFailedMission, DismissSurveyMission, DispatchSurveyMission,
};
use crate::survey::types::{
    AnomalyState, MissionFailureReason, MissionStatus, SurveyDimension, MAX_TIER,
    WARNING_CONFIDENCE,
};
use bevy::ecs::system::SystemParam;
use std::borrow::Cow;
use std::collections::HashSet;
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

/// Sub-views for the Resources ledger. The first variant is the
/// `Default` so callers using `tab_strip(..., DossierResourceView::default(), ...)`
/// get the existing by-category grid. PR-B introduces the enum as
/// `theme::tab_strip<T: Tab>` proof; full migration of the dossier UX
/// to per-entity persistent state is parked for the next v2 PR.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum DossierResourceView {
    #[default]
    ByCategory,
    Compact,
}

impl Tab for DossierResourceView {
    fn id(&self) -> &'static str {
        match self {
            DossierResourceView::ByCategory => "by_category",
            DossierResourceView::Compact => "compact",
        }
    }
    fn label(&self) -> Cow<'static, str> {
        match self {
            DossierResourceView::ByCategory => Cow::Borrowed("By Category"),
            DossierResourceView::Compact => Cow::Borrowed("Compact"),
        }
    }
    fn icon(&self) -> Option<&'static str> {
        None
    }
}

/// Section header font
fn heading_font() -> egui::FontId {
    theme::heading()
}

/// Monospace value font
fn mono_font(size: f32) -> egui::FontId {
    theme::mono(size)
}

// ─── Main System ─────────────────────────────────────────────────────────

/// Bundled `SystemParam` for [`ui_planet_dossier`].
///
/// The function takes too many SystemParams for `IntoSystem` to derive
/// directly (Bevy 0.18's `SystemParam` chain breaks at ~16 fields). We
/// pack everything into a single struct so the system has exactly one
/// generic parameter at the function level.
///
/// This is the same workaround the resources-bar / construction-panel
/// systems use for the same Bevy 0.18 limit.
#[derive(SystemParam)]
pub(super) struct DossierUiParams<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub contexts: EguiContexts<'w, 's>,
    pub selection: Res<'w, Selection>,
    pub active_menu: Res<'w, ActiveMenu>,
    pub nearby_stars: Res<'w, NearbyStarsData>,
    /// Immutable body tuple (15 components — Bevy 0.18's tuple limit
    /// for queries). `&mut SurveyLevel` lives in a separate query to
    /// keep this tuple under the cap.
    pub body_query: Query<
        'w,
        's,
        (
            &'static CelestialBody,
            Option<&'static SpaceCoordinates>,
            Option<&'static KeplerOrbit>,
            Option<&'static PlanetResources>,
            Option<&'static AtmosphereComposition>,
            Option<&'static crate::plugins::starmap::PlanetCategory>,
            Option<&'static Population>,
            Option<&'static SurfaceTemperature>,
            Option<&'static StellarProperties>,
            Option<&'static crate::astronomy::components::SystemId>,
            Option<&'static StarSystem>,
            Option<&'static LogicalParent>,
            Option<&'static OceanProperties>,
            Option<&'static Colony>,
            Option<&'static SurveyState>,
        ),
    >,
    /// `SurveyLevel` is the dossier's *display* adapter. The v0.5.0
    /// `SurveyState` is the source of truth; the dossier derives a
    /// `SurveyLevel` for the status badge and the resource-grid
    /// fidelity slicing. The UI never mutates this enum — the legacy
    /// click-to-upgrade button was removed in GRA-107.
    pub survey_level_query: Query<'w, 's, &'static SurveyLevel>,
    pub parent_coords_query: Query<'w, 's, &'static SpaceCoordinates>,
    pub all_bodies_query: Query<
        'w,
        's,
        (
            Entity,
            &'static CelestialBody,
            Option<&'static LogicalParent>,
            Option<&'static KeplerOrbit>,
            Option<&'static crate::astronomy::components::SystemId>,
        ),
    >,
    pub star_system_query: Query<
        'w,
        's,
        (
            Entity,
            &'static StarSystemIcon,
            Option<&'static SelectedStarSystem>,
        ),
    >,
    pub rate_tracker: Res<'w, ResourceRateTracker>,
    pub mission_templates: Res<'w, SurveyMissionTemplates>,
    pub pending_actions: ResMut<'w, PendingConstructionActions>,
    /// v3.6: RON-driven building definitions + colony constants. Used by
    /// the housing-capacity and population-growth displays.
    pub buildings_data: Option<Res<'w, crate::colony::data::BuildingsData>>,
    /// PR-F (GRA-117): needed by the legacy-to-state fallback so the
    /// dossier can render a `SurveyState` view for bodies that still
    /// only carry the old `SurveyLevel` component (Phase 1 migration
    /// window). The fallback is built once per body lookup.
    pub simulation_time: Res<'w, crate::ui::SimulationTime>,
    /// GRA-83b: orbital survey station surface needs the per-body
    /// bonus cache plus the active mining flag to render the
    /// aggregated section. Keep these as normal immutable queries so
    /// the egui system can coexist with mutable params.
    pub station_bonus_query: Query<'w, 's, &'static ContinuousStationBonus>,
    pub mining_query: Query<'w, 's, &'static MiningOperation>,
    /// Enumerate every `ContinuousSurveyStation` in the world to
    /// compute the per-tier list for the orbited body. Immutable
    /// (we read `tier` and `orbiting_body` only).
    pub stations_query: Query<'w, 's, &'static ContinuousSurveyStation>,
}

/// Renders the right-side "Celestial Body Dossier" panel when a body is
/// selected and no full-screen menu is active.
pub(super) fn ui_planet_dossier(mut params: DossierUiParams) {
    // Don't show when full-screen menus are active
    if matches!(
        params.active_menu.current,
        GameMenu::Research | GameMenu::Construction | GameMenu::Economy | GameMenu::Fleets
    ) {
        return;
    }

    let ctx = match params.contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // If a star system is selected (starmap view), don't show body dossier
    let has_selected_star = params
        .star_system_query
        .iter()
        .any(|(_, _, sel)| sel.is_some());
    if has_selected_star {
        return;
    }

    if !params.selection.has_selection() {
        return;
    }

    let entity = match params.selection.get() {
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
        population,
        surface_temp,
        stellar_props,
        system_id,
        star_system,
        logical_parent,
        ocean_props,
        existing_colony,
        survey_state,
    )) = params.body_query.get(entity)
    else {
        return;
    };
    let survey_level_opt: Option<&SurveyLevel> = params.survey_level_query.get(entity).ok();

    egui::SidePanel::right("selection_panel")
        .min_width(340.0)
        .max_width(420.0)
        .frame(theme::panel_frame())
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
                        &params.parent_coords_query,
                        &params.all_bodies_query,
                    );

                    if body.body_type == BodyType::Star {
                        section_divider(ui);
                        draw_star_properties_section(
                            ui,
                            body,
                            stellar_props,
                            system_id,
                            star_system,
                            &params.nearby_stars,
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

                    // Compute the v0.5.0 effective level once at the
                    // dossier level so the SURVEY ledger's status badge
                    // and the RESOURCES grid's fidelity slicing both
                    // share the same source of truth (the legacy
                    // click-to-upgrade button was removed in GRA-107).
                    let current_level =
                        effective_survey_level(survey_level_opt.copied(), survey_state);

                    // PR-F (GRA-117): build a `SurveyState` view that
                    // falls back to a `from_legacy_level` projection when
                    // the body still only carries the old `SurveyLevel`
                    // component. Bodies that have neither a legacy
                    // `SurveyLevel` nor a v0.5.0 `SurveyState` (i.e. the
                    // player hasn't dispatched a survey mission there
                    // yet) get a fresh default `SurveyState` so the
                    // SURVEY ledger — and the dispatch mission picker —
                    // still render. The player needs the picker to be
                    // visible on every body, including unsurveyed ones,
                    // otherwise they'd have no way to start the survey
                    // game loop.
                    let sim_now = params.simulation_time.elapsed_seconds();
                    let survey_view: SurveyState =
                        fallback_survey_state(survey_level_opt.copied(), survey_state, sim_now)
                            .unwrap_or_default();

                    // ── Survey Ledger (SURVEY_REWORK.md §10) ──────
                    // PR-F (GRA-108): the dossier gains a top-level
                    // SURVEY section. The legacy layout buried survey
                    // content (status / active missions / dispatch) inside
                    // the RESOURCES ledger; §10 lifts them out into their
                    // own `theme::ledger_panel` shell. The deposit grid
                    // stays in RESOURCES (issue AC #8).
                    //
                    // PR-F (GRA-118): the ledger is now ALWAYS
                    // rendered for every body type that supports
                    // surveys (i.e. not a ring or star). Unsurveyed
                    // bodies show "0% surveyed" / "UNSURVEYED" in the
                    // COVERAGE SUMMARY, an empty ACTIVE MISSIONS list,
                    // and the full DISPATCH MISSION picker — so the
                    // player can send the first survey mission to
                    // any body in any star system.
                    if body.body_type != BodyType::Star && body.body_type != BodyType::Ring {
                        section_divider(ui);
                        theme::ledger_panel(ui, "dossier_survey", "SURVEY", &(), |ui| {
                            draw_survey_section(
                                ui,
                                entity,
                                &body.name,
                                &survey_view,
                                current_level,
                                sim_now,
                                &mut params.commands,
                                &params.mission_templates,
                            );
                        });
                    }

                    // ── Resource Grid ───────────────────────────────
                    if let Some(res) = resources {
                        section_divider(ui);
                        // PR-B (GRA-67) wraps the resource section in
                        // `theme::ledger_panel` to demonstrate the Pattern 2
                        // collapsible shell. The `()` token is the canonical
                        // "no filter" choice — the generic is reserved for
                        // future typed callback tokens.
                        let mut active_view: DossierResourceView = ui
                            .data(|d| d.get_temp(egui::Id::new("dossier_resource_view")))
                            .unwrap_or_default();
                        theme::ledger_panel(ui, "dossier_resources", "RESOURCES", &(), |ui| {
                            let next = theme::tab_strip(
                                ui,
                                &[
                                    DossierResourceView::ByCategory,
                                    DossierResourceView::Compact,
                                ],
                                active_view,
                                |selected| active_view = selected,
                            );
                            ui.data_mut(|d| {
                                d.insert_temp(egui::Id::new("dossier_resource_view"), next)
                            });
                            draw_resource_section(
                                ui,
                                entity,
                                res,
                                current_level,
                                &params.rate_tracker,
                                Some(&survey_view),
                                next,
                            );
                        });
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
                            &mut params.pending_actions,
                            params.buildings_data.as_deref().unwrap_or(&crate::colony::data::BuildingsData::default()),
                        );
                    }

                    // ── GRA-83b: Orbital Survey Stations ────────────
                    // Surfaced as a body-level section because the
                    // station is a body-scoped passive effect
                    // (per-body aggregation, see
                    // `apply_continuous_station_bonus`).
                    section_divider(ui);
                    draw_orbital_station_section(
                        ui,
                        params.station_bonus_query.get(entity).ok(),
                        params.mining_query.get(entity).ok(),
                        params.stations_query.iter(),
                        entity,
                        &body.name,
                    );
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
    theme::section_h1(ui, &body.name);

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
                            theme::stat_row_with_tooltip(ui, "DISTANCE", &value_str, tooltip);
                        }
                    } else if let Some(c) = coords {
                        let star_pos = find_star_position(
                            logical_parent,
                            parent_coords_query,
                            all_bodies_query,
                        );
                        let distance_au = (c.position - star_pos).length();
                        theme::stat_row_with_tooltip(
                            ui,
                            "DISTANCE",
                            &format!("{distance_au:.3} AU"),
                            "Current distance from the system's primary star.",
                        );
                    }
                }

                theme::stat_row_with_tooltip(
                    ui,
                    "RADIUS",
                    &format!("{:.1} km", body.radius),
                    "Mean body radius in kilometers.",
                );
                theme::stat_row_with_tooltip(
                    ui,
                    "MASS",
                    &format!("{:.2e} kg", body.mass),
                    "Total mass of the body in kilograms.",
                );
                theme::stat_row_with_tooltip(
                    ui,
                    "GRAVITY",
                    &format!("{:.2} g", body.surface_gravity()),
                    "Surface gravity relative to Earth gravity (1.0 g).",
                );

                if let Some(pop) = population {
                    if pop.count > 0.0 {
                        theme::stat_row_with_tooltip(
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
                egui::Stroke::new(1.0_f32, BORDER),
            );
            ui.add_space(10.0);

            ui.vertical(|ui| {
                egui::Grid::new("header_stats_orbital")
                    .num_columns(2)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        theme::stat_row_with_tooltip(
                            ui,
                            "SMA",
                            &format!("{:.4} AU", orbit.semi_major_axis),
                            "Semi-major axis: average orbital distance from the parent body.",
                        );
                        theme::stat_row_with_tooltip(
                            ui,
                            "ECC",
                            &format!("{:.5}", orbit.eccentricity),
                            "Eccentricity: 0 is circular; higher values are more elongated.",
                        );
                        theme::stat_row_with_tooltip(
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
                            theme::stat_row_with_tooltip(
                                ui,
                                "PERIOD",
                                &format!("{period_d:.1} d"),
                                "Time required to complete one full orbit.",
                            );
                        } else {
                            theme::stat_row_with_tooltip(
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

/// Thin horizontal tactical divider.
fn section_divider(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        egui::Stroke::new(1.0_f32, BORDER),
    );
    ui.add_space(theme::Spacing::sm);
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

    theme::section_h2(ui, "STAR PROPERTIES");

    egui::Grid::new("star_properties_grid")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            theme::stat_row(ui, "TYPE", &spectral_type);
            theme::stat_row(ui, "LUMINOSITY", &format!("{luminosity_sol:.3} L☉"));
            theme::stat_row(ui, "TEMP", &format!("{temperature_kelvin:.0} K"));
            theme::stat_row(ui, "MASS", &format!("{mass_sol:.2} M☉"));
            theme::stat_row(ui, "RADIUS", &format!("{radius_sol:.2} R☉"));
            theme::stat_row(ui, "FROST LINE", &format!("{frost_line_au:.2} AU"));
            theme::stat_row(
                ui,
                "HZ",
                &format!("{:.2}-{:.2} AU", habitable_zone.0, habitable_zone.1),
            );
            if let Some(metallicity) = metallicity {
                theme::stat_row(ui, "METALLICITY", &format!("[Fe/H] {metallicity:+.2}"));
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
            ui.add_space(theme::Spacing::sm);
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
            egui::Stroke::new(0.5_f32, BORDER),
        ));
    }

    // Axis lines
    for &a in &angles {
        let tip = center + egui::Vec2::new(a.cos() * max_r, a.sin() * max_r);
        painter.line_segment([center, tip], egui::Stroke::new(0.5_f32, BORDER));
    }

    // Earth reference pentagon (all 1.0) — thin cyan outline
    let earth_pts: Vec<egui::Pos2> = angles
        .iter()
        .map(|&a| center + egui::Vec2::new(a.cos() * max_r, a.sin() * max_r))
        .collect();
    painter.add(egui::Shape::closed_line(
        earth_pts,
        egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgba_premultiplied(0, 242, 255, 60),
        ),
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
        egui::Stroke::new(1.0_f32, ACCENT),
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

        ui.add_space(theme::Spacing::sm);

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
                format!("{} {:.1}%", gas.name, gas.percentage)
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
        egui::Stroke::new(1.0_f32, BORDER),
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
                theme::stat_row(ui, "LOCATION", "Beneath ice crust");
            } else {
                theme::stat_row(
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
            theme::stat_row(ui, "DEPTH", &depth_text);
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

// ─── Survey Ledger (SURVEY_REWORK.md §10) ────────────────────────────────

/// Render the dossier's top-level SURVEY ledger. Layout follows
/// `docs/design/SURVEY_REWORK.md` §10 verbatim:
///
/// ```text
/// Progress: 32% (target: 100%)
/// Scientists assigned: 3
/// Active missions: 2
/// Status: ORBITAL                    (read-only badge; GRA-107)
/// DIMENSIONS                          TIER   CONFIDENCE   BAR
/// ● Orbital mechanics                 5/5   98%  ▓▓▓▓▓
/// ● Atmosphere                        4/5   82%  ▓▓▓▓▓
/// …
/// RECOMMENDED NEXT STEP
/// → Surface lander at equatorial site
///   Targeting Subsurface; tier 0→2 over ~730 sim-days.
/// ACTIVE MISSIONS
/// • Orbital satellite "Mare Imbrium 1" (…)
/// FAILED MISSIONS                     (PR-G, hidden when empty)
/// • "Ariel-2" ROVER STUCK on sim-day 1,234
/// ANOMALIES DETECTED
/// • "Anomalous reflectance, suspected hydrated silicates" 65% SUSPECTED
/// DISPATCH MISSION
/// [pick template ▼] [DISPATCH]
/// ```
///
/// The deposit grid stays in the RESOURCES section (per the issue
/// out-of-scope note). This function reuses `draw_active_missions_list`
/// and `draw_failed_missions_list` so mission-state rendering lives in
/// one place across the dossier.
#[allow(clippy::too_many_arguments)]
fn draw_survey_section(
    ui: &mut egui::Ui,
    body: Entity,
    body_name: &str,
    state: &SurveyState,
    current_level: SurveyLevel,
    sim_time: f64,
    commands: &mut Commands,
    mission_templates: &SurveyMissionTemplates,
) {
    let progress_pct = (state.average_tier() * 100.0).round() as u32;
    let assigned_scientist_ids: HashSet<u64> = state
        .active_missions
        .iter()
        .flat_map(|m| m.assigned_scientists.iter().copied())
        .collect();
    let active_mission_count = state
        .active_missions
        .iter()
        .filter(|m| m.status.is_in_progress())
        .count();
    let candidate_count = state
        .detected_anomalies
        .iter()
        .filter(|anomaly| anomaly.state == AnomalyState::Suspected)
        .count();
    let verified_count = state
        .detected_anomalies
        .iter()
        .filter(|anomaly| anomaly.state == AnomalyState::Verified)
        .count();

    let (status_color, status_label) = match current_level {
        SurveyLevel::Unsurveyed => (TEXT_DIM, "UNSURVEYED"),
        SurveyLevel::OrbitalScan => (egui::Color32::LIGHT_BLUE, "ORBITAL"),
        SurveyLevel::SeismicSurvey => (egui::Color32::YELLOW, "SEISMIC"),
        SurveyLevel::CoreSample => (GREEN_ACCENT, "CORE SAMPLE"),
    };

    theme::section_h3(ui, "COVERAGE SUMMARY");
    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{progress_pct}% surveyed"))
                    .font(mono_font(11.0))
                    .color(TEXT_VALUE),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Scientists {}  •  Missions {}",
                    assigned_scientist_ids.len(),
                    active_mission_count
                ))
                .font(mono_font(9.0))
                .color(TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(status_label)
                        .font(mono_font(10.0))
                        .color(status_color),
                );
            });
        });
        ui.add_space(theme::Spacing::xs);

        for dim in SurveyDimension::ALL {
            let fidelity = state.fidelity(dim);
            draw_dimension_coverage_row(ui, dim, fidelity, state.active_missions.iter());
        }

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Anomalies")
                    .font(mono_font(10.0))
                    .color(TEXT_VALUE),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{} candidates  •  {} verified",
                    candidate_count, verified_count
                ))
                .font(mono_font(9.0))
                .color(TEXT_DIM),
            );
        });
    });

    // ── Recommended next step (LGD-coordinated heuristic) ────
    // Per GRA-112: the sophisticated helper scores templates by
    // tier-gap + confidence-deficit + cross-dim bonus + cost-time
    // penalty + roster-match. The dossier has no per-body roster
    // in scope yet (v0.5.0 ships with the dossier-side signature
    // widened; a future PR can plumb the per-body roster from
    // the personnel plugin). The dispatch picker in the Personnel
    // menu (not in this file) passes `Some(&on_station_roster)`.
    //
    // GRA-114: the helper now also returns a `ReasonTag` so the
    // dossier can tell the player *why* this mission is the pick.
    // The render is two lines: the duration tier line, then a
    // single "Reason: …" line. No new colors (the dossier SURVEY
    // branch is in the Color32 audit baseline).
    ui.add_space(theme::Spacing::xs);
    theme::section_h3(ui, "RECOMMENDED NEXT STEP");
    if let Some((dim, recommended, reason)) =
        recommended_survey_action(state, mission_templates, None)
    {
        let target_tier = recommended.target_tiers.get(&dim).copied().unwrap_or(1);
        ui.label(
            egui::RichText::new(format!(
                "Targeting {}; tier 0→{target_tier} over ~{} sim-days.",
                dim.display_name(),
                recommended.base_duration_days
            ))
            .font(mono_font(9.0))
            .color(TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(format!("Reason: {}", reason_text(&reason)))
                .font(mono_font(9.0))
                .color(TEXT_DIM),
        );
    } else {
        ui.colored_label(GREEN_ACCENT, "All dimensions adequately characterized.");
    }

    ui.add_space(theme::Spacing::sm);

    theme::section_h3(ui, "ACTIVE MISSIONS");
    draw_active_missions_list(ui, body, &state.active_missions, sim_time, commands);

    if !state.failed_mission_notifications.is_empty() {
        ui.add_space(theme::Spacing::xs);
        theme::section_h3(ui, "FAILED MISSIONS");
        draw_failed_missions_list(
            ui,
            body,
            &state.failed_mission_notifications,
            &state.active_missions,
            commands,
        );
    }

    ui.add_space(theme::Spacing::xs);
    theme::section_h3(ui, "ANOMALY LOG");
    draw_anomaly_log(ui, state);

    ui.add_space(theme::Spacing::xs);
    theme::section_h3(ui, "DISPATCH MISSION");
    draw_dispatch_mission_picker(ui, body, body_name, mission_templates, commands);
}

fn draw_dimension_coverage_row<'a>(
    ui: &mut egui::Ui,
    dim: SurveyDimension,
    fidelity: crate::survey::components::DimensionFidelity,
    missions: impl Iterator<Item = &'a ActiveSurveyMission>,
) {
    let active_progress = missions
        .filter(|mission| mission.status.is_in_progress())
        .filter_map(|mission| {
            mission
                .per_axis_progress
                .get(&dim)
                .copied()
                .map(|progress| {
                    let remaining_days =
                        mission.total_duration_seconds() * (1.0 - progress as f64) / 86_400.0;
                    (progress, remaining_days.max(0.0))
                })
        })
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let tier_text = if fidelity.tier >= MAX_TIER {
        format!("T{}", MAX_TIER)
    } else {
        format!("T{} → T{}", fidelity.tier, fidelity.tier + 1)
    };
    let eta_text = if fidelity.tier >= MAX_TIER {
        "—".to_string()
    } else if let Some((_, remaining_days)) = active_progress {
        format_sim_days(remaining_days)
    } else if fidelity.tier == 0 {
        "UNDISCOVERED".to_string()
    } else {
        "READY".to_string()
    };

    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, 18.0],
            egui::Label::new(
                egui::RichText::new(dim.display_name())
                    .font(mono_font(10.0))
                    .color(TEXT_VALUE),
            ),
        );
        draw_dimension_progress_bar(ui, fidelity.tier, active_progress.map(|p| p.0));
        ui.add_sized(
            [58.0, 18.0],
            egui::Label::new(
                egui::RichText::new(tier_text)
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            ),
        );
        ui.add_sized(
            [84.0, 18.0],
            egui::Label::new(
                egui::RichText::new(eta_text)
                    .font(mono_font(9.0))
                    .color(if fidelity.is_stale() { AMBER } else { TEXT_DIM }),
            ),
        );
    });
}

fn draw_dimension_progress_bar(ui: &mut egui::Ui, tier: u8, active_progress: Option<f32>) {
    let width = 92.0;
    let height = 10.0;
    let gap = 2.0;
    let segments = MAX_TIER as usize;
    let segment_width = (width - gap * (segments.saturating_sub(1) as f32)) / segments as f32;
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(width, height), egui::Sense::hover());

    for index in 0..segments {
        let min_x = rect.left() + index as f32 * (segment_width + gap);
        let segment = egui::Rect::from_min_size(
            egui::Pos2::new(min_x, rect.top()),
            egui::Vec2::new(segment_width, height),
        );
        ui.painter().rect_filled(segment, 2.0, BORDER_DIM);

        let fill = if (index as u8) < tier {
            1.0
        } else if (index as u8) == tier && tier < MAX_TIER {
            active_progress.unwrap_or(0.0).clamp(0.0, 1.0)
        } else {
            0.0
        };

        if fill > 0.0 {
            let fill_rect = egui::Rect::from_min_size(
                segment.min,
                egui::Vec2::new(segment.width() * fill, segment.height()),
            );
            let color = if fill < 1.0 {
                theme::with_alpha(ACCENT, 150)
            } else {
                ACCENT
            };
            ui.painter().rect_filled(fill_rect, 2.0, color);
        }

        ui.painter().rect_stroke(
            segment,
            2.0,
            egui::Stroke::new(1.0_f32, BORDER),
            egui::StrokeKind::Outside,
        );
    }
}

fn format_sim_days(days: f64) -> String {
    if days >= 365.0 {
        format!("{:.1} yr", days / 365.25)
    } else {
        format!("{:.0} d", days.max(1.0))
    }
}

fn draw_anomaly_log(ui: &mut egui::Ui, state: &SurveyState) {
    if state.detected_anomalies.is_empty() {
        ui.colored_label(TEXT_DIM, "No anomaly events logged yet.");
        return;
    }

    let mut anomalies: Vec<_> = state.detected_anomalies.iter().collect();
    anomalies.sort_by(|a, b| {
        b.last_updated_sim_time
            .partial_cmp(&a.last_updated_sim_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for anomaly in anomalies.into_iter().take(3) {
        let (glyph, color) = match anomaly.state {
            AnomalyState::Suspected => ("⚠", AMBER),
            AnomalyState::Verified => ("✓", GREEN_ACCENT),
            AnomalyState::Refuted => ("✗", RED_ACCENT),
            AnomalyState::Dormant => ("•", TEXT_DIM),
        };
        let provenance = anomaly
            .evidence
            .last()
            .map(|evidence| {
                format!(
                    "{}  •  sim-day {:.0}",
                    format_evidence_kind(evidence.kind),
                    evidence.sim_time / 86_400.0
                )
            })
            .unwrap_or_else(|| format!("sim-day {:.0}", anomaly.detected_sim_time / 86_400.0));

        ui.label(
            egui::RichText::new(format!(
                "{} {}  confidence {:.2}",
                glyph,
                title_case_words(anomaly.anomaly_type.ron_id()),
                anomaly.confidence
            ))
            .font(mono_font(10.0))
            .color(color),
        );
        ui.label(
            egui::RichText::new(provenance)
                .font(mono_font(9.0))
                .color(TEXT_DIM),
        );
    }
}

fn format_evidence_kind(kind: crate::survey::types::EvidenceKind) -> &'static str {
    match kind {
        crate::survey::types::EvidenceKind::DataPoint => "Detection",
        crate::survey::types::EvidenceKind::Verification => "Verified by mission",
        crate::survey::types::EvidenceKind::Refutation => "Refuted by mission",
        crate::survey::types::EvidenceKind::Reactivation => "Reactivated",
    }
}

/// Recommend a single mission template that targets the body's
/// lowest-fidelity dimension. Returns `None` if every dimension is
/// fully characterised or no template covers the lowest-fidelity
/// dim.
///
/// GRA-112: sophisticated LGD-coordinated `recommended_survey_action`
/// heuristic. Replaces the GRA-108 stub.
///
/// Per the LGD design brief (GRA-112):
/// 1. Pick the **primary dim** — the dimension with the lowest
///    `fidelity.tier` (ties broken by lowest confidence), with a
///    synthetic boost for warning-tier dims so they sort ahead of
///    healthy dims at the same tier.
/// 2. Filter candidate templates to those that actually target the
///    primary dim, then score each one against a weighted multi-
///    factor objective (tier-gap, confidence-deficit, cross-dim
///    bonus, cost-time penalty, roster-match bonus).
/// 3. Tie-break by tier-gap on the primary DESC, then by
///    `base_duration_days` ASC, then by `template.id` ASC for
///    deterministic modder-stable output.
/// 4. Return `None` if every dim is fully characterized, no
///    template covers an under-characterized dim, or the template
///    registry is empty.
///
/// GRA-114: the return shape widens to a 3-tuple that includes a
/// [`ReasonTag`] — the single most-applicable reason this template
/// won, in priority order:
/// 1. `SpecialistOnStation` — a roster scientist matches the
///    template's method AND contributes a non-zero roster bonus.
/// 2. `ConfidenceRescue` — primary dim's confidence is below
///    `WARNING_CONFIDENCE`.
/// 3. `CrossDim` — template covers ≥ 2 dimensions.
/// 4. `TierGap { from_tier, to_tier }` — a tier-gap win on the
///    primary dim.
/// 5. `BestFit` — fallback (zero score, zero gap).
fn recommended_survey_action<'a>(
    state: &SurveyState,
    mission_templates: &'a SurveyMissionTemplates,
    scientist_roster: Option<&[ScientistSummary]>,
) -> Option<(SurveyDimension, &'a SurveyMissionTemplate, ReasonTag)> {
    // Step 1: pick the primary dim. A warning-tier dim (confidence
    // below WARNING_CONFIDENCE) gets a synthetic -0.5 boost on its
    // sort key so it sorts ahead of a non-warning dim at the same
    // tier. Skips fully-characterized dims.
    const WARNING_BOOST: f32 = 0.5;
    let primary_sort_key = |d: SurveyDimension| -> (f32, f32) {
        let f = state.fidelity(d);
        let adj_conf = if f.confidence < WARNING_CONFIDENCE {
            f.confidence - WARNING_BOOST
        } else {
            f.confidence
        };
        (f.tier as f32, adj_conf)
    };
    let primary = SurveyDimension::ALL
        .iter()
        .copied()
        .filter(|d| !state.fidelity(*d).is_fully_characterized())
        .min_by(|a, b| {
            primary_sort_key(*a)
                .partial_cmp(&primary_sort_key(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

    let mut candidates: Vec<&SurveyMissionTemplate> = mission_templates
        .templates
        .values()
        .filter(|t| t.target_tiers.contains_key(&primary))
        .collect();
    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        let score_a = score_template(a, state, primary, scientist_roster);
        let score_b = score_template(b, state, primary, scientist_roster);
        let tier_gap_a = a
            .target_tiers
            .get(&primary)
            .copied()
            .unwrap_or(0)
            .saturating_sub(state.fidelity(primary).tier) as i32;
        let tier_gap_b = b
            .target_tiers
            .get(&primary)
            .copied()
            .unwrap_or(0)
            .saturating_sub(state.fidelity(primary).tier) as i32;

        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| tier_gap_b.cmp(&tier_gap_a))
            .then_with(|| a.base_duration_days.cmp(&b.base_duration_days))
            .then_with(|| a.id.cmp(&b.id))
    });

    let chosen = candidates.first().copied()?;
    let reason = select_reason(state, chosen, primary, scientist_roster);
    Some((primary, chosen, reason))
}

/// Pick the single most-applicable [`ReasonTag`] for why `chosen`
/// was recommended. Priority per the LGD GRA-114 design contract:
///
/// 1. `SpecialistOnStation` — at least one roster scientist's
///    specialty matches the template's method AND would contribute
///    a non-zero roster bonus (i.e. seniority throughput > 0).
/// 2. `ConfidenceRescue` — primary dim's confidence is below
///    `WARNING_CONFIDENCE`.
/// 3. `CrossDim` — template covers ≥ 2 dimensions.
/// 4. `TierGap { from_tier, to_tier }` — a tier-gap win on the
///    primary dim (gap > 0).
/// 5. `BestFit` — fallback (no other reason applies).
fn select_reason(
    state: &SurveyState,
    chosen: &SurveyMissionTemplate,
    primary: SurveyDimension,
    scientist_roster: Option<&[ScientistSummary]>,
) -> ReasonTag {
    // Priority 1: SpecialistOnStation.
    if let Some(roster) = scientist_roster {
        if let Some(specialty) = roster.iter().find_map(|s| {
            if s.specialty.matches_method(chosen.method)
                && s.seniority.throughput_multiplier() > 0.0
            {
                Some(s.specialty)
            } else {
                None
            }
        }) {
            return ReasonTag::SpecialistOnStation { specialty };
        }
    }

    // Priority 2: ConfidenceRescue.
    if state.fidelity(primary).confidence < WARNING_CONFIDENCE {
        return ReasonTag::ConfidenceRescue;
    }

    // Priority 3: CrossDim.
    if chosen.target_tiers.len() >= 2 {
        return ReasonTag::CrossDim;
    }

    // Priority 4: TierGap (only when the template actually closes
    // a tier on the primary dim; otherwise fall through to
    // BestFit).
    let current_tier = state.fidelity(primary).tier;
    if let Some(&target_tier) = chosen.target_tiers.get(&primary) {
        if target_tier > current_tier {
            return ReasonTag::TierGap {
                from_tier: current_tier,
                to_tier: target_tier,
            };
        }
    }

    // Priority 5: BestFit (fallback).
    ReasonTag::BestFit
}

/// Render a [`ReasonTag`] as the second line of the dossier
/// RECOMMENDED NEXT STEP block. Per the LGD GRA-114 design
/// contract: English-only for now, i18n-friendly (a future PR can
/// swap in `&'static str` + localization keys without changing the
/// enum shape).
fn reason_text(tag: &ReasonTag) -> String {
    match tag {
        ReasonTag::SpecialistOnStation { specialty } => {
            format!("Specialist on station ({})", specialty.display_name())
        }
        ReasonTag::ConfidenceRescue => "Rescues dim below confidence threshold".to_string(),
        ReasonTag::CrossDim => "Closes gaps on multiple dimensions at once".to_string(),
        ReasonTag::TierGap { from_tier, to_tier } => {
            format!("Closes largest gap (tier {}→{})", from_tier, to_tier)
        }
        ReasonTag::BestFit => "Best available fit".to_string(),
    }
}

fn score_template(
    template: &SurveyMissionTemplate,
    state: &SurveyState,
    primary: SurveyDimension,
    roster: Option<&[ScientistSummary]>,
) -> f32 {
    const W_TIER: f32 = 1.0;
    const W_CONF: f32 = 0.5;
    const W_DUPS: f32 = 0.25;
    const W_COST: f32 = 0.5;
    const W_ROSTER: f32 = 0.5;

    let tier_gap: f32 = template
        .target_tiers
        .iter()
        .map(|(d, target)| {
            let current = state.fidelity(*d).tier;
            target.saturating_sub(current) as f32
        })
        .sum();

    let conf_deficit: f32 = template
        .target_tiers
        .keys()
        .filter(|d| state.fidelity(**d).confidence < WARNING_CONFIDENCE)
        .count() as f32;

    let cross_dim_bonus: f32 = ((template.target_tiers.len() as f32) - 1.0).max(0.0) * 0.25;
    let cost_penalty: f32 = -(1.0_f32 + (template.base_duration_days as f32) / 365.0).ln() * 0.5;
    let roster_bonus: f32 = match roster {
        None => 0.0,
        Some(rs) => rs
            .iter()
            .map(|s| {
                if s.specialty.matches_method(template.method) {
                    1.5 * s.seniority.throughput_multiplier()
                } else {
                    0.0
                }
            })
            .fold(0.0_f32, f32::max),
    };

    let _ = primary;

    W_TIER * tier_gap
        + W_CONF * conf_deficit
        + W_DUPS * cross_dim_bonus
        + W_COST * cost_penalty
        + W_ROSTER * roster_bonus
}

// ─── Resource Grid ───────────────────────────────────────────────────────

/// Derive a [`SurveyLevel`] for the dossier status badge and the
/// resource-grid fidelity slicing.
///
/// v0.5.0: the source of truth is [`SurveyState`] (per-body multi-axis
/// dimensions). We map the state's `average_tier()` to a legacy enum
/// value so the deposit grid, the resource tile tooltips, and the
/// status label all show a consistent "ORBITAL / SEISMIC / CORE SAMPLE"
/// reading. The legacy `SurveyLevel` component is still attached to
/// bodies at game start (e.g. Earth → `CoreSample`) — we prefer it when
/// no `SurveyState` is present yet (Phase 1 migration window per
/// SURVEY_REWORK.md §15).
fn effective_survey_level(legacy: Option<SurveyLevel>, state: Option<&SurveyState>) -> SurveyLevel {
    if let Some(s) = state {
        let avg = s.average_tier();
        if avg >= 0.99 {
            SurveyLevel::CoreSample
        } else if avg >= 0.39 {
            SurveyLevel::SeismicSurvey
        } else if avg > 0.0 || s.has_active_missions() {
            SurveyLevel::OrbitalScan
        } else {
            legacy.unwrap_or(SurveyLevel::Unsurveyed)
        }
    } else {
        legacy.unwrap_or(SurveyLevel::Unsurveyed)
    }
}

/// Build a `SurveyState` view for the dossier when the body still
/// only carries a legacy `SurveyLevel` (Phase 1 migration window).
///
/// PR-F (GRA-117): the SURVEY ledger, the resource discovery summary,
/// and the per-tile tooltips all want a `&SurveyState` so the new
/// multi-axis code path is the only rendering branch. Before this
/// helper, legacy-only bodies silently lost the SURVEY section because
/// the UI gated it on `Option<&SurveyState>` and never inserted the
/// state itself. This helper keeps the migration promise — `SurveyLevel`
/// remains the disk-side adapter; the UI just projects it into the new
/// shape on the fly so the dossier stops hiding survey info.
///
/// Returns `None` for genuinely unsurveyed bodies (no legacy level
/// either) so callers can decide whether to render a placeholder.
fn fallback_survey_state(
    legacy: Option<SurveyLevel>,
    state: Option<&SurveyState>,
    sim_time: f64,
) -> Option<SurveyState> {
    if let Some(s) = state {
        return Some(s.clone());
    }
    let level = legacy?;
    if matches!(level, SurveyLevel::Unsurveyed) {
        return None;
    }
    Some(SurveyState::from_legacy_level(level, sim_time))
}

fn draw_resource_section(
    ui: &mut egui::Ui,
    entity: Entity,
    resources: &PlanetResources,
    current_level: SurveyLevel,
    rate_tracker: &ResourceRateTracker,
    survey_state: Option<&SurveyState>,
    view: DossierResourceView,
) {
    // PR-F (GRA-108): the survey-status badge, ACTIVE MISSIONS list,
    // FAILED MISSIONS, and DISPATCH MISSION picker all moved into the
    // new top-level `draw_survey_section` (SURVEY_REWORK.md §10). What
    // remains here is the data-driven deposit grid + reveal matrix +
    // summary. `current_level` is the pre-computed v0.5.0 effective
    // level shared with the SURVEY ledger so the two sections read
    // consistently. `survey_state` is still needed for the reveal
    // matrix's T1/T2/T3 gate logic (per GRA-110 / GRA-111).

    // PR-B (GRA-67) — `theme::section_h3` introduces each sub-section.
    // The "By Category" tab keeps the existing visual identity; the
    // "Compact" tab swaps in a flat one-line-per-deposit list.
    match view {
        DossierResourceView::ByCategory => {
            theme::section_h3(ui, "DEPOSITS");
            draw_resource_grid(
                ui,
                resources,
                current_level,
                survey_state,
                rate_tracker,
                entity,
            );
        }
        DossierResourceView::Compact => {
            theme::section_h3(ui, "DEPOSITS \u{2014} COMPACT");
            draw_resource_compact(
                ui,
                resources,
                current_level,
                survey_state,
                rate_tracker,
                entity,
            );
        }
    }

    if matches!(view, DossierResourceView::ByCategory) {
        ui.add_space(theme::Spacing::xs);
        draw_resource_survey_summary(ui, resources, survey_state);
    }

    // Summary line — `section_h3` frames the totals so the visual
    // hierarchy reads as [RESOURCES] → [DEPOSITS] → [REVEAL MATRIX] → [SUMMARY].
    ui.add_space(theme::Spacing::xs);
    theme::section_h3(ui, "SUMMARY");
    ui.label(
        egui::RichText::new(format!(
            "VIABLE {}  \u{2502}  VALUE {:.1}",
            resources.viable_count(),
            resources.total_value()
        ))
        .font(mono_font(10.0))
        .color(TEXT_VALUE),
    );
}

/// Render the active-mission list for the dossier SURVEY section.
///
/// PR-F (GRA-117): the list now splits into in-progress and
/// recently-completed sections. In-progress missions show a
/// progress bar + status + ABORT button. Completed missions
/// (Succeeded / Failed / Aborted) show a result summary
/// (dimensions advanced, drill-mission flag, completion
/// timestamp) and a DISMISS button — the player can clear them
/// immediately, or wait for the auto-archive sweep after
/// `ARCHIVE_LINGER_DAYS` sim-days. Aborts fire an
/// `AbortSurveyMission` message — the actual removal is
/// performed by the sim system, not the UI (action-queue
/// decoupling).
fn draw_active_missions_list(
    ui: &mut egui::Ui,
    body: Entity,
    missions: &[ActiveSurveyMission],
    sim_time: f64,
    commands: &mut Commands,
) {
    let in_progress: Vec<&ActiveSurveyMission> = missions
        .iter()
        .filter(|m| m.status.is_in_progress())
        .collect();
    let completed: Vec<&ActiveSurveyMission> = missions
        .iter()
        .filter(|m| m.status.is_terminal() && !m.dismissed)
        .collect();

    if in_progress.is_empty() && completed.is_empty() {
        ui.colored_label(TEXT_DIM, "No missions in progress.");
        return;
    }

    if !in_progress.is_empty() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("In flight")
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            );
        });
        for mission in &in_progress {
            draw_in_progress_mission_row(ui, body, mission, commands);
        }
    }

    if !completed.is_empty() {
        if !in_progress.is_empty() {
            ui.add_space(theme::Spacing::xs);
        }
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Recent results")
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            );
        });
        for mission in &completed {
            draw_completed_mission_row(ui, body, mission, sim_time, commands);
        }
    }
}

/// One row of the in-progress section: progress bar + status +
/// ABORT.
fn draw_in_progress_mission_row(
    ui: &mut egui::Ui,
    body: Entity,
    mission: &ActiveSurveyMission,
    commands: &mut Commands,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&mission.name)
                .font(mono_font(11.0))
                .color(TEXT_VALUE),
        );
        ui.label(
            egui::RichText::new(mission.method.ron_id())
                .font(mono_font(9.0))
                .color(TEXT_DIM),
        );
    });

    let progress = mission.progress.clamp(0.0, 1.0);
    let bar = egui::ProgressBar::new(progress)
        .desired_width(ui.available_width())
        .text(format!("{:.0}%", progress * 100.0));
    ui.add(bar);

    ui.horizontal(|ui| {
        let (status_color, status_label) = match mission.status {
            MissionStatus::Queued => (TEXT_DIM, "QUEUED"),
            MissionStatus::Inflight => (egui::Color32::LIGHT_BLUE, "INFLIGHT"),
            MissionStatus::Active => (ACCENT, "ACTIVE"),
            MissionStatus::Completing => (AMBER, "COMPLETING"),
            // Terminal states are rendered in
            // `draw_completed_mission_row`; this match arm is
            // unreachable in practice but kept exhaustive.
            MissionStatus::Succeeded | MissionStatus::Failed | MissionStatus::Aborted => {
                (TEXT_DIM, "TERMINAL")
            }
        };
        ui.label(
            egui::RichText::new(status_label)
                .font(mono_font(9.0))
                .color(status_color),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if mission.status.is_in_progress()
                && ui
                    .small_button(
                        egui::RichText::new("\u{26D4} ABORT")
                            .font(mono_font(9.0))
                            .color(RED_ACCENT),
                    )
                    .clicked()
            {
                commands.write_message(AbortSurveyMission {
                    body,
                    mission_id: mission.id,
                });
            }
        });
    });

    ui.add_space(4.0);
}

/// One row of the recent-results section: status badge, result
/// summary (dimensions advanced, drill flag, recovery link),
/// completion timestamp, and a DISMISS button.
///
/// PR-F (GRA-117): the summary tells the player what the
/// mission actually *did* — without it the dossier just said
/// "SUCCEEDED" which was opaque (no idea what was found or what
/// benefit the mission produced). For a `Succeeded` mission we
/// list the dimensions it advanced to tier N+1; for a `Failed`
/// mission we surface the failure reason (when known) and the
/// linked recovery mission (when auto-spawned).
fn draw_completed_mission_row(
    ui: &mut egui::Ui,
    body: Entity,
    mission: &ActiveSurveyMission,
    sim_time: f64,
    commands: &mut Commands,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&mission.name)
                    .font(mono_font(11.0))
                    .color(TEXT_VALUE),
            );
            ui.label(
                egui::RichText::new(mission.method.ron_id())
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(
                        egui::RichText::new("DISMISS")
                            .font(mono_font(9.0))
                            .color(TEXT_DIM),
                    )
                    .clicked()
                {
                    commands.write_message(DismissSurveyMission {
                        body,
                        mission_id: mission.id,
                    });
                }
            });
        });

        ui.horizontal(|ui| {
            let (status_color, status_label) = match mission.status {
                MissionStatus::Succeeded => (GREEN_ACCENT, "SUCCEEDED"),
                MissionStatus::Failed => (RED_ACCENT, "FAILED"),
                MissionStatus::Aborted => (RED_ACCENT, "ABORTED"),
                _ => (TEXT_DIM, "TERMINAL"),
            };
            ui.label(
                egui::RichText::new(status_label)
                    .font(mono_font(9.0))
                    .color(status_color),
            );
            if let Some(t) = mission.completed_sim_time {
                let elapsed_days = ((sim_time - t) / 86_400.0).max(0.0);
                ui.label(
                    egui::RichText::new(format!("{:.0} d ago", elapsed_days))
                        .font(mono_font(9.0))
                        .color(TEXT_DIM),
                );
            }
        });

        // Result summary line.
        let summary = mission_result_summary(mission);
        ui.label(
            egui::RichText::new(summary)
                .font(mono_font(9.0))
                .color(TEXT_VALUE),
        );

        ui.add_space(2.0);
    });

    ui.add_space(4.0);
}

/// Build the human-readable "what did this mission do" line for a
/// terminal mission. PR-F (GRA-117). For `Succeeded` missions we
/// list the dimensions advanced; for `Failed` / `Aborted` we say
/// "no data returned" so the player knows the result was zero.
fn mission_result_summary(mission: &ActiveSurveyMission) -> String {
    let dim_count = mission.per_axis_progress.len();
    match mission.status {
        MissionStatus::Succeeded => {
            if dim_count == 0 {
                return "Mission complete. No dimensions advanced.".to_string();
            }
            let dims: Vec<String> = mission
                .per_axis_progress
                .keys()
                .map(|d| d.ron_id().to_string())
                .collect();
            let prefix = "Advanced";
            let drill_suffix = if mission.method == crate::survey::types::SurveyMethod::Drill {
                "  \u{2022}  Drill gate opened (+1 to T3 unlock)"
            } else {
                ""
            };
            format!(
                "{prefix} {} dimension{}{drill_suffix}",
                dims.join(", "),
                if dim_count == 1 { "" } else { "s" }
            )
        }
        MissionStatus::Failed | MissionStatus::Aborted => {
            "No data returned. See FAILED MISSIONS for recovery options.".to_string()
        }
        _ => String::new(),
    }
}

/// Render the failed-mission notification cards (PR-G, GRA-85).
///
/// Each card shows:
/// - The mission's name + method (small dim text)
/// - The failure reason as a red badge (e.g. "ROVER STUCK")
/// - The sim-day timestamp the mission failed
/// - One or more action buttons on the right:
///   - **ACCEPT LOSS** — always present. Fires
///     `DismissFailedMission` to remove the card.
///   - **DISPATCH RECOVERY** — only when the failure mode
///     named a `recovery_mission_id` AND the record's
///     `recovery_mission_active_id` is `None` (i.e. the
///     auto-spawn path didn't fire, or the player wants to
///     re-dispatch after a recovery was aborted). Fires a
///     `DispatchSurveyMission` event for the recovery
///     template id.
///   - **RECOVERED** — read-only badge shown when the linked
///     recovery mission's id resolves to a `Succeeded`
///     mission in the body's `active_missions` (the recovery
///     succeeded; the original was flipped to `Succeeded`
///     too).
fn draw_failed_missions_list(
    ui: &mut egui::Ui,
    body: Entity,
    records: &[FailedMissionRecord],
    active_missions: &[ActiveSurveyMission],
    commands: &mut Commands,
) {
    for rec in records {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&rec.display_name)
                    .font(mono_font(11.0))
                    .color(TEXT_VALUE),
            );
            ui.label(
                egui::RichText::new(rec.method.ron_id())
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            );
        });
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format_failure_reason(rec.reason))
                    .font(mono_font(9.0))
                    .color(RED_ACCENT),
            );
            ui.label(
                egui::RichText::new(format!("on sim-day {:.0}", rec.failed_sim_time / 86_400.0))
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            );
        });
        // Recovery description (if the failure mode names one).
        if let Some(desc) = &rec.recovery_mission_display_name {
            ui.label(
                egui::RichText::new(format!("Recovery: {desc}"))
                    .font(mono_font(9.0))
                    .color(TEXT_DIM),
            );
        }

        ui.horizontal(|ui| {
            // Action buttons sit on the right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // ACCEPT LOSS — always present.
                if ui
                    .small_button(
                        egui::RichText::new("ACCEPT LOSS")
                            .font(mono_font(9.0))
                            .color(RED_ACCENT),
                    )
                    .clicked()
                {
                    commands.write_message(DismissFailedMission {
                        body,
                        mission_id: rec.mission_id,
                    });
                }
                // DISPATCH RECOVERY — only when the record names a
                // recovery template and no recovery is currently
                // active. If the recovery has already succeeded,
                // show a "RECOVERED" badge instead.
                if let Some(recovery_id) = rec.recovery_mission_active_id {
                    if let Some(rmission) = active_missions.iter().find(|m| m.id == recovery_id) {
                        if rmission.status == MissionStatus::Succeeded {
                            ui.small_button(
                                egui::RichText::new("\u{2713} RECOVERED")
                                    .font(mono_font(9.0))
                                    .color(GREEN_ACCENT),
                            )
                            .on_disabled_hover_text(
                                "Recovery mission succeeded; original flipped to Succeeded.",
                            );
                        } else {
                            // Recovery still in flight — no button
                            // (the player can ABORT the active
                            // recovery from the ACTIVE MISSIONS
                            // list above).
                            ui.label(
                                egui::RichText::new("RECOVERY IN FLIGHT")
                                    .font(mono_font(9.0))
                                    .color(AMBER),
                            );
                        }
                    }
                } else if let Some(recovery_template_id) = &rec.recovery_mission_id {
                    if ui
                        .small_button(
                            egui::RichText::new("\u{25B6} DISPATCH RECOVERY")
                                .font(mono_font(9.0))
                                .color(ACCENT),
                        )
                        .clicked()
                    {
                        let name = format!("{} Recovery", rec.display_name);
                        commands.write_message(DispatchSurveyMission {
                            body,
                            template_id: recovery_template_id.clone(),
                            name,
                            scientist_ids: vec![],
                        });
                    }
                }
            });
        });

        ui.add_space(4.0);
    }
}

/// Short upper-case badge label for a failure reason.
fn format_failure_reason(reason: MissionFailureReason) -> &'static str {
    match reason {
        MissionFailureReason::ProbeLoss => "PROBE LOST",
        MissionFailureReason::RoverStuck => "ROVER STUCK",
        MissionFailureReason::DrillBitStuck => "DRILL BIT STUCK",
        MissionFailureReason::SolarStorm => "SOLAR STORM",
        MissionFailureReason::CrewInjury => "CREW INJURED",
    }
}

/// Render the dispatch-mission picker: a combo box of available
/// templates, plus a DISPATCH button. The selected template id and a
/// per-body counter live in `egui::data` so the choice persists across
/// frames. Pressing DISPATCH writes a `DispatchSurveyMission` message
/// that the sim system consumes.
fn draw_dispatch_mission_picker(
    ui: &mut egui::Ui,
    body: Entity,
    body_name: &str,
    mission_templates: &SurveyMissionTemplates,
    commands: &mut Commands,
) {
    if mission_templates.templates.is_empty() {
        ui.colored_label(TEXT_DIM, "No mission templates loaded.");
        return;
    }

    let template_id_key = egui::Id::new(("dispatch_template_id", body));
    let counter_key = egui::Id::new(("dispatch_counter", body));

    // Default to the first template's id on first render per body.
    let mut selected_id: String = ui.data(|d| d.get_temp(template_id_key)).unwrap_or_else(|| {
        mission_templates
            .templates
            .keys()
            .next()
            .cloned()
            .unwrap_or_default()
    });
    let counter: u32 = ui.data(|d| d.get_temp(counter_key)).unwrap_or(1);

    let prev_selected = selected_id.clone();
    egui::ComboBox::from_id_salt(("dispatch_combo", body))
        .selected_text(
            mission_templates
                .templates
                .get(&selected_id)
                .map(|t| t.display_name.clone())
                .unwrap_or_else(|| selected_id.clone()),
        )
        .show_ui(ui, |ui| {
            for (id, tpl) in &mission_templates.templates {
                let label = format!("{}  ({} d)", tpl.display_name, tpl.base_duration_days);
                ui.selectable_value(&mut selected_id, id.clone(), label);
            }
        });
    let _ = prev_selected;

    ui.add_space(2.0);

    if ui
        .add(
            egui::Button::new(
                egui::RichText::new("\u{25B6}  DISPATCH")
                    .font(mono_font(11.0))
                    .color(ACCENT),
            )
            .min_size(egui::Vec2::new(140.0, 24.0)),
        )
        .clicked()
    {
        let name = format!("{body_name} Mission {counter}");
        commands.write_message(DispatchSurveyMission {
            body,
            template_id: selected_id.clone(),
            name,
            scientist_ids: vec![],
        });
        ui.data_mut(|d| d.insert_temp(counter_key, counter + 1));
    }

    ui.data_mut(|d| d.insert_temp(template_id_key, selected_id));
}

/// Flat sorted list of mineable deposits. Demonstrates the
/// `DossierResourceView::Compact` tab alternative — each deposit is
/// rendered as one row: symbol, name, viable mass, monthly balance.
fn draw_resource_compact(
    ui: &mut egui::Ui,
    resources: &PlanetResources,
    survey_level: SurveyLevel,
    survey_state: Option<&SurveyState>,
    rate_tracker: &ResourceRateTracker,
    entity: Entity,
) {
    let mut rows: Vec<(ResourceType, f64)> = Vec::new();
    // GRA-120 (range render): prefer the v0.5.0 `SurveyState`
    // fidelity for the MineralDeposits axis and route the deposit
    // through `estimate_with_fidelity`. The legacy
    // `SurveyLevel::discovered_amount` returns the proven/deep/
    // bulk split as a single scalar, but the v0.5.0 fidelity
    // already includes the (low, mid, high) range the UI is
    // supposed to render. `mid_or_zero()` is the equivalent scalar
    // for legacy callers — the cell renderer stays the same.
    let fidelity = survey_state
        .map(|s| s.fidelity(crate::survey::SurveyDimension::MineralDeposits))
        .unwrap_or_else(|| survey_level.as_deposit_fidelity(0.0));
    for (_category, items) in ResourceType::by_category() {
        for r in &items {
            if !r.is_mineable() {
                continue;
            }
            if let Some(d) = resources.get_deposit(r) {
                if d.reserve.total_mass() > 0.001 {
                    let discovered =
                        crate::survey::estimate_with_fidelity(d, fidelity).mid_or_zero();
                    rows.push((*r, discovered));
                }
            }
        }
    }
    // Sort by discovered mass descending so the most important deposits
    // appear first; secondary sort by symbol for determinism.
    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if rows.is_empty() {
        ui.colored_label(TEXT_DIM, "No deposits visible at current survey level.");
        return;
    }

    egui::Grid::new("dossier_resource_compact")
        .num_columns(4)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            for (resource, discovered) in rows {
                let sym = resource.symbol();
                let name = resource.display_name();
                let mass = format_mass_compact(discovered);
                let rate = rate_tracker.get_entity_resource_rate(entity, &resource);
                let (rate_text, _rate_color) = format_rate_monthly(rate);
                ui.label(egui::RichText::new(sym).font(mono_font(11.0)).color(ACCENT));
                ui.label(
                    egui::RichText::new(name)
                        .font(mono_font(10.0))
                        .color(TEXT_VALUE),
                );
                ui.label(
                    egui::RichText::new(mass)
                        .font(mono_font(10.0))
                        .color(TEXT_VALUE),
                );
                // v3.8.11 (2026-08-07): attach a hover tooltip that
                // breaks the rate into production / per-cap / maint /
                // synthesis input. Without this, the +X.X Mt/mo number
                // is opaque — the user has no way to know whether the
                // rate is being eaten by a per-cap draw, by building
                // maintenance, or (the v3.8.0-v3.8.10 bug for Methane)
                // by a synthesis-input draw that was being added
                // instead of subtracted.
                //
                // The tooltip also flags when this resource is
                // consumed as an industrial-process input (e.g. Methane
                // → PolymerSynthesis). Cap / reserve throttling notes
                // are skipped here (the resources bar already shows
                // the per-category cap lock icon) — this surface
                // focuses on the *composition* of the rate.
                let tooltip_text = rate_tooltip(&resource, rate_tracker, f64::MAX, &[]);
                ui.label(
                    egui::RichText::new(rate_text)
                        .font(mono_font(9.0))
                        .color(TEXT_DIM),
                )
                .on_hover_text(tooltip_text);
                ui.end_row();
            }
        });
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
                        egui::Stroke::new(0.5_f32, theme::BORDER_DIM),
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
        egui::Stroke::new(1.0_f32, BORDER),
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
    survey_state: Option<&SurveyState>,
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
                    survey_state,
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

fn draw_resource_survey_summary(
    ui: &mut egui::Ui,
    resources: &PlanetResources,
    survey_state: Option<&SurveyState>,
) {
    let Some(state) = survey_state else {
        return;
    };

    let total_deposits = resources
        .deposits
        .values()
        .filter(|deposit| deposit.reserve.total_mass() > 0.001)
        .count();
    if total_deposits == 0 {
        return;
    }

    let atmosphere_fidelity = state.fidelity(SurveyDimension::Atmosphere);
    let mineral_fidelity = state.fidelity(SurveyDimension::MineralDeposits);
    let subsurface_fidelity = state.fidelity(SurveyDimension::Subsurface);
    let anomaly_fidelity = state.fidelity(SurveyDimension::Anomalies);

    let atmospheric_deposits = resources
        .deposits
        .values()
        .filter(|deposit| deposit.is_atmospheric && deposit.reserve.total_mass() > 0.001)
        .count();
    let surface_deposits = resources
        .deposits
        .values()
        .filter(|deposit| !deposit.is_atmospheric && deposit.reserve.proven_crustal > 0.001)
        .count();
    let deep_deposits = resources
        .deposits
        .values()
        .filter(|deposit| !deposit.is_atmospheric && deposit.reserve.deep_deposits > 0.001)
        .count();
    let bulk_deposits = resources
        .deposits
        .values()
        .filter(|deposit| !deposit.is_atmospheric && deposit.reserve.planetary_bulk > 0.001)
        .count();

    theme::section_h3(ui, "DISCOVERY STATUS");
    theme::elevated_frame().show(ui, |ui| {
        draw_resource_discovery_row(
            ui,
            "Atmosphere",
            atmosphere_fidelity,
            &format!("{} atmospheric resources modelled", atmospheric_deposits),
        );
        draw_resource_discovery_row(
            ui,
            "Surface deposits",
            mineral_fidelity,
            &format!(
                "{} of {} resources have near-surface reserves",
                surface_deposits, total_deposits
            ),
        );
        draw_resource_discovery_row(
            ui,
            "Deep structure",
            subsurface_fidelity,
            &format!("{} deep-resource classes identified", deep_deposits),
        );
        ui.horizontal(|ui| {
            ui.add_sized(
                [110.0, 18.0],
                egui::Label::new(
                    egui::RichText::new("Planetary bulk")
                        .font(mono_font(10.0))
                        .color(TEXT_VALUE),
                ),
            );
            ui.label(
                egui::RichText::new(if state.planetary_bulk_unlocked() {
                    format!("Drill verified  •  {} bulk classes unlocked", bulk_deposits)
                } else {
                    "Requires Subsurface T5 + one drill mission".to_string()
                })
                .font(mono_font(9.0))
                .color(if state.planetary_bulk_unlocked() {
                    GREEN_ACCENT
                } else {
                    TEXT_DIM
                }),
            );
        });
        draw_resource_discovery_row(
            ui,
            "Anomaly leads",
            anomaly_fidelity,
            &format!("{} logged events", state.detected_anomalies.len()),
        );
    });
}

fn draw_resource_discovery_row(
    ui: &mut egui::Ui,
    label: &str,
    fidelity: crate::survey::components::DimensionFidelity,
    detail: &str,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, 18.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .font(mono_font(10.0))
                    .color(TEXT_VALUE),
            ),
        );
        ui.label(
            egui::RichText::new(format!("T{}/{}", fidelity.tier, MAX_TIER))
                .font(mono_font(9.0))
                .color(if fidelity.is_stale() { AMBER } else { TEXT_DIM }),
        );
        ui.label(
            egui::RichText::new(detail)
                .font(mono_font(9.0))
                .color(TEXT_DIM),
        );
    });
}

/// Format a concentration value as a percentage / ppm / ppb / e-format
/// string. Mirrors the `conc_text` block at
/// `dossier_panel.rs:2134-2142` so the matrix matches the existing
/// dossier tooltip formatting.
fn format_concentration(conc: f32) -> String {
    let conc = conc.clamp(0.0, 1.0);
    if conc >= 0.01 {
        format!("{:.1}%", conc * 100.0)
    } else if conc >= 0.000_01 {
        format!("{:.1} ppm", conc * 1_000_000.0)
    } else if conc >= 0.000_000_01 {
        format!("{:.2} ppb", conc * 1_000_000_000.0)
    } else {
        format!("{conc:.2e}")
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
    survey_state: Option<&SurveyState>,
    size: f32,
    cat_color: egui::Color32,
    rate_tracker: &ResourceRateTracker,
    entity: Entity,
) {
    let has_deposit = deposit.is_some_and(|d| d.reserve.total_mass() > 0.001);
    // GRA-120 (range render): compute the estimate once for both
    // the tile and the hover tooltip so the two readouts agree.
    // The legacy `discovered_amount` returns a scalar only; the
    // v0.5.0 fidelity already encodes the (low, mid, high) range
    // the cell renderer is supposed to show. `mid_or_zero()` is
    // the scalar the existing `paint_resource_tile` cell expects.
    let fidelity = survey_state
        .map(|s| s.fidelity(crate::survey::SurveyDimension::MineralDeposits))
        .unwrap_or_else(|| survey_level.as_deposit_fidelity(0.0));
    let response = if let Some(d) = deposit.filter(|d| d.reserve.total_mass() > 0.001) {
        let discovered = crate::survey::estimate_with_fidelity(d, fidelity).mid_or_zero();
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
        // GRA-120 (range render): reuse the fidelity computed above
        // so the tooltip mid matches the tile mid. See comment
        // above the `fidelity` binding for the back-compat chain.
        let discovered = crate::survey::estimate_with_fidelity(d, fidelity).mid_or_zero();
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
                .stroke(egui::Stroke::new(1.0_f32, ACCENT_DIM))
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

                        let total_potential = d.reserve.total_mass();
                        tooltip_row(ui, "Potential", &format_mass(total_potential));

                        if let Some(state) = survey_state {
                            if d.is_atmospheric {
                                let atmosphere = state.fidelity(SurveyDimension::Atmosphere);
                                tooltip_row(
                                    ui,
                                    "Survey",
                                    &format!(
                                        "Atmosphere T{}/{}  •  {:.0}% confidence",
                                        atmosphere.tier,
                                        MAX_TIER,
                                        atmosphere.confidence * 100.0
                                    ),
                                );
                            } else {
                                let mineral = state.fidelity(SurveyDimension::MineralDeposits);
                                let subsurface = state.fidelity(SurveyDimension::Subsurface);
                                tooltip_row(
                                    ui,
                                    "Survey",
                                    &format!(
                                        "Minerals T{}/{}  •  Subsurface T{}/{}",
                                        mineral.tier, MAX_TIER, subsurface.tier, MAX_TIER
                                    ),
                                );
                                tooltip_row(
                                    ui,
                                    "Bulk gate",
                                    if state.planetary_bulk_unlocked() {
                                        "Drill verified"
                                    } else {
                                        "Needs Subsurface T5 + drill"
                                    },
                                );
                            }
                        } else {
                            let legacy_label = match survey_level {
                                SurveyLevel::Unsurveyed => "Unsurveyed",
                                SurveyLevel::OrbitalScan => "Orbital scan",
                                SurveyLevel::SeismicSurvey => "Seismic survey",
                                SurveyLevel::CoreSample => "Core sample",
                            };
                            tooltip_row(ui, "Survey", legacy_label);
                        }

                        if !d.is_atmospheric {
                            let conc = d.reserve.concentration;
                            let conc_text = format_concentration(conc);
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
    buildings_data: &crate::colony::data::BuildingsData,
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
        let housing = colony.housing_capacity(buildings_data);
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

// ─── GRA-83b: Orbital Survey Station dossier surface ────────────────────

/// Per-body summary for the orbital survey station section.
///
/// Pure data — no egui types — so the unit test in this module
/// can construct a `Summary` from mocked components and verify
/// the per-year rate / mining bonus strings without spinning up
/// the egui context. The render function [`draw_orbital_station_section`]
/// consumes the summary and lays out the rows.
///
/// The per-year rate and mining bonus are read from the body's
/// `ContinuousStationBonus` cache (already on the body) so the
/// dossier stays decoupled from tier→rate mapping — the LGD
/// owns the tier table, and the dossier reads what the system
/// computed. See `apply_continuous_station_bonus`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OrbitalStationSummary {
    /// Display name of the orbited body (e.g. "Mars"). Echoed in
    /// the section header.
    pub body_name: String,
    /// Tiers of every station orbiting the body, in the order the
    /// query returned them. The dossier renders this as a
    /// comma-separated list ("T1, T1, T3").
    pub tiers: Vec<u8>,
    /// Combined axis-advance rate (sum across all stations), in
    /// axes per sim-year. Read from
    /// `ContinuousStationBonus::axis_advance_per_year`.
    pub per_year_rate: f32,
    /// Combined mining bonus percent, derived from
    /// `ContinuousStationBonus::mining_yield_multiplier - 1.0`.
    /// E.g. `1.05` → `5` (`+5%`).
    pub mining_bonus_pct: i32,
    /// Count of active mining operations on the body. Used for
    /// the "on N local mines" line in the dossier.
    pub mine_count: usize,
    /// Visibility note from the LGD spec — surfaced verbatim in
    /// the section so the player understands the per-year rate
    /// is informational while the cumulative integer tier ticks.
    pub visibility_note: &'static str,
}

impl OrbitalStationSummary {
    /// LGD-supplied visibility note copy.
    pub const VISIBILITY_NOTE: &'static str =
        "Survey accumulation is fractional until the integer tier ticks. The per-year \
         rate is shown so the dossier reflects active work, not just the cumulative integer.";

    /// Build a summary from the relevant live data, or `None` if
    /// the body has no active orbital survey station cache. The
    /// dossier hides the section when this returns `None`.
    pub fn from_components<'w, I>(
        bonus: Option<&ContinuousStationBonus>,
        mining_operation: Option<&MiningOperation>,
        body_entity: Entity,
        body_name: &str,
        stations: I,
    ) -> Option<Self>
    where
        I: IntoIterator<Item = &'w ContinuousSurveyStation>,
    {
        let bonus = bonus?;
        if !bonus.is_active() {
            // The cache may be present on a body but at neutral
            // values (the system resets rather than removes — see
            // `apply_continuous_station_bonus`). Treat neutral as
            // "no station" for the dossier.
            return None;
        }
        let mut tiers: Vec<u8> = stations
            .into_iter()
            .filter_map(|s| {
                if s.orbiting_body == Some(body_entity) {
                    Some(s.tier)
                } else {
                    None
                }
            })
            .collect();
        // Stable, ascending order for predictable display
        // ("T1, T1, T3" not "T3, T1, T1").
        tiers.sort_unstable();
        let mine_count = mining_operation
            .map(|op| if op.active { 1 } else { 0 })
            .unwrap_or(0);
        let mining_bonus_pct = ((bonus.mining_yield_multiplier - 1.0) * 100.0).round() as i32;
        Some(Self {
            body_name: body_name.to_string(),
            tiers,
            per_year_rate: bonus.axis_advance_per_year,
            mining_bonus_pct,
            mine_count,
            visibility_note: Self::VISIBILITY_NOTE,
        })
    }

    /// LGD-supplied per-tier display label, e.g. "T1" for tier 1.
    /// Unknown tiers render as "T?" so a buggy RON entry doesn't
    /// silently produce an empty label.
    pub fn tier_label(tier: u8) -> String {
        match tier {
            1..=3 => format!("T{tier}"),
            _ => "T?".to_string(),
        }
    }

    /// Comma-separated tier list, e.g. "T1, T1, T3". Empty if
    /// no stations (which `from_world` already filters out).
    pub fn tier_list(&self) -> String {
        self.tiers
            .iter()
            .map(|t| Self::tier_label(*t))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The always-visible per-year rate string, e.g. "0.05 axes/yr".
    /// Two-decimal fixed format — the LGD tier table produces values
    /// in `{0.05, 0.075, 0.10, 0.15, ...}` so two decimals
    /// preserves the precision a player needs.
    pub fn rate_label(&self) -> String {
        format!("{:.2} axes/yr", self.per_year_rate)
    }

    /// The mining bonus string, e.g. "+5%". 0% renders as "+0%".
    pub fn bonus_label(&self) -> String {
        format!("+{}%", self.mining_bonus_pct)
    }
}

fn draw_orbital_station_section<'w, I>(
    ui: &mut egui::Ui,
    bonus: Option<&ContinuousStationBonus>,
    mining_operation: Option<&MiningOperation>,
    stations: I,
    body_entity: Entity,
    body_name: &str,
) where
    I: IntoIterator<Item = &'w ContinuousSurveyStation>,
{
    let Some(summary) = OrbitalStationSummary::from_components(
        bonus,
        mining_operation,
        body_entity,
        body_name,
        stations,
    ) else {
        // No active station — hide the section entirely. The
        // body dossier would otherwise grow a useless empty
        // "ORBITAL SURVEY STATIONS" header on every body that
        // doesn't have a station.
        return;
    };

    theme::section_h2(ui, "ORBITAL SURVEY STATIONS");

    // "Orbited body" — the body's own name. Redundant with the
    // dossier header, but LGD spec calls for it explicitly and
    // it confirms the player is looking at the right surface
    // when scanning.
    stat_row(ui, "ORBITED BODY", &summary.body_name);

    // Tiers — comma-separated list of station tiers orbiting the
    // body.
    stat_row(ui, "TIERS", &summary.tier_list());

    // Per-year rate — the LGD's primary motivation for this
    // section: the cumulative integer tier truncates 0.05/yr to
    // 0 for ~20 sim-years, so a player without this line would
    // conclude the station is broken. ALWAYS visible.
    stat_row(ui, "SURVEY RATE", &summary.rate_label());

    // Mining bonus — combined multiplier converted to a percent.
    stat_row(
        ui,
        "MINING BONUS",
        &format!(
            "{} on {} local mine{}",
            summary.bonus_label(),
            summary.mine_count,
            if summary.mine_count == 1 { "" } else { "s" }
        ),
    );

    ui.add_space(4.0);
    ui.colored_label(
        TEXT_DIM,
        egui::RichText::new(summary.visibility_note).small(),
    );
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .font(mono_font(10.0))
                .color(TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(value)
                .font(mono_font(11.0))
                .color(TEXT_VALUE),
        );
    });
}

#[cfg(test)]
mod tests {
    //! GRA-83b unit tests for the dossier surface.
    //!
    //! Pure-data tests on [`OrbitalStationSummary`] — no egui
    //! context. The render function is verified manually in-game
    //! (the smallest in-game check from the issue body).
    //!
    //! GRA-112 (2026-06-13) extends the suite with the 5-test
    //! acceptance contract from the LGD design brief for the
    //! sophisticated `recommended_survey_action` heuristic, plus
    //! 3 bonus tests (roster-flip, empty-templates, no-template-
    //! covers-primary). The test imports below pull in
    //! `SurveyMethod`, `DimensionFidelity`, `ScientistSpecialty`,
    //! and `SeniorityTier` from the production modules — those
    //! aren't used by the production code in this file, so the
    //! `use super::*;` doesn't transitively re-export them.

    use super::*;
    use crate::personnel::{ScientistSpecialty, SeniorityTier};
    use crate::survey::components::DimensionFidelity;
    use crate::survey::data::ReasonTag;
    use crate::survey::types::SurveyMethod;

    fn mars_cache_tier_1() -> ContinuousStationBonus {
        // A tier-1 station's combined cache: 0.05 axes/yr and a
        // +5% mining yield multiplier.
        ContinuousStationBonus {
            axis_advance_per_year: 0.05,
            mining_yield_multiplier: 1.05,
        }
    }

    #[test]
    fn rate_label_always_visible_for_tier_1_at_mars() {
        // Issue AC: "Per-year rate is always visible, regardless
        // of cumulative integer state on SurveyState." A tier-1
        // station's per-year rate is 0.05 axes/yr; the cumulative
        // integer tier for the body's SurveyState might still be
        // 0 (the integer truncates 0.05/yr to 0). The dossier
        // shows the per-year rate string unconditionally.
        let s = OrbitalStationSummary {
            body_name: "Mars".to_string(),
            tiers: vec![1],
            per_year_rate: 0.05,
            mining_bonus_pct: 5,
            mine_count: 1,
            visibility_note: OrbitalStationSummary::VISIBILITY_NOTE,
        };
        assert_eq!(s.rate_label(), "0.05 axes/yr");
        assert_eq!(s.bonus_label(), "+5%");
        assert_eq!(s.tier_list(), "T1");
    }

    #[test]
    fn rate_label_renders_two_stations_stacked_at_mars() {
        // Two tier-1 stations on Mars sum to 0.10 axes/yr and
        // +10% mining. Verify the cache values flow through the
        // label helpers.
        let s = OrbitalStationSummary {
            body_name: "Mars".to_string(),
            tiers: vec![1, 1],
            per_year_rate: 0.10,
            mining_bonus_pct: 10,
            mine_count: 3,
            visibility_note: OrbitalStationSummary::VISIBILITY_NOTE,
        };
        assert_eq!(s.rate_label(), "0.10 axes/yr");
        assert_eq!(s.bonus_label(), "+10%");
        assert_eq!(s.tier_list(), "T1, T1");
    }

    #[test]
    fn tier_list_sorts_stably() {
        // Query order is non-deterministic. The summary sorts
        // the tier list so the display is always "T1, T1, T3"
        // regardless of spawn order.
        let mut s = OrbitalStationSummary {
            body_name: "Phobos".to_string(),
            tiers: vec![3, 1, 1],
            per_year_rate: 0.20,
            mining_bonus_pct: 20,
            mine_count: 0,
            visibility_note: OrbitalStationSummary::VISIBILITY_NOTE,
        };
        s.tiers.sort_unstable();
        assert_eq!(s.tier_list(), "T1, T1, T3");
    }

    #[test]
    fn tier_label_handles_unknown_tier() {
        // A buggy RON or future tier-0 stub must not produce an
        // empty label. The helper returns "T?" so a player sees
        // something is off rather than an empty cell.
        assert_eq!(OrbitalStationSummary::tier_label(1), "T1");
        assert_eq!(OrbitalStationSummary::tier_label(2), "T2");
        assert_eq!(OrbitalStationSummary::tier_label(3), "T3");
        assert_eq!(OrbitalStationSummary::tier_label(0), "T?");
        assert_eq!(OrbitalStationSummary::tier_label(99), "T?");
    }

    #[test]
    fn bonus_label_renders_zero_percent() {
        // Edge case: a future balance change could set
        // mining_yield_multiplier = 1.0 while leaving
        // axis_advance_per_year > 0. The dossier must not
        // render "-0%" or omit the line — it should show
        // "+0%".
        let s = OrbitalStationSummary {
            body_name: "Ceres".to_string(),
            tiers: vec![1],
            per_year_rate: 0.05,
            mining_bonus_pct: 0,
            mine_count: 0,
            visibility_note: OrbitalStationSummary::VISIBILITY_NOTE,
        };
        assert_eq!(s.bonus_label(), "+0%");
    }

    #[test]
    fn from_components_omits_body_with_no_bonus_cache() {
        // A body that no station orbits has no
        // `ContinuousStationBonus` component. The summary
        // builder returns `None` and the dossier hides the
        // section.
        let mut app = App::new();
        let body = app.world_mut().spawn_empty().id();
        let stations: Vec<&ContinuousSurveyStation> = Vec::new();
        let result = OrbitalStationSummary::from_components(None, None, body, "Earth", stations);
        assert!(
            result.is_none(),
            "a body with no ContinuousStationBonus should produce no summary"
        );
    }

    #[test]
    fn from_components_includes_body_with_active_bonus_cache() {
        // The dossier should appear for a body that has an
        // active `ContinuousStationBonus` cache. We construct a
        // minimal App with the components and verify the
        // summary builder produces the expected rate.
        let mut app = App::new();
        let mars = app
            .world_mut()
            .spawn((
                mars_cache_tier_1(),
                MiningOperation {
                    active: true,
                    ..Default::default()
                },
            ))
            .id();
        // Two tier-1 stations orbiting Mars.
        let s1 = app
            .world_mut()
            .spawn(ContinuousSurveyStation {
                orbiting_body: Some(mars),
                tier: 1,
            })
            .id();
        let s2 = app
            .world_mut()
            .spawn(ContinuousSurveyStation {
                orbiting_body: Some(mars),
                tier: 1,
            })
            .id();
        // Borrow the stations through `app.world()` (immutable)
        // so we can hand a `Vec<&ContinuousSurveyStation>` to
        // the function's `IntoIterator` bound.
        let world = app.world();
        let s1_ref: &ContinuousSurveyStation = world.get::<ContinuousSurveyStation>(s1).unwrap();
        let s2_ref: &ContinuousSurveyStation = world.get::<ContinuousSurveyStation>(s2).unwrap();
        let stations = vec![s1_ref, s2_ref];

        let summary = OrbitalStationSummary::from_components(
            world.get::<ContinuousStationBonus>(mars),
            world.get::<MiningOperation>(mars),
            mars,
            "Mars",
            stations,
        )
        .expect("Mars with an active bonus should produce a summary");

        assert_eq!(summary.body_name, "Mars");
        assert_eq!(summary.tier_list(), "T1, T1");
        assert_eq!(summary.rate_label(), "0.05 axes/yr");
        assert_eq!(summary.bonus_label(), "+5%");
        assert_eq!(summary.mine_count, 1);
    }

    #[test]
    fn multiplier_or_neutral_helper_unifies_mining_sites() {
        // GRA-83b: the DRY helper for the mining_bonus lookup.
        // Locks the contract: None → 1.0; Some → mining_yield_multiplier
        // promoted to f64. Used by all three call sites in
        // `economy::mining`.
        assert_eq!(ContinuousStationBonus::multiplier_or_neutral(None), 1.0);
        assert_eq!(
            ContinuousStationBonus::multiplier_or_neutral(Some(&ContinuousStationBonus::NEUTRAL)),
            1.0
        );
        // Use `1.05` as a real-world tier-1 value, but compare
        // against the same value after the same `f32 → f64`
        // promotion the helper performs. This avoids the
        // `1.05_f32 → 1.0499999523162842_f64` rounding trap
        // that breaks a direct `== 1.05_f64` comparison.
        let tier_1 = ContinuousStationBonus {
            axis_advance_per_year: 0.05,
            mining_yield_multiplier: 1.05,
        };
        assert_eq!(
            ContinuousStationBonus::multiplier_or_neutral(Some(&tier_1)),
            tier_1.mining_yield_multiplier as f64
        );
    }

    // ─── GRA-112 sophisticated `recommended_survey_action` tests ──
    //
    // These tests implement the 5-test acceptance suite from the
    // LGD design brief (comment `42784e2f-c90f-4742-a63a-01317e4b41ab`
    // on GRA-112). All tests are pure data — no egui context.

    /// Build a `SurveyMissionTemplate` for tests. Only the scoring
    /// fields (`target_tiers`, `base_duration_days`, `method`) are
    /// set; the rest use sensible defaults.
    fn test_template(
        id: &str,
        method: SurveyMethod,
        target_tiers: Vec<(SurveyDimension, u8)>,
        base_duration_days: u32,
    ) -> SurveyMissionTemplate {
        SurveyMissionTemplate {
            id: id.to_string(),
            display_name: id.to_string(),
            method,
            instrument_id: "test_instrument".to_string(),
            target_tiers: target_tiers.into_iter().collect(),
            base_duration_days,
            axis_yield_per_day: 1.0,
            is_ground_team: false,
            failure_modes: vec![],
            requires_ship_class: None,
            requires_min_ship_count: 1,
            min_assigned_scientists: 0,
        }
    }

    /// Build a `SurveyMissionTemplates` registry from a vec of
    /// templates.
    fn test_templates(templates: Vec<SurveyMissionTemplate>) -> SurveyMissionTemplates {
        SurveyMissionTemplates {
            templates: templates.into_iter().map(|t| (t.id.clone(), t)).collect(),
        }
    }

    /// Set a single dim's fidelity on a `SurveyState` builder.
    fn set_dim(state: &mut SurveyState, dim: SurveyDimension, tier: u8, confidence: f32) {
        state.set_fidelity(dim, DimensionFidelity::at_tier(tier, confidence, Some(0.0)));
    }

    #[test]
    fn recommender_returns_none_when_all_dims_fully_characterized() {
        // LGD brief §6 — Test 1: a body where every dim is at
        // tier 5 with confidence ≥ 0.8 produces no
        // recommendation. The dossier renders the "all
        // dimensions adequately characterized" branch in
        // that case.
        let mut state = SurveyState::default();
        for dim in SurveyDimension::ALL {
            set_dim(&mut state, dim, MAX_TIER, 1.0);
        }
        let templates = test_templates(vec![test_template(
            "t",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 1)],
            90,
        )]);
        let result = recommended_survey_action(&state, &templates, None);
        assert!(
            result.is_none(),
            "fully-characterized body must return None"
        );
    }

    #[test]
    fn recommender_picks_single_template_targeting_lowest_dim() {
        // LGD brief §6 — Test 2: a body with 8 dims at tier 0
        // and one template targeting OrbitalMech at tier 3.
        // OrbitalMech is primary (first in SurveyDimension::ALL
        // on tier-0 tiebreak), the template targets it, so the
        // helper returns that pair.
        let state = SurveyState::default();
        let template = test_template(
            "orbital_scan",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 3)],
            90,
        );
        let templates = test_templates(vec![template.clone()]);
        let result = recommended_survey_action(&state, &templates, None);
        let (dim, recommended, _reason) = result.expect("single-template should recommend");
        assert_eq!(dim, SurveyDimension::OrbitalMech);
        assert_eq!(recommended.id, template.id);
        assert_eq!(recommended.target_tiers[&dim], 3);
    }

    #[test]
    fn recommender_prefers_multi_dim_template_on_near_tie() {
        // LGD brief §6 — Test 3 (the discriminating version):
        // A targets OrbitalMech at tier 2 (single-dim, gap 2).
        // B targets OrbitalMech at tier 1 + SurfaceFeatures at
        // tier 1 (two-dim, gaps 1+1=2). With the cross-dim
        // bonus + confidence-deficit boost, B wins.
        //
        // Both dims at tier 0 confidence 0.0 → both
        // warning-tier (< WARNING_CONFIDENCE = 0.3), so the
        // confidence-deficit term in the score adds 1 per
        // targeted dim. The primary dim is OrbitalMech (first
        // in canonical order on tier-0 tiebreak with
        // confidence-deficit boost applied symmetrically).
        // A is the only template that targets OrbitalMech
        // AND SurfaceFeatures both. B targets the primary
        // dim and gets a cross-dim bonus on top of a
        // conf-deficit boost for the second dim too.
        //
        // The LGD's verbatim analysis ("B wins by 0.06") used
        // conf_deficit=0; the actual implementation correctly
        // credits conf_deficit for the two warning-tier
        // targets (2 vs 1 for A), so B wins by a wider
        // margin. The discriminating property — B wins on a
        // same-tier-gap test — is what this test locks in.
        let state = SurveyState::default();
        let a = test_template(
            "a",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 2)],
            90,
        );
        let b = test_template(
            "b",
            SurveyMethod::Orbital,
            vec![
                (SurveyDimension::OrbitalMech, 1),
                (SurveyDimension::SurfaceFeatures, 1),
            ],
            90,
        );
        let templates = test_templates(vec![a.clone(), b.clone()]);
        let (dim, recommended, _reason) = recommended_survey_action(&state, &templates, None)
            .expect("non-empty templates + non-fully-characterized body must recommend");
        // Primary dim is OrbitalMech (canonical-first on tier-0 tie).
        assert_eq!(dim, SurveyDimension::OrbitalMech);
        // B wins on the cross-dim + conf-deficit combination.
        assert_eq!(
            recommended.id, "b",
            "multi-dim template must win on near-tie per LGD brief §6 Test 3"
        );
    }

    #[test]
    fn recommender_boosts_warning_tier_dim_to_primary() {
        // LGD brief §6 — Test 4 (the discriminating version):
        // MineralDeposits is at tier 0 conf 0.2 (warning, <
        // 0.3). OrbitalMech is at tier 0 conf 0.5 (healthy).
        // Step 2 of the heuristic gives MineralDeposits a
        // synthetic -0.5 boost on its sort weight, so it
        // becomes the primary dim even though OrbitalMech is
        // "first" in canonical order.
        //
        // Template A targets OrbitalMech at tier 2 (gap 2 on
        // a non-primary dim). Template B targets
        // MineralDeposits at tier 2 (gap 2 on the primary
        // dim). The helper returns (MineralDeposits, B) —
        // A is filtered out by the primary-dim filter, and
        // B is the only candidate for MineralDeposits.
        //
        // The LGD brief's verbatim example used conf 0.4/0.6,
        // but WARNING_CONFIDENCE is 0.3, so 0.4 is not
        // warning-tier. The discriminating test uses
        // 0.2/0.5 to actually exercise the boost.
        //
        // All 8 dims at tier 0. We need exactly ONE warning-tier
        // dim so the warning boost pulls it to primary without
        // competition from the other 6 default-state dims (which
        // would also be warning-tier at conf 0.0 and tie on the
        // boost). Set the non-target dims to a healthy
        // confidence (0.5) so the warning boost only applies to
        // MineralDeposits.
        let mut state = SurveyState::default();
        for dim in SurveyDimension::ALL {
            let conf = if dim == SurveyDimension::MineralDeposits {
                0.2
            } else {
                0.5
            };
            set_dim(&mut state, dim, 0, conf);
        }
        let a = test_template(
            "a",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 2)],
            90,
        );
        let b = test_template(
            "b",
            SurveyMethod::Drill,
            vec![(SurveyDimension::MineralDeposits, 2)],
            180,
        );
        let templates = test_templates(vec![a.clone(), b.clone()]);
        let (dim, recommended, _reason) = recommended_survey_action(&state, &templates, None)
            .expect("warning-tier dim with a target template must recommend");
        // Warning-tier dim gets boosted to primary; A targeting
        // OrbitalMech is filtered out, B targeting the
        // warning-tier dim is the recommendation.
        assert_eq!(dim, SurveyDimension::MineralDeposits);
        assert_eq!(recommended.id, "b");
    }

    #[test]
    fn recommender_breaks_near_tie_on_cost_time() {
        // LGD brief §6 — Test 5 (the discriminating version):
        // both templates target the primary dim (OrbitalMech,
        // canonical-first at tier 0) with the same tier gap
        // (2). Template A is 90 days, Template B is 730
        // days. The log-scale cost penalty breaks the tie:
        // A scores cost = -(1+90/365).ln() * 0.5 ≈ -0.110;
        // B scores cost = -(1+730/365).ln() * 0.5 ≈ -0.549.
        // A wins on the cost-time fit by ~0.22.
        //
        // The LGD brief's verbatim example had B targeting
        // MineralDeposits (a different dim than A), which
        // would filter B out and make the test trivially
        // pass. The discriminating setup below has both
        // templates target the same primary dim so the
        // cost-time term is the actual deciding factor.
        let state = SurveyState::default();
        let a = test_template(
            "short",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 2)],
            90,
        );
        let b = test_template(
            "long",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 2)],
            730,
        );
        let templates = test_templates(vec![a.clone(), b.clone()]);
        let (dim, recommended, _reason) = recommended_survey_action(&state, &templates, None)
            .expect("non-empty templates + non-fully-characterized body must recommend");
        assert_eq!(dim, SurveyDimension::OrbitalMech);
        assert_eq!(
            recommended.id, "short",
            "shorter mission must win on cost-time tie per LGD brief §6 Test 5"
        );
    }

    #[test]
    fn recommender_specialist_roster_changes_pick() {
        // Bonus test (LGD brief §3 step 5): the roster
        // parameter reshapes the recommendation. A geologist
        // (specialty matches `Drill`) gives a positive bonus
        // to template B (method = Drill); template A is Orbital
        // and gets 0 from the roster. With the same tier gap
        // on the primary dim, B wins when the roster is
        // present and A wins when the roster is empty.
        let state = SurveyState::default();
        let a = test_template(
            "orbital_short",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 1)],
            90,
        );
        let b = test_template(
            "drill_long",
            SurveyMethod::Drill,
            vec![(SurveyDimension::OrbitalMech, 1)],
            365,
        );
        let templates = test_templates(vec![a.clone(), b.clone()]);

        // No roster: A wins (cheaper cost).
        let (dim_no, rec_no, _reason_no) = recommended_survey_action(&state, &templates, None)
            .expect("must recommend with no roster");
        assert_eq!(dim_no, SurveyDimension::OrbitalMech);
        assert_eq!(rec_no.id, "orbital_short");

        // With a geologist + principal seniority, B's
        // roster_bonus = 1.5 * 2.0 = 3.0; W_ROSTER (0.5) *
        // 3.0 = 1.5. B's score: 1.0 * 1 + 0.5 * 1 + 0 + 0.5 *
        // (-0.347) + 1.5 = 1.5 - 0.174 + 1.5 = 2.826.
        // A's score: 1.0 * 1 + 0.5 * 1 + 0 + 0.5 * (-0.110) +
        // 0 = 1.5 - 0.055 = 1.445. B wins.
        let roster = vec![ScientistSummary {
            id: 1,
            specialty: ScientistSpecialty::Geology,
            seniority: SeniorityTier::Principal,
        }];
        let (dim_yes, rec_yes, _reason_yes) =
            recommended_survey_action(&state, &templates, Some(&roster))
                .expect("must recommend with a roster");
        assert_eq!(dim_yes, SurveyDimension::OrbitalMech);
        assert_eq!(
            rec_yes.id, "drill_long",
            "geologist principal must flip the pick to the drill template"
        );
    }

    #[test]
    fn recommender_empty_templates_returns_none() {
        // Edge case (LGD brief §7): a body with an empty
        // template registry returns None even if some dims
        // are under-characterized.
        let state = SurveyState::default();
        let templates = SurveyMissionTemplates::default();
        assert!(recommended_survey_action(&state, &templates, None).is_none());
    }

    #[test]
    fn recommender_no_template_covers_primary_returns_none() {
        // Edge case (LGD brief §7): if no template targets
        // the primary dim (e.g. registry has only atmosphere
        // templates, but primary is OrbitalMech), return None.
        let state = SurveyState::default();
        let templates = test_templates(vec![test_template(
            "atmo_only",
            SurveyMethod::AtmosphericProbe,
            vec![(SurveyDimension::Atmosphere, 2)],
            90,
        )]);
        assert!(recommended_survey_action(&state, &templates, None).is_none());
    }

    // ─── GRA-114 reason-tag acceptance tests ───────────────────
    //
    // LGD GRA-114 design contract §"Tests (3 acceptance)":
    //   Test 6: specialist_reason_wins
    //   Test 7: cross_dim_reason_wins_over_single_dim
    //   Test 8: confidence_rescue_reason_wins
    //
    // The reason's priority is the discriminator: a higher-priority
    // reason wins even when the conditions for a lower-priority
    // reason are also met.

    #[test]
    fn reason_specialist_wins_over_tier_gap() {
        // Test 6: a senior geologist (specialty matches `Drill`)
        // is on the roster; the template uses Drill and closes a
        // tier gap on the primary dim. Priority 1 (Specialist)
        // wins, even though the conditions for TierGap (priority
        // 4) are also met.
        let mut state = SurveyState::default();
        // All dims at tier 0, all at healthy confidence 0.5.
        // OrbitalMech (canonical-first) wins primary on tier-0
        // tiebreak. No warning-tier dims means the warning
        // boost does not pull another dim ahead.
        for dim in SurveyDimension::ALL {
            set_dim(&mut state, dim, 0, 0.5);
        }
        let template = test_template(
            "drill_geology",
            SurveyMethod::Drill,
            vec![(SurveyDimension::OrbitalMech, 2)],
            180,
        );
        let templates = test_templates(vec![template]);
        let roster = vec![ScientistSummary {
            id: 1,
            specialty: ScientistSpecialty::Geology,
            seniority: SeniorityTier::Principal,
        }];
        let (dim, recommended, reason) =
            recommended_survey_action(&state, &templates, Some(&roster))
                .expect("geologist + template must recommend");
        assert_eq!(dim, SurveyDimension::OrbitalMech);
        assert_eq!(recommended.id, "drill_geology");
        // SpecialistOnStation wins over TierGap even though
        // the template closes a tier 0→2 gap (which would
        // also satisfy TierGap priority 4).
        assert_eq!(
            reason,
            ReasonTag::SpecialistOnStation {
                specialty: ScientistSpecialty::Geology,
            },
            "geologist on station must produce SpecialistOnStation reason"
        );
    }

    #[test]
    fn reason_cross_dim_wins_over_tier_gap() {
        // Test 7: two templates, A covers 1 dim at tier gap 2,
        // B covers 2 dims at tier gap 1 each. B wins. The
        // reason is CrossDim (priority 3), not TierGap (priority
        // 4). Primary dim is at a healthy confidence so
        // ConfidenceRescue (priority 2) does not fire first.
        let mut state = SurveyState::default();
        for dim in SurveyDimension::ALL {
            set_dim(&mut state, dim, 0, 0.5);
        }
        let a = test_template(
            "a_single",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 2)],
            90,
        );
        let b = test_template(
            "b_multi",
            SurveyMethod::Orbital,
            vec![
                (SurveyDimension::OrbitalMech, 1),
                (SurveyDimension::SurfaceFeatures, 1),
            ],
            90,
        );
        let templates = test_templates(vec![a.clone(), b.clone()]);
        let (dim, recommended, reason) = recommended_survey_action(&state, &templates, None)
            .expect("non-empty templates + non-fully-characterized body must recommend");
        // B wins on the cross-dim bonus (+0.25) over A.
        assert_eq!(dim, SurveyDimension::OrbitalMech);
        assert_eq!(recommended.id, "b_multi");
        // B's tier_gap is 1+1=2 and A's tier_gap is 2 — both
        // would satisfy TierGap. But B is a multi-dim template
        // so CrossDim (priority 3) wins over TierGap (priority 4).
        assert_eq!(
            reason,
            ReasonTag::CrossDim,
            "multi-dim template must produce CrossDim reason (priority 3) over TierGap (priority 4)"
        );
    }

    #[test]
    fn reason_confidence_rescue_wins_over_best_fit() {
        // Test 8: primary dim is at confidence 0.2 (below
        // WARNING_CONFIDENCE = 0.3); template covers 1 dim at
        // tier gap 0 (no tier gain). The reason is
        // ConfidenceRescue (priority 2), not BestFit (priority
        // 5). The score may be low but the confidence-deficit
        // boost still lifts it; the reason reflects the why.
        let mut state = SurveyState::default();
        for dim in SurveyDimension::ALL {
            let conf = if dim == SurveyDimension::OrbitalMech {
                0.2
            } else {
                0.5
            };
            set_dim(&mut state, dim, 0, conf);
        }
        // Template targets OrbitalMech at tier 0 — no tier gain.
        let template = test_template(
            "low_conf",
            SurveyMethod::Orbital,
            vec![(SurveyDimension::OrbitalMech, 0)],
            90,
        );
        let templates = test_templates(vec![template]);
        let (dim, recommended, reason) = recommended_survey_action(&state, &templates, None)
            .expect("warning-tier dim with a target template must recommend");
        assert_eq!(dim, SurveyDimension::OrbitalMech);
        assert_eq!(recommended.id, "low_conf");
        // ConfidenceRescue (priority 2) wins over BestFit
        // (priority 5) — the tier gap is 0, so TierGap does
        // not fire, and there is no roster, no cross-dim —
        // the discriminator is the warning-tier confidence.
        assert_eq!(
            reason,
            ReasonTag::ConfidenceRescue,
            "warning-tier primary dim must produce ConfidenceRescue reason over BestFit"
        );
    }
}
