use bevy::ecs::system::SystemParam;

use super::dashboard::{format_mass, format_rate_monthly};
use super::research_panel::{render_research_tech_tooltip_content, ActiveProjectInfo};
use super::time::{
    estimate_engineering_project_end_timestamp, estimate_research_project_end_timestamp,
    format_timestamp_date_time,
};
use super::*;

pub(super) fn get_resource_category_icon(category: &str) -> &'static str {
    match category {
        "Biological" => "\u{1F35E}",       // 🍞
        "Volatiles" => "\u{1F4A7}",        // 💧
        "Atmospheric Gases" => "\u{2601}", // ☁
        "Construction" => "\u{1F9F1}",     // 🧱
        "Fusion Fuel" => "\u{1F50B}",      // 🔋
        "Fissiles" => "\u{2622}",          // ☢
        "Precious Metals" => "\u{1F48E}",  // 💎
        "Strategic" => "\u{2699}",         // ⚙
        "Exotic" => "\u{1F52E}",           // 🔮
        _ => "\u{1F4E6}",                  // 📦
    }
}

/// Canonical representative resource for a category. Was used to
/// pick the PNG icon shown in the top resources-bar category tile
/// and in the category popup header. Replaced in v0.5.2 PR-A.4 by
/// `render_category_icon`, which draws the actual category-badge
/// PNG (`category-construction.png`, etc.) tinted to the category
/// color rather than the first resource's icon.
///
/// Kept `pub(super)` because the function is small, well-tested,
/// and a future tooltip / category-preview UI is a likely caller
/// (e.g. "preview which resources live in this category" hover).
/// If no caller materialises before the next sweep, mark it
/// `#[allow(dead_code)]` or remove.
#[allow(dead_code)]
pub(super) fn representative_resource_for_category(category: &str) -> ResourceType {
    for (cat, resources) in ResourceType::by_category() {
        if cat == category {
            if let Some(&first) = resources.first() {
                return first;
            }
        }
    }
    ResourceType::Food
}

/// Get the icon for a specific resource type
pub(super) fn get_resource_icon(resource: &ResourceType) -> &'static str {
    match resource {
        // Biological
        ResourceType::Food => "\u{1F35E}", // 🍞

        // Volatiles
        ResourceType::Water => "\u{1F4A7}",      // 💧
        ResourceType::Hydrogen => "\u{1F388}",   // 🎈
        ResourceType::Ammonia => "\u{1F9FC}",    // 🧼
        ResourceType::Methane => "\u{1F525}",    // 🔥
        ResourceType::Phosphorus => "\u{1F331}", // 🌱

        // Atmospheric
        ResourceType::Nitrogen => "\u{1F32C}",      // 🌬
        ResourceType::Oxygen => "\u{1F4A8}",        // 💨
        ResourceType::CarbonDioxide => "\u{1F32B}", // 🌫
        ResourceType::Argon => "\u{1F7E3}",         // 🟣

        // Construction
        ResourceType::Iron => "\u{1F529}",      // 🔩
        ResourceType::Aluminum => "\u{2708}",   // ✈
        ResourceType::Titanium => "\u{1F6E1}",  // 🛡
        ResourceType::Silicates => "\u{1FAA8}", // 🪨
        ResourceType::Nickel => "\u{1F9F2}",    // 🧲
        ResourceType::Tungsten => "\u{1F3AF}",  // 🎯
        ResourceType::Carbon => "\u{2666}",     // ♦
        ResourceType::Chromium => "\u{1F6E0}",  // 🛠
        ResourceType::Magnesium => "\u{2728}",  // ✨

        // Energy
        ResourceType::Helium3 => "\u{2600}",   // ☀
        ResourceType::Deuterium => "\u{269B}", // ⚛
        ResourceType::Tritium => "\u{2622}",   // ☢

        // Fissiles
        ResourceType::Uranium => "\u{2622}",    // ☢
        ResourceType::Thorium => "\u{26A1}",    // ⚡
        ResourceType::Plutonium => "\u{1F9EA}", // 🧪

        // Precious
        ResourceType::Gold => "\u{1F451}",     // 👑
        ResourceType::Silver => "\u{1F948}",   // 🥈
        ResourceType::Platinum => "\u{1F48D}", // 💍

        // Strategic
        ResourceType::Copper => "\u{1F50C}",     // 🔌
        ResourceType::RareEarths => "\u{1F4F1}", // 📱
        ResourceType::Lithium => "\u{1F50B}",    // 🔋
        ResourceType::Sulfur => "\u{1F9EA}",     // 🧪
        ResourceType::Cobalt => "\u{1F535}",     // 🔵
        ResourceType::Fluorine => "\u{1F4A0}",   // 💠
        ResourceType::Polymers => "\u{1F9F4}",   // 🧴

        // Exotic
        ResourceType::Antimatter => "\u{2604}",     // ☄
        ResourceType::ExoticMatter => "\u{1F300}",  // 🌀
        ResourceType::Metamaterials => "\u{1F52C}", // 🔬
        ResourceType::Computronium => "\u{1F9E0}",  // 🧠
    }
}

/// v0.5.2: render a resource's line-art PNG icon at the given size
/// inside the current egui row. The icons are 256×256 dark-on-white
/// PNGs in `assets/textures/ui/resources/`, post-processed by
/// `super::resource_icons::load_resource_icons` (white background →
/// transparent, dark lines → premultiplied white) and stored as
/// `egui::TextureHandle`s. Falls back to a small cyan-tinted square
/// (matches the Build card placeholder) if the icon hasn't loaded
/// yet or was never authored.
///
/// The `size` is in egui logical pixels — call sites pass 14 for
/// resource rows in popups, 16 for headers, etc. The texture is
/// bilinear-filtered so it scales down cleanly to whatever the
/// caller asks for.
pub(super) fn render_resource_icon(
    ui: &mut egui::Ui,
    icons: &super::resource_icons::ResourceIcons,
    resource: ResourceType,
    size: f32,
) {
    use super::resource_icons::get_resource_icon_handle;
    if let Some(handle) = get_resource_icon_handle(icons, resource) {
        // Tint the white pixels to the panel's accent color so the
        // icon reads as part of the UI.  The egui shader multiplies
        // the texture's white by the `tint` color.
        let tint = egui::Color32::from_rgb(0x60, 0xC8, 0xD8); // cyan, matches menu icons
        ui.add(
            egui::Image::from_texture(handle)
                .tint(tint)
                .fit_to_exact_size(egui::Vec2::splat(size)),
        );
    } else {
        // Fallback: cyan square (same look as the Build card's
        // icon-placeholder square so the visual language stays
        // consistent across egui and bevy_ui).
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(2),
            egui::Color32::from_rgba_unmultiplied(0x60, 0xC8, 0xD8, 153),
        );
    }
}

/// Render a category-badge PNG icon (the `category-*.png` set in
/// `assets/textures/ui/resources/`) at the requested size. Tinted
/// to the category color so the tile reads as the category
/// identity (Construction = amber, Volatiles = blue, Fissiles =
/// red, etc.) rather than as a generic monochrome mark.
///
/// Falls back to the same cyan placeholder square as
/// `render_resource_icon` when the asset hasn't loaded yet or the
/// category has no authored slug — visually consistent with the
/// resource-icon fallback and easy to spot in dev.
///
/// Used by the top resource bar tile (was: representative-resource
/// icon at 16 px) and the category popup header (was:
/// representative-resource icon at 18 px). The previous approach
/// rendered the **first resource of the category** (Iron for
/// Construction, Water for Volatiles, Nitrogen for Atmospheric
/// Gases), which never matched the actual category identity —
/// the category PNGs were authored but never wired up. See
/// `super::resource_icons::category_icon_basename` for the
/// category-name → filename mapping.
pub(super) fn render_category_icon(
    ui: &mut egui::Ui,
    icons: &super::resource_icons::ResourceIcons,
    category: &str,
    size: f32,
) {
    use super::resource_icons::get_category_icon_handle;
    if let Some(handle) = get_category_icon_handle(icons, category) {
        // Tint to the category color so the badge reads as part
        // of the category (amber for Construction, blue for
        // Volatiles, red for Fissiles, etc.). `category_color`
        // returns the canonical theme color.
        let tint = theme::category_color(category);
        ui.add(
            egui::Image::from_texture(handle)
                .tint(tint)
                .fit_to_exact_size(egui::Vec2::splat(size)),
        );
    } else {
        // Fallback: cyan square (matches the resource-icon
        // fallback so the bar reads as a row of evenly-sized
        // tiles even before the icons have loaded on the first
        // frame).
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(2),
            egui::Color32::from_rgba_unmultiplied(0x60, 0xC8, 0xD8, 153),
        );
    }
}

/// Render the dedicated energy icon
/// (`assets/textures/ui/resources/energy.png`) at the requested
/// size, tinted to the given colour. Energy is not a `ResourceType`
/// so it lives outside `render_resource_icon`; this is the call
/// site for the top resource bar's power chip (green/red) and the
/// Build/Mining card energy rows.
///
/// Falls back to a small tinted square (same look as the
/// resource/category fallbacks) when the PNG hasn't been decoded
/// yet — visually consistent with the rest of the resource bar.
pub(super) fn render_energy_icon(
    ui: &mut egui::Ui,
    icons: &super::resource_icons::ResourceIcons,
    tint: egui::Color32,
    size: f32,
) {
    use super::resource_icons::get_energy_icon_handle;
    if let Some(handle) = get_energy_icon_handle(icons) {
        ui.add(
            egui::Image::from_texture(handle)
                .tint(tint)
                .fit_to_exact_size(egui::Vec2::splat(size)),
        );
    } else {
        // Fallback: tinted square, matches the resource/category
        // icon fallbacks so the chip still reads as "energy
        // pending" before the PNG lands on the first frame.
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(2),
            egui::Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 153),
        );
    }
}

/// v0.5.2: render a resource row as `[icon] [name]` — the icon
/// uses the loaded PNG via `render_resource_icon` (cyan-tinted,
/// matching the menu icons) and the name is a normal `Label`.
///
/// Replaces the old `format!("{} {}", get_resource_icon(...),
/// display_name())` pattern, which embedded the emoji glyph in
/// `RichText` and prevented the PNG icon from ever being used.
/// Used by the category popup grid and the forecast hover
/// tooltip header — both contexts that already have a
/// `&ResourceIcons` resource in scope.
///
/// `name_size` is the egui font size of the label (14 for the
/// popup grid, 12 for the forecast tooltip). The icon is sized
/// to match (`name_size + 2.0`) so it visually centers against
/// the text baseline.
pub(super) fn render_resource_name_row(
    ui: &mut egui::Ui,
    icons: &super::resource_icons::ResourceIcons,
    resource: ResourceType,
    name_size: f32,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        render_resource_icon(ui, icons, resource, name_size + 2.0);
        ui.add(
            egui::Label::new(egui::RichText::new(resource.display_name()).size(name_size))
                .selectable(false),
        );
    });
}

/// Get color for resource category
fn get_category_color(category: &str) -> egui::Color32 {
    theme::category_color(category)
}

/// Resource popup that is currently open (if any).
///
/// Two popup kinds coexist:
/// - **Category popup**: when the player clicks a category tile in
///   the top bar, `open` is set.
/// - **Resource popup**: when the player clicks an individual resource
///   row inside a category popup, `resource_open` is set; this opens
///   a chart-only popup to the right of the category popup.
///
/// `resource_open` is layered on top of the category popup visually
/// but does not close it.  Both can be closed by clicking outside their
/// combined rect.
#[derive(Resource, Default)]
pub(super) struct OpenResourcePopup {
    /// Which category popup is open, and where to anchor it.
    open: Option<(String, egui::Rect)>,
    /// Which resource popup is open, and where to anchor it.  The
    /// resource popup is positioned to the right of the originating
    /// category popup.
    resource_open: Option<(crate::economy::ResourceType, egui::Rect)>,
}

const HISTORY_PANEL_SECONDS: f64 = crate::economy::HISTORY_MAX_AGE_SECONDS;

#[derive(Debug, Clone)]
struct HistoryPoint {
    sim_seconds: f64,
    value: f64,
    value_text: String,
    detail_text: Option<String>,
}

#[derive(Debug, Clone)]
struct HistorySeriesData {
    title: String,
    headline: String,
    supporting_text: String,
    accent: egui::Color32,
    points: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum HistoryTimeAxisMode {
    #[default]
    RelativeYears,
    CalendarYears,
}

#[derive(Debug, Clone)]
struct HistoryCursorInfo {
    sim_seconds: f64,
    value_text: String,
    detail_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum HistoryPanelMetric {
    #[default]
    Kardashev,
    PowerProduced,
    Population,
    Colonies,
    Ships,
    SurveyCoverage,
    SurveyedBodies,
    ResourceStockpile,
    ResourceNetRate,
    ResourceProduction,
    ResourceConsumption,
}

impl HistoryPanelMetric {
    fn label(self) -> &'static str {
        match self {
            Self::Kardashev => "Kardashev Scale",
            Self::PowerProduced => "Power Produced",
            Self::Population => "Population",
            Self::Colonies => "Colonies",
            Self::Ships => "Ships",
            Self::SurveyCoverage => "Survey Coverage",
            Self::SurveyedBodies => "Surveyed Bodies",
            Self::ResourceStockpile => "Resource Stockpile",
            Self::ResourceNetRate => "Resource Net Rate",
            Self::ResourceProduction => "Resource Production",
            Self::ResourceConsumption => "Resource Consumption",
        }
    }

    fn selection_label(self, resource: ResourceType) -> String {
        if self.is_resource_metric() {
            format!("{}: {}", self.label(), resource.display_name())
        } else {
            self.label().to_string()
        }
    }

    fn is_resource_metric(self) -> bool {
        matches!(
            self,
            Self::ResourceStockpile
                | Self::ResourceNetRate
                | Self::ResourceProduction
                | Self::ResourceConsumption
        )
    }

