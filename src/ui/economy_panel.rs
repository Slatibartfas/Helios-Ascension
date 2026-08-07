use super::dashboard::format_mass;
use super::resources_bar::{get_resource_category_icon, get_resource_icon};
use super::tab::Tab;
use super::*;
use bevy::ecs::system::SystemParam;
use std::borrow::Cow;

/// Persisted state for the economy panel's selected tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EconomyTab {
    #[default]
    Overview,
    Resources,
    Colonies,
    /// 20-year forward projection of resource stockpiles, with
    /// pending-construction awareness.  Lands in the slot previously
    /// occupied by `Mining` (now `Mining = 4`); old saved `byte=3`
    /// silently remaps to `Mining` via `from_byte`'s default branch.
    Forecast,
    Mining,
    PowerGrid,
    Logistics,
    PrivateShipping,
}

impl EconomyTab {
    /// All eight variants in display order. Used by
    /// `theme::tab_strip` to render the panel's sub-tab strip and
    /// to gate the active-tab styling in `ui_economy_panels`. The
    /// `Default` variant (Overview) is the first slot by the
    /// `Default`-first convention from `Tab`.
    const ALL: [EconomyTab; 8] = [
        EconomyTab::Overview,
        EconomyTab::Resources,
        EconomyTab::Colonies,
        EconomyTab::Forecast,
        EconomyTab::Mining,
        EconomyTab::PowerGrid,
        EconomyTab::Logistics,
        EconomyTab::PrivateShipping,
    ];

    /// Stable `u8` discriminator for `egui::data::get_persisted` /
    /// `insert_persisted`. Replaces the previous `From<u8>` /
    /// `From<EconomyTab>` pair so the byte survives the Tab-trait
    /// migration without a backwards-compat shim.
    fn to_byte(self) -> u8 {
        match self {
            EconomyTab::Overview => 0,
            EconomyTab::Resources => 1,
            EconomyTab::Colonies => 2,
            EconomyTab::Forecast => 3,
            EconomyTab::Mining => 4,
            EconomyTab::PowerGrid => 5,
            EconomyTab::Logistics => 6,
            EconomyTab::PrivateShipping => 7,
        }
    }

    fn from_byte(v: u8) -> Self {
        match v {
            1 => EconomyTab::Resources,
            2 => EconomyTab::Colonies,
            3 => EconomyTab::Forecast,
            4 => EconomyTab::Mining,
            5 => EconomyTab::PowerGrid,
            6 => EconomyTab::Logistics,
            7 => EconomyTab::PrivateShipping,
            _ => EconomyTab::Overview,
        }
    }
}

impl Tab for EconomyTab {
    fn id(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Resources => "resources",
            Self::Colonies => "colonies",
            Self::Forecast => "forecast",
            Self::Mining => "mining",
            Self::PowerGrid => "power_grid",
            Self::Logistics => "logistics",
            Self::PrivateShipping => "private_shipping",
        }
    }

    fn label(&self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Self::Overview => "Overview",
            Self::Resources => "Resources",
            Self::Colonies => "Colonies",
            Self::Forecast => "Forecast",
            Self::Mining => "Mining",
            Self::PowerGrid => "Power Grid",
            Self::Logistics => "Logistics",
            Self::PrivateShipping => "Private Shipping",
        })
    }

    fn icon(&self) -> Option<&'static str> {
        match self {
            Self::Overview => Some("📊"),
            Self::Resources => Some("📦"),
            Self::Colonies => Some("🏠"),
            Self::Forecast => Some("📈"),
            Self::Mining => Some("⛏"),
            Self::PowerGrid => Some("⚡"),
            Self::Logistics => Some("🚚"),
            Self::PrivateShipping => Some("🚢"),
        }
    }
}

/// Per-colony ledger row marker used by `theme::ledger_panel`
/// in both the Economy panel's "📋 Buildings" breakdown and the
/// Construction panel's "Resource Depletion" list. The same T
/// token makes the cross-panel reuse explicit.
#[allow(dead_code)]
pub(super) struct ColonyRow;

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
        ui.set_min_width(142.0);
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

fn draw_tab_h1(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    // PR-F (GRA-71) replaces the bespoke `draw_section_title` with
    // a thin wrapper over `theme::section_h1` + an optional
    // subtitle. The title size and accent color match the
    // `theme::section_h1` primitive; the subtitle is the 11pt dim
    // line that used to live in `draw_section_title`.
    theme::section_h1(ui, title);
    if !subtitle.is_empty() {
        ui.label(
            egui::RichText::new(subtitle)
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MiningSurveyFilter {
    Surveyed,
    SeismicPlus,
    CoreOnly,
}

impl MiningSurveyFilter {
    fn all() -> &'static [Self] {
        &[Self::Surveyed, Self::SeismicPlus, Self::CoreOnly]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Surveyed => "Surveyed",
            Self::SeismicPlus => "Seismic+",
            Self::CoreOnly => "Core Sample",
        }
    }

    /// Match against a body's mean coverage in `[0.0, 1.0]`. The
    /// thresholds mirror the legacy `SurveyLevel` enum values:
    /// Unsurveyed=0.0, OrbitalScan=0.2, SeismicSurvey=0.4,
    /// CoreSample=1.0. PR-F replaces the enum with `mean_coverage`
    /// from `SurveyState::average_tier()`.
    fn matches(self, mean_coverage: f32) -> bool {
        match self {
            Self::Surveyed => mean_coverage > 0.0,
            Self::SeismicPlus => mean_coverage >= 0.4,
            Self::CoreOnly => mean_coverage >= 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MiningActivityFilter {
    AllSites,
    ActiveMining,
    ColonySites,
    Untapped,
}

impl MiningActivityFilter {
    fn all() -> &'static [Self] {
        &[
            Self::AllSites,
            Self::ActiveMining,
            Self::ColonySites,
            Self::Untapped,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::AllSites => "All Sites",
            Self::ActiveMining => "Active Mining",
            Self::ColonySites => "Colony Sites",
            Self::Untapped => "Untapped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MiningSortMode {
    Orbit,
    Name,
    ReserveEstimate,
    DepositCount,
    Accessibility,
    MeanCoverage,
}

impl MiningSortMode {
    fn all() -> &'static [Self] {
        &[
            Self::Orbit,
            Self::Name,
            Self::ReserveEstimate,
            Self::DepositCount,
            Self::Accessibility,
            Self::MeanCoverage,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Orbit => "Orbit",
            Self::Name => "Name",
            Self::ReserveEstimate => "Reserve Estimate",
            Self::DepositCount => "Deposit Count",
            Self::Accessibility => "Accessibility",
            Self::MeanCoverage => "Mean Coverage",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MiningTabUiState {
    search_text: String,
    resource_filter: Option<ResourceType>,
    survey_filter: MiningSurveyFilter,
    activity_filter: MiningActivityFilter,
    sort_mode: MiningSortMode,
    sort_descending: bool,
    selected_body: Option<Entity>,
}

impl Default for MiningTabUiState {
    fn default() -> Self {
        Self {
            search_text: String::new(),
            resource_filter: None,
            survey_filter: MiningSurveyFilter::Surveyed,
            activity_filter: MiningActivityFilter::AllSites,
            sort_mode: MiningSortMode::Orbit,
            sort_descending: false,
            selected_body: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MiningDepositRow {
    resource_type: ResourceType,
    deposit: MineralDeposit,
    estimated_mt: f64,
    active_rate_mt_per_year: Option<f64>,
}

fn cmp_f64(left: f64, right: f64) -> std::cmp::Ordering {
    left.partial_cmp(&right)
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Map a body's `SurveyState` (preferred) or legacy `SurveyLevel`
/// (fallback) to a 0..=1 mean coverage value. Used by the economy
/// panel's filter and sort during the v0.5.0 migration window —
/// the v0.5.0 source of truth is `SurveyState::average_tier()`,
/// which already normalizes across all 8 dimensions.
fn legacy_to_mean_coverage(
    survey_level: Option<SurveyLevel>,
    survey_state: Option<&crate::survey::SurveyState>,
) -> f32 {
    if let Some(state) = survey_state {
        return state.average_tier();
    }
    match survey_level.unwrap_or(SurveyLevel::Unsurveyed) {
        SurveyLevel::Unsurveyed => 0.0,
        SurveyLevel::OrbitalScan => 0.2,
        SurveyLevel::SeismicSurvey => 0.4,
        SurveyLevel::CoreSample => 1.0,
    }
}

/// PR-F (GRA-84): look up the MineralDeposits axis fidelity for a
/// body, preferring the v0.5.0 `SurveyState` and falling back to
/// the legacy `SurveyLevel` adapter. Mirrors the dossier and
/// dashboard path so the mining tab, the dossier rows, and the
/// dashboard's resource totals all use the same effective
/// fidelity.
fn mineral_deposit_fidelity(
    survey_level: Option<SurveyLevel>,
    survey_state: Option<&crate::survey::SurveyState>,
) -> crate::survey::DimensionFidelity {
    if let Some(state) = survey_state {
        return state.fidelity(crate::survey::SurveyDimension::MineralDeposits);
    }
    survey_level
        .unwrap_or(SurveyLevel::Unsurveyed)
        .as_deposit_fidelity(0.0)
}

fn mining_body_icon(body_type: BodyType) -> &'static str {
    match body_type {
        BodyType::Planet | BodyType::GasGiant => "🪐",
        BodyType::Moon => "🌙",
        BodyType::Asteroid => "🪨",
        BodyType::DwarfPlanet => "⚫",
        BodyType::Comet => "☄",
        BodyType::Star => "★",
        BodyType::Ring => "◌",
    }
}

/// Legacy rank helper kept during the v0.5.0 migration window for
/// any callers that haven't migrated to `mean_coverage` yet.
/// Suppress the dead_code warning while the migration is in
/// progress.
#[allow(dead_code)]
fn mining_survey_rank(level: SurveyLevel) -> u8 {
    match level {
        SurveyLevel::Unsurveyed => 0,
        SurveyLevel::OrbitalScan => 1,
        SurveyLevel::SeismicSurvey => 2,
        SurveyLevel::CoreSample => 3,
    }
}

fn mining_survey_label(level: SurveyLevel) -> &'static str {
    match level {
        SurveyLevel::Unsurveyed => "Unsurveyed",
        SurveyLevel::OrbitalScan => "Orbital Scan",
        SurveyLevel::SeismicSurvey => "Seismic Survey",
        SurveyLevel::CoreSample => "Core Sample",
    }
}

fn mining_survey_color(level: SurveyLevel) -> egui::Color32 {
    match level {
        SurveyLevel::Unsurveyed => theme::TEXT_DIM,
        SurveyLevel::OrbitalScan => theme::ACCENT,
        SurveyLevel::SeismicSurvey => theme::EP_TEAL,
        SurveyLevel::CoreSample => theme::GREEN,
    }
}

/// v0.5.0 mean-coverage bucket label. Rounds to the nearest
/// 25% band — `0%`, `25%`, `50%`, `75%`, `100%` — so the mining
/// panel row meta is comparable to the dossier's coverage readout.
fn coverage_label(mean_coverage: f32) -> String {
    let pct = (mean_coverage.clamp(0.0, 1.0) * 100.0).round() as u32;
    format!("{pct}% surveyed")
}

fn mining_matching_active_ops_count(
    body_entry: &BodyEconomyEntry,
    resource_filter: Option<ResourceType>,
) -> usize {
    body_entry
        .mining_ops
        .iter()
        .filter(|op| {
            op.active && resource_filter.is_none_or(|resource| op.resource_type == resource)
        })
        .count()
}

fn mining_visible_deposit_rows(
    body_entry: &BodyEconomyEntry,
    resource_filter: Option<ResourceType>,
) -> Vec<MiningDepositRow> {
    // PR-F (GRA-84): use the body's stored
    // `SurveyState::MineralDeposits` fidelity (preferred) or the
    // legacy `SurveyLevel` adapter as fallback. This matches the
    // dossier and dashboard path so a body with a v0.5.0
    // `SurveyState` (or a legacy `SurveyLevel` whose new tier is
    // class-only) still shows its deposit rows in the mining tab.
    let fidelity = body_entry.mineral_fidelity;
    let mut rows: Vec<_> = body_entry
        .deposits
        .iter()
        .filter(|(resource_type, _)| {
            resource_filter.is_none_or(|resource| *resource_type == resource)
        })
        .filter_map(|(resource_type, deposit)| {
            let estimate = crate::survey::estimate_with_fidelity(deposit, fidelity);
            if !estimate.is_quantified() {
                return None;
            }
            let estimated_mt = estimate.mid_or_zero();

            let active_rate_mt_per_year = body_entry
                .mining_ops
                .iter()
                .find(|op| op.active && op.resource_type == *resource_type)
                .map(|op| op.rate_mt_per_year);

            Some(MiningDepositRow {
                resource_type: *resource_type,
                deposit: *deposit,
                estimated_mt,
                active_rate_mt_per_year,
            })
        })
        .collect();

    rows.sort_by(|left, right| {
        right
            .active_rate_mt_per_year
            .is_some()
            .cmp(&left.active_rate_mt_per_year.is_some())
            .then_with(|| cmp_f64(right.estimated_mt, left.estimated_mt))
            .then_with(|| {
                left.resource_type
                    .display_name()
                    .cmp(right.resource_type.display_name())
            })
    });
    rows
}

fn mining_body_estimated_total(
    body_entry: &BodyEconomyEntry,
    resource_filter: Option<ResourceType>,
) -> f64 {
    mining_visible_deposit_rows(body_entry, resource_filter)
        .iter()
        .map(|row| row.estimated_mt)
        .sum()
}

fn mining_body_average_accessibility(
    body_entry: &BodyEconomyEntry,
    resource_filter: Option<ResourceType>,
) -> f32 {
    let deposits = mining_visible_deposit_rows(body_entry, resource_filter);
    if deposits.is_empty() {
        return 0.0;
    }

    (deposits
        .iter()
        .map(|row| row.deposit.accessibility as f64)
        .sum::<f64>()
        / deposits.len() as f64) as f32
}

fn mining_body_meta(
    body_entry: &BodyEconomyEntry,
    resource_filter: Option<ResourceType>,
) -> String {
    let deposit_count = mining_visible_deposit_rows(body_entry, resource_filter).len();
    let estimate = mining_body_estimated_total(body_entry, resource_filter);
    let ops = mining_matching_active_ops_count(body_entry, resource_filter);

    let mut parts = vec![format!("{} dep", deposit_count)];
    if estimate > 0.0 {
        parts.push(format!("{} est", format_mass(estimate)));
    }
    if ops > 0 {
        parts.push(format!("{} op", ops));
    }
    parts.push(coverage_label(body_entry.mean_coverage).to_string());
    parts.join(" • ")
}

fn mining_compare_body_entries(
    left: &BodyEconomyEntry,
    right: &BodyEconomyEntry,
    state: &MiningTabUiState,
) -> std::cmp::Ordering {
    let ordering = match state.sort_mode {
        MiningSortMode::Orbit => cmp_f64(
            left.semi_major_axis_au.unwrap_or(0.0),
            right.semi_major_axis_au.unwrap_or(0.0),
        )
        .then_with(|| left.body_name.cmp(&right.body_name)),
        MiningSortMode::Name => left.body_name.cmp(&right.body_name),
        MiningSortMode::ReserveEstimate => cmp_f64(
            mining_body_estimated_total(left, state.resource_filter),
            mining_body_estimated_total(right, state.resource_filter),
        )
        .then_with(|| left.body_name.cmp(&right.body_name)),
        MiningSortMode::DepositCount => mining_visible_deposit_rows(left, state.resource_filter)
            .len()
            .cmp(&mining_visible_deposit_rows(right, state.resource_filter).len())
            .then_with(|| left.body_name.cmp(&right.body_name)),
        MiningSortMode::Accessibility => cmp_f64(
            mining_body_average_accessibility(left, state.resource_filter) as f64,
            mining_body_average_accessibility(right, state.resource_filter) as f64,
        )
        .then_with(|| left.body_name.cmp(&right.body_name)),
        MiningSortMode::MeanCoverage => {
            cmp_f64(left.mean_coverage as f64, right.mean_coverage as f64)
                .then_with(|| left.body_name.cmp(&right.body_name))
        }
    };

    if state.sort_descending {
        ordering.reverse()
    } else {
        ordering
    }
}

fn render_mining_group_header(
    ui: &mut egui::Ui,
    label: &str,
    count: usize,
    is_open: bool,
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
        theme::TEXT_DIM,
    );

    response
}

fn render_mining_body_row(
    ui: &mut egui::Ui,
    body_entry: &BodyEconomyEntry,
    selected: bool,
    meta: &str,
) -> egui::Response {
    let desired_size = egui::vec2(ui.available_width(), 24.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    let fill = if response.hovered() {
        theme::SURFACE_RAISED
    } else if selected {
        theme::SURFACE
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke = if response.hovered() {
        egui::Stroke::new(1.0_f32, theme::ACCENT)
    } else if selected {
        egui::Stroke::new(1.0_f32, theme::ACCENT_DIM)
    } else {
        egui::Stroke::NONE
    };
    let text_color = if response.hovered() || selected {
        theme::ACCENT
    } else {
        theme::TEXT
    };

    let row_rect = rect.shrink2(egui::vec2(0.0, 1.0));
    ui.painter()
        .rect(row_rect, 3.0, fill, stroke, egui::StrokeKind::Outside);
    ui.painter().text(
        row_rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!(
            "{} {}",
            mining_body_icon(body_entry.body_type),
            body_entry.body_name
        ),
        theme::body(13.0),
        text_color,
    );
    ui.painter().text(
        row_rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        meta,
        theme::mono(10.0),
        if selected {
            theme::TEXT_VALUE
        } else {
            theme::TEXT_DIM
        },
    );

    response
}

/// Source classification for economic entries in the hierarchical view.
/// Prepared for future expansion with stations and mining ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EconomySourceKind {
    Colony,
    MiningOp,
    // Future: Station, MiningShip
}

/// Snapshot of a body's economic contribution, aggregated per-frame.
#[allow(dead_code)]
#[derive(Clone)]
pub(super) struct BodyEconomyEntry {
    entity: Entity,
    #[allow(dead_code)]
    pub(super) body_name: String,
    /// Prepared for future use (stations, mining ships)
    #[allow(dead_code)]
    pub(super) body_type: BodyType,
    /// Legacy survey level (kept for the 1:1 migration shim and
    /// for the "is the body surveyed at all?" predicate).
    survey_level: SurveyLevel,
    /// Mean coverage across all dimensions, in `[0.0, 1.0]`. The
    /// v0.5.0 source of truth for the new tier-based filter and
    /// sort. Derived from the body's `SurveyState` (preferred) or
    /// the legacy `SurveyLevel` (fallback).
    mean_coverage: f32,
    /// PR-F (GRA-84): v0.5.0 `SurveyState` MineralDeposits axis
    /// fidelity (or the legacy `SurveyLevel` adapter as fallback).
    /// Used by the mining tab to filter and rank deposit rows so
    /// it agrees with the dossier and dashboard views.
    mineral_fidelity: crate::survey::DimensionFidelity,
    logical_parent: Option<Entity>,
    semi_major_axis_au: Option<f64>,
    source_kind: EconomySourceKind,
    /// Colony data (if colonised)
    pub(super) colony: Option<ColonySnapshot>,
    /// Standalone mining operations on this body
    mining_ops: Vec<MiningOpSnapshot>,
    /// Resource deposits on this body
    deposits: Vec<(ResourceType, MineralDeposit)>,
    /// Power generators on this body
    pub(super) generators: Vec<PowerGenSnapshot>,
}

/// Lightweight copy of colony data for the economy UI.
#[derive(Clone)]
pub(super) struct ColonySnapshot {
    pub(super) name: String,
    pub(super) population: f64,
    pub(super) growth_per_year: f64,
    pub(super) housing_capacity: f64,
    pub(super) total_buildings: u32,
    pub(super) workforce_efficiency: f64,
    pub(super) logistics_efficiency: f64,
    pub(super) income_per_year: f64,
    pub(super) operating_cost_per_year: f64,
    pub(super) power_generation_watts: f64,
    pub(super) power_load_watts: f64,
    pub(super) buildings: Vec<(BuildingType, u32)>,
}

#[derive(Clone)]
struct MiningOpSnapshot {
    resource_type: ResourceType,
    rate_mt_per_year: f64,
    active: bool,
}

#[derive(Clone)]
pub(super) struct PowerGenSnapshot {
    pub(super) source_type: PowerSourceType,
    pub(super) output_watts: f64,
}

/// A star system grouping for the hierarchical economy view.
pub(super) struct StarSystemGroup {
    pub(super) system_name: String,
    pub(super) bodies: Vec<BodyEconomyEntry>,
}

#[derive(Clone)]
pub(super) struct BuildingPowerRow {
    pub(super) building_type: BuildingType,
    pub(super) count: u32,
    pub(super) produced_watts: f64,
    pub(super) consumed_watts: f64,
}

#[derive(Clone)]
pub(super) struct PowerBodyRow {
    pub(super) system_name: String,
    pub(super) body_name: String,
    pub(super) body_type: BodyType,
    pub(super) total_generation_watts: f64,
    pub(super) net_power_watts: f64,
    pub(super) colony: Option<ColonySnapshot>,
    pub(super) generators: Vec<PowerGenSnapshot>,
}

pub(super) fn calculate_building_power_profile(
    building_type: BuildingType,
    count: u32,
    buildings_data: Option<&BuildingsData>,
) -> (f64, f64) {
    let Some(data) = buildings_data else {
        return (0.0, count as f64 * 400_000_000.0);
    };

    let Some(def) = data.get(&building_type) else {
        return (0.0, count as f64 * 400_000_000.0);
    };

    // RON `PowerGeneration` values are in **GW per unit** (see the inline
    // `// <value> GW` annotations next to every entry in
    // assets/data/buildings.ron and the per-building display strings in
    // src/colony/types.rs: "+5 GW power output", "+20 GW power output",
    // etc.). The 1e9 factor converts GW → W, matching the canonical
    // `calculate_colony_power_totals` in src/economy/budget.rs and the
    // EnergyGrid units (W).
    let produced_watts = def
        .modifiers
        .iter()
        .filter(|modifier| modifier.modifier_type == "PowerGeneration")
        .map(|modifier| modifier.value * count as f64 * 1_000_000_000.0)
        .sum();
    let consumed_watts = def.power_demand_mw * count as f64 * 1_000_000.0;

    (produced_watts, consumed_watts)
}

fn body_has_resource_flows(
    body_entry: &BodyEconomyEntry,
    buildings_data: Option<&BuildingsData>,
) -> bool {
    if body_entry
        .mining_ops
        .iter()
        .any(|op| op.active && op.rate_mt_per_year > 0.0)
    {
        return true;
    }

    let Some(colony) = &body_entry.colony else {
        return false;
    };

    if colony.population > 0.0 {
        return true;
    }

    if colony.buildings.iter().any(|(building_type, count)| {
        *count > 0
            && matches!(
                building_type,
                crate::colony::BuildingType::Farm | crate::colony::BuildingType::AgriDome
            )
    }) {
        return true;
    }

    let Some(data) = buildings_data else {
        return false;
    };

    colony.buildings.iter().any(|(building_type, count)| {
        if *count == 0 {
            return false;
        }

        if !data.maintenance_resources(building_type).is_empty() {
            return true;
        }

        data.get(building_type)
            .map(|def| {
                def.modifiers.iter().any(|modifier| {
                    matches!(
                        modifier.modifier_type.as_str(),
                        "MiningEfficiency"
                            | "DeepMiningEfficiency"
                            | "BulkMiningEfficiency"
                            | "AtmosphericHarvesting"
                    ) && modifier.value > 0.0
                })
            })
            .unwrap_or(false)
    })
}

pub(super) fn power_body_icon(body_type: BodyType) -> &'static str {
    match body_type {
        BodyType::Planet | BodyType::GasGiant => "🪐",
        BodyType::Moon => "🌙",
        BodyType::Asteroid => "🪨",
        BodyType::DwarfPlanet => "⚫",
        BodyType::Comet => "☄",
        _ => "🔵",
    }
}

pub(super) fn build_colony_power_rows(
    colony: &ColonySnapshot,
    buildings_data: Option<&BuildingsData>,
) -> Vec<BuildingPowerRow> {
    let mut building_rows: Vec<BuildingPowerRow> = colony
        .buildings
        .iter()
        .filter_map(|(building_type, count)| {
            let (produced_watts, consumed_watts) =
                calculate_building_power_profile(*building_type, *count, buildings_data);
            if produced_watts <= 0.0 && consumed_watts <= 0.0 {
                None
            } else {
                Some(BuildingPowerRow {
                    building_type: *building_type,
                    count: *count,
                    produced_watts,
                    consumed_watts,
                })
            }
        })
        .collect();

    building_rows.sort_by(|left, right| {
        right
            .produced_watts
            .partial_cmp(&left.produced_watts)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .consumed_watts
                    .partial_cmp(&left.consumed_watts)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| right.count.cmp(&left.count))
    });

    building_rows
}

pub(super) fn collect_power_body_rows(hierarchy: &[StarSystemGroup]) -> Vec<PowerBodyRow> {
    let mut rows: Vec<PowerBodyRow> = hierarchy
        .iter()
        .flat_map(|group| {
            group.bodies.iter().filter_map(|body_entry| {
                let generator_output_watts: f64 = body_entry
                    .generators
                    .iter()
                    .map(|gen| gen.output_watts)
                    .sum();
                let colony_generation_watts = body_entry
                    .colony
                    .as_ref()
                    .map(|colony| colony.power_generation_watts)
                    .unwrap_or(0.0);
                let colony_load_watts = body_entry
                    .colony
                    .as_ref()
                    .map(|colony| colony.power_load_watts)
                    .unwrap_or(0.0);
                let total_generation_watts = generator_output_watts + colony_generation_watts;

                if total_generation_watts <= 0.0 {
                    return None;
                }

                Some(PowerBodyRow {
                    system_name: group.system_name.clone(),
                    body_name: body_entry.body_name.clone(),
                    body_type: body_entry.body_type,
                    total_generation_watts,
                    net_power_watts: total_generation_watts - colony_load_watts,
                    colony: body_entry.colony.clone(),
                    generators: body_entry.generators.clone(),
                })
            })
        })
        .collect();

    rows.sort_by(|left, right| {
        right
            .total_generation_watts
            .partial_cmp(&left.total_generation_watts)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .net_power_watts
                    .partial_cmp(&left.net_power_watts)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.body_name.cmp(&right.body_name))
    });

    rows
}

pub(super) fn render_power_body_detail_tooltip(
    ui: &mut egui::Ui,
    body: &PowerBodyRow,
    buildings_data: Option<&BuildingsData>,
) {
    ui.set_min_width(360.0);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} {}",
                power_body_icon(body.body_type),
                body.body_name
            ))
            .strong()
            .size(13.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format_power(body.total_generation_watts))
                    .color(theme::GREEN)
                    .strong(),
            );
        });
    });
    ui.label(
        egui::RichText::new(&body.system_name)
            .size(11.0)
            .color(theme::TEXT_DIM),
    );
    ui.separator();

    if let Some(colony) = &body.colony {
        let utilization = if colony.power_generation_watts > 0.0 {
            colony.power_load_watts / colony.power_generation_watts
        } else {
            0.0
        };
        let net_color = if body.net_power_watts >= 0.0 {
            theme::GREEN
        } else {
            theme::RED
        };
        let util_color = if colony.power_generation_watts <= 0.0 {
            theme::TEXT_DIM
        } else if utilization < 0.8 {
            theme::GREEN
        } else if utilization < 1.0 {
            theme::AMBER
        } else {
            theme::RED
        };

        let building_rows = build_colony_power_rows(colony, buildings_data);
        if !building_rows.is_empty() {
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    egui::Grid::new(format!("power_tooltip_buildings_{}", body.body_name))
                        .num_columns(5)
                        .spacing([16.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Building Type").strong());
                            ui.label(egui::RichText::new("Count").strong());
                            ui.label(egui::RichText::new("Generation").strong());
                            ui.label(egui::RichText::new("Load").strong());
                            ui.label(egui::RichText::new("Net").strong());
                            ui.end_row();

                            for row in building_rows {
                                let building_net = row.produced_watts - row.consumed_watts;
                                let building_net_color = if building_net >= 0.0 {
                                    theme::GREEN
                                } else {
                                    theme::RED
                                };

                                ui.label(row.building_type.display_name());
                                ui.label(row.count.to_string());
                                ui.label(
                                    egui::RichText::new(format_power(row.produced_watts))
                                        .color(theme::GREEN)
                                        .monospace(),
                                );
                                ui.label(
                                    egui::RichText::new(format_power(row.consumed_watts))
                                        .color(theme::AMBER)
                                        .monospace(),
                                );
                                ui.label(
                                    egui::RichText::new(format_power(building_net))
                                        .color(building_net_color)
                                        .monospace(),
                                );
                                ui.end_row();
                            }
                        });
                });

            ui.separator();
        }

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "Production {}",
                    format_power(colony.power_generation_watts)
                ))
                .color(theme::GREEN),
            );
            ui.label(
                egui::RichText::new(format!("Load {}", format_power(colony.power_load_watts)))
                    .color(theme::AMBER),
            );
            ui.label(
                egui::RichText::new(format!("Net {}", format_power(body.net_power_watts)))
                    .color(net_color),
            );
            ui.label(
                egui::RichText::new(if colony.power_generation_watts > 0.0 {
                    format!("Utilization {:.1}%", utilization * 100.0)
                } else {
                    "Utilization n/a".to_string()
                })
                .color(util_color),
            );
        });
    }

    if !body.generators.is_empty() {
        if body.colony.is_some() {
            ui.separator();
        }

        ui.label(egui::RichText::new("Standalone Generators").strong());
        let mut generators = body.generators.clone();
        generators.sort_by(|left, right| {
            right
                .output_watts
                .partial_cmp(&left.output_watts)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        egui::Grid::new(format!("power_tooltip_generators_{}", body.body_name))
            .num_columns(2)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for generator in generators {
                    ui.label(generator.source_type.to_string());
                    ui.label(
                        egui::RichText::new(format_power(generator.output_watts))
                            .color(theme::GREEN)
                            .monospace(),
                    );
                    ui.end_row();
                }
            });
    }
}

