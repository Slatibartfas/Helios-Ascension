use bevy::ecs::system::SystemParam;

use super::dashboard::{format_mass, format_rate_monthly};
use super::research_panel::{render_research_tech_tooltip_content, ActiveProjectInfo};
use super::time::{
    estimate_engineering_project_end_timestamp, estimate_research_project_end_timestamp,
    format_timestamp_date_time,
};
use super::*;

fn get_resource_category_icon(category: &str) -> &'static str {
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

/// Get color for resource category
fn get_category_color(category: &str) -> egui::Color32 {
    theme::category_color(category)
}

/// Resource popup that is currently open (if any)
#[derive(Resource, Default)]
pub(super) struct OpenResourcePopup {
    /// Which category is open, and where to anchor the popup
    open: Option<(String, egui::Rect)>,
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
    PowerConsumed,
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
            Self::PowerConsumed => "Power Consumed",
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
            Self::PowerConsumed => egui::Color32::from_rgb(255, 161, 94),
            Self::Population => egui::Color32::from_rgb(116, 224, 170),
            Self::Colonies => egui::Color32::from_rgb(236, 197, 96),
            Self::Ships => egui::Color32::from_rgb(120, 178, 255),
            Self::SurveyCoverage | Self::SurveyedBodies => {
                egui::Color32::from_rgb(121, 235, 210)
            }
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
            Self::PowerConsumed => "Power Consumption History".to_string(),
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
    sample
        .survey
        .total_bodies
        .saturating_sub(sample.survey.unsurveyed)
}