    fn accent(self, resource: ResourceType) -> egui::Color32 {
        match self {
            Self::Kardashev => theme::CAT_STRATEGIC,
            Self::PowerProduced => theme::ACCENT,
            Self::Population => theme::RB_POPULATION,
            Self::Colonies => theme::RB_COLONIES,
            Self::Ships => theme::RB_SHIPS,
            Self::SurveyCoverage | Self::SurveyedBodies => theme::RB_SURVEY,
            Self::ResourceStockpile
            | Self::ResourceNetRate
            | Self::ResourceProduction
            | Self::ResourceConsumption => get_category_color(resource.category()),
        }
    }

    fn title(self, resource: ResourceType) -> String {
        match self {
            Self::Kardashev => "Kardashev Development".to_string(),
            Self::PowerProduced => "Power Production History".to_string(),
            Self::Population => "Population History".to_string(),
            Self::Colonies => "Colony Count History".to_string(),
            Self::Ships => "Ship Count History".to_string(),
            Self::SurveyCoverage => "Survey Coverage History".to_string(),
            Self::SurveyedBodies => "Surveyed Bodies History".to_string(),
            Self::ResourceStockpile => format!("{} Stockpile History", resource.display_name()),
            Self::ResourceNetRate => format!("{} Net Flow History", resource.display_name()),
            Self::ResourceProduction => {
                format!("{} Gross Production History", resource.display_name())
            }
            Self::ResourceConsumption => {
                format!("{} Gross Consumption History", resource.display_name())
            }
        }
    }
}

pub(super) struct KardashevTrendState {
    detail_open: bool,
    axis_mode: HistoryTimeAxisMode,
    metric: HistoryPanelMetric,
    resource: ResourceType,
    last_window_pos: Option<egui::Pos2>,
}

impl Default for KardashevTrendState {
    fn default() -> Self {
        Self {
            detail_open: false,
            axis_mode: HistoryTimeAxisMode::RelativeYears,
            metric: HistoryPanelMetric::Kardashev,
            resource: ResourceType::Iron,
            last_window_pos: None,
        }
    }
}

impl KardashevTrendState {
    fn open_metric(&mut self, metric: HistoryPanelMetric) {
        self.detail_open = true;
        self.metric = metric;
    }
}

fn surveyed_body_count(sample: &crate::economy::SimulationHistorySample) -> u32 {
    sample.survey.surveyed_total()
}

fn build_kardashev_history(
    simulation_history: &crate::economy::SimulationHistory,
    current_sim_seconds: f64,
) -> Vec<HistoryPoint> {
    simulation_history
        .samples_within_window(current_sim_seconds, HISTORY_PANEL_SECONDS)
        .into_iter()
        .map(|sample| {
            let power = sample.power_produced_watts.max(1.0);
            let kardashev = crate::economy::kardashev_scale_from_watts(power);
            HistoryPoint {
                sim_seconds: sample.sim_seconds,
                value: kardashev,
                value_text: format!("Type {kardashev:.3}"),
                detail_text: Some(format!("Power production {}", format_power(power))),
            }
        })
        .collect()
}

fn format_monthly_throughput(value: f64) -> String {
    format!("{} /mo", format_mass(value))
}

fn format_history_value(metric: HistoryPanelMetric, value: f64, resource: ResourceType) -> String {
    match metric {
        HistoryPanelMetric::Kardashev => format!("Type {value:.3}"),
        HistoryPanelMetric::PowerProduced => format_power(value.max(0.0)),
        HistoryPanelMetric::Population => format_population(value),
        HistoryPanelMetric::Colonies
        | HistoryPanelMetric::Ships
        | HistoryPanelMetric::SurveyedBodies => format!("{value:.0}"),
        HistoryPanelMetric::SurveyCoverage => format!("{value:.1}%"),
        HistoryPanelMetric::ResourceStockpile => format_mass(value),
        HistoryPanelMetric::ResourceNetRate => format_rate_monthly(value).0,
        HistoryPanelMetric::ResourceProduction | HistoryPanelMetric::ResourceConsumption => {
            let _ = resource;
            format_monthly_throughput(value)
        }
    }
}

fn format_history_axis_value(
    metric: HistoryPanelMetric,
    value: f64,
    resource: ResourceType,
) -> String {
    match metric {
        HistoryPanelMetric::Kardashev => format!("{value:.3}"),
        HistoryPanelMetric::SurveyCoverage => format!("{value:.0}%"),
        _ => format_history_value(metric, value, resource),
    }
}

fn build_history_series(
    simulation_history: &crate::economy::SimulationHistory,
    current_sim_seconds: f64,
    metric: HistoryPanelMetric,
    resource: ResourceType,
) -> HistorySeriesData {
    let points: Vec<HistoryPoint> = simulation_history
        .samples_within_window(current_sim_seconds, HISTORY_PANEL_SECONDS)
        .into_iter()
        .map(|sample| {
            let (value, detail_text) = match metric {
                HistoryPanelMetric::Kardashev => {
                    let power = sample.power_produced_watts.max(1.0);
                    (
                        crate::economy::kardashev_scale_from_watts(power),
                        Some(format!("Power production {}", format_power(power))),
                    )
                }
                HistoryPanelMetric::PowerProduced => (
                    sample.power_produced_watts,
                    Some(format!(
                        "Power consumed {}",
                        format_power(sample.power_consumed_watts)
                    )),
                ),
                HistoryPanelMetric::Population => (
                    sample.total_population,
                    Some(format!("Colonies {}", sample.colony_count)),
                ),
                HistoryPanelMetric::Colonies => (
                    sample.colony_count as f64,
                    Some(format!(
                        "Population {}",
                        format_population(sample.total_population)
                    )),
                ),
                HistoryPanelMetric::Ships => (
                    sample.ship_count as f64,
                    Some(format!("Colonies {}", sample.colony_count)),
                ),
                HistoryPanelMetric::SurveyCoverage => {
                    let surveyed = surveyed_body_count(sample);
                    let total = sample.survey.total_bodies.max(1);
                    (
                        surveyed as f64 * 100.0 / total as f64,
                        Some(format!(
                            "{surveyed}/{} bodies surveyed",
                            sample.survey.total_bodies
                        )),
                    )
                }
                HistoryPanelMetric::SurveyedBodies => {
                    let surveyed = surveyed_body_count(sample);
                    (
                        surveyed as f64,
                        Some(format!(
                            "{} total bodies tracked",
                            sample.survey.total_bodies
                        )),
                    )
                }
                HistoryPanelMetric::ResourceStockpile => (
                    sample.resource_amount(resource),
                    Some(format!(
                        "{} ({})",
                        resource.display_name(),
                        resource.symbol()
                    )),
                ),
                HistoryPanelMetric::ResourceNetRate => (
                    sample.resource_net_rate(resource),
                    Some(format!("{} net monthly flow", resource.display_name())),
                ),
                HistoryPanelMetric::ResourceProduction => (
                    sample.resource_gross_production_rate(resource),
                    Some(format!(
                        "{} gross monthly production",
                        resource.display_name()
                    )),
                ),
                HistoryPanelMetric::ResourceConsumption => (
                    sample.resource_gross_consumption_rate(resource),
                    Some(format!(
                        "{} gross monthly consumption",
                        resource.display_name()
                    )),
                ),
            };

            HistoryPoint {
                sim_seconds: sample.sim_seconds,
                value,
                value_text: format_history_value(metric, value, resource),
                detail_text,
            }
        })
        .collect();

    let headline = points
        .last()
        .map(|point| point.value_text.clone())
        .unwrap_or_else(|| "Awaiting history samples".to_string());
    let supporting_text = match (metric, simulation_history.latest()) {
        (HistoryPanelMetric::Kardashev, Some(sample)) => {
            format!(
                "Power production: {}",
                format_power(sample.power_produced_watts.max(1.0))
            )
        }
        (HistoryPanelMetric::PowerProduced, Some(sample)) => {
            format!(
                "Current consumption: {}",
                format_power(sample.power_consumed_watts)
            )
        }
        (HistoryPanelMetric::Population, Some(sample)) => {
            format!("Colonies tracked: {}", sample.colony_count)
        }
        (HistoryPanelMetric::Colonies, Some(sample)) => {
            format!("Population: {}", format_population(sample.total_population))
        }
        (HistoryPanelMetric::Ships, Some(sample)) => {
            format!("Colony count: {}", sample.colony_count)
        }
        (HistoryPanelMetric::SurveyCoverage | HistoryPanelMetric::SurveyedBodies, Some(sample)) => {
            let surveyed = surveyed_body_count(sample);
            format!("Surveyed bodies: {surveyed}/{}", sample.survey.total_bodies)
        }
        (HistoryPanelMetric::ResourceStockpile, _) => {
            format!("{} ({})", resource.display_name(), resource.category())
        }
        (HistoryPanelMetric::ResourceNetRate, _) => {
            format!("{} net flow per month", resource.display_name())
        }
        (HistoryPanelMetric::ResourceProduction, _) => {
            format!("{} gross production per month", resource.display_name())
        }
        (HistoryPanelMetric::ResourceConsumption, _) => {
            format!("{} gross consumption per month", resource.display_name())
        }
        (_, None) => format!(
            "Last {:.0} years, oldest on the left",
            crate::economy::HISTORY_MAX_AGE_YEARS
        ),
    };

    HistorySeriesData {
        title: metric.title(resource),
        headline,
        supporting_text,
        accent: metric.accent(resource),
        points,
    }
}

fn current_calendar_year(sim_time: &SimulationTime) -> f64 {
    sim_time
        .format_date_time()
        .split(['.', ' '])
        .nth(2)
        .and_then(|year| year.parse::<f64>().ok())
        .unwrap_or(2026.0)
}

fn format_history_time_label(
    axis_mode: HistoryTimeAxisMode,
    current_year: f64,
    current_sim_seconds: f64,
    target_sim_seconds: f64,
    with_fraction: bool,
) -> String {
    let years_ago =
        ((current_sim_seconds - target_sim_seconds) / crate::economy::SECONDS_PER_YEAR).max(0.0);
    match axis_mode {
        HistoryTimeAxisMode::RelativeYears => {
            if years_ago <= 0.25 {
                "Now".to_string()
            } else if with_fraction {
                format!("{years_ago:.1}y ago")
            } else {
                format!("{years_ago:.0}y ago")
            }
        }
        HistoryTimeAxisMode::CalendarYears => {
            let year = current_year - years_ago;
            if with_fraction {
                format!("{year:.1}")
            } else {
                format!("{year:.0}")
            }
        }
    }
}

fn render_history_plot(
    ui: &mut egui::Ui,
    series: &HistorySeriesData,
    current_sim_seconds: f64,
    current_year: f64,
    axis_mode: HistoryTimeAxisMode,
    metric: HistoryPanelMetric,
    resource: ResourceType,
    desired_size: egui::Vec2,
    interactive: bool,
) -> Option<HistoryCursorInfo> {
    let sense = egui::Sense::hover();
    let (rect, response) = ui.allocate_exact_size(desired_size, sense);
    let painter = ui.painter();

    painter.rect_filled(rect, 4.0, theme::SURFACE);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, theme::BORDER),
        egui::StrokeKind::Outside,
    );

    if series.points.len() < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Awaiting history samples",
            theme::body(11.0),
            theme::TEXT_DIM,
        );
        return None;
    }

    let plot_rect = rect.shrink2(egui::vec2(10.0, 16.0));
    let window_start = current_sim_seconds - HISTORY_PANEL_SECONDS;
    let (min_y, max_y) = compute_history_y_bounds(series, metric);

    let to_screen = |sim_seconds: f64, value: f64| {
        let x_t = ((sim_seconds - window_start) / HISTORY_PANEL_SECONDS).clamp(0.0, 1.0);
        let y_t = ((value - min_y) / (max_y - min_y)).clamp(0.0, 1.0);
        egui::pos2(
            plot_rect.left() + plot_rect.width() * x_t as f32,
            plot_rect.bottom() - plot_rect.height() * y_t as f32,
        )
    };

    for tick in 0..=4 {
        let t = tick as f64 / 4.0;
        let x = egui::lerp(plot_rect.left()..=plot_rect.right(), t as f32);
        painter.line_segment(
            [
                egui::pos2(x, plot_rect.top()),
                egui::pos2(x, plot_rect.bottom()),
            ],
            egui::Stroke::new(1.0_f32, theme::BORDER.linear_multiply(0.6)),
        );

        let tick_sim_seconds = window_start + HISTORY_PANEL_SECONDS * t;
        let label = format_history_time_label(
            axis_mode,
            current_year,
            current_sim_seconds,
            tick_sim_seconds,
            false,
        );
        painter.text(
            egui::pos2(x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            theme::mono(10.0),
            theme::TEXT_HINT,
        );
    }

    for tick in 0..=3 {
        let t = tick as f64 / 3.0;
        let y = egui::lerp(plot_rect.bottom()..=plot_rect.top(), t as f32);
        painter.line_segment(
            [
                egui::pos2(plot_rect.left(), y),
                egui::pos2(plot_rect.right(), y),
            ],
            egui::Stroke::new(1.0_f32, theme::BORDER.linear_multiply(0.35)),
        );

        let value = min_y + (max_y - min_y) * t;
        painter.text(
            egui::pos2(plot_rect.left() + 2.0, y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format_history_axis_value(metric, value, resource),
            theme::mono(10.0),
            theme::TEXT_HINT,
        );
    }

    let line_points: Vec<egui::Pos2> = series
        .points
        .iter()
        .map(|point| to_screen(point.sim_seconds, point.value))
        .collect();
    painter.add(egui::Shape::line(
        line_points,
        egui::Stroke::new(2.0_f32, series.accent),
    ));

    if let Some(last) = series.points.last() {
        let current_pos = to_screen(last.sim_seconds, last.value);
        painter.circle_filled(current_pos, 3.5, theme::ACCENT);
        painter.circle_stroke(
            current_pos,
            5.0,
            egui::Stroke::new(1.0_f32, theme::ACCENT_DIM),
        );
    }

    if interactive {
        if let Some(pointer_pos) = response.hover_pos().filter(|pos| plot_rect.contains(*pos)) {
            let fraction = ((pointer_pos.x - plot_rect.left()) / plot_rect.width()).clamp(0.0, 1.0);
            let target_sim_seconds = window_start + HISTORY_PANEL_SECONDS * fraction as f64;
            let nearest_point = series.points.iter().min_by(|left, right| {
                let left_distance = (left.sim_seconds - target_sim_seconds).abs();
                let right_distance = (right.sim_seconds - target_sim_seconds).abs();
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;

            let nearest_pos = to_screen(nearest_point.sim_seconds, nearest_point.value);
            painter.line_segment(
                [
                    egui::pos2(nearest_pos.x, plot_rect.top()),
                    egui::pos2(nearest_pos.x, plot_rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, theme::ACCENT),
            );
            painter.line_segment(
                [
                    egui::pos2(plot_rect.left(), nearest_pos.y),
                    egui::pos2(plot_rect.right(), nearest_pos.y),
                ],
                egui::Stroke::new(1.0_f32, theme::ACCENT_DIM),
            );
            painter.circle_filled(nearest_pos, 4.0, theme::ACCENT);

            return Some(HistoryCursorInfo {
                sim_seconds: nearest_point.sim_seconds,
                value_text: nearest_point.value_text.clone(),
                detail_text: nearest_point.detail_text.clone(),
            });
        }
    }

    None
}

fn percentile_sorted(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let clamped = percentile.clamp(0.0, 1.0);
    let index = ((sorted_values.len() - 1) as f64 * clamped).round() as usize;
    sorted_values[index.min(sorted_values.len() - 1)]
}

fn is_resource_history_metric(metric: HistoryPanelMetric) -> bool {
    matches!(
        metric,
        HistoryPanelMetric::ResourceStockpile
            | HistoryPanelMetric::ResourceNetRate
            | HistoryPanelMetric::ResourceProduction
            | HistoryPanelMetric::ResourceConsumption
    )
}

fn compute_history_y_bounds(series: &HistorySeriesData, metric: HistoryPanelMetric) -> (f64, f64) {
    let mut values: Vec<f64> = series
        .points
        .iter()
        .map(|point| point.value)
        .filter(|value| value.is_finite())
        .collect();

    if values.is_empty() {
        return (0.0, 1.0);
    }

    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let raw_min = values[0];
    let raw_max = *values.last().unwrap_or(&1.0);
    let non_negative_series = raw_min >= 0.0;
    let latest_value = series
        .points
        .last()
        .map(|point| point.value)
        .unwrap_or(raw_max);
    let use_zero_baseline = non_negative_series
        && match metric {
            HistoryPanelMetric::SurveyCoverage => true,
            _ => raw_min <= (raw_max * 0.25),
        };

    let mut min_y = if use_zero_baseline { 0.0 } else { raw_min };
    let mut max_y = raw_max;
    let value_scale = raw_max.abs().max(raw_min.abs()).max(latest_value.abs());
    let tiny_padding = value_scale.max(1.0e-9) * 0.25;

    if values.len() >= 8 && use_zero_baseline {
        let robust_percentile = if is_resource_history_metric(metric) {
            0.85
        } else {
            0.95
        };
        let robust_max = percentile_sorted(&values, robust_percentile).max(0.0);
        let high_outlier_count = values
            .iter()
            .filter(|value| **value > robust_max * 2.5)
            .count();

        if robust_max > 0.0 && raw_max > robust_max * 6.0 && high_outlier_count <= 2 {
            max_y = robust_max.max(latest_value);
        }

        if is_resource_history_metric(metric) && latest_value > 0.0 {
            let current_scale_ceiling = (latest_value * 12.0).max(percentile_sorted(&values, 0.75));
            if max_y > current_scale_ceiling * 2.0 {
                max_y = current_scale_ceiling.max(latest_value);
            }
        }
    }

    if (max_y - min_y).abs() < 0.01 {
        if use_zero_baseline {
            max_y = (max_y + tiny_padding).max(tiny_padding * 2.0);
        } else {
            min_y -= tiny_padding;
            max_y += tiny_padding;
        }
    } else {
        let padding = (max_y - min_y) * 0.12;
        if !use_zero_baseline {
            min_y -= padding;
        }
        max_y += padding;
    }

    if !min_y.is_finite() || !max_y.is_finite() || max_y <= min_y {
        return (0.0, 1.0);
    }

    (min_y, max_y)
}

fn render_kardashev_hover_content(
    ui: &mut egui::Ui,
    history: &[HistoryPoint],
    current_sim_seconds: f64,
    current_year: f64,
    current_kardashev: f64,
    current_power: f64,
) {
    ui.set_min_width(260.0);
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new("Kardashev Development")
                .font(theme::heading())
                .color(theme::CAT_STRATEGIC),
        );
        ui.label(
            egui::RichText::new(format!(
                "Last {:.0} years, oldest on the left",
                crate::economy::HISTORY_MAX_AGE_YEARS
            ))
            .size(10.0)
            .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        let hover_series = HistorySeriesData {
            title: "Kardashev Development".to_string(),
            headline: format!("Type {current_kardashev:.3}"),
            supporting_text: format!("Power production: {}", format_power(current_power)),
            accent: HistoryPanelMetric::Kardashev.accent(ResourceType::Iron),
            points: history.to_vec(),
        };
        let _ = render_history_plot(
            ui,
            &hover_series,
            current_sim_seconds,
            current_year,
            HistoryTimeAxisMode::RelativeYears,
            HistoryPanelMetric::Kardashev,
            ResourceType::Iron,
            egui::vec2(240.0, 120.0),
            false,
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Type {current_kardashev:.3}"))
                    .strong()
                    .color(theme::CAT_STRATEGIC),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(format_power(current_power))
                    .monospace()
                    .color(theme::TEXT_VALUE),
            );
        });
        ui.label(
            egui::RichText::new("Click to open the full overlay")
                .size(10.0)
                .color(theme::TEXT_HINT),
        );
    });
}