/// Build the hierarchical economy data: star systems → bodies → colonies/mining/power.
pub(super) fn build_economy_hierarchy(
    body_query: &Query<(
        Entity,
        &CelestialBody,
        Option<&SystemId>,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&Colony>,
        Option<&PlanetResources>,
        Option<&SurveyLevel>,
        Option<&crate::survey::SurveyState>,
        Option<&crate::economy::components::PowerGenerator>,
        Option<&MiningOperation>,
    )>,
    star_query: &Query<(&CelestialBody, &SystemId), With<crate::plugins::solar_system::Star>>,
    buildings_data: Option<&BuildingsData>,
) -> Vec<StarSystemGroup> {
    use std::collections::BTreeMap;

    // Map system_id → star name
    let mut system_names: BTreeMap<usize, String> = BTreeMap::new();
    for (body, sys_id) in star_query.iter() {
        system_names
            .entry(sys_id.0)
            .or_insert_with(|| body.name.clone());
    }

    // Group bodies by star system
    let mut system_bodies: BTreeMap<usize, Vec<BodyEconomyEntry>> = BTreeMap::new();

    for (
        entity,
        body,
        sys_id_opt,
        logical_parent,
        orbit,
        colony_opt,
        resources_opt,
        survey_level,
        survey_state,
        gen_opt,
        mining_opt,
    ) in body_query.iter()
    {
        let sys_id = sys_id_opt.map(|s| s.0).unwrap_or(0);

        // Skip stars themselves in the body list
        if body.body_type == BodyType::Star {
            // Ensure system exists even if star has no economic children
            system_names
                .entry(sys_id)
                .or_insert_with(|| body.name.clone());
            // But still record power generators on stars
            if let Some(gen) = gen_opt {
                system_bodies
                    .entry(sys_id)
                    .or_default()
                    .push(BodyEconomyEntry {
                        entity,
                        body_name: body.name.clone(),
                        body_type: body.body_type,
                        survey_level: SurveyLevel::Unsurveyed,
                        mean_coverage: legacy_to_mean_coverage(survey_level.copied(), survey_state),
                        mineral_fidelity: mineral_deposit_fidelity(
                            survey_level.copied(),
                            survey_state,
                        ),
                        logical_parent: None,
                        semi_major_axis_au: None,
                        source_kind: EconomySourceKind::MiningOp,
                        colony: None,
                        mining_ops: Vec::new(),
                        deposits: Vec::new(),
                        generators: vec![PowerGenSnapshot {
                            source_type: gen.source_type,
                            output_watts: gen.output,
                        }],
                    });
            }
            continue;
        }

        // Only include bodies with economic activity
        let has_colony = colony_opt.is_some();
        let has_mining = mining_opt.is_some();
        let has_deposits = resources_opt
            .map(|r| !r.deposits.is_empty())
            .unwrap_or(false);
        let has_power = gen_opt.is_some();

        if !has_colony && !has_mining && !has_deposits && !has_power {
            continue;
        }

        let colony_snap = colony_opt.map(|c| {
            let power_totals = crate::economy::calculate_colony_power_totals(c, buildings_data);
            // v3.6: Colony methods need &BuildingsData. If the data
            // hasn't loaded yet (early frame), use a default. The
            // power totals are unaffected (they read the same data).
            let default_data = BuildingsData::default();
            let data = buildings_data.unwrap_or(&default_data);
            ColonySnapshot {
                name: c.name.clone(),
                population: c.population,
                growth_per_year: c.population_growth_per_year(1.0, data),
                housing_capacity: c.housing_capacity(data),
                total_buildings: c.total_buildings(),
                workforce_efficiency: c.workforce_efficiency(data),
                logistics_efficiency: c.logistics_efficiency(data),
                income_per_year: c.wealth_generation_per_year(data),
                operating_cost_per_year: c.operating_cost_per_year(data),
                power_generation_watts: power_totals.produced_watts,
                power_load_watts: power_totals.consumed_watts,
                buildings: c
                    .buildings
                    .iter()
                    .filter(|(_, &n)| n > 0)
                    .map(|(b, &n)| (*b, n))
                    .collect(),
            }
        });

        let mut mining_ops = Vec::new();
        if let Some(op) = mining_opt {
            mining_ops.push(MiningOpSnapshot {
                resource_type: op.resource_type,
                rate_mt_per_year: op.base_rate_mt_per_year,
                active: op.active,
            });
        }

        let deposits: Vec<(ResourceType, MineralDeposit)> = resources_opt
            .map(|r| r.deposits.iter().map(|(rt, d)| (*rt, *d)).collect())
            .unwrap_or_default();

        let mut generators = Vec::new();
        if let Some(gen) = gen_opt {
            generators.push(PowerGenSnapshot {
                source_type: gen.source_type,
                output_watts: gen.output,
            });
        }

        let source_kind = if has_colony {
            EconomySourceKind::Colony
        } else {
            EconomySourceKind::MiningOp
        };

        system_bodies
            .entry(sys_id)
            .or_default()
            .push(BodyEconomyEntry {
                entity,
                body_name: body.name.clone(),
                body_type: body.body_type,
                survey_level: survey_level.copied().unwrap_or(SurveyLevel::Unsurveyed),
                mean_coverage: legacy_to_mean_coverage(survey_level.copied(), survey_state),
                mineral_fidelity: mineral_deposit_fidelity(survey_level.copied(), survey_state),
                logical_parent: logical_parent.map(|parent| parent.0),
                semi_major_axis_au: orbit.map(|orbit| orbit.semi_major_axis),
                source_kind,
                colony: colony_snap,
                mining_ops,
                deposits,
                generators,
            });
    }

    // Build final groups
    let mut groups: Vec<StarSystemGroup> = Vec::new();
    for (sys_id, bodies) in system_bodies {
        let system_name = system_names
            .get(&sys_id)
            .cloned()
            .unwrap_or_else(|| format!("System #{}", sys_id));
        groups.push(StarSystemGroup {
            system_name: format!("{} System", system_name),
            bodies,
        });
    }

    groups
}

/// Format a rate value with sign and color helper.
fn rate_text(rate: f64, suffix: &str) -> (String, egui::Color32) {
    if rate.abs() < 1e-9 {
        return (format!("0{}", suffix), theme::TEXT_DIM);
    }
    // v3.8.6 (2026-08-07): fix missing minus sign on negative
    // rates.  Previous version returned "" for the else branch,
    // so the rate display read "8.4 Mt/yr" for both +8.4 and -8.4
    // (only the colour differed).  Now negative rates show "-8.4".
    let sign = if rate > 0.0 { "+" } else { "−" };
    let text = format!("{}{}{}", sign, format_mass(rate.abs()), suffix);
    let color = if rate > 0.0 { theme::GREEN } else { theme::RED };
    (text, color)
}

fn colony_balance(colony: &ColonySnapshot) -> f64 {
    colony.income_per_year - colony.operating_cost_per_year
}

fn group_has_population_or_colonies(group: &StarSystemGroup) -> bool {
    group.bodies.iter().any(|body| {
        body.colony
            .as_ref()
            .map(|colony| colony.population > 0.0 || colony.total_buildings > 0)
            .unwrap_or(false)
    })
}

