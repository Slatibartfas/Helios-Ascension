use super::*;

/// Time scale resource for controlling simulation speed
#[derive(Resource, Debug, Clone)]
pub struct TimeScale {
    /// Current time scale multiplier (0.0 = paused, 1.0 = normal, up to 31,557,600.0 ≈ 1 year/second)
    pub scale: f32,
    /// Last active scale before pausing, restored on resume
    last_active_scale: f32,
}

impl TimeScale {
    /// Create a new time scale with default value (1 hr/s).
    pub fn new() -> Self {
        Self {
            scale: 3_600.0,
            last_active_scale: 3_600.0,
        }
    }

    /// Set a new simulation speed, updating the resume-target and unpausing.
    pub fn set_speed(&mut self, new_scale: f32) {
        self.scale = new_scale;
        self.last_active_scale = new_scale;
    }

    /// Pause the simulation
    pub fn pause(&mut self) {
        if self.scale > 0.0 {
            self.last_active_scale = self.scale;
        }
        self.scale = 0.0;
    }

    /// Resume at the speed that was active before pausing
    pub fn resume(&mut self) {
        self.scale = self.last_active_scale;
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.scale == 0.0
    }
}

impl Default for TimeScale {
    fn default() -> Self {
        Self::new()
    }
}

/// Custom simulation clock that tracks game-world elapsed time.
///
/// Unlike Bevy's `Time<Virtual>`, this has **no max-delta cap**, so analytical
/// calculations (Keplerian orbits, body rotation) scale to any speed.
/// Each frame the clock advances by `real_delta × time_scale`.
#[derive(Resource, Debug, Clone)]
pub struct SimulationTime {
    /// Total elapsed simulation time in seconds (f64 for precision)
    pub elapsed: f64,
    /// Starting date as Unix timestamp (January 1, 2026 00:00:00 UTC)
    start_timestamp: i64,
}

impl SimulationTime {
    /// January 1, 2026 00:00:00 UTC as Unix timestamp
    const START_TIMESTAMP: i64 = 1_767_225_600; // Jan 1, 2026 00:00:00 UTC

    pub fn new() -> Self {
        Self {
            elapsed: 0.0,
            start_timestamp: Self::START_TIMESTAMP,
        }
    }

    /// Create a SimulationTime with a custom start date
    ///
    /// For custom game start dates, use this constructor along with
    /// `crate::astronomy::calculate_positions_at_timestamp()` to compute
    /// initial orbital positions for all celestial bodies.
    pub fn with_start_timestamp(start_timestamp: i64) -> Self {
        Self {
            elapsed: 0.0,
            start_timestamp,
        }
    }