fn render_kardashev_overlay(
    ctx: &egui::Context,
    resource_icons: &super::resource_icons::ResourceIcons,
    trend_state: &mut KardashevTrendState,
    simulation_history: &crate::economy::SimulationHistory,
    current_sim_seconds: f64,
    current_year: f64,
) {
    if !trend_state.detail_open {
        return;
    }

    let escape_pressed =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape_pressed {
        trend_state.detail_open = false;
        return;
    }

    let series = build_history_series(
        simulation_history,
        current_sim_seconds,
        trend_state.metric,
        trend_state.resource,
    );

    let mut still_open = true;
    let scrim_painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("kardashev_overlay_scrim"),
    ));
    scrim_painter.rect_filled(
        ctx.content_rect(),
        0.0,
        egui::Color32::from_rgba_premultiplied(4, 6, 12, 104),
    );

    let centered_pos = {
        let content_rect = ctx.content_rect();
        egui::pos2(
            content_rect.center().x - 480.0,
            content_rect.center().y - 310.0,
        )
    };

    let mut window = egui::Window::new(series.title.as_str())
        .id(egui::Id::new("history_overlay"))
        .collapsible(false)
        .resizable(false)
        .movable(true)
        .fixed_size(egui::vec2(960.0, 620.0))
        .open(&mut still_open)
        .frame(theme::elevated_frame());

    window = if let Some(last_pos) = trend_state.last_window_pos {
        window.current_pos(last_pos)
    } else {
        window.default_pos(centered_pos)
    };

    let window_response = window.show(ctx, |ui| {
        ui.set_min_width(960.0);
        ui.set_max_width(960.0);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(series.headline.as_str())
                        .font(theme::title())
                        .color(series.accent),
                );
                ui.label(
                    egui::RichText::new(series.supporting_text.as_str())
                        .font(theme::mono(11.0))
                        .color(theme::TEXT_DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    trend_state.detail_open = false;
                }
            });
        });

        ui.add_space(theme::Spacing::sm);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("Metric")
                    .font(theme::mono(10.0))
                    .color(theme::TEXT_DIM),
            );
            egui::ComboBox::from_id_salt("history_panel_metric")
                .selected_text(trend_state.metric.selection_label(trend_state.resource))
                .show_ui(ui, |ui| {
                    for metric in [
                        HistoryPanelMetric::Kardashev,
                        HistoryPanelMetric::PowerProduced,
                        HistoryPanelMetric::Population,
                        HistoryPanelMetric::Colonies,
                        HistoryPanelMetric::Ships,
                        HistoryPanelMetric::SurveyCoverage,
                        HistoryPanelMetric::SurveyedBodies,
                        HistoryPanelMetric::ResourceStockpile,
                        HistoryPanelMetric::ResourceNetRate,
                        HistoryPanelMetric::ResourceProduction,
                        HistoryPanelMetric::ResourceConsumption,
                    ] {
                        ui.selectable_value(&mut trend_state.metric, metric, metric.label());
                    }
                });

            if trend_state.metric.is_resource_metric() {
                ui.separator();
                ui.label(
                    egui::RichText::new("Resource")
                        .font(theme::mono(10.0))
                        .color(theme::TEXT_DIM),
                );
                egui::ComboBox::from_id_salt("history_panel_resource")
                    // v0.5.2: trigger keeps the resource name only
                    // (egui ComboBox `selected_text` takes a string,
                    // not a closure, so the loaded PNG icon can't
                    // be embedded inline). The dropdown options use
                    // `selectable_label` inside `show_ui` so the
                    // cyan-tinted PNG icon renders next to each
                    // option name — the visual upgrade is visible
                    // the moment the player opens the dropdown.
                    .selected_text(trend_state.resource.display_name())
                    .show_ui(ui, |ui| {
                        for resource in ResourceType::all() {
                            let is_selected = *resource == trend_state.resource;
                            // Row = [icon] [selectable label]. The
                            // icon is a non-interactive decorator;
                            // the click target is the label. Clicking
                            // the label swaps `trend_state.resource`
                            // to the chosen resource (matching the
                            // legacy `selectable_value` behavior).
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                render_resource_icon(ui, resource_icons, *resource, 16.0);
                                if ui
                                    .selectable_label(is_selected, resource.display_name())
                                    .clicked()
                                    && !is_selected
                                {
                                    trend_state.resource = *resource;
                                }
                            });
                        }
                    });
            }

            ui.separator();
            ui.label(
                egui::RichText::new("Time Axis")
                    .font(theme::mono(10.0))
                    .color(theme::TEXT_DIM),
            );
            ui.selectable_value(
                &mut trend_state.axis_mode,
                HistoryTimeAxisMode::RelativeYears,
                "Years Ago",
            );
            ui.selectable_value(
                &mut trend_state.axis_mode,
                HistoryTimeAxisMode::CalendarYears,
                "Calendar Years",
            );
        });

        ui.add_space(theme::Spacing::sm);
        let cursor_info = render_history_plot(
            ui,
            &series,
            current_sim_seconds,
            current_year,
            trend_state.axis_mode,
            trend_state.metric,
            trend_state.resource,
            egui::vec2(920.0, 420.0),
            true,
        );

        ui.add_space(theme::Spacing::sm);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if let Some(cursor) = cursor_info {
                    ui.label(
                        egui::RichText::new("Cursor")
                            .font(theme::mono(10.0))
                            .color(theme::TEXT_DIM),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format_history_time_label(
                            trend_state.axis_mode,
                            current_year,
                            current_sim_seconds,
                            cursor.sim_seconds,
                            true,
                        ))
                        .font(theme::mono(11.0))
                        .color(theme::TEXT_VALUE),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(cursor.value_text.as_str())
                            .font(theme::mono(11.0))
                            .color(series.accent),
                    );
                    if let Some(detail_text) = cursor.detail_text.as_deref() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(detail_text)
                                .font(theme::mono(11.0))
                                .color(theme::TEXT_VALUE),
                        );
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Hover the plot to inspect a point in the history.")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            },
        );
    });

    if let Some(window_response) = window_response {
        trend_state.last_window_pos = Some(window_response.response.rect.min);
    }

    if !still_open {
        trend_state.detail_open = false;
    }
}

#[derive(SystemParam)]
pub(super) struct ResourceBarPowerQueries<'w, 's> {
    body_query: Query<
        'w,
        's,
        (
            Entity,
            &'static CelestialBody,
            Option<&'static SystemId>,
            Option<&'static LogicalParent>,
            Option<&'static KeplerOrbit>,
            Option<&'static Colony>,
            Option<&'static PlanetResources>,
            Option<&'static SurveyLevel>,
            Option<&'static crate::survey::SurveyState>,
            Option<&'static crate::economy::components::PowerGenerator>,
            Option<&'static MiningOperation>,
        ),
    >,
    star_query: Query<
        'w,
        's,
        (&'static CelestialBody, &'static SystemId),
        With<crate::plugins::solar_system::Star>,
    >,
    buildings_data: Option<Res<'w, BuildingsData>>,
}

#[derive(SystemParam)]
pub(super) struct ResourceBarUiRuntime<'w> {
    sim_time: Res<'w, SimulationTime>,
    time: Res<'w, Time<Real>>,
    ui_prefs: Res<'w, ResearchUiPreferences>,
    // v0.5.2: 39 line-art PNG resource icons (water.png, iron.png,
    // …) replace the legacy emoji glyphs. Bundled into this
    // SystemParam (instead of taking it as a separate function
    // arg) so the host function stays under Bevy's 16-generic
    // cap. The render side calls `render_resource_icon` which
    // falls back to a small cyan square if the icon hasn't
    // loaded yet.
    resource_icons: Res<'w, super::resource_icons::ResourceIcons>,
}

/// GRA-31 PR-A: per-body / per-system resource breakdown + in-transit
/// indicator.  Bundled into one SystemParam so the host function stays
/// under Bevy's 16-generic tuple cap when chained with sibling systems.
#[derive(SystemParam)]
pub(super) struct ResourceBarBreakdownQueries<'w, 's> {
    /// All bodies with a `LocalStockpile`, indexed for the per-body /
    /// per-system breakdown table.
    per_body_breakdown: Query<
        'w,
        's,
        (
            Option<&'static crate::plugins::solar_system::CelestialBody>,
            Option<&'static crate::astronomy::components::SystemId>,
            &'static crate::economy::components::LocalStockpile,
        ),
    >,
    /// In-transit and open requests — drives the in-transit chip in each
    /// resource row.
    pending_resource_requests: Res<'w, crate::economy::logistics::PendingResourceRequests>,
    /// System vs Starmap view — drives the breakdown layout choice.
    view_mode: Res<'w, ViewMode>,
    /// Currently focused star system (used in System view).
    current_star_system: Res<'w, CurrentStarSystem>,
}