/// System that renders the Economy UI when the Economy menu is active.
///
/// This system provides a hierarchical view of the empire's economy broken down
/// by star system → celestial body → buildings/operations. Includes tabs for
/// overview, resources, colonies, mining, and power grid. The architecture is
/// prepared for future expansion with stations and mining ships.
pub(super) fn ui_economy_panels(
    mut contexts: EguiContexts,
    mut active_menu: ResMut<ActiveMenu>,
    budget: Res<GlobalBudget>,
    contextual: Res<crate::economy::ContextualStockpile>,
    rate_tracker: Res<ResourceRateTracker>,
    mut mining_ui_state: Local<MiningTabUiState>,
    body_query: Query<(
        Entity,
        &CelestialBody,
        Option<&SystemId>,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&Colony>,
        Option<&PlanetResources>,
        Option<&SurveyLevel>,
        Option<&crate::survey::SurveyState>,
        Option<&crate::economy::components::PowerGenerator>,
        Option<&MiningOperation>,
    )>,
    star_query: Query<(&CelestialBody, &SystemId), With<crate::plugins::solar_system::Star>>,
    buildings_data: Option<Res<BuildingsData>>,
    resource_requests: Res<crate::economy::PendingResourceRequests>,
    mut shipping_companies: ResMut<crate::economy::ShippingCompanies>,
    mut shipping_company_filter: ResMut<super::fleets_panel::ShippingCompanyFilter>,
    settings: Res<Settings>,
    forecast: ForecastInputs,
) {
    if active_menu.current != GameMenu::Economy {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let hierarchy = build_economy_hierarchy(&body_query, &star_query, buildings_data.as_deref());

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
            // Tab state (persisted across frames as a `u8` byte so
            // the on-disk format is stable across the PR-F
            // Tab-trait migration; `from_byte` / `to_byte` replace
            // the previous `From<u8>` / `From<EconomyTab>` shims).
            let tab_id = ui.id().with("economy_tab");
            let mut current_tab: EconomyTab = EconomyTab::from_byte(
                ui.data_mut(|data| data.get_persisted(tab_id).unwrap_or(0u8)),
            );

            draw_menu_header(
                ui,
                "ECONOMY",
                "Treasury, industrial throughput, and system-level performance.",
            );

            let balance = budget.balance_per_year();
            let sign = if balance >= 0.0 { "+" } else { "" };
            let balance_color = if balance >= 0.0 {
                theme::GREEN
            } else {
                theme::RED
            };
            ui.horizontal_wrapped(|ui| {
                draw_status_chip(
                    ui,
                    "TREASURY",
                    format_currency(budget.treasury),
                    theme::GOLD,
                );
                ui.separator();
                draw_status_chip(
                    ui,
                    "NET FLOW",
                    format!("{}{}/yr", sign, format_currency(balance)),
                    balance_color,
                );
                ui.separator();
                draw_status_chip(
                    ui,
                    "CIV SCORE",
                    format!("{:.0}", budget.civilization_score),
                    theme::ACCENT,
                );
            });

            theme::divider(ui);

            // Tab bar — PR-F (GRA-71) replaces the hand-rolled
            // `for (tab, label) in tabs` loop + `draw_tab_button`
            // with the PR-B `theme::tab_strip` primitive. The
            // `EconomyTab::ALL` const carries the seven variants in
            // display order; icons + labels come from the `Tab`
            // trait impl. Click semantics match the previous
            // bespoke version: the callback updates
            // `current_tab`, the persistence write below keeps the
            // selection across frames.
            ui.horizontal_wrapped(|ui| {
                theme::tab_strip(ui, &EconomyTab::ALL, current_tab, |tab| {
                    current_tab = tab;
                });
            });

            theme::divider(ui);

            // Persist tab
            ui.data_mut(|data| {
                data.insert_persisted(tab_id, current_tab.to_byte());
            });

            match current_tab {
                EconomyTab::Overview => {
                    render_econ_overview(ui, &budget, &rate_tracker, &hierarchy)
                }
                EconomyTab::Resources => render_econ_resources(
                    ui,
                    &contextual,
                    &rate_tracker,
                    &hierarchy,
                    buildings_data.as_deref(),
                ),
                EconomyTab::Colonies => render_econ_colonies(ui, &budget, &hierarchy),
                EconomyTab::Forecast => render_econ_forecast(
                    ui,
                    &contextual,
                    &rate_tracker,
                    &forecast,
                    buildings_data.as_deref(),
                ),
                EconomyTab::Mining => render_econ_mining(ui, &hierarchy, &mut mining_ui_state),
                EconomyTab::PowerGrid => {
                    render_econ_power_grid(ui, &budget, &hierarchy, buildings_data.as_deref())
                }
                EconomyTab::Logistics => {
                    render_econ_logistics(ui, &resource_requests, &mut shipping_companies, &budget)
                }
                EconomyTab::PrivateShipping => {
                    let clicked = render_shipping_overview(
                        ui,
                        &resource_requests,
                        &shipping_companies,
                        &budget,
                        settings.show_freighters_in_transit,
                    );
                    if let Some(company_idx) = clicked {
                        // AC#3 click-through: filter the Fleets panel
                        // and switch the active menu.  Both happen in
                        // the same frame; the Fleets panel reads the
                        // filter on its next tick.
                        shipping_company_filter.0 = Some(company_idx);
                        active_menu.current = GameMenu::Fleets;
                    }
                }
            }
        });
}

// ---- Economy Tab: Overview ----

fn render_econ_overview(
    ui: &mut egui::Ui,
    budget: &GlobalBudget,
    rate_tracker: &ResourceRateTracker,
    hierarchy: &[StarSystemGroup],
) {
    let populated_systems = hierarchy
        .iter()
        .filter(|group| group_has_population_or_colonies(group))
        .count();

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("💰 Treasury & Budget")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            egui::Grid::new("econ_ov_treasury")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Treasury:");
                    ui.label(
                        egui::RichText::new(format_currency(budget.treasury))
                            .strong()
                            .color(theme::GOLD),
                    );
                    ui.end_row();

                    ui.label("Income:");
                    ui.label(
                        egui::RichText::new(format!(
                            "{}/yr",
                            format_currency(budget.income_per_year)
                        ))
                        .color(theme::GREEN),
                    );
                    ui.end_row();

                    ui.label("Expenses:");
                    ui.label(
                        egui::RichText::new(format!(
                            "{}/yr",
                            format_currency(budget.expenses_per_year)
                        ))
                        .color(theme::RED),
                    );
                    ui.end_row();

                    let balance = budget.balance_per_year();
                    let (sign, color) = if balance >= 0.0 {
                        ("+", theme::GREEN)
                    } else {
                        ("", theme::RED)
                    };
                    ui.label("Balance:");
                    ui.label(
                        egui::RichText::new(format!("{}{}/yr", sign, format_currency(balance)))
                            .strong()
                            .color(color),
                    );
                    ui.end_row();
                });
        });

        ui.add_space(theme::Spacing::sm);

        ui.columns(2, |cols| {
            theme::elevated_frame().show(&mut cols[0], |ui| {
                ui.label(
                    egui::RichText::new("⚡ Power Grid")
                        .font(theme::heading())
                        .color(theme::ACCENT),
                );
                ui.separator();

                let grid = &budget.energy_grid;
                let surplus = grid.surplus();
                let utilization = grid.load_factor();

                egui::Grid::new("econ_ov_power")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label("Production:");
                        ui.label(
                            egui::RichText::new(format_power(grid.produced)).color(theme::GREEN),
                        );
                        ui.end_row();
                        ui.label("Consumption:");
                        ui.label(
                            egui::RichText::new(format_power(grid.consumed)).color(theme::AMBER),
                        );
                        ui.end_row();
                        ui.label("Surplus:");
                        let sc = if surplus >= 0.0 {
                            theme::GREEN
                        } else {
                            theme::RED
                        };
                        ui.label(
                            egui::RichText::new(format_power(surplus))
                                .strong()
                                .color(sc),
                        );
                        ui.end_row();
                        ui.label("Load:");
                        let lc = if utilization < 0.8 {
                            theme::GREEN
                        } else if utilization < 1.0 {
                            theme::AMBER
                        } else {
                            theme::RED
                        };
                        ui.label(
                            egui::RichText::new(format!("{:.1}%", utilization * 100.0)).color(lc),
                        );
                        ui.end_row();
                    });
            });

            theme::elevated_frame().show(&mut cols[1], |ui| {
                ui.label(
                    egui::RichText::new("🧭 Civilization")
                        .font(theme::heading())
                        .color(theme::ACCENT),
                );
                ui.separator();

                let total_pop: f64 = hierarchy
                    .iter()
                    .flat_map(|g| g.bodies.iter())
                    .filter_map(|b| b.colony.as_ref())
                    .map(|c| c.population)
                    .sum();
                let total_colonies: usize = hierarchy
                    .iter()
                    .flat_map(|g| g.bodies.iter())
                    .filter(|b| b.colony.is_some())
                    .count();

                egui::Grid::new("econ_ov_civ")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label("Score:");
                        ui.label(
                            egui::RichText::new(format!("{:.0}", budget.civilization_score))
                                .strong()
                                .color(theme::GOLD),
                        );
                        ui.end_row();
                        ui.label("Colonies:");
                        ui.label(egui::RichText::new(format!("{}", total_colonies)).strong());
                        ui.end_row();
                        ui.label("Population:");
                        ui.label(
                            egui::RichText::new(Colony::format_population(total_pop)).strong(),
                        );
                        ui.end_row();
                        ui.label("Systems:");
                        ui.label(egui::RichText::new(format!("{}", populated_systems)).strong());
                        ui.end_row();
                    });
            });
        });

        ui.add_space(theme::Spacing::sm);

        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🔻 Critical Resources")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            let mut has_critical = false;
            for resource in ResourceType::all() {
                let stockpile = budget.get_stockpile(resource);
                let rate = rate_tracker.get_resource_rate(resource);
                let is_critical_rate = rate < -0.01;
                let is_low_stock = stockpile < 100.0 && resource.is_critical();

                if is_critical_rate || is_low_stock {
                    has_critical = true;
                    ui.horizontal(|ui| {
                        let icon = if is_critical_rate { "⚠" } else { "🔻" };
                        ui.label(icon);
                        ui.label(egui::RichText::new(resource.display_name()).strong());
                        ui.label(format!("Stock: {}", format_mass(stockpile)));
                        let (txt, col) = rate_text(rate, "/mo");
                        ui.label(egui::RichText::new(txt).color(col));
                    });
                }
            }

            if !has_critical {
                ui.label(
                    egui::RichText::new("All resources at healthy levels")
                        .italics()
                        .color(theme::GREEN),
                );
            }
        });

        ui.add_space(theme::Spacing::sm);

        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🛰 Per-System Summary")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            if hierarchy.is_empty() {
                ui.label(
                    egui::RichText::new("No economic activity")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                for group in hierarchy {
                    if !group_has_population_or_colonies(group) {
                        continue;
                    }

                    let sys_colonies: usize =
                        group.bodies.iter().filter(|b| b.colony.is_some()).count();
                    let sys_pop: f64 = group
                        .bodies
                        .iter()
                        .filter_map(|b| b.colony.as_ref())
                        .map(|c| c.population)
                        .sum();
                    let sys_income: f64 = group
                        .bodies
                        .iter()
                        .filter_map(|b| b.colony.as_ref())
                        .map(|c| c.income_per_year)
                        .sum();
                    let sys_cost: f64 = group
                        .bodies
                        .iter()
                        .filter_map(|b| b.colony.as_ref())
                        .map(|c| c.operating_cost_per_year)
                        .sum();
                    let sys_net = sys_income - sys_cost;
                    let net_color = if sys_net >= 0.0 {
                        theme::GREEN
                    } else {
                        theme::RED
                    };
                    let sign = if sys_net >= 0.0 { "+" } else { "" };

                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!(
                            "{} - {} colonies, Pop: {}, Net: {}{}/yr",
                            group.system_name,
                            sys_colonies,
                            Colony::format_population(sys_pop),
                            sign,
                            format_currency(sys_net)
                        ))
                        .strong()
                        .color(net_color),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        let mut body_count = 0usize;

                        for body_entry in &group.bodies {
                            let Some(colony) = body_entry.colony.as_ref() else {
                                continue;
                            };

                            let body_net = colony_balance(colony);
                            if body_net.abs() < 1e-9 {
                                continue;
                            }

                            body_count += 1;
                            let body_color = if body_net >= 0.0 {
                                theme::GREEN
                            } else {
                                theme::RED
                            };
                            let body_sign = if body_net >= 0.0 { "+" } else { "" };
                            let body_icon = match body_entry.body_type {
                                BodyType::Planet | BodyType::GasGiant => "🪐",
                                BodyType::Moon => "🌙",
                                BodyType::Asteroid => "🪨",
                                BodyType::DwarfPlanet => "⚫",
                                BodyType::Comet => "☄",
                                _ => "🔵",
                            };

                            ui.horizontal(|ui| {
                                ui.label(body_icon);
                                ui.label(egui::RichText::new(&body_entry.body_name).strong());
                                if colony.name != body_entry.body_name {
                                    ui.label(
                                        egui::RichText::new(format!("({})", colony.name))
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Net: {}{}/yr",
                                        body_sign,
                                        format_currency(body_net)
                                    ))
                                    .color(body_color),
                                );
                            });
                        }

                        if body_count == 0 {
                            ui.label(
                                egui::RichText::new("No body-level non-zero balances")
                                    .italics()
                                    .color(theme::TEXT_DIM),
                            );
                        }
                    });
                }

                if populated_systems == 0 {
                    ui.label(
                        egui::RichText::new("No populated or colonized systems")
                            .italics()
                            .color(theme::TEXT_DIM),
                    );
                }
            }
        });
    });
}

// ---- Economy Tab: Resources ----

/// Render resource stockpiles and net rates with per-system breakdown.
fn render_econ_resources(
    ui: &mut egui::Ui,
    contextual: &crate::economy::ContextualStockpile,
    rate_tracker: &ResourceRateTracker,
    hierarchy: &[StarSystemGroup],
    buildings_data: Option<&BuildingsData>,
) {
    draw_tab_h1(
        ui,
        "RESOURCE FLOWS",
        &format!(
            "Showing resources for: {}  •  Rates are net monthly.",
            contextual.context_label
        ),
    );
    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Contextual resource stockpiles by category
        let categories = ResourceType::by_category();
        for (category_name, resources) in &categories {
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("📦 {}", category_name))
                    .strong()
                    .size(14.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(format!("econ_res_{}", category_name))
                    .num_columns(4)
                    .spacing([15.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Resource").strong());
                        ui.label(egui::RichText::new("Symbol").strong());
                        ui.label(egui::RichText::new("Stockpile").strong());
                        ui.label(egui::RichText::new("Net Rate (/mo)").strong());
                        ui.end_row();

                        for resource in resources {
                            let stockpile = contextual.get(resource);
                            let rate = rate_tracker.get_resource_rate(resource);

                            ui.label(resource.display_name());
                            ui.label(
                                egui::RichText::new(resource.symbol())
                                    .monospace()
                                    .color(theme::TEXT_DIM),
                            );

                            let stock_color = if stockpile <= 0.0 {
                                theme::RED
                            } else if stockpile < 100.0 && resource.is_critical() {
                                theme::AMBER
                            } else {
                                theme::TEXT
                            };
                            ui.label(
                                egui::RichText::new(format_mass(stockpile))
                                    .monospace()
                                    .color(stock_color),
                            );

                            let (txt, col) = rate_text(rate, "/mo");
                            ui.label(egui::RichText::new(txt).monospace().color(col));
                            ui.end_row();
                        }
                    });
            });
        }

        ui.add_space(theme::Spacing::sm);

        // Research & Engineering rates
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🔬 Research & Engineering Output")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();
            egui::Grid::new("econ_res_rp_ep")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Research Points:");
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.1} RP/mo",
                            rate_tracker.research_rate_per_month
                        ))
                        .color(theme::RP_BLUE),
                    );
                    ui.end_row();
                    ui.label("Engineering Points:");
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.1} EP/mo",
                            rate_tracker.engineering_rate_per_month
                        ))
                        .color(theme::EP_TEAL),
                    );
                    ui.end_row();
                });
        });

        ui.add_space(theme::Spacing::sm);

        // Per-system resource production breakdown
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🌟 Production & Consumption by Location")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            if hierarchy.is_empty() {
                ui.label(
                    egui::RichText::new("No economic activity detected")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
                return;
            }

            for group in hierarchy {
                let bodies_with_flows: Vec<&BodyEconomyEntry> = group
                    .bodies
                    .iter()
                    .filter(|body_entry| body_has_resource_flows(body_entry, buildings_data))
                    .collect();

                if bodies_with_flows.is_empty() {
                    continue;
                }

                egui::CollapsingHeader::new(
                    egui::RichText::new(format!("⭐ {}", group.system_name))
                        .strong()
                        .size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    for body_entry in bodies_with_flows {
                        let body_icon = match body_entry.body_type {
                            BodyType::Planet | BodyType::GasGiant => "🪐",
                            BodyType::Moon => "🌙",
                            BodyType::Asteroid => "🪨",
                            BodyType::DwarfPlanet => "⚫",
                            BodyType::Comet => "☄",
                            _ => "🔵",
                        };

                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!("{} {}", body_icon, body_entry.body_name))
                                .size(12.0),
                        )
                        .default_open(false)
                        .show(ui, |ui| {
                            // Colony building production/consumption
                            if let Some(colony) = &body_entry.colony {
                                if let Some(data) = buildings_data {
                                    let mut production_rows: Vec<(String, ResourceType, f64)> =
                                        Vec::new();
                                    let mut consumption_rows: Vec<(String, ResourceType, f64)> =
                                        Vec::new();

                                    for (building_type, count) in &colony.buildings {
                                        if *count == 0 {
                                            continue;
                                        }
                                        // Maintenance consumption
                                        let maint = data.maintenance_resources(building_type);
                                        for (res_name, annual_amt) in maint {
                                            if let Some(rt) =
                                                crate::colony::data::parse_resource_type(res_name)
                                            {
                                                consumption_rows.push((
                                                    format!(
                                                        "{} ×{}",
                                                        building_type.display_name(),
                                                        count
                                                    ),
                                                    rt,
                                                    annual_amt * (*count as f64) / 12.0,
                                                ));
                                            }
                                        }
                                    }

                                    // Mining production (estimate from colony's deposits)
                                    // Show which resources the colony's mines/atmo processors extract
                                    let mut ui_surface_rate = 0.0_f64;
                                    let mut ui_deep_rate = 0.0_f64;
                                    let mut ui_bulk_rate = 0.0_f64;
                                    let mut total_atmo_rate = 0.0_f64;
                                    for (bt, count) in &colony.buildings {
                                        if *count == 0 {
                                            continue;
                                        }
                                        if let Some(def) = data.get(bt) {
                                            for modifier in &def.modifiers {
                                                match modifier.modifier_type.as_str() {
                                                    "MiningEfficiency" => {
                                                        ui_surface_rate +=
                                                            modifier.value * *count as f64
                                                    }
                                                    "DeepMiningEfficiency" => {
                                                        ui_deep_rate +=
                                                            modifier.value * *count as f64
                                                    }
                                                    "BulkMiningEfficiency" => {
                                                        ui_bulk_rate +=
                                                            modifier.value * *count as f64
                                                    }
                                                    "AtmosphericHarvesting" => {
                                                        total_atmo_rate +=
                                                            modifier.value * *count as f64
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }

                                    // Solid mining production breakdown — three tiers, no overflow
                                    if ui_surface_rate > 0.0 {
                                        let eligible: Vec<(ResourceType, f64)> = body_entry
                                            .deposits
                                            .iter()
                                            .filter(|(_, d)| {
                                                !d.is_atmospheric
                                                    && d.reserve.proven_crustal > 0.001
                                            })
                                            .map(|(rt, d)| {
                                                (*rt, (d.reserve.concentration as f64).max(1e-10))
                                            })
                                            .collect();
                                        let total_weight: f64 =
                                            eligible.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly = ui_surface_rate / 12.0;
                                            for (rt, weight) in &eligible {
                                                production_rows.push((
                                                    "Mining".to_string(),
                                                    *rt,
                                                    monthly * weight / total_weight,
                                                ));
                                            }
                                        }
                                    }
                                    if ui_deep_rate > 0.0 {
                                        let eligible: Vec<(ResourceType, f64)> = body_entry
                                            .deposits
                                            .iter()
                                            .filter(|(_, d)| {
                                                !d.is_atmospheric && d.reserve.deep_deposits > 0.001
                                            })
                                            .map(|(rt, d)| {
                                                (*rt, (d.reserve.concentration as f64).max(1e-10))
                                            })
                                            .collect();
                                        let total_weight: f64 =
                                            eligible.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly = ui_deep_rate / 12.0;
                                            for (rt, weight) in &eligible {
                                                production_rows.push((
                                                    "Deep Mining".to_string(),
                                                    *rt,
                                                    monthly * weight / total_weight,
                                                ));
                                            }
                                        }
                                    }
                                    if ui_bulk_rate > 0.0 {
                                        let eligible: Vec<(ResourceType, f64)> = body_entry
                                            .deposits
                                            .iter()
                                            .filter(|(_, d)| {
                                                !d.is_atmospheric
                                                    && d.reserve.planetary_bulk > 0.001
                                            })
                                            .map(|(rt, d)| {
                                                (*rt, (d.reserve.concentration as f64).max(1e-10))
                                            })
                                            .collect();
                                        let total_weight: f64 =
                                            eligible.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly = ui_bulk_rate / 12.0;
                                            for (rt, weight) in &eligible {
                                                production_rows.push((
                                                    "Bulk Mining".to_string(),
                                                    *rt,
                                                    monthly * weight / total_weight,
                                                ));
                                            }
                                        }
                                    }

                                    // Atmospheric harvesting production breakdown
                                    if total_atmo_rate > 0.0 {
                                        let harvestable: Vec<(ResourceType, f64)> = body_entry
                                            .deposits
                                            .iter()
                                            .filter(|(_, d)| {
                                                d.is_atmospheric
                                                    && (d.reserve.proven_crustal > 0.001
                                                        || d.reserve.deep_deposits > 0.001)
                                            })
                                            .map(|(rt, d)| {
                                                (*rt, (d.reserve.concentration as f64).max(1e-10))
                                            })
                                            .collect();
                                        let total_weight: f64 =
                                            harvestable.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly_total = total_atmo_rate / 12.0;
                                            for (rt, weight) in &harvestable {
                                                let share = weight / total_weight;
                                                production_rows.push((
                                                    "Atmo Harvesting".to_string(),
                                                    *rt,
                                                    monthly_total * share,
                                                ));
                                            }
                                        }
                                    }

                                    // Food production from agricultural buildings
                                    let farm_count: f64 = colony
                                        .buildings
                                        .iter()
                                        .find(|(bt, _)| *bt == crate::colony::BuildingType::Farm)
                                        .map(|(_, n)| *n as f64)
                                        .unwrap_or(0.0);
                                    let agri_count: f64 = colony
                                        .buildings
                                        .iter()
                                        .find(|(bt, _)| {
                                            *bt == crate::colony::BuildingType::AgriDome
                                        })
                                        .map(|(_, n)| *n as f64)
                                        .unwrap_or(0.0);
                                    let food_production_monthly =
                                        (farm_count * 100.0 + agri_count * 0.4) / 12.0;
                                    if food_production_monthly > 0.0 {
                                        production_rows.push((
                                            "Agriculture".to_string(),
                                            ResourceType::Food,
                                            food_production_monthly,
                                        ));
                                    }

                                    // Population food consumption
                                    let food_consumption_monthly =
                                        colony.population * 0.0001 / 12.0;
                                    if food_consumption_monthly > 0.0 {
                                        consumption_rows.push((
                                            "Population".to_string(),
                                            ResourceType::Food,
                                            food_consumption_monthly,
                                        ));
                                    }

                                    if !production_rows.is_empty() {
                                        ui.label(
                                            egui::RichText::new("Production (/mo):")
                                                .strong()
                                                .size(11.0)
                                                .color(theme::GREEN),
                                        );
                                        egui::Grid::new(format!(
                                            "econ_prod_{}",
                                            body_entry.body_name
                                        ))
                                        .num_columns(3)
                                        .spacing([10.0, 2.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            for (source, rt, monthly) in &production_rows {
                                                ui.label(egui::RichText::new(source).size(11.0));
                                                ui.label(
                                                    egui::RichText::new(rt.display_name())
                                                        .size(11.0),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "+{}",
                                                        format_mass(*monthly)
                                                    ))
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(theme::GREEN),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    }

                                    if !consumption_rows.is_empty() {
                                        ui.label(
                                            egui::RichText::new("Consumption (/mo):")
                                                .strong()
                                                .size(11.0)
                                                .color(theme::RED),
                                        );
                                        egui::Grid::new(format!(
                                            "econ_cons_{}",
                                            body_entry.body_name
                                        ))
                                        .num_columns(3)
                                        .spacing([10.0, 2.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            for (source, rt, monthly) in &consumption_rows {
                                                ui.label(egui::RichText::new(source).size(11.0));
                                                ui.label(
                                                    egui::RichText::new(rt.display_name())
                                                        .size(11.0),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "-{}",
                                                        format_mass(*monthly)
                                                    ))
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(theme::RED),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    }

                                    if production_rows.is_empty() && consumption_rows.is_empty() {
                                        ui.label(
                                            egui::RichText::new("No resource flows")
                                                .italics()
                                                .size(11.0)
                                                .color(theme::TEXT_DIM),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        egui::RichText::new("Building data not loaded")
                                            .italics()
                                            .size(11.0)
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                            }

                            // Standalone mining operations
                            for op in &body_entry.mining_ops {
                                let status = if op.active { "Active" } else { "Idle" };
                                let monthly = op.rate_mt_per_year / 12.0;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "⛏ {} — {}/mo [{}]",
                                            op.resource_type.display_name(),
                                            format_mass(monthly),
                                            status
                                        ))
                                        .size(11.0),
                                    );
                                });
                            }
                        });
                    }
                });
            }
        });

        // Placeholder for future sources
        ui.add_space(theme::Spacing::sm);
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🚧 Future Sources")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new("Stations and mining ships will appear here when implemented.")
                    .italics()
                    .size(11.0)
                    .color(theme::TEXT_HINT),
            );
        });
    });
}

