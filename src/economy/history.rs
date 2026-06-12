use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{GlobalBudget, ResourceRateTracker, ResourceType, SECONDS_PER_YEAR};
use crate::colony::Colony;
use crate::economy::components::{LocalStockpile, Population, SurveyLevel};
use crate::fleets::ShipInstance;
use crate::plugins::solar_system::CelestialBody;
use crate::ui::SimulationTime;

pub const HISTORY_MAX_AGE_YEARS: f64 = 100.0;
pub const HISTORY_MAX_AGE_SECONDS: f64 = HISTORY_MAX_AGE_YEARS * SECONDS_PER_YEAR;

const HISTORY_RECENT_YEARS: f64 = 2.0;
const HISTORY_MEDIUM_YEARS: f64 = 10.0;
const HISTORY_LONG_YEARS: f64 = 30.0;

const HISTORY_RECENT_STEP_SECONDS: f64 = 7.0 * 86_400.0;
const HISTORY_MEDIUM_STEP_SECONDS: f64 = 30.0 * 86_400.0;
const HISTORY_LONG_STEP_SECONDS: f64 = 180.0 * 86_400.0;
const HISTORY_ARCHIVE_STEP_SECONDS: f64 = 365.25 * 86_400.0;

/// v0.5.0 replacement for the old `SurveyHistoryStats` enum-bucket
/// counters. Bodies with any survey data at all (mean coverage > 0)
/// are bucketed into four bands — (0%, 25%], (25%, 50%],
/// (50%, 75%], (75%, 100%]. Bodies with no survey data (mean
/// coverage = 0) are NOT bucketed; they show up implicitly via
/// `unsurveyed()` = `total_bodies - surveyed_total()`.
///
/// `#[serde(default)]` on each band keeps old saved samples
/// (pre-PR-F, which only had `total_bodies` / `unsurveyed` /
/// `orbital_scan` etc.) loadable. The old fields are gone, but
/// defaults let the deserializer fill in zeros instead of erroring
/// out.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct SurveyHistoryStats {
    #[serde(default)]
    pub total_bodies: u32,
    /// (0%, 25%] — mean_coverage strictly greater than 0 and ≤ 25%.
    /// Bodies with mean_coverage = 0 are NOT counted here; they
    /// show up via `unsurveyed()` instead.
    #[serde(default)]
    pub band_0_to_25: u32,
    /// (25%, 50%]
    #[serde(default)]
    pub band_25_to_50: u32,
    /// (50%, 75%]
    #[serde(default)]
    pub band_50_to_75: u32,
    /// (75%, 100%]
    #[serde(default)]
    pub band_75_to_100: u32,
}

impl SurveyHistoryStats {
    /// Number of bodies with any survey data at all (mean > 0).
    /// Surfaces as the "surveyed" total in the dashboard and
    /// dossier.
    pub fn surveyed_total(&self) -> u32 {
        self.band_0_to_25 + self.band_25_to_50 + self.band_50_to_75 + self.band_75_to_100
    }

    /// Number of bodies with no survey data (mean = 0).
    pub fn unsurveyed(&self) -> u32 {
        self.total_bodies.saturating_sub(self.surveyed_total())
    }
}