/// Per-frame scratch state for the resource bar UI.  Bundled into one
/// SystemParam to keep the host function under Bevy's 16-generic cap.
#[derive(SystemParam)]
pub(super) struct ResourceBarLocalState<'s> {
    open_popup: Local<'s, OpenResourcePopup>,
    kardashev_trend: Local<'s, KardashevTrendState>,
}

const CONTEXT_TILE_WIDTH: f32 = 88.0;
const CONTEXT_TILE_HEIGHT: f32 = 28.0;
const CONTEXT_NAME_FONT_SIZE: f32 = 11.5;

/// Category-badge icon size in the top resource bar. 28 px fills
/// the available vertical space inside the 48-px panel without
/// growing the bar itself; the previous 22 px value left a
/// noticeable empty band above and below the icon (especially
/// against the 13-pt total + 10-pt rate column).
const CATEGORY_TILE_ICON_SIZE: f32 = 28.0;

fn render_context_name_marquee(ui: &mut egui::Ui, text: &str) {
    let font_id = egui::FontId::proportional(CONTEXT_NAME_FONT_SIZE);
    let color = theme::TEXT_VALUE;
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font_id.clone(), color);
    let text_size = galley.size();
    let row_height = text_size.y.max(12.0);
    let available_width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(available_width, row_height),
        egui::Sense::hover(),
    );
    let clip = rect;
    let painter = ui.painter().with_clip_rect(clip);

    if text_size.x <= clip.width() {
        painter.text(
            clip.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            font_id,
            color,
        );
    } else {
        let gap = 36.0_f32;
        let cycle = text_size.x + gap;
        let speed = 35.0_f64;
        let t = ui.ctx().input(|i| i.time);
        let offset_x = ((t * speed) % cycle as f64) as f32;
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

/// Render the resources bar at the top of the screen (above the menu)
pub(super) fn ui_resources_bar(
    mut contexts: EguiContexts,
    mut pending_research: ResMut<PendingResearchActions>,
    budget: Res<GlobalBudget>,
    contextual: Res<crate::economy::ContextualStockpile>,
    rate_tracker: Res<ResourceRateTracker>,
    power_popup_queries: ResourceBarPowerQueries,
    research_state: Res<ResearchState>,
    population_query: Query<(
        &Population,
        Option<&crate::plugins::solar_system::CelestialBody>,
        Option<&crate::colony::Colony>,
    )>,
    // GRA-31 PR-A: per-body / per-system breakdown + in-transit chip.
    breakdown_queries: ResourceBarBreakdownQueries,
    // Bundles the two per-frame `Local<...>` scratch states.
    local_state: ResourceBarLocalState,
    research_projects: Query<&ResearchProject>,
    engineering_projects: Query<&EngineeringProject>,
    research_teams: Query<&ResearchTeam>,
    technologies: Res<TechnologiesData>,
    sim_history: Res<crate::economy::SimulationHistory>,
    ui_runtime: ResourceBarUiRuntime,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let sim_time = &ui_runtime.sim_time;
    let time = &ui_runtime.time;
    let ui_prefs = &ui_runtime.ui_prefs;

    // Unpack the bundled `Local<...>` scratch state.
    let mut open_popup = local_state.open_popup;
    let mut kardashev_trend = local_state.kardashev_trend;

    let current_sim_seconds = sim_time.elapsed_seconds();
    let current_power = budget.energy_grid.produced.max(1.0);
    let current_kardashev = crate::economy::kardashev_scale_from_watts(current_power);
    let current_year = current_calendar_year(sim_time);
    let kardashev_history = build_kardashev_history(&sim_history, current_sim_seconds);

    // Calculate total population
    let total_population: f64 = population_query.iter().map(|(p, _, _)| p.count).sum();

    egui::TopBottomPanel::top("resources_bar")
        .exact_height(48.0)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(4.0);

                // Context label (e.g. "Sol System" or "All Systems")
                ui.allocate_ui_with_layout(
                    egui::vec2(CONTEXT_TILE_WIDTH, CONTEXT_TILE_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(CONTEXT_TILE_WIDTH);
                        ui.set_max_width(CONTEXT_TILE_WIDTH);
                        ui.set_min_height(CONTEXT_TILE_HEIGHT);
                        ui.set_max_height(CONTEXT_TILE_HEIGHT);
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("📍").size(15.0).color(theme::ACCENT),
                                )
                                .selectable(false),
                            );
                            ui.add_space(2.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.set_width((CONTEXT_TILE_WIDTH - 22.0).max(1.0));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("CURRENT SYSTEM")
                                            .size(6.0)
                                            .color(theme::TEXT_HINT),
                                    )
                                    .selectable(false),
                                );
                                render_context_name_marquee(ui, &contextual.context_label);
                            });
                        });
                    },
                );
                ui.add_space(1.0);
                ui.separator();
                ui.add_space(1.0);

                // Show resource categories
                for (category_name, resources) in ResourceType::by_category() {
                    // Calculate total for category from contextual stockpile
                    let category_total: f64 = resources.iter().map(|r| contextual.get(r)).sum();
                    let category_rate: f64 = resources
                        .iter()
                        .map(|r| rate_tracker.get_resource_rate(r))
                        .sum();

                    // v3.8.1: aggregate fill ratio = total / (cap × N_bodies).
                    // The per-body cap is `effective_stockpile_cap(r)` which
                    // already includes the storage_multiplier; multiplying by
                    // the number of bodies in view gives the aggregate cap.
                    // We count bodies that have a LocalStockpile (every
                    // surveyed body) to get the right denominator.
                    let n_bodies = breakdown_queries.per_body_breakdown.iter().count() as f64;
                    let category_cap: f64 = resources
                        .iter()
                        .map(|r| budget.effective_stockpile_cap(*r) * n_bodies)
                        .sum();
                    let fill_ratio = if category_cap > 0.0 {
                        (category_total / category_cap).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    let color = get_category_color(category_name);
                    let text_color = theme::TEXT;
                    // v0.5.2 PR-A.4: use the category-badge PNG
                    // (`category-atmospheric.png`,
                    // `category-construction.png`, …) tinted to the
                    // category color. The previous approach rendered
                    // a representative-resource icon (Iron for
                    // Construction, Water for Volatiles, …) which
                    // never matched the actual category identity —
                    // the category PNGs were authored but never
                    // wired up. The fallback (cyan square) lives
                    // inside `render_category_icon` so dev builds
                    // without icons still read as a row of evenly
                    // sized tiles.

                    let is_this_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == category_name);

                    // v3.8.1: cap-throttle indicator. Lit when any
                    // body in this category is past the soft-knee
                    // (fill > 0.8) so the player can see "this rate
                    // is being reduced" at a glance. Per-body fill
                    // is the body-stockpile / body-cap; we surface
                    // the worst-case body.
                    // v3.8.7 (2026-08-07): the per-category lock icon
                    // is shown only when at least one body in the
                    // category is *at the cap* (fill ≥ 1.0), not at
                    // the 0.8 soft-knee.  The soft-knee is a warning
                    // (orange fill bar) that the throttle is *starting*
                    // to bite; the lock is a *status* that the cap is
                    // hit and production is at the consumption floor.
                    // Showing the lock at the soft-knee was misleading
                    // because the player had visual signal that
                    // production was already capped when in fact
                    // production was still positive.
                    let any_body_at_cap = resources.iter().any(|r| {
                        let cap = budget.effective_stockpile_cap(*r);
                        if cap <= 0.0 || cap >= f64::MAX {
                            return false;
                        }
                        // Sample the per-body max fill across bodies
                        // in view — only the hard cap (fill ≥ 1.0)
                        // counts as "at the cap".
                        breakdown_queries
                            .per_body_breakdown
                            .iter()
                            .any(|(_, _, stockpile)| {
                                let current = stockpile.get(r);
                                current / cap >= 1.0 - 1e-9
                            })
                    });

                    // Use a Frame for the category display
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                render_category_icon(
                                    ui,
                                    &ui_runtime.resource_icons,
                                    category_name,
                                    CATEGORY_TILE_ICON_SIZE,
                                );
                                ui.add_space(2.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(68.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(68.0);
                                    // Top row: total + (optional) cap icon
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_mass(category_total))
                                                    .size(13.0)
                                                    .color(text_color),
                                            )
                                            .selectable(false),
                                        );
                                        if any_body_at_cap {
                                            ui.add_space(2.0);
                                            let lock_response = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new("🔒")
                                                        .size(11.0)
                                                        .color(theme::AMBER),
                                                )
                                                .selectable(false),
                                            );
                                            lock_response.on_hover_text(
                                                "At the storage cap: at least one body in this\n\
                                                 category is full. Production is throttled to the\n\
                                                 per-body consumption draw (no net stockpile gain).\n\
                                                 Build more Warehouses / Resource Depots to expand\n\
                                                 the cap, or get a trade route set up to move\n\
                                                 the surplus off-world.",
                                            );
                                        }
                                    });
                                    let (rate_text, rate_color) =
                                        format_rate_monthly(category_rate);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(rate_text)
                                                .size(10.0)
                                                .color(rate_color),
                                        )
                                        .selectable(false),
                                    );
                                    // v3.8.1: tiny fill-ratio bar
                                    // (4px tall) so the player can see
                                    // how full the aggregate stockpile
                                    // is at a glance. Coloured by fill
                                    // band (green < 60%, yellow 60-80%,
                                    // orange 80-95%, red 95%+).
                                    let bar_color = if fill_ratio >= 0.95 {
                                        theme::RED
                                    } else if fill_ratio >= 0.80 {
                                        egui::Color32::from_rgb(255, 165, 0) // orange
                                    } else if fill_ratio >= 0.60 {
                                        egui::Color32::from_rgb(255, 215, 0) // yellow
                                    } else {
                                        egui::Color32::from_rgb(80, 200, 120) // green
                                    };
                                    let bar_rect = ui.allocate_space(egui::vec2(60.0, 4.0)).1;
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(
                                            bar_rect.min,
                                            egui::vec2(60.0, 4.0),
                                        ),
                                        1.0,
                                        egui::Color32::from_gray(50),
                                    );
                                    let fill_w = (60.0 * fill_ratio as f32).max(0.0);
                                    if fill_w > 0.0 {
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(
                                                bar_rect.min,
                                                egui::vec2(fill_w, 4.0),
                                            ),
                                            1.0,
                                            bar_color,
                                        );
                                    }
                                });
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());

                    // Hover and open-state border effect
                    if interact.hovered() || is_this_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, color),
                            egui::StrokeKind::Outside,
                        );
                    }

                    // Toggle popup on click
                    if interact.clicked() {
                        if is_this_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some((category_name.to_string(), interact.rect));
                        }
                    }

                    ui.add_space(4.0);
                }

                // Research Points display
                {
                    let rp_color = theme::RP_BLUE;
                    let text_color = theme::TEXT;
                    let warning_color = theme::RED;
                    let is_rp_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "ResearchPoints");

                    // Find active research projects
                    let mut active_rps: Vec<_> =
                        research_projects.iter().filter(|p| p.active).collect();
                    active_rps.sort_by(|a, b| {
                        b.progress_percent()
                            .partial_cmp(&a.progress_percent())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let furthest_rp = active_rps.first();
                    let has_active_rp = !active_rps.is_empty();

                    // Warning flash
                    let flash = if !has_active_rp && ui_prefs.show_inactive_warning {
                        (time.elapsed_secs() * 5.0).sin().abs()
                    } else {
                        0.0
                    };

                    let border_color = if flash > 0.5 {
                        warning_color
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .stroke(egui::Stroke::new(
                            if flash > 0.0 { 2.0_f32 } else { 0.0_f32 },
                            border_color,
                        ))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("🔬").size(20.0).color(rp_color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(115.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(115.0);

                                    if let Some(project) = furthest_rp {
                                        if let Some(tech) =
                                            technologies.technologies.get(&project.tech_id)
                                        {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&tech.name)
                                                        .size(12.0)
                                                        .color(text_color),
                                                )
                                                .selectable(false),
                                            );

                                            let progress_fraction = project.progress_percent();
                                            ui.add(
                                                egui::ProgressBar::new(progress_fraction)
                                                    .desired_width(100.0)
                                                    .desired_height(4.0)
                                                    .fill(theme::RP_BLUE),
                                            );
                                        } else {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new("Unknown Project")
                                                        .size(10.0)
                                                        .color(text_color),
                                                )
                                                .selectable(false),
                                            );
                                        }
                                    } else {
                                        let warning_text = if !has_active_rp {
                                            "No Active Research!"
                                        } else {
                                            "Idle"
                                        };
                                        let warning_text_color = if flash > 0.5 {
                                            warning_color
                                        } else {
                                            text_color
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(warning_text)
                                                    .size(10.0)
                                                    .color(warning_text_color),
                                            )
                                            .selectable(false),
                                        );
                                    }
                                });
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());
                    if interact.hovered() || is_rp_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, rp_color),
                            egui::StrokeKind::Outside,
                        );
                    }

                    if interact.double_clicked() {
                        pending_research.navigate_to_available_tab = true;
                        open_popup.open = None;
                    } else if interact.clicked() {
                        if is_rp_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("ResearchPoints".to_string(), interact.rect));
                        }
                    }
                    ui.add_space(4.0);
                }

                // Engineering Points display
                {
                    let ep_color = theme::EP_TEAL;
                    let text_color = theme::TEXT;
                    let warning_color = theme::RED;
                    let is_ep_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "EngineeringPoints");

                    // Find active engineering projects
                    let mut active_eps: Vec<_> = engineering_projects.iter().collect();
                    active_eps.sort_by(|a, b| {
                        b.progress_percent()
                            .partial_cmp(&a.progress_percent())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let furthest_ep = active_eps.first();
                    let has_active_ep = !active_eps.is_empty();

                    // Warning flash
                    let flash = if !has_active_ep && ui_prefs.show_inactive_warning {
                        (time.elapsed_secs() * 5.0).sin().abs()
                    } else {
                        0.0
                    };

                    let border_color = if flash > 0.5 {
                        warning_color
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .stroke(egui::Stroke::new(
                            if flash > 0.0 { 2.0_f32 } else { 0.0_f32 },
                            border_color,
                        ))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("⚙").size(20.0).color(ep_color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(115.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(115.0);

                                    if let Some(project) = furthest_ep {
                                        let name = technologies
                                            .components
                                            .get(&project.component_id)
                                            .map(|c| c.name.as_str())
                                            .unwrap_or("Unknown Component");
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(name)
                                                    .size(12.0)
                                                    .color(text_color),
                                            )
                                            .selectable(false),
                                        );

                                        let progress_fraction = project.progress_percent();
                                        ui.add(
                                            egui::ProgressBar::new(progress_fraction)
                                                .desired_width(100.0)
                                                .desired_height(4.0)
                                                .fill(theme::RP_BLUE),
                                        );
                                    } else {
                                        let warning_text = if !has_active_ep {
                                            "No Active Eng.!"
                                        } else {
                                            "Idle"
                                        };
                                        let warning_text_color = if flash > 0.5 {
                                            warning_color
                                        } else {
                                            text_color
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(warning_text)
                                                    .size(10.0)
                                                    .color(warning_text_color),
                                            )
                                            .selectable(false),
                                        );
                                    }
                                });
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());
                    if interact.hovered() || is_ep_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, ep_color),
                            egui::StrokeKind::Outside,
                        );
                    }

                    if interact.double_clicked() {
                        pending_research.navigate_to_available_engineering_tab = true;
                        open_popup.open = None;
                    } else if interact.clicked() {
                        if is_ep_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open =
                                Some(("EngineeringPoints".to_string(), interact.rect));
                        }
                    }
                }

                // Push to the right side
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);

                    // Kardashev scale calculation (based on total power)
                    // type I: 10^16 W, Type II: 10^26 W. Scale is logarithmic.
                    // K = (log10(Power_in_Watts) - 6) / 10 is the Carl Sagan formula.
                    let kardashev_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "Type {:.3}",
                                            current_kardashev
                                        ))
                                        .size(14.0)
                                        .color(theme::CAT_STRATEGIC),
                                    )
                                    .selectable(false),
                                );
                            });
                        })
                        .response;

                    let kardashev_interact = kardashev_response.interact(egui::Sense::click());
                    if kardashev_interact.hovered() || kardashev_trend.detail_open {
                        ui.painter().rect_stroke(
                            kardashev_interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, theme::CAT_STRATEGIC),
                            egui::StrokeKind::Outside,
                        );
                    }

                    if kardashev_interact.hovered() && !kardashev_trend.detail_open {
                        ctx.request_repaint();
                    }

                    let kardashev_hover = kardashev_interact
                        .clone()
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if !kardashev_trend.detail_open {
                        kardashev_hover.on_hover_ui(|ui| {
                            theme::tooltip_frame().show(ui, |ui| {
                                render_kardashev_hover_content(
                                    ui,
                                    &kardashev_history,
                                    current_sim_seconds,
                                    current_year,
                                    current_kardashev,
                                    current_power,
                                );
                            });
                        });
                    }

                    if kardashev_interact.clicked() {
                        open_popup.open = None;
                        kardashev_trend.open_metric(HistoryPanelMetric::Kardashev);
                    }

                    ui.separator();

                    // Power grid status
                    // v0.5.2 PR-A.7 (2026-08-04): three-band text
                    // colour ladder keyed off the current grid
                    // surplus (`net_power`, produced − consumed, in
                    // MW):
                    //   * ≤ 0 MW  → red    (deficit or zero — grid
                    //                      can't cover demand)
                    //   * 0–50 GW → yellow (low/medium surplus —
                    //                      comfortable but not
                    //                      abundant)
                    //   * > 50 GW → green  (abundant surplus)
                    // The icon itself stays `theme::GOLD` (the egui
                    // twin of `bevy_theme::YELLOW_ENERGY`) so the
                    // power chip reads as a distinct third category
                    // — the gold icon is the "this is power" marker,
                    // the text colour carries the surplus signal.
                    // 50 GW = 50_000 MW (matches the `format_power`
                    // SI ladder's MW→GW band at 1000×).
                    let net_power = budget.net_power();
                    const POWER_SURPLUS_YELLOW_MAX_MW: f64 = 50_000.0;
                    let power_text_color = if net_power <= 0.0 {
                        theme::RED
                    } else if net_power <= POWER_SURPLUS_YELLOW_MAX_MW {
                        theme::STATUS_WARN
                    } else {
                        theme::GREEN
                    };

                    let is_power_open = open_popup.open.as_ref().is_some_and(|(n, _)| n == "Power");

                    // Power generation display (clickable with tooltip)
                    // v0.5.2+: dedicated bolt-in-hex PNG (`energy.png`)
                    // replaces the legacy ⚡ emoji glyph. Icon is
                    // always tinted gold (the energy-chrome constant);
                    // text colour carries the three-band surplus
                    // signal. 16-px icon size to sit visually next to
                    // the 14-pt bold power number.
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(82.0); // Fixed width to prevent wiggling
                                ui.set_max_width(82.0);
                                render_energy_icon(
                                    ui,
                                    &ui_runtime.resource_icons,
                                    theme::GOLD,
                                    16.0,
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format_power(
                                            budget.energy_grid.produced,
                                        ))
                                        .size(14.0)
                                        .strong()
                                        .color(power_text_color),
                                    )
                                    .selectable(false),
                                );
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());

                    if interact.hovered() || is_power_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, power_text_color),
                            egui::StrokeKind::Outside,
                        );
                        interact
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if interact.clicked() {
                        if is_power_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("Power".to_string(), interact.rect));
                        }
                    }

                    ui.separator();

                    // Treasury / Financial status
                    let balance = budget.balance_per_year();
                    let treasury_color = if balance >= 0.0 {
                        theme::GOLD
                    } else {
                        theme::RED
                    };

                    let is_treasury_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "Treasury");

                    let treasury_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("💰").size(20.0).color(treasury_color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.scope(|ui| {
                                    // Fixed width to prevent layout issues in right-to-left container
                                    ui.set_min_width(90.0);
                                    ui.set_max_width(90.0);
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_currency(
                                                    budget.treasury,
                                                ))
                                                .size(14.0)
                                                .strong()
                                                .color(treasury_color),
                                            )
                                            .selectable(false),
                                        );
                                        let balance_sign = if balance >= 0.0 { "+" } else { "" };
                                        let balance_text = format!(
                                            "{}{}/yr",
                                            balance_sign,
                                            format_currency(balance)
                                        );
                                        let balance_color = if balance >= 0.0 {
                                            theme::GREEN
                                        } else {
                                            theme::RED
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(balance_text)
                                                    .size(10.0)
                                                    .color(balance_color),
                                            )
                                            .selectable(false),
                                        );
                                    });
                                });
                            });
                        })
                        .response;

                    let treasury_interact = treasury_response.interact(egui::Sense::click());

                    if treasury_interact.hovered() || is_treasury_open {
                        ui.painter().rect_stroke(
                            treasury_interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, treasury_color),
                            egui::StrokeKind::Outside,
                        );
                        treasury_interact
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if treasury_interact.clicked() {
                        if is_treasury_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open =
                                Some(("Treasury".to_string(), treasury_interact.rect));
                        }
                    }

                    ui.separator();

                    // Population
                    let is_pop_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "Population");

                    // Use a Frame for the population display
                    let pop_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(68.0); // Fixed width to prevent wiggling
                                ui.set_max_width(68.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format_population(total_population))
                                            .size(16.0),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("👥").size(20.0).color(theme::TEXT),
                                    )
                                    .selectable(false),
                                );
                            });
                        })
                        .response;

                    let pop_interact = pop_response.interact(egui::Sense::click());

                    if pop_interact.hovered() || is_pop_open {
                        ui.painter().rect_stroke(
                            pop_interact.rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, theme::TEXT),
                            egui::StrokeKind::Outside,
                        );
                        pop_interact
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if pop_interact.clicked() {
                        if is_pop_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("Population".to_string(), pop_interact.rect));
                        }
                    }
                });
            });
        });

    // Render the resource popup as a floating egui::Window OUTSIDE the panel
    // so it is not clipped by the TopBottomPanel's bounds.
    if let Some((ref cat_name, anchor_rect)) = open_popup.open.clone() {
        if cat_name == "Power" {
            let mut still_open = true;
            let hierarchy = super::economy_panel::build_economy_hierarchy(
                &power_popup_queries.body_query,
                &power_popup_queries.star_query,
                power_popup_queries.buildings_data.as_deref(),
            );
            let power_rows = super::economy_panel::collect_power_body_rows(&hierarchy);
            // Determine color from budget - recalculate here.
            // v0.5.2 PR-A.7 (2026-08-04): three-band ladder
            // matching the top-bar chip — ≤ 0 red, 0–50 GW
            // yellow, > 50 GW green. The header title and the
            // per-row amount share the same colour so the
            // breakdown reads as a unified state read.
            let net_power = budget.net_power();
            const POWER_SURPLUS_YELLOW_MAX_MW: f64 = 50_000.0;
            let power_color = if net_power <= 0.0 {
                theme::RED
            } else if net_power <= POWER_SURPLUS_YELLOW_MAX_MW {
                theme::STATUS_WARN
            } else {
                theme::GREEN
            };

            let window_response = egui::Window::new("Power Breakdown")
                .id(egui::Id::new("power_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(300.0);
                    ui.horizontal(|ui| {
                        // v0.5.2 PR-A.7 (2026-08-04): the
                        // dedicated bolt-in-hex PNG
                        // (`energy.png`) replaces the
                        // legacy ⚡ emoji glyph in the popup
                        // header. Tinted gold
                        // (`theme::GOLD` = the egui twin of
                        // `bevy_theme::YELLOW_ENERGY`) so the
                        // "this is power" marker is
                        // consistent with the top-bar chip
                        // and the Build/Mining card chips.
                        // 22-px size so it reads as the
                        // primary visual element next to the
                        // 16-pt bold title text.
                        render_energy_icon(
                            ui,
                            &ui_runtime.resource_icons,
                            theme::GOLD,
                            22.0,
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Power Production")
                                    .size(16.0)
                                    .strong()
                                    .color(power_color),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    if power_rows.is_empty() {
                        ui.add(egui::Label::new("No active power generation").selectable(false));
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Top Contributing Bodies")
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            )
                            .selectable(false),
                        );
                        ui.add_space(4.0);

                        let top_count = power_rows.len().min(10);
                        for body in power_rows.iter().take(top_count) {
                            let row_response = ui
                                .horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(format!(
                                            "{} {}",
                                            super::economy_panel::power_body_icon(body.body_type),
                                            body.body_name
                                        ))
                                        .selectable(false),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!("({})", body.system_name))
                                                .size(10.5)
                                                .color(theme::TEXT_DIM),
                                        )
                                        .selectable(false),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format_power(
                                                        body.total_generation_watts,
                                                    ))
                                                    .strong(),
                                                )
                                                .selectable(false),
                                            );
                                        },
                                    );
                                })
                                .response;

                            row_response.on_hover_ui(|ui| {
                                super::economy_panel::render_power_body_detail_tooltip(
                                    ui,
                                    body,
                                    power_popup_queries.buildings_data.as_deref(),
                                );
                            });
                        }

                        let rest_count = power_rows.len().saturating_sub(top_count);
                        let rest_generation: f64 = power_rows
                            .iter()
                            .skip(top_count)
                            .map(|body| body.total_generation_watts)
                            .sum();
                        if rest_count > 0 && rest_generation > 0.0 {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "Rest ({} bodies)",
                                            rest_count
                                        ))
                                        .color(theme::TEXT_DIM),
                                    )
                                    .selectable(false),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_power(rest_generation))
                                                    .color(theme::TEXT_DIM),
                                            )
                                            .selectable(false),
                                        );
                                    },
                                );
                            });
                        }
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Total").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_power(budget.energy_grid.produced))
                                        .strong()
                                        .color(power_color),
                                )
                                .selectable(false),
                            );
                        });
                    });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "Treasury" {
            let mut still_open = true;
            let balance = budget.balance_per_year();
            let balance_color = if balance >= 0.0 {
                theme::GREEN
            } else {
                theme::RED
            };

            let window_response = egui::Window::new("Treasury Breakdown")
                .id(egui::Id::new("treasury_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("💰").size(18.0).color(theme::GOLD),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Financial Overview")
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Treasury:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(budget.treasury)).strong(),
                                )
                                .selectable(false),
                            );
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Income/yr:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(budget.income_per_year))
                                        .color(theme::GREEN),
                                )
                                .selectable(false),
                            );
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Expenses/yr:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(budget.expenses_per_year))
                                        .color(theme::RED),
                                )
                                .selectable(false),
                            );
                        });
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Balance/yr:").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(balance))
                                        .strong()
                                        .color(balance_color),
                                )
                                .selectable(false),
                            );
                        });
                    });
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "ResearchPoints" {
            let mut still_open = true;
            let window_response = egui::Window::new("Research Breakdown")
                .id(egui::Id::new("research_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(250.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("🔬").size(18.0).color(theme::RP_BLUE),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Active Research Projects")
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    let mut active_rps: Vec<_> =
                        research_projects.iter().filter(|p| p.active).collect();
                    active_rps.sort_by(|a, b| {
                        let a_progress = if a.required_points > 0.0 {
                            a.progress / a.required_points
                        } else {
                            1.0
                        };
                        let b_progress = if b.required_points > 0.0 {
                            b.progress / b.required_points
                        } else {
                            1.0
                        };
                        b_progress
                            .partial_cmp(&a_progress)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let total_allocation: f64 = active_rps
                        .iter()
                        .filter(|project| {
                            project.required_points > project.progress
                                && project.rp_allocation_percent > 0.0
                        })
                        .map(|project| project.rp_allocation_percent)
                        .sum();

                    if active_rps.is_empty() {
                        ui.add(egui::Label::new("No active research projects.").selectable(false));
                    } else {
                        for project in &active_rps {
                            if let Some(tech) = technologies.technologies.get(&project.tech_id) {
                                let progress = if project.required_points > 0.0 {
                                    (project.progress / project.required_points * 100.0)
                                        .clamp(0.0, 100.0)
                                } else {
                                    100.0
                                };
                                let end_date_text = estimate_research_project_end_timestamp(
                                    project,
                                    research_teams.get(project.team_id).ok(),
                                    &technologies,
                                    &research_state,
                                    total_allocation,
                                    sim_time.current_timestamp(),
                                )
                                .map(format_timestamp_date_time)
                                .unwrap_or_else(|| "ETA: Paused".to_string());

                                let row = ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(tech.name.as_str()).selectable(false));
                                });
                                let active_info = ActiveProjectInfo {
                                    entity: Entity::PLACEHOLDER,
                                    progress_percent: (progress / 100.0) as f32,
                                    progress: project.progress,
                                    required_points: project.required_points,
                                    allocation_percent: project.rp_allocation_percent,
                                    active: project.active,
                                };
                                row.response.on_hover_ui(|ui| {
                                    render_research_tech_tooltip_content(
                                        ui,
                                        tech,
                                        &technologies,
                                        &research_state,
                                        None,
                                        Some(&active_info),
                                    );
                                });
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("  {}", end_date_text))
                                            .size(10.0)
                                            .color(egui::Color32::GRAY),
                                    )
                                    .selectable(false),
                                );
                                ui.add(
                                    egui::ProgressBar::new((progress / 100.0) as f32)
                                        .desired_width(220.0),
                                );
                                ui.add_space(4.0);
                            }
                        }
                    }
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "EngineeringPoints" {
            let mut still_open = true;
            let window_response = egui::Window::new("Engineering Breakdown")
                .id(egui::Id::new("engineering_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(250.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("⚙").size(18.0).color(theme::EP_TEAL),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Active Engineering Projects")
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    let mut active_eps: Vec<_> = engineering_projects.iter().collect();
                    active_eps.sort_by(|a, b| {
                        let a_progress = if a.required_points > 0.0 {
                            a.progress / a.required_points
                        } else {
                            1.0
                        };
                        let b_progress = if b.required_points > 0.0 {
                            b.progress / b.required_points
                        } else {
                            1.0
                        };
                        b_progress
                            .partial_cmp(&a_progress)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    if active_eps.is_empty() {
                        ui.add(
                            egui::Label::new("No active engineering projects.").selectable(false),
                        );
                    } else {
                        for project in &active_eps {
                            let name = technologies
                                .components
                                .get(&project.component_id)
                                .map(|c| c.name.as_str())
                                .unwrap_or("Unknown Component");
                            let progress = if project.required_points > 0.0 {
                                (project.progress / project.required_points * 100.0)
                                    .clamp(0.0, 100.0)
                            } else {
                                100.0
                            };
                            let end_date_text = estimate_engineering_project_end_timestamp(
                                project,
                                research_teams.get(project.team_id).ok(),
                                &research_state,
                                sim_time.current_timestamp(),
                            )
                            .map(format_timestamp_date_time)
                            .unwrap_or_else(|| "ETA: Unassigned".to_string());

                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(name).selectable(false));
                            });
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("  {}", end_date_text))
                                        .size(10.0)
                                        .color(egui::Color32::GRAY),
                                )
                                .selectable(false),
                            );
                            ui.add(
                                egui::ProgressBar::new((progress / 100.0) as f32)
                                    .desired_width(220.0),
                            );
                            ui.add_space(4.0);
                        }
                    }
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "Population" {
            let mut still_open = true;
            let window_response = egui::Window::new("Population Breakdown")
                .id(egui::Id::new("population_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("👥").size(18.0).color(theme::TEXT),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Population")
                                    .size(16.0)
                                    .strong()
                                    .color(theme::TEXT),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    // Collect colony data: (name, population, housing_cap, growth_per_year)
                    let mut pops: Vec<(String, f64, f64, f64)> = population_query
                        .iter()
                        .filter(|(p, _, _)| p.count > 0.0)
                        .map(|(p, body, colony_opt)| {
                            let name = if let Some(b) = body {
                                b.name.clone()
                            } else {
                                "Unknown".to_string()
                            };
                            let default_data = BuildingsData::default();
                            let data = power_popup_queries
                                .buildings_data
                                .as_deref()
                                .unwrap_or(&default_data);
                            let housing = colony_opt
                                .map(|c| c.housing_capacity(data))
                                .unwrap_or(0.0);
                            let growth_yr = colony_opt
                                .map(|c| c.population_growth_per_year(1.0, data))
                                .unwrap_or(0.0);
                            (name, p.count, housing, growth_yr)
                        })
                        .collect();

                    // Sort descending by population
                    pops.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                    // Max population for relative bar scaling
                    let max_pop = pops.first().map(|(_, c, _, _)| *c).unwrap_or(1.0).max(1.0);

                    let top_count = pops.len().min(10);

                    for (name, count, housing, growth_yr) in pops.iter().take(top_count) {
                        ui.add_space(3.0);
                        // Name (left) + population count + growth rate (right)
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(name.as_str()).size(11.0))
                                    .selectable(false),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let growth_month = growth_yr / 12.0;
                                    let (growth_text, growth_color) = if growth_month > 0.5 {
                                        (
                                            format!("+{}/mo", format_population(growth_month)),
                                            theme::GREEN,
                                        )
                                    } else if growth_month < -0.5 {
                                        (
                                            format!("{}/mo", format_population(growth_month)),
                                            theme::RED,
                                        )
                                    } else {
                                        ("\u{2014}".to_string(), theme::TEXT_DIM)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(growth_text)
                                                .size(10.0)
                                                .color(growth_color),
                                        )
                                        .selectable(false),
                                    );
                                    ui.add_space(4.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_population(*count))
                                                .size(11.0)
                                                .strong(),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                        });

                        // Progress bar: outer = relative to largest, inner = housing utilisation
                        let bar_fill = (*count / max_pop) as f32;
                        let housing_fill = if *housing > 0.0 {
                            (*count / housing).min(1.0) as f32
                        } else {
                            0.0
                        };
                        let bar_color = if housing_fill >= 0.99 {
                            theme::RED
                        } else if housing_fill > 0.85 {
                            theme::AMBER
                        } else {
                            theme::RB_HOUSING
                        };
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 4.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(rect.width() * bar_fill, rect.height()),
                            ),
                            0.0,
                            bar_color.linear_multiply(0.3),
                        );
                        if *housing > 0.0 {
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    rect.min,
                                    egui::vec2(
                                        rect.width() * bar_fill * housing_fill,
                                        rect.height(),
                                    ),
                                ),
                                0.0,
                                bar_color,
                            );
                        }
                        if *housing > 0.0 {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "  \u{1F3E0} {:.0}% of {}",
                                        housing_fill * 100.0,
                                        format_population(*housing)
                                    ))
                                    .size(9.0)
                                    .color(theme::TEXT_DIM),
                                )
                                .selectable(false),
                            );
                        }
                    }

                    // Summarize colonies beyond top 10
                    if pops.len() > 10 {
                        let other_total: f64 = pops.iter().skip(10).map(|(_, c, _, _)| c).sum();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new("Other").italics())
                                    .selectable(false),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_population(other_total))
                                                .italics(),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                        });
                    }

                    ui.separator();
                    // Total + aggregate growth rate
                    let default_data = BuildingsData::default();
                    let data = power_popup_queries
                        .buildings_data
                        .as_deref()
                        .unwrap_or(&default_data);
                    let total_growth_yr: f64 = population_query
                        .iter()
                        .filter_map(|(p, _, c)| {
                            if p.count > 0.0 {
                                Some(
                                    c.map(|col| col.population_growth_per_year(1.0, data))
                                        .unwrap_or(0.0),
                                )
                            } else {
                                None
                            }
                        })
                        .sum();
                    let total_growth_month = total_growth_yr / 12.0;
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Total").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (g_text, g_color) = if total_growth_month >= 1.0 {
                                (
                                    format!("+{}/mo", format_population(total_growth_month)),
                                    theme::GREEN,
                                )
                            } else if total_growth_month < -1.0 {
                                (
                                    format!("{}/mo", format_population(total_growth_month)),
                                    theme::RED,
                                )
                            } else {
                                ("\u{2014}".to_string(), theme::TEXT_DIM)
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(g_text).size(10.0).color(g_color),
                                )
                                .selectable(false),
                            );
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_population(total_population))
                                        .strong()
                                        .color(theme::TEXT),
                                )
                                .selectable(false),
                            );
                        });
                    });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if let Some((_, resources)) = ResourceType::by_category()
            .into_iter()
            .find(|(name, _)| *name == cat_name.as_str())
        {
            let color = get_category_color(cat_name);
            // v0.5.2 PR-A.4: category popup header gets the same
            // category-badge PNG as the top-bar tile, sized 22 px
            // to match the 16-pt bold heading. Falls back to the
            // cyan square via `render_category_icon` if the asset
            // hasn't loaded yet.

            let mut still_open = true;
            let window_response = egui::Window::new(cat_name.as_str())
                .id(egui::Id::new(format!("res_window_{}", cat_name)))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(280.0);
                    ui.horizontal(|ui| {
                        render_category_icon(
                            ui,
                            &ui_runtime.resource_icons,
                            cat_name.as_str(),
                            30.0,
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(cat_name.as_str())
                                    .size(16.0)
                                    .strong()
                                    .color(color),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    // Header + data rows in a single grid so columns stay aligned
                    egui::Grid::new(format!("res_popup_{}", cat_name))
                        .num_columns(3)
                        .spacing([20.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            // Header
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("Resource").strong().size(11.0),
                                )
                                .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("Stockpile").strong().size(11.0),
                                )
                                .selectable(false),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new("/mo").strong().size(11.0),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                            ui.end_row();

                            for resource in &resources {
                                let amount = contextual.get(resource);
                                let rate = rate_tracker.get_resource_rate(resource);

                                // Icon + Name in one cell — wrap in an
                                // allocating Ui so we can capture a
                                // Response for hover-mini-chart and click
                                // open-popup behavior. v0.5.2: switches
                                // from the legacy emoji glyph to the
                                // loaded PNG icon via
                                // `render_resource_name_row` (cyan-tinted
                                // to match the menu icons). The
                                // `ResourceIcons` resource is already in
                                // scope via `ui_runtime`.
                                let name_response = ui
                                    .horizontal(|ui| {
                                        render_resource_name_row(
                                            ui,
                                            &ui_runtime.resource_icons,
                                            *resource,
                                            12.0,
                                        );
                                    })
                                    .response;

                                // Hover-mini-chart: shows a compact 20-yr
                                // forecast preview for this resource.
                                name_response.clone().on_hover_ui(|ui| {
                                    render_resource_hover_preview(
                                        ui,
                                        &ui_runtime.resource_icons,
                                        *resource,
                                        &contextual,
                                        &rate_tracker,
                                        &breakdown_queries,
                                        current_sim_seconds,
                                    );
                                });

                                // Click on row → open per-resource popup
                                // anchored to the category popup rect.
                                if name_response.clicked() {
                                    let is_resource_open = open_popup
                                        .resource_open
                                        .as_ref()
                                        .is_some_and(|(r, _)| *r == *resource);
                                    if is_resource_open {
                                        open_popup.resource_open = None;
                                    } else if let Some((_, cat_rect)) = open_popup.open.clone() {
                                        // Anchor the resource popup to the
                                        // right of the category popup rect.
                                        open_popup.resource_open =
                                            Some((*resource, cat_rect));
                                    }
                                }
                                // Stockpile — left-aligned, with amber
                                // in-transit chip appended if any resource
                                // is currently in transit to a body in the
                                // current view context.
                                {
                                    let stock_color = if amount <= 0.0 {
                                        theme::RED
                                    } else if amount < 100.0 && resource.is_critical() {
                                        theme::AMBER
                                    } else {
                                        theme::TEXT
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_mass(amount))
                                                    .monospace()
                                                    .size(12.0)
                                                    .color(stock_color),
                                            )
                                            .selectable(false),
                                        );
                                        // v3.8.7 (2026-08-07): per-resource
                                        // cap-throttle lock.  Shown only
                                        // when at least one body in view
                                        // is at the *hard cap* (fill ≥
                                        // 1.0), not the 0.8 soft-knee.  The
                                        // soft-knee is communicated by the
                                        // orange fill bar in the per-body
                                        // breakdown; the lock is the
                                        // "production is at the consumption
                                        // floor" signal.
                                        {
                                            let per_body_cap =
                                                budget.effective_stockpile_cap(*resource);
                                            let any_body_at_cap = per_body_cap > 0.0
                                                && per_body_cap < f64::MAX
                                                && breakdown_queries
                                                    .per_body_breakdown
                                                    .iter()
                                                    .any(|(_, _, stockpile)| {
                                                        let current = stockpile.get(resource);
                                                        current / per_body_cap
                                                            >= 1.0 - 1e-9
                                                    });
                                            if any_body_at_cap {
                                                ui.add_space(3.0);
                                                let lock_response = ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new("🔒")
                                                            .size(10.0)
                                                            .color(theme::AMBER),
                                                    )
                                                    .selectable(false),
                                                );
                                                lock_response.on_hover_text(format!(
                                                    "{}: at the storage cap on at least one\n\
                                                     body in the current view. Production is\n\
                                                     throttled to that body's consumption\n\
                                                     draw (no net stockpile gain).\n\n\
                                                     Build more Warehouses / Resource Depots to\n\
                                                     expand the cap, or set up an off-world\n\
                                                     trade route to ship the surplus out.",
                                                    resource.display_name(),
                                                ));
                                            }
                                        }
                                        let in_transit: f64 = breakdown_queries
                                            .pending_resource_requests
                                            .requests
                                            .iter()
                                            .filter(|r| {
                                                r.resource == *resource
                                                    && r.state
                                                        == crate::economy::logistics::RequestState::InTransit
                                                    && in_view_context(
                                                        r.destination_body,
                                                        &breakdown_queries.per_body_breakdown,
                                                        &breakdown_queries.view_mode,
                                                        &breakdown_queries.current_star_system,
                                                    )
                                            })
                                            .map(|r| r.in_transit_mt)
                                            .sum();
                                        if in_transit > 0.0 {
                                            ui.add_space(4.0);
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "(+{} in transit)",
                                                        format_mass(in_transit)
                                                    ))
                                                    .italics()
                                                    .size(10.0)
                                                    .color(theme::AMBER),
                                                )
                                                .selectable(false),
                                            );
                                        }
                                    });
                                }
                                // Rate — right-aligned
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let (rt, rc) = format_rate_monthly(rate);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(rt)
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(rc),
                                            )
                                            .selectable(false),
                                        );
                                    },
                                );
                                ui.end_row();
                            }
                        });

                    // ── Per-body / per-system breakdown (GRA-31 PR-A) ──
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(2.0);
                    render_per_body_breakdown(
                        ui,
                        &resources,
                        &breakdown_queries.per_body_breakdown,
                        &breakdown_queries.pending_resource_requests,
                        *breakdown_queries.view_mode,
                        breakdown_queries.current_star_system.0,
                        &budget,
                    );
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else {
            // Category not found (shouldn't happen), close
            open_popup.open = None;
        }
    }

    // Resource popup — opens when the player clicks an individual
    // resource row in a category popup.  Anchored to the right of the
    // category popup (or below if the right edge is past the viewport).
    if let Some((resource, cat_anchor_rect)) = open_popup.resource_open {
        let anchor_right = cat_anchor_rect.right() + 4.0;
        let ctx_size = ctx.content_rect().size();
        let popup_w = 320.0_f32;
        let popup_h = 360.0_f32;
        // Prefer the right of the category popup; fall back to
        // `cat_anchor_rect.left()` (below the popup) when the right
        // anchor would overflow the viewport.
        let pos = if anchor_right + popup_w < ctx_size.x {
            egui::pos2(anchor_right, cat_anchor_rect.top())
        } else {
            egui::pos2(cat_anchor_rect.left(), cat_anchor_rect.bottom() + 4.0)
        };

        let mut still_open = true;
        let popup_id = egui::Id::new("resource_forecast_popup").with(resource);
        let window_response = egui::Window::new(format!("forecast_{}", resource.display_name()))
            .id(popup_id)
            .fixed_pos(pos)
            .fixed_size(egui::vec2(popup_w, popup_h))
            .collapsible(false)
            .resizable(false)
            .title_bar(true)
            .open(&mut still_open)
            .frame(egui::Frame::popup(ctx.style().as_ref()))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // v0.5.2: 16-px PNG icon for the resource
                    // (16 px so it sits visually next to the 16-px
                    // bold resource name; the legacy 16-pt emoji
                    // glyph rendered at variable width depending on
                    // the emoji codepoint).
                    render_resource_icon(ui, &ui_runtime.resource_icons, resource, 24.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(resource.display_name())
                                .strong()
                                .size(15.0)
                                .color(theme::forecast_series_color(resource.category())),
                        )
                        .selectable(false),
                    );
                });
                ui.add_space(2.0);
                let amount = contextual.get(&resource);
                ui.label(
                    egui::RichText::new(format!(
                        "Current: {}",
                        format_mass(amount)
                    ))
                    .monospace()
                    .size(11.0)
                    .color(theme::TEXT_VALUE),
                );

                let series = build_single_resource_forecast(
                    resource,
                    &contextual,
                    &rate_tracker,
                    &breakdown_queries,
                    current_sim_seconds,
                );

                ui.add_space(4.0);
                let desired = egui::vec2(popup_w - 24.0, 180.0);
                let refs = vec![&series];
                super::economy_panel::render_forecast_chart(ui, &refs, current_sim_seconds, desired, true);

                ui.add_space(4.0);
                let annual = series.annual_net_rate_mt;
                let sign = if annual >= 0.0 { "+" } else { "" };
                let color = if annual >= 0.0 { theme::GREEN } else { theme::RED };
                ui.label(
                    egui::RichText::new(format!(
                        "Net rate: {}{}/yr",
                        sign,
                        format_mass(annual.abs())
                    ))
                    .color(color)
                    .monospace()
                    .size(11.0),
                );
                if let Some(runs_out) = series.runs_out_at_s {
                    let years = runs_out / crate::economy::SECONDS_PER_YEAR;
                    let color = if years < 5.0 {
                        theme::RED
                    } else if years < 10.0 {
                        theme::AMBER
                    } else {
                        theme::GREEN
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "Runs out in {}",
                            if years < 1.0 {
                                format!("{:.0} mo", years * 12.0)
                            } else {
                                format!("{:.1} yr", years)
                            }
                        ))
                        .color(color)
                        .size(11.0),
                    );
                }
                if let Some(cap) = series.reserve_upper_bound_mt {
                    ui.label(
                        egui::RichText::new(format!(
                            "Survey-known reserves: {} (cap)",
                            format_mass(cap)
                        ))
                        .color(theme::AMBER)
                        .monospace()
                        .size(10.0),
                    );
                }
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(
                        "Click outside to close.  Open the Forecast tab in the Economy panel for the full multi-resource chart.",
                    )
                    .italics()
                    .size(9.0)
                    .color(theme::TEXT_DIM),
                );
            });

        // Outside-click lifecycle.
        if let Some(inner_response) = window_response {
            if ctx.input(|i| i.pointer.any_pressed()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !inner_response.response.rect.contains(pos) && !cat_anchor_rect.contains(pos)
                    {
                        open_popup.resource_open = None;
                    }
                }
            }
        }
        if !still_open {
            open_popup.resource_open = None;
        }
    }

    render_kardashev_overlay(
        ctx,
        &ui_runtime.resource_icons,
        &mut kardashev_trend,
        &sim_history,
        current_sim_seconds,
        current_year,
    );
}