// ---- Economy Tab: Colonies ----

fn render_econ_colonies(ui: &mut egui::Ui, budget: &GlobalBudget, hierarchy: &[StarSystemGroup]) {
    // Summary bar
    let all_colonies: Vec<&ColonySnapshot> = hierarchy
        .iter()
        .flat_map(|g| g.bodies.iter())
        .filter_map(|b| b.colony.as_ref())
        .collect();

    let total_pop: f64 = all_colonies.iter().map(|c| c.population).sum();
    let total_income: f64 = all_colonies.iter().map(|c| c.income_per_year).sum();
    let total_cost: f64 = all_colonies.iter().map(|c| c.operating_cost_per_year).sum();
    let net = total_income - total_cost;
    let net_color = if net >= 0.0 { theme::GREEN } else { theme::RED };

    draw_tab_h1(
        ui,
        "COLONIES",
        "Population, housing, labor, and financial performance.",
    );
    theme::elevated_frame().show(ui, |ui| {
        let sign = if net >= 0.0 { "+" } else { "" };
        ui.horizontal_wrapped(|ui| {
            draw_status_chip(
                ui,
                "COLONIES",
                all_colonies.len().to_string(),
                theme::ACCENT,
            );
            draw_status_chip(
                ui,
                "POPULATION",
                Colony::format_population(total_pop),
                theme::TEXT_VALUE,
            );
            draw_status_chip(
                ui,
                "NET",
                format!("{}{}/yr", sign, format_currency(net)),
                net_color,
            );
            draw_status_chip(
                ui,
                "TREASURY",
                format_currency(budget.treasury),
                theme::GOLD,
            );
        });
    });

    theme::divider(ui);

    if all_colonies.is_empty() {
        ui.add_space(20.0);
        ui.label(
            egui::RichText::new("No colonies established yet")
                .size(14.0)
                .italics()
                .color(theme::TEXT_DIM),
        );
        ui.label("Establish a colony to see economic breakdowns here.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for group in hierarchy {
            let sys_colonies: Vec<&BodyEconomyEntry> =
                group.bodies.iter().filter(|b| b.colony.is_some()).collect();
            if sys_colonies.is_empty() {
                continue;
            }

            let sys_income: f64 = sys_colonies
                .iter()
                .filter_map(|b| b.colony.as_ref())
                .map(|c| c.income_per_year)
                .sum();
            let sys_cost: f64 = sys_colonies
                .iter()
                .filter_map(|b| b.colony.as_ref())
                .map(|c| c.operating_cost_per_year)
                .sum();
            let sys_net = sys_income - sys_cost;
            let sys_net_color = if sys_net >= 0.0 {
                theme::GREEN
            } else {
                theme::RED
            };
            let sys_sign = if sys_net >= 0.0 { "+" } else { "" };

            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "⭐ {} — {} colonies, Net: {}{}/yr",
                    group.system_name,
                    sys_colonies.len(),
                    sys_sign,
                    format_currency(sys_net),
                ))
                .strong()
                .size(14.0)
                .color(sys_net_color),
            )
            .default_open(true)
            .show(ui, |ui| {
                for body_entry in &sys_colonies {
                    let colony = body_entry.colony.as_ref().unwrap();
                    let income = colony.income_per_year;
                    let cost = colony.operating_cost_per_year;
                    let colony_net = income - cost;
                    let cn_color = if colony_net >= 0.0 {
                        theme::GREEN
                    } else {
                        theme::RED
                    };
                    let cn_sign = if colony_net >= 0.0 { "+" } else { "" };

                    let body_icon = match body_entry.body_type {
                        BodyType::Planet | BodyType::GasGiant => "🪐",
                        BodyType::Moon => "🌙",
                        BodyType::Asteroid => "🪨",
                        BodyType::DwarfPlanet => "⚫",
                        _ => "🔵",
                    };

                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!(
                            "{} {} ({}) — Net: {}{}/yr",
                            body_icon,
                            colony.name,
                            body_entry.body_name,
                            cn_sign,
                            format_currency(colony_net),
                        ))
                        .strong()
                        .color(cn_color),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new(format!("econ_col_{}", colony.name))
                            .num_columns(2)
                            .spacing([20.0, 3.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Population:");
                                ui.label(Colony::format_population(colony.population));
                                ui.end_row();

                                ui.label("Growth:");
                                ui.label(format!(
                                    "+{}/yr",
                                    Colony::format_population(colony.growth_per_year)
                                ));
                                ui.end_row();

                                ui.label("Housing:");
                                let util = if colony.housing_capacity > 0.0 {
                                    colony.population / colony.housing_capacity * 100.0
                                } else {
                                    0.0
                                };
                                ui.label(format!(
                                    "{} / {} ({:.0}%)",
                                    Colony::format_population(colony.population),
                                    Colony::format_population(colony.housing_capacity),
                                    util
                                ));
                                ui.end_row();

                                ui.label("Buildings:");
                                ui.label(format!("{}", colony.total_buildings));
                                ui.end_row();

                                ui.label("Workforce:");
                                let wf_color = if colony.workforce_efficiency >= 1.0 {
                                    theme::GREEN
                                } else if colony.workforce_efficiency >= 0.5 {
                                    theme::AMBER
                                } else {
                                    theme::RED
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.0}%",
                                        colony.workforce_efficiency * 100.0
                                    ))
                                    .color(wf_color),
                                );
                                ui.end_row();

                                ui.label("Logistics:");
                                let log_color = if colony.logistics_efficiency >= 1.0 {
                                    theme::GREEN
                                } else if colony.logistics_efficiency >= 0.5 {
                                    theme::AMBER
                                } else {
                                    theme::RED
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.0}%",
                                        colony.logistics_efficiency * 100.0
                                    ))
                                    .color(log_color),
                                );
                                ui.end_row();

                                ui.label("Income:");
                                ui.label(
                                    egui::RichText::new(format!("{}/yr", format_currency(income)))
                                        .color(theme::GREEN),
                                );
                                ui.end_row();

                                ui.label("Operating Cost:");
                                ui.label(
                                    egui::RichText::new(format!("{}/yr", format_currency(cost)))
                                        .color(theme::RED),
                                );
                                ui.end_row();

                                ui.label("Net:");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}{}/yr",
                                        cn_sign,
                                        format_currency(colony_net)
                                    ))
                                    .strong()
                                    .color(cn_color),
                                );
                                ui.end_row();
                            });

                        // Buildings breakdown by category.
                        // PR-F (GRA-71) replaces the bespoke
                        // `egui::CollapsingHeader` chain with
                        // `theme::ledger_panel<ColonyRow>` — the
                        // `ColonyRow` marker signals "this is a
                        // per-colony ledger", and the `id_salt` is
                        // stable per colony so egui can remember
                        // open/closed state across frames.
                        if colony.total_buildings > 0 {
                            ui.add_space(4.0);
                            theme::ledger_panel(
                                ui,
                                &format!("econ_colony_buildings_{}", colony.name),
                                "📋 Buildings",
                                &ColonyRow,
                                |ui| {
                                    for category in BuildingCategory::all() {
                                        let in_cat: Vec<(BuildingType, u32)> = colony
                                            .buildings
                                            .iter()
                                            .filter(|(bt, _)| category.buildings().contains(bt))
                                            .map(|(bt, n)| (*bt, *n))
                                            .collect();

                                        if !in_cat.is_empty() {
                                            ui.label(
                                                egui::RichText::new(category.display_name())
                                                    .size(12.0)
                                                    .strong(),
                                            );
                                            for (building, count) in in_cat {
                                                ui.label(format!(
                                                    "  {} {} × {}",
                                                    building.icon(),
                                                    building.display_name(),
                                                    count
                                                ));
                                            }
                                        }
                                    }
                                },
                            );
                        }
                    });
                    ui.add_space(3.0);
                }
            });
        }

        // Future: Stations section placeholder
        ui.add_space(theme::Spacing::sm);
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🛸 Stations")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new("Space stations will appear here when implemented.")
                    .italics()
                    .size(11.0)
                    .color(theme::TEXT_HINT),
            );
        });
    });
}

// ---- Economy Tab: Mining ----

fn mining_body_matches_filters(
    body_entry: &BodyEconomyEntry,
    system_name: &str,
    state: &MiningTabUiState,
) -> bool {
    if !state.survey_filter.matches(body_entry.mean_coverage) {
        return false;
    }

    let visible_deposits = mining_visible_deposit_rows(body_entry, state.resource_filter);
    if visible_deposits.is_empty() {
        return false;
    }

    let active_ops = mining_matching_active_ops_count(body_entry, state.resource_filter);
    let activity_matches = match state.activity_filter {
        MiningActivityFilter::AllSites => true,
        MiningActivityFilter::ActiveMining => active_ops > 0,
        MiningActivityFilter::ColonySites => body_entry.colony.is_some(),
        MiningActivityFilter::Untapped => active_ops == 0,
    };
    if !activity_matches {
        return false;
    }

    let search = state.search_text.trim();
    if search.is_empty() {
        return true;
    }

    let search = search.to_ascii_lowercase();
    body_entry.body_name.to_ascii_lowercase().contains(&search)
        || system_name.to_ascii_lowercase().contains(&search)
}

fn render_mining_grouped_children(
    ui: &mut egui::Ui,
    children: &[Entity],
    group_name: &str,
    body_lookup: &std::collections::HashMap<Entity, &BodyEconomyEntry>,
    hierarchy: &std::collections::HashMap<Entity, Vec<Entity>>,
    state: &mut MiningTabUiState,
) {
    if children.is_empty() {
        return;
    }

    let id = ui.make_persistent_id(("econ_mining_group", group_name, children[0]));
    let collapsing_state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let is_open = collapsing_state.is_open();
    let mut header_clicked = false;
    let mut header = collapsing_state.show_header(ui, |ui| {
        header_clicked =
            render_mining_group_header(ui, group_name, children.len(), is_open).clicked();
    });
    if header_clicked {
        header.toggle();
    }
    header.body(|ui| {
        for child in children {
            render_mining_body_tree(ui, *child, body_lookup, hierarchy, state);
        }
    });
}

fn render_mining_body_tree(
    ui: &mut egui::Ui,
    entity: Entity,
    body_lookup: &std::collections::HashMap<Entity, &BodyEconomyEntry>,
    hierarchy: &std::collections::HashMap<Entity, Vec<Entity>>,
    state: &mut MiningTabUiState,
) {
    let Some(body_entry) = body_lookup.get(&entity).copied() else {
        return;
    };

    let mut child_planets = Vec::new();
    let mut child_dwarf_planets = Vec::new();
    let mut child_moons = Vec::new();
    let mut child_asteroids = Vec::new();
    let mut child_comets = Vec::new();
    let mut child_others = Vec::new();

    if let Some(children) = hierarchy.get(&entity) {
        for child in children {
            let Some(child_entry) = body_lookup.get(child).copied() else {
                continue;
            };

            match child_entry.body_type {
                BodyType::Planet | BodyType::GasGiant => child_planets.push(*child),
                BodyType::DwarfPlanet => child_dwarf_planets.push(*child),
                BodyType::Moon => child_moons.push(*child),
                BodyType::Asteroid => child_asteroids.push(*child),
                BodyType::Comet => child_comets.push(*child),
                _ => child_others.push(*child),
            }
        }
    }

    let has_children = !child_planets.is_empty()
        || !child_dwarf_planets.is_empty()
        || !child_moons.is_empty()
        || !child_asteroids.is_empty()
        || !child_comets.is_empty()
        || !child_others.is_empty();

    let meta = mining_body_meta(body_entry, state.resource_filter);

    if !has_children {
        if render_mining_body_row(ui, body_entry, state.selected_body == Some(entity), &meta)
            .clicked()
        {
            state.selected_body = Some(entity);
        }
        return;
    }

    let id = ui.make_persistent_id(("econ_mining_body", entity));
    let collapsing_state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        state.selected_body == Some(entity),
    );
    let mut header_clicked = false;
    let mut header = collapsing_state.show_header(ui, |ui| {
        header_clicked =
            render_mining_body_row(ui, body_entry, state.selected_body == Some(entity), &meta)
                .clicked();
    });
    if header_clicked {
        state.selected_body = Some(entity);
        header.toggle();
    }
    header.body(|ui| {
        for child in child_planets {
            render_mining_body_tree(ui, child, body_lookup, hierarchy, state);
        }
        render_mining_grouped_children(
            ui,
            &child_dwarf_planets,
            "Dwarf Planets",
            body_lookup,
            hierarchy,
            state,
        );
        for child in child_moons {
            render_mining_body_tree(ui, child, body_lookup, hierarchy, state);
        }
        render_mining_grouped_children(
            ui,
            &child_asteroids,
            "Asteroids",
            body_lookup,
            hierarchy,
            state,
        );
        render_mining_grouped_children(ui, &child_comets, "Comets", body_lookup, hierarchy, state);
        for child in child_others {
            render_mining_body_tree(ui, child, body_lookup, hierarchy, state);
        }
    });
}

