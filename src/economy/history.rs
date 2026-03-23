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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct SurveyHistoryStats {
    pub total_bodies: u32,
    pub unsurveyed: u32,
    pub orbital_scan: u32,
    pub seismic_survey: u32,
    pub core_sample: u32,
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
            let should_keep = last_kept_sim_seconds.is_none_or(|last_kept| {
                (last_kept - sample.sim_seconds) >= spacing - 1.0
            });
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
    industrial_factor: f64,
    power_factor: f64,
    space_factor: f64,
    survey_factor: f64,
}

const HISTORIC_EARTH_ANCHORS: [HistoricEarthAnchor; 6] = [
    HistoricEarthAnchor {
        years_ago: 100.0,
        population_factor: 0.24,
        industrial_factor: 0.12,
        power_factor: 0.10,
        space_factor: 0.0,
        survey_factor: 0.0,
    },
    HistoricEarthAnchor {
        years_ago: 70.0,
        population_factor: 0.34,
        industrial_factor: 0.18,
        power_factor: 0.17,
        space_factor: 0.0,
        survey_factor: 0.0,
    },
    HistoricEarthAnchor {
        years_ago: 50.0,
        population_factor: 0.50,
        industrial_factor: 0.35,
        power_factor: 0.33,
        space_factor: 0.04,
        survey_factor: 0.10,
    },
    HistoricEarthAnchor {
        years_ago: 25.0,
        population_factor: 0.74,
        industrial_factor: 0.60,
        power_factor: 0.57,
        space_factor: 0.25,
        survey_factor: 0.55,
    },
    HistoricEarthAnchor {
        years_ago: 10.0,
        population_factor: 0.90,
        industrial_factor: 0.82,
        power_factor: 0.78,
        space_factor: 0.55,
        survey_factor: 0.80,
    },
    HistoricEarthAnchor {
        years_ago: 0.0,
        population_factor: 1.0,
        industrial_factor: 1.0,
        power_factor: 1.0,
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
                population_factor: egui_lerp(older.population_factor, newer.population_factor, t),
                industrial_factor: egui_lerp(older.industrial_factor, newer.industrial_factor, t),
                power_factor: egui_lerp(older.power_factor, newer.power_factor, t),
                space_factor: egui_lerp(older.space_factor, newer.space_factor, t),
                survey_factor: egui_lerp(older.survey_factor, newer.survey_factor, t),
            };
        }
    }

    HISTORIC_EARTH_ANCHORS
        .last()
        .copied()
        .unwrap_or(HistoricEarthAnchor {
            years_ago,
            population_factor: 1.0,
            industrial_factor: 1.0,
            power_factor: 1.0,
            space_factor: 1.0,
            survey_factor: 1.0,
        })
}

fn egui_lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

fn historic_resource_factor(resource: ResourceType, anchor: HistoricEarthAnchor) -> f64 {
    if resource.is_biological() {
        return anchor.population_factor;
    }
    if resource.is_atmospheric_gas() {
        return 0.65 * anchor.population_factor + 0.35 * anchor.industrial_factor;
    }
    if resource.is_volatile() {
        return 0.45 * anchor.population_factor + 0.55 * anchor.industrial_factor;
    }
    if resource.is_exotic() {
        return anchor.space_factor.powf(2.5);
    }
    if resource.is_fusion_fuel() {
        return (0.35 * anchor.industrial_factor + 0.65 * anchor.space_factor).clamp(0.0, 1.0);
    }
    if resource.is_fissile() || resource.is_precious_metal() || resource.is_strategic() {
        return anchor.industrial_factor;
    }

    anchor.industrial_factor
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

    let core_sample = (current.survey.core_sample as f64 * anchor.survey_factor.powf(1.8))
        .round()
        .clamp(0.0, current.survey.core_sample as f64) as u32;
    let seismic_survey = (current.survey.seismic_survey as f64 * anchor.survey_factor.powf(1.45))
        .round()
        .clamp(0.0, current.survey.seismic_survey as f64) as u32;
    let orbital_scan = (current.survey.orbital_scan as f64 * anchor.survey_factor.powf(1.1))
        .round()
        .clamp(0.0, current.survey.orbital_scan as f64) as u32;
    let surveyed_total = orbital_scan + seismic_survey + core_sample;
    let unsurveyed = current.survey.total_bodies.saturating_sub(surveyed_total);

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
            current.resource_net_rates_per_month[index] * historic_resource_factor(*resource, anchor)
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
            unsurveyed,
            orbital_scan,
            seismic_survey,
            core_sample,
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
    bodies: &Query<Option<&SurveyLevel>, With<CelestialBody>>,
) -> SimulationHistorySample {
    let total_population = populations.iter().map(|population| population.count).sum();
    let colony_count = colonies.iter().count() as u32;
    let ship_count = ships.iter().count() as u32;

    let mut survey = SurveyHistoryStats::default();
    for survey_level in bodies.iter() {
        survey.total_bodies += 1;
        match survey_level.copied().unwrap_or(SurveyLevel::Unsurveyed) {
            SurveyLevel::Unsurveyed => survey.unsurveyed += 1,
            SurveyLevel::OrbitalScan => survey.orbital_scan += 1,
            SurveyLevel::SeismicSurvey => survey.seismic_survey += 1,
            SurveyLevel::CoreSample => survey.core_sample += 1,
        }
    }

    let mut resource_stockpiles = vec![0.0; ResourceType::all().len()];
    for (index, resource) in ResourceType::all().iter().copied().enumerate() {
        resource_stockpiles[index] = budget.get_stockpile(&resource)
            + local_stockpiles.iter().map(|stockpile| stockpile.get(&resource)).sum::<f64>();
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
    bodies: Query<Option<&SurveyLevel>, With<CelestialBody>>,
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
                unsurveyed: 40,
                orbital_scan: 30,
                seismic_survey: 20,
                core_sample: 10,
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
        let last = history.samples.last().expect("history should contain current snapshot");

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
}