    /// Total elapsed simulation seconds
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed
    }

    /// Get the Unix timestamp at which the game started (simulation epoch, elapsed == 0)
    pub fn start_timestamp(&self) -> i64 {
        self.start_timestamp
    }

    /// Get the current simulation date as Unix timestamp
    pub fn current_timestamp(&self) -> i64 {
        self.start_timestamp + self.elapsed as i64
    }

    /// Current tick counter (1 tick = 1 simulation second, 30 ticks/month, 360 ticks/year).
    pub fn tick(&self) -> u64 {
        self.elapsed as u64
    }

    /// Format the current date/time as DD.MM.YYYY HH:MM
    pub fn format_date_time(&self) -> String {
        format_timestamp_date_time(self.current_timestamp())
    }

    /// Format an arbitrary simulation elapsed time (seconds from simulation epoch) as a
    /// `DD.MM.YYYY HH:MM` string, using the same calendar as `format_date_time`.
    pub fn format_arrival_date(&self, arrival_elapsed_seconds: f64) -> String {
        let timestamp = self.start_timestamp + arrival_elapsed_seconds as i64;
        format_timestamp_date_time(timestamp)
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get the number of days in each month for a given year
fn get_days_in_months(year: i64) -> [i64; 12] {
    let feb_days = if is_leap_year(year) { 29 } else { 28 };
    [31, feb_days, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
}

pub(super) fn format_timestamp_date_time(timestamp: i64) -> String {
    let total_days = timestamp / 86400;
    let time_of_day = timestamp % 86400;

    let hours = (time_of_day / 3600) % 24;
    let minutes = (time_of_day % 3600) / 60;

    let mut days_remaining = total_days;
    let mut year = 1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days_remaining >= days_in_year {
            days_remaining -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let mut month = 1;
    let days_in_months = get_days_in_months(year);

    for &days_in_month in &days_in_months {
        if days_remaining >= days_in_month {
            days_remaining -= days_in_month;
            month += 1;
        } else {
            break;
        }
    }

    let day = days_remaining + 1;

    format!(
        "{:02}.{:02}.{} {:02}:{:02}",
        day, month, year, hours, minutes
    )
}

pub(super) fn estimate_research_project_end_timestamp(
    project: &ResearchProject,
    team: Option<&ResearchTeam>,
    technologies: &TechnologiesData,
    research_state: &ResearchState,
    total_allocation: f64,
    current_timestamp: i64,
) -> Option<i64> {
    if project.progress >= project.required_points {
        return Some(current_timestamp);
    }

    if !project.active || project.rp_allocation_percent <= 0.0 || total_allocation <= 0.0 {
        return None;
    }

    let base_rate =
        research_state.rp_rate_per_second * (project.rp_allocation_percent / total_allocation);
    if base_rate <= 0.0 {
        return None;
    }

    let technology = technologies.technologies.get(&project.tech_id);
    let category_bonus = technology
        .map(|tech| 1.0 + (research_state.category_research_bonus(tech.category) / 100.0))
        .unwrap_or(1.0);

    let team_efficiency = technology
        .map(|tech| {
            team.map(|entry| entry.category_efficiency(tech.category) as f64)
                .unwrap_or(1.0)
        })
        .unwrap_or(1.0);

    let effective_rate = base_rate * category_bonus * team_efficiency;
    if effective_rate <= 0.0 {
        return None;
    }

    let remaining_points = (project.required_points - project.progress).max(0.0);
    let eta_seconds = remaining_points / effective_rate;
    if !eta_seconds.is_finite() {
        return None;
    }

    Some(current_timestamp + eta_seconds.ceil() as i64)
}

pub(super) fn estimate_engineering_project_end_timestamp(
    project: &EngineeringProject,
    team: Option<&ResearchTeam>,
    research_state: &ResearchState,
    current_timestamp: i64,
) -> Option<i64> {
    if project.progress >= project.required_points {
        return Some(current_timestamp);
    }

    let team_efficiency = team.map(|entry| entry.efficiency as f64).unwrap_or(1.0);
    let effective_rate = team_efficiency * research_state.engineering_speed_multiplier();
    if effective_rate <= 0.0 {
        return None;
    }

    let remaining_points = (project.required_points - project.progress).max(0.0);
    let eta_seconds = remaining_points / effective_rate;
    if !eta_seconds.is_finite() {
        return None;
    }

    Some(current_timestamp + eta_seconds.ceil() as i64)
}

impl Default for SimulationTime {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a time scale multiplier as a human-readable rate string.
/// Examples: "Real time", "2.5 min/s", "1.0 day/s", "1.0 wk/s"
#[allow(dead_code)]
pub(super) fn format_time_rate(scale: f32) -> String {
    if scale <= 0.0 {
        "Paused".to_string()
    } else if (scale - 1.0).abs() < 0.01 {
        "Real time".to_string()
    } else if scale < 60.0 {
        format!("{:.1}x", scale)
    } else if scale < 3_600.0 {
        format!("{:.1} min/s", scale / 60.0)
    } else if scale < 86_400.0 {
        format!("{:.1} hr/s", scale / 3_600.0)
    } else if scale < 604_800.0 {
        format!("{:.1} day/s", scale / 86_400.0)
    } else if scale < 2_592_000.0 {
        format!("{:.1} wk/s", scale / 604_800.0)
    } else if scale < 31_557_600.0 {
        format!("{:.1} mo/s", scale / 2_592_000.0)
    } else {
        format!("{:.1} yr/s", scale / 31_557_600.0)
    }
}

pub(super) fn advance_simulation_time(
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    mut sim_time: ResMut<SimulationTime>,
) {
    let real_delta = real_time.delta_secs_f64();
    sim_time.elapsed += real_delta * time_scale.scale as f64;
}