fn render_mining_body_details(
    ui: &mut egui::Ui,
    system_name: &str,
    body_entry: &BodyEconomyEntry,
    state: &MiningTabUiState,
) {
    let visible_deposits = mining_visible_deposit_rows(body_entry, state.resource_filter);
    let estimated_total: f64 = visible_deposits.iter().map(|row| row.estimated_mt).sum();
    let active_ops = mining_matching_active_ops_count(body_entry, state.resource_filter);
    let avg_access = mining_body_average_accessibility(body_entry, state.resource_filter);

    ui.label(
        egui::RichText::new(format!(
            "{} {}",
            mining_body_icon(body_entry.body_type),
            body_entry.body_name
        ))
        .font(theme::title())
        .color(theme::ACCENT),
    );
    ui.label(
        egui::RichText::new(system_name)
            .font(theme::body(11.5))
            .color(theme::TEXT_DIM),
    );
    ui.add_space(theme::Spacing::sm);

    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            draw_status_chip(
                ui,
                "SURVEY",
                mining_survey_label(body_entry.survey_level).to_string(),
                mining_survey_color(body_entry.survey_level),
            );
            draw_status_chip(
                ui,
                "VISIBLE DEPOSITS",
                visible_deposits.len().to_string(),
                theme::TEXT_VALUE,
            );
            draw_status_chip(ui, "ESTIMATE", format_mass(estimated_total), theme::GREEN);
            draw_status_chip(
                ui,
                "AVG ACCESS",
                format!("{:.0}%", avg_access * 100.0),
                theme::EP_TEAL,
            );
            draw_status_chip(ui, "ACTIVE OPS", active_ops.to_string(), theme::ACCENT);
        });
    });

    ui.add_space(theme::Spacing::sm);
    ui.label(
        egui::RichText::new(match body_entry.survey_level {
            SurveyLevel::OrbitalScan => {
                "Estimate includes proven crustal reserves. Deeper layers remain hidden until seismic work completes."
            }
            SurveyLevel::SeismicSurvey => {
                "Estimate includes proven and deep deposits. Planetary bulk remains hidden until a core sample is completed."
            }
            SurveyLevel::CoreSample => {
                "Full reserve model unlocked. Estimates now include proven, deep, and planetary bulk layers."
            }
            SurveyLevel::Unsurveyed => {
                "No survey data available. This body should not appear under the current mining filters."
            }
        })
        .size(11.0)
        .color(theme::TEXT_DIM),
    );

    if let Some(colony) = &body_entry.colony {
        ui.add_space(theme::Spacing::sm);
        theme::elevated_frame().show(ui, |ui| {
            draw_tab_h1(
                ui,
                "SITE SUPPORT",
                "Industrial footprint and colony backing.",
            );
            egui::Grid::new(("econ_mining_site_support", body_entry.entity))
                .num_columns(2)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Colony");
                    ui.label(egui::RichText::new(&colony.name).color(theme::TEXT_VALUE));
                    ui.end_row();

                    ui.label("Population");
                    ui.label(Colony::format_population(colony.population));
                    ui.end_row();

                    ui.label("Buildings");
                    ui.label(colony.total_buildings.to_string());
                    ui.end_row();

                    ui.label("Logistics Efficiency");
                    ui.label(format!("{:.0}%", colony.logistics_efficiency * 100.0));
                    ui.end_row();
                });
        });
    }

    if active_ops > 0 {
        ui.add_space(theme::Spacing::sm);
        theme::elevated_frame().show(ui, |ui| {
            draw_tab_h1(
                ui,
                "ACTIVE EXTRACTION",
                "Current mining operations on this body.",
            );
            egui::Grid::new(("econ_mining_active_ops", body_entry.entity))
                .num_columns(3)
                .spacing([14.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Resource")
                            .font(theme::mono(10.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new("Rate")
                            .font(theme::mono(10.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new("Status")
                            .font(theme::mono(10.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.end_row();

                    for op in body_entry.mining_ops.iter().filter(|op| {
                        op.active
                            && state
                                .resource_filter
                                .is_none_or(|resource| op.resource_type == resource)
                    }) {
                        ui.label(op.resource_type.display_name());
                        ui.label(format!("{:.2} Mt/yr", op.rate_mt_per_year));
                        ui.label(egui::RichText::new("Active").color(theme::GREEN));
                        ui.end_row();
                    }
                });
        });
    }

    ui.add_space(theme::Spacing::sm);
    theme::elevated_frame().show(ui, |ui| {
        draw_tab_h1(
            ui,
            "SURVEYED DEPOSITS",
            "Known reserves filtered by the current mining view.",
        );

        if visible_deposits.is_empty() {
            ui.label(
                egui::RichText::new("No deposits match the current filters.")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
            return;
        }

        egui::Grid::new(("econ_mining_deposits", body_entry.entity))
            .num_columns(6)
            .spacing([12.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Resource")
                        .font(theme::mono(10.5))
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new("Estimate")
                        .font(theme::mono(10.5))
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new("Access")
                        .font(theme::mono(10.5))
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new("Concentration")
                        .font(theme::mono(10.5))
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new("Phase")
                        .font(theme::mono(10.5))
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new("Ops")
                        .font(theme::mono(10.5))
                        .color(theme::TEXT_DIM),
                );
                ui.end_row();

                for row in visible_deposits {
                    ui.label(format!(
                        "{} ({})",
                        row.resource_type.display_name(),
                        row.resource_type.symbol()
                    ));
                    ui.label(
                        egui::RichText::new(format_mass(row.estimated_mt)).color(theme::TEXT_VALUE),
                    );
                    ui.label(format!("{:.0}%", row.deposit.accessibility * 100.0));
                    ui.label(if row.deposit.is_atmospheric {
                        "--".to_string()
                    } else {
                        format!("{:.1}%", row.deposit.reserve.concentration * 100.0)
                    });
                    ui.label(if row.deposit.is_atmospheric {
                        "Atmospheric".to_string()
                    } else {
                        row.deposit.phase.to_string()
                    });
                    if let Some(rate) = row.active_rate_mt_per_year {
                        ui.label(
                            egui::RichText::new(format!("{:.2} Mt/yr", rate)).color(theme::GREEN),
                        );
                    } else {
                        ui.label(egui::RichText::new("Idle").color(theme::TEXT_DIM));
                    }
                    ui.end_row();
                }
            });
    });
}

// ---- Economy Tab: Forecast ----

/// Bundled `SystemParam` for the Forecast sub-tab.  Lets
/// `ui_economy_panels` stay under Bevy 0.18's 16-parameter fn-item
/// limit while still giving the Forecast tab the world data it needs.
#[derive(SystemParam)]
pub(super) struct ForecastInputs<'w, 's> {
    sim_time: Res<'w, SimulationTime>,
    view_mode: Res<'w, ViewMode>,
    current_star_system: Res<'w, CurrentStarSystem>,
    // v3.8.3: GlobalBudget for the per-body stockpile cap
    // (storage_multiplier × base_cap) and the StorageCaps
    // aggregate.  The forecast plateaus at this cap, with
    // the survey reserve as a hard geological limit below it.
    budget: Res<'w, crate::economy::GlobalBudget>,
    local_stockpile_query: Query<
        'w,
        's,
        (
            Option<&'static SystemId>,
            &'static crate::economy::components::LocalStockpile,
        ),
    >,
    construction_project_query:
        Query<'w, 's, (Entity, &'static ConstructionProject, &'static Colony)>,
    /// Reserve-aggregation query: per-body PlanetResources +
    /// SurveyLevel + SurveyState + SystemId, used to compute the
    /// survey-filtered geological reserves that bound the forecast
    /// curve (the player's warehouse can never exceed what can be
    /// extracted from known deposits).
    reserve_query: Query<
        'w,
        's,
        (
            Option<&'static SystemId>,
            Option<&'static SurveyLevel>,
            Option<&'static crate::survey::SurveyState>,
            &'static crate::economy::components::PlanetResources,
        ),
    >,
}

/// Per-resource toggle state for the Forecast sub-tab.
/// Stored as a sorted list of resource indices so the user-visible
/// default ("all enabled") survives UI restarts and is shareable.
#[derive(Default)]
struct ForecastUiState {
    /// Indices into `ResourceType::all()` for resources the user
    /// has explicitly disabled.  When empty, all are enabled.
    disabled: std::collections::BTreeSet<usize>,
}

impl ForecastUiState {
    fn is_enabled(&self, idx: usize) -> bool {
        !self.disabled.contains(&idx)
    }

    fn toggle(&mut self, idx: usize) {
        if !self.disabled.remove(&idx) {
            self.disabled.insert(idx);
        }
    }
}

fn render_econ_forecast(
    ui: &mut egui::Ui,
    contextual: &crate::economy::ContextualStockpile,
    rate_tracker: &ResourceRateTracker,
    forecast: &ForecastInputs,
    buildings_data: Option<&BuildingsData>,
) {
    draw_tab_h1(
        ui,
        "FORECAST",
        "20-year projection of resource stockpiles.  Lines use category colors; dashed red lines mark depletion dates; dashed amber lines mark when extraction hits the survey-filtered reserve cap.  Toggle resources on/off with the chips below.",
    );

    // --- Load persisted UI state ---
    let toggles_id = ui.id().with("forecast_toggles");
    let mut ui_state: ForecastUiState = ui
        .data_mut(|data| data.get_persisted::<Vec<u8>>(toggles_id))
        .map(|bytes| {
            let mut s = ForecastUiState::default();
            for b in bytes {
                s.disabled.insert(b as usize);
            }
            s
        })
        .unwrap_or_default();

    // Wrap the rest of the tab in a scroll area so the chart, summary
    // grids, and net-rate panel are reachable on small viewports or
    // when many resources are enabled (the chart alone is 220 px and
    // the toggle row alone can take 80+ px on wide chips).
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {

    // --- Scope: matches ViewMode (System = active system, Starmap = all) ---
    let scope_inputs = crate::economy::aggregate_scope_inputs(
        &forecast.view_mode,
        &forecast.current_star_system,
        &forecast.local_stockpile_query,
        rate_tracker,
    );

    // --- Build pending construction impacts ---
    let current_sim = forecast.sim_time.elapsed_seconds();
    let mut pending: Vec<(Entity, &ConstructionProject, f64)> = Vec::new();
    for (proj_entity, project, colony_comp) in forecast.construction_project_query.iter() {
        let bp_per_year = 1.0
            + (colony_comp.building_count(crate::colony::types::BuildingType::Factory) as f64) * 10.0;
        pending.push((proj_entity, project, bp_per_year));
    }
    let impacts = crate::economy::pending_construction_impacts(&pending, buildings_data, current_sim);

    // --- Aggregate survey-filtered reserve upper bounds for the
    // active scope.  Without this clamp, the forecast projects the
    // warehouse into teraton-scale values within ~20 years because
    // a 2026-calibrated mining modifier of ~1.8 Gt/yr × 12 mo × 240
    // mo ≈ 432 Gt is way more than a planet's actual extractable
    // reserves.  The clamp turns the curve into a flat plateau once
    // production has extracted the entire geological endowment.
    //
    // IMPORTANT: we deliberately sum `proven_crustal + deep_deposits`
    // only.  The `planetary_bulk` tier is the entire planetary mass
    // (e.g. 1.1 Pt of oxygen from silicate mantles) and is *never*
    // realistically mineable, even with future tech - that's why the
    // survey module calls it "effectively inaccessible early-game".
    // Survey tier 4+ in the existing model unlocks it, but that's a
    // visualisation choice for the dossier, not a mining claim.
    // The forecast should match reality: bulk-tier resources are
    // not part of the player's 20-year extraction ceiling.
    //
    // v3.8.3: also aggregate the per-body STORAGE cap (the user-
    // visible "indicated stockpile size" = per-body stockpile
    // cap × N_bodies in view).  This is the primary plateau the
    // forecast respects; the survey reserve acts as a hard
    // geological limit below it.
    let mut reserve_bounds = crate::economy::ReserveBounds::new();
    let mut storage_caps = crate::economy::StorageCaps::new();
    {
        use crate::survey::visibility::reserve_slice;
        let active_sys = forecast.current_star_system.0;
        let starmap = matches!(*forecast.view_mode, ViewMode::Starmap);
        // v3.8.3: count bodies in view so the storage cap is
        // scaled correctly.  The per-body cap is constant
        // (storage_multiplier is global) so cap × N_bodies
        // gives the aggregate cap for the view.
        let n_bodies: f64 = forecast
            .local_stockpile_query
            .iter()
            .filter(|(sid_opt, _)| match *forecast.view_mode {
                ViewMode::Starmap => true,
                ViewMode::System => {
                    sid_opt.map(|s| s.0) == Some(forecast.current_star_system.0)
                }
            })
            .count() as f64;
        // Insert per-resource storage cap once (same for every
        // body in view).
        for rt in crate::economy::ResourceType::all() {
            let base = crate::economy::GlobalBudget::stockpile_cap(*rt);
            if base >= f64::MAX {
                continue;
            }
            // Per-body effective cap = base × storage_multiplier.
            // Aggregate = per-body × N_bodies.
            let per_body = base * forecast.budget.storage_multiplier;
            let aggregate = per_body * n_bodies;
            if aggregate > 0.0 {
                storage_caps.insert(*rt, aggregate);
            }
        }
        for (sid_opt, level_opt, state_opt, resources) in forecast.reserve_query.iter() {
            let in_scope = starmap
                || sid_opt.is_some_and(|s| s.0 == active_sys);
            if !in_scope {
                continue;
            }
            // Tier number for the MineralDeposits axis (preferred
            // SurveyState, fallback to legacy SurveyLevel).
            let tier: u8 = state_opt
                .map(|s| s.fidelity(crate::survey::SurveyDimension::MineralDeposits).tier)
                .or_else(|| {
                    level_opt
                        .copied()
                        .map(|l| l.as_deposit_fidelity(0.0).tier)
                })
                .unwrap_or(0);
            if tier < 2 {
                // Unsurveyed / class-only: don't bound the projection;
                // the player has no idea how much is there.  Same as
                // not specifying a cap at all — fall through to the
                // legacy unbounded projection.
                continue;
            }
            for (rt, deposit) in &resources.deposits {
                let Some(slice) = reserve_slice(deposit, tier) else {
                    continue;
                };
                // Cap = proven + deep tiers only (never bulk).  The
                // survey system uses `planetary_bulk` for the dossier
                // total but it represents the planetary mantle/core
                // mass and is physically unrealistic to mine in any
                // 20-year horizon.  Excluding it here matches the
                // player's intuition of "what can my mines actually
                // touch".
                let upper_mt = slice.proven_crustal + slice.deep_deposits;
                if upper_mt > 0.0 {
                    let entry = reserve_bounds.resources.entry(*rt).or_insert(0.0);
                    *entry += upper_mt;
                }
            }
        }
    }

    // --- Build full forecast series list ---
    let all_series = crate::economy::build_forecast(
        &scope_inputs,
        &impacts,
        current_sim,
        &storage_caps,
        &reserve_bounds,
    );

    // --- Toggle row, grouped by category (matches the resource-bar popup) ---
    //
    // Each category header bar uses the category icon + color so the
    // player can read which series belongs to which group at a glance.
    // The chip background and foreground use the same category colors
    // as the resource bar — see `theme::CAT_*` and `category_color`.
    ui.add_space(theme::Spacing::sm);
    for (category_name, resources) in ResourceType::by_category() {
        // Skip categories that have no data in the current scope.
        let category_has_data = resources
            .iter()
            .any(|r| all_series.iter().any(|s| s.resource == *r));
        if !category_has_data {
            continue;
        }
        let cat_icon = get_resource_category_icon(category_name);
        let cat_color = theme::category_color(category_name);

        // Category header — icon + name in the category color, matching
        // the resource-bar category popup header.
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(cat_icon).size(13.0).color(cat_color))
                    .selectable(false),
            );
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(category_name)
                        .strong()
                        .size(11.0)
                        .color(cat_color),
                )
                .selectable(false),
            );
        });

        // Resource chips in this category — same icon + display name
        // format as the resource-bar popup rows, with a tinted
        // background showing whether the series is on (full color) or
        // off (dimmed color).
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            for resource in resources.iter().copied() {
                let has_data = all_series.iter().any(|s| s.resource == resource);
                if !has_data {
                    continue;
                }
                let idx = ResourceType::all()
                    .iter()
                    .position(|r| *r == resource)
                    .unwrap_or(usize::MAX);
                let enabled = ui_state.is_enabled(idx);
                let res_color = theme::category_color(resource.category());
                let chip_bg = if enabled {
                    res_color.linear_multiply(0.25)
                } else {
                    theme::SURFACE.linear_multiply(0.5)
                };
                let chip_text_color = if enabled { res_color } else { theme::TEXT_DIM };
                let text = format!(
                    "{} {}",
                    get_resource_icon(&resource),
                    resource.display_name()
                );
                let response = egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .fill(chip_bg)
                    .corner_radius(egui::CornerRadius::same(3))
                    .stroke(egui::Stroke::new(1.0, res_color.linear_multiply(0.4)))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text)
                                    .size(10.0)
                                    .color(chip_text_color),
                            )
                            .selectable(false),
                        );
                    })
                    .response;
                let click = response.interact(egui::Sense::click());
                if click.clicked() {
                    ui_state.toggle(idx);
                }
                if click.hovered() {
                    response.on_hover_text(format!(
                        "{} ({})\nClick to {} this series.",
                        resource.display_name(),
                        resource.category(),
                        if enabled { "hide" } else { "show" }
                    ));
                }
            }
        });
        ui.add_space(2.0);
    }

    // --- Persist the disabled set ---
    let disabled_bytes: Vec<u8> = ui_state.disabled.iter().map(|i| *i as u8).collect();
    ui.data_mut(|data| {
        data.insert_persisted(toggles_id, disabled_bytes);
    });

    // --- Filter enabled series ---
    let enabled_series: Vec<&crate::economy::ForecastSeries> = all_series
        .iter()
        .filter(|s| {
            let idx = ResourceType::all()
                .iter()
                .position(|r| *r == s.resource)
                .unwrap_or(usize::MAX);
            ui_state.is_enabled(idx)
        })
        .collect();

    ui.add_space(theme::Spacing::xs);
    theme::divider(ui);

    if enabled_series.is_empty() {
        ui.add_space(20.0);
        ui.label(
            egui::RichText::new(
                "No resource data to project.  Build a colony or wait for stockpiles to accumulate.",
            )
            .italics()
            .size(12.0)
            .color(theme::TEXT_DIM),
        );
        return;
    }

    // --- Chart canvas ---
    let desired_size = egui::vec2(ui.available_width(), theme::FORECAST_CHART_HEIGHT);
    ui.add_space(theme::Spacing::xs);
    render_forecast_chart(ui, &enabled_series, current_sim, desired_size, true);

    ui.add_space(theme::Spacing::md);

    // --- "Runs out" summary ---
    let any_depleting = enabled_series
        .iter()
        .any(|s| s.runs_out_at_s.is_some());

    // --- "Reserve cap" summary: surfaces survey-aware upper bounds
    // for mineable resources.  Without this, the chart lies about
    // sustainability when production rate × 20 yr exceeds the
    // geological endowment.
    let any_capped = enabled_series
        .iter()
        .any(|s| s.reserve_upper_bound_mt.is_some());

    let date_label = forecast.sim_time.format_date_time();
    let current_year_f: f64 = date_label
        .split(' ')
        .nth(2)
        .and_then(|y| y.parse().ok())
        .unwrap_or(2026.0);

    if any_capped {
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("⛏  Survey-known reserves on body")
                    .strong()
                    .color(theme::AMBER),
            );
            ui.add_space(2.0);
            // Gather entries: enabled series with a cap, plus any
            // resource that has a *surveyed* reserve bound but is not
            // currently shown (so a resource whose stockpile dipped to
            // 0 still appears in the panel instead of jumping in/out).
            let mut cap_entries: Vec<&crate::economy::ForecastSeries> = enabled_series
                .iter()
                .copied()
                .filter(|s| s.reserve_upper_bound_mt.is_some())
                .collect();
            for s in &all_series {
                if s.reserve_upper_bound_mt.is_some()
                    && !cap_entries.iter().any(|e| e.resource == s.resource)
                {
                    cap_entries.push(s);
                }
            }
            // Multi-column layout: pack entries into as many columns
            // as fit horizontally so the panel uses the full width
            // instead of stacking everything into a single tall
            // column.
            let row_count = cap_entries.len();
            let columns = thread_resources_columns(row_count);
            let per_col = row_count.div_ceil(columns);
            for chunk in cap_entries.chunks(per_col.max(1)) {
                ui.horizontal(|ui| {
                    for s in chunk {
                        let cap = s.reserve_upper_bound_mt.unwrap_or(0.0);
                        // v3.8.6 (2026-08-07): the section now shows
                        // the survey reserve value (the "resources
                        // left on the body") without a "(cap in Xy)"
                        // suffix.  The previous suffix referred to
                        // the time the forecast curve hits the
                        // *effective* cap (which is usually the
                        // storage cap, not the survey reserve) —
                        // confusing because the section is labeled
                        // "survey reserves".  The storage cap is
                        // already shown in the top-bar resource
                        // breakdown; this section is just "how much
                        // is in the ground".
                        let cap_label = format_mass(cap);
                        let name_text = format!(
                            "{} {}",
                            get_resource_icon(&s.resource),
                            s.resource.display_name()
                        );
                        ui.vertical(|ui| {
                            ui.set_width(180.0);
                            ui.label(
                                egui::RichText::new(name_text)
                                    .color(theme::forecast_series_color(s.resource.category()))
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(cap_label)
                                    .color(theme::AMBER)
                                    .monospace()
                                    .size(11.0),
                            );
                        });
                    }
                });
            }
        });
    }

    if any_depleting {
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("⏱  Runs out (next 20 yr)")
                    .strong()
                    .color(theme::forecast_runs_out_color()),
            );
            ui.add_space(2.0);
            // Multi-column layout: pack entries into as many columns
            // as fit horizontally so the panel uses the full width
            // instead of stacking everything into a single tall
            // column.
            let runs_out_entries: Vec<&crate::economy::ForecastSeries> = enabled_series
                .iter()
                .copied()
                .filter(|s| s.runs_out_at_s.is_some())
                .collect();
            let row_count = runs_out_entries.len();
            let columns = thread_resources_columns(row_count);
            let per_col = row_count.div_ceil(columns);
            for chunk in runs_out_entries.chunks(per_col.max(1)) {
                ui.horizontal(|ui| {
                    for s in chunk {
                        let runs_out = s.runs_out_at_s.unwrap_or(0.0);
                        let years = runs_out / crate::economy::SECONDS_PER_YEAR;
                        let target_year = current_year_f + years;
                        let color = forecast_depletion_color(years);
                        let name_text = format!(
                            "{} {}",
                            get_resource_icon(&s.resource),
                            s.resource.display_name()
                        );
                        ui.vertical(|ui| {
                            ui.set_width(180.0);
                            ui.label(
                                egui::RichText::new(name_text)
                                    .color(theme::forecast_series_color(s.resource.category()))
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "@ {target_year:.1}  ({})",
                                    format_forecast_years_remaining(years)
                                ))
                                .color(color)
                                .strong()
                                .monospace()
                                .size(11.0),
                            );
                        });
                    }
                });
            }
        });
    } else {
        theme::elevated_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("✓")
                        .size(16.0)
                        .color(theme::forecast_safe_color()),
                );
                ui.label(
                    egui::RichText::new(
                        "All enabled resources sustainable for 20+ years at current rates.",
                    )
                    .color(theme::forecast_safe_color()),
                );
            });
        });
    }

    ui.add_space(theme::Spacing::sm);

    // --- Net rate summary ---
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new("Net rate (annual)")
                .strong()
                .color(theme::TEXT_DIM),
        );
        ui.add_space(2.0);
        // Multi-column layout: pack entries into as many columns
        // as fit horizontally so the panel uses the full width
        // instead of stacking everything into a single tall column.
        let row_count = enabled_series.len();
        let columns = thread_resources_columns(row_count);
        let per_col = row_count.div_ceil(columns);
        for chunk in enabled_series.chunks(per_col.max(1)) {
            ui.horizontal(|ui| {
                for s in chunk {
                    let annual = s.annual_net_rate_mt;
                    let (rate_text_str, rate_color) = if annual.abs() < 1e-9 {
                        ("0/yr".to_string(), theme::TEXT_DIM)
                    } else {
                        let sign = if annual > 0.0 { "+" } else { "" };
                        (
                            format!("{}{}/yr", sign, format_mass(annual.abs())),
                            if annual > 0.0 { theme::GREEN } else { theme::RED },
                        )
                    };
                    let name_text = format!(
                        "{} {}",
                        get_resource_icon(&s.resource),
                        s.resource.display_name()
                    );
                    ui.vertical(|ui| {
                        ui.set_width(180.0);
                        ui.label(
                            egui::RichText::new(name_text)
                                .color(theme::forecast_series_color(s.resource.category()))
                                .size(11.0),
                        );
                        ui.label(
                            egui::RichText::new(rate_text_str)
                                .monospace()
                                .color(rate_color)
                                .size(11.0),
                        );
                    });
                }
            });
        }
    });

    let _ = contextual; // explicit unused-binding silence
    }); // end ScrollArea::vertical
}