/// Bucket a mean coverage value in `[0.0, 1.0]` into one of the
/// four bands in `SurveyHistoryStats`. `mean_coverage == 0.0`
/// returns `None` (these bodies are unsurveyed — they show up via
/// `unsurveyed()`, not in any band). Otherwise bands are
/// `(0%, 25%]`, `(25%, 50%]`, `(50%, 75%]`, `(75%, 100%]`.
fn coverage_band(mean_coverage: f32) -> Option<Band> {
    let clamped = mean_coverage.clamp(0.0, 1.0);
    if clamped == 0.0 {
        return None;
    }
    Some(if clamped <= 0.25 {
        Band::B0To25
    } else if clamped <= 0.50 {
        Band::B25To50
    } else if clamped <= 0.75 {
        Band::B50To75
    } else {
        Band::B75To100
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Band {
    B0To25,
    B25To50,
    B50To75,
    B75To100,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationHistorySample {
    pub sim_seconds: f64,
    pub total_population: f64,
    pub colony_count: u32,
    pub ship_count: u32,
    pub power_produced_watts: f64,
    pub power_consumed_watts: f64,
    pub survey: SurveyHistoryStats,
    pub resource_stockpiles: Vec<f64>,
    pub resource_net_rates_per_month: Vec<f64>,
    pub resource_gross_production_per_month: Vec<f64>,
    pub resource_gross_consumption_per_month: Vec<f64>,
}

impl SimulationHistorySample {
    pub fn resource_amount(&self, resource: ResourceType) -> f64 {
        self.resource_stockpiles
            .get(resource_index(resource))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn resource_net_rate(&self, resource: ResourceType) -> f64 {
        self.resource_net_rates_per_month
            .get(resource_index(resource))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn resource_gross_production_rate(&self, resource: ResourceType) -> f64 {
        self.resource_gross_production_per_month
            .get(resource_index(resource))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn resource_gross_consumption_rate(&self, resource: ResourceType) -> f64 {
        self.resource_gross_consumption_per_month
            .get(resource_index(resource))
            .copied()
            .unwrap_or(0.0)
    }
}

#[derive(Resource, Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimulationHistory {
    #[serde(default)]
    pub samples: Vec<SimulationHistorySample>,
}

impl SimulationHistory {
    pub fn latest(&self) -> Option<&SimulationHistorySample> {
        self.samples.last()
    }

    pub fn samples_within_window(
        &self,
        current_sim_seconds: f64,
        window_seconds: f64,
    ) -> impl Iterator<Item = &SimulationHistorySample> {
        let cutoff = current_sim_seconds - window_seconds;
        self.samples
            .iter()
            .filter(move |sample| sample.sim_seconds >= cutoff)
    }

    fn record_snapshot(&mut self, snapshot: SimulationHistorySample) {
        if self.samples.is_empty() {
            self.seed_historic_earth_prehistory(&snapshot);
        }

        if self
            .samples
            .last()
            .is_some_and(|last| (last.sim_seconds - snapshot.sim_seconds).abs() < 1.0)
        {
            if let Some(last) = self.samples.last_mut() {
                *last = snapshot;
            }
        } else {
            self.samples.push(snapshot);
        }

        let current_sim_seconds = self
            .samples
            .last()
            .map(|sample| sample.sim_seconds)
            .unwrap_or(0.0);
        self.thin_samples(current_sim_seconds);
    }

    fn thin_samples(&mut self, current_sim_seconds: f64) {
        if self.samples.len() <= 2 {
            return;
        }

        let mut kept_reversed = Vec::with_capacity(self.samples.len());
        let mut last_kept_sim_seconds: Option<f64> = None;

        for sample in self.samples.iter().rev() {
            let age_seconds = current_sim_seconds - sample.sim_seconds;
            if age_seconds > HISTORY_MAX_AGE_SECONDS {
                continue;
            }

            let spacing = sample_spacing_for_age(age_seconds.max(0.0));
            let should_keep = last_kept_sim_seconds
                .is_none_or(|last_kept| (last_kept - sample.sim_seconds) >= spacing - 1.0);
            if should_keep {
                kept_reversed.push(sample.clone());
                last_kept_sim_seconds = Some(sample.sim_seconds);
            }
        }

        kept_reversed.reverse();
        self.samples = kept_reversed;
    }

    fn seed_historic_earth_prehistory(&mut self, current: &SimulationHistorySample) {
        let mut seeded_samples = Vec::new();
        let mut age_seconds = HISTORY_MAX_AGE_SECONDS;

        while age_seconds > 1.0 {
            seeded_samples.push(build_historic_earth_sample(current, age_seconds));
            age_seconds -= sample_spacing_for_age(age_seconds);
        }

        self.samples = seeded_samples;
    }
}

#[derive(Clone, Copy)]
struct HistoricEarthAnchor {
    years_ago: f64,
    population_factor: f64,
    power_factor: f64,
    agriculture_factor: f64,
    bulk_industry_factor: f64,
    electrification_factor: f64,
    nuclear_factor: f64,
    space_factor: f64,
    survey_factor: f64,
}

const HISTORIC_EARTH_ANCHORS: [HistoricEarthAnchor; 12] = [
    HistoricEarthAnchor {
        years_ago: 100.0,
        population_factor: 0.24,
        power_factor: 0.09,
        agriculture_factor: 0.20,
        bulk_industry_factor: 0.07,
        electrification_factor: 0.04,
        nuclear_factor: 0.0,
        space_factor: 0.0,
        survey_factor: 0.02,
    },
    HistoricEarthAnchor {
        years_ago: 76.0,
        population_factor: 0.30,
        power_factor: 0.15,
        agriculture_factor: 0.28,
        bulk_industry_factor: 0.12,
        electrification_factor: 0.08,
        nuclear_factor: 0.03,
        space_factor: 0.0,
        survey_factor: 0.03,
    },
    HistoricEarthAnchor {
        years_ago: 69.0,
        population_factor: 0.35,
        power_factor: 0.18,
        agriculture_factor: 0.32,
        bulk_industry_factor: 0.15,
        electrification_factor: 0.10,
        nuclear_factor: 0.05,
        space_factor: 0.002,
        survey_factor: 0.04,
    },
    HistoricEarthAnchor {
        years_ago: 66.0,
        population_factor: 0.37,
        power_factor: 0.22,
        agriculture_factor: 0.35,
        bulk_industry_factor: 0.18,
        electrification_factor: 0.13,
        nuclear_factor: 0.07,
        space_factor: 0.006,
        survey_factor: 0.05,
    },
    HistoricEarthAnchor {
        years_ago: 56.0,
        population_factor: 0.45,
        power_factor: 0.36,
        agriculture_factor: 0.46,
        bulk_industry_factor: 0.27,
        electrification_factor: 0.24,
        nuclear_factor: 0.22,
        space_factor: 0.03,
        survey_factor: 0.09,
    },
    HistoricEarthAnchor {
        years_ago: 46.0,
        population_factor: 0.54,
        power_factor: 0.47,
        agriculture_factor: 0.60,
        bulk_industry_factor: 0.36,
        electrification_factor: 0.38,
        nuclear_factor: 0.48,
        space_factor: 0.06,
        survey_factor: 0.15,
    },
    HistoricEarthAnchor {
        years_ago: 36.0,
        population_factor: 0.65,
        power_factor: 0.57,
        agriculture_factor: 0.72,
        bulk_industry_factor: 0.48,
        electrification_factor: 0.52,
        nuclear_factor: 0.68,
        space_factor: 0.10,
        survey_factor: 0.24,
    },
    HistoricEarthAnchor {
        years_ago: 26.0,
        population_factor: 0.75,
        power_factor: 0.66,
        agriculture_factor: 0.79,
        bulk_industry_factor: 0.58,
        electrification_factor: 0.66,
        nuclear_factor: 0.82,
        space_factor: 0.15,
        survey_factor: 0.38,
    },
    HistoricEarthAnchor {
        years_ago: 16.0,
        population_factor: 0.86,
        power_factor: 0.82,
        agriculture_factor: 0.90,
        bulk_industry_factor: 0.74,
        electrification_factor: 0.82,
        nuclear_factor: 0.88,
        space_factor: 0.23,
        survey_factor: 0.60,
    },
    HistoricEarthAnchor {
        years_ago: 6.0,
        population_factor: 0.96,
        power_factor: 0.90,
        agriculture_factor: 0.98,
        bulk_industry_factor: 0.93,
        electrification_factor: 0.95,
        nuclear_factor: 0.95,
        space_factor: 0.45,
        survey_factor: 0.84,
    },
    HistoricEarthAnchor {
        years_ago: 2.0,
        population_factor: 0.99,
        power_factor: 0.98,
        agriculture_factor: 1.0,
        bulk_industry_factor: 0.99,
        electrification_factor: 0.99,
        nuclear_factor: 0.98,
        space_factor: 0.85,
        survey_factor: 0.97,
    },
    HistoricEarthAnchor {
        years_ago: 0.0,
        population_factor: 1.0,
        power_factor: 1.0,
        agriculture_factor: 1.0,
        bulk_industry_factor: 1.0,
        electrification_factor: 1.0,
        nuclear_factor: 1.0,
        space_factor: 1.0,
        survey_factor: 1.0,
    },
];

pub fn kardashev_scale_from_watts(produced_watts: f64) -> f64 {
    ((produced_watts.max(1.0).log10() - 6.0) / 10.0).max(0.0)
}

pub fn resource_index(resource: ResourceType) -> usize {
    ResourceType::all()
        .iter()
        .position(|candidate| *candidate == resource)
        .expect("resource type must exist in ResourceType::all")
}

fn sample_spacing_for_age(age_seconds: f64) -> f64 {
    if age_seconds <= HISTORY_RECENT_YEARS * SECONDS_PER_YEAR {
        HISTORY_RECENT_STEP_SECONDS
    } else if age_seconds <= HISTORY_MEDIUM_YEARS * SECONDS_PER_YEAR {
        HISTORY_MEDIUM_STEP_SECONDS
    } else if age_seconds <= HISTORY_LONG_YEARS * SECONDS_PER_YEAR {
        HISTORY_LONG_STEP_SECONDS
    } else {
        HISTORY_ARCHIVE_STEP_SECONDS
    }
}

fn interpolate_historic_anchor(years_ago: f64) -> HistoricEarthAnchor {
    for window in HISTORIC_EARTH_ANCHORS.windows(2) {
        let older = window[0];
        let newer = window[1];
        if years_ago <= older.years_ago && years_ago >= newer.years_ago {
            let span = (older.years_ago - newer.years_ago).max(f64::EPSILON);
            let t = ((older.years_ago - years_ago) / span).clamp(0.0, 1.0);
            return HistoricEarthAnchor {
                years_ago,
                population_factor: historic_factor_lerp(
                    older.population_factor,
                    newer.population_factor,
                    t,
                ),
                power_factor: historic_factor_lerp(older.power_factor, newer.power_factor, t),
                agriculture_factor: historic_factor_lerp(
                    older.agriculture_factor,
                    newer.agriculture_factor,
                    t,
                ),
                bulk_industry_factor: historic_factor_lerp(
                    older.bulk_industry_factor,
                    newer.bulk_industry_factor,
                    t,
                ),
                electrification_factor: historic_factor_lerp(
                    older.electrification_factor,
                    newer.electrification_factor,
                    t,
                ),
                nuclear_factor: historic_factor_lerp(older.nuclear_factor, newer.nuclear_factor, t),
                space_factor: historic_factor_lerp(older.space_factor, newer.space_factor, t),
                survey_factor: historic_factor_lerp(older.survey_factor, newer.survey_factor, t),
            };
        }
    }

    HISTORIC_EARTH_ANCHORS
        .last()
        .copied()
        .unwrap_or(HistoricEarthAnchor {
            years_ago,
            population_factor: 1.0,
            power_factor: 1.0,
            agriculture_factor: 1.0,
            bulk_industry_factor: 1.0,
            electrification_factor: 1.0,
            nuclear_factor: 1.0,
            space_factor: 1.0,
            survey_factor: 1.0,
        })
}

fn egui_lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

fn historic_factor_lerp(start: f64, end: f64, t: f64) -> f64 {
    if start > 0.0 && end > 0.0 {
        let log_start = start.ln();
        let log_end = end.ln();
        (log_start + (log_end - log_start) * t).exp()
    } else {
        egui_lerp(start, end, t)
    }
}

fn historic_resource_factor(resource: ResourceType, anchor: HistoricEarthAnchor) -> f64 {
    use ResourceType::*;

    let factor = match resource {
        Food => anchor.agriculture_factor,
        Water => {
            0.45 * anchor.population_factor
                + 0.40 * anchor.agriculture_factor
                + 0.15 * anchor.bulk_industry_factor
        }
        Hydrogen | Ammonia | Methane => {
            0.20 * anchor.population_factor
                + 0.35 * anchor.bulk_industry_factor
                + 0.45 * anchor.power_factor
        }
        Phosphorus => 0.25 * anchor.population_factor + 0.75 * anchor.agriculture_factor,
        Nitrogen | Oxygen | CarbonDioxide | Argon => {
            0.65 * anchor.population_factor + 0.35 * anchor.power_factor
        }
        Iron | Silicates => anchor.bulk_industry_factor,
        Aluminum | Copper => anchor.electrification_factor,
        Titanium | Nickel | Tungsten | Chromium | Magnesium => {
            0.55 * anchor.bulk_industry_factor + 0.45 * anchor.electrification_factor
        }
        Carbon => 0.50 * anchor.bulk_industry_factor + 0.50 * anchor.electrification_factor,
        Helium3 | Deuterium | Tritium => {
            0.55 * anchor.power_factor
                + 0.20 * anchor.electrification_factor
                + 0.25 * anchor.space_factor
        }
        Uranium | Thorium | Plutonium => anchor.nuclear_factor,
        Gold | Silver | Platinum => {
            0.35 * anchor.bulk_industry_factor
                + 0.45 * anchor.electrification_factor
                + 0.20 * anchor.survey_factor
        }
        RareEarths | Lithium | Cobalt | Fluorine => {
            0.20 * anchor.bulk_industry_factor
                + 0.55 * anchor.electrification_factor
                + 0.25 * anchor.nuclear_factor
        }
        Sulfur => 0.45 * anchor.agriculture_factor + 0.55 * anchor.bulk_industry_factor,
        Polymers => 0.35 * anchor.population_factor + 0.65 * anchor.electrification_factor,
        Antimatter | ExoticMatter | Metamaterials | Computronium => anchor.space_factor.powf(2.6),
    };

    factor.clamp(0.0, 1.0)
}

fn build_historic_earth_sample(
    current: &SimulationHistorySample,
    age_seconds: f64,
) -> SimulationHistorySample {
    let anchor = interpolate_historic_anchor(age_seconds / SECONDS_PER_YEAR);
    let current_colonies = current.colony_count.max(1) as f64;
    let current_ships = current.ship_count as f64;

    let colony_count = if current.colony_count <= 1 {
        current.colony_count
    } else {
        (1.0 + (current_colonies - 1.0) * anchor.space_factor.powf(1.8))
            .round()
            .clamp(1.0, current_colonies) as u32
    };

    let ship_count = (current_ships * anchor.space_factor.powf(1.35))
        .round()
        .clamp(0.0, current_ships) as u32;

    let band_75_to_100 = (current.survey.band_75_to_100 as f64 * anchor.survey_factor.powf(1.8))
        .round()
        .clamp(0.0, current.survey.band_75_to_100 as f64) as u32;
    let band_50_to_75 = (current.survey.band_50_to_75 as f64 * anchor.survey_factor.powf(1.45))
        .round()
        .clamp(0.0, current.survey.band_50_to_75 as f64) as u32;
    let band_25_to_50 = (current.survey.band_25_to_50 as f64 * anchor.survey_factor.powf(1.2))
        .round()
        .clamp(0.0, current.survey.band_25_to_50 as f64) as u32;
    let band_0_to_25 = (current.survey.band_0_to_25 as f64 * anchor.survey_factor.powf(1.1))
        .round()
        .clamp(0.0, current.survey.band_0_to_25 as f64) as u32;

    let resource_stockpiles = ResourceType::all()
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            current.resource_stockpiles[index] * historic_resource_factor(*resource, anchor)
        })
        .collect();
    let resource_net_rates_per_month = ResourceType::all()
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            current.resource_net_rates_per_month[index]
                * historic_resource_factor(*resource, anchor)
        })
        .collect();
    let resource_gross_production_per_month = ResourceType::all()
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            current.resource_gross_production_per_month[index]
                * historic_resource_factor(*resource, anchor)
        })
        .collect();
    let resource_gross_consumption_per_month = ResourceType::all()
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            current.resource_gross_consumption_per_month[index]
                * historic_resource_factor(*resource, anchor)
        })
        .collect();

    SimulationHistorySample {
        sim_seconds: current.sim_seconds - age_seconds,
        total_population: current.total_population * anchor.population_factor,
        colony_count,
        ship_count,
        power_produced_watts: current.power_produced_watts * anchor.power_factor,
        power_consumed_watts: current.power_consumed_watts * anchor.power_factor * 0.92,
        survey: SurveyHistoryStats {
            total_bodies: current.survey.total_bodies,
            band_0_to_25,
            band_25_to_50,
            band_50_to_75,
            band_75_to_100,
        },
        resource_stockpiles,
        resource_net_rates_per_month,
        resource_gross_production_per_month,
        resource_gross_consumption_per_month,
    }
}