// ── Resource forecast preview (hover mini-chart + click popup) ──────

/// Build a single-resource forecast series for the resource popup /
/// hover tooltip.  Lightweight wrapper that reads from the contextual
/// stockpile + per-entity rates.
fn build_single_resource_forecast(
    resource: crate::economy::ResourceType,
    contextual: &crate::economy::ContextualStockpile,
    rate_tracker: &ResourceRateTracker,
    breakdown_queries: &ResourceBarBreakdownQueries,
    current_sim_seconds: f64,
) -> crate::economy::ForecastSeries {
    // Aggregated current stock + annual rate for this single resource.
    let current_mt = contextual.get(&resource);
    // Per-entity rate sum across the active scope.
    let monthly_mt: f64 = match *breakdown_queries.view_mode {
        ViewMode::System => {
            let sys_id = breakdown_queries.current_star_system.0;
            rate_tracker
                .per_entity_rates
                .iter()
                .filter_map(|(entity, rates)| {
                    let (_body, sid_opt, _stock) =
                        breakdown_queries.per_body_breakdown.get(*entity).ok()?;
                    let body_sys = sid_opt.map(|s| s.0).unwrap_or(0);
                    if body_sys != sys_id {
                        return None;
                    }
                    rates.get(&resource).copied()
                })
                .sum()
        }
        ViewMode::Starmap => rate_tracker
            .per_entity_rates
            .values()
            .filter_map(|rates| rates.get(&resource).copied())
            .sum(),
    };
    let annual_mt = monthly_mt * 12.0;
    // Per-resource, per-scope reserve upper bound is intentionally
    // not threaded through the per-resource popup/hover path - that
    // mini-chart is a planning aid and the additional query wiring
    // would balloon the SystemParam surface.  See the Forecast sub-tab
    // for the reserve-aware variant.  v3.8.3: pass `None` for the
    // storage cap too (this is the per-resource mini-chart, not the
    // full forecast tab).
    let mut series =
        crate::economy::project_stockpile(current_mt, annual_mt, None, None);
    series.resource = resource;
    let _ = current_sim_seconds;
    series
}