/// Choose how many columns to split a flat list of forecast entries
/// across, so the panel fills the available horizontal width instead
/// of stacking everything into one tall column.  Each column is
/// ~180 px wide (see `set_width(180.0)` above); the threshold aims
/// for roughly 6–10 rows per column.
fn thread_resources_columns(row_count: usize) -> usize {
    if row_count == 0 {
        return 1;
    }
    if row_count <= 6 {
        return 1;
    }
    if row_count <= 14 {
        return 2;
    }
    if row_count <= 24 {
        return 3;
    }
    if row_count <= 36 {
        return 4;
    }
    5
}

/// Color a "years remaining" depletion badge by severity.
/// green ≥ 10 yr, amber 5–10 yr, red < 5 yr.
fn forecast_depletion_color(years_remaining: f64) -> egui::Color32 {
    if years_remaining < 5.0 {
        theme::RED
    } else if years_remaining < 10.0 {
        theme::AMBER
    } else {
        theme::GREEN
    }
}

/// Format a "years remaining" value for the depletion summary.
/// "∞" for infinite, "<1 yr" for sub-year, else "X.X yr".
fn format_forecast_years_remaining(years: f64) -> String {
    if !years.is_finite() {
        "∞".to_string()
    } else if years < 1.0 {
        format!("{:.0} mo", years * 12.0)
    } else if years < 100.0 {
        format!("{:.1} yr", years)
    } else {
        "100+ yr".to_string()
    }
}

/// Render the multi-series forecast chart into a fixed rectangle.
/// Mirrors the `render_history_plot` pattern from `resources_bar.rs`.
/// Draws grid, axis labels, one line per series, dashed vertical
/// "runs out" markers, and an interactive crosshair cursor.
///
/// `pub(super)` so the top-bar resource popup (in `resources_bar.rs`)
/// can re-use it for the per-resource click popup.
pub(super) fn render_forecast_chart(
    ui: &mut egui::Ui,
    series: &[&crate::economy::ForecastSeries],
    current_sim_seconds: f64,
    desired_size: egui::Vec2,
    interactive: bool,
) {
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

    if series.is_empty() || series.iter().all(|s| s.samples.len() < 2) {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Awaiting forecast samples",
            theme::body(11.0),
            theme::TEXT_DIM,
        );
        return;
    }

    let plot_rect = rect.shrink2(egui::vec2(10.0, 16.0));
    let horizon_s = crate::economy::FORECAST_HORIZON_YEARS * crate::economy::SECONDS_PER_YEAR;

    // Compute y-axis bounds across all enabled series.
    let (min_y, max_y) = compute_forecast_y_bounds(series);

    let to_screen = |sim_seconds_offset: f64, value: f64| {
        let x_t = (sim_seconds_offset / horizon_s).clamp(0.0, 1.0);
        let y_t = if (max_y - min_y).abs() < 1e-9 {
            0.5
        } else {
            ((value - min_y) / (max_y - min_y)).clamp(0.0, 1.0)
        };
        egui::pos2(
            plot_rect.left() + plot_rect.width() * x_t as f32,
            plot_rect.bottom() - plot_rect.height() * y_t as f32,
        )
    };

    // Vertical grid + x-axis tick labels (relative years).
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
        let years = crate::economy::FORECAST_HORIZON_YEARS * t;
        let label = if years < 1.0 {
            format!("{:.0}mo", years * 12.0)
        } else {
            format!("{years:.0}y")
        };
        painter.text(
            egui::pos2(x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            theme::mono(10.0),
            theme::TEXT_HINT,
        );
    }

    // Horizontal grid + y-axis tick labels.
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
            format_mass(value),
            theme::mono(10.0),
            theme::TEXT_HINT,
        );
    }

    // "Runs out" markers — dashed vertical lines for each series that depletes.
    let runs_out_color = theme::forecast_runs_out_color();
    let dash_len = theme::FORECAST_RUNS_OUT_DASH_LEN;
    let stroke_w = theme::FORECAST_RUNS_OUT_STROKE_WIDTH;
    for s in series {
        let Some(runs_out_offset) = s.runs_out_at_s else {
            continue;
        };
        if runs_out_offset > horizon_s {
            continue;
        }
        let x = to_screen(runs_out_offset, 0.0).x;
        // Dashed pattern: alternating `dash_len` px segments.
        let mut y = plot_rect.top();
        while y < plot_rect.bottom() {
            let next_y = (y + dash_len).min(plot_rect.bottom());
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, next_y)],
                egui::Stroke::new(stroke_w, runs_out_color),
            );
            y = next_y + dash_len;
        }
        // Floating label: "ResourceName @ Yr"
        let years = runs_out_offset / crate::economy::SECONDS_PER_YEAR;
        let label = format!("{} {:.1}y", s.resource.symbol(), years);
        painter.text(
            egui::pos2(x + 3.0, plot_rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            label,
            theme::mono(9.0),
            runs_out_color,
        );
    }

    // Series lines.
    for s in series {
        if s.samples.len() < 2 {
            continue;
        }
        let line_points: Vec<egui::Pos2> = s
            .samples
            .iter()
            .map(|p| to_screen(p.sim_seconds_offset, p.value_mt))
            .collect();
        let stroke_color = theme::forecast_series_color(s.resource.category());
        painter.add(egui::Shape::line(
            line_points,
            egui::Stroke::new(theme::FORECAST_LINE_WIDTH, stroke_color),
        ));
        // Highlight the t=0 anchor with a small dot.
        let anchor_pos = to_screen(0.0, s.samples[0].value_mt);
        painter.circle_filled(anchor_pos, 3.0, stroke_color);
    }

    // v3.8.5 (2026-08-07): "Reserve runway" — for any series whose
    // survey reserve is *larger* than the per-body cap the curve
    // plateaus at, draw a vertical dashed extension from the curve
    // plateau at year 20 up to the chart's top edge.  This visually
    // communicates "your stockpile caps the line here, but the
    // survey reserve is much larger — build more Warehouses to fill
    // it".  Per-series labels stack at the top with a small ▲ arrow
    // so the player can see the reserve cap value without the chart
    // re-scaling the y-axis.
    //
    // The y-axis is left untouched (still based on the 99th-percentile
    // of the sample plateau values), so a 600 Gt survey reserve can
    // sit "above" a 30 Gt storage-cap plateau without dominating the
    // chart.
    let reserve_color = theme::TEXT_HINT;
    let reserve_dash = theme::FORECAST_RUNS_OUT_DASH_LEN;
    let reserve_stroke_w = theme::FORECAST_RUNS_OUT_STROKE_WIDTH;
    let mut reserve_labels: Vec<(egui::Color32, String)> = Vec::new();
    for s in series {
        if s.samples.len() < 2 {
            continue;
        }
        let Some(reserve) = s.reserve_upper_bound_mt else {
            continue;
        };
        let effective = s
            .effective_upper_bound_mt
            .unwrap_or(s.samples.last().map(|p| p.value_mt).unwrap_or(0.0));
        // Only show the runway when the reserve is *meaningfully*
        // larger than the effective plateau (>= 1.5× ratio filters
        // out near-ties where the survey cap is just a hair above
        // the storage cap).
        if reserve < effective * 1.5 {
            continue;
        }
        // Vertical dashed line at year 20 from the plateau
        // (effective cap) up to the chart's top edge.
        let plateau_pos = to_screen(horizon_s, effective);
        let x = plateau_pos.x;
        let mut y = plateau_pos.y;
        while y > plot_rect.top() {
            let next_y = (y - reserve_dash).max(plot_rect.top());
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, next_y)],
                egui::Stroke::new(reserve_stroke_w, reserve_color),
            );
            y = next_y - reserve_dash;
        }
        // Small ▲ marker at the top of the dashed line, then
        // off-chart label with the reserve value.
        let label = format!("▲ {} {}", s.resource.symbol(), format_mass(reserve));
        reserve_labels.push((reserve_color, label));
    }
    // Stack labels from the top of the plot downward, offsetting by
    // line height so multiple series with reserves don't overlap.
    let mut y_cursor = plot_rect.top() + 2.0;
    for (color, text) in &reserve_labels {
        painter.text(
            egui::pos2(plot_rect.right() - 4.0, y_cursor),
            egui::Align2::RIGHT_TOP,
            text.clone(),
            theme::mono(9.0),
            *color,
        );
        y_cursor += 12.0;
    }

    // Interactive crosshair cursor.
    if interactive {
        if let Some(pointer_pos) = response.hover_pos().filter(|pos| plot_rect.contains(*pos)) {
            let fraction =
                ((pointer_pos.x - plot_rect.left()) / plot_rect.width()).clamp(0.0, 1.0) as f64;
            let target_offset = horizon_s * fraction;

            // Vertical crosshair.
            painter.line_segment(
                [
                    egui::pos2(pointer_pos.x, plot_rect.top()),
                    egui::pos2(pointer_pos.x, plot_rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, theme::ACCENT),
            );

            // Per-series horizontal lines + value labels.
            let mut cursor_y = plot_rect.top() + 8.0;
            for s in series {
                if s.samples.is_empty() {
                    continue;
                }
                // Find nearest sample to the cursor.
                let nearest = s
                    .samples
                    .iter()
                    .min_by(|a, b| {
                        (a.sim_seconds_offset - target_offset)
                            .abs()
                            .partial_cmp(&(b.sim_seconds_offset - target_offset).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .copied();
                let Some(nearest) = nearest else { continue };
                let pos = to_screen(nearest.sim_seconds_offset, nearest.value_mt);
                painter.line_segment(
                    [
                        egui::pos2(plot_rect.left(), pos.y),
                        egui::pos2(plot_rect.right(), pos.y),
                    ],
                    egui::Stroke::new(0.5_f32, theme::ACCENT_DIM),
                );
                painter.circle_filled(pos, 3.5, theme::ACCENT);

                // Floating label.
                let label = format!(
                    "{} {} @ {:.1}y",
                    s.resource.symbol(),
                    format_mass(nearest.value_mt),
                    nearest.sim_seconds_offset / crate::economy::SECONDS_PER_YEAR,
                );
                painter.text(
                    egui::pos2(plot_rect.right() - 4.0, cursor_y),
                    egui::Align2::RIGHT_TOP,
                    label,
                    theme::mono(9.0),
                    theme::forecast_series_color(s.resource.category()),
                );
                cursor_y += 12.0;
            }

            let _ = current_sim_seconds; // not currently used (no extra annotation)
        }
    }
}

/// Compute y-axis bounds for the forecast chart.  Clamps at zero
/// (resources never go negative in the projection), and uses a robust
/// percentile cutoff to avoid runaway scales from outliers.
///
/// The 99th-percentile cutoff means the top 1% of samples (typically
/// the long, flat plateau of one runaway uncapped series) doesn't
/// dominate the chart's y-axis — the y-scale is calibrated for the
/// "typical" 99% of the visible data.  Series that plateau above
/// this cutoff simply clip at the chart's top edge; the player can
/// still see them in the right-edge legend.
fn compute_forecast_y_bounds(series: &[&crate::economy::ForecastSeries]) -> (f64, f64) {
    let mut values: Vec<f64> = Vec::new();
    for s in series {
        for p in &s.samples {
            values.push(p.value_mt);
        }
    }
    if values.is_empty() {
        return (0.0, 1.0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let raw_min = values[0];
    // 99th-percentile: skip the top 1% (runaway plateau values).
    let p99_idx = ((values.len() as f64 * 0.99) as usize).min(values.len() - 1);
    let raw_max = values[p99_idx];
    let max_y = raw_max.max(0.0) * 1.10;
    let min_y = raw_min.clamp(0.0, 0.0); // never negative
    if (max_y - min_y).abs() < 1e-9 {
        (min_y, min_y + 1.0)
    } else {
        (min_y, max_y)
    }
}

fn render_econ_mining(
    ui: &mut egui::Ui,
    hierarchy: &[StarSystemGroup],
    mining_ui_state: &mut MiningTabUiState,
) {
    draw_tab_h1(
        ui,
        "MINING",
        "Surveyed deposits, reserve estimates, and extraction sites by orbital hierarchy.",
    );
    theme::divider(ui);

    let mut visible_groups: Vec<(&StarSystemGroup, Vec<&BodyEconomyEntry>)> = hierarchy
        .iter()
        .filter_map(|group| {
            let visible_bodies: Vec<_> = group
                .bodies
                .iter()
                .filter(|body_entry| {
                    mining_body_matches_filters(body_entry, &group.system_name, mining_ui_state)
                })
                .collect();

            if visible_bodies.is_empty() {
                None
            } else {
                Some((group, visible_bodies))
            }
        })
        .collect();
    visible_groups.sort_by(|left, right| left.0.system_name.cmp(&right.0.system_name));

    let first_visible_body = visible_groups
        .iter()
        .flat_map(|(_, bodies)| bodies.iter())
        .map(|body_entry| body_entry.entity)
        .next();

    if mining_ui_state.selected_body.is_none_or(|entity| {
        !visible_groups
            .iter()
            .any(|(_, bodies)| bodies.iter().any(|body| body.entity == entity))
    }) {
        mining_ui_state.selected_body = first_visible_body;
    }

    let visible_body_count: usize = visible_groups.iter().map(|(_, bodies)| bodies.len()).sum();
    let visible_deposit_count: usize = visible_groups
        .iter()
        .map(|(_, bodies)| {
            bodies
                .iter()
                .map(|body| {
                    mining_visible_deposit_rows(body, mining_ui_state.resource_filter).len()
                })
                .sum::<usize>()
        })
        .sum();
    let visible_estimate: f64 = visible_groups
        .iter()
        .map(|(_, bodies)| {
            bodies
                .iter()
                .map(|body| mining_body_estimated_total(body, mining_ui_state.resource_filter))
                .sum::<f64>()
        })
        .sum();
    let active_sites: usize = visible_groups
        .iter()
        .map(|(_, bodies)| {
            bodies
                .iter()
                .filter(|body| {
                    mining_matching_active_ops_count(body, mining_ui_state.resource_filter) > 0
                })
                .count()
        })
        .sum();

    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            draw_status_chip(
                ui,
                "MATCHING BODIES",
                visible_body_count.to_string(),
                theme::ACCENT,
            );
            draw_status_chip(
                ui,
                "SURVEYED DEPOSITS",
                visible_deposit_count.to_string(),
                theme::TEXT_VALUE,
            );
            draw_status_chip(
                ui,
                "VISIBLE ESTIMATE",
                format_mass(visible_estimate),
                theme::GREEN,
            );
            draw_status_chip(ui, "ACTIVE SITES", active_sites.to_string(), theme::EP_TEAL);
        });
    });

    ui.add_space(theme::Spacing::sm);
    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("FILTERS")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.add(
                egui::TextEdit::singleline(&mut mining_ui_state.search_text)
                    .hint_text("Search bodies or systems")
                    .desired_width(180.0),
            );

            egui::ComboBox::from_id_salt("econ_mining_resource_filter")
                .selected_text(match mining_ui_state.resource_filter {
                    Some(resource) => resource.display_name(),
                    None => "All Resources",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut mining_ui_state.resource_filter,
                        None,
                        "All Resources",
                    );
                    for resource in ResourceType::all()
                        .iter()
                        .copied()
                        .filter(ResourceType::is_mineable)
                    {
                        ui.selectable_value(
                            &mut mining_ui_state.resource_filter,
                            Some(resource),
                            resource.display_name(),
                        );
                    }
                });

            egui::ComboBox::from_id_salt("econ_mining_survey_filter")
                .selected_text(mining_ui_state.survey_filter.label())
                .show_ui(ui, |ui| {
                    for filter in MiningSurveyFilter::all() {
                        ui.selectable_value(
                            &mut mining_ui_state.survey_filter,
                            *filter,
                            filter.label(),
                        );
                    }
                });

            egui::ComboBox::from_id_salt("econ_mining_activity_filter")
                .selected_text(mining_ui_state.activity_filter.label())
                .show_ui(ui, |ui| {
                    for filter in MiningActivityFilter::all() {
                        ui.selectable_value(
                            &mut mining_ui_state.activity_filter,
                            *filter,
                            filter.label(),
                        );
                    }
                });

            egui::ComboBox::from_id_salt("econ_mining_sort_mode")
                .selected_text(mining_ui_state.sort_mode.label())
                .show_ui(ui, |ui| {
                    for sort_mode in MiningSortMode::all() {
                        ui.selectable_value(
                            &mut mining_ui_state.sort_mode,
                            *sort_mode,
                            sort_mode.label(),
                        );
                    }
                });

            if ui
                .button(if mining_ui_state.sort_descending {
                    "Desc"
                } else {
                    "Asc"
                })
                .on_hover_text("Toggle sort direction")
                .clicked()
            {
                mining_ui_state.sort_descending = !mining_ui_state.sort_descending;
            }

            if ui.button("Reset").clicked() {
                *mining_ui_state = MiningTabUiState::default();
            }
        });
    });

    ui.add_space(theme::Spacing::sm);

    if visible_groups.is_empty() {
        ui.add_space(20.0);
        ui.label(
            egui::RichText::new("No surveyed deposits match the current mining filters.")
                .size(14.0)
                .italics()
                .color(theme::TEXT_DIM),
        );
        return;
    }

    let selected_body = mining_ui_state.selected_body;
    let selected_entry = visible_groups.iter().find_map(|(group, bodies)| {
        bodies.iter().find_map(|body| {
            if Some(body.entity) == selected_body {
                Some((group.system_name.as_str(), *body))
            } else {
                None
            }
        })
    });

    ui.columns(2, |cols| {
        theme::elevated_frame().show(&mut cols[0], |ui| {
            ui.label(
                egui::RichText::new("RESOURCE LEDGER")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.label(
                egui::RichText::new("Grouped by system and orbital hierarchy.")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("econ_mining_ledger")
                .show(ui, |ui| {
                    for (group, visible_bodies) in &visible_groups {
                        let body_lookup: std::collections::HashMap<Entity, &BodyEconomyEntry> =
                            visible_bodies
                                .iter()
                                .copied()
                                .map(|body_entry| (body_entry.entity, body_entry))
                                .collect();
                        let mut children_by_parent: std::collections::HashMap<Entity, Vec<Entity>> =
                            std::collections::HashMap::new();
                        let mut roots = Vec::new();

                        for body_entry in visible_bodies {
                            if let Some(parent) = body_entry.logical_parent {
                                if body_lookup.contains_key(&parent) {
                                    children_by_parent
                                        .entry(parent)
                                        .or_default()
                                        .push(body_entry.entity);
                                    continue;
                                }
                            }
                            roots.push(body_entry.entity);
                        }

                        let sort_entities = |entities: &mut Vec<Entity>| {
                            entities.sort_by(|left, right| {
                                let left_entry = body_lookup.get(left).copied().unwrap();
                                let right_entry = body_lookup.get(right).copied().unwrap();
                                mining_compare_body_entries(
                                    left_entry,
                                    right_entry,
                                    mining_ui_state,
                                )
                            });
                        };

                        sort_entities(&mut roots);
                        for children in children_by_parent.values_mut() {
                            sort_entities(children);
                        }

                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!(
                                "⭐ {} ({})",
                                group.system_name,
                                visible_bodies.len()
                            ))
                            .font(theme::heading())
                            .color(theme::ACCENT),
                        )
                        .default_open(true)
                        .show(ui, |ui| {
                            for root in roots {
                                render_mining_body_tree(
                                    ui,
                                    root,
                                    &body_lookup,
                                    &children_by_parent,
                                    mining_ui_state,
                                );
                            }
                        });
                    }
                });
        });

        theme::elevated_frame().show(&mut cols[1], |ui| {
            ui.label(
                egui::RichText::new("BODY DETAILS")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.label(
                egui::RichText::new(
                    "Survey estimates and extraction readiness for the selected body.",
                )
                .size(11.0)
                .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("econ_mining_details")
                .show(ui, |ui| {
                    if let Some((system_name, body_entry)) = selected_entry {
                        render_mining_body_details(ui, system_name, body_entry, mining_ui_state);
                    } else {
                        ui.label(
                            egui::RichText::new("Select a body from the resource ledger.")
                                .size(13.0)
                                .color(theme::TEXT_DIM),
                        );
                    }
                });
        });
    });
}

// ---- Economy Tab: Power Grid ----

fn render_econ_power_grid(
    ui: &mut egui::Ui,
    budget: &GlobalBudget,
    hierarchy: &[StarSystemGroup],
    buildings_data: Option<&BuildingsData>,
) {
    let grid = &budget.energy_grid;
    let surplus = grid.surplus();
    let utilization = grid.load_factor();

    // Grid status header
    draw_tab_h1(
        ui,
        "POWER GRID",
        "Generation, load, and body-level power allocation.",
    );
    theme::elevated_frame().show(ui, |ui| {
        let (status_text, status_color) = if utilization < 0.5 {
            ("Abundant Power", theme::GREEN)
        } else if utilization < 0.8 {
            ("Healthy", theme::GREEN)
        } else if utilization < 1.0 {
            ("Strained", theme::AMBER)
        } else {
            ("DEFICIT — Build more power!", theme::RED)
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⚡ Grid Status:").strong().size(14.0));
            ui.label(
                egui::RichText::new(status_text)
                    .strong()
                    .size(14.0)
                    .color(status_color),
            );
        });

        let bar_pct = utilization.min(1.0) as f32;
        ui.add(
            egui::ProgressBar::new(bar_pct)
                .text(format!(
                    "{} / {} ({:.1}%)",
                    format_power(grid.consumed),
                    format_power(grid.produced),
                    utilization * 100.0,
                ))
                .desired_width(ui.available_width().min(600.0)),
        );

        let surplus_color = if surplus >= 0.0 {
            theme::GREEN
        } else {
            theme::RED
        };
        ui.label(
            egui::RichText::new(format!("Surplus: {}", format_power(surplus))).color(surplus_color),
        );
    });

    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Power breakdown by source type
        if !budget.power_breakdown.is_empty() {
            theme::elevated_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new("🔋 Production by Source Type")
                        .font(theme::heading())
                        .color(theme::ACCENT),
                );
                ui.separator();

                egui::Grid::new("econ_pwr_breakdown")
                    .num_columns(2)
                    .spacing([20.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (source_type, wattage) in &budget.power_breakdown {
                            ui.label(format!("{}", source_type));
                            ui.label(
                                egui::RichText::new(format_power(*wattage))
                                    .monospace()
                                    .color(theme::GREEN),
                            );
                            ui.end_row();
                        }
                    });
            });
            ui.add_space(theme::Spacing::sm);
        }

        let mut colony_power: Vec<&ColonySnapshot> = hierarchy
            .iter()
            .flat_map(|group| group.bodies.iter())
            .filter_map(|body| body.colony.as_ref())
            .collect();
        colony_power.sort_by(|left, right| {
            right
                .power_load_watts
                .partial_cmp(&left.power_load_watts)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if !colony_power.is_empty() {
            theme::elevated_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new("🏠 Colony Power Breakdown")
                        .font(theme::heading())
                        .color(theme::ACCENT),
                );
                ui.separator();

                for colony in colony_power {
                    let net_power = colony.power_generation_watts - colony.power_load_watts;
                    let utilization = if colony.power_generation_watts > 0.0 {
                        colony.power_load_watts / colony.power_generation_watts
                    } else {
                        0.0
                    };
                    let net_color = if net_power >= 0.0 {
                        theme::GREEN
                    } else {
                        theme::RED
                    };
                    let util_color = if colony.power_generation_watts <= 0.0 {
                        theme::TEXT_DIM
                    } else if utilization < 0.8 {
                        theme::GREEN
                    } else if utilization < 1.0 {
                        theme::AMBER
                    } else {
                        theme::RED
                    };

                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!(
                            "{} | Gen {} | Load {} | Net {} | {}",
                            colony.name,
                            format_power(colony.power_generation_watts),
                            format_power(colony.power_load_watts),
                            format_power(net_power),
                            if colony.power_generation_watts > 0.0 {
                                format!("Util {:.1}%", utilization * 100.0)
                            } else {
                                "Util n/a".to_string()
                            }
                        ))
                        .size(12.5),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        let building_rows = build_colony_power_rows(colony, buildings_data);

                        egui::Grid::new(format!("colony_power_buildings_{}", colony.name))
                            .num_columns(5)
                            .spacing([16.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Building Type").strong());
                                ui.label(egui::RichText::new("Count").strong());
                                ui.label(egui::RichText::new("Generation").strong());
                                ui.label(egui::RichText::new("Load").strong());
                                ui.label(egui::RichText::new("Net").strong());
                                ui.end_row();

                                for row in building_rows {
                                    let building_net = row.produced_watts - row.consumed_watts;
                                    let building_net_color = if building_net >= 0.0 {
                                        theme::GREEN
                                    } else {
                                        theme::RED
                                    };

                                    ui.label(row.building_type.display_name());
                                    ui.label(row.count.to_string());
                                    ui.label(
                                        egui::RichText::new(format_power(row.produced_watts))
                                            .color(theme::GREEN)
                                            .monospace(),
                                    );
                                    ui.label(
                                        egui::RichText::new(format_power(row.consumed_watts))
                                            .color(theme::AMBER)
                                            .monospace(),
                                    );
                                    ui.label(
                                        egui::RichText::new(format_power(building_net))
                                            .color(building_net_color)
                                            .monospace(),
                                    );
                                    ui.end_row();
                                }
                            });

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Production {}",
                                    format_power(colony.power_generation_watts)
                                ))
                                .color(theme::GREEN),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Load {}",
                                    format_power(colony.power_load_watts)
                                ))
                                .color(theme::AMBER),
                            );
                            ui.label(
                                egui::RichText::new(format!("Net {}", format_power(net_power)))
                                    .color(net_color),
                            );
                            ui.label(
                                egui::RichText::new(if colony.power_generation_watts > 0.0 {
                                    format!("Utilization {:.1}%", utilization * 100.0)
                                } else {
                                    "Utilization n/a".to_string()
                                })
                                .color(util_color),
                            );
                        });
                    });
                }
            });
            ui.add_space(theme::Spacing::sm);
        }

        // Per-system power breakdown
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🌟 Power by Location")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            if hierarchy.is_empty() {
                ui.label(
                    egui::RichText::new("No power sources detected")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
                return;
            }

            for group in hierarchy {
                let has_power_data = group
                    .bodies
                    .iter()
                    .any(|b| !b.generators.is_empty() || b.colony.is_some());
                if !has_power_data {
                    continue;
                }

                egui::CollapsingHeader::new(
                    egui::RichText::new(format!("⭐ {}", group.system_name))
                        .strong()
                        .size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    for body_entry in &group.bodies {
                        if body_entry.generators.is_empty() && body_entry.colony.is_none() {
                            continue;
                        }

                        let body_icon = power_body_icon(body_entry.body_type);

                        ui.horizontal(|ui| {
                            let generator_output_watts: f64 = body_entry
                                .generators
                                .iter()
                                .map(|gen| gen.output_watts)
                                .sum();
                            let colony_generation_watts = body_entry
                                .colony
                                .as_ref()
                                .map(|colony| colony.power_generation_watts)
                                .unwrap_or(0.0);
                            let colony_load_watts = body_entry
                                .colony
                                .as_ref()
                                .map(|colony| colony.power_load_watts)
                                .unwrap_or(0.0);
                            let total_generation_watts =
                                generator_output_watts + colony_generation_watts;
                            let net_power_watts = total_generation_watts - colony_load_watts;

                            ui.label(
                                egui::RichText::new(format!(
                                    "{} {}",
                                    body_icon, body_entry.body_name
                                ))
                                .strong()
                                .size(12.0),
                            );

                            if total_generation_watts > 0.0 {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "| Gen {}",
                                        format_power(total_generation_watts)
                                    ))
                                    .size(11.0)
                                    .color(theme::GREEN),
                                );
                            }

                            if colony_load_watts > 0.0 {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "| Load {}",
                                        format_power(colony_load_watts)
                                    ))
                                    .size(11.0)
                                    .color(theme::AMBER),
                                );
                            }

                            if total_generation_watts > 0.0 || colony_load_watts > 0.0 {
                                let net_color = if net_power_watts >= 0.0 {
                                    theme::GREEN
                                } else {
                                    theme::RED
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "| Net {}",
                                        format_power(net_power_watts)
                                    ))
                                    .size(11.0)
                                    .color(net_color),
                                );
                            }

                            for gen in &body_entry.generators {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "| {} {}",
                                        gen.source_type,
                                        format_power(gen.output_watts)
                                    ))
                                    .size(11.0)
                                    .color(theme::TEXT_DIM),
                                );
                            }
                        });
                    }
                });
            }
        });

        // Future sources
        ui.add_space(theme::Spacing::sm);
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🚧 Future Power Sources")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(
                    "Station and ship power grids will appear here when implemented.",
                )
                .italics()
                .size(11.0)
                .color(theme::TEXT_HINT),
            );
        });
    });
}