fn collect_current_snapshot(
    current_sim_seconds: f64,
    budget: &GlobalBudget,
    rate_tracker: &ResourceRateTracker,
    local_stockpiles: &Query<&LocalStockpile>,
    populations: &Query<&Population>,
    colonies: &Query<&Colony>,
    ships: &Query<&ShipInstance>,
    bodies: &Query<
        (Option<&SurveyLevel>, Option<&crate::survey::SurveyState>),
        With<CelestialBody>,
    >,
) -> SimulationHistorySample {
    let total_population = populations.iter().map(|population| population.count).sum();
    let colony_count = colonies.iter().count() as u32;
    let ship_count = ships.iter().count() as u32;

    // PR-F: bucket by mean coverage (v0.5.0 source of truth) with
    // the legacy `SurveyLevel` as a fallback during the migration
    // window. The legacy `as_deposit_fidelity` adapter isn't needed
    // here — we just want a 0..=1 mean, which the legacy enum
    // maps to the same 0/0.2/0.4/1.0 series it always used.
    let mut survey = SurveyHistoryStats::default();
    for (survey_level, survey_state) in bodies.iter() {
        survey.total_bodies += 1;
        let mean_coverage = if let Some(state) = survey_state {
            state.average_tier()
        } else {
            match survey_level.copied().unwrap_or(SurveyLevel::Unsurveyed) {
                SurveyLevel::Unsurveyed => 0.0,
                SurveyLevel::OrbitalScan => 0.2,
                SurveyLevel::SeismicSurvey => 0.4,
                SurveyLevel::CoreSample => 1.0,
            }
        };
        if let Some(band) = coverage_band(mean_coverage) {
            match band {
                Band::B0To25 => survey.band_0_to_25 += 1,
                Band::B25To50 => survey.band_25_to_50 += 1,
                Band::B50To75 => survey.band_50_to_75 += 1,
                Band::B75To100 => survey.band_75_to_100 += 1,
            }
        }
    }

    let mut resource_stockpiles = vec![0.0; ResourceType::all().len()];
    for (index, resource) in ResourceType::all().iter().copied().enumerate() {
        resource_stockpiles[index] = budget.get_stockpile(&resource)
            + local_stockpiles
                .iter()
                .map(|stockpile| stockpile.get(&resource))
                .sum::<f64>();
    }

    let resource_net_rates_per_month = ResourceType::all()
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            rate_tracker
                .resource_rates
                .get(resource)
                .copied()
                .unwrap_or(resource_stockpiles[index] * 0.0)
        })
        .collect();
    let resource_gross_production_per_month = ResourceType::all()
        .iter()
        .map(|resource| {
            rate_tracker
                .gross_production_rates
                .get(resource)
                .copied()
                .unwrap_or(0.0)
        })
        .collect();
    let resource_gross_consumption_per_month = ResourceType::all()
        .iter()
        .map(|resource| {
            rate_tracker
                .gross_consumption_rates
                .get(resource)
                .copied()
                .unwrap_or(0.0)
        })
        .collect();

    SimulationHistorySample {
        sim_seconds: current_sim_seconds,
        total_population,
        colony_count,
        ship_count,
        power_produced_watts: budget.energy_grid.produced,
        power_consumed_watts: budget.energy_grid.consumed,
        survey,
        resource_stockpiles,
        resource_net_rates_per_month,
        resource_gross_production_per_month,
        resource_gross_consumption_per_month,
    }
}