fn build_kardashev_history(
    simulation_history: &crate::economy::SimulationHistory,
    current_sim_seconds: f64,
) -> Vec<HistoryPoint> {
    simulation_history
        .samples_within_window(current_sim_seconds, HISTORY_PANEL_SECONDS)
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

fn format_history_value(
    metric: HistoryPanelMetric,
    value: f64,
    resource: ResourceType,
) -> String {
    match metric {
        HistoryPanelMetric::Kardashev => format!("Type {value:.3}"),
        HistoryPanelMetric::PowerProduced | HistoryPanelMetric::PowerConsumed => {
            format_power(value.max(0.0))
        }
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
                    Some(format!("Power consumed {}", format_power(sample.power_consumed_watts))),
                ),
                HistoryPanelMetric::PowerConsumed => (
                    sample.power_consumed_watts,
                    Some(format!("Power produced {}", format_power(sample.power_produced_watts))),
                ),
                HistoryPanelMetric::Population => (
                    sample.total_population,
                    Some(format!("Colonies {}", sample.colony_count)),
                ),
                HistoryPanelMetric::Colonies => (
                    sample.colony_count as f64,
                    Some(format!("Population {}", format_population(sample.total_population))),
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
                        Some(format!("{surveyed}/{} bodies surveyed", sample.survey.total_bodies)),
                    )
                }
                HistoryPanelMetric::SurveyedBodies => {
                    let surveyed = surveyed_body_count(sample);
                    (
                        surveyed as f64,
                        Some(format!("{} total bodies tracked", sample.survey.total_bodies)),
                    )
                }
                HistoryPanelMetric::ResourceStockpile => (
                    sample.resource_amount(resource),
                    Some(format!("{} ({})", resource.display_name(), resource.symbol())),
                ),
                HistoryPanelMetric::ResourceNetRate => (
                    sample.resource_net_rate(resource),
                    Some(format!("{} net monthly flow", resource.display_name())),
                ),
                HistoryPanelMetric::ResourceProduction => (
                    sample.resource_gross_production_rate(resource),
                    Some(format!("{} gross monthly production", resource.display_name())),
                ),
                HistoryPanelMetric::ResourceConsumption => (
                    sample.resource_gross_consumption_rate(resource),
                    Some(format!("{} gross monthly consumption", resource.display_name())),
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
            format!("Power production: {}", format_power(sample.power_produced_watts.max(1.0)))
        }
        (HistoryPanelMetric::PowerProduced, Some(sample)) => {
            format!("Current consumption: {}", format_power(sample.power_consumed_watts))
        }
        (HistoryPanelMetric::PowerConsumed, Some(sample)) => {
            format!("Current production: {}", format_power(sample.power_produced_watts))
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
    let years_ago = ((current_sim_seconds - target_sim_seconds) / crate::economy::SECONDS_PER_YEAR)
        .max(0.0);
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
        egui::Stroke::new(1.0, theme::BORDER),
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
            [egui::pos2(x, plot_rect.top()), egui::pos2(x, plot_rect.bottom())],
            egui::Stroke::new(1.0, theme::BORDER.linear_multiply(0.6)),
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
            [egui::pos2(plot_rect.left(), y), egui::pos2(plot_rect.right(), y)],
            egui::Stroke::new(1.0, theme::BORDER.linear_multiply(0.35)),
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
        egui::Stroke::new(2.0, series.accent),
    ));

    if let Some(last) = series.points.last() {
        let current_pos = to_screen(last.sim_seconds, last.value);
        painter.circle_filled(current_pos, 3.5, theme::ACCENT);
        painter.circle_stroke(
            current_pos,
            5.0,
            egui::Stroke::new(1.0, theme::ACCENT_DIM),
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
                egui::Stroke::new(1.0, theme::ACCENT),
            );
            painter.line_segment(
                [
                    egui::pos2(plot_rect.left(), nearest_pos.y),
                    egui::pos2(plot_rect.right(), nearest_pos.y),
                ],
                egui::Stroke::new(1.0, theme::ACCENT_DIM),
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
    let latest_value = series.points.last().map(|point| point.value).unwrap_or(raw_max);
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
    trend_state: &mut KardashevTrendState,
    simulation_history: &crate::economy::SimulationHistory,
    current_sim_seconds: f64,
    current_year: f64,
) {
    if !trend_state.detail_open {
        return;
    }

    let escape_pressed = ctx.input_mut(|input| {
        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
    });
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

            ui.add_space(8.0);
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
                            HistoryPanelMetric::PowerConsumed,
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
                        .selected_text(format!(
                            "{} {}",
                            get_resource_icon(&trend_state.resource),
                            trend_state.resource.display_name()
                        ))
                        .show_ui(ui, |ui| {
                            for resource in ResourceType::all() {
                                ui.selectable_value(
                                    &mut trend_state.resource,
                                    *resource,
                                    format!(
                                        "{} {}",
                                        get_resource_icon(resource),
                                        resource.display_name()
                                    ),
                                );
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

            ui.add_space(8.0);
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

            ui.add_space(8.0);
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
}

const CONTEXT_TILE_WIDTH: f32 = 88.0;
const CONTEXT_TILE_HEIGHT: f32 = 28.0;
const CONTEXT_NAME_FONT_SIZE: f32 = 11.5;

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
    mut open_popup: Local<OpenResourcePopup>,
    research_projects: Query<&ResearchProject>,
    engineering_projects: Query<&EngineeringProject>,
    research_teams: Query<&ResearchTeam>,
    technologies: Res<TechnologiesData>,
    sim_history: Res<crate::economy::SimulationHistory>,
    ui_runtime: ResourceBarUiRuntime,
    mut kardashev_trend: Local<KardashevTrendState>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let sim_time = &ui_runtime.sim_time;
    let time = &ui_runtime.time;
    let ui_prefs = &ui_runtime.ui_prefs;

    let current_sim_seconds = sim_time.elapsed_seconds();
    let current_power = budget.energy_grid.produced.max(1.0);
    let current_kardashev = crate::economy::kardashev_scale_from_watts(current_power);
    let current_year = current_calendar_year(sim_time);
    let kardashev_history = build_kardashev_history(&sim_history, current_sim_seconds);

    // Calculate total population
    let total_population: f64 = population_query.iter().map(|(p, _, _)| p.count).sum();

    egui::TopBottomPanel::top("resources_bar")
        .exact_height(40.0)
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

                    let icon = get_resource_category_icon(category_name);
                    let color = get_category_color(category_name);
                    let text_color = theme::TEXT;

                    let is_this_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == category_name);

                    // Use a Frame for the category display
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(icon).size(16.0).color(color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(68.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(68.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_mass(category_total))
                                                .size(13.0)
                                                .color(text_color),
                                        )
                                        .selectable(false),
                                    );
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
                            egui::Stroke::new(1.0, color),
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
                            if flash > 0.0 { 2.0 } else { 0.0 },
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
                            egui::Stroke::new(1.0, rp_color),
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
                            if flash > 0.0 { 2.0 } else { 0.0 },
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
                            egui::Stroke::new(1.0, ep_color),
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
                            egui::Stroke::new(1.0, theme::CAT_STRATEGIC),
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
                    // Color code power: Green if surplus, Red if deficit
                    let net_power = budget.net_power();
                    let power_color = if net_power >= 0.0 {
                        theme::GREEN
                    } else {
                        theme::RED
                    };

                    let is_power_open = open_popup.open.as_ref().is_some_and(|(n, _)| n == "Power");

                    // Power generation display (clickable with tooltip)
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(82.0); // Fixed width to prevent wiggling
                                ui.set_max_width(82.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "⚡ {}",
                                            format_power(budget.energy_grid.produced)
                                        ))
                                        .size(14.0)
                                        .strong()
                                        .color(power_color),
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
                            egui::Stroke::new(1.0, power_color),
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
                            egui::Stroke::new(1.0, treasury_color),
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
                            egui::Stroke::new(1.0, theme::TEXT),
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
            // Determine color from budget - recalculate here
            let net_power = budget.net_power();
            let power_color = if net_power >= 0.0 {
                theme::GREEN
            } else {
                theme::RED
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
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("⚡").size(18.0).color(power_color),
                            )
                            .selectable(false),
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
                            let housing = colony_opt.map(|c| c.housing_capacity()).unwrap_or(0.0);
                            let growth_yr = colony_opt
                                .map(|c| c.population_growth_per_year(1.0))
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
                            egui::Color32::from_rgb(100, 180, 255)
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
                    let total_growth_yr: f64 = population_query
                        .iter()
                        .filter_map(|(p, _, c)| {
                            if p.count > 0.0 {
                                Some(
                                    c.map(|col| col.population_growth_per_year(1.0))
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
            let icon = get_resource_category_icon(cat_name);
            let color = get_category_color(cat_name);

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
                        ui.add(
                            egui::Label::new(egui::RichText::new(icon).size(18.0).color(color))
                                .selectable(false),
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

                                // Icon + Name in one cell
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(get_resource_icon(resource))
                                                .size(14.0),
                                        )
                                        .selectable(false),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(resource.display_name()).size(12.0),
                                        )
                                        .selectable(false),
                                    );
                                });
                                // Stockpile — left-aligned
                                {
                                    let stock_color = if amount <= 0.0 {
                                        theme::RED
                                    } else if amount < 100.0 && resource.is_critical() {
                                        theme::AMBER
                                    } else {
                                        theme::TEXT
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_mass(amount))
                                                .monospace()
                                                .size(12.0)
                                                .color(stock_color),
                                        )
                                        .selectable(false),
                                    );
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

    render_kardashev_overlay(
        ctx,
        &mut kardashev_trend,
        &sim_history,
        current_sim_seconds,
        current_year,
    );
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