// ── Logistics tab ────────────────────────────────────────────────────────────

/// Render the Logistics tab: open resource requests and shipping company registry.
fn render_econ_logistics(
    ui: &mut egui::Ui,
    resource_requests: &crate::economy::PendingResourceRequests,
    companies: &mut crate::economy::ShippingCompanies,
    budget: &GlobalBudget,
) {
    use crate::economy::{CompanyAIPolicy, CompanyBuildPolicy, RequestPriority, RequestState};

    draw_tab_h1(
        ui,
        "LOGISTICS",
        "Freight capacity, open requests, and recent delivery activity.",
    );
    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── Shipping Companies ────────────────────────────────────────────────
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("🚀 Shipping Companies")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            if companies.companies.is_empty() {
                ui.label(
                    egui::RichText::new("No private shipping companies active yet.")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                egui::Grid::new("shipping_companies")
                    .num_columns(8)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Company").strong().size(11.0));
                        ui.label(egui::RichText::new("Treasury").strong().size(11.0));
                        ui.label(egui::RichText::new("Fleet").strong().size(11.0));
                        ui.label(egui::RichText::new("Available").strong().size(11.0));
                        ui.label(egui::RichText::new("Deliveries").strong().size(11.0));
                        ui.label(egui::RichText::new("AI Policy").strong().size(11.0));
                        ui.label(egui::RichText::new("Build Policy").strong().size(11.0));
                        ui.label(egui::RichText::new("Queued").strong().size(11.0));
                        ui.end_row();

                        for company in &mut companies.companies {
                            ui.label(egui::RichText::new(&company.name).size(12.0).strong());
                            ui.label(
                                egui::RichText::new(format_currency(company.treasury_mc))
                                    .size(11.0)
                                    .color(theme::GOLD),
                            );
                            ui.label(
                                egui::RichText::new(format!("{} ships", company.freighter_count))
                                    .size(11.0),
                            );
                            let available_color = if company.available_freighters > 0 {
                                theme::GREEN
                            } else {
                                theme::RED
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} idle",
                                    company.available_freighters
                                ))
                                .size(11.0)
                                .color(available_color),
                            );
                            ui.label(
                                egui::RichText::new(format!("{}", company.total_deliveries))
                                    .size(11.0),
                            );
                            // GRA-38: per-company AI policy toggle.  Default
                            // is AutoFreight (DW2-style opt-out).  Player
                            // can switch a specific company to Manual to
                            // keep it as a passive treasury / reputation
                            // holder.
                            let policy_before = company.policy;
                            egui::ComboBox::from_id_salt(format!(
                                "company_policy_{}",
                                company.name
                            ))
                            .selected_text(company.policy.to_string())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut company.policy,
                                    CompanyAIPolicy::AutoFreight,
                                    "🤖 Auto-Freight",
                                );
                                ui.selectable_value(
                                    &mut company.policy,
                                    CompanyAIPolicy::Manual,
                                    "✋ Manual",
                                );
                            });
                            if company.policy != policy_before {
                                info!(
                                    "GRA-38: company {} AI policy changed {:?} → {:?}",
                                    company.name, policy_before, company.policy
                                );
                            }
                            // GRA-39: per-company build policy toggle.
                            // Default is Manual (opt-in).  A company with
                            // no `home_body` (e.g. the seeded defaults) is
                            // a no-op even on AutoBuild — the auto-build
                            // loop skips it.
                            let build_policy_before = company.build_policy;
                            egui::ComboBox::from_id_salt(format!(
                                "company_build_policy_{}",
                                company.name
                            ))
                            .selected_text(company.build_policy.to_string())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut company.build_policy,
                                    CompanyBuildPolicy::AutoBuild,
                                    "🏗 Auto-Build",
                                );
                                ui.selectable_value(
                                    &mut company.build_policy,
                                    CompanyBuildPolicy::Manual,
                                    "✋ Manual",
                                );
                            });
                            if company.build_policy != build_policy_before {
                                info!(
                                    "GRA-39: company {} build policy changed {:?} → {:?}",
                                    company.name, build_policy_before, company.build_policy
                                );
                            }
                            // GRA-39: queued-builds column.  Reads the
                            // cached `active_builds` written by
                            // `auto_build_loop` each tick.
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} / {}",
                                    company.active_builds, company.max_active_builds
                                ))
                                .size(11.0)
                                .color(
                                    if company.active_builds >= company.max_active_builds {
                                        theme::AMBER
                                    } else {
                                        theme::TEXT
                                    },
                                ),
                            );
                            ui.end_row();
                        }
                    });
            }
        });

        ui.add_space(theme::Spacing::sm);

        // ── Treasury Info ─────────────────────────────────────────────────────
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("💰 Logistics Expenditure")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();
            let total_paid: f64 = companies
                .companies
                .iter()
                .map(|c| c.total_deliveries as f64 * 100.0) // rough estimate
                .sum();
            ui.label(
                egui::RichText::new(format!(
                    "Player treasury: {}",
                    format_currency(budget.treasury)
                ))
                .size(12.0),
            );
            ui.label(
                egui::RichText::new(
                    "Payments are deducted from treasury when deliveries complete.",
                )
                .size(11.0)
                .color(theme::TEXT_DIM),
            );
            let _ = total_paid;
        });

        ui.add_space(theme::Spacing::sm);

        // ── Open Resource Requests ────────────────────────────────────────────
        let open: Vec<_> = resource_requests.open_requests().collect();
        let delivered: Vec<_> = resource_requests
            .requests
            .iter()
            .filter(|r| matches!(r.state, RequestState::Delivered))
            .collect();

        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("📋 Open Requests ({})", open.len()))
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            if open.is_empty() {
                ui.label(
                    egui::RichText::new("No open resource requests.")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                egui::Grid::new("open_requests_grid")
                    .num_columns(5)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Destination").strong().size(11.0));
                        ui.label(egui::RichText::new("Resource").strong().size(11.0));
                        ui.label(egui::RichText::new("Amount").strong().size(11.0));
                        ui.label(egui::RichText::new("Priority").strong().size(11.0));
                        ui.label(egui::RichText::new("Status").strong().size(11.0));
                        ui.end_row();

                        for req in &open {
                            ui.label(
                                egui::RichText::new(&req.destination_name)
                                    .size(11.0)
                                    .strong(),
                            );
                            ui.label(egui::RichText::new(format!("{:?}", req.resource)).size(11.0));
                            ui.label(
                                egui::RichText::new(format!("{:.1} Mt", req.amount_mt)).size(11.0),
                            );
                            let priority_color = match req.priority {
                                RequestPriority::Emergency => theme::RED,
                                RequestPriority::Construction => theme::AMBER,
                                RequestPriority::Maintenance => theme::ACCENT,
                                RequestPriority::Trade => theme::TEXT_DIM,
                            };
                            ui.label(
                                egui::RichText::new(format!("{}", req.priority))
                                    .size(11.0)
                                    .color(priority_color),
                            );
                            let (state_text, state_color) = match req.state {
                                RequestState::Pending => ("⏳ Pending", theme::TEXT_DIM),
                                RequestState::Assigned => ("📋 Assigned", theme::AMBER),
                                RequestState::InTransit => ("🚀 In Transit", theme::GREEN),
                                _ => ("?", theme::TEXT_DIM),
                            };
                            ui.label(
                                egui::RichText::new(state_text)
                                    .size(11.0)
                                    .color(state_color),
                            );
                            ui.end_row();
                        }
                    });
            }
        });

        ui.add_space(theme::Spacing::sm);

        // ── Recent Deliveries ─────────────────────────────────────────────────
        if !delivered.is_empty() {
            theme::elevated_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("✅ Recent Deliveries ({})", delivered.len()))
                        .font(theme::heading())
                        .color(theme::ACCENT),
                );
                ui.separator();

                egui::Grid::new("delivered_grid")
                    .num_columns(4)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Destination").strong().size(11.0));
                        ui.label(egui::RichText::new("Resource").strong().size(11.0));
                        ui.label(egui::RichText::new("Delivered").strong().size(11.0));
                        ui.label(egui::RichText::new("Company").strong().size(11.0));
                        ui.end_row();

                        // Show last 20 deliveries newest first.
                        for req in delivered.iter().rev().take(20) {
                            ui.label(
                                egui::RichText::new(&req.destination_name)
                                    .size(11.0)
                                    .strong(),
                            );
                            ui.label(egui::RichText::new(format!("{:?}", req.resource)).size(11.0));
                            ui.label(
                                egui::RichText::new(format!("{:.1} Mt", req.in_transit_mt))
                                    .size(11.0)
                                    .color(theme::GREEN),
                            );
                            let company_name = req
                                .assigned_company_idx
                                .and_then(|i| companies.companies.get(i))
                                .map(|c| c.name.as_str())
                                .unwrap_or("—");
                            ui.label(egui::RichText::new(company_name).size(11.0));
                            ui.end_row();
                        }
                    });
            });
        }
    });
}