/// Compact hover-tooltip preview chart shown when the player hovers a
/// resource row inside a category popup.  Renders a small
/// `FORECAST_MINI_CHART_HEIGHT` painter chart + a one-line summary.
fn render_resource_hover_preview(
    ui: &mut egui::Ui,
    icons: &super::resource_icons::ResourceIcons,
    resource: crate::economy::ResourceType,
    contextual: &crate::economy::ContextualStockpile,
    rate_tracker: &ResourceRateTracker,
    breakdown_queries: &ResourceBarBreakdownQueries,
    current_sim_seconds: f64,
) {
    ui.set_max_width(260.0);
    // v0.5.2: tooltip header uses the same PNG icon as the main
    // popup grid via `render_resource_name_row` (cyan-tinted to
    // match the menu icons). The forecast tooltip is now
    // visually consistent with the resource row it pops up from.
    render_resource_name_row(ui, icons, resource, 13.0);
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new("20-yr forecast")
            .strong()
            .size(10.0)
            .color(theme::forecast_series_color(resource.category())),
    );
    ui.add_space(2.0);

    let series = build_single_resource_forecast(
        resource,
        contextual,
        rate_tracker,
        breakdown_queries,
        current_sim_seconds,
    );

    let desired = egui::vec2(240.0, theme::FORECAST_MINI_CHART_HEIGHT);
    render_resource_mini_chart(ui, &series, desired);

    ui.add_space(2.0);
    let annual = series.annual_net_rate_mt;
    if let Some(runs_out) = series.runs_out_at_s {
        let years = runs_out / crate::economy::SECONDS_PER_YEAR;
        let color = if years < 5.0 {
            theme::RED
        } else if years < 10.0 {
            theme::AMBER
        } else {
            theme::GREEN
        };
        ui.label(
            egui::RichText::new(format!(
                "Runs out in {}",
                if years < 1.0 {
                    format!("{:.0} mo", years * 12.0)
                } else {
                    format!("{:.1} yr", years)
                }
            ))
            .color(color)
            .size(11.0),
        );
        ui.label(
            egui::RichText::new(format!("Net {:+} /yr", format_mass(annual.abs())))
                .color(if annual < 0.0 {
                    theme::RED
                } else {
                    theme::GREEN
                })
                .size(10.0)
                .monospace(),
        );
    } else {
        ui.label(
            egui::RichText::new(if annual < 0.0 {
                format!("Net {:+} /yr", format_mass(annual.abs()))
            } else {
                format!("Sustainable · net {:+} /yr", format_mass(annual.abs()))
            })
            .color(if annual < 0.0 {
                theme::RED
            } else {
                theme::GREEN
            })
            .size(11.0),
        );
    }
}

