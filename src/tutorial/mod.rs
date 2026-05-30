use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::game_state::GameMenu;

pub mod hint;
pub mod popup;

/// Tutorial state resource - persists across save/load
#[derive(Resource, Serialize, Deserialize)]
pub struct TutorialState {
    pub current_step: usize,
    pub steps_completed: Vec<usize>,
    pub active_hint: Option<HintInfo>,
    pub hint_cooldowns: HashMap<GameMenu, f64>,
    pub disabled: bool,
    pub idle_times: HashMap<GameMenu, f64>,
    pub total_elapsed_seconds: f64,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self {
            current_step: 0,
            steps_completed: Vec::new(),
            active_hint: None,
            hint_cooldowns: HashMap::new(),
            disabled: false,
            idle_times: HashMap::new(),
            total_elapsed_seconds: 0.0,
        }
    }
}

impl TutorialState {
    pub fn is_step_completed(&self, step_id: usize) -> bool {
        self.steps_completed.contains(&step_id)
    }

    pub fn advance_step(&mut self) {
        self.steps_completed.push(self.current_step);
        self.current_step += 1;
    }

    pub fn all_steps_complete(&self) -> bool {
        self.current_step >= TUTORIAL_STEPS.len()
    }
}

/// Trigger condition for tutorial step activation
#[derive(Debug, Clone)]
pub enum TriggerCondition {
    TimeAfterNewGame(f64),
    MenuOpened(GameMenu),
    BuildingCompleted,
    FleetCreated,
    BodySelected,
    TechQueued,
}

/// Hint information displayed as toast notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HintInfo {
    pub message: String,
    pub screen: GameMenu,
    pub show_until: f64,
}

/// A single tutorial step definition
#[derive(Debug, Clone)]
pub struct TutorialStep {
    pub id: usize,
    pub title: &'static str,
    pub body_text: &'static str,
    pub target_screen: GameMenu,
    pub trigger_condition: TriggerCondition,
    pub hint_text: &'static str,
}

/// The 5 tutorial steps as defined in DELA-39 design brief
pub const TUTORIAL_STEPS: &[TutorialStep] = &[
    TutorialStep {
        id: 0,
        title: "Welcome to Helios Ascension",
        body_text: "You're now commanding humanity's first interstellar colony. Use WASD to pan, right-click drag to rotate, and mouse wheel to zoom. Press Home to recenter on the Sun.",
        target_screen: GameMenu::Survey,
        trigger_condition: TriggerCondition::TimeAfterNewGame(120.0),
        hint_text: "Tip: The starmap shows your empire at a glance. Zoom out to see all star systems.",
    },
    TutorialStep {
        id: 1,
        title: "Colony Fundamentals",
        body_text: "Select a body and open Construction (F4) to see available buildings. Place a Mining Complex to start extracting local resources. Each body has its own resource stockpile.",
        target_screen: GameMenu::Construction,
        trigger_condition: TriggerCondition::MenuOpened(GameMenu::Construction),
        hint_text: "Tip: Different body types have different resource deposits. Rocky worlds often have Minerals.",
    },
    TutorialStep {
        id: 2,
        title: "Resource Economy",
        body_text: "Resources are now stored locally on each body. Freighter ships (Cargo-class) transport resources between bodies. Open Economy (F6) to see system-wide stockpiles.",
        target_screen: GameMenu::Economy,
        trigger_condition: TriggerCondition::BuildingCompleted,
        hint_text: "Tip: If a colony lacks resources, create a resource request from the Economy panel and assign a freighter.",
    },
    TutorialStep {
        id: 3,
        title: "Fleet Operations",
        body_text: "Fleet missions use realistic orbital mechanics. Plan transfers with the Transfer Planner (F5). Hohmann transfers are the most efficient; fast transfers cost more delta-v.",
        target_screen: GameMenu::Fleets,
        trigger_condition: TriggerCondition::FleetCreated,
        hint_text: "Tip: Watch the synodic period countdown. Transfers are only possible when the alignment window opens.",
    },
    TutorialStep {
        id: 4,
        title: "Research Technology",
        body_text: "Open Research (F3) to see the tech tree. Queue technologies to unlock new buildings, ship components, and efficiency bonuses. Prerequisites must be completed first.",
        target_screen: GameMenu::Research,
        trigger_condition: TriggerCondition::MenuOpened(GameMenu::Research),
        hint_text: "Tip: Focus on Mining Efficiency early — it multiplies your resource output across all colonies.",
    },
];

/// Check if a trigger condition has been met
pub fn trigger_condition_met(
    condition: &TriggerCondition,
    state: &TutorialState,
    current_menu: &GameMenu,
) -> bool {
    match condition {
        TriggerCondition::TimeAfterNewGame(seconds) => {
            state.total_elapsed_seconds >= *seconds
        }
        TriggerCondition::MenuOpened(menu) => *current_menu == *menu,
        TriggerCondition::BuildingCompleted => {
            state.steps_completed.contains(&1) || state.current_step >= 2
        }
        TriggerCondition::FleetCreated => {
            state.steps_completed.contains(&2) || state.current_step >= 3
        }
        TriggerCondition::BodySelected => {
            state.steps_completed.contains(&0) || state.current_step >= 1
        }
        TriggerCondition::TechQueued => {
            state.steps_completed.contains(&3) || state.current_step >= 4
        }
    }
}

/// Tutorial plugin - registers tutorial systems
pub struct TutorialPlugin;

/// Advance the tutorial timer from simulation time.
/// This drives the TimeAfterNewGame trigger condition.
fn advance_tutorial_timer(
    time: Res<crate::ui::time::SimulationTime>,
    mut tutorial_state: ResMut<TutorialState>,
) {
    tutorial_state.total_elapsed_seconds = time.elapsed;
}

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                advance_tutorial_timer,
                hint::update_hint_system.after(advance_tutorial_timer),
            ),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                popup::ui_tutorial_popup,
                hint::ui_hint_toast,
            ),
        )
        .init_resource::<TutorialState>();
    }
}