// ---- Economy Tab: Private Shipping (GRA-37.e) ---------------------------------

/// Per-company aggregated row for the Private Shipping overview.
struct ShippingOverviewRow {
    company_idx: usize,
    name: String,
    policy: crate::economy::CompanyAIPolicy,
    freighter_count: u32,
    available_freighters: u32,
    in_transit: u32,
    /// Open demand in megatons, grouped by `ResourceType`.  Uses
    /// `HashMap` because `ResourceType` derives `Hash + Eq` but not `Ord`.
    open_demand_by_resource: std::collections::HashMap<crate::economy::ResourceType, f64>,
    /// 0.0..=1.0 — fraction of requests created in the last `WINDOW_S` that
    /// have transitioned to `Delivered`.  `None` when no requests were
    /// created in the window (avoid divide-by-zero).
    fulfillment_rate: Option<f64>,
    /// `treasury_mc - treasury_window_start_mc` — positive = net earned,
    /// negative = net spent.  See `ShippingCompany::maybe_roll_treasury_window`.
    treasury_delta_mc: f64,
}

/// Length of the rolling window (seconds) for the fulfillment-rate and
/// treasury-delta columns.  Matches the per-company treasury window.
const SHIPPING_OVERVIEW_WINDOW_S: f64 = 60.0 * 86_400.0;

fn build_shipping_overview_rows(
    companies: &crate::economy::ShippingCompanies,
    resource_requests: &crate::economy::PendingResourceRequests,
    now: f64,
) -> Vec<ShippingOverviewRow> {
    let window_start = now - SHIPPING_OVERVIEW_WINDOW_S;
    let mut rows: Vec<ShippingOverviewRow> = companies
        .companies
        .iter()
        .enumerate()
        .map(|(idx, c)| {
            let in_transit = c.freighter_count.saturating_sub(c.available_freighters);
            let mut open_demand_by_resource: std::collections::HashMap<
                crate::economy::ResourceType,
                f64,
            > = Default::default();
            for req in resource_requests.requests.iter() {
                if req.assigned_company_idx != Some(idx) || !req.is_open() {
                    continue;
                }
                *open_demand_by_resource.entry(req.resource).or_insert(0.0) += req.amount_mt;
            }

            // Fulfillment rate: of the requests created in the window
            // targeting this company, what fraction has been delivered?
            // Excludes `Expired` and `InTransit` — only counts outcomes.
            let mut created_in_window = 0u32;
            let mut delivered_in_window = 0u32;
            for req in resource_requests.requests.iter() {
                if req.assigned_company_idx != Some(idx) {
                    continue;
                }
                if req.created_at_seconds < window_start {
                    continue;
                }
                created_in_window += 1;
                if matches!(req.state, crate::economy::RequestState::Delivered) {
                    delivered_in_window += 1;
                }
            }
            let fulfillment_rate = if created_in_window == 0 {
                None
            } else {
                Some(delivered_in_window as f64 / created_in_window as f64)
            };

            let treasury_delta_mc = c.treasury_mc - c.treasury_window_start_mc;

            ShippingOverviewRow {
                company_idx: idx,
                name: c.name.clone(),
                policy: c.policy,
                freighter_count: c.freighter_count,
                available_freighters: c.available_freighters,
                in_transit,
                open_demand_by_resource,
                fulfillment_rate,
                treasury_delta_mc,
            }
        })
        .collect();

    // Default sort: fulfillment rate desc (None treated as 0.0 so untested
    // companies sort to the bottom), then by name for stable presentation.
    rows.sort_by(|a, b| {
        let ar = a.fulfillment_rate.unwrap_or(0.0);
        let br = b.fulfillment_rate.unwrap_or(0.0);
        br.partial_cmp(&ar)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });
    rows
}

/// Render the Private Shipping overview (GRA-37.e) — one row per
/// `ShippingCompany` with freighter counts, open demand by cargo, a
/// rolling-window fulfillment rate, treasury delta, and AI policy.
/// Clicking a company row navigates to the Fleets panel filtered to
/// that company (sets `ShippingCompanyFilter`).
///
/// Returns the index of the clicked company row (if any); the caller is
/// responsible for setting `ShippingCompanyFilter` and switching
/// `ActiveMenu` to `Fleets` (action-queue decoupling — see
/// `helios-architecture`).
fn render_shipping_overview(
    ui: &mut egui::Ui,
    resource_requests: &crate::economy::PendingResourceRequests,
    companies: &crate::economy::ShippingCompanies,
    budget: &GlobalBudget,
    show_freighters_in_transit: bool,
) -> Option<usize> {
    let mut clicked: Option<usize> = None;

    draw_tab_h1(
        ui,
        "PRIVATE SHIPPING",
        "Per-company freight capacity, open demand, and recent throughput. \
         Click a row to open the Fleets panel filtered to that company.",
    );
    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── Header chips ─────────────────────────────────────────────────────
        let total_open_mt: f64 = resource_requests.open_requests().map(|r| r.amount_mt).sum();
        let total_companies = companies.companies.len();
        let total_freighters: u32 = companies.companies.iter().map(|c| c.freighter_count).sum();
        let total_idle: u32 = companies
            .companies
            .iter()
            .map(|c| c.available_freighters)
            .sum();

        ui.horizontal_wrapped(|ui| {
            draw_status_chip(
                ui,
                "COMPANIES",
                total_companies.to_string(),
                theme::TEXT_VALUE,
            );
            ui.separator();
            draw_status_chip(
                ui,
                "FLEETERS",
                format!("{}/{}", total_idle, total_freighters),
                theme::RP_BLUE,
            );
            ui.separator();
            draw_status_chip(
                ui,
                "OPEN DEMAND",
                format!("{:.1} Mt", total_open_mt),
                theme::AMBER,
            );
            ui.separator();
            draw_status_chip(ui, "WINDOW", "60 in-game days".to_string(), theme::TEXT_DIM);
        });

        ui.add_space(6.0);
        let _ = budget; // budget surfaced via Logistics tab; kept for symmetry

        // ── Rows ─────────────────────────────────────────────────────────────
        if companies.companies.is_empty() {
            ui.label(
                egui::RichText::new("No private shipping companies active yet.")
                    .italics()
                    .color(theme::TEXT_DIM),
            );
            return;
        }

        // Use a single fixed timestamp (the latest `created_at_seconds`) for
        // windowing.  We don't read `SimulationTime` here so this fn stays
        // testable in isolation — the data is freshly read on each frame.
        let now = resource_requests
            .requests
            .iter()
            .map(|r| r.completed_at_seconds.unwrap_or(r.created_at_seconds))
            .fold(0.0_f64, f64::max);

        let rows = build_shipping_overview_rows(companies, resource_requests, now);

        // AC#4: hide the in-transit column when the GRA-41 setting is off.
        let num_cols = if show_freighters_in_transit { 7 } else { 6 };

        theme::elevated_frame().show(ui, |ui| {
            egui::Grid::new("shipping_overview_grid")
                .num_columns(num_cols)
                .striped(true)
                .spacing([14.0, 4.0])
                .min_col_width(60.0)
                .show(ui, |ui| {
                    // Header row.
                    ui.label(egui::RichText::new("Company").strong().size(11.0));
                    ui.label(
                        egui::RichText::new("Fleet (idle / total)")
                            .strong()
                            .size(11.0),
                    );
                    if show_freighters_in_transit {
                        ui.label(egui::RichText::new("In Transit").strong().size(11.0));
                    }
                    ui.label(egui::RichText::new("Open Demand").strong().size(11.0));
                    ui.label(egui::RichText::new("Fulfill (60d)").strong().size(11.0));
                    ui.label(egui::RichText::new("Δ Treasury").strong().size(11.0));
                    ui.label(egui::RichText::new("AI").strong().size(11.0));
                    ui.end_row();

                    for row in &rows {
                        // Company name (clickable row → click-through).
                        let name_response = ui.add(
                            egui::Label::new(egui::RichText::new(&row.name).size(12.0).strong())
                                .sense(egui::Sense::click()),
                        );
                        if name_response.clicked() {
                            clicked = Some(row.company_idx);
                        }
                        name_response.on_hover_text(format!(
                            "Click to open the Fleets panel filtered to {}",
                            row.name
                        ));

                        // Fleet (idle / total).
                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {}",
                                row.available_freighters, row.freighter_count
                            ))
                            .size(11.0),
                        );

                        // In Transit (in_transit == total - idle).  Hidden
                        // when GRA-41 setting is off.
                        if show_freighters_in_transit {
                            let in_transit_text = format!("{}", row.in_transit);
                            let in_transit_color = if row.in_transit == 0 {
                                theme::TEXT_DIM
                            } else {
                                theme::GREEN
                            };
                            ui.label(
                                egui::RichText::new(in_transit_text)
                                    .size(11.0)
                                    .color(in_transit_color),
                            );
                        }

                        // Open Demand — flat list of (resource, mt) chips.
                        if row.open_demand_by_resource.is_empty() {
                            ui.label(egui::RichText::new("—").size(11.0).color(theme::TEXT_DIM));
                        } else {
                            let summary: String = row
                                .open_demand_by_resource
                                .iter()
                                .map(|(r, mt)| format!("{} {:.1}", r.symbol(), mt))
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(egui::RichText::new(summary).size(11.0));
                        }

                        // Fulfillment rate.
                        let (rate_text, rate_color) = match row.fulfillment_rate {
                            Some(r) => {
                                let pct = (r * 100.0).round() as u32;
                                let color = if r >= 0.9 {
                                    theme::GREEN
                                } else if r >= 0.5 {
                                    theme::AMBER
                                } else {
                                    theme::RED
                                };
                                (format!("{pct}%"), color)
                            }
                            None => ("—".to_string(), theme::TEXT_DIM),
                        };
                        ui.label(egui::RichText::new(rate_text).size(11.0).color(rate_color));

                        // Treasury delta (window).
                        let delta = row.treasury_delta_mc;
                        let (delta_text, delta_color) = if delta.abs() < 0.005 {
                            ("0 MC".to_string(), theme::TEXT_DIM)
                        } else if delta > 0.0 {
                            (
                                format!("+{} MC", format_currency_short(delta)),
                                theme::GREEN,
                            )
                        } else {
                            (format!("-{} MC", format_currency_short(-delta)), theme::RED)
                        };
                        ui.label(
                            egui::RichText::new(delta_text)
                                .size(11.0)
                                .color(delta_color),
                        );

                        // AI policy indicator.
                        let (icon, color) = match row.policy {
                            crate::economy::CompanyAIPolicy::AutoFreight => {
                                ("🤖 Auto", theme::ACCENT)
                            }
                            crate::economy::CompanyAIPolicy::Manual => {
                                ("✋ Manual", theme::TEXT_DIM)
                            }
                        };
                        ui.label(egui::RichText::new(icon).size(11.0).color(color));

                        ui.end_row();
                    }
                });
        });

        ui.add_space(6.0);
        let in_transit_hint = if show_freighters_in_transit {
            "The \"In Transit\" column is filtered by the \"Show freighters in transit\" \
             toggle in the Fleets panel (GRA-41) — turn it off to hide the column here too."
        } else {
            "The \"In Transit\" column is currently hidden (GRA-41 \"Show freighters in \
             transit\" toggle is off in the Fleets panel)."
        };
        ui.label(
            egui::RichText::new(format!("💡 {in_transit_hint}"))
                .size(10.5)
                .color(theme::TEXT_DIM),
        );
    });

    clicked
}

/// Compact MC formatter for delta column (1.2K / 1.5M / 2.3B style).
fn format_currency_short(mc: f64) -> String {
    let abs = mc.abs();
    if abs >= 1.0e9 {
        format!("{:.1}B", abs / 1.0e9)
    } else if abs >= 1.0e6 {
        format!("{:.1}M", abs / 1.0e6)
    } else if abs >= 1.0e3 {
        format!("{:.1}K", abs / 1.0e3)
    } else {
        format!("{:.1}", abs)
    }
}

// ── Tests (GRA-37.e acceptance) ───────────────────────────────────────────────

#[cfg(test)]
mod shipping_overview_tests {
    use super::*;
    use crate::economy::company::ShippingCompany;
    use crate::economy::{
        CompanyAIPolicy, PendingResourceRequests, RequestPriority, RequestState, ResourceRequest,
        ResourceType, ShippingCompanies,
    };
    use bevy::prelude::Entity;

    fn req(
        assigned_company_idx: Option<usize>,
        resource: ResourceType,
        amount_mt: f64,
        state: RequestState,
        created_at_seconds: f64,
        completed_at_seconds: Option<f64>,
    ) -> ResourceRequest {
        ResourceRequest {
            id: 0,
            destination_body: Entity::PLACEHOLDER,
            destination_name: "Earth".into(),
            resource,
            amount_mt,
            priority: RequestPriority::Maintenance,
            state,
            in_transit_mt: amount_mt,
            eta_seconds: None,
            assigned_company_idx,
            created_at_seconds,
            source_body: None,
            linked_project: None,
            payment_made: false,
            completed_at_seconds,
            assignee_fleet_id: None,
        }
    }

    #[test]
    fn rows_aggregate_per_company() {
        // Two companies, three requests, one delivered.  The build
        // helper should produce two rows whose aggregations match the
        // hand-computed expected values below.
        let companies = ShippingCompanies {
            companies: vec![
                ShippingCompany::new("Helios Freight Co.", 3, 50_000.0),
                ShippingCompany::new("Solar Carriers Ltd.", 1, 20_000.0),
            ],
        };

        // Window: now is 100.0; window covers [100 - 60*86_400, 100].
        // Anything with created_at < (100 - WINDOW_S) is outside the window.
        let now = 100.0;
        let inside = now - 1.0;
        let outside = now - (SHIPPING_OVERVIEW_WINDOW_S + 1.0);

        let mut pool = PendingResourceRequests::default();
        // 1 open, 1 in-transit, both for company 0 — open demand by resource
        // should sum two resources (Food + Water) for company 0.
        pool.requests.push(req(
            Some(0),
            ResourceType::Food,
            10.0,
            RequestState::Pending,
            inside,
            None,
        ));
        pool.requests.push(req(
            Some(0),
            ResourceType::Water,
            5.0,
            RequestState::InTransit,
            inside,
            None,
        ));
        // 1 delivered for company 0, with created_at OUTSIDE the window —
        // it does NOT count toward the in-window fulfillment rate but
        // its `is_open() == false` keeps it out of open demand too.
        pool.requests.push(req(
            Some(0),
            ResourceType::Iron,
            3.0,
            RequestState::Delivered,
            outside,
            Some(outside + 0.5),
        ));
        // 1 open for company 1, but created OUTSIDE the window so it
        // doesn't count toward fulfillment-rate denominator (and stays
        // an "open demand" item — is_open() == true regardless of age).
        pool.requests.push(req(
            Some(1),
            ResourceType::Oxygen,
            4.0,
            RequestState::Pending,
            outside,
            None,
        ));

        let rows = build_shipping_overview_rows(&companies, &pool, now);
        assert_eq!(rows.len(), 2);

        // Sort: company 0 fulfillment 0% (no created-in-window requests
        // that were delivered — only Pending/InTransit created inside),
        // company 1 fulfillment None.  Row 0 wins by name (Helios
        // Freight Co. < Solar Carriers Ltd. when both rates are 0).
        let r0 = &rows[0];
        assert_eq!(r0.company_idx, 0);
        assert_eq!(r0.name, "Helios Freight Co.");
        assert_eq!(r0.freighter_count, 3);
        assert_eq!(r0.available_freighters, 3);
        assert_eq!(r0.in_transit, 0);
        // Open demand: Food 10.0 + Water 5.0; the Delivered one is
        // `is_open() == false` so excluded.
        assert_eq!(
            r0.open_demand_by_resource.get(&ResourceType::Food),
            Some(&10.0)
        );
        assert_eq!(
            r0.open_demand_by_resource.get(&ResourceType::Water),
            Some(&5.0)
        );
        // 2 created inside the window, 0 delivered (the only Delivered
        // was created outside the window) → 0/2 = 0.0.
        assert_eq!(r0.fulfillment_rate, Some(0.0));
        // Treasury delta: company 0 hasn't earned/spent (treasury unchanged).
        assert_eq!(r0.treasury_delta_mc, 0.0);
        assert!(matches!(r0.policy, CompanyAIPolicy::AutoFreight));

        // Row 1 is company 1.
        let r1 = &rows[1];
        assert_eq!(r1.company_idx, 1);
        assert_eq!(r1.freighter_count, 1);
        assert_eq!(r1.in_transit, 0);
        assert_eq!(
            r1.open_demand_by_resource.get(&ResourceType::Oxygen),
            Some(&4.0)
        );
        assert_eq!(r1.fulfillment_rate, None);
        assert_eq!(r1.treasury_delta_mc, 0.0);
    }

    #[test]
    fn in_transit_count_uses_total_minus_idle() {
        // When a company has dispatched freighters, in_transit = total - idle.
        let mut companies = ShippingCompanies {
            companies: vec![ShippingCompany::new("Co.", 5, 0.0)],
        };
        companies.companies[0].available_freighters = 2; // 3 in transit
        let pool = PendingResourceRequests::default();
        let rows = build_shipping_overview_rows(&companies, &pool, 0.0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].in_transit, 3);
    }

    #[test]
    fn treasury_delta_reflects_window_anchor() {
        // After rolling the window, treasury_delta = 0 again.  A new
        // delivery inside the rolled window is a non-zero delta.
        let mut companies = ShippingCompanies {
            companies: vec![ShippingCompany::new("Co.", 1, 10_000.0)],
        };
        companies.companies[0].treasury_window_start_mc = 9_000.0;
        companies.companies[0].treasury_window_start_seconds = 0.0;
        companies.companies[0].treasury_mc = 10_000.0; // +1000 in window
        let pool = PendingResourceRequests::default();
        let rows = build_shipping_overview_rows(&companies, &pool, 100.0);
        assert_eq!(rows[0].treasury_delta_mc, 1_000.0);
    }

    #[test]
    fn rows_sort_by_fulfillment_desc_then_name() {
        // Company A: 100% fulfillment, Company B: 0%, Company C: 50%.
        // Expected order: A, C, B.
        let companies = ShippingCompanies {
            companies: vec![
                ShippingCompany::new("Bravo", 1, 0.0),
                ShippingCompany::new("Alpha", 1, 0.0),
                ShippingCompany::new("Charlie", 1, 0.0),
            ],
        };
        let mut pool = PendingResourceRequests::default();
        let now = 100.0;
        let inside = now - 1.0;
        // Alpha 100%: 1 created, 1 delivered.
        pool.requests.push(req(
            Some(1),
            ResourceType::Food,
            1.0,
            RequestState::Delivered,
            inside,
            Some(inside + 1.0),
        ));
        // Charlie 50%: 2 created, 1 delivered.
        pool.requests.push(req(
            Some(2),
            ResourceType::Food,
            1.0,
            RequestState::Delivered,
            inside,
            Some(inside + 1.0),
        ));
        pool.requests.push(req(
            Some(2),
            ResourceType::Food,
            1.0,
            RequestState::Pending,
            inside,
            None,
        ));
        // Bravo 0%: 1 created, 0 delivered.
        pool.requests.push(req(
            Some(0),
            ResourceType::Food,
            1.0,
            RequestState::Pending,
            inside,
            None,
        ));
        let rows = build_shipping_overview_rows(&companies, &pool, now);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Charlie", "Bravo"]);
    }

    #[test]
    fn format_currency_short_rounds_by_magnitude() {
        assert_eq!(format_currency_short(0.0), "0.0");
        assert_eq!(format_currency_short(500.0), "500.0");
        assert_eq!(format_currency_short(1_500.0), "1.5K");
        assert_eq!(format_currency_short(2_500_000.0), "2.5M");
        assert_eq!(format_currency_short(1.2e10), "12.0B");
    }
}