/// Render a small single-series forecast chart — used by the
/// hover-tooltip preview and the per-resource popup.  Includes:
/// - x-axis labels (0y / 5y / 10y / 15y / 20y)
/// - y-axis label (max stockpile at top, rotated 90° vertical)
/// - "now" vertical line at x=0 in the series colour (so the
///   player can see today's value vs. the projected curve)
/// - dashed "runs out" marker if applicable
/// - dashed amber "reserve cap" marker if the curve plateaus
/// - a soft t=20y grid line
/// - a smooth cap-crossing vertex so the line doesn't spike at the cap
fn project_to_screen(
    p: &crate::economy::ForecastSample,
    plot_rect: &egui::Rect,
    horizon_s: f64,
    min_y: f64,
    max_y: f64,
) -> egui::Pos2 {
    let x_t = (p.sim_seconds_offset / horizon_s).clamp(0.0, 1.0);
    let y_t = if (max_y - min_y).abs() < 1e-9 {
        0.5
    } else {
        ((p.value_mt - min_y) / (max_y - min_y)).clamp(0.0, 1.0)
    };
    egui::pos2(
        plot_rect.left() + plot_rect.width() * x_t as f32,
        plot_rect.bottom() - plot_rect.height() * y_t as f32,
    )
}

fn render_resource_mini_chart(
    ui: &mut egui::Ui,
    series: &crate::economy::ForecastSeries,
    desired_size: egui::Vec2,
) {
    let sense = egui::Sense::hover();
    let (rect, _) = ui.allocate_exact_size(desired_size, sense);
    let painter = ui.painter();

    painter.rect_filled(rect, 4.0, theme::SURFACE);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, theme::BORDER),
        egui::StrokeKind::Outside,
    );

    if series.samples.len() < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No data",
            theme::body(11.0),
            theme::TEXT_DIM,
        );
        return;
    }

    // Shrink rect to leave room for axis labels (left margin for y,
    // bottom margin for x).  Same convention as the full chart.
    let plot_rect = rect.shrink2(egui::vec2(34.0, 14.0));
    let horizon_s = crate::economy::FORECAST_HORIZON_YEARS * crate::economy::SECONDS_PER_YEAR;
    let max_y = series
        .samples
        .iter()
        .map(|p| p.value_mt)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(0.0)
        .max(1.0);
    let min_y = 0.0_f64;

    let stroke_color = theme::forecast_series_color(series.resource.category());

    // ── Y-axis: rotated 90° so it fits in a 12 px column without
    // clipping into the tooltip padding.  Top label is the max-stockpile
    // magnitude ("107 Mt"); bottom is "0".  Both rotated, anchored to
    // the plot_rect left edge.
    //
    // Note: `painter.text` with a rotated galley requires `text_with_rotation`
    // on older egui; this version uses the standard `text` API and a
    // horizontal label trimmed to the magnitude unit ("Mt", "kt", etc.).
    // Trade-off: a few extra pixels of width, no rotation.
    let max_label = format_mass(max_y);
    // Trim to the unit suffix to save width: "107 Mt" → "Mt".
    let unit_only = max_label
        .rsplit_once(' ')
        .map(|(_, unit)| unit.to_string())
        .unwrap_or_else(|| max_label.clone());
    painter.text(
        egui::pos2(plot_rect.left() - 4.0, plot_rect.top()),
        egui::Align2::RIGHT_TOP,
        unit_only.clone(),
        theme::mono(9.0),
        theme::TEXT_HINT,
    );
    painter.text(
        egui::pos2(plot_rect.left() - 4.0, plot_rect.bottom()),
        egui::Align2::RIGHT_BOTTOM,
        "0",
        theme::mono(9.0),
        theme::TEXT_HINT,
    );

    // ── X-axis labels: 0y / 5y / 10y / 15y / 20y ──
    for tick in 0..=4 {
        let t = tick as f32 / 4.0;
        let x = egui::lerp(plot_rect.left()..=plot_rect.right(), t);
        let years = (tick as f64) * 5.0;
        painter.text(
            egui::pos2(x, plot_rect.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            if years < 1.0 {
                "0y".to_string()
            } else {
                format!("{years:.0}y")
            },
            theme::mono(9.0),
            theme::TEXT_HINT,
        );
        // Light vertical grid line.
        painter.line_segment(
            [
                egui::pos2(x, plot_rect.top()),
                egui::pos2(x, plot_rect.bottom()),
            ],
            egui::Stroke::new(0.5_f32, theme::BORDER.linear_multiply(0.3)),
        );
    }

    // ── "Now" vertical line (subtle, series color, full-height) ──
    let x_now = plot_rect.left();
    painter.line_segment(
        [
            egui::pos2(x_now, plot_rect.top()),
            egui::pos2(x_now, plot_rect.bottom()),
        ],
        egui::Stroke::new(1.0_f32, stroke_color.linear_multiply(0.6)),
    );

    // ── Single line series ──
    // Build the line points, inserting an interpolated vertex at the
    // cap-crossing instant so the line transitions smoothly from
    // rising to flat at the cap instead of forming a sharp spike.
    let mut line_points: Vec<egui::Pos2> = Vec::with_capacity(series.samples.len() + 1);
    for w in series.samples.windows(2) {
        let a = &w[0];
        let b = &w[1];
        let pos_a = project_to_screen(a, &plot_rect, horizon_s, min_y, max_y);
        line_points.push(pos_a);
        // Detect cap-crossing between a and b.
        if let Some(cap) = series.reserve_upper_bound_mt {
            // Was a below the cap and b at the cap?
            let a_below = a.value_mt < cap;
            let b_at_cap = (b.value_mt - cap).abs() < 1e-6;
            if a_below && b_at_cap && (b.value_mt - a.value_mt) > 1e-6 {
                let frac = (cap - a.value_mt) / (b.value_mt - a.value_mt);
                let offset = (b.sim_seconds_offset - a.sim_seconds_offset) * frac;
                let cross_offset = a.sim_seconds_offset + offset;
                let x_t = (cross_offset / horizon_s).clamp(0.0, 1.0);
                line_points.push(egui::pos2(
                    plot_rect.left() + plot_rect.width() * x_t as f32,
                    plot_rect.top(),
                ));
            }
        }
    }
    if let Some(last) = series.samples.last() {
        line_points.push(project_to_screen(last, &plot_rect, horizon_s, min_y, max_y));
    }
    painter.add(egui::Shape::line(
        line_points,
        egui::Stroke::new(theme::FORECAST_LINE_WIDTH, stroke_color),
    ));

    // ── Dashed "runs out" marker (red) ──
    if let Some(runs_out) = series.runs_out_at_s {
        if runs_out < horizon_s {
            let x_t = (runs_out / horizon_s).clamp(0.0, 1.0);
            let x = plot_rect.left() + plot_rect.width() * x_t as f32;
            let dash_len = theme::FORECAST_RUNS_OUT_DASH_LEN;
            let runs_out_color = theme::forecast_runs_out_color();
            let mut y = plot_rect.top();
            while y < plot_rect.bottom() {
                let next_y = (y + dash_len).min(plot_rect.bottom());
                painter.line_segment(
                    [egui::pos2(x, y), egui::pos2(x, next_y)],
                    egui::Stroke::new(theme::FORECAST_RUNS_OUT_STROKE_WIDTH, runs_out_color),
                );
                y = next_y + dash_len;
            }
            // Floating label.
            let years = runs_out / crate::economy::SECONDS_PER_YEAR;
            painter.text(
                egui::pos2(x - 2.0, plot_rect.top() + 1.0),
                egui::Align2::RIGHT_TOP,
                format!("{years:.1}y"),
                theme::mono(8.0),
                runs_out_color,
            );
        }
    }

    // ── Dashed "reserve cap" marker (amber) ──
    if let Some(cap_at) = series.hits_reserve_cap_at_s {
        if cap_at < horizon_s {
            let x_t = (cap_at / horizon_s).clamp(0.0, 1.0);
            let x = plot_rect.left() + plot_rect.width() * x_t as f32;
            let dash_len = theme::FORECAST_RUNS_OUT_DASH_LEN;
            let cap_color = theme::AMBER;
            let mut y = plot_rect.top();
            while y < plot_rect.bottom() {
                let next_y = (y + dash_len).min(plot_rect.bottom());
                painter.line_segment(
                    [egui::pos2(x, y), egui::pos2(x, next_y)],
                    egui::Stroke::new(theme::FORECAST_RUNS_OUT_STROKE_WIDTH, cap_color),
                );
                y = next_y + dash_len;
            }
            // Floating label.
            let years = cap_at / crate::economy::SECONDS_PER_YEAR;
            painter.text(
                egui::pos2(x + 2.0, plot_rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                format!("cap {years:.1}y"),
                theme::mono(8.0),
                cap_color,
            );
        }
    }
}

// ── GRA-31 PR-A: per-body / per-system breakdown helpers ──────────────

/// Is a `body_entity` (typically a `ResourceRequest::destination_body`)
/// part of the player's current view context?  In **System** view only
/// bodies in `current_star_system` count; in **Starmap** view all bodies
/// count.  Bodies without a `SystemId` component are excluded.
fn in_view_context(
    body_entity: Entity,
    per_body_breakdown: &Query<(
        Option<&crate::plugins::solar_system::CelestialBody>,
        Option<&crate::astronomy::components::SystemId>,
        &crate::economy::components::LocalStockpile,
    )>,
    view_mode: &ViewMode,
    current_star_system: &CurrentStarSystem,
) -> bool {
    let Ok((_body, sid_opt, _stockpile)) = per_body_breakdown.get(body_entity) else {
        return false;
    };
    let Some(sid) = sid_opt else {
        return false;
    };
    match *view_mode {
        ViewMode::System => sid.0 == current_star_system.0,
        ViewMode::Starmap => true,
    }
}

/// Render the per-body / per-system breakdown table that appears in the
/// resource category popup (GRA-31 PR-A).
///
/// v3.8.1: render a single per-body fill cell in the per-body
/// breakdown table. Shows a small progress bar (40px wide) plus
/// the fill % as text. The bar colour matches the v0.5.2 PR-A.4
/// fill-ratio bands so the player can see at a glance which
/// bodies are at the soft-knee (80%+) and are being throttled.
fn render_fill_cell(ui: &mut egui::Ui, fill_ratio: f64) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let bar_color = if fill_ratio >= 0.95 {
            theme::RED
        } else if fill_ratio >= 0.80 {
            egui::Color32::from_rgb(255, 165, 0) // orange = soft-knee
        } else if fill_ratio >= 0.60 {
            egui::Color32::from_rgb(255, 215, 0) // yellow
        } else {
            egui::Color32::from_rgb(80, 200, 120) // green
        };
        let bar_rect = ui.allocate_space(egui::vec2(40.0, 6.0)).1;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(bar_rect.min, egui::vec2(40.0, 6.0)),
            1.0,
            egui::Color32::from_gray(50),
        );
        let fill_w = (40.0 * fill_ratio as f32).max(0.0);
        if fill_w > 0.0 {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 6.0)),
                1.0,
                bar_color,
            );
        }
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("{:>3.0}%", fill_ratio * 100.0))
                    .monospace()
                    .size(10.0)
                    .color(if fill_ratio >= 0.80 {
                        theme::AMBER
                    } else {
                        theme::TEXT_DIM
                    }),
            )
            .selectable(false),
        );
    });
}