pub fn record_simulation_history(
    mut history: ResMut<SimulationHistory>,
    sim_time: Res<SimulationTime>,
    budget: Res<GlobalBudget>,
    rate_tracker: Res<ResourceRateTracker>,
    local_stockpiles: Query<&LocalStockpile>,
    populations: Query<&Population>,
    colonies: Query<&Colony>,
    ships: Query<&ShipInstance>,
    bodies: Query<(Option<&SurveyLevel>, Option<&crate::survey::SurveyState>), With<CelestialBody>>,
) {
    let current_snapshot = collect_current_snapshot(
        sim_time.elapsed_seconds(),
        &budget,
        &rate_tracker,
        &local_stockpiles,
        &populations,
        &colonies,
        &ships,
        &bodies,
    );
    history.record_snapshot(current_snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_at(sim_seconds: f64) -> SimulationHistorySample {
        let resource_count = ResourceType::all().len();
        SimulationHistorySample {
            sim_seconds,
            total_population: 8.2e9,
            colony_count: 3,
            ship_count: 12,
            power_produced_watts: 3.65e12,
            power_consumed_watts: 3.1e12,
            survey: SurveyHistoryStats {
                total_bodies: 100,
                band_0_to_25: 30,
                band_25_to_50: 25,
                band_50_to_75: 25,
                band_75_to_100: 20,
            },
            resource_stockpiles: vec![100.0; resource_count],
            resource_net_rates_per_month: vec![10.0; resource_count],
            resource_gross_production_per_month: vec![14.0; resource_count],
            resource_gross_consumption_per_month: vec![4.0; resource_count],
        }
    }

    #[test]
    fn kardashev_scale_matches_sagan_formula() {
        assert!((kardashev_scale_from_watts(1.0e16) - 1.0).abs() < 1.0e-9);
        assert!((kardashev_scale_from_watts(1.0e26) - 2.0).abs() < 1.0e-9);
    }

    #[test]
    fn seeded_history_covers_full_window() {
        let mut history = SimulationHistory::default();
        history.record_snapshot(sample_at(0.0));

        let first = history.samples.first().expect("history should be seeded");
        let last = history
            .samples
            .last()
            .expect("history should contain current snapshot");

        assert!(first.sim_seconds <= -HISTORY_MAX_AGE_SECONDS + HISTORY_ARCHIVE_STEP_SECONDS);
        assert!((last.sim_seconds - 0.0).abs() < 1.0);
        assert!(history.samples.len() < 400);
    }

    #[test]
    fn thinning_discards_old_high_frequency_samples() {
        let mut history = SimulationHistory {
            samples: (0..500)
                .map(|index| sample_at(index as f64 * HISTORY_RECENT_STEP_SECONDS))
                .collect(),
        };
        history.thin_samples(500.0 * HISTORY_RECENT_STEP_SECONDS);

        assert!(history.samples.len() < 200);
    }

    #[test]
    fn seeded_iron_production_is_monotonic() {
        let current = sample_at(0.0);
        let iron_index = resource_index(ResourceType::Iron);

        let mut previous = f64::NEG_INFINITY;
        let mut age_seconds = HISTORY_MAX_AGE_SECONDS;
        while age_seconds > 1.0 {
            let sample = build_historic_earth_sample(&current, age_seconds);
            let iron_production = sample.resource_gross_production_per_month[iron_index];
            assert!(
                iron_production >= previous,
                "iron production regressed at age {:.2} years: {:.6} < {:.6}",
                age_seconds / SECONDS_PER_YEAR,
                iron_production,
                previous
            );
            previous = iron_production;
            age_seconds -= sample_spacing_for_age(age_seconds);
        }

        let current_iron_production = current.resource_gross_production_per_month[iron_index];
        assert!(current_iron_production >= previous);
    }
}