/// - **System view** → list every body in the current system that holds
///   any of the resources in `resources`, sorted by total stockpile desc.
/// - **Starmap view** → group by system, show system subtotals, list
///   bodies within each system sorted by total stockpile desc.
#[allow(clippy::too_many_arguments)]
fn render_per_body_breakdown(
    ui: &mut egui::Ui,
    resources: &[crate::economy::ResourceType],
    per_body_breakdown: &Query<(
        Option<&crate::plugins::solar_system::CelestialBody>,
        Option<&crate::astronomy::components::SystemId>,
        &crate::economy::components::LocalStockpile,
    )>,
    pending_resource_requests: &crate::economy::logistics::PendingResourceRequests,
    view_mode: ViewMode,
    current_star_id: usize,
    budget: &GlobalBudget,
) {
    // v3.8.1: per-body cap = sum of `effective_stockpile_cap(r)` across
    // the resources in this category.  The cap is the same for every
    // body (storage_multiplier is global), so we can compute it once
    // outside the body loop and reuse it.
    let body_cap: f64 = resources
        .iter()
        .map(|r| budget.effective_stockpile_cap(*r))
        .sum();
    let cap_is_meaningful = body_cap > 0.0 && body_cap < f64::MAX;

    // Collect (body_name, system_id, total_for_category, per_resource) rows
    // for the bodies that hold at least one of the resources in `resources`.
    let mut rows: Vec<BreakdownRow> = Vec::new();
    for (body_opt, sid_opt, stockpile) in per_body_breakdown.iter() {
        let body_name = body_opt
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let Some(sid) = sid_opt else {
            continue;
        };
        if matches!(view_mode, ViewMode::System) && sid.0 != current_star_id {
            continue;
        }
        let total: f64 = resources.iter().map(|r| stockpile.get(r)).sum();
        if total <= 0.0 {
            continue;
        }
        let fill_ratio = if cap_is_meaningful {
            (total / body_cap).clamp(0.0, 1.0)
        } else {
            0.0
        };
        rows.push(BreakdownRow {
            body_name,
            system_id: sid.0,
            total,
            fill_ratio,
        });
    }
    rows.sort_by(|a, b| {
        b.total
            .partial_cmp(&a.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if rows.is_empty() {
        return;
    }

    let label = match view_mode {
        ViewMode::System => "Per-body in current system",
        ViewMode::Starmap => "Per-system breakdown",
    };
    ui.add(egui::Label::new(egui::RichText::new(label).strong().size(11.0)).selectable(false));
    ui.add_space(2.0);

    // Sum pending (Pending + Assigned + InTransit) requests per body so the
    // player sees incoming supply, not just the on-hand amount.
    let incoming_per_body: std::collections::HashMap<Entity, f64> = {
        let mut map: std::collections::HashMap<Entity, f64> = std::collections::HashMap::new();
        for r in &pending_resource_requests.requests {
            if !r.is_open() {
                continue;
            }
            if !resources.contains(&r.resource) {
                continue;
            }
            *map.entry(r.destination_body).or_insert(0.0) += r.amount_mt;
        }
        map
    };

    // v3.8.1: 4 columns now — Body, Stockpile, Fill, Incoming.  The
    // Fill column shows the per-body cap-throttle state so the
    // player can see at a glance which bodies are past the soft
    // knee (80% fill) and are having their production reduced.
    egui::Grid::new("res_popup_breakdown")
        .num_columns(4)
        .spacing([16.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new("Body").strong().size(10.0)).selectable(false),
            );
            ui.add(
                egui::Label::new(egui::RichText::new("Stockpile").strong().size(10.0))
                    .selectable(false),
            );
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new("Fill").strong().size(10.0))
                        .selectable(false),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new("Incoming").strong().size(10.0))
                        .selectable(false),
                );
            });
            ui.end_row();

            match view_mode {
                ViewMode::System => {
                    for row in &rows {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&row.body_name).size(11.0))
                                .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format_mass(row.total))
                                    .monospace()
                                    .size(11.0),
                            )
                            .selectable(false),
                        );
                        render_fill_cell(ui, row.fill_ratio);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Look up incoming by walking the request list
                            // for the body name.  Bodies are not addressable
                            // by name from the requests struct, so we
                            // match by entity.  The total here is across
                            // all resources in the category.
                            let _ = incoming_per_body; // keep the symbol
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("—").size(10.0).color(theme::TEXT_DIM),
                                )
                                .selectable(false),
                            );
                        });
                        ui.end_row();
                    }
                }
                ViewMode::Starmap => {
                    // Group rows by system.
                    let mut by_system: std::collections::BTreeMap<usize, Vec<&BreakdownRow>> =
                        std::collections::BTreeMap::new();
                    for row in &rows {
                        by_system.entry(row.system_id).or_default().push(row);
                    }
                    for (system_id, group) in &by_system {
                        let subtotal: f64 = group.iter().map(|r| r.total).sum();
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("System {system_id}"))
                                    .strong()
                                    .size(11.0)
                                    .color(theme::ACCENT),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format_mass(subtotal))
                                    .monospace()
                                    .strong()
                                    .size(11.0),
                            )
                            .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new("").size(10.0))
                                    .selectable(false),
                            );
                        });
                        ui.end_row();

                        for row in group {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("  {}", row.body_name)).size(11.0),
                                )
                                .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_mass(row.total))
                                        .monospace()
                                        .size(11.0),
                                )
                                .selectable(false),
                            );
                            render_fill_cell(ui, row.fill_ratio);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new("—")
                                                .size(10.0)
                                                .color(theme::TEXT_DIM),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                            ui.end_row();
                        }
                    }
                }
            }
        });
}

struct BreakdownRow {
    body_name: String,
    system_id: usize,
    total: f64,
    /// v3.8.1: per-body CATEGORY fill ratio (0..1). The
    /// category cap is the same for every body (storage
    /// multiplier is global) so this is computed once per
    /// row from `total` + the precomputed `body_cap`.
    fill_ratio: f64,
}

pub(super) fn format_population(count: f64) -> String {
    if count < 1_000.0 {
        return format!("{:.0}", count);
    }
    if count < 1_000_000.0 {
        return format!("{:.1} k", count / 1_000.0);
    }
    if count < 1_000_000_000.0 {
        return format!("{:.1} M", count / 1_000_000.0);
    }
    format!("{:.2} B", count / 1_000_000_000.0)
}